//! VT edit engine: pen (SGR), tab stops, character sets, and cell-level mutations.
//!
//! Methods that need cursor state receive `&mut CursorEngine` or individual
//! cursor fields.  Methods that modify the grid take `&mut NormalBuf`.

use crate::normal_buf::NormalBuf;
use harbor_parser::Params;
use harbor_types::{Cell, CellAttrs, Color};
use unicode_width::UnicodeWidthChar;

use super::cursor::{CursorEngine, Margins};

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

    pub(crate) fn set_character_protection(&mut self, ps: usize) {
        match ps {
            0 | 2 => {
                self.pen.protected = false;
            }
            1 => {
                self.pen.protected = true;
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
            // Simulate newline: carriage_return + index-like advance.
            cursor.carriage_return();
            self.newline_inner(normal, cursor);
            cursor.modes.pending_wrap = false;
        }

        let right_limit = if cursor.margins.enabled {
            cursor.margins.right
        } else {
            normal.cols().saturating_sub(1)
        };

        // 2. Clamp cursor if autowrap is off to prevent overflow
        if !cursor.modes.autowrap && cursor.cursor.x >= right_limit {
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
        if cursor.modes.insert {
            self.insert_chars(normal, cursor, width);
        }

        let start_x = cursor.cursor.x;
        let start_col = if start_x > 0 && normal.cell(cursor.cursor.y, start_x).wide_continuation {
            start_x - 1
        } else {
            start_x
        };
        let end_col = (start_x + width + 1).min(normal.cols());
        normal.mark_range_dirty(cursor.cursor.y, start_col, end_col);

        let index = normal.display_to_ring(cursor.cursor.y) * normal.cols() + cursor.cursor.x;
        self.clear_cell_for_write(normal, index);
        if width == 2 && cursor.cursor.x < right_limit {
            self.clear_cell_for_write(normal, index + 1);
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
            *normal.cell_linear_mut(index + 1) = Cell {
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
    /// Does NOT call carriage_return (caller does that).
    fn newline_inner(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine) {
        if cursor.index_needs_scroll() {
            // Inline scroll_region_up_one logic (needed for inline use from write_char).
            self.scroll_region_up_one_inner(normal, cursor);
        } else {
            cursor.index_advance(normal);
        }
    }

    /// Clears the target cell *and* any joined cell from a double-width glyph that overlaps it.
    fn clear_cell_for_write(&self, normal: &mut NormalBuf, index: usize) {
        debug_assert!(
            index > 0 || !normal.cell_linear(index).wide_continuation,
            "wide_continuation at column 0 is invalid"
        );

        if normal.cell_linear(index).wide_continuation {
            *normal.cell_linear_mut(index - 1) = Cell::default();
            *normal.cell_linear_mut(index) = Cell::default();
            return;
        }

        if UnicodeWidthChar::width(normal.cell_linear(index).ch).unwrap_or(0) == 2
            && index % normal.cols() + 1 < normal.cols()
        {
            *normal.cell_linear_mut(index + 1) = Cell::default();
        }
        *normal.cell_linear_mut(index) = Cell::default();
    }

    // ── erase display / line / chars ──────────────────────────────

    pub(crate) fn erase_display(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, mode: usize) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let cell = self.erase_cell();
        let cols = normal.cols();
        let (left_col, right_col) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, cols - 1)
        };

        match mode {
            0 => {
                normal.mark_range_dirty(cursor.cursor.y, cursor.cursor.x, right_col + 1);
                let ring_row = normal.display_to_ring(cursor.cursor.y);
                let start = ring_row * cols + cursor.cursor.x;
                let end = ring_row * cols + right_col + 1;
                normal.fill_linear_range_with(start, end, cell);
                for row in cursor.cursor.y + 1..normal.rows() {
                    normal.mark_row_dirty(row);
                    let r_row = normal.display_to_ring(row);
                    normal.fill_linear_range_with(
                        r_row * cols + left_col,
                        r_row * cols + right_col + 1,
                        cell,
                    );
                }
            }
            1 => {
                for row in 0..cursor.cursor.y {
                    normal.mark_row_dirty(row);
                    let r_row = normal.display_to_ring(row);
                    normal.fill_linear_range_with(
                        r_row * cols + left_col,
                        r_row * cols + right_col + 1,
                        cell,
                    );
                }
                normal.mark_range_dirty(cursor.cursor.y, left_col, cursor.cursor.x + 1);
                let ring_row = normal.display_to_ring(cursor.cursor.y);
                let start = ring_row * cols + left_col;
                let end = ring_row * cols + cursor.cursor.x + 1;
                normal.fill_linear_range_with(start, end, cell);
            }
            2 => {
                for row in 0..normal.rows() {
                    let r_row = normal.display_to_ring(row);
                    normal.fill_linear_range_with(
                        r_row * cols + left_col,
                        r_row * cols + right_col + 1,
                        cell,
                    );
                }
                cursor.home_cursor();
                normal.mark_all_dirty();
            }
            _ => {}
        }
    }

    pub(crate) fn erase_line(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, mode: usize) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let cell = self.erase_cell();
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let cols = normal.cols();
        let (left_col, right_col) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, cols - 1)
        };
        let start = ring_row * cols + left_col;
        let cursor_idx = ring_row * cols + cursor.cursor.x;
        let end = ring_row * cols + right_col + 1;
        match mode {
            0 => normal.mark_range_dirty(
                cursor.cursor.y,
                cursor.cursor.x.saturating_sub(1),
                right_col + 1,
            ),
            1 => normal.mark_range_dirty(cursor.cursor.y, left_col, (cursor.cursor.x + 2).min(cols)),
            2 => normal.mark_range_dirty(cursor.cursor.y, left_col, right_col + 1),
            _ => {}
        }
        match mode {
            0 => normal.fill_linear_range_with(cursor_idx, end, cell),
            1 => normal.fill_linear_range_with(start, cursor_idx + 1, cell),
            2 => normal.fill_linear_range_with(start, end, cell),
            _ => {}
        }
    }

    pub(crate) fn erase_chars(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, n: usize) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        cursor.modes.pending_wrap = false;
        let cell = self.erase_cell();
        let n = if n == 0 { 1 } else { n };
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let cols = normal.cols();
        let right_col = if cursor.margins.enabled {
            cursor.margins.right
        } else {
            cols - 1
        };
        let end_col = (cursor.cursor.x + n).min(right_col + 1);
        normal.mark_range_dirty(
            cursor.cursor.y,
            cursor.cursor.x.saturating_sub(1),
            (end_col + 1).min(cols),
        );
        let start = ring_row * cols + cursor.cursor.x;
        let end = (start + n).min(ring_row * cols + right_col + 1);
        normal.fill_linear_range_with(start, end, cell);
    }

    // ── selective erase ───────────────────────────────────────────

    pub(crate) fn selective_erase_display(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, mode: usize) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        let erase = self.erase_cell();
        let cols = normal.cols();
        let (left_col, right_col) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, cols - 1)
        };

        match mode {
            0 => {
                normal.mark_range_dirty(cursor.cursor.y, cursor.cursor.x, right_col + 1);
                let ring_row = normal.display_to_ring(cursor.cursor.y);
                let start_idx = ring_row * cols + cursor.cursor.x;
                let row_end = ring_row * cols + right_col + 1;
                for idx in start_idx..row_end {
                    let cell = normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
                for row in cursor.cursor.y + 1..normal.rows() {
                    normal.mark_range_dirty(row, left_col, right_col + 1);
                    let r_row = normal.display_to_ring(row);
                    let r_start = r_row * cols + left_col;
                    let r_end = r_row * cols + right_col + 1;
                    for idx in r_start..r_end {
                        let cell = normal.cell_linear_mut(idx);
                        if !cell.protected {
                            *cell = erase;
                        }
                    }
                }
            }
            1 => {
                for row in 0..cursor.cursor.y {
                    normal.mark_range_dirty(row, left_col, right_col + 1);
                    let r_row = normal.display_to_ring(row);
                    let r_start = r_row * cols + left_col;
                    let r_end = r_row * cols + right_col + 1;
                    for idx in r_start..r_end {
                        let cell = normal.cell_linear_mut(idx);
                        if !cell.protected {
                            *cell = erase;
                        }
                    }
                }
                normal.mark_range_dirty(cursor.cursor.y, left_col, cursor.cursor.x + 1);
                let ring_row = normal.display_to_ring(cursor.cursor.y);
                let start_idx = ring_row * cols + left_col;
                let end_idx = ring_row * cols + cursor.cursor.x + 1;
                for idx in start_idx..end_idx {
                    let cell = normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            2 => {
                for row in 0..normal.rows() {
                    normal.mark_range_dirty(row, left_col, right_col + 1);
                    let r_row = normal.display_to_ring(row);
                    let r_start = r_row * cols + left_col;
                    let r_end = r_row * cols + right_col + 1;
                    for idx in r_start..r_end {
                        let cell = normal.cell_linear_mut(idx);
                        if !cell.protected {
                            *cell = erase;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn selective_erase_line(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, mode: usize) {
        if cursor.margins.enabled
            && (cursor.cursor.x < cursor.margins.left || cursor.cursor.x > cursor.margins.right)
        {
            return;
        }
        let erase = self.erase_cell();
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let cols = normal.cols();
        let (left_col, right_col) = if cursor.margins.enabled {
            (cursor.margins.left, cursor.margins.right)
        } else {
            (0, cols - 1)
        };
        let start_idx = ring_row * cols + left_col;
        let cursor_idx = ring_row * cols + cursor.cursor.x;
        let end_idx = ring_row * cols + right_col + 1;
        match mode {
            0 => {
                normal.mark_range_dirty(cursor.cursor.y, cursor.cursor.x, right_col + 1);
                for idx in cursor_idx..end_idx {
                    let cell = normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            1 => {
                normal.mark_range_dirty(cursor.cursor.y, left_col, cursor.cursor.x + 1);
                for idx in start_idx..=cursor_idx {
                    let cell = normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            2 => {
                normal.mark_range_dirty(cursor.cursor.y, left_col, right_col + 1);
                for idx in start_idx..end_idx {
                    let cell = normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            _ => {}
        }
    }

    // ── insert / delete chars ─────────────────────────────────────

    pub(crate) fn insert_chars(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, n: usize) {
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
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let row_start = ring_row * normal.cols();

        let src_start = row_start + col;
        let src_end = row_start + right - n + 1;
        let dst = row_start + col + n;
        if src_start < src_end {
            normal.copy_linear_range(src_start, src_end, dst);
        }

        normal.fill_linear_range_with(row_start + col, row_start + col + n, self.erase_cell());
    }

    pub(crate) fn delete_chars(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, n: usize) {
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
        let ring_row = normal.display_to_ring(cursor.cursor.y);
        let row_start = ring_row * normal.cols();

        let src_start = row_start + col + n;
        let src_end = row_start + right + 1;
        let dst = row_start + col;
        if src_start < src_end {
            normal.copy_linear_range(src_start, src_end, dst);
        }

        let blank_start = row_start + right + 1 - n;
        normal.fill_linear_range_with(blank_start, blank_start + n, self.erase_cell());
    }

    // ── insert / delete lines ─────────────────────────────────────

    pub(crate) fn insert_lines(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, n: usize) {
        let n = if n == 0 { 1 } else { n };
        if cursor.cursor.y < cursor.scroll_region.top || cursor.cursor.y > cursor.scroll_region.bottom {
            return;
        }
        let max_n = cursor.scroll_region.bottom - cursor.cursor.y + 1;
        let n = n.min(max_n);
        normal.mark_rows_dirty(cursor.cursor.y, cursor.scroll_region.bottom.saturating_add(1));
        if cursor.margins.enabled {
            self.scroll_margin_rect_down(normal, cursor, cursor.cursor.y, cursor.scroll_region.bottom, n);
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
        cursor.cursor.x = 0;
    }

    pub(crate) fn delete_lines(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, n: usize) {
        let n = if n == 0 { 1 } else { n };
        if cursor.cursor.y < cursor.scroll_region.top || cursor.cursor.y > cursor.scroll_region.bottom {
            return;
        }
        let max_n = cursor.scroll_region.bottom - cursor.cursor.y + 1;
        let n = n.min(max_n);
        normal.mark_rows_dirty(cursor.cursor.y, cursor.scroll_region.bottom.saturating_add(1));
        if cursor.margins.enabled {
            self.scroll_margin_rect_up(normal, cursor, cursor.cursor.y, cursor.scroll_region.bottom, n);
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
        cursor.cursor.x = 0;
    }

    // ── scroll region ─────────────────────────────────────────────

    pub(crate) fn scroll_up_region(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, n: usize) {
        let n = if n == 0 { 1 } else { n };
        let region_height = cursor.scroll_region.bottom - cursor.scroll_region.top + 1;
        let n = n.min(region_height);
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            normal.mark_row_dirty(row);
        }
        if cursor.margins.enabled {
            self.scroll_margin_rect_up(normal, cursor, cursor.scroll_region.top, cursor.scroll_region.bottom, n);
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
    }

    pub(crate) fn scroll_down_region(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine, n: usize) {
        let n = if n == 0 { 1 } else { n };
        let region_height = cursor.scroll_region.bottom - cursor.scroll_region.top + 1;
        let n = n.min(region_height);
        for row in cursor.scroll_region.top..=cursor.scroll_region.bottom {
            normal.mark_row_dirty(row);
        }
        if cursor.margins.enabled {
            self.scroll_margin_rect_down(normal, cursor, cursor.scroll_region.top, cursor.scroll_region.bottom, n);
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
    }

    // ── margin-rect scroll helpers ────────────────────────────────

    fn scroll_margin_rect_up(&mut self, normal: &mut NormalBuf, cursor: &CursorEngine, top: usize, bottom: usize, n: usize) {
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
    }

    fn scroll_margin_rect_down(&mut self, normal: &mut NormalBuf, cursor: &CursorEngine, top: usize, bottom: usize, n: usize) {
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
    }

    // ── internal scroll_region_up_one (used by write_char inline path) ──

    /// Scrolls the scrolling region up by one row and sets the cursor to the
    /// bottom of the region.  This is the core of the Screen-level
    /// `scroll_region_up_one` coordinator method, extracted so `write_char`
    /// can use it without going through Screen.
    pub(crate) fn scroll_region_up_one_inner(&mut self, normal: &mut NormalBuf, cursor: &mut CursorEngine) {
        normal.mark_rows_dirty(
            cursor.scroll_region.top,
            cursor.scroll_region.bottom.saturating_add(1),
        );
        if cursor.margins.enabled {
            self.scroll_margin_rect_up(normal, cursor, cursor.scroll_region.top, cursor.scroll_region.bottom, 1);
        } else if cursor.scroll_region.top == 0 && cursor.scroll_region.bottom == normal.rows() - 1 {
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
        let erase = self.erase_cell();
        for row in t..=b {
            normal.mark_range_dirty(row, l, r + 1);
            for col in l..=r {
                *normal.cell_mut(row, col) = erase;
            }
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
        let erase = self.erase_cell();
        for row in t..=b {
            normal.mark_range_dirty(row, l, r + 1);
            for col in l..=r {
                let cell = normal.cell_mut(row, col);
                if !cell.protected {
                    *cell = erase;
                }
            }
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
            normal.mark_range_dirty(row, l, r + 1);
            for col in l..=r {
                *normal.cell_mut(row, col) = cell;
            }
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

        let mut temp = Vec::with_capacity(height * width);
        for row in st..=sb {
            for col in sl..=sr {
                temp.push(*normal.cell(row, col));
            }
        }

        let max_rows = normal.rows();
        let max_cols = normal.cols();

        for h in 0..height {
            let dest_row = dt_start + h;
            if dest_row >= max_rows {
                break;
            }
            normal.mark_range_dirty(dest_row, dl_start, (dl_start + width).min(max_cols));
            for w in 0..width {
                let dest_col = dl_start + w;
                if dest_col >= max_cols {
                    break;
                }
                let src_cell = temp[h * width + w];
                *normal.cell_mut(dest_row, dest_col) = src_cell;
            }
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
            normal.mark_range_dirty(row, l, r + 1);
            for col in l..=r {
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
            normal.mark_range_dirty(row, l, r + 1);
            for col in l..=r {
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
