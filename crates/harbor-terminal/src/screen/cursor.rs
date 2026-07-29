//! Cursor engine: position, scrolling region, margins, and terminal modes.
//!
//! Owns cursor state and mode flags. Methods that need read access to the
//! visible grid take `&NormalBuf`; methods that also modify the grid or call
//! into the edit engine live on `Screen` (the coordinator).

use crate::InputModes;
use crate::normal_buf::NormalBuf;
use harbor_types::{CellAttrs, Color, CursorShape};

use super::edit::{Pen, Rect};

/// Saved terminal state for cursor save/restore (DECSC/DECRC). Captures the cursor position
/// and SGR attributes so the screen can be restored after a screen-altering operation.
#[derive(Debug, Clone)]
pub(crate) struct SavedCursor {
    pub(crate) cursor_x: usize,
    pub(crate) cursor_y: usize,
    pub(crate) fg: Color,
    pub(crate) bg: Color,
    pub(crate) attrs: CellAttrs,
    pub(crate) origin_mode: bool,
    pub(crate) autowrap: bool,
    pub(crate) pending_wrap: bool,
}

/// Cursor position, appearance, and saved-state (DECSC/DECRC).
///
/// All coordinates are 0-based. The saved cursor snapshot persists until
/// overwritten by a subsequent DECSC, a reset, or alt-screen entry.
#[derive(Debug, Clone)]
pub(crate) struct CursorState {
    /// 0-based column.
    pub(crate) x: usize,
    /// 0-based row.
    pub(crate) y: usize,
    /// Current cursor shape (DECSCUSR).
    pub(crate) shape: CursorShape,
    /// Whether the cursor blinks (DECSCUSR).
    pub(crate) blink: bool,
    /// Whether the cursor is visible (DECTCEM).
    pub(crate) visible: bool,
    /// Saved cursor snapshot from DECSC, or `None` before any save.
    pub(crate) saved: Option<SavedCursor>,
}

impl CursorState {
    pub(crate) fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            shape: CursorShape::default(),
            blink: true,
            visible: true,
            saved: None,
        }
    }
}

/// Vertical scrolling region (DECSTBM).  Both boundaries are 0-based and
/// inclusive: `top=0, bottom=rows-1` covers the full screen.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScrollRegion {
    /// Top boundary of the scrolling region, inclusive.
    pub(crate) top: usize,
    /// Bottom boundary of the scrolling region, inclusive.
    pub(crate) bottom: usize,
}

impl ScrollRegion {
    pub(crate) fn full(rows: usize) -> Self {
        Self {
            top: 0,
            bottom: rows.saturating_sub(1),
        }
    }
}

/// Horizontal left/right margins (DECLRMM, private mode 69).
/// Both boundaries are 0-based and inclusive.  Only active when `enabled`
/// is true.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Margins {
    /// Whether DECLRMM is active.
    pub(crate) enabled: bool,
    /// Left margin column, inclusive.
    pub(crate) left: usize,
    /// Right margin column, inclusive.
    pub(crate) right: usize,
}

impl Margins {
    pub(crate) fn full(cols: usize) -> Self {
        Self {
            enabled: false,
            left: 0,
            right: cols.saturating_sub(1),
        }
    }

    /// Clamps both margin boundaries into `[0, cols-1]` without reordering.
    pub(crate) fn clamp(&mut self, cols: usize) {
        let rightmost = cols.saturating_sub(1);
        self.left = self.left.min(rightmost);
        self.right = self.right.min(rightmost);
    }
}

/// Set of binary terminal modes — each maps to a DEC private or standard
/// mode whose state is stored directly in the screen.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerminalModes {
    /// DECAWM: autowrap at the right margin.
    pub(crate) autowrap: bool,
    /// Internal flag: true when the cursor reached the right margin and the
    /// next printable character should wrap before printing.
    pub(crate) pending_wrap: bool,
    /// DECOM: cursor positioning is relative to the scrolling region.
    pub(crate) origin: bool,
    /// IRM (standard mode 4): insert characters instead of overwriting.
    pub(crate) insert: bool,
    /// LNM (standard mode 20): line feed also performs a carriage return.
    pub(crate) line_feed: bool,
    /// DECCKM: application cursor keys send SS3-style sequences.
    pub(crate) application_cursor: bool,
    /// DECKPAM: application keypad sends SS3-style sequences.
    pub(crate) application_keypad: bool,
    /// Bracketed paste mode (DECSET ?2004).
    pub(crate) bracketed_paste: bool,
}

impl TerminalModes {
    pub(crate) fn default() -> Self {
        Self {
            autowrap: true,
            pending_wrap: false,
            origin: false,
            insert: false,
            line_feed: false,
            application_cursor: false,
            application_keypad: false,
            bracketed_paste: false,
        }
    }
}

