//! Terminal screen: thin coordinator delegating to three internal engines.
//!
//! - [`cursor::CursorEngine`] — cursor position, scroll region, margins, modes
//! - [`edit::VtEditEngine`]   — pen (SGR), tab stops, charsets, cell mutations
//! - [`alt::AltScreenStack`]  — alt-screen flag and pending request
//!
//! `Screen` keeps the public API stable; most methods are one-line
//! delegations.  Cross-engine operations (e.g. `reverse_index`,
//! `scroll_region_up_one`) stay here.

mod alt;
mod cursor;
mod edit;
mod reader;
#[cfg(test)]
mod tests;

use crate::normal_buf::CellsIter;
use crate::{DirtyRange, InputModes, NormalBuf};
use harbor_parser::Params;

use self::alt::AltScreenStack;
use self::cursor::CursorEngine;
use self::edit::{TabStops, VtEditEngine};

pub use self::reader::ScreenReader;

// ── re-exports ────────────────────────────────────────────────────────

pub use harbor_types::AltScreenAction;
pub use harbor_types::Cell;
pub use harbor_types::CellAttrs;
pub use harbor_types::CharacterProtection;
pub use harbor_types::Color;
pub use harbor_types::CursorShape;
pub use harbor_types::CursorStyleArg;
pub use harbor_types::SelectionBounds;

// ── Screen ────────────────────────────────────────────────────────────

