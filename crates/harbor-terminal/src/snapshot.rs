//! Owned cell projections with row-local sharing, independent of renderer damage.
use crate::model::Cell;
use crate::normal_buf::NormalBuf;
use std::ops::{Index, IndexMut};
use std::sync::Arc;

/// A flat-indexed, row-shared cell projection. Cloning shares immutable payloads.
///
/// `iter().cloned().collect::<Vec<_>>()` explicitly materializes contiguous cells.
/// Mutating a synthetic snapshot detaches only its directory and touched rows.
#[derive(Clone, Debug)]
pub struct SnapshotCells {
    rows: Arc<[Arc<[Cell]>]>,
    cols: usize,
    len: usize,
}

impl SnapshotCells {
    fn from_rows(rows: Vec<Arc<[Cell]>>, cols: usize) -> Self {
        debug_assert!(cols > 0 && rows.iter().all(|r| r.len() == cols));
        let len = rows.len() * cols;
        Self {
            rows: rows.into(),
            cols,
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, index: usize) -> Option<&Cell> {
        (index < self.len).then(|| &self.rows[index / self.cols][index % self.cols])
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Cell> {
        self.rows.iter().flat_map(|row| row.iter())
    }
    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Cell> {
        Arc::make_mut(&mut self.rows)
            .iter_mut()
            .flat_map(|row| Arc::make_mut(row).iter_mut())
    }

    #[cfg(test)]
    pub(crate) fn shared_rows_with(&self, other: &Self) -> usize {
        self.rows
            .iter()
            .zip(other.rows.iter())
            .filter(|(a, b)| Arc::ptr_eq(a, b))
            .count()
    }
}

impl Index<usize> for SnapshotCells {
    type Output = Cell;
    fn index(&self, index: usize) -> &Cell {
        self.get(index).expect("snapshot cell index out of bounds")
    }
}
impl IndexMut<usize> for SnapshotCells {
    fn index_mut(&mut self, index: usize) -> &mut Cell {
        assert!(index < self.len, "snapshot cell index out of bounds");
        &mut Arc::make_mut(&mut Arc::make_mut(&mut self.rows)[index / self.cols])[index % self.cols]
    }
}
impl From<Vec<Cell>> for SnapshotCells {
    fn from(cells: Vec<Cell>) -> Self {
        let len = cells.len();
        Self {
            rows: if len == 0 {
                Arc::from([])
            } else {
                Arc::from([Arc::from(cells)])
            },
            cols: len.max(1),
            len,
        }
    }
}
impl FromIterator<Cell> for SnapshotCells {
    fn from_iter<T: IntoIterator<Item = Cell>>(iter: T) -> Self {
        Vec::from_iter(iter).into()
    }
}
impl PartialEq for SnapshotCells {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.rows, &other.rows)
            || (self.len == other.len && self.iter().eq(other.iter()))
    }
}
impl Eq for SnapshotCells {}

/// Only the latest visible directory is retained. Old updates own their own row
/// references; history capacity does not add a second cache of retained contents.
#[derive(Clone, Debug, Default)]
pub(crate) struct SnapshotCache {
    generation: u64,
    start: u64,
    versions: Vec<Option<u64>>,
    cells: Option<SnapshotCells>,
    #[cfg(test)]
    pub(crate) copied_cells: usize,
}

impl SnapshotCache {
    /// Input-tail overflow bookkeeping is projected but is not semantic content.
    pub(crate) fn invalidate_row(&mut self, generation: u64) {
        if let Some(index) = generation
            .checked_sub(self.start)
            .and_then(|n| usize::try_from(n).ok())
            && let Some(version) = self.versions.get_mut(index)
        {
            *version = None;
        }
    }

    pub(crate) fn project(&mut self, normal: &NormalBuf) -> SnapshotCells {
        let start = normal.history_start() + (normal.scroll_count() - normal.view_offset()) as u64;
        let generation = normal.content_generation();
        if self.generation == generation
            && self.start == start
            && self.versions.len() == normal.rows()
            && self
                .versions
                .iter()
                .enumerate()
                .all(|(row, &version)| version == normal.row_version(start + row as u64))
            && let Some(cells) = &self.cells
        {
            return cells.clone();
        }
        let rows = (0..normal.rows())
            .map(|row| {
                let version = normal.row_version(start + row as u64);
                let old_row = (start + row as u64)
                    .checked_sub(self.start)
                    .and_then(|n| usize::try_from(n).ok());
                if self.generation == generation
                    && let Some(old_row) = old_row
                    && self.versions.get(old_row) == Some(&version)
                    && let Some(cells) = &self.cells
                    && cells.cols == normal.cols()
                {
                    return cells.rows[old_row].clone();
                }
                let source = normal
                    .retained_row(start + row as u64)
                    .expect("visible row");
                #[cfg(test)]
                {
                    self.copied_cells += source.cells.len();
                }
                Arc::from(source.cells)
            })
            .collect();
        let cells = SnapshotCells::from_rows(rows, normal.cols());
        self.generation = generation;
        self.start = start;
        self.versions.clear();
        self.versions
            .extend((0..normal.rows()).map(|row| normal.row_version(start + row as u64)));
        self.cells = Some(cells.clone());
        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_projection_copies_one_row_even_while_older_versions_survive() {
        for (rows, cols) in [(2, 8), (24, 80), (160, 400)] {
            let mut normal = NormalBuf::with_history_limits(
                rows,
                cols,
                harbor_config::HistoryLimits {
                    max_rows: 0,
                    max_bytes: 0,
                },
            );
            let mut cache = SnapshotCache::default();
            let original = cache.project(&normal);
            assert_eq!(cache.copied_cells, rows * cols);
            let repeated = cache.project(&normal);
            assert_eq!(cache.copied_cells, rows * cols);
            assert_eq!(original.shared_rows_with(&repeated), rows);
            normal.cell_mut(0, 0).ch = 'A';
            normal.cell_mut(0, 1).suffix = "\u{301}".into();
            normal.clear_dirty();
            let changed = cache.project(&normal);
            assert_eq!(cache.copied_cells, rows * cols + cols);
            assert_eq!(original.shared_rows_with(&changed), rows - 1);
            assert_eq!(original[0].ch, ' ');
            assert_eq!(changed[0].ch, 'A');
            let _ = cache.project(&normal);
            assert_eq!(cache.copied_cells, rows * cols + cols);
        }
    }

    #[test]
    fn synthetic_mutation_detaches_only_touched_rows_and_flat_materialization_is_explicit() {
        let normal = NormalBuf::with_history_limits(
            3,
            8,
            harbor_config::HistoryLimits {
                max_rows: 0,
                max_bytes: 0,
            },
        );
        let source = normal.snapshot_cells();
        let mut detached = source.clone();
        detached[9].ch = 'X';
        assert_eq!(source.shared_rows_with(&detached), 2);
        assert_eq!(source[9].ch, ' ');
        assert_eq!(detached[9].ch, 'X');
        let flat: SnapshotCells = detached.iter().cloned().collect();
        assert_eq!(flat, detached);
        assert_eq!(flat.len(), 24);
        assert!(flat.get(24).is_none());
    }
}
