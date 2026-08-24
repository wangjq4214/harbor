//! Terminal screen: thin coordinator delegating to internal engines.
//!
//! - [`cursor::CursorEngine`] — cursor position, scroll region, margins, modes
//! - [`edit::PenState`]       — pen (SGR), tab stops, charsets, erase-cell helper
//! - [`edit::CellOps`]        — cell-level mutations (erase, insert, delete, scroll, DEC rects)
//! - [`edit::CellWriter`]     — character writing (write_char and helpers)
//! - [`alt::AltScreenStack`]  — alt-screen flag and pending request
//! - [`synchronized_output::SynchronizedOutput`] — saturating `?2026` nesting
//!
//! `Screen` keeps the public API stable; most methods are one-line
//! delegations.  Cross-engine operations (e.g. `reverse_index`,
//! `scroll_region_up_one`) stay here.

mod alt;
mod cursor;
mod edit;
mod reader;
mod synchronized_output;
#[cfg(test)]
mod tests;

use crate::normal_buf::CellsIter;
use crate::{DirtyRange, InputModes, NormalBuf};
use harbor_parser::Params;

use self::alt::AltScreenStack;
use self::cursor::CursorEngine;
use self::edit::{CellOps, CellWriter, PenState};
use self::synchronized_output::SynchronizedOutput;

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

/// State reported by DECRPM for a queried terminal mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// The permanent DECRPM statuses are reserved for future fixed-mode support.
#[allow(dead_code)]
pub(crate) enum ModeStatus {
    Unknown,
    Set,
    Reset,
    PermanentlySet,
    PermanentlyReset,
}

impl ModeStatus {
    pub(crate) const fn code(self) -> usize {
        match self {
            Self::Unknown => 0,
            Self::Set => 1,
            Self::Reset => 2,
            Self::PermanentlySet => 3,
            Self::PermanentlyReset => 4,
        }
    }
}

