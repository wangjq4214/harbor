use crate::terminal::screen::Cell;

/// Ring-buffer backed scrollback buffer.
///
/// Storage is a single `Vec<Cell>` of `total_rows * cols` elements.
/// Visible rows occupy `visible_rows` consecutive rows in the ring starting
/// at `visible_start`.  When the viewport is at the live bottom
/// (`view_offset == 0`), the ring head advances O(1) on full-screen scroll
/// — no cell copies, just a pointer bump and blank-fill of the newly
/// exposed row(s).
#[derive(Debug)]
pub(crate) struct NormalBuf {
    /// Ring buffer: `total_rows * cols` cells — accessible via helper methods.
    cells: Vec<Cell>,
    /// Ring capacity in rows = `max_scrollback + visible_rows`.
    total_rows: usize,
    /// Viewport height (visible row count).
    visible_rows: usize,
    /// Number of columns.
    cols: usize,
    /// Ring index of the first visible display row.
    visible_start: usize,
    /// Number of saved scrollback rows (0 ..= max_scrollback).
    scroll_count: usize,
    /// View offset from live bottom: 0 = bottom (live), >0 = scrolled back.
    view_offset: usize,
    /// Damage tracker.
    damage_tracker: DamageTracker,
    /// Maximum scrollback row count (hard-coded for now).
    max_scrollback: usize,
    /// Monotonically increasing scrollback generation base.
    /// Incremented when ring-buffer wraparound evicts old rows.
    history_start: u64,
}

