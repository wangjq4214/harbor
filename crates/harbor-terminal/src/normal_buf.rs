use crate::damage::{DamageTracker, DirtyRange};
use crate::screen::Cell;

/// Ring-buffer backed scrollback buffer.
///
/// Storage is a single `Vec<Cell>` of `total_rows * cols` elements.
/// Visible rows occupy `visible_rows` consecutive rows in the ring starting
/// at `visible_start`.  When the viewport is at the live bottom
/// (`view_offset == 0`), the ring head advances O(1) on full-screen scroll
/// — no cell copies, just a pointer bump and blank-fill of the newly
/// exposed row(s).
#[derive(Debug)]
pub struct NormalBuf {
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
    /// Per-ring-row soft-wrap flags, parallel to `cells`. `true` means this row
    /// is a continuation of the logical line from the row above.
    wrapped: Vec<bool>,
}

/// Copies a linear slice range with ring-buffer wraparound handling.
///
/// When `src_start <= src_end` the range is contiguous; otherwise it is split
/// into two parts `[src_start, ring_end)` and `[0, src_end)`, copied in order.
fn copy_ring_slice<T: Copy>(
    slice: &mut [T],
    src_start: usize,
    src_end: usize,
    dst: usize,
    ring_end: usize,
) {
    if src_start <= src_end {
        slice.copy_within(src_start..src_end, dst);
    } else {
        let first_len = ring_end - src_start;
        slice.copy_within(src_start..ring_end, dst);
        slice.copy_within(0..src_end, dst + first_len);
    }
}

