//! VT edit engine: pen (SGR), tab stops, character sets, and cell-level mutations.
//!
//! Methods that need cursor state receive `&mut CursorEngine` or individual
//! cursor fields.  Methods that modify the grid take `&mut NormalBuf`.

use crate::normal_buf::NormalBuf;
use harbor_parser::Params;
use harbor_types::{Cell, CellAttrs, CharacterProtection, Color};
use unicode_width::UnicodeWidthChar;

use super::cursor::CursorEngine;

/// Current SGR pen state — the active foreground, background, attributes,
/// and protection flag applied to each newly written character.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pen {
    /// Foreground color (SGR 30–39, 90–97, 38).
    pub(crate) fg: Color,
    /// Background color (SGR 40–49, 100–107, 48).
    pub(crate) bg: Color,
    /// Active text attributes (bold, italic, underline, etc.).
    pub(crate) attrs: CellAttrs,
    /// Whether newly written cells are protected (DECSCA).
    pub(crate) protected: bool,
}

impl Pen {
    pub(crate) fn reset() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
            protected: false,
        }
    }
}

/// Horizontal tab stops.  `true` at column `c` means a tab stop is set.
/// Default stops are at every 8th column.
#[derive(Debug, Clone)]
pub(crate) struct TabStops(pub(crate) Vec<bool>);

impl TabStops {
    pub(crate) fn new(cols: usize) -> Self {
        let mut stops = vec![false; cols];
        for (col, stop) in stops.iter_mut().enumerate() {
            if col % 8 == 0 {
                *stop = true;
            }
        }
        Self(stops)
    }

    pub(crate) fn resize(&mut self, cols: usize) {
        let old_len = self.0.len();
        self.0.resize(cols, false);
        for col in old_len..cols {
            if col % 8 == 0 {
                self.0[col] = true;
            }
        }
    }
}

/// Character set state for GL mapping via G0/G1 designation.
///
/// `g0` and `g1` hold the final character of the designation escape
/// (e.g. `b'B'` for US-ASCII, `b'0'` for DEC Special Graphics).
/// `active` selects which set (0 = G0, 1 = G1) maps GL characters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CharacterSets {
    /// Most recently printed character (used by REP / CSI Ps b).
    pub(crate) last_char: Option<char>,
    /// G0 character set designation.
    pub(crate) g0: u8,
    /// G1 character set designation.
    pub(crate) g1: u8,
    /// Active charset: 0 = G0, 1 = G1.
    pub(crate) active: u8,
}

impl CharacterSets {
    pub(crate) fn default() -> Self {
        Self {
            last_char: None,
            g0: b'B',
            g1: b'B',
            active: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.last_char = None;
        self.g0 = b'B';
        self.g1 = b'B';
        self.active = 0;
    }
}

/// A rectangular region in display coordinates (0-based, inclusive).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rect {
    pub(crate) top: usize,
    pub(crate) left: usize,
    pub(crate) bottom: usize,
    pub(crate) right: usize,
}

/// Owns pen state, tab stops, and character-set designations.
#[derive(Debug)]
pub(crate) struct VtEditEngine {
    pub(crate) pen: Pen,
    pub(crate) tab_stops: TabStops,
    pub(crate) charsets: CharacterSets,
}

impl VtEditEngine {
    pub(crate) fn new(cols: usize) -> Self {
        Self {
            pen: Pen::reset(),
            tab_stops: TabStops::new(cols),
            charsets: CharacterSets::default(),
        }
    }

    /// Returns a blank cell tinted with the current SGR attributes, for erase ops.
    pub(crate) fn erase_cell(&self) -> Cell {
        Cell {
            ch: ' ',
            wide_continuation: false,
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
            protected: false,
        }
    }

    /// Returns the complete valid wide-glyph range containing `col`.
    fn wide_range(normal: &NormalBuf, row: usize, col: usize) -> Option<(usize, usize)> {
        let cols = normal.cols();
        let cell = normal.cell(row, col);
        if cell.wide_continuation {
            let base = col.checked_sub(1)?;
            return (UnicodeWidthChar::width(normal.cell(row, base).ch).unwrap_or(0) == 2)
                .then_some((base, col));
        }
        (UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2
            && col + 1 < cols
            && normal.cell(row, col + 1).wide_continuation)
            .then_some((col, col + 1))
    }

    /// Expands an exclusive row range to include complete wide glyphs without
    /// crossing the supplied inclusive bounds, and marks every touched cell dirty.
    fn normalize_touched_range(
        &self,
        normal: &mut NormalBuf,
        row: usize,
        start: usize,
        end: usize,
        left: usize,
        right: usize,
    ) -> (usize, usize) {
        let mut start = start.clamp(left, right + 1);
        let mut end = end.clamp(left, right + 1);
        if start >= end {
            return (start, end);
        }
        for col in start..end {
            if let Some((base, continuation)) = Self::wide_range(normal, row, col)
                && base >= left
                && continuation <= right
            {
                start = start.min(base);
                end = end.max(continuation + 1);
            }
        }
        normal.mark_range_dirty(row, start, end);
        (start, end)
    }

