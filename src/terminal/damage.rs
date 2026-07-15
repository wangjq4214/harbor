use bitvec::prelude::BitVec;

/// A contiguous range of dirty cells within a single row.
///
/// `start_col` is inclusive, `end_col` is exclusive: `[start_col, end_col)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirtyRange {
    pub(crate) row: usize,
    pub(crate) start_col: usize,
    pub(crate) end_col: usize,
}

/// Tracks dirty grid cells and rows for incremental GPU uploads.
///
/// # Convention
/// Always mark damage (dirty) *before* writing/modifying cell data in the screen buffer,
/// because the damage range calculation may need to read the old state of the cells
/// (e.g. to inspect wide glyph continuation flags).
#[derive(Debug, Clone)]
pub(crate) struct DamageTracker {
    rows: usize,
    cols: usize,
    grid: BitVec,
    /// One bit per row: set ⟹ that row may contain dirty cells.
    /// Enables O(dirty_rows) scanning instead of O(total_rows).
    dirty_row_bits: Vec<u64>,
}

impl DamageTracker {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        let mut dirty_row_bits = vec![u64::MAX; rows.div_ceil(64)];
        Self::mask_trailing_bits(&mut dirty_row_bits, rows);
        Self {
            rows,
            cols,
            grid: {
                let mut g = BitVec::new();
                g.resize(rows * cols, true);
                g
            },
            dirty_row_bits,
        }
    }

    pub(crate) fn resize(&mut self, new_rows: usize, new_cols: usize) {
        self.rows = new_rows;
        self.cols = new_cols;
        self.grid.clear();
        self.grid.resize(new_rows * new_cols, true);
        self.dirty_row_bits = vec![u64::MAX; new_rows.div_ceil(64)];
        Self::mask_trailing_bits(&mut self.dirty_row_bits, new_rows);
    }

    pub(crate) fn clear(&mut self) {
        self.grid.fill(false);
        self.dirty_row_bits.fill(0);
    }

    pub(crate) fn mark_all_dirty(&mut self) {
        self.grid.fill(true);
        self.dirty_row_bits.fill(u64::MAX);
        Self::mask_trailing_bits(&mut self.dirty_row_bits, self.rows);
    }

    pub(crate) fn mark_row_dirty(&mut self, row: usize) {
        if row < self.rows {
            let start = row * self.cols;
            let end = start + self.cols;
            self.grid[start..end].fill(true);
            self.dirty_row_bits[row / 64] |= 1u64 << (row % 64);
        }
    }

    pub(crate) fn mark_rows_dirty(&mut self, start_row: usize, end_row: usize) {
        let start_row = start_row.min(self.rows);
        let end_row = end_row.min(self.rows);
        if start_row < end_row {
            let start = start_row * self.cols;
            let end = end_row * self.cols;
            self.grid[start..end].fill(true);
            for row in start_row..end_row {
                self.dirty_row_bits[row / 64] |= 1u64 << (row % 64);
            }
        }
    }

    pub(crate) fn mark_range_dirty(&mut self, row: usize, start_col: usize, end_col: usize) {
        if row < self.rows {
            let cols = self.cols;
            let start_col = start_col.min(cols);
            let end_col = end_col.min(cols);
            if start_col < end_col {
                let start = row * cols + start_col;
                let end = row * cols + end_col;
                self.grid[start..end].fill(true);
                self.dirty_row_bits[row / 64] |= 1u64 << (row % 64);
            }
        }
    }

    pub(crate) fn dirty_ranges(&self) -> Vec<DirtyRange> {
        let mut ranges = Vec::new();
        for (word_idx, &word) in self.dirty_row_bits.iter().enumerate() {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                w &= w - 1; // clear lowest set bit
                let row = word_idx * 64 + bit;
                if row >= self.rows {
                    break;
                }
                let base = row * self.cols;
                let mut col = 0;
                while col < self.cols {
                    if self.grid[base + col] {
                        let start = col;
                        while col < self.cols && self.grid[base + col] {
                            col += 1;
                        }
                        debug_assert!(start < col, "DirtyRange must be non-empty");
                        ranges.push(DirtyRange {
                            row,
                            start_col: start,
                            end_col: col,
                        });
                    } else {
                        col += 1;
                    }
                }
            }
        }
        ranges
    }

    /// Clear bits beyond `rows` in the last word so they are never visited.
    fn mask_trailing_bits(bits: &mut [u64], rows: usize) {
        let remainder = rows % 64;
        if remainder != 0
            && let Some(last) = bits.last_mut()
        {
            *last &= (1u64 << remainder) - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damage_tracker() {
        let mut tracker = DamageTracker::new(3, 4);
        // New tracker has everything dirty.
        assert_eq!(
            tracker.dirty_ranges(),
            vec![
                DirtyRange {
                    row: 0,
                    start_col: 0,
                    end_col: 4
                },
                DirtyRange {
                    row: 1,
                    start_col: 0,
                    end_col: 4
                },
                DirtyRange {
                    row: 2,
                    start_col: 0,
                    end_col: 4
                }
            ]
        );

        tracker.clear();
        assert!(tracker.dirty_ranges().is_empty());

        // Mark single cell dirty using range
        tracker.mark_range_dirty(1, 2, 3);
        assert_eq!(
            tracker.dirty_ranges(),
            vec![DirtyRange {
                row: 1,
                start_col: 2,
                end_col: 3
            }]
        );

        // Mark range dirty
        tracker.mark_range_dirty(1, 1, 3);
        assert_eq!(
            tracker.dirty_ranges(),
            vec![DirtyRange {
                row: 1,
                start_col: 1,
                end_col: 3
            }]
        );

        // Mark row dirty
        tracker.mark_row_dirty(0);
        assert_eq!(
            tracker.dirty_ranges(),
            vec![
                DirtyRange {
                    row: 0,
                    start_col: 0,
                    end_col: 4
                },
                DirtyRange {
                    row: 1,
                    start_col: 1,
                    end_col: 3
                }
            ]
        );

        // Resize tracker
        tracker.resize(2, 2);
        assert_eq!(
            tracker.dirty_ranges(),
            vec![
                DirtyRange {
                    row: 0,
                    start_col: 0,
                    end_col: 2
                },
                DirtyRange {
                    row: 1,
                    start_col: 0,
                    end_col: 2
                }
            ]
        );
    }

    #[test]
    fn test_mark_rows_dirty() {
        let mut tracker = DamageTracker::new(5, 4);
        tracker.clear();
        tracker.mark_rows_dirty(1, 3);
        assert_eq!(
            tracker.dirty_ranges(),
            vec![
                DirtyRange {
                    row: 1,
                    start_col: 0,
                    end_col: 4
                },
                DirtyRange {
                    row: 2,
                    start_col: 0,
                    end_col: 4
                },
            ]
        );

        // Test empty range (length = 0)
        tracker.clear();
        tracker.mark_rows_dirty(2, 2);
        assert!(tracker.dirty_ranges().is_empty());

        // Test DamageTracker with rows = 0
        let mut zero_tracker = DamageTracker::new(0, 4);
        zero_tracker.clear();
        zero_tracker.mark_rows_dirty(0, 5);
        assert!(zero_tracker.dirty_ranges().is_empty());

        // Test out of bounds clamping
        tracker.clear();
        tracker.mark_rows_dirty(4, 10);
        assert_eq!(
            tracker.dirty_ranges(),
            vec![DirtyRange {
                row: 4,
                start_col: 0,
                end_col: 4
            },]
        );

        // Test crossing 64-bit word boundary (rows 60 to 66)
        let mut large_tracker = DamageTracker::new(70, 4);
        large_tracker.clear();
        large_tracker.mark_rows_dirty(60, 66);
        let ranges = large_tracker.dirty_ranges();
        assert_eq!(ranges.len(), 6);
        assert_eq!(ranges[0].row, 60);
        assert_eq!(ranges[5].row, 65);
    }
}
