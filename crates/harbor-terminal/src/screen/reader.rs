//! Read-only queries over terminal screen state.
//!
//! `ScreenReader` takes a shared reference to `Screen` and produces
//! snapshots and extracted text without mutation. This separates
//! the read path from the mutation methods on `Screen`.

use harbor_types::{SelectionBounds, TerminalSnapshot};

use super::Screen;

/// Façade for read-only screen queries — snapshots, text extraction, etc.
///
/// Created via `Screen::reader()`. All methods are `&self` (no mutation).
pub struct ScreenReader<'a> {
    screen: &'a Screen,
}

impl<'a> ScreenReader<'a> {
    pub(crate) fn new(screen: &'a Screen) -> Self {
        Self { screen }
    }

    /// Builds a full `TerminalSnapshot` for the UI/update contract.
    pub fn terminal_snapshot(&self) -> TerminalSnapshot {
        let rows = self.screen.rows();
        let cols = self.screen.cols();
        let mut cells = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                cells.push(*self.screen.cell(r, c));
            }
        }
        TerminalSnapshot {
            rows,
            cols,
            cells,
            cursor_x: self.screen.cursor_x(),
            cursor_y: self.screen.cursor_y(),
            cursor_visible: self.screen.cursor_visible(),
            cursor_blink: self.screen.cursor_blink(),
            cursor_shape: self.screen.cursor_shape(),
            scroll_count: self.screen.scroll_count(),
            view_offset: self.screen.view_offset(),
            history_start: self.screen.history_start(),
            wrapped: (0..rows).map(|row| self.screen.is_wrapped(row)).collect(),
            is_alt: self.screen.is_alt(),
            input_modes: self.screen.input_modes(),
            dirty_ranges: self.screen.dirty_ranges(),
        }
    }

    /// Extracts text between two generation coordinates, trimming trailing
    /// whitespace from each row and joining with newlines.
    pub fn selected_text(&self, bounds: SelectionBounds) -> String {
        let SelectionBounds {
            start_row,
            start_col,
            end_row,
            end_col,
        } = bounds;
        let cols = self.screen.cols();
        let hist_start = self.screen.history_start();
        let scroll_count = self.screen.scroll_count();
        let visible_rows = self.screen.visible_rows();
        let retained_rows = scroll_count + visible_rows;
        let max_gen = hist_start + retained_rows as u64 - 1;

        let orig_start = start_row;
        let orig_end = end_row;
        let start_row = start_row.max(hist_start);
        let end_row = end_row.min(max_gen);
        if start_row > end_row {
            return String::new();
        }

        let mut buf = String::new();

        for generation in start_row..=end_row {
            let col_start = if generation == orig_start {
                start_col
            } else {
                0
            };
            let col_end = if generation == orig_end {
                end_col
            } else {
                cols.saturating_sub(1)
            };

            let row_len_before = buf.len();
            for col in col_start..=col_end {
                let Some(cell) = self.screen.cell_at_generation(generation, col) else {
                    continue;
                };
                if cell.wide_continuation {
                    continue;
                }
                buf.push(cell.ch);
            }
            let row_text = &buf[row_len_before..];
            let trim_len = row_text.trim_end().len();
            buf.truncate(row_len_before + trim_len);
            if generation < end_row && !self.screen.is_wrapped_at_generation(generation + 1) {
                buf.push('\n');
            }
        }
        buf
    }
}
