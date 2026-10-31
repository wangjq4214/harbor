//! Detached storage construction. Layout policy stays with primary reflow;
//! this module owns allocation, final cell materialization, and rectangular retention.
use super::{CellState, LogicalLineId, NormalBuf, RowMetadata};
use crate::damage::DamageTracker;
use crate::screen::Cell;
use harbor_config::HistoryLimits;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageError {
    ArithmeticOverflow,
    AllocationFailed,
    Invariant(&'static str),
}

/// Final materialization contract: preparation may borrow payload, storage must own it.
pub(crate) trait CellSource {
    fn materialize(&self) -> Cell;
    fn source_bytes(&self) -> usize;
}

impl CellSource for Cell {
    fn materialize(&self) -> Cell {
        self.clone()
    }
    fn source_bytes(&self) -> usize {
        if self.wide_continuation {
            0
        } else {
            self.ch.len_utf8() + self.suffix.len()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DetachedRow<C = Cell> {
    pub(crate) cells: Vec<C>,
    pub(crate) cell_state: Vec<CellState>,
    pub(crate) metadata: RowMetadata,
}

impl NormalBuf {
    pub(crate) fn from_prepared_rows<C: CellSource>(
        retained: &[DetachedRow<C>],
        visible_rows: usize,
        cols: usize,
        limits: HistoryLimits,
        next_logical_line_id: u64,
        history_start: u64,
        view_offset: usize,
    ) -> Result<Self, StorageError> {
        let mut detached = Self::allocate_detached(
            retained.len(),
            visible_rows,
            cols,
            limits,
            next_logical_line_id,
            history_start,
            view_offset,
        )?;
        for (offset, row) in retained.iter().enumerate() {
            if row.cells.len() != cols || row.cell_state.len() != cols {
                return Err(StorageError::Invariant("detached row width mismatch"));
            }
            let ring = detached.detached_row_index(offset);
            let start = ring * cols;
            for (target, source) in detached.cells[start..start + cols]
                .iter_mut()
                .zip(&row.cells)
            {
                *target = source.materialize();
            }
            detached.cell_state[start..start + cols].copy_from_slice(&row.cell_state);
            detached.row_metadata[ring] = row.metadata;
        }
        detached.history_bytes = detached
            .retained_rows()
            .take(detached.scroll_count)
            .map(|row| Self::row_source_bytes(row.cells, row.metadata.meaningful_extent))
            .sum();
        Ok(detached)
    }

    fn detached_row_index(&self, offset: usize) -> usize {
        // Fresh rings place retained rows contiguously, ending at total_rows.
        self.visible_start - self.scroll_count + offset
    }

    /// Copies only retained rows into final storage, after selecting row/byte eviction.
    pub(crate) fn prepare_history_limits(
        &self,
        limits: HistoryLimits,
    ) -> Result<Self, StorageError> {
        let mut dropped = self.scroll_count.saturating_sub(limits.max_rows);
        let mut bytes: usize = self
            .retained_rows()
            .skip(dropped)
            .take(self.scroll_count - dropped)
            .map(|row| Self::row_source_bytes(row.cells, row.metadata.meaningful_extent))
            .sum();
        for row in self
            .retained_rows()
            .skip(dropped)
            .take(self.scroll_count - dropped)
        {
            if bytes <= limits.max_bytes {
                break;
            }
            bytes -= Self::row_source_bytes(row.cells, row.metadata.meaningful_extent);
            dropped += 1;
        }
        let count = self.scroll_count + self.visible_rows - dropped;
        let history_start = self
            .history_start
            .checked_add(dropped as u64)
            .ok_or(StorageError::ArithmeticOverflow)?;
        let mut detached = Self::allocate_detached(
            count,
            self.visible_rows,
            self.cols,
            limits,
            self.next_logical_line_id,
            history_start,
            self.view_offset,
        )?;
        let preceding = dropped
            .checked_sub(1)
            .and_then(|offset| self.retained_rows().nth(offset));
        for (offset, row) in self.retained_rows().skip(dropped).enumerate() {
            let ring = detached.detached_row_index(offset);
            let start = ring * self.cols;
            detached.cells[start..start + self.cols].clone_from_slice(row.cells);
            detached.cell_state[start..start + self.cols].copy_from_slice(row.cell_state);
            let mut metadata = row.metadata;
            if offset == 0
                && preceding
                    .is_some_and(|prior| prior.metadata.logical_line_id == metadata.logical_line_id)
            {
                metadata.head_truncated = true;
            }
            detached.row_metadata[ring] = metadata;
        }
        detached.history_bytes = bytes;
        Ok(detached)
    }

    /// Rectangular policy writes directly into the final detached ring; no owned rows in between.
    pub(crate) fn prepare_rectangular_resize(
        &self,
        requested_rows: usize,
        requested_cols: usize,
    ) -> Result<Self, StorageError> {
        let rows = requested_rows.max(1);
        let cols = requested_cols.max(2);
        let copy_rows = self.visible_rows.min(rows);
        let copy_cols = self.cols.min(cols);
        let next = self
            .next_logical_line_id
            .checked_add((rows - copy_rows) as u64)
            .ok_or(StorageError::ArithmeticOverflow)?;
        let history_start = self
            .newest_generation_exclusive()
            .ok_or(StorageError::ArithmeticOverflow)?;
        let mut detached = Self::allocate_detached(
            rows,
            rows,
            cols,
            HistoryLimits {
                max_rows: 0,
                max_bytes: 0,
            },
            next,
            history_start,
            0,
        )?;
        for (index, source) in self
            .retained_rows()
            .skip(self.scroll_count)
            .take(copy_rows)
            .enumerate()
        {
            let start = index * cols;
            let cells = &mut detached.cells[start..start + cols];
            let states = &mut detached.cell_state[start..start + cols];
            cells[..copy_cols].clone_from_slice(&source.cells[..copy_cols]);
            states[..copy_cols].copy_from_slice(&source.cell_state[..copy_cols]);
            Self::normalize_wide_row(cells, states);
            let mut metadata = source.metadata;
            metadata.meaningful_extent = states
                .iter()
                .rposition(|s| s.is_meaningful())
                .map_or(0, |col| col + 1);
            detached.row_metadata[index] = metadata;
        }
        for index in copy_rows..rows {
            detached.row_metadata[index].logical_line_id =
                LogicalLineId(self.next_logical_line_id + (index - copy_rows) as u64);
        }
        let mut previous: Option<(RowMetadata, usize)> = None;
        for index in 0..rows {
            let row = &mut detached.row_metadata[index];
            let start = index * cols;
            let atom_count = detached.cells[start..start + row.meaningful_extent]
                .iter()
                .filter(|cell| !cell.wide_continuation)
                .count();
            if row.soft_wrapped {
                if let Some((prior, count)) = previous
                    && row.logical_line_id == prior.logical_line_id
                {
                    row.logical_start = prior
                        .logical_start
                        .checked_add(prior.meaningful_extent)
                        .ok_or(StorageError::ArithmeticOverflow)?;
                    row.logical_atom_start = prior
                        .logical_atom_start
                        .checked_add(count)
                        .ok_or(StorageError::ArithmeticOverflow)?;
                } else {
                    row.head_truncated = true;
                }
            }
            previous = Some((*row, atom_count));
        }
        detached.mark_all_dirty();
        Ok(detached)
    }
    fn allocate_detached(
        retained_count: usize,
        visible_rows: usize,
        cols: usize,
        limits: HistoryLimits,
        mut next_logical_line_id: u64,
        history_start: u64,
        view_offset: usize,
    ) -> Result<Self, StorageError> {
        let max_scrollback = limits.max_rows;

        let total_rows = max_scrollback
            .checked_add(visible_rows)
            .ok_or(StorageError::ArithmeticOverflow)?;
        if visible_rows == 0
            || cols == 0
            || retained_count < visible_rows
            || retained_count > total_rows
        {
            return Err(StorageError::Invariant("invalid detached ring dimensions"));
        }
        let cell_count = total_rows
            .checked_mul(cols)
            .ok_or(StorageError::ArithmeticOverflow)?;
        let retained_generation_count =
            u64::try_from(retained_count).map_err(|_| StorageError::ArithmeticOverflow)?;
        history_start
            .checked_add(retained_generation_count)
            .ok_or(StorageError::ArithmeticOverflow)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(cell_count)
            .map_err(|_| StorageError::AllocationFailed)?;
        cells.resize(cell_count, Cell::default());
        let mut cell_state = Vec::new();
        cell_state
            .try_reserve_exact(cell_count)
            .map_err(|_| StorageError::AllocationFailed)?;
        cell_state.resize(cell_count, CellState::default());

        let mut row_metadata = Vec::new();
        row_metadata
            .try_reserve_exact(total_rows)
            .map_err(|_| StorageError::AllocationFailed)?;
        for _ in 0..total_rows {
            let id = LogicalLineId(next_logical_line_id);
            next_logical_line_id = next_logical_line_id
                .checked_add(1)
                .ok_or(StorageError::ArithmeticOverflow)?;
            row_metadata.push(RowMetadata {
                logical_line_id: id,
                logical_start: 0,
                logical_atom_start: 0,
                meaningful_extent: 0,
                soft_wrapped: false,
                head_truncated: false,
            });
        }

        let scroll_count = retained_count - visible_rows;
        let visible_start = max_scrollback;
        let mut row_versions = Vec::new();
        row_versions
            .try_reserve_exact(total_rows)
            .map_err(|_| StorageError::AllocationFailed)?;
        row_versions.resize(total_rows, 0);
        Ok(Self {
            snapshot_cache: Default::default(),
            cells,
            cell_state,
            row_metadata,
            content_generation: crate::read_model::fresh_identity(),
            content_revision: 0,
            row_versions,
            next_logical_line_id,
            total_rows,
            visible_rows,
            cols,
            visible_start,
            scroll_count,
            view_offset: view_offset.min(scroll_count),
            damage_tracker: DamageTracker::new(visible_rows, cols),
            max_scrollback,
            max_history_bytes: limits.max_bytes,
            history_bytes: 0,
            history_start,
        })
    }
}