impl From<bool> for ModeStatus {
    fn from(enabled: bool) -> Self {
        if enabled { Self::Set } else { Self::Reset }
    }
}

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
    /// Pen state, tab stops, character-set designations, and saved-pen snapshot.
    pen_state: PenState,
    /// Alt-screen flag and pending request.
    alt: AltScreenStack,
    /// Primary screen saved while the alternate screen is active.
    /// Invariant: `Some` iff in alt screen (`is_alt()` is true).
    saved_primary: Option<Box<Screen>>,
    /// Alternate screen parked across `?47` exit/re-enter.
    /// Invariant: `Some` iff not in alt screen (parked).
    parked_alt: Option<Box<Screen>>,
    /// Outgoing VT replies buffer.
    pub(crate) replies: Vec<u8>,
    /// Session-owned `?2026` nesting; preserved across alt-screen swap.
    synchronized_output: SynchronizedOutput,
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            normal: NormalBuf::new(rows, cols),
            cursor: CursorEngine::new(rows, cols),
            pen_state: PenState::new(cols),
            alt: AltScreenStack::default(),
            saved_primary: None,
            parked_alt: None,
            replies: Vec::new(),
            synchronized_output: SynchronizedOutput::default(),
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

    #[cfg(test)]
    pub(crate) fn pending_wrap(&self) -> bool {
        self.cursor.modes.pending_wrap
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

    /// Current SGR foreground, background, and attributes as observed for DECRQSS.
    pub(crate) fn current_sgr(&self) -> (Color, Color, CellAttrs) {
        (
            self.pen_state.pen.fg,
            self.pen_state.pen.bg,
            self.pen_state.pen.attrs,
        )
    }

    /// 1-based inclusive DECSTBM top/bottom bounds.
    pub(crate) fn scroll_region(&self) -> (usize, usize) {
        (
            self.cursor.scroll_region.top + 1,
            self.cursor.scroll_region.bottom + 1,
        )
    }

    /// 1-based inclusive saved DECSLRM left/right bounds.
    pub(crate) fn left_right_margins(&self) -> (usize, usize) {
        (self.cursor.margins.left + 1, self.cursor.margins.right + 1)
    }

    /// Canonical DECSCUSR style derived from current shape and blink.
    pub(crate) fn cursor_style(&self) -> CursorStyleArg {
        match (self.cursor.cursor.shape, self.cursor.cursor.blink) {
            (CursorShape::Block, true) => CursorStyleArg::BlinkingBlock,
            (CursorShape::Block, false) => CursorStyleArg::SteadyBlock,
            (CursorShape::Underline, true) => CursorStyleArg::BlinkingUnderline,
            (CursorShape::Underline, false) => CursorStyleArg::SteadyUnderline,
            (CursorShape::Bar, true) => CursorStyleArg::BlinkingBar,
            (CursorShape::Bar, false) => CursorStyleArg::SteadyBar,
        }
    }

    /// Current DECSCA protection applied to newly written cells.
    pub(crate) fn character_protection(&self) -> CharacterProtection {
        if self.pen_state.pen.protected {
            CharacterProtection::Protected
        } else {
            CharacterProtection::Unprotected
        }
    }

    pub fn push_reply(&mut self, reply: &[u8]) {
        if self.replies.len() + reply.len() <= 1024 {
            self.replies.extend_from_slice(reply);
        } else {
            tracing::warn!("Terminal reply buffer limit (1024 bytes) reached. Discarding reply.");
        }
    }

    pub fn drain_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    /// Returns the 1-based (row, col) coordinates relative to origin/margins if DECOM is enabled.
    pub fn cpr_coordinates(&self) -> (usize, usize) {
        let row = if self.cursor.modes.origin {
            self.cursor
                .cursor_y(&self.normal)
                .saturating_sub(self.cursor.scroll_region.top)
                + 1
        } else {
            self.cursor.cursor_y(&self.normal) + 1
        };
        let col = if self.cursor.modes.origin && self.cursor.margins.enabled {
            self.cursor
                .cursor_x()
                .saturating_sub(self.cursor.margins.left)
                + 1
        } else {
            self.cursor.cursor_x() + 1
        };
        (row, col)
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

    pub fn is_wrapped_at_generation(&self, generation: u64) -> bool {
        self.normal
            .is_wrapped_at_generation(generation)
            .unwrap_or(false)
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

    pub fn request_alt_enter(&mut self, clear: bool) {
        self.alt.request_enter(clear);
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

    pub fn enter_alt(&mut self, clear: bool) {
        if self.alt.is_alt() {
            return;
        }
        let rows = self.rows();
        let cols = self.cols();
        let replies = std::mem::take(&mut self.replies);
        let sync = self.synchronized_output;
        // Save the primary screen (cells + scrollback + cursor + pen + modes),
        // carrying its parked alternate buffer along to the fresh screen.
        let mut primary = std::mem::replace(self, Self::new(rows, cols));
        let parked = primary.parked_alt.take();
        // Install the alternate screen: clear it, or restore the persistent one.
        if clear {
            // Parked contents (if any) are dropped — `?1047`/`?1049` clear on entry.
        } else if let Some(alt) = parked {
            *self = *alt;
            self.mark_all_dirty();
        }
        // Save the primary after the install block: a restored parked buffer
        // carries `saved_primary = None`, so assigning first would be clobbered.
        self.saved_primary = Some(Box::new(primary));
        debug_assert!(self.saved_primary.is_some(), "in alt => primary saved");
        self.replies = replies;
        self.synchronized_output = sync;
        self.alt.mark_active();
    }

    pub fn exit_alt(&mut self) {
        let replies = std::mem::take(&mut self.replies);
        let sync = self.synchronized_output;
        if let Some(primary) = self.saved_primary.take() {
            // Preserve the alternate-screen contents for a later `?47` re-entry.
            let rows = self.rows();
            let cols = self.cols();
            let alt = std::mem::replace(self, Self::new(rows, cols));

            *self = *primary;
            // Park the alternate buffer after the primary is back, since the
            // saved primary carries an empty `parked_alt` slot.
            self.parked_alt = Some(Box::new(alt));
            self.mark_all_dirty();
        }
        self.replies = replies;
        self.synchronized_output = sync;
        self.alt.mark_inactive();
        debug_assert!(
            self.saved_primary.is_none(),
            "not in alt => no primary saved"
        );
    }

    // ── resize ─────────────────────────────────────────────────────────

    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        self.normal.resize(rows, cols);
        self.cursor.clamp_to_grid(rows, cols);
        self.pen_state.tab_stops.resize(cols);
        if let Some(saved) = &mut self.saved_primary {
            saved.resize(rows, cols);
        }
        if let Some(alt) = &mut self.parked_alt {
            alt.resize(rows, cols);
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
            .set_cursor_position(&self.normal, row_1_based, col_1_based);
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
            47 => {
                if enabled {
                    self.request_alt_enter(false);
                } else {
                    self.request_alt_exit();
                }
            }
            1047 => {
                if enabled {
                    self.request_alt_enter(true);
                } else {
                    self.request_alt_exit();
                }
            }
            1048 => {
                if enabled {
                    self.save_cursor();
                } else {
                    self.restore_cursor();
                }
            }
            1049 => {
                if enabled {
                    self.request_alt_enter(true);
                } else {
                    self.request_alt_exit();
                }
            }
            SynchronizedOutput::MODE => {
                if enabled {
                    self.synchronized_output.enable();
                } else {
                    self.synchronized_output.disable();
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

    pub(crate) fn mode_status(&self, private: bool, param: usize) -> ModeStatus {
        let enabled = if private {
            match param {
                47 | 1047 | 1049 => Some(self.is_alt()),
                1048 => Some(self.cursor.cursor.saved.is_some()),
                SynchronizedOutput::MODE => return self.synchronized_output.mode_status(),
                _ => self.cursor.private_mode_enabled(param),
            }
        } else {
            self.cursor.standard_mode_enabled(param)
        };
        enabled.map(ModeStatus::from).unwrap_or(ModeStatus::Unknown)
    }

    pub fn set_application_keypad(&mut self, enabled: bool) {
        self.cursor.set_application_keypad(enabled);
    }

    pub(crate) fn ordinary_present_eligible(&self) -> bool {
        self.synchronized_output.ordinary_present_eligible()
    }

    pub(crate) fn clear_synchronized_output(&mut self) {
        self.synchronized_output.clear();
    }

    // ── SGR / charsets / protection ────────────────────────────────────

    pub fn set_sgr(&mut self, params: &Params) {
        self.pen_state.set_sgr(params);
    }

    pub fn set_sgr_slice(&mut self, slice: &[Option<usize>]) {
        self.pen_state.set_sgr_slice(slice);
    }

    pub fn designate_g0(&mut self, charset: u8) {
        self.pen_state.designate_g0(charset);
    }

    pub fn designate_g1(&mut self, charset: u8) {
        self.pen_state.designate_g1(charset);
    }

    pub fn set_active_charset(&mut self, active: u8) {
        self.pen_state.set_active_charset(active);
    }

    pub fn set_character_protection(&mut self, arg: CharacterProtection) {
        self.pen_state.set_character_protection(arg);
    }

    // ── erase ──────────────────────────────────────────────────────────

    pub fn erase_display(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::erase_display(pen_state, normal, cursor, mode);
    }

    pub fn erase_line(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::erase_line(pen_state, normal, cursor, mode);
    }

    pub fn erase_chars(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::erase_chars(pen_state, normal, cursor, n);
    }

    pub fn selective_erase_display(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::selective_erase_display(pen_state, normal, cursor, mode);
    }

    pub fn selective_erase_line(&mut self, mode: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::selective_erase_line(pen_state, normal, cursor, mode);
    }

    // ── insert / delete ────────────────────────────────────────────────

    pub fn insert_chars(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::insert_chars(pen_state, normal, cursor, n);
    }

    pub fn delete_chars(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::delete_chars(pen_state, normal, cursor, n);
    }

    pub fn insert_lines(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::insert_lines(pen_state, normal, cursor, n);
    }

    pub fn delete_lines(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::delete_lines(pen_state, normal, cursor, n);
    }

    // ── scroll region (CSI S / CSI T) ──────────────────────────────────

    pub fn scroll_up_region(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::scroll_up_region(pen_state, normal, cursor, n);
    }

    pub fn scroll_down_region(&mut self, n: usize) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::scroll_down_region(pen_state, normal, cursor, n);
    }

    // ── DEC rectangle ops ──────────────────────────────────────────────

    pub fn decera(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::decera(pen_state, normal, cursor, params);
    }

    pub fn decsera(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::decsera(pen_state, normal, cursor, params);
    }

    pub fn decfra(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::decfra(pen_state, normal, cursor, params);
    }

    pub fn deccra(&mut self, params: &Params) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellOps::deccra(pen_state, normal, cursor, params);
    }

    pub fn deccara(&mut self, params: &Params) {
        let Screen { normal, cursor, .. } = self;
        CellOps::deccara(normal, cursor, params);
    }

    pub fn decrara(&mut self, params: &Params) {
        let Screen { normal, cursor, .. } = self;
        CellOps::decrara(normal, cursor, params);
    }

    // ── tab stops ──────────────────────────────────────────────────────

    pub fn set_tab_stop(&mut self) {
        self.pen_state.set_tab_stop(self.cursor.cursor.x);
    }

    pub fn clear_tab_stops(&mut self, mode: usize) {
        self.pen_state.clear_tab_stops(self.cursor.cursor.x, mode);
    }

    // ── cursor save / restore ──────────────────────────────────────────

    pub fn save_cursor(&mut self) {
        self.cursor.save_cursor_position();
        self.pen_state.save_pen();
    }

    pub fn restore_cursor(&mut self) {
        self.cursor.restore_cursor_position();
        self.pen_state.restore_pen();
    }

    // ── write_char (coordinator) ───────────────────────────────────────

    pub fn write_char(&mut self, ch: char) {
        let Screen {
            normal,
            cursor,
            pen_state,
            ..
        } = self;
        CellWriter::write_char(pen_state, normal, cursor, ch);
    }

    // ── horizontal_tab (coordinator) ───────────────────────────────────

    pub fn horizontal_tab(&mut self) {
        self.cursor.clear_pending_wrap();
        let right_limit = if self.cursor.margins.enabled {
            self.cursor.margins.right
        } else {
            self.normal.cols()
        };
        let mut target = right_limit;
        for col in (self.cursor.cursor.x + 1)..=right_limit {
            if col < self.pen_state.tab_stops.0.len() && self.pen_state.tab_stops.0[col] {
                target = col;
                break;
            }
        }
        if target > self.cursor.cursor.x {
            let spaces = target - self.cursor.cursor.x;
            let Screen {
                normal,
                cursor,
                pen_state,
                ..
            } = self;
            for _ in 0..spaces {
                CellWriter::write_char(pen_state, normal, cursor, ' ');
            }
        }
        self.cursor.clear_pending_wrap();
    }

    /// Moves forward over `steps` tab stops without changing any cells.
    pub fn forward_tab(&mut self, steps: usize) {
        self.cursor.clear_pending_wrap();
        let steps = steps.max(1);
        let (left_limit, right_limit) = if self.cursor.margins.enabled {
            (self.cursor.margins.left, self.cursor.margins.right)
        } else {
            (0, self.normal.cols().saturating_sub(1))
        };
        let start = self.cursor.cursor.x.saturating_add(1).max(left_limit);

        if start <= right_limit {
            let mut remaining = steps;
            for col in start..=right_limit {
                if self.pen_state.tab_stops.0[col] {
                    remaining -= 1;
                    if remaining == 0 {
                        self.cursor.cursor.x = col;
                        return;
                    }
                }
            }
        }
        self.cursor.cursor.x = right_limit;
    }

    /// Moves backward over `steps` tab stops without changing any cells.
    pub fn backward_tab(&mut self, steps: usize) {
        self.cursor.clear_pending_wrap();
        let steps = steps.max(1);
        let (left_limit, right_limit) = if self.cursor.margins.enabled {
            (self.cursor.margins.left, self.cursor.margins.right)
        } else {
            (0, self.normal.cols().saturating_sub(1))
        };
        let end = self.cursor.cursor.x.saturating_sub(1).min(right_limit);

        if end >= left_limit {
            let mut remaining = steps;
            for col in (left_limit..=end).rev() {
                if self.pen_state.tab_stops.0[col] {
                    remaining -= 1;
                    if remaining == 0 {
                        self.cursor.cursor.x = col;
                        return;
                    }
                }
            }
        }
        self.cursor.cursor.x = left_limit;
    }

    // ── repeat_char (coordinator) ──────────────────────────────────────

    pub fn repeat_char(&mut self, n: usize) {
        if let Some(ch) = self.pen_state.charsets.last_char {
            let n = if n == 0 { 1 } else { n };
            let count = n.min(self.normal.cols());
            let Screen {
                normal,
                cursor,
                pen_state,
                ..
            } = self;
            for _ in 0..count {
                CellWriter::write_char(pen_state, normal, cursor, ch);
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
        self.cursor.clear_pending_wrap();
        let before = self.cursor.cursor.y;
        let scrolled = self.cursor.index_needs_scroll();
        if scrolled {
            self.scroll_region_up_one();
        } else {
            self.cursor.index_advance(&self.normal);
        }
        // Only mark the destination row as a new logical line when the cursor
        // actually moved or a scroll occurred; a no-op index (cursor pinned at
        // the bottom below the scroll region) must not clear an existing flag.
        if scrolled || self.cursor.cursor.y != before {
            self.normal.set_wrapped(self.cursor.cursor.y, false);
        }
    }

    // ── reverse_index (coordinator) ────────────────────────────────────

    pub fn reverse_index(&mut self) {
        self.cursor.clear_pending_wrap();
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
                    pen_state,
                    ..
                } = self;
                CellOps::scroll_margin_rect_down(
                    pen_state,
                    normal,
                    cursor,
                    cursor.scroll_region.top,
                    cursor.scroll_region.bottom,
                    1,
                );
            } else {
                let tr = self.normal.total_rows();
                let vis = self.normal.visible_start();
                let c = self.normal.cols();
                let src_start = ((vis + self.cursor.scroll_region.top) % tr) * c;
                let src_end = ((vis + self.cursor.scroll_region.bottom) % tr) * c;
                let dst = ((vis + self.cursor.scroll_region.top + 1) % tr) * c;
                self.normal.copy_ring_range(src_start, src_end, dst);
                self.normal
                    .copy_wrapped_ring_range(src_start / c, src_end / c, dst / c);
                self.normal
                    .fill_row_with(self.cursor.scroll_region.top, self.pen_state.erase_cell());
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
                pen_state,
                ..
            } = self;
            CellOps::scroll_margin_rect_up(
                pen_state,
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                1,
            );
        } else if self.cursor.scroll_region.top == 0
            && self.cursor.scroll_region.bottom == self.normal.rows() - 1
        {
            self.normal
                .scroll_up_full_screen(1, self.pen_state.erase_cell());
        } else {
            let tr = self.normal.total_rows();
            let vis = self.normal.visible_start();
            let c = self.normal.cols();
            let src_start = ((vis + self.cursor.scroll_region.top + 1) % tr) * c;
            let src_end = ((vis + self.cursor.scroll_region.bottom + 1) % tr) * c;
            let dst = ((vis + self.cursor.scroll_region.top) % tr) * c;
            self.normal.copy_ring_range(src_start, src_end, dst);
            self.normal
                .copy_wrapped_ring_range(src_start / c, src_end / c, dst / c);
            self.normal.fill_row_with(
                self.cursor.scroll_region.bottom,
                self.pen_state.erase_cell(),
            );
        }
        self.cursor.cursor.y = self.cursor.scroll_region.bottom;
    }

    // ── screen alignment / reset ───────────────────────────────────────

    /// Performs DECALN (`ESC # 8`) on the active visible buffer.
    pub fn decaln(&mut self) {
        let cell = Cell {
            ch: 'E',
            ..Cell::default()
        };
        self.normal.fill_all_with(cell);
        self.cursor.alignment_home();
        self.mark_all_dirty();
    }

    pub fn reset_display(&mut self) {
        self.synchronized_output.clear();
        self.alt.mark_inactive();
        self.saved_primary = None;
        self.parked_alt = None;

        let rows = self.normal.rows();
        let cols = self.normal.cols();
        self.normal.fill_all();
        self.cursor.reset(rows, cols);
        self.pen_state.reset(cols);
        self.mark_all_dirty();
    }

    pub fn soft_reset(&mut self) {
        let rows = self.normal.rows();
        let cols = self.normal.cols();
        self.cursor.reset(rows, cols);
        self.pen_state.soft_reset();
    }

    // ── misc ───────────────────────────────────────────────────────────

    pub fn row_text(&self, row: usize) -> String {
        self.normal.row_text(row)
    }

    /// Returns whether the given display row is a soft-wrapped continuation of
    /// the logical line above. `row` is viewport-relative (0..`rows`) and
    /// view-offset aware — it reports the row currently displayed at that
    /// position, consistent with `cell`/`cells`. Consumers working in
    /// scrollback generation coordinates must map to display rows first.
    pub fn is_wrapped(&self, row: usize) -> bool {
        self.normal.is_wrapped(row)
    }
}