impl NormalBuf {
    const DEFAULT_MAX_SCROLLBACK: usize = 1000;

    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let max_scrollback = Self::DEFAULT_MAX_SCROLLBACK;
        let total_rows = max_scrollback
            .checked_add(rows)
            .expect("terminal row count overflow");
        let cell_count = total_rows
            .checked_mul(cols)
            .expect("terminal cell count overflow");
        Self {
            total_rows,
            cells: vec![Cell::default(); cell_count],
            wrapped: vec![false; total_rows],
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

    pub fn rows(&self) -> usize {
        self.visible_rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn view_offset(&self) -> usize {
        self.view_offset
    }

    pub fn scroll_count(&self) -> usize {
        self.scroll_count
    }

    pub fn is_scrolled_back(&self) -> bool {
        self.view_offset > 0
    }

    pub(crate) fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub(crate) fn visible_start(&self) -> usize {
        self.visible_start
    }

    pub fn history_start(&self) -> u64 {
        self.history_start
    }
    #[allow(dead_code)]
    pub(crate) fn max_scrollback(&self) -> usize {
        self.max_scrollback
    }

    // ── row/col accessors (for write_char, avoiding manual index math) ──

    /// Returns a mutable reference to the cell at `(display_row, col)`
    /// in the **live** view (no scrollback offset).
    pub fn live_cell_mut(&mut self, display_row: usize, col: usize) -> &mut Cell {
        self.cell_mut(display_row, col)
    }

    /// Fills a row range `[start_col, end_col)` with a cell value and marks it dirty.
    pub fn fill_row_range(&mut self, row: usize, start_col: usize, end_col: usize, cell: Cell) {
        let start_col = start_col.min(self.cols);
        let end_col = end_col.min(self.cols);
        if start_col >= end_col {
            return;
        }
        let ring_row = self.display_to_ring(row);
        let start = ring_row * self.cols + start_col;
        let end = ring_row * self.cols + end_col;
        self.cells[start..end].fill(cell);
        self.mark_range_dirty(row, start_col, end_col);
    }

    /// Selectively erases unprotected cells in a row range `[start_col, end_col)` and marks it dirty.
    pub fn selective_erase_row_range(
        &mut self,
        row: usize,
        start_col: usize,
        end_col: usize,
        erase: Cell,
    ) {
        let start_col = start_col.min(self.cols);
        let end_col = end_col.min(self.cols);
        if start_col >= end_col {
            return;
        }
        let ring_row = self.display_to_ring(row);
        let start = ring_row * self.cols + start_col;
        let end = ring_row * self.cols + end_col;
        for idx in start..end {
            if !self.cells[idx].protected {
                self.cells[idx] = erase;
            }
        }
        self.mark_range_dirty(row, start_col, end_col);
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
    #[allow(dead_code)]
    pub(crate) fn copy_cells_within<R: std::ops::RangeBounds<usize>>(
        &mut self,
        src: R,
        dst: usize,
    ) {
        self.cells.copy_within(src, dst);
    }

    /// Copies a linear range `[src_start, src_end)` to `dst`, correctly
    /// handling the case where the source range wraps around the end of
    /// the ring buffer. All three arguments are linear cell indices.
    ///
    /// When `src_start <= src_end` the range is contiguous; otherwise
    /// it is split into two parts: `[src_start, ring_end)` and
    /// `[0, src_end)`, and both are copied in order.
    pub(crate) fn copy_ring_range(&mut self, src_start: usize, src_end: usize, dst: usize) {
        let ring_end = self.total_rows * self.cols;
        copy_ring_slice(&mut self.cells[..], src_start, src_end, dst, ring_end);
    }

    /// Returns the text content of a display row as a string.
    #[allow(dead_code)]
    pub fn row_text(&self, row: usize) -> String {
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
    pub fn display_to_ring(&self, display_row: usize) -> usize {
        (self.visible_start + display_row) % self.total_rows
    }

    /// Returns a reference to the cell at `(display_row, col)`.
    pub fn cell(&self, display_row: usize, col: usize) -> &Cell {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        let actual_row = (top + display_row) % self.total_rows;
        &self.cells[actual_row * self.cols + col]
    }

    /// Returns a mutable reference to the cell at `(display_row, col)`.
    ///
    /// Safe to call only when `view_offset == 0` (writing to live view).
    pub fn cell_mut(&mut self, display_row: usize, col: usize) -> &mut Cell {
        debug_assert!(display_row < self.visible_rows);
        debug_assert!(col < self.cols);
        self.mark_range_dirty(display_row, col, col + 1);
        let actual_row = self.display_to_ring(display_row);
        &mut self.cells[actual_row * self.cols + col]
    }

    /// Returns an iterator over all visible cells as `(display_row, col, ch)`.
    pub fn cells(&self) -> CellsIter<'_> {
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
    pub fn dirty_rows(&self) -> Vec<usize> {
        let mut rows: Vec<usize> = self.dirty_ranges().into_iter().map(|r| r.row).collect();
        rows.dedup();
        rows
    }

    pub fn dirty_ranges(&self) -> Vec<DirtyRange> {
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
    pub fn clear_dirty(&mut self) {
        self.damage_tracker.clear();
    }

    pub fn mark_row_dirty(&mut self, display_row: usize) {
        self.damage_tracker.mark_row_dirty(display_row);
    }

    pub fn mark_rows_dirty(&mut self, start_row: usize, end_row: usize) {
        self.damage_tracker.mark_rows_dirty(start_row, end_row);
    }

    pub fn mark_range_dirty(&mut self, display_row: usize, start_col: usize, end_col: usize) {
        self.damage_tracker
            .mark_range_dirty(display_row, start_col, end_col);
    }

    pub fn mark_all_dirty(&mut self) {
        self.damage_tracker.mark_all_dirty();
    }

    /// Read a cell by stable generation coordinate.
    /// Returns `None` when the generation has been evicted from the ring buffer.
    pub fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
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

    /// Returns whether the given display row is a soft-wrapped continuation
    /// of the logical line from the row above.
    pub fn is_wrapped(&self, display_row: usize) -> bool {
        debug_assert!(display_row < self.visible_rows);
        let top = (self.visible_start + self.total_rows - self.view_offset) % self.total_rows;
        self.wrapped[(top + display_row) % self.total_rows]
    }

    /// Sets the soft-wrap flag for a live display row.
    pub(crate) fn set_wrapped(&mut self, display_row: usize, wrapped: bool) {
        debug_assert!(display_row < self.visible_rows);
        let ring_row = self.display_to_ring(display_row);
        self.wrapped[ring_row] = wrapped;
    }

    /// Copies soft-wrap flags for a row range, mirroring `copy_ring_range`
    /// but operating on ring-row indices and handling ring-buffer wraparound.
    pub(crate) fn copy_wrapped_ring_range(&mut self, src_start: usize, src_end: usize, dst: usize) {
        let ring_end = self.total_rows;
        copy_ring_slice(&mut self.wrapped[..], src_start, src_end, dst, ring_end);
    }

    // ── viewport scroll (user scrolling through history) ────────────

    /// Scroll the viewport up by `n` rows (toward older history).
    pub fn scroll_up(&mut self, n: usize) {
        self.view_offset = (self.view_offset + n).min(self.scroll_count);
        self.mark_all_dirty();
    }

    /// Scroll the viewport down by `n` rows (toward live content).
    pub fn scroll_down(&mut self, n: usize) {
        self.view_offset = self.view_offset.saturating_sub(n);
        self.mark_all_dirty();
    }

    /// Snap the viewport back to the live bottom.
    pub fn scroll_to_bottom(&mut self) {
        self.view_offset = 0;
        self.mark_all_dirty();
    }

    // ── full-screen scroll (O(1) ring-buffer advance) ───────────────

    /// Advance the ring by `n` rows when full-screen scrolling.
    pub fn scroll_up_full_screen(&mut self, n: usize, cell: Cell) {
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
            self.wrapped[row] = false;
        }
        if self.view_offset > 0 {
            self.view_offset = (self.view_offset + n).min(self.scroll_count);
        }
        // TODO: row remapping would reduce this to O(n)
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

    /// Rebuilds the ring buffer for a new viewport size while retaining available scrollback.
    ///
    /// Rows are copied without reflow.  When the viewport is scrolled back, the existing
    /// generation coordinates and displayed history remain valid up to the new capacity.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(1);
        let cols = cols.max(1);
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

        let old_rows = self.visible_rows;
        let old_cols = self.cols;
        let old_total = self.total_rows;
        let old_visible_start = self.visible_start;
        let old_scroll_count = self.scroll_count;
        let old_history_start = self.history_start;
        let keep_history = old_scroll_count.min(self.max_scrollback);
        let copied_live_rows = old_rows.min(rows);
        let copied_cols = old_cols.min(cols);
        let new_total = self
            .max_scrollback
            .checked_add(rows)
            .expect("terminal row count overflow");
        let new_cell_count = new_total
            .checked_mul(cols)
            .expect("terminal cell count overflow");
        let old_cells = std::mem::take(&mut self.cells);
        let old_wrapped = std::mem::take(&mut self.wrapped);
        let mut new_cells = vec![Cell::default(); new_cell_count];
        let mut new_wrapped = vec![false; new_total];

        // Copy the retained history in generation order immediately before the live viewport.
        for history_index in 0..keep_history {
            let old_sequence_index = old_scroll_count - keep_history + history_index;
            let old_ring_row =
                (old_visible_start + old_total - old_scroll_count + old_sequence_index) % old_total;
            let new_ring_row = (self.max_scrollback - keep_history + history_index) % new_total;
            let old_start = old_ring_row * old_cols;
            let new_start = new_ring_row * cols;
            new_cells[new_start..new_start + copied_cols]
                .copy_from_slice(&old_cells[old_start..old_start + copied_cols]);
            new_wrapped[new_ring_row] = old_wrapped[old_ring_row];
        }

        // Preserve the top-left rectangle of the live viewport; newly exposed rows are blank.
        for live_row in 0..copied_live_rows {
            let old_ring_row =
                (old_visible_start + old_total - old_scroll_count + old_scroll_count + live_row)
                    % old_total;
            let new_ring_row = (self.max_scrollback + live_row) % new_total;
            let old_start = old_ring_row * old_cols;
            let new_start = new_ring_row * cols;
            new_cells[new_start..new_start + copied_cols]
                .copy_from_slice(&old_cells[old_start..old_start + copied_cols]);
            new_wrapped[new_ring_row] = old_wrapped[old_ring_row];
        }

        self.total_rows = new_total;
        self.cells = new_cells;
        self.wrapped = new_wrapped;
        self.visible_rows = rows;
        self.cols = cols;
        self.visible_start = self.max_scrollback;
        self.scroll_count = keep_history;
        self.view_offset = self.view_offset.min(keep_history);
        self.history_start =
            old_history_start.saturating_add((old_scroll_count - keep_history) as u64);
        self.damage_tracker.resize(rows, cols);

        tracing::debug!(
            new_visible_start = self.visible_start,
            new_total_rows = self.total_rows,
            scroll_count = self.scroll_count,
            view_offset = self.view_offset,
            history_start = self.history_start,
            "resize: done"
        );
    }

    // ── bulk helpers for Screen's mutation methods ──

    #[inline]
    pub fn fill_row(&mut self, display_row: usize) {
        self.fill_row_with(display_row, Cell::default());
    }

    /// Fills one display row with a specific cell value.
    #[inline]
    pub fn fill_row_with(&mut self, display_row: usize, cell: Cell) {
        let ring_row = self.display_to_ring(display_row);

        tracing::debug!(
            display_row,
            ring_row,
            cols = self.cols,
            "fill_row_with: clearing row"
        );

        let start = ring_row * self.cols;
        self.cells[start..start + self.cols].fill(cell);
        self.wrapped[ring_row] = false;
    }

    /// Fill every visible row with the supplied cell value.
    pub fn fill_all_with(&mut self, cell: Cell) {
        tracing::debug!(
            visible_rows = self.visible_rows,
            "fill_all_with: replacing all visible rows"
        );

        for d in 0..self.visible_rows {
            self.fill_row_with(d, cell);
        }
    }

    /// Fill every visible row with default cells.
    pub fn fill_all(&mut self) {
        self.fill_all_with(Cell::default());
    }
}

/// Iterator over visible cells in a `NormalBuf`.
///
/// Yields `(display_row, col, ch)` tuples in row-major order.
pub struct CellsIter<'a> {
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
    fn resize_preserves_scrollback_generations() {
        let mut buf = NormalBuf::new(2, 3);
        for ch in ['A', 'B', 'C'] {
            buf.live_cell_mut(0, 0).ch = ch;
            buf.scroll_up_full_screen(1, Cell::default());
        }
        buf.live_cell_mut(0, 0).ch = 'D';
        buf.scroll_up(2);
        let displayed = buf.row_text(0);
        let history_start = buf.history_start();

        buf.resize(3, 5);

        assert_eq!(buf.rows(), 3);
        assert_eq!(buf.cols(), 5);
        assert_eq!(buf.scroll_count, 3);
        assert_eq!(buf.view_offset, 2);
        assert_eq!(buf.history_start, history_start);
        assert_eq!(buf.row_text(0), format!("{displayed}  "));
        assert_eq!(
            buf.cell_at_generation(history_start, 0)
                .expect("oldest retained generation")
                .ch,
            'A'
        );
        assert_eq!(
            buf.cell_at_generation(history_start + 2, 0)
                .expect("latest retained history generation")
                .ch,
            'C'
        );
        assert_eq!(buf.dirty_rows().len(), 3);
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
    fn resize_after_ring_wrap_preserves_generation_coordinates() {
        let mut buf = NormalBuf::new(2, 2);
        let max = buf.max_scrollback();
        for index in 0..max + 7 {
            buf.live_cell_mut(0, 0).ch = (b'a' + (index % 26) as u8) as char;
            buf.scroll_up_full_screen(1, Cell::default());
        }
        buf.scroll_up(1);
        let displayed = buf.row_text(0);
        let history_start = buf.history_start();

        buf.resize(2, 4);

        assert_eq!(buf.history_start(), history_start);
        assert_eq!(buf.scroll_count(), max);
        assert_eq!(buf.view_offset(), 1);
        assert_eq!(buf.row_text(0), format!("{displayed}  "));
        assert_eq!(
            buf.cell_at_generation(history_start, 0)
                .expect("oldest retained generation")
                .ch,
            (b'a' + 7) as char
        );
        assert!(buf.cell_at_generation(history_start - 1, 0).is_none());
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

    // ── per-row soft-wrap flags ───────────────────────────────────

    #[test]
    fn should_roundtrip_wrapped_flag_through_set_and_get() {
        // Arrange
        let mut buf = NormalBuf::new(3, 4);
        // Act
        buf.set_wrapped(1, true);
        // Assert
        assert!(!buf.is_wrapped(0), "unset rows default to unwrapped");
        assert!(buf.is_wrapped(1), "set_wrapped must persist the flag");
        assert!(!buf.is_wrapped(2), "unset rows default to unwrapped");
        buf.set_wrapped(1, false);
        assert!(!buf.is_wrapped(1), "set_wrapped(false) must clear the flag");
    }

    #[test]
    fn should_clear_wrapped_flag_when_fill_row_with() {
        // Arrange
        let mut buf = NormalBuf::new(2, 3);
        buf.set_wrapped(0, true);
        buf.set_wrapped(1, true);
        // Act
        buf.fill_row_with(0, Cell::default());
        // Assert
        assert!(!buf.is_wrapped(0), "filled row must clear its wrap flag");
        assert!(buf.is_wrapped(1), "untouched row must keep its wrap flag");
    }

    #[test]
    fn should_fill_every_visible_row_with_supplied_cell_and_clear_wrap_flags() {
        // Arrange
        let mut buf = NormalBuf::new(3, 2);
        for row in 0..buf.rows() {
            buf.set_wrapped(row, true);
        }
        let cell = Cell {
            ch: 'E',
            ..Cell::default()
        };

        // Act
        buf.fill_all_with(cell);

        // Assert
        for row in 0..buf.rows() {
            for col in 0..buf.cols() {
                assert_eq!(buf.cell(row, col), &cell);
            }
            assert!(!buf.is_wrapped(row));
        }
    }

    #[test]
    fn should_keep_default_blank_behavior_for_fill_all() {
        // Arrange
        let mut buf = NormalBuf::new(1, 2);
        buf.cell_mut(0, 0).ch = 'X';

        // Act
        buf.fill_all();

        // Assert
        assert_eq!(buf.row_text(0), "  ");
    }

    #[test]
    fn should_preserve_wrapped_flags_on_surviving_rows_and_blank_new_rows_when_resize() {
        // Arrange
        let mut buf = NormalBuf::new(2, 3);
        buf.set_wrapped(0, true);
        buf.set_wrapped(1, true);
        // Act
        buf.resize(4, 5);
        // Assert
        assert!(buf.is_wrapped(0), "surviving row 0 keeps its flag");
        assert!(buf.is_wrapped(1), "surviving row 1 keeps its flag");
        assert!(!buf.is_wrapped(2), "newly exposed row 2 is unwrapped");
        assert!(!buf.is_wrapped(3), "newly exposed row 3 is unwrapped");
    }

    #[test]
    fn should_clear_wrapped_flags_on_blanked_rows_when_scroll_up_full_screen() {
        // Arrange
        let mut buf = NormalBuf::new(2, 3);
        buf.set_wrapped(0, true);
        buf.set_wrapped(1, true);
        // Act
        buf.scroll_up_full_screen(1, Cell::default());
        // Assert: ring advanced — old row 1 is now display row 0 (flag kept),
        // the newly blanked bottom row is unwrapped.
        assert!(buf.is_wrapped(0), "scrolled-up row keeps its flag");
        assert!(!buf.is_wrapped(1), "blanked bottom row is unwrapped");
    }

    #[test]
    fn should_handle_ring_wraparound_when_copy_wrapped_ring_range() {
        // Arrange — mark ring rows on both sides of the wraparound boundary.
        let mut buf = NormalBuf::new(2, 3);
        let total = buf.total_rows();
        buf.wrapped[total - 2] = true;
        buf.wrapped[total - 1] = true;
        buf.wrapped[0] = true;
        buf.wrapped[1] = true;
        // Act — copy [total-2, total) then [0, 2) into dst = 2.
        buf.copy_wrapped_ring_range(total - 2, 2, 2);
        // Assert — the two segments land contiguously at dst.
        assert!(buf.wrapped[2]);
        assert!(buf.wrapped[3]);
        assert!(buf.wrapped[4]);
        assert!(buf.wrapped[5]);
        assert!(!buf.wrapped[6], "no write beyond the destination range");
    }
}