    /// Removes malformed wide-cell fragments in a bounded row region.
    ///
    /// A valid glyph which crosses a boundary is deliberately left alone. We
    /// cannot repair the other half without mutating outside the operation's
    /// bounds, so boundary-crossing glyphs are excluded from the operation.
    fn normalize_row_region(&self, normal: &mut NormalBuf, row: usize, left: usize, right: usize) {
        let mut col = left;
        while col <= right {
            if let Some((_, continuation)) = Self::wide_range(normal, row, col) {
                col = continuation + 1;
                continue;
            }
            let cell = normal.cell(row, col);
            if cell.wide_continuation || UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2 {
                *normal.cell_mut(row, col) = self.erase_cell();
            }
            col += 1;
        }
    }

    fn has_boundary_wide(
        normal: &NormalBuf,
        row: usize,
        start: usize,
        end: usize,
        left: usize,
        right: usize,
    ) -> bool {
        (start..end).any(|col| {
            Self::wide_range(normal, row, col).is_some_and(|(base, continuation)| {
                (base < left || continuation > right) && base < end && continuation >= start
            })
        })
    }

    fn erase_row_range(
        &self,
        normal: &mut NormalBuf,
        row: usize,
        start: usize,
        end: usize,
        left: usize,
        right: usize,
        selective: bool,
    ) {
        let (start, end) = self.normalize_touched_range(normal, row, start, end, left, right);
        let erase = self.erase_cell();
        let mut col = start;
        while col < end {
            if let Some((base, continuation)) = Self::wide_range(normal, row, col) {
                if base < left || continuation > right {
                    // Never erase only the in-bound half of a boundary glyph.
                    col = continuation + 1;
                    continue;
                }
                if !selective
                    || (!normal.cell(row, base).protected
                        && !normal.cell(row, continuation).protected)
                {
                    *normal.cell_mut(row, base) = erase;
                    *normal.cell_mut(row, continuation) = erase;
                }
                col = continuation + 1;
            } else {
                if !selective || !normal.cell(row, col).protected {
                    *normal.cell_mut(row, col) = erase;
                }
                col += 1;
            }
        }
        self.normalize_row_region(normal, row, left, right);
    }

    // ── SGR ───────────────────────────────────────────────────────