/// Visible terminal screen state rendered by the text pipeline.
///
/// `Screen` owns only display state: cell contents, dimensions, and cursor position. It does not
/// parse byte streams; `TerminalParser` calls these methods after recognizing control sequences.
#[derive(Debug)]
pub struct Screen {
    /// Ring-buffer scrollback storage.
    normal: NormalBuf,
    /// Cursor position, scroll region, margins, and terminal modes.
    cursor: CursorEngine,
    /// Pen state, tab stops, and character-set designations.
    edit: VtEditEngine,
    /// Alt-screen flag and pending request.
    alt: AltScreenStack,
    /// Saved normal-screen state while the alternate screen is active.
    alt_saved: Option<Box<Screen>>,
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            normal: NormalBuf::new(rows, cols),
            cursor: CursorEngine::new(rows, cols),
            edit: VtEditEngine::new(cols),
            alt: AltScreenStack::new(),
            alt_saved: None,
        }
    }

    // ── dimensions / viewport ──────────────────────────────────────────

    pub fn rows(&self) -> usize {
        self.normal.rows()
    }

    pub fn cols(&self) -> usize {
        self.normal.cols()
    }

    pub fn scroll_count(&self) -> usize {
        self.normal.scroll_count()
    }

    pub fn view_offset(&self) -> usize {
        self.normal.view_offset()
    }

    pub fn visible_rows(&self) -> usize {
        self.normal.rows()
    }

    pub fn history_start(&self) -> u64 {
        self.normal.history_start()
    }

    // ── cursor queries ─────────────────────────────────────────────────

    pub fn cursor_x(&self) -> usize {
        self.cursor.cursor_x()
    }

    pub fn cursor_y(&self) -> usize {
        self.cursor.cursor_y(&self.normal)
    }

    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor.cursor_shape()
    }

    pub fn cursor_blink(&self) -> bool {
        self.cursor.cursor_blink()
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor.cursor_visible()
    }

    pub fn set_cursor_style(&mut self, arg: CursorStyleArg) {
        self.cursor.set_cursor_style(arg);
    }

    pub fn input_modes(&self) -> InputModes {
        self.cursor.input_modes()
    }

    pub fn margin_mode(&self) -> bool {
        self.cursor.margin_mode()
    }

    // ── cell access ────────────────────────────────────────────────────

    pub fn cells(&self) -> CellsIter<'_> {
        self.normal.cells()
    }

    pub fn cell_char(&self, row: usize, col: usize) -> char {
        self.normal.cell(row, col).ch
    }

    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        self.normal.cell(row, col)
    }

    pub fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
        self.normal.cell_at_generation(generation, col)
    }

    /// Direct cell mutation for test setup.
    #[cfg(test)]
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        self.normal.cell_mut(row, col)
    }

    // ── read-only queries ──────────────────────────────────────────────

    /// Returns a `ScreenReader` for snapshot and text-extraction queries.
    pub fn reader(&self) -> ScreenReader<'_> {
        ScreenReader::new(self)
    }

    pub fn terminal_snapshot(&self) -> harbor_types::TerminalSnapshot {
        self.reader().terminal_snapshot()
    }

    pub fn selected_text(&self, bounds: SelectionBounds) -> String {
        self.reader().selected_text(bounds)
    }

    // ── dirty tracking ─────────────────────────────────────────────────

    pub fn dirty_rows(&self) -> Vec<usize> {
        self.normal.dirty_rows()
    }

    pub fn dirty_ranges(&self) -> Vec<DirtyRange> {
        self.normal.dirty_ranges()
    }

    pub fn clear_dirty(&mut self) {
        self.normal.clear_dirty()
    }

    pub fn mark_row_dirty(&mut self, row: usize) {
        self.normal.mark_row_dirty(row);
    }

    pub fn mark_rows_dirty(&mut self, start_row: usize, end_row: usize) {
        self.normal.mark_rows_dirty(start_row, end_row);
    }

    pub fn mark_range_dirty(&mut self, row: usize, start_col: usize, end_col: usize) {
        self.normal.mark_range_dirty(row, start_col, end_col);
    }

    pub fn mark_all_dirty(&mut self) {
        self.normal.mark_all_dirty();
    }

    // ── viewport scroll ────────────────────────────────────────────────

    pub fn scroll_up(&mut self, n: usize) {
        self.normal.scroll_up(n);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.normal.scroll_down(n);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.normal.scroll_to_bottom();
    }

    // ── alt screen ─────────────────────────────────────────────────────

    pub fn is_alt(&self) -> bool {
        self.alt.is_alt()
    }

    pub fn request_alt_enter(&mut self) {
        self.alt.request_enter();
    }

    pub fn request_alt_exit(&mut self) {
        self.alt.request_exit();
    }

    pub fn alt_request(&self) -> Option<AltScreenAction> {
        self.alt.alt_request()
    }

    pub fn take_alt_request(&mut self) -> Option<AltScreenAction> {
        self.alt.take_alt_request()
    }

    pub fn enter_alt(&mut self) {
        if self.alt.is_alt() {
            return;
        }
        let rows = self.rows();
        let cols = self.cols();
        let saved = std::mem::replace(self, Self::new(rows, cols));
        self.alt_saved = Some(Box::new(saved));
        self.alt.mark_active();
    }

    pub fn exit_alt(&mut self) {
        if let Some(saved) = self.alt_saved.take() {
            *self = *saved;
            self.mark_all_dirty();
        }
        self.alt.mark_inactive();
    }

    // ── resize ─────────────────────────────────────────────────────────

    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        self.normal.resize(rows, cols);
        self.cursor.clamp_to_grid(rows, cols);
        self.edit.tab_stops.resize(cols);
        if let Some(saved) = &mut self.alt_saved {
            saved.resize(rows, cols);
        }
    }

    // ── cursor movement (delegated) ────────────────────────────────────

    pub fn cursor_up(&mut self, n: usize) {
        self.cursor.cursor_up(n);
    }

    pub fn cursor_down(&mut self, n: usize) {
        self.cursor.cursor_down(&self.normal, n);
    }

    pub fn cursor_left(&mut self, n: usize) {
        self.cursor.cursor_left(n);
    }

    pub fn cursor_right(&mut self, n: usize) {
        self.cursor.cursor_right(&self.normal, n);
    }

    pub fn carriage_return(&mut self) {
        self.cursor.carriage_return();
    }

    pub fn backspace(&mut self) {
        self.cursor.backspace(&self.normal);
    }

    pub fn set_cursor_position(&mut self, row_1_based: usize, col_1_based: usize) {
        self.cursor
            .set_cursor_position(&self.normal, row_1_based, col_1_based);
    }

    pub fn set_cursor_col(&mut self, col_1_based: usize) {
        self.cursor.set_cursor_col(&self.normal, col_1_based);
    }

    pub fn set_cursor_row(&mut self, row_1_based: usize) {
        self.cursor.set_cursor_row(&self.normal, row_1_based);
    }

    pub fn set_cursor(&mut self, row_1_based: usize, col_1_based: usize) {
        self.cursor
            .set_cursor(&self.normal, row_1_based, col_1_based);
    }

    pub fn home_cursor(&mut self) {
        self.cursor.home_cursor();
    }

    // ── scroll region / margins ────────────────────────────────────────

    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.cursor.set_scroll_region(&self.normal, top, bottom);
    }

    pub fn set_left_right_margins(&mut self, left: usize, right: usize) {
        self.cursor
            .set_left_right_margins(&self.normal, left, right);
    }

    // ── modes ──────────────────────────────────────────────────────────

    pub fn set_private_mode(&mut self, param: usize, enabled: bool) {
        match param {
            1049 => {
                if enabled {
                    self.request_alt_enter();
                } else {
                    self.request_alt_exit();
                }
            }
            other => {
                if !self.cursor.set_private_mode(&self.normal, other, enabled) {
                    tracing::warn!("unsupported private mode: ?{}", other);
                }
            }
        }
    }

    pub fn set_standard_mode(&mut self, param: usize, enabled: bool) {
        if !self.cursor.set_standard_mode(param, enabled) {
            tracing::warn!("unsupported standard mode: {}", param);
        }
    }

    pub fn set_application_keypad(&mut self, enabled: bool) {
        self.cursor.set_application_keypad(enabled);
    }

    // ── SGR / charsets / protection ────────────────────────────────────

    pub fn set_sgr(&mut self, params: &Params) {
        self.edit.set_sgr(params);
    }

    pub fn set_sgr_slice(&mut self, slice: &[Option<usize>]) {
        self.edit.set_sgr_slice(slice);
    }

    pub fn designate_g0(&mut self, charset: u8) {
        self.edit.designate_g0(charset);
    }

    pub fn designate_g1(&mut self, charset: u8) {
        self.edit.designate_g1(charset);
    }

    pub fn set_active_charset(&mut self, active: u8) {
        self.edit.set_active_charset(active);
    }

    pub fn set_character_protection(&mut self, arg: CharacterProtection) {
        self.edit.set_character_protection(arg);
    }

    // ── erase ──────────────────────────────────────────────────────────

    pub fn erase_display(&mut self, mode: usize) {
        // We need to split borrows: self.edit needs &mut self, self.cursor needs &mut self.
        // destructure to satisfy borrow checker.
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.erase_display(normal, cursor, mode);
    }

    pub fn erase_line(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.erase_line(normal, cursor, mode);
    }

    pub fn erase_chars(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.erase_chars(normal, cursor, n);
    }

    pub fn selective_erase_display(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.selective_erase_display(normal, cursor, mode);
    }

    pub fn selective_erase_line(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.selective_erase_line(normal, cursor, mode);
    }

    // ── insert / delete ────────────────────────────────────────────────

    pub fn insert_chars(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.insert_chars(normal, cursor, n);
    }

    pub fn delete_chars(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.delete_chars(normal, cursor, n);
    }

    pub fn insert_lines(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.insert_lines(normal, cursor, n);
    }

    pub fn delete_lines(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.delete_lines(normal, cursor, n);
    }

    // ── scroll region (CSI S / CSI T) ──────────────────────────────────

    pub fn scroll_up_region(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.scroll_up_region(normal, cursor, n);
    }

    pub fn scroll_down_region(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.scroll_down_region(normal, cursor, n);
    }

    // ── DEC rectangle ops ──────────────────────────────────────────────

    pub fn decera(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.decera(normal, cursor, params);
    }

    pub fn decsera(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.decsera(normal, cursor, params);
    }

    pub fn decfra(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.decfra(normal, cursor, params);
    }

    pub fn deccra(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.deccra(normal, cursor, params);
    }

    pub fn deccara(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.deccara(normal, cursor, params);
    }

    pub fn decrara(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.decrara(normal, cursor, params);
    }

    // ── tab stops ──────────────────────────────────────────────────────

    pub fn set_tab_stop(&mut self) {
        self.edit.set_tab_stop(self.cursor.cursor.x);
    }

    pub fn clear_tab_stops(&mut self, mode: usize) {
        self.edit.clear_tab_stops(self.cursor.cursor.x, mode);
    }

    // ── cursor save / restore ──────────────────────────────────────────

    pub fn save_cursor(&mut self) {
        self.cursor.save_cursor(&self.edit.pen);
    }

    pub fn restore_cursor(&mut self) {
        self.cursor.restore_cursor(&mut self.edit.pen);
    }

    // ── write_char (coordinator) ───────────────────────────────────────

    pub fn write_char(&mut self, ch: char) {
        let Screen {
            normal,
            cursor,
            edit,
            ..
        } = self;
        edit.write_char(normal, cursor, ch);
    }

    // ── horizontal_tab (coordinator) ───────────────────────────────────

    pub fn horizontal_tab(&mut self) {
        self.cursor.modes.pending_wrap = false;
        let right_limit = if self.cursor.margins.enabled {
            self.cursor.margins.right
        } else {
            self.normal.cols()
        };
        let mut target = right_limit;
        for col in (self.cursor.cursor.x + 1)..=right_limit {
            if col < self.edit.tab_stops.0.len() && self.edit.tab_stops.0[col] {
                target = col;
                break;
            }
        }
        if target > self.cursor.cursor.x {
            let spaces = target - self.cursor.cursor.x;
            let Screen {
                normal,
                cursor,
                edit,
                ..
            } = self;
            for _ in 0..spaces {
                edit.write_char(normal, cursor, ' ');
            }
        }
    }

    // ── repeat_char (coordinator) ──────────────────────────────────────

    pub fn repeat_char(&mut self, n: usize) {
        if let Some(ch) = self.edit.charsets.last_char {
            let n = if n == 0 { 1 } else { n };
            let count = n.min(self.normal.cols());
            let Screen {
                normal,
                cursor,
                edit,
                ..
            } = self;
            for _ in 0..count {
                edit.write_char(normal, cursor, ch);
            }
        }
    }

    // ── newline / line_feed / index (coordinators) ─────────────────────

    pub fn newline(&mut self) {
        self.cursor.carriage_return();
        self.index();
    }

    pub fn line_feed(&mut self) {
        if self.cursor.modes.line_feed {
            self.cursor.carriage_return();
        }
        self.index();
    }

    pub fn index(&mut self) {
        if self.cursor.index_needs_scroll() {
            self.scroll_region_up_one();
        } else {
            self.cursor.index_advance(&self.normal);
        }
    }

    // ── reverse_index (coordinator) ────────────────────────────────────

    pub fn reverse_index(&mut self) {
        tracing::debug!(
            cursor_y = self.cursor.cursor.y,
            scroll_top = self.cursor.scroll_region.top,
            scroll_bottom = self.cursor.scroll_region.bottom,
            full_screen = (self.cursor.scroll_region.top == 0
                && self.cursor.scroll_region.bottom == self.normal.rows() - 1),
            "reverse_index"
        );

        if self.cursor.cursor.y == self.cursor.scroll_region.top
            && self.cursor.cursor.y <= self.cursor.scroll_region.bottom
        {
            self.mark_rows_dirty(
                self.cursor.scroll_region.top,
                self.cursor.scroll_region.bottom.saturating_add(1),
            );
            if self.cursor.margins.enabled {
                let Screen {
                    normal,
                    cursor,
                    edit,
                    ..
                } = self;
                let top = cursor.scroll_region.top;
                let bottom = cursor.scroll_region.bottom;
                let height = bottom - top + 1;
                if 1 < height {
                    for dst_row in ((top + 1)..=bottom).rev() {
                        let src_row = dst_row - 1;
                        for col in cursor.margins.left..=cursor.margins.right {
                            let cell = *normal.cell(src_row, col);
                            *normal.cell_mut(dst_row, col) = cell;
                        }
                    }
                }
                let blank = edit.erase_cell();
                for col in cursor.margins.left..=cursor.margins.right {
                    *normal.cell_mut(top, col) = blank;
                }
            } else {
                let tr = self.normal.total_rows();
                let vis = self.normal.visible_start();
                let c = self.normal.cols();
                let src_start = ((vis + self.cursor.scroll_region.top) % tr) * c;
                let src_end = ((vis + self.cursor.scroll_region.bottom) % tr) * c;
                let dst = ((vis + self.cursor.scroll_region.top + 1) % tr) * c;
                self.normal.copy_ring_range(src_start, src_end, dst);
                self.normal
                    .fill_row_with(self.cursor.scroll_region.top, self.edit.erase_cell());
            }
        } else if self.cursor.cursor.y > 0 {
            self.cursor.cursor.y -= 1;
        }
    }

    // ── scroll_region_up_one (coordinator) ─────────────────────────────

    fn scroll_region_up_one(&mut self) {
        tracing::debug!(
            scroll_top = self.cursor.scroll_region.top,
            scroll_bottom = self.cursor.scroll_region.bottom,
            visible_rows = self.normal.rows(),
            full_screen = (self.cursor.scroll_region.top == 0
                && self.cursor.scroll_region.bottom == self.normal.rows() - 1),
            "scroll_region_up_one"
        );

        self.mark_rows_dirty(
            self.cursor.scroll_region.top,
            self.cursor.scroll_region.bottom.saturating_add(1),
        );
        if self.cursor.margins.enabled {
            let Screen {
                normal,
                cursor,
                edit,
                ..
            } = self;
            let top = cursor.scroll_region.top;
            let bottom = cursor.scroll_region.bottom;
            let height = bottom - top + 1;
            if 1 < height {
                for dst_row in top..=(bottom - 1) {
                    let src_row = dst_row + 1;
                    for col in cursor.margins.left..=cursor.margins.right {
                        let cell = *normal.cell(src_row, col);
                        *normal.cell_mut(dst_row, col) = cell;
                    }
                }
            }
            let blank = edit.erase_cell();
            for col in cursor.margins.left..=cursor.margins.right {
                *normal.cell_mut(bottom, col) = blank;
            }
        } else if self.cursor.scroll_region.top == 0
            && self.cursor.scroll_region.bottom == self.normal.rows() - 1
        {
            self.normal.scroll_up_full_screen(1, self.edit.erase_cell());
        } else {
            let tr = self.normal.total_rows();
            let vis = self.normal.visible_start();
            let c = self.normal.cols();
            let src_start = ((vis + self.cursor.scroll_region.top + 1) % tr) * c;
            let src_end = ((vis + self.cursor.scroll_region.bottom + 1) % tr) * c;
            let dst = ((vis + self.cursor.scroll_region.top) % tr) * c;
            self.normal.copy_ring_range(src_start, src_end, dst);
            self.normal
                .fill_row_with(self.cursor.scroll_region.bottom, self.edit.erase_cell());
        }
        self.cursor.cursor.y = self.cursor.scroll_region.bottom;
    }

    // ── reset ──────────────────────────────────────────────────────────

    pub fn reset_display(&mut self) {
        self.alt.mark_inactive();
        self.alt_saved = None;

        self.normal.fill_all();
        self.cursor.cursor.x = 0;
        self.cursor.cursor.y = 0;
        self.cursor.cursor.visible = true;
        self.edit.pen = self::edit::Pen::reset();
        self.cursor.scroll_region = self::cursor::ScrollRegion::full(self.normal.rows());
        self.cursor.margins = self::cursor::Margins::full(self.normal.cols());
        self.cursor.modes = self::cursor::TerminalModes::default();
        self.edit.charsets.reset();
        self.edit.tab_stops = TabStops::new(self.normal.cols());
        self.cursor.cursor.saved = None;
        self.mark_all_dirty();
    }

    pub fn soft_reset(&mut self) {
        self.edit.pen = self::edit::Pen::reset();
        self.cursor.scroll_region = self::cursor::ScrollRegion::full(self.normal.rows());
        self.cursor.margins = self::cursor::Margins::full(self.normal.cols());
        self.cursor.modes = self::cursor::TerminalModes::default();

        self.cursor.cursor.saved = None;
        self.edit.charsets.last_char = None;
        self.cursor.cursor.x = 0;
        self.cursor.cursor.y = 0;
        self.cursor.cursor.visible = true;
    }

    // ── misc ───────────────────────────────────────────────────────────

    pub fn row_text(&self, row: usize) -> String {
        self.normal.row_text(row)
    }
}