impl NormalBuf {
    const DEFAULT_MAX_SCROLLBACK: usize = 1000;
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        let max_scrollback = Self::DEFAULT_MAX_SCROLLBACK;
        Self {
            total_rows: max_scrollback + rows,
            cells: vec![Cell::default(); (max_scrollback + rows) * cols],
            visible_rows: rows,
            cols,
            visible_start: max_scrollback,
            scroll_count: 0,
            max_scrollback,
            view_offset: 0,
            history_start: 0,
            damage_tracker: DamageTracker::new(rows, cols),
        }
    }

    // ── read-only accessors ─────────────────────────────────────────

    pub(crate) fn rows(&self) -> usize {
        self.visible_rows
    }

    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    pub(crate) fn view_offset(&self) -> usize {
        self.view_offset
    }

    pub(crate) fn scroll_count(&self) -> usize {
        self.scroll_count
    }

    #[allow(dead_code)]
    pub(crate) fn is_scrolled_back(&self) -> bool {
        self.view_offset > 0
    }

    pub(crate) fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub(crate) fn visible_start(&self) -> usize {
        self.visible_start
    }

    pub(crate) fn history_start(&self) -> u64 {
        self.history_start
    }
    #[allow(dead_code)]
    pub(crate) fn max_scrollback(&self) -> usize {
        self.max_scrollback
    }

    // ── row/col accessors (for write_char, avoiding manual index math) ──

    /// Returns a mutable reference to the cell at `(display_row, col)`
    /// in the **live** view (no scrollback offset).
    pub(crate) fn live_cell_mut(&mut self, display_row: usize, col: usize) -> &mut Cell {
        self.cell_mut(display_row, col)
    }

    // ── linear-index helpers (for Screen's ring-buffer operations) ──

    /// Returns a mutable reference to a cell by linear index.
    ///
    /// Screen uses this for direct per-cell writes after computing the
    /// ring-buffer index itself.
    pub(crate) fn cell_linear_mut(&mut self, index: usize) -> &mut Cell {
        &mut self.cells[index]
    }

    /// Returns a reference to a cell by linear index.
    pub(crate) fn cell_linear(&self, index: usize) -> &Cell {
        &self.cells[index]
    }

    /// Fills a contiguous range of cells with a specific cell value.
    pub(crate) fn fill_linear_range_with(&mut self, start: usize, end: usize, cell: Cell) {
        self.cells[start..end].fill(cell);
    }

    /// Copies cells within the ring buffer by linear index range.
    pub(crate) fn copy_linear_range(&mut self, src_start: usize, src_end: usize, dst: usize) {
        self.cells.copy_within(src_start..src_end, dst);
    }

    /// Copies cells within the ring buffer — delegates to `Vec::copy_within`.
    ///
    /// Accepts any `RangeBounds<usize>` so both inline ranges and Range variables work.
    pub(crate) fn copy_cells_within<R: std::ops::RangeBounds<usize>>(
        &mut self,
        src: R,
        dst: usize,
    ) {
        self.cells.copy_within(src, dst);
    }

    /// Returns the text content of a display row as a string.
    #[allow(dead_code)]
    pub(crate) fn row_text(&self, row: usize) -> String {
        assert!(row < self.visible_rows, "terminal row out of bounds");
        let ring_row = self.display_to_ring(row);
        let start = ring_row * self.cols;
        self.cells[start..start + self.cols]
            .iter()
            .map(|cell| cell.ch)
            .collect()
    }
    /// Maps a display row (0-based visible row) to its ring-buffer index.
    ///
    /// Caller must ensure `view_offset == 0` (live view) when calling this
    /// for *writing* — scrollback rows must not be mutated.
    #[inline]
    pub(crate) fn display_to_ring(&self, display_row: usize) -> usize {
        (self.visible_start + display_row) % self.total_rows
    }

    /// Returns a reference to the cell at `(display_row, col)`.
    pub(crate) fn cell(&self, display_row: usize, col: usize) -> &Cell {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        let actual_row = (top + display_row) % self.total_rows;
        &self.cells[actual_row * self.cols + col]
    }

    /// Returns a mutable reference to the cell at `(display_row, col)`.
    ///
    /// Safe to call only when `view_offset == 0` (writing to live view).
    pub(crate) fn cell_mut(&mut self, display_row: usize, col: usize) -> &mut Cell {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        let actual_row = self.display_to_ring(display_row);
        &mut self.cells[actual_row * self.cols + col]
    }

    /// Returns an iterator over all visible cells as `(display_row, col, ch)`.
    pub(crate) fn cells(&self) -> CellsIter<'_> {
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        CellsIter {
            cells: &self.cells,
            total_rows: self.total_rows,
            cols: self.cols,
            visible_rows: self.visible_rows,
            top,
            row: 0,
            col: 0,
        }
    }

    /// Returns dirty display-row indices.
    ///
    /// When `view_offset > 0` (scrolled back), every visible row is
    /// considered dirty.
    pub(crate) fn dirty_rows(&self) -> Vec<usize> {
        let mut rows: Vec<usize> = self.dirty_ranges().into_iter().map(|r| r.row).collect();
        rows.dedup();
        rows
    }

    pub(crate) fn dirty_ranges(&self) -> Vec<DirtyRange> {
        if self.view_offset > 0 {
            (0..self.visible_rows)
                .map(|row| DirtyRange {
                    row,
                    start_col: 0,
                    end_col: self.cols,
                })
                .collect()
        } else {
            self.damage_tracker.dirty_ranges()
        }
    }

    /// Resets all dirty flags to false.
    pub(crate) fn clear_dirty(&mut self) {
        self.damage_tracker.clear();
    }

    pub(crate) fn mark_row_dirty(&mut self, display_row: usize) {
        self.damage_tracker.mark_row_dirty(display_row);
    }

    #[allow(dead_code)]
    pub(crate) fn mark_cell_dirty(&mut self, display_row: usize, col: usize) {
        self.damage_tracker.mark_cell_dirty(display_row, col);
    }

    pub(crate) fn mark_range_dirty(
        &mut self,
        display_row: usize,
        start_col: usize,
        end_col: usize,
    ) {
        self.damage_tracker
            .mark_range_dirty(display_row, start_col, end_col);
    }

    pub(crate) fn mark_all_dirty(&mut self) {
        self.damage_tracker.mark_all_dirty();
    }

    /// Read a cell by stable generation coordinate.
    /// Returns `None` when the generation has been evicted from the ring buffer.
    pub(crate) fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
        debug_assert!(col < self.cols);
        if generation < self.history_start {
            return None;
        }
        let offset = (generation - self.history_start) as usize;
        if offset >= self.scroll_count + self.visible_rows {
            return None;
        }
        let ring_row =
            (self.visible_start + self.total_rows - self.scroll_count + offset) % self.total_rows;
        Some(&self.cells[ring_row * self.cols + col])
    }

    // ── viewport scroll (user scrolling through history) ────────────

    /// Scroll the viewport up by `n` rows (toward older history).
    pub(crate) fn scroll_up(&mut self, n: usize) {
        self.view_offset = (self.view_offset + n).min(self.scroll_count);
        self.mark_all_dirty();
    }

    /// Scroll the viewport down by `n` rows (toward live content).
    pub(crate) fn scroll_down(&mut self, n: usize) {
        self.view_offset = self.view_offset.saturating_sub(n);
        self.mark_all_dirty();
    }

    /// Snap the viewport back to the live bottom.
    pub(crate) fn scroll_to_bottom(&mut self) {
        self.view_offset = 0;
        self.mark_all_dirty();
    }

    // ── full-screen scroll (O(1) ring-buffer advance) ───────────────

    /// Advance the ring by `n` rows when full-screen scrolling.
    pub(crate) fn scroll_up_full_screen(&mut self, n: usize, cell: Cell) {
        tracing::debug!(
            n,
            visible_start = self.visible_start,
            total_rows = self.total_rows,
            scroll_count = self.scroll_count,
            view_offset = self.view_offset,
            "scroll_up_full_screen: advancing ring"
        );

        let n = n.min(self.visible_rows);
        let old_sc = self.scroll_count;
        self.scroll_count = (self.scroll_count + n).min(self.max_scrollback);
        self.visible_start = (self.visible_start + n) % self.total_rows;
        if old_sc + n > self.max_scrollback {
            self.history_start += (old_sc + n - self.max_scrollback) as u64;
        }
        // Blank the newly exposed rows at the bottom of the viewport.
        for i in 0..n {
            let row = (self.visible_start + self.visible_rows - 1 - i) % self.total_rows;
            self.cells[row * self.cols..(row + 1) * self.cols].fill(cell);
        }
        if self.view_offset > 0 {
            self.view_offset = (self.view_offset + n).min(self.scroll_count);
        }
        self.mark_all_dirty();

        tracing::debug!(
            n,
            visible_start = self.visible_start,
            scroll_count = self.scroll_count,
            view_offset = self.view_offset,
            "scroll_up_full_screen: ring advanced"
        );
    }
    // ── resize ──────────────────────────────────────────────────────

    /// Rebuilds the ring buffer for a new viewport size.
    ///
    /// Preserves the top-left visible rectangle.  Scrollback is discarded
    /// when column count changes (standard terminal behaviour).
    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        if self.visible_rows == rows && self.cols == cols {
            return;
        }

        tracing::debug!(
            old_rows = self.visible_rows,
            old_cols = self.cols,
            new_rows = rows,
            new_cols = cols,
            visible_start = self.visible_start,
            total_rows = self.total_rows,
            scroll_count = self.scroll_count,
            "resize: rebuilding ring buffer"
        );

        let new_total = self.max_scrollback + rows;
        let mut new_cells = vec![Cell::default(); new_total * cols];
        // Copy the overlapping top-left rectangle from the current viewport.
        let copied_rows = self.visible_rows.min(rows);
        let copied_cols = self.cols.min(cols);
        for d_row in 0..copied_rows {
            let ring_row = (self.visible_start + d_row) % self.total_rows;
            let old_start = ring_row * self.cols;
            let new_start = (self.max_scrollback + d_row) * cols;
            new_cells[new_start..new_start + copied_cols]
                .copy_from_slice(&self.cells[old_start..old_start + copied_cols]);
        }
        self.total_rows = new_total;
        self.cells = new_cells;
        self.visible_rows = rows;
        self.cols = cols;
        self.visible_start = self.max_scrollback;
        self.scroll_count = 0;
        self.view_offset = 0;
        self.history_start = 0;
        self.damage_tracker.resize(rows, cols);

        tracing::debug!(
            new_visible_start = self.visible_start,
            new_total_rows = self.total_rows,
            "resize: done"
        );
    }

    // ── bulk helpers for Screen's mutation methods ──

    #[inline]
    pub(crate) fn fill_row(&mut self, display_row: usize) {
        self.fill_row_with(display_row, Cell::default());
    }

    /// Fills one display row with a specific cell value.
    #[inline]
    pub(crate) fn fill_row_with(&mut self, display_row: usize, cell: Cell) {
        let ring_row = self.display_to_ring(display_row);

        tracing::debug!(
            display_row,
            ring_row,
            cols = self.cols,
            "fill_row_with: clearing row"
        );

        let start = ring_row * self.cols;
        self.cells[start..start + self.cols].fill(cell);
    }

    /// Fill every visible row with default cells.
    pub(crate) fn fill_all(&mut self) {
        tracing::debug!(
            visible_rows = self.visible_rows,
            "fill_all: clearing all visible rows"
        );

        for d in 0..self.visible_rows {
            self.fill_row(d);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirtyRange {
    pub(crate) row: usize,
    pub(crate) start_col: usize,
    pub(crate) end_col: usize, // exclusive
}

#[derive(Debug, Clone)]
pub(crate) struct DamageTracker {
    rows: usize,
    cols: usize,
    grid: Vec<bool>,
}

impl DamageTracker {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            grid: vec![true; rows * cols],
        }
    }

    pub(crate) fn resize(&mut self, new_rows: usize, new_cols: usize) {
        self.rows = new_rows;
        self.cols = new_cols;
        self.grid = vec![true; new_rows * new_cols];
    }

    pub(crate) fn clear(&mut self) {
        self.grid.fill(false);
    }

    pub(crate) fn mark_all_dirty(&mut self) {
        self.grid.fill(true);
    }

    pub(crate) fn mark_row_dirty(&mut self, row: usize) {
        if row < self.rows {
            let start = row * self.cols;
            let end = start + self.cols;
            self.grid[start..end].fill(true);
        }
    }

    pub(crate) fn mark_cell_dirty(&mut self, row: usize, col: usize) {
        if row < self.rows && col < self.cols {
            self.grid[row * self.cols + col] = true;
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
            }
        }
    }

    pub(crate) fn dirty_ranges(&self) -> Vec<DirtyRange> {
        let mut ranges = Vec::new();
        for row in 0..self.rows {
            let mut col = 0;
            while col < self.cols {
                if self.grid[row * self.cols + col] {
                    let start = col;
                    while col < self.cols && self.grid[row * self.cols + col] {
                        col += 1;
                    }
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
        ranges
    }
}

/// Iterator over visible cells in a `NormalBuf`.
///
/// Yields `(display_row, col, ch)` tuples in row-major order.
pub(crate) struct CellsIter<'a> {
    cells: &'a [Cell],
    total_rows: usize,
    cols: usize,
    visible_rows: usize,
    top: usize, // ring index of the first visible display row
    row: usize, // current display row (0 .. visible_rows)
    col: usize, // current column (0 .. cols)
}

impl<'a> Iterator for CellsIter<'a> {
    type Item = (usize, usize, char);

    fn next(&mut self) -> Option<Self::Item> {
        while self.row < self.visible_rows {
            if self.col < self.cols {
                let actual_row = (self.top + self.row) % self.total_rows;
                let ch = self.cells[actual_row * self.cols + self.col].ch;
                let item = (self.row, self.col, ch);
                self.col += 1;
                if self.col == self.cols {
                    self.col = 0;
                    self.row += 1;
                }
                return Some(item);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.visible_rows - self.row) * self.cols - self.col;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for CellsIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_blank_grid() {
        let buf = NormalBuf::new(3, 4);
        assert_eq!(buf.rows(), 3);
        assert_eq!(buf.cols(), 4);
        for row in 0..3 {
            for col in 0..4 {
                assert_eq!(buf.cell(row, col).ch, ' ');
            }
        }
    }

    #[test]
    fn write_and_read_cells() {
        let mut buf = NormalBuf::new(2, 3);
        buf.cell_mut(0, 0).ch = 'a';
        buf.cell_mut(0, 1).ch = 'b';
        buf.cell_mut(0, 2).ch = 'c';
        assert_eq!(buf.cell(0, 0).ch, 'a');
        assert_eq!(buf.cell(0, 1).ch, 'b');
        assert_eq!(buf.cell(0, 2).ch, 'c');
    }

    #[test]
    fn cell_mut_writes_through_ring() {
        let mut buf = NormalBuf::new(2, 3);
        buf.cell_mut(0, 0).ch = 'x';
        assert_eq!(buf.cell(0, 0).ch, 'x');
    }

    #[test]
    fn cells_iter_yields_row_major() {
        let mut buf = NormalBuf::new(2, 3);
        for i in 0..6 {
            let ch = (b'a' + i as u8) as char;
            buf.cells[buf.visible_start * buf.cols + i].ch = ch;
        }
        let collected: Vec<(usize, usize, char)> = buf.cells().collect();
        assert_eq!(collected.len(), 6);
        assert_eq!(collected[0], (0, 0, 'a'));
        assert_eq!(collected[1], (0, 1, 'b'));
        assert_eq!(collected[2], (0, 2, 'c'));
        assert_eq!(collected[3], (1, 0, 'd'));
        assert_eq!(collected[4], (1, 1, 'e'));
        assert_eq!(collected[5], (1, 2, 'f'));
    }

    #[test]
    fn scroll_up_full_screen_advances_ring() {
        let mut buf = NormalBuf::new(2, 3);
        let old_start = buf.visible_start;
        buf.scroll_up_full_screen(1, Cell::default());
        assert_eq!(
            buf.visible_start,
            (old_start + 1) % buf.total_rows,
            "ring head should advance by 1"
        );
        assert!(buf.scroll_count >= 1, "scrollback should increase");
        assert_eq!(buf.dirty_rows().len(), 2, "all rows should be dirty");
    }

    #[test]
    fn viewport_scroll_marks_all_dirty() {
        let mut buf = NormalBuf::new(3, 4);
        buf.clear_dirty();
        buf.scroll_count = 5;
        buf.scroll_up(2);
        assert_eq!(buf.view_offset, 2);
        assert_eq!(buf.dirty_rows().len(), 3);
    }

    #[test]
    fn dirty_rows_returns_all_when_scrolled_back() {
        let mut buf = NormalBuf::new(3, 4);
        buf.clear_dirty();
        buf.scroll_count = 5;
        buf.scroll_up(1);
        let dirty = buf.dirty_rows();
        assert_eq!(dirty.len(), 3, "all rows dirty when scrolled back");
    }

    #[test]
    fn resize_discards_scrollback() {
        let mut buf = NormalBuf::new(2, 3);
        buf.visible_start = (buf.visible_start + 5) % buf.total_rows;
        buf.scroll_count = 5;
        buf.resize(4, 5);
        assert_eq!(buf.rows(), 4);
        assert_eq!(buf.cols(), 5);
        assert_eq!(buf.scroll_count, 0, "scrollback discarded");
        assert_eq!(buf.view_offset, 0);
        assert_eq!(buf.dirty_rows().len(), 4);
    }

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

        // Mark single cell dirty
        tracker.mark_cell_dirty(1, 2);
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
    fn cells_iter_exact_size() {
        let buf = NormalBuf::new(2, 3);
        let iter = buf.cells();
        assert_eq!(iter.len(), 6);
        assert_eq!(iter.size_hint(), (6, Some(6)));
    }

    // ── generation stability ───────────────────────────────────────

    #[test]
    fn history_starts_at_zero() {
        let buf = NormalBuf::new(5, 10);
        assert_eq!(buf.history_start(), 0);
    }

    #[test]
    fn scroll_up_full_screen_eviction_increments_history_start() {
        let mut buf = NormalBuf::new(5, 3);
        let max = buf.max_scrollback();
        for _ in 0..max + 10 {
            buf.scroll_up_full_screen(1, Cell::default());
        }
        assert_eq!(buf.history_start(), 10);
        assert_eq!(buf.scroll_count(), max);
    }

    #[test]
    fn cell_at_generation_returns_none_for_evicted() {
        let mut buf = NormalBuf::new(5, 3);
        let max = buf.max_scrollback();
        for _ in 0..max + 1 {
            buf.scroll_up_full_screen(1, Cell::default());
        }
        assert!(
            buf.cell_at_generation(0, 0).is_none(),
            "evicted gen should be None"
        );
        assert!(
            buf.cell_at_generation(buf.history_start(), 0).is_some(),
            "oldest valid gen should be Some"
        );
    }

    #[test]
    fn cell_at_generation_returns_correct_content() {
        let mut buf = NormalBuf::new(5, 3);
        buf.live_cell_mut(0, 0).ch = 'X';
        let cell = buf
            .cell_at_generation(0, 0)
            .expect("valid gen should return cell");
        assert_eq!(cell.ch, 'X');
    }

    #[test]
    fn cell_at_generation_out_of_range_returns_none() {
        let buf = NormalBuf::new(5, 3);
        let max_gen = buf.scroll_count() + buf.rows();
        assert!(
            buf.cell_at_generation(max_gen as u64, 0).is_none(),
            "gen past end should be None"
        );
    }
}