    pub(crate) fn set_sgr(&mut self, params: &Params) {
        let mut i = 0usize;
        while i < params.len() {
            let sub_params_len = params
                .sub_params_len(i)
                .expect("index is bounded by params.len()");
            let n = params.get_or(i, 0);
            match n {
                0 => {
                    self.pen.fg = Color::Default;
                    self.pen.bg = Color::Default;
                    self.pen.attrs = CellAttrs::default();
                }
                1 => self.pen.attrs.set(CellAttrs::BOLD),
                2 => self.pen.attrs.set(CellAttrs::DIM),
                3 => self.pen.attrs.set(CellAttrs::ITALIC),
                4 => self.pen.attrs.set(CellAttrs::UNDERLINE),
                5 => self.pen.attrs.set(CellAttrs::BLINK),
                7 => self.pen.attrs.set(CellAttrs::INVERSE),
                9 => self.pen.attrs.set(CellAttrs::STRIKETHROUGH),
                22 => self.pen.attrs.clear(CellAttrs::BOLD | CellAttrs::DIM),
                23 => self.pen.attrs.clear(CellAttrs::ITALIC),
                24 => self.pen.attrs.clear(CellAttrs::UNDERLINE),
                25 => self.pen.attrs.clear(CellAttrs::BLINK),
                27 => self.pen.attrs.clear(CellAttrs::INVERSE),
                29 => self.pen.attrs.clear(CellAttrs::STRIKETHROUGH),
                30..=37 => self.pen.fg = Color::Named((n - 30) as u8),
                40..=47 => self.pen.bg = Color::Named((n - 40) as u8),
                39 => self.pen.fg = Color::Default,
                49 => self.pen.bg = Color::Default,
                90..=97 => self.pen.fg = Color::Bright((n - 90) as u8),
                100..=107 => self.pen.bg = Color::Bright((n - 100) as u8),
                38 | 48 => {
                    let is_fg = n == 38;
                    if sub_params_len > 1 {
                        let sub = params.get_sub_param(i, 1).unwrap_or_default();
                        match sub {
                            5 => {
                                if let Some(val) = params.get_sub_param(i, 2)
                                    && val <= 255
                                {
                                    if is_fg {
                                        self.pen.fg = Color::Indexed(val as u8);
                                    } else {
                                        self.pen.bg = Color::Indexed(val as u8);
                                    }
                                }
                            }
                            2 => {
                                let (r_idx, g_idx, b_idx) = if sub_params_len >= 6 {
                                    (3, 4, 5)
                                } else {
                                    (2, 3, 4)
                                };
                                if let (Some(r), Some(g), Some(b)) = (
                                    params.get_sub_param(i, r_idx),
                                    params.get_sub_param(i, g_idx),
                                    params.get_sub_param(i, b_idx),
                                ) && r <= 255
                                    && g <= 255
                                    && b <= 255
                                {
                                    if is_fg {
                                        self.pen.fg = Color::Rgb(r as u8, g as u8, b as u8);
                                    } else {
                                        self.pen.bg = Color::Rgb(r as u8, g as u8, b as u8);
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else {
                        if i + 1 >= params.len() {
                            break;
                        }
                        let sub = params.get_or(i + 1, 0);
                        match sub {
                            5 => {
                                if i + 2 >= params.len() {
                                    break;
                                }
                                if let Some(val) = params.get(i + 2)
                                    && val <= 255
                                {
                                    if is_fg {
                                        self.pen.fg = Color::Indexed(val as u8);
                                    } else {
                                        self.pen.bg = Color::Indexed(val as u8);
                                    }
                                }
                                i += 2;
                            }
                            2 => {
                                if i + 4 >= params.len() {
                                    break;
                                }
                                if let (Some(r), Some(g), Some(b)) =
                                    (params.get(i + 2), params.get(i + 3), params.get(i + 4))
                                    && r <= 255
                                    && g <= 255
                                    && b <= 255
                                {
                                    if is_fg {
                                        self.pen.fg = Color::Rgb(r as u8, g as u8, b as u8);
                                    } else {
                                        self.pen.bg = Color::Rgb(r as u8, g as u8, b as u8);
                                    }
                                }
                                i += 4;
                            }
                            _ => {
                                i += 1;
                            }
                        }
                    }
                }
                _ => { /* unknown SGR code — silently ignore */ }
            }
            i += 1;
        }
    }

    pub(crate) fn set_sgr_slice(&mut self, slice: &[Option<usize>]) {
        self.set_sgr(&Params::from(slice));
    }

    // ── character sets ────────────────────────────────────────────

    pub(crate) fn designate_g0(&mut self, charset: u8) {
        self.charsets.g0 = charset;
    }

    pub(crate) fn designate_g1(&mut self, charset: u8) {
        self.charsets.g1 = charset;
    }

    pub(crate) fn set_active_charset(&mut self, active: u8) {
        self.charsets.active = active;
    }

    // ── character protection ──────────────────────────────────────

    pub(crate) fn set_character_protection(&mut self, arg: CharacterProtection) {
        self.pen.protected = match arg {
            CharacterProtection::Protected => true,
            CharacterProtection::Unprotected => false,
        };
    }

    // ── tab stops ─────────────────────────────────────────────────

    pub(crate) fn set_tab_stop(&mut self, cursor_x: usize) {
        if cursor_x < self.tab_stops.0.len() {
            self.tab_stops.0[cursor_x] = true;
        }
    }

    pub(crate) fn clear_tab_stops(&mut self, cursor_x: usize, mode: usize) {
        match mode {
            0 => {
                if cursor_x < self.tab_stops.0.len() {
                    self.tab_stops.0[cursor_x] = false;
                }
            }
            3 => {
                self.tab_stops.0.fill(false);
            }
            _ => {}
        }
    }

    // ── write_char ────────────────────────────────────────────────

    /// Writes one already-decoded printable character at the cursor and advances by its terminal
    /// cell width.
    pub(crate) fn write_char(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        ch: char,
    ) {
        let active_set = if self.charsets.active == 0 {
            self.charsets.g0
        } else {
            self.charsets.g1
        };
        let ch = if active_set == b'0' {
            map_dec_graphics(ch)
        } else {
            ch
        };

        let width = UnicodeWidthChar::width(ch).unwrap_or(0).min(2);
        if width == 0 {
            return;
        }

        // 1. Handle pending wrap if autowrap is on
        if cursor.modes.autowrap && cursor.modes.pending_wrap {
            cursor.carriage_return();
            self.newline_inner(normal, cursor);
            cursor.modes.pending_wrap = false;
        }

        let (left_limit, right_limit) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols().saturating_sub(1))
        };

        // 2. Ignore writes outside active horizontal bounds. A cursor can be
        // positioned there while origin mode is disabled, but printing must
        // not mutate cells beyond DECLRMM.
        if cursor.cursor.x < left_limit || cursor.cursor.x > right_limit {
            return;
        }
        if !cursor.modes.autowrap && cursor.cursor.x == right_limit {
            cursor.cursor.x = right_limit;
        }

        // 3. If a wide character cannot fit, wrap only when DECAWM is enabled.
        if width == 2 && cursor.cursor.x + 1 > right_limit {
            if !cursor.modes.autowrap {
                return;
            }
            cursor.carriage_return();
            self.newline_inner(normal, cursor);
            cursor.modes.pending_wrap = false;
        }
        let start_x = cursor.cursor.x;
        // Do not split a glyph crossing an active margin. Leaving the whole
        // pair intact is safer than mutating its out-of-bounds half.
        if (start_x..start_x + width).any(|col| {
            Self::wide_range(normal, cursor.cursor.y, col)
                .is_some_and(|(base, continuation)| base < left_limit || continuation > right_limit)
        }) {
            return;
        }

        if cursor.modes.insert {
            // ICH resolves a continuation to its base so the complete glyph
            // moves. Print at that same effective insertion column rather than
            // clearing the shifted glyph at the old continuation column.
            let insert_col = Self::wide_range(normal, cursor.cursor.y, start_x)
                .map_or(start_x, |(base, _)| base);
            if !self.insert_chars(normal, cursor, width) {
                return;
            }
            cursor.cursor.x = insert_col;
        }

        let start_x = cursor.cursor.x;
        let start_col = if start_x > 0 && normal.cell(cursor.cursor.y, start_x).wide_continuation {
            start_x - 1
        } else {
            start_x
        };
        let end_col = (start_x + width + 1).min(right_limit + 1);
        normal.mark_range_dirty(cursor.cursor.y, start_col, end_col);

        self.clear_cell_for_write(
            normal,
            cursor.cursor.y,
            cursor.cursor.x,
            left_limit,
            right_limit,
        );
        if width == 2 && cursor.cursor.x < right_limit {
            self.clear_cell_for_write(
                normal,
                cursor.cursor.y,
                cursor.cursor.x + 1,
                left_limit,
                right_limit,
            );
        }

        let cell = normal.live_cell_mut(cursor.cursor.y, cursor.cursor.x);
        cell.set(
            ch,
            self.pen.fg,
            self.pen.bg,
            self.pen.attrs,
            self.pen.protected,
        );

        if width == 2 && cursor.cursor.x < right_limit {
            *normal.cell_mut(cursor.cursor.y, cursor.cursor.x + 1) = Cell {
                ch: ' ',
                wide_continuation: true,
                fg: self.pen.fg,
                bg: self.pen.bg,
                attrs: self.pen.attrs,
                protected: self.pen.protected,
            };
        }

        // 4. Advance cursor and handle autowrap boundaries
        cursor.cursor.x += width;
        if cursor.cursor.x > right_limit {
            cursor.cursor.x = right_limit;
            if cursor.modes.autowrap {
                cursor.modes.pending_wrap = true;
            }
        }
        self.charsets.last_char = Some(ch);
    }

    /// Internal helper: handles the line-feed / index portion of newline for write_char.
    fn newline_inner(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine) {
        if cursor.index_needs_scroll() {
            self.scroll_region_up_one_inner(normal, cursor);
        } else {
            cursor.index_advance(normal);
        }
    }

    /// Clears the target cell and its joined cell, provided the complete glyph
    /// is inside the active horizontal bounds.
    fn clear_cell_for_write(
        &self,
        normal: &mut NormalBuf,
        row: usize,
        col: usize,
        left: usize,
        right: usize,
    ) {
        if let Some((base, continuation)) = Self::wide_range(normal, row, col) {
            if base < left || continuation > right {
                return;
            }
            *normal.cell_mut(row, base) = Cell::default();
            *normal.cell_mut(row, continuation) = Cell::default();
            return;
        }

        *normal.cell_mut(row, col) = Cell::default();
    }

    // ── erase display / line / chars ──────────────────────────────

    pub(crate) fn erase_display(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => {
                self.erase_row_range(
                    normal,
                    cursor.cursor.y,
                    cursor.cursor.x,
                    right + 1,
                    left,
                    right,
                    false,
                );
                for row in cursor.cursor.y + 1..normal.rows() {
                    self.erase_row_range(normal, row, left, right + 1, left, right, false);
                }
            }
            1 => {
                for row in 0..cursor.cursor.y {
                    self.erase_row_range(normal, row, left, right + 1, left, right, false);
                }
                self.erase_row_range(
                    normal,
                    cursor.cursor.y,
                    left,
                    cursor.cursor.x + 1,
                    left,
                    right,
                    false,
                );
            }
            2 => {
                for row in 0..normal.rows() {
                    self.erase_row_range(normal, row, left, right + 1, left, right, false);
                }
                cursor.home_cursor();
            }
            _ => {}
        }
    }

    pub(crate) fn erase_line(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => self.erase_row_range(
                normal,
                cursor.cursor.y,
                cursor.cursor.x,
                right + 1,
                left,
                right,
                false,
            ),
            1 => self.erase_row_range(
                normal,
                cursor.cursor.y,
                left,
                cursor.cursor.x + 1,
                left,
                right,
                false,
            ),
            2 => self.erase_row_range(normal, cursor.cursor.y, left, right + 1, left, right, false),
            _ => return,
        }
        normal.mark_range_dirty(
            cursor.cursor.y,
            cursor.cursor.x.saturating_sub(1).max(left),
            right + 1,
        );
    }

    pub(crate) fn erase_chars(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let right = if cursor.margins.enabled {
            cursor.margins.right
        } else {
            normal.cols() - 1
        };
        let left = if cursor.margins.enabled {
            cursor.margins.left
        } else {
            0
        };
        let end = (cursor.cursor.x + n.max(1)).min(right + 1);
        self.erase_row_range(
            normal,
            cursor.cursor.y,
            cursor.cursor.x,
            end,
            left,
            right,
            false,
        );
        normal.mark_range_dirty(
            cursor.cursor.y,
            cursor.cursor.x.saturating_sub(1).max(left),
            (end + 1).min(right + 1),
        );
    }

    // ── selective erase ───────────────────────────────────────────

    pub(crate) fn selective_erase_display(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => {
                self.erase_row_range(
                    normal,
                    cursor.cursor.y,
                    cursor.cursor.x,
                    right + 1,
                    left,
                    right,
                    true,
                );
                for row in cursor.cursor.y + 1..normal.rows() {
                    self.erase_row_range(normal, row, left, right + 1, left, right, true);
                }
            }
            1 => {
                for row in 0..cursor.cursor.y {
                    self.erase_row_range(normal, row, left, right + 1, left, right, true);
                }
                self.erase_row_range(
                    normal,
                    cursor.cursor.y,
                    left,
                    cursor.cursor.x + 1,
                    left,
                    right,
                    true,
                );
            }
            2 => {
                for row in 0..normal.rows() {
                    self.erase_row_range(normal, row, left, right + 1, left, right, true);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn selective_erase_line(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        mode: usize,
    ) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        match mode {
            0 => self.erase_row_range(
                normal,
                cursor.cursor.y,
                cursor.cursor.x,
                right + 1,
                left,
                right,
                true,
            ),
            1 => self.erase_row_range(
                normal,
                cursor.cursor.y,
                left,
                cursor.cursor.x + 1,
                left,
                right,
                true,
            ),
            2 => self.erase_row_range(normal, cursor.cursor.y, left, right + 1, left, right, true),
            _ => {}
        }
    }

    // ── insert / delete chars ─────────────────────────────────────

    /// Inserts blank cells and reports whether the shift was applied.
    ///
    /// A caller in insert mode must not fall back to overwrite when a
    /// boundary-crossing wide glyph makes the bounded shift unsafe.
    pub(crate) fn insert_chars(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) -> bool {
        cursor.modes.pending_wrap = false;
        let n = if n == 0 { 1 } else { n };
        let requested_col = cursor.cursor.x;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        normal.mark_range_dirty(cursor.cursor.y, requested_col.saturating_sub(1), right + 1);
        if requested_col < left || requested_col > right {
            return false;
        }
        // Insert before the complete glyph when the cursor is on its
        // continuation; otherwise the base would remain behind the shift.
        let col = Self::wide_range(normal, cursor.cursor.y, requested_col)
            .map_or(requested_col, |(base, _)| base);
        let n = n.min(right - col + 1);
        if n == 0 {
            return false;
        }
        // Shifting a margin region containing a boundary-crossing glyph would
        // overwrite its in-bound half and orphan the untouched outside half.
        // Leave the whole operation alone under the bounded policy.
        if Self::has_boundary_wide(normal, cursor.cursor.y, left, right + 1, left, right) {
            return false;
        }
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let row_start = ring_row * normal.cols();

        let src_start = row_start + col;
        let src_end = row_start + right - n + 1;
        let dst = row_start + col + n;
        if src_start < src_end {
            normal.copy_linear_range(src_start, src_end, dst);
        }

        normal.fill_linear_range_with(row_start + col, row_start + col + n, self.erase_cell());
        self.normalize_row_region(normal, cursor.cursor.y, left, right);
        true
    }

    pub(crate) fn delete_chars(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        cursor.modes.pending_wrap = false;
        let n = if n == 0 { 1 } else { n };
        let col = cursor.cursor.x;
        let (left, right) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, normal.cols() - 1)
        };
        normal.mark_range_dirty(cursor.cursor.y, col.saturating_sub(1), right + 1);
        if col < left || col > right {
            return;
        }
        let n = n.min(right - col + 1);
        if n == 0 {
            return;
        }
        // A shift cannot safely move a region containing a glyph straddling
        // either active horizontal boundary.
        if Self::has_boundary_wide(normal, cursor.cursor.y, left, right + 1, left, right) {
            return;
        }
        // Resolve the complete deletion range before mutating the row. In
        // particular, a cursor on a continuation deletes its base as well.
        let (delete_start, delete_end) =
            self.normalize_touched_range(normal, cursor.cursor.y, col, col + n, left, right);
        let delete_width = delete_end - delete_start;
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let row_start = ring_row * normal.cols();
        let src_start = row_start + delete_end;
        let src_end = row_start + right + 1;
        let dst = row_start + delete_start;
        if src_start < src_end {
            normal.copy_linear_range(src_start, src_end, dst);
        }
        let blank_start = row_start + right + 1 - delete_width;
        normal.fill_linear_range_with(blank_start, blank_start + delete_width, self.erase_cell());
        self.normalize_row_region(normal, cursor.cursor.y, left, right);
    }

    // ── insert / delete lines ─────────────────────────────────────

    pub(crate) fn insert_lines(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        if cursor.cursor.y < cursor.scroll_region.top
            || cursor.cursor.y > cursor.scroll_region.bottom
        {
            return;
        }
        let max_n = cursor.scroll_region.bottom - cursor.cursor.y + 1;
        let n = n.min(max_n);
        normal.mark_rows_dirty(
            cursor.cursor.y,
            cursor.scroll_region.bottom.saturating_add(1),
        );
        if cursor.margins.enabled {
            self.scroll_margin_rect_down(
                normal,
                cursor,
                cursor.cursor.y,
                cursor.scroll_region.bottom,
                n,
            );
            cursor.cursor.x = 0;
            return;
        }
        if n == max_n {
            for row in cursor.cursor.y..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, self.erase_cell());
            }
            cursor.cursor.x = 0;
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.cursor.y) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom - n + 1) % tr) * c;
        let dst = ((vis + cursor.cursor.y + n) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.cursor.y + i, self.erase_cell());
        }
        for row in cursor.cursor.y..=cursor.scroll_region.bottom {
            self.normalize_row_region(normal, row, 0, normal.cols() - 1);
        }
        cursor.cursor.x = 0;
    }

    pub(crate) fn delete_lines(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        if cursor.cursor.y < cursor.scroll_region.top
            || cursor.cursor.y > cursor.scroll_region.bottom
        {
            return;
        }
        let max_n = cursor.scroll_region.bottom - cursor.cursor.y + 1;
        let n = n.min(max_n);
        normal.mark_rows_dirty(
            cursor.cursor.y,
            cursor.scroll_region.bottom.saturating_add(1),
        );
        if cursor.margins.enabled {
            self.scroll_margin_rect_up(
                normal,
                cursor,
                cursor.cursor.y,
                cursor.scroll_region.bottom,
                n,
            );
            cursor.cursor.x = 0;
            return;
        }
        if n == max_n {
            for row in cursor.cursor.y..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, self.erase_cell());
            }
            cursor.cursor.x = 0;
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.cursor.y + n) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom + 1) % tr) * c;
        let dst = ((vis + cursor.cursor.y) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.scroll_region.bottom - i, self.erase_cell());
        }
        for row in cursor.cursor.y..=cursor.scroll_region.bottom {
            self.normalize_row_region(normal, row, 0, normal.cols() - 1);
        }
        cursor.cursor.x = 0;
    }

    // ── scroll region ─────────────────────────────────────────────

    pub(crate) fn scroll_up_region(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        let region_height = cursor.scroll_region.bottom - cursor.scroll_region.top + 1;
        let n = n.min(region_height);
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            normal.mark_row_dirty(row);
        }
        if cursor.margins.enabled {
            self.scroll_margin_rect_up(
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                n,
            );
            return;
        }
        if n == region_height {
            for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, self.erase_cell());
            }
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.scroll_region.top + n) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom + 1) % tr) * c;
        let dst = ((vis + cursor.scroll_region.top) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.scroll_region.bottom - i, self.erase_cell());
        }
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            self.normalize_row_region(normal, row, 0, normal.cols() - 1);
        }
    }

    pub(crate) fn scroll_down_region(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
        n: usize,
    ) {
        let n = if n == 0 { 1 } else { n };
        let region_height = cursor.scroll_region.bottom - cursor.scroll_region.top + 1;
        let n = n.min(region_height);
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            normal.mark_row_dirty(row);
        }
        if cursor.margins.enabled {
            self.scroll_margin_rect_down(
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                n,
            );
            return;
        }
        if n == region_height {
            for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
                normal.fill_row_with(row, self.erase_cell());
            }
            return;
        }
        let tr = normal.total_rows();
        let vis = normal.visible_start();
        let c = normal.cols();
        let src_start = ((vis + cursor.scroll_region.top) % tr) * c;
        let src_end = ((vis + cursor.scroll_region.bottom - n + 1) % tr) * c;
        let dst = ((vis + cursor.scroll_region.top + n) % tr) * c;
        normal.copy_ring_range(src_start, src_end, dst);
        for i in 0..n {
            normal.fill_row_with(cursor.scroll_region.top + i, self.erase_cell());
        }
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            self.normalize_row_region(normal, row, 0, normal.cols() - 1);
        }
    }

    // ── margin-rect scroll helpers ────────────────────────────────

    fn scroll_margin_rect_up(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        top: usize,
        bottom: usize,
        n: usize,
    ) {
        let height = bottom - top + 1;
        if n < height {
            for dst_row in top..=(bottom - n) {
                let src_row = dst_row + n;
                for col in cursor.margins.left..=cursor.margins.right {
                    let cell = *normal.cell(src_row, col);
                    *normal.cell_mut(dst_row, col) = cell;
                }
            }
        }
        let blank = self.erase_cell();
        for row in (bottom + 1 - n)..=bottom {
            for col in cursor.margins.left..=cursor.margins.right {
                *normal.cell_mut(row, col) = blank;
            }
        }
        for row in top..=bottom {
            self.normalize_row_region(normal, row, cursor.margins.left, cursor.margins.right);
        }
    }

    fn scroll_margin_rect_down(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        top: usize,
        bottom: usize,
        n: usize,
    ) {
        let height = bottom - top + 1;
        if n < height {
            for dst_row in ((top + n)..=bottom).rev() {
                let src_row = dst_row - n;
                for col in cursor.margins.left..=cursor.margins.right {
                    let cell = *normal.cell(src_row, col);
                    *normal.cell_mut(dst_row, col) = cell;
                }
            }
        }
        let blank = self.erase_cell();
        for row in top..(top + n) {
            for col in cursor.margins.left..=cursor.margins.right {
                *normal.cell_mut(row, col) = blank;
            }
        }
        for row in top..=bottom {
            self.normalize_row_region(normal, row, cursor.margins.left, cursor.margins.right);
        }
    }

    // ── internal scroll_region_up_one (used by write_char inline path) ──

    /// Scrolls the scrolling region up by one row and sets the cursor to the
    /// bottom of the region.  This is the core of the Screen-level
    /// `scroll_region_up_one` coordinator method, extracted so `write_char`
    /// can use it without going through Screen.
    pub(crate) fn scroll_region_up_one_inner(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &mut CursorEngine,
    ) {
        normal.mark_rows_dirty(
            cursor.scroll_region.top,
            cursor.scroll_region.bottom.saturating_add(1),
        );
        if cursor.margins.enabled {
            self.scroll_margin_rect_up(
                normal,
                cursor,
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                1,
            );
        } else if cursor.scroll_region.top == 0 && cursor.scroll_region.bottom == normal.rows() - 1
        {
            normal.scroll_up_full_screen(1, self.erase_cell());
        } else {
            let tr = normal.total_rows();
            let vis = normal.visible_start();
            let c = normal.cols();
            let src_start = ((vis + cursor.scroll_region.top + 1) % tr) * c;
            let src_end = ((vis + cursor.scroll_region.bottom + 1) % tr) * c;
            let dst = ((vis + cursor.scroll_region.top) % tr) * c;
            normal.copy_ring_range(src_start, src_end, dst);
            normal.fill_row_with(cursor.scroll_region.bottom, self.erase_cell());
        }
        if !cursor.margins.enabled {
            for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
                self.normalize_row_region(normal, row, 0, normal.cols() - 1);
            }
        }
        cursor.cursor.y = cursor.scroll_region.bottom;
    }

    // ── DEC rectangle operations ──────────────────────────────────

    pub(crate) fn decera(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };
        for row in t..=b {
            self.erase_row_range(normal, row, l, r + 1, 0, normal.cols() - 1, false);
        }
    }

    pub(crate) fn decsera(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };
        for row in t..=b {
            self.erase_row_range(normal, row, l, r + 1, 0, normal.cols() - 1, true);
        }
    }

    pub(crate) fn decfra(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let ch_val = params.get_or(0, 0);
        let top = params.get_or(1, 0);
        let left = params.get_or(2, 0);
        let bottom = params.get_or(3, 0);
        let right = params.get_or(4, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };
        let fill_char = if (32..=126).contains(&ch_val) || (160..=255).contains(&ch_val) {
            (ch_val as u8) as char
        } else {
            ' '
        };

        let cell = Cell {
            ch: fill_char,
            wide_continuation: false,
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
            protected: self.pen.protected,
        };

        for row in t..=b {
            let (start, end) =
                self.normalize_touched_range(normal, row, l, r + 1, 0, normal.cols() - 1);
            for col in start..end {
                *normal.cell_mut(row, col) = cell;
            }
            self.normalize_row_region(normal, row, 0, normal.cols() - 1);
        }
    }

    pub(crate) fn deccra(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let src_top = params.get_or(0, 0);
        let src_left = params.get_or(1, 0);
        let src_bottom = params.get_or(2, 0);
        let src_right = params.get_or(3, 0);
        let dest_top = params.get_or(5, 0);
        let dest_left = params.get_or(6, 0);

        let Some(Rect {
            top: st,
            left: sl,
            bottom: sb,
            right: sr,
        }) = cursor.resolve_rect(normal, src_top, src_left, src_bottom, src_right)
        else {
            return;
        };

        let dt_start = if cursor.modes.origin {
            let r = dest_top.saturating_sub(1);
            cursor.scroll_region.top + r
        } else {
            dest_top.saturating_sub(1)
        };
        let dl_start = if cursor.modes.origin {
            let c = dest_left.saturating_sub(1);
            cursor.margins.left + c
        } else {
            dest_left.saturating_sub(1)
        };

        let height = sb - st + 1;
        let width = sr - sl + 1;

        let erase = self.erase_cell();
        let mut temp = Vec::with_capacity(height * width);
        for row in st..=sb {
            for col in sl..=sr {
                let complete = match Self::wide_range(normal, row, col) {
                    Some((base, continuation)) => base >= sl && continuation <= sr,
                    None => {
                        !normal.cell(row, col).wide_continuation
                            && UnicodeWidthChar::width(normal.cell(row, col).ch).unwrap_or(0) != 2
                    }
                };
                temp.push(if complete {
                    *normal.cell(row, col)
                } else {
                    erase
                });
            }
        }

        // Destination coordinates use the same bounds as resolve_rect:
        // origin-off copies use the full screen, while origin-on copies are
        // confined to the scroll region and active horizontal margins.
        let (dest_top, dest_bottom, dest_left, dest_right) = if cursor.modes.origin {
            (
                cursor.scroll_region.top,
                cursor.scroll_region.bottom,
                cursor.margins.left,
                cursor.margins.right,
            )
        } else {
            (0, normal.rows() - 1, 0, normal.cols() - 1)
        };
        let Some(dest_row_end) = dt_start.checked_add(height) else {
            return;
        };
        let Some(dest_col_end) = dl_start.checked_add(width) else {
            return;
        };
        let row_start = dt_start.max(dest_top);
        let row_end = dest_row_end.min(dest_bottom + 1);
        let col_start = dl_start.max(dest_left);
        let col_end = dest_col_end.min(dest_right + 1);
        if row_start >= row_end || col_start >= col_end {
            return;
        }

        for dest_row in row_start..row_end {
            let h = dest_row - dt_start;
            self.erase_row_range(
                normal, dest_row, col_start, col_end, dest_left, dest_right, false,
            );
            for dest_col in col_start..col_end {
                let w = dest_col - dl_start;
                // A pre-existing pair crossing the active destination edge is
                // indivisible. The erase helper leaves it intact; keep the
                // write from replacing its in-bound half as well.
                if Self::wide_range(normal, dest_row, dest_col).is_some_and(
                    |(base, continuation)| base < dest_left || continuation > dest_right,
                ) {
                    continue;
                }
                let mut cell = temp[h * width + w];
                if cell.wide_continuation || UnicodeWidthChar::width(cell.ch).unwrap_or(0) == 2 {
                    let pair_in_bounds = if cell.wide_continuation {
                        dest_col > col_start && dest_col > dest_left
                    } else {
                        dest_col + 1 < col_end && dest_col < dest_right
                    };
                    if !pair_in_bounds {
                        cell = erase;
                    }
                }
                *normal.cell_mut(dest_row, dest_col) = cell;
            }
            self.normalize_row_region(normal, dest_row, dest_left, dest_right);
        }
    }

    pub(crate) fn deccara(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };

        for row in t..=b {
            let (start, end) =
                self.normalize_touched_range(normal, row, l, r + 1, 0, normal.cols() - 1);
            for col in start..end {
                let cell = normal.cell_mut(row, col);
                for code in params.iter_flat().skip(4).flatten() {
                    cell.apply_sgr(code);
                }
            }
        }
    }

    pub(crate) fn decrara(
        &mut self,
        normal: &mut NormalBuf,
        cursor: &CursorEngine,
        params: &Params,
    ) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = cursor.resolve_rect(normal, top, left, bottom, right)
        else {
            return;
        };

        for row in t..=b {
            let (start, end) =
                self.normalize_touched_range(normal, row, l, r + 1, 0, normal.cols() - 1);
            for col in start..end {
                let cell = normal.cell_mut(row, col);
                for code in params.iter_flat().skip(4).flatten() {
                    cell.toggle_sgr(code);
                }
            }
        }
    }
}

/// Maps the DEC Special Graphics character set (designator `'0'`).
pub(crate) fn map_dec_graphics(ch: char) -> char {
    match ch {
        '`' => '◆',
        'a' => '▒',
        'f' => '°',
        'g' => '±',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        _ => ch,
    }
}