/// Owns cursor position, scroll region, margins, and terminal modes.
#[derive(Debug)]
pub(crate) struct CursorEngine {
    pub(crate) cursor: CursorState,
    pub(crate) scroll_region: ScrollRegion,
    pub(crate) margins: Margins,
    pub(crate) modes: TerminalModes,
}

impl CursorEngine {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        Self {
            cursor: CursorState::new(),
            scroll_region: ScrollRegion::full(rows),
            margins: Margins::full(cols),
            modes: TerminalModes::default(),
        }
    }

    // ── resize / clamp ────────────────────────────────────────────

    /// Clamps cursor position, margins, and scroll region into the new grid
    /// dimensions. Also clamps any saved cursor snapshot (DECSC).
    pub(crate) fn clamp_to_grid(&mut self, rows: usize, cols: usize) {
        self.cursor.y = self.cursor.y.min(rows.saturating_sub(1));
        self.cursor.x = self.cursor.x.min(cols.saturating_sub(1));
        self.margins.clamp(cols);
        self.scroll_region = ScrollRegion::full(rows);
        if let Some(ref mut saved) = self.cursor.saved {
            saved.cursor_x = saved.cursor_x.min(cols.saturating_sub(1));
            saved.cursor_y = saved.cursor_y.min(rows.saturating_sub(1));
        }
    }

    // ── read-only cursor queries ──────────────────────────────────

    pub(crate) fn cursor_x(&self) -> usize {
        self.cursor.x
    }

    pub(crate) fn cursor_y(&self, normal: &NormalBuf) -> usize {
        if normal.view_offset() > 0 {
            normal.rows()
        } else {
            self.cursor.y
        }
    }

    pub(crate) fn cursor_shape(&self) -> CursorShape {
        self.cursor.shape
    }

    pub(crate) fn cursor_blink(&self) -> bool {
        self.cursor.blink
    }

    pub(crate) fn cursor_visible(&self) -> bool {
        self.cursor.visible
    }

    pub(crate) fn set_cursor_style(&mut self, arg: harbor_types::CursorStyleArg) {
        let (shape, blink) = match arg {
            harbor_types::CursorStyleArg::BlinkingBlock => (CursorShape::Block, true),
            harbor_types::CursorStyleArg::SteadyBlock => (CursorShape::Block, false),
            harbor_types::CursorStyleArg::BlinkingUnderline => (CursorShape::Underline, true),
            harbor_types::CursorStyleArg::SteadyUnderline => (CursorShape::Underline, false),
            harbor_types::CursorStyleArg::BlinkingBar => (CursorShape::Bar, true),
            harbor_types::CursorStyleArg::SteadyBar => (CursorShape::Bar, false),
        };
        self.cursor.shape = shape;
        self.cursor.blink = blink;
    }

    pub(crate) fn input_modes(&self) -> InputModes {
        InputModes {
            application_cursor: self.modes.application_cursor,
            application_keypad: self.modes.application_keypad,
            bracketed_paste: self.modes.bracketed_paste,
        }
    }

    pub(crate) fn margin_mode(&self) -> bool {
        self.margins.enabled
    }

    // ── cursor movement ───────────────────────────────────────────

    pub(crate) fn cursor_up(&mut self, n: usize) {
        self.modes.pending_wrap = false;
        let limit = if self.modes.origin
            || (self.cursor.y >= self.scroll_region.top
                && self.cursor.y <= self.scroll_region.bottom)
        {
            self.scroll_region.top
        } else {
            0
        };
        self.cursor.y = self.cursor.y.saturating_sub(n).max(limit);
    }

    pub(crate) fn cursor_down(&mut self, normal: &NormalBuf, n: usize) {
        self.modes.pending_wrap = false;
        let limit = if self.modes.origin
            || (self.cursor.y >= self.scroll_region.top
                && self.cursor.y <= self.scroll_region.bottom)
        {
            self.scroll_region.bottom
        } else {
            normal.rows() - 1
        };
        self.cursor.y = self.cursor.y.saturating_add(n).min(limit);
    }

    pub(crate) fn cursor_left(&mut self, n: usize) {
        self.modes.pending_wrap = false;
        let limit = if self.margins.enabled
            && self.cursor.x >= self.margins.left
            && self.cursor.x <= self.margins.right
        {
            self.margins.left
        } else {
            0
        };
        self.cursor.x = self.cursor.x.saturating_sub(n).max(limit);
    }

    pub(crate) fn cursor_right(&mut self, normal: &NormalBuf, n: usize) {
        self.modes.pending_wrap = false;
        let limit = if self.margins.enabled
            && self.cursor.x >= self.margins.left
            && self.cursor.x <= self.margins.right
        {
            self.margins.right
        } else {
            normal.cols() - 1
        };
        self.cursor.x = self.cursor.x.saturating_add(n).min(limit);
    }

    /// Positions the cursor from 1-based ANSI coordinates, clamped to the visible grid.
    pub(crate) fn set_cursor_position(
        &mut self,
        normal: &NormalBuf,
        row_1_based: usize,
        col_1_based: usize,
    ) {
        self.modes.pending_wrap = false;
        if self.modes.origin {
            let relative_row = row_1_based.saturating_sub(1);
            let absolute_row = self.scroll_region.top.saturating_add(relative_row);
            self.cursor.y = absolute_row.clamp(self.scroll_region.top, self.scroll_region.bottom);

            let relative_col = col_1_based.saturating_sub(1);
            let absolute_col = self.margins.left.saturating_add(relative_col);
            self.cursor.x = absolute_col.clamp(self.margins.left, self.margins.right);
        } else {
            let row = row_1_based.saturating_sub(1).min(normal.rows() - 1);
            let col = col_1_based.saturating_sub(1).min(normal.cols() - 1);
            self.cursor.y = row;
            self.cursor.x = col;
        }
    }

    pub(crate) fn set_cursor_col(&mut self, normal: &NormalBuf, col_1_based: usize) {
        self.modes.pending_wrap = false;
        if self.modes.origin {
            let relative_col = col_1_based.saturating_sub(1);
            let absolute_col = self.margins.left.saturating_add(relative_col);
            self.cursor.x = absolute_col.clamp(self.margins.left, self.margins.right);
        } else {
            self.cursor.x = col_1_based.saturating_sub(1).min(normal.cols() - 1);
        }
    }

    pub(crate) fn set_cursor_row(&mut self, normal: &NormalBuf, row_1_based: usize) {
        self.modes.pending_wrap = false;
        if self.modes.origin {
            let relative_row = row_1_based.saturating_sub(1);
            let absolute_row = self.scroll_region.top.saturating_add(relative_row);
            self.cursor.y = absolute_row.clamp(self.scroll_region.top, self.scroll_region.bottom);
        } else {
            self.cursor.y = row_1_based.saturating_sub(1).min(normal.rows() - 1);
        }
    }

    pub(crate) fn set_cursor(
        &mut self,
        normal: &NormalBuf,
        row_1_based: usize,
        col_1_based: usize,
    ) {
        self.set_cursor_position(normal, row_1_based, col_1_based);
    }

    pub(crate) fn home_cursor(&mut self) {
        if self.modes.origin {
            self.cursor.y = self.scroll_region.top;
            self.cursor.x = self.margins.left;
        } else {
            self.cursor.y = 0;
            self.cursor.x = 0;
        }
        self.modes.pending_wrap = false;
    }

    /// Resets `cursor_x`, implementing the carriage-return (`\r`) semantics.
    pub(crate) fn carriage_return(&mut self) {
        self.modes.pending_wrap = false;
        self.cursor.x = if self.margins.enabled {
            self.margins.left
        } else {
            0
        };
    }

    /// VT non-destructive backspace: move cursor left, skipping wide-continuation cells.
    pub(crate) fn backspace(&mut self, normal: &NormalBuf) {
        self.modes.pending_wrap = false;
        if self.cursor.x == 0 {
            return;
        }
        self.cursor.x -= 1;

        if normal.cell(self.cursor.y, self.cursor.x).wide_continuation && self.cursor.x > 0 {
            self.cursor.x -= 1;
        }
    }

    // ── scroll region / margins ───────────────────────────────────

    pub(crate) fn set_scroll_region(&mut self, normal: &NormalBuf, top: usize, bottom: usize) {
        let top = if top == 0 { 1 } else { top };
        let bottom = if bottom == 0 { normal.rows() } else { bottom };
        let top = top.max(1).min(normal.rows());
        let bottom = bottom.min(normal.rows());
        if top >= bottom {
            return;
        }
        self.scroll_region.top = top - 1;
        self.scroll_region.bottom = bottom - 1;
        self.home_cursor();
    }

    pub(crate) fn set_left_right_margins(&mut self, normal: &NormalBuf, left: usize, right: usize) {
        let left = if left == 0 { 1 } else { left };
        let right = if right == 0 { normal.cols() } else { right };
        let left = left.max(1).min(normal.cols());
        let right = right.min(normal.cols());
        if left < right {
            self.margins.left = left - 1;
            self.margins.right = right - 1;
        }
        self.home_cursor();
    }

    // ── private / standard modes ──────────────────────────────────

    /// Sets a DEC private mode. Returns `true` if the mode was handled,
    /// `false` if it should be handled by the caller (e.g. alt-screen).
    pub(crate) fn set_private_mode(
        &mut self,
        normal: &NormalBuf,
        param: usize,
        enabled: bool,
    ) -> bool {
        match param {
            1 => self.modes.application_cursor = enabled,
            66 => self.modes.application_keypad = enabled,
            2004 => self.modes.bracketed_paste = enabled,
            6 => {
                self.modes.origin = enabled;
                self.home_cursor();
            }
            7 => self.modes.autowrap = enabled,
            25 => self.cursor.visible = enabled,
            69 => {
                self.margins.enabled = enabled;
                if !enabled {
                    self.margins.left = 0;
                    self.margins.right = normal.cols().saturating_sub(1);
                }
                self.home_cursor();
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn set_standard_mode(&mut self, param: usize, enabled: bool) -> bool {
        match param {
            4 => self.modes.insert = enabled,
            20 => self.modes.line_feed = enabled,
            _ => return false,
        }
        true
    }

    pub(crate) fn set_application_keypad(&mut self, enabled: bool) {
        self.modes.application_keypad = enabled;
    }

    // ── cursor save / restore ─────────────────────────────────────

    pub(crate) fn save_cursor(&mut self, pen: &Pen) {
        self.cursor.saved = Some(SavedCursor {
            cursor_x: self.cursor.x,
            cursor_y: self.cursor.y,
            fg: pen.fg,
            bg: pen.bg,
            attrs: pen.attrs,
            origin_mode: self.modes.origin,
            autowrap: self.modes.autowrap,
            pending_wrap: self.modes.pending_wrap,
        });
    }

    pub(crate) fn restore_cursor(&mut self, pen: &mut Pen) {
        if let Some(saved) = &self.cursor.saved {
            self.cursor.x = saved.cursor_x;
            self.cursor.y = saved.cursor_y;
            pen.fg = saved.fg;
            pen.bg = saved.bg;
            pen.attrs = saved.attrs;
            self.modes.origin = saved.origin_mode;
            self.modes.autowrap = saved.autowrap;
            self.modes.pending_wrap = saved.pending_wrap;
        }
    }

    // ── helpers for cross-engine methods ──────────────────────────

    /// Returns true when the cursor is at the bottom of the scrolling region
    /// and a line-feed should trigger a scroll-up.
    pub(crate) fn index_needs_scroll(&self) -> bool {
        self.cursor.y == self.scroll_region.bottom
            && self.cursor.y >= self.scroll_region.top
            && self.cursor.y <= self.scroll_region.bottom
    }

    /// Advances the cursor down one row for VT Index, without scrolling.
    /// Only call when `index_needs_scroll` returned `false`.
    pub(crate) fn index_advance(&mut self, normal: &NormalBuf) {
        if self.cursor.y >= self.scroll_region.top && self.cursor.y <= self.scroll_region.bottom {
            if self.cursor.y < self.scroll_region.bottom {
                self.cursor.y += 1;
            }
        } else if self.cursor.y + 1 < normal.rows() {
            self.cursor.y += 1;
        }
        self.modes.pending_wrap = false;
    }

    /// Resolves a DEC rectangle (DECERA, DECSERA, DECFRA, DECCRA, DECCARA, DECRARA).
    /// All four coordinates are 1-based; 0 means "use the boundary value".
    pub(crate) fn resolve_rect(
        &self,
        normal: &NormalBuf,
        top: usize,
        left: usize,
        bottom: usize,
        right: usize,
    ) -> Option<Rect> {
        let (top_bound, bottom_bound, left_bound, right_bound) = if self.modes.origin {
            (
                self.scroll_region.top,
                self.scroll_region.bottom,
                self.margins.left,
                self.margins.right,
            )
        } else {
            (0, normal.rows() - 1, 0, normal.cols() - 1)
        };

        let row_origin = if self.modes.origin { top_bound } else { 0 };
        let col_origin = if self.modes.origin { left_bound } else { 0 };
        let default_bottom = bottom_bound - row_origin + 1;
        let default_right = right_bound - col_origin + 1;

        let top = if top == 0 { 1 } else { top };
        let left = if left == 0 { 1 } else { left };
        let bottom = if bottom == 0 { default_bottom } else { bottom };
        let right = if right == 0 { default_right } else { right };

        let top = row_origin.saturating_add(top - 1);
        let left = col_origin.saturating_add(left - 1);
        let bottom = row_origin.saturating_add(bottom - 1);
        let right = col_origin.saturating_add(right - 1);

        if top > bottom || left > right {
            return None;
        }

        Some(Rect {
            top: top.clamp(top_bound, bottom_bound),
            left: left.clamp(left_bound, right_bound),
            bottom: bottom.clamp(top_bound, bottom_bound),
            right: right.clamp(left_bound, right_bound),
        })
    }
}
