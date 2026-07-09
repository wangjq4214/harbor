use super::NormalBuf;
use super::normal_buf::CellsIter;

use unicode_width::UnicodeWidthChar;

/// Pending alternate-screen action set by the parser and consumed by Terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AltScreenAction {
    Enter,
    Exit,
}

/// Terminal color value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Color {
    /// Use the terminal's default foreground/background.
    Default,
    /// Standard ANSI colors 0-7.
    Named(u8),
    /// Bright ANSI colors 0-7 (rendered as palette entries 8-15).
    Bright(u8),
    /// 256-color palette index 0-255.
    Indexed(u8),
    /// Truecolor RGB.
    Rgb(u8, u8, u8),
}

/// Standard ANSI color palette (indices 0-7).
const ANSI_COLORS: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0],          // Black
    [0.8039, 0.0, 0.0],       // Red
    [0.0, 0.8039, 0.0],       // Green
    [0.8039, 0.8039, 0.0],    // Yellow
    [0.0, 0.0, 0.8039],       // Blue
    [0.8039, 0.0, 0.8039],    // Magenta
    [0.0, 0.8039, 0.8039],    // Cyan
    [0.8980, 0.8980, 0.8980], // White
];

/// Bright ANSI color palette (indices 0-7) — lighter variants.
const BRIGHT_COLORS: [[f32; 3]; 8] = [
    [0.4980, 0.4980, 0.4980], // Bright Black (Gray)
    [1.0, 0.0, 0.0],          // Bright Red
    [0.0, 1.0, 0.0],          // Bright Green
    [1.0, 1.0, 0.0],          // Bright Yellow
    [0.3608, 0.3608, 1.0],    // Bright Blue
    [1.0, 0.0, 1.0],          // Bright Magenta
    [0.0, 1.0, 1.0],          // Bright Cyan
    [1.0, 1.0, 1.0],          // Bright White
];

impl Color {
    /// Converts to normalized [r, g, b, a] at full opacity.
    /// `Default` returns white; background layers skip `Default` cells.
    pub(crate) fn to_rgba(self) -> [f32; 4] {
        match self {
            Color::Default => [1.0, 1.0, 1.0, 1.0],
            Color::Named(n) => {
                let &[r, g, b] = ANSI_COLORS.get(n as usize).unwrap_or(&[0.0, 0.0, 0.0]);
                [r, g, b, 1.0]
            }
            Color::Bright(n) => {
                let &[r, g, b] = BRIGHT_COLORS.get(n as usize).unwrap_or(&[0.0, 0.0, 0.0]);
                [r, g, b, 1.0]
            }
            Color::Indexed(n) => match n {
                0..=7 => {
                    let [r, g, b] = ANSI_COLORS[n as usize];
                    [r, g, b, 1.0]
                }
                8..=15 => {
                    let [r, g, b] = BRIGHT_COLORS[(n - 8) as usize];
                    [r, g, b, 1.0]
                }
                16..=231 => {
                    let idx = n - 16;
                    let r = idx / 36;
                    let g = (idx % 36) / 6;
                    let b = idx % 6;
                    let expand = |v: u8| -> f32 {
                        match v {
                            0 => 0.0,
                            1 => 95.0 / 255.0,
                            2 => 135.0 / 255.0,
                            3 => 175.0 / 255.0,
                            4 => 215.0 / 255.0,
                            _ => 1.0,
                        }
                    };
                    [expand(r), expand(g), expand(b), 1.0]
                }
                _ => {
                    // 232-255: greyscale ramp from (8,8,8) to (238,238,238)
                    let step = n - 232;
                    let v = (8 + step * 10) as f32 / 255.0;
                    [v, v, v, 1.0]
                }
            },
            Color::Rgb(r, g, b) => [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0],
        }
    }
}

/// Text style attributes, stored as a compact bitset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CellAttrs(u8);

impl CellAttrs {
    pub(crate) const BOLD: u8 = 1 << 0;
    pub(crate) const DIM: u8 = 1 << 1;
    pub(crate) const ITALIC: u8 = 1 << 2;
    pub(crate) const UNDERLINE: u8 = 1 << 3;
    pub(crate) const BLINK: u8 = 1 << 4;
    pub(crate) const INVERSE: u8 = 1 << 5;
    pub(crate) const STRIKETHROUGH: u8 = 1 << 6;

    #[allow(dead_code)]
    pub(crate) fn contains(self, bits: u8) -> bool {
        self.0 & bits != 0
    }
    pub(crate) fn set(&mut self, bits: u8) {
        self.0 |= bits;
    }
    pub(crate) fn clear(&mut self, bits: u8) {
        self.0 &= !bits;
    }
    #[allow(dead_code)]
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One visible terminal grid cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    /// Character currently displayed in this cell.
    pub(crate) ch: char,
    /// True when this cell is the hidden trailing half of a double-width character.
    wide_continuation: bool,
    /// Foreground color.
    pub(crate) fg: Color,
    /// Background color.
    pub(crate) bg: Color,
    /// Text style attributes.
    pub(crate) attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            wide_continuation: false,
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
        }
    }
}

impl Cell {
    /// Sets all fields atomically (ensures no field is forgotten on add).
    pub(crate) fn set(&mut self, ch: char, fg: Color, bg: Color, attrs: CellAttrs) {
        self.ch = ch;
        self.wide_continuation = false;
        self.fg = fg;
        self.bg = bg;
        self.attrs = attrs;
    }
}

/// Saved terminal state for cursor save/restore (DECSC/DECRC). Captures the cursor position
/// and SGR attributes so the screen can be restored after a screen-altering operation.
#[derive(Debug, Clone)]
struct SavedCursor {
    cursor_x: usize,
    cursor_y: usize,
    fg: Color,
    bg: Color,
    attrs: CellAttrs,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CursorShape {
    Block,
    Underline,
    #[default]
    Bar,
}

/// Display-coordinate bounds of a text selection, row-major, inclusive.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectionBounds {
    pub(crate) start_row: usize,
    pub(crate) start_col: usize,
    pub(crate) end_row: usize,
    pub(crate) end_col: usize,
}

/// Visible terminal screen state rendered by the text pipeline.
///
/// `Screen` owns only display state: cell contents, dimensions, and cursor position. It does not
/// parse byte streams; `TerminalParser` calls these methods after recognizing control sequences.
#[derive(Debug)]
pub(crate) struct Screen {
    /// Ring-buffer scrollback storage.
    normal: NormalBuf,
    /// Whether the alternate screen is active.
    in_alt: bool,
    /// 0-based column within the current row.
    cursor_x: usize,
    /// 0-based row index.
    cursor_y: usize,
    /// Current SGR foreground applied to each character written.
    current_fg: Color,
    /// Current SGR background applied to each character written.
    current_bg: Color,
    /// Current SGR attributes applied to each character written.
    current_attrs: CellAttrs,
    /// Pending alternate-screen request set by the parser, consumed by Terminal.
    alt_request: Option<AltScreenAction>,
    /// Top boundary of the scrolling region (DECSTBM). 0-based, inclusive.
    scroll_top: usize,
    /// Bottom boundary of the scrolling region (DECSTBM). 0-based, inclusive.
    scroll_bottom: usize,
    /// Current cursor shape from DECSCUSR.
    cursor_shape: CursorShape,
    /// Whether cursor should blink.
    cursor_blink: bool,
    /// Saved cursor state (DECSC/DECRC), or `None` before any save.
    saved_cursor: Option<SavedCursor>,
    /// Saved normal-screen state while the alternate screen is active.
    alt_saved: Option<AltSavedState>,
}

/// State saved when entering the alternate screen and restored on exit.
#[derive(Debug)]
struct AltSavedState {
    cells: Vec<Cell>,
    dirty_rows: Vec<bool>,
    cursor_x: usize,
    cursor_y: usize,
    current_fg: Color,
    current_bg: Color,
    current_attrs: CellAttrs,
    scroll_top: usize,
    scroll_bottom: usize,
    cursor_shape: CursorShape,
    cursor_blink: bool,
    saved_cursor: Option<SavedCursor>,
    visible_start: usize,
    scroll_count: usize,
}

impl Screen {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        assert!(rows > 0, "terminal rows must be non-zero");
        assert!(cols > 0, "terminal cols must be non-zero");
        Self {
            normal: NormalBuf::new(rows, cols),
            in_alt: false,
            cursor_x: 0,
            cursor_y: 0,
            current_fg: Color::Default,
            current_bg: Color::Default,
            current_attrs: CellAttrs::default(),
            alt_request: None,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            cursor_shape: CursorShape::default(),
            cursor_blink: true,
            saved_cursor: None,
            alt_saved: None,
        }
    }

    pub(crate) fn rows(&self) -> usize {
        self.normal.rows()
    }
    pub(crate) fn cols(&self) -> usize {
        self.normal.cols()
    }

    /// Number of saved scrollback rows (for scrollbar rendering).
    pub(crate) fn scroll_count(&self) -> usize {
        self.normal.scroll_count()
    }

    /// View offset from live bottom (for scrollbar rendering).
    pub(crate) fn view_offset(&self) -> usize {
        self.normal.view_offset()
    }

    /// Visible row count (for scrollbar rendering).
    pub(crate) fn visible_rows(&self) -> usize {
        self.normal.rows()
    }

    pub(crate) fn cursor_x(&self) -> usize {
        self.cursor_x
    }

    pub(crate) fn cursor_y(&self) -> usize {
        if self.normal.view_offset() > 0 {
            // Force cursor off-screen when viewing scrollback.
            self.normal.rows()
        } else {
            self.cursor_y
        }
    }

    pub(crate) fn cursor_shape(&self) -> CursorShape {
        self.cursor_shape
    }

    pub(crate) fn cursor_blink(&self) -> bool {
        self.cursor_blink
    }

    pub(crate) fn set_cursor_style(&mut self, ps: usize) {
        let (shape, blink) = match ps {
            0 => (CursorShape::Bar, true),
            1 => (CursorShape::Block, true),
            2 => (CursorShape::Block, false),
            3 => (CursorShape::Underline, true),
            4 => (CursorShape::Underline, false),
            5 => (CursorShape::Bar, true),
            6 => (CursorShape::Bar, false),
            _ => return,
        };
        self.cursor_shape = shape;
        self.cursor_blink = blink;
    }

    pub(crate) fn cells(&self) -> CellsIter<'_> {
        self.normal.cells()
    }

    pub(crate) fn cell_char(&self, row: usize, col: usize) -> char {
        self.normal.cell(row, col).ch
    }

    pub(crate) fn cell(&self, row: usize, col: usize) -> &Cell {
        self.normal.cell(row, col)
    }

    /// Extracts the selected text for the given display-coordinate bounds.
    pub(crate) fn selected_text(&self, bounds: SelectionBounds) -> String {
        let SelectionBounds {
            start_row,
            start_col,
            end_row,
            end_col,
        } = bounds;
        let cols = self.cols();
        let mut buf = String::new();

        for row in start_row..=end_row {
            let col_start = if row == start_row { start_col } else { 0 };
            let col_end = if row == end_row {
                end_col
            } else {
                cols.saturating_sub(1)
            };

            // Build this row's text separately so we can trim trailing
            // whitespace without affecting previous rows or newline separators.
            let row_len_before = buf.len();
            for col in col_start..=col_end {
                let cell = self.cell(row, col);
                if cell.wide_continuation {
                    continue;
                }
                buf.push(cell.ch);
            }
            // Trim trailing whitespace from this row only.
            let row_text = &buf[row_len_before..];
            let trim_len = row_text.trim_end().len();
            buf.truncate(row_len_before + trim_len);
            if row < end_row {
                buf.push('\n');
            }
        }
        buf
    }

    /// Applies SGR (Select Graphic Rendition) parameters to the current pen state.
    ///
    /// `None` parameters are treated as reset (same as `0`). Multi-parameter sub-sequences
    /// (`38`/`48`) are validated fully; on partial or out-of-range params the pen state is
    /// left unchanged.
    pub(crate) fn set_sgr(&mut self, params: &[Option<usize>]) {
        let mut i = 0usize;
        while i < params.len() {
            let n = params[i].unwrap_or_default();
            match n {
                0 => {
                    self.current_fg = Color::Default;
                    self.current_bg = Color::Default;
                    self.current_attrs = CellAttrs::default();
                }
                1 => self.current_attrs.set(CellAttrs::BOLD),
                2 => self.current_attrs.set(CellAttrs::DIM),
                3 => self.current_attrs.set(CellAttrs::ITALIC),
                4 => self.current_attrs.set(CellAttrs::UNDERLINE),
                5 => self.current_attrs.set(CellAttrs::BLINK),
                7 => self.current_attrs.set(CellAttrs::INVERSE),
                9 => self.current_attrs.set(CellAttrs::STRIKETHROUGH),
                22 => self.current_attrs.clear(CellAttrs::BOLD | CellAttrs::DIM),
                23 => self.current_attrs.clear(CellAttrs::ITALIC),
                24 => self.current_attrs.clear(CellAttrs::UNDERLINE),
                25 => self.current_attrs.clear(CellAttrs::BLINK),
                27 => self.current_attrs.clear(CellAttrs::INVERSE),
                29 => self.current_attrs.clear(CellAttrs::STRIKETHROUGH),
                30..=37 => self.current_fg = Color::Named((n - 30) as u8),
                40..=47 => self.current_bg = Color::Named((n - 40) as u8),
                39 => self.current_fg = Color::Default,
                49 => self.current_bg = Color::Default,
                90..=97 => self.current_fg = Color::Bright((n - 90) as u8),
                100..=107 => self.current_bg = Color::Bright((n - 100) as u8),
                38 | 48 => {
                    let is_fg = n == 38;
                    if i + 1 >= params.len() {
                        break;
                    }
                    let sub = params[i + 1].unwrap_or_default();
                    match sub {
                        5 => {
                            // 256-color: 38;5;N  or  48;5;N
                            if i + 2 >= params.len() {
                                break;
                            }
                            if let Some(val) = params[i + 2]
                                && val <= 255
                            {
                                if is_fg {
                                    self.current_fg = Color::Indexed(val as u8);
                                } else {
                                    self.current_bg = Color::Indexed(val as u8);
                                }
                            }
                            i += 2;
                        }
                        2 => {
                            // Truecolor: 38;2;R;G;B  or  48;2;R;G;B
                            if i + 4 >= params.len() {
                                break;
                            }
                            if let (Some(r), Some(g), Some(b)) =
                                (params[i + 2], params[i + 3], params[i + 4])
                                && r <= 255
                                && g <= 255
                                && b <= 255
                            {
                                if is_fg {
                                    self.current_fg = Color::Rgb(r as u8, g as u8, b as u8);
                                } else {
                                    self.current_bg = Color::Rgb(r as u8, g as u8, b as u8);
                                }
                            }
                            i += 4;
                        }
                        _ => {
                            // Unknown sub-type; consume sub only (i+1).
                            i += 1;
                        }
                    }
                }
                _ => { /* unknown SGR code — silently ignore */ }
            }
            i += 1;
        }
    }

    /// Resizes the visible grid while preserving the top-left rectangle of existing cells.
    ///
    /// Resize does not touch parser state. Newly exposed cells are blank, and the cursor is
    /// clamped into the new bounds.
    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        self.normal.resize(rows, cols);
        self.cursor_y = self.cursor_y.min(rows.saturating_sub(1));
        self.cursor_x = self.cursor_x.min(cols.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        if let Some(ref mut saved) = self.saved_cursor {
            saved.cursor_x = saved.cursor_x.min(cols.saturating_sub(1));
            saved.cursor_y = saved.cursor_y.min(rows.saturating_sub(1));
        }
    }

    /// Returns dirty display-row indices (Vec for direct iteration).
    pub(crate) fn dirty_rows(&self) -> Vec<usize> {
        self.normal.dirty_rows()
    }

    /// Resets all dirty flags to false.
    pub(crate) fn clear_dirty(&mut self) {
        self.normal.clear_dirty()
    }

    fn mark_row_dirty(&mut self, row: usize) {
        self.normal.mark_row_dirty(row);
    }

    pub(crate) fn mark_all_dirty(&mut self) {
        self.normal.mark_all_dirty();
    }

    pub(crate) fn request_alt_enter(&mut self) {
        self.alt_request = Some(AltScreenAction::Enter);
    }

    pub(crate) fn request_alt_exit(&mut self) {
        self.alt_request = Some(AltScreenAction::Exit);
    }

    /// Peeks at the pending alt-screen request, if any, without consuming it.
    /// The parser uses this to decide whether to stop processing mid-batch;
    /// the Terminal consumes it later via `take_alt_request`.
    pub(crate) fn alt_request(&self) -> Option<AltScreenAction> {
        self.alt_request
    }

    /// Takes the pending alt-screen request, resetting the field to `None`.
    pub(crate) fn take_alt_request(&mut self) -> Option<AltScreenAction> {
        self.alt_request.take()
    }

    #[cfg(test)]
    pub(crate) fn row_text(&self, row: usize) -> String {
        self.normal.row_text(row)
    }

    /// Direct cell mutation for test setup.
    #[cfg(test)]
    pub(crate) fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        self.normal.cell_mut(row, col)
    }

    // ── semantic cursor / display operations ────────────────────────

    /// Resets `cursor_x` to column 0, implementing the carriage-return (`\r`) semantics.
    pub(crate) fn carriage_return(&mut self) {
        self.cursor_x = 0;
    }

    pub(crate) fn cursor_up(&mut self, n: usize) {
        self.cursor_y = self.cursor_y.saturating_sub(n);
    }

    pub(crate) fn cursor_down(&mut self, n: usize) {
        self.cursor_y = self.cursor_y.saturating_add(n).min(self.normal.rows() - 1);
    }

    pub(crate) fn cursor_left(&mut self, n: usize) {
        self.cursor_x = self.cursor_x.saturating_sub(n);
    }

    pub(crate) fn cursor_right(&mut self, n: usize) {
        self.cursor_x = self.cursor_x.saturating_add(n).min(self.normal.cols() - 1);
    }

    /// Expands horizontal tab into spaces up to the next 8-column tab stop or row end.
    pub(crate) fn horizontal_tab(&mut self) {
        let target = ((self.cursor_x / 8) + 1)
            .saturating_mul(8)
            .min(self.normal.cols());
        let spaces = target.saturating_sub(self.cursor_x);
        for _ in 0..spaces {
            self.write_char(' ');
        }
    }

    /// Writes one already-decoded printable character at the cursor and advances by its terminal
    /// cell width.
    pub(crate) fn write_char(&mut self, ch: char) {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0).min(2);
        if width == 0 {
            return;
        }

        if width == 2 && self.cursor_x + 1 >= self.normal.cols() {
            self.newline();
        }
        self.mark_row_dirty(self.cursor_y);

        let index = self.normal.display_to_ring(self.cursor_y) * self.normal.cols() + self.cursor_x;
        let ring_row = index / self.normal.cols();
        let old_ch = self.normal.live_cell(self.cursor_y, self.cursor_x).ch;

        tracing::trace!(
            ch = format_args!("{ch:?}"),
            cursor_x = self.cursor_x,
            cursor_y = self.cursor_y,
            ring_row,
            ring_cell = index,
            old_char = format_args!("{old_ch:?}"),
            "write_char"
        );

        self.clear_cell_for_write(index);
        if width == 2 && self.cursor_x + 1 < self.normal.cols() {
            self.clear_cell_for_write(index + 1);
        }
        let cell = self.normal.live_cell_mut(self.cursor_y, self.cursor_x);
        cell.set(ch, self.current_fg, self.current_bg, self.current_attrs);
        if width == 2 && self.cursor_x + 1 < self.normal.cols() {
            *self.normal.cell_linear_mut(index + 1) = Cell {
                ch: ' ',
                wide_continuation: true,
                fg: self.current_fg,
                bg: self.current_bg,
                attrs: self.current_attrs,
            };
        }

        self.cursor_x += width;
        if self.cursor_x >= self.normal.cols() {
            self.cursor_x = 0;
            self.newline();
        }
    }

    /// Clears the target cell *and* any joined cell from a double-width glyph that overlaps it.
    ///
    /// Three cases are handled:
    /// 1. The target is a wide-continuation cell → clear both halves (the base at `index - 1`
    ///    and this continuation cell).
    /// 2. The target itself is the start of a double-width glyph that extends into the next
    ///    column → clear both cells.
    /// 3. Otherwise → clear only the target cell.
    fn clear_cell_for_write(&mut self, index: usize) {
        debug_assert!(
            index > 0 || !self.normal.cell_linear(index).wide_continuation,
            "wide_continuation at column 0 is invalid"
        );
        let old_ch = self.normal.cell_linear(index).ch;
        let row = index / self.normal.cols();
        let col = index % self.normal.cols();

        tracing::trace!(
            index,
            row,
            col,
            old_char = format_args!("{old_ch:?}"),
            "clear_cell_for_write"
        );

        if self.normal.cell_linear(index).wide_continuation {
            *self.normal.cell_linear_mut(index - 1) = Cell::default();
            *self.normal.cell_linear_mut(index) = Cell::default();
            return;
        }

        if UnicodeWidthChar::width(self.normal.cell_linear(index).ch).unwrap_or(0) == 2
            && index % self.normal.cols() + 1 < self.normal.cols()
        {
            *self.normal.cell_linear_mut(index + 1) = Cell::default();
        }
        *self.normal.cell_linear_mut(index) = Cell::default();
    }

    /// Moves to the start of the next row, scrolling the registered region when already
    /// at the bottom of the scrolling region. When the cursor is outside the scroll
    /// region, moves down without scrolling.
    pub(crate) fn newline(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y >= self.scroll_top && self.cursor_y <= self.scroll_bottom {
            if self.cursor_y == self.scroll_bottom {
                self.scroll_region_up_one();
            } else {
                self.cursor_y += 1;
            }
        } else if self.cursor_y + 1 < self.normal.rows() {
            self.cursor_y += 1;
        }
    }

    /// VT non-destructive backspace: move cursor left, skipping wide-continuation cells.
    pub(crate) fn backspace(&mut self) {
        if self.cursor_x == 0 {
            return;
        }
        self.cursor_x -= 1;

        // Step past the continuation half of a wide glyph so the cursor
        // lands at the start column rather than sitting mid-glyph.
        let index = self.normal.display_to_ring(self.cursor_y) * self.normal.cols() + self.cursor_x;
        if self.normal.cell_linear(index).wide_continuation && self.cursor_x > 0 {
            self.cursor_x -= 1;
        }
    }

    /// Positions the cursor from 1-based ANSI coordinates, clamped to the visible grid.
    pub(crate) fn set_cursor(&mut self, row_1_based: usize, col_1_based: usize) {
        self.cursor_y = row_1_based.saturating_sub(1).min(self.normal.rows() - 1);
        self.cursor_x = col_1_based.saturating_sub(1).min(self.normal.cols() - 1);
    }

    /// Implements CSI `J` erase-display modes that affect visible cells.
    pub(crate) fn erase_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.mark_row_dirty(self.cursor_y);
                let ring_row = self.normal.display_to_ring(self.cursor_y);
                let start = ring_row * self.normal.cols() + self.cursor_x;
                self.normal
                    .fill_linear_range(start, ring_row * self.normal.cols() + self.normal.cols());
                for row in self.cursor_y + 1..self.normal.rows() {
                    self.mark_row_dirty(row);
                    self.normal.fill_row(row);
                }
            }
            1 => {
                for row in 0..self.cursor_y {
                    self.mark_row_dirty(row);
                    self.normal.fill_row(row);
                }
                self.mark_row_dirty(self.cursor_y);
                let ring_row = self.normal.display_to_ring(self.cursor_y);
                let end = ring_row * self.normal.cols() + self.cursor_x + 1;
                self.normal
                    .fill_linear_range(ring_row * self.normal.cols(), end);
            }
            2 => {
                self.normal.fill_all();
                self.cursor_x = 0;
                self.cursor_y = 0;
                self.mark_all_dirty();
            }
            _ => {}
        }
    }

    /// Implements CSI `K` erase-line modes for the current row.
    pub(crate) fn erase_line(&mut self, mode: usize) {
        let ring_row = self.normal.display_to_ring(self.cursor_y);
        let start = ring_row * self.normal.cols();
        let cursor = start + self.cursor_x;
        let end = ring_row * self.normal.cols() + self.normal.cols();
        self.mark_row_dirty(self.cursor_y);
        match mode {
            0 => self.normal.fill_linear_range(cursor, end),
            1 => self.normal.fill_linear_range(start, cursor + 1),
            2 => self.normal.fill_linear_range(start, end),
            _ => {}
        }
    }

    /// Implements CSI `X` (ECH): erase `n` characters from the cursor rightward.
    ///
    /// Replaces cells with default (space) characters without moving the cursor.
    /// The default parameter (0) acts as 1.
    pub(crate) fn erase_chars(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        self.mark_row_dirty(self.cursor_y);
        let ring_row = self.normal.display_to_ring(self.cursor_y);
        let start = ring_row * self.normal.cols() + self.cursor_x;
        let end = (start + n).min(ring_row * self.normal.cols() + self.normal.cols());
        self.normal.fill_linear_range(start, end);
    }

    /// Clears all visible cells and homes the cursor.
    pub(crate) fn reset_display(&mut self) {
        self.normal.fill_all();
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.mark_all_dirty();
    }

    /// Implements reverse index (`ESC M`): move up, or scroll the scrolling region down
    /// when already at the top of the region. When the cursor is above the region,
    /// moves up if not already at the top of the full screen.
    pub(crate) fn reverse_index(&mut self) {
        tracing::debug!(
            cursor_y = self.cursor_y,
            scroll_top = self.scroll_top,
            scroll_bottom = self.scroll_bottom,
            full_screen = (self.scroll_top == 0 && self.scroll_bottom == self.normal.rows() - 1),
            "reverse_index"
        );

        if self.cursor_y == self.scroll_top && self.cursor_y <= self.scroll_bottom {
            // Mark only region rows dirty.
            for row in self.scroll_top..=self.scroll_bottom {
                self.mark_row_dirty(row);
            }
            if self.scroll_top == 0 && self.scroll_bottom == self.normal.rows() - 1 {
                // Full-screen reverse: use scroll_up_full_screen in reverse direction.
                // Shift down by 1: blank top, everything moves down.
                let tr = self.normal.total_rows();
                let vis = self.normal.visible_start();
                let c = self.normal.cols();
                let src_start = ((vis + self.scroll_top) % tr) * c;
                let src_end = ((vis + self.scroll_bottom) % tr) * c;
                let dst = ((vis + self.scroll_top + 1) % tr) * c;
                if src_start <= src_end {
                    self.normal.copy_linear_range(src_start, src_end, dst);
                } else {
                    let first_part = src_start..tr * c;
                    let second_part = 0..src_end;
                    let first_len = first_part.len();
                    self.normal.copy_cells_within(first_part, dst);
                    self.normal.copy_cells_within(second_part, dst + first_len);
                }
                self.normal.fill_row(self.scroll_top);
            } else {
                // Partial region: ring-aware copy_within down.
                let tr = self.normal.total_rows();
                let vis = self.normal.visible_start();
                let c = self.normal.cols();
                let src_start = ((vis + self.scroll_top) % tr) * c;
                let src_end = ((vis + self.scroll_bottom) % tr) * c;
                let dst = ((vis + self.scroll_top + 1) % tr) * c;
                if src_start <= src_end {
                    self.normal.copy_linear_range(src_start, src_end, dst);
                } else {
                    let first_part = src_start..tr * c;
                    let second_part = 0..src_end;
                    let first_len = first_part.len();
                    self.normal.copy_cells_within(first_part, dst);
                    self.normal.copy_cells_within(second_part, dst + first_len);
                }
                self.normal.fill_row(self.scroll_top);
            }
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
        }
    }

    /// Scrolls the scrolling region up by one row: shifts rows within `[scroll_top, scroll_bottom]`
    /// upward (row N → N-1) and fills the newly exposed bottom row of the region with blank
    /// cells. The cursor moves to column 0 of the bottom region row.
    fn scroll_region_up_one(&mut self) {
        tracing::debug!(
            scroll_top = self.scroll_top,
            scroll_bottom = self.scroll_bottom,
            visible_rows = self.normal.rows(),
            full_screen = (self.scroll_top == 0 && self.scroll_bottom == self.normal.rows() - 1),
            "scroll_region_up_one"
        );

        for row in self.scroll_top..=self.scroll_bottom {
            self.mark_row_dirty(row);
        }
        if self.scroll_top == 0 && self.scroll_bottom == self.normal.rows() - 1 {
            // Full-screen: O(1) ring-buffer advance.
            self.normal.scroll_up_full_screen(1);
        } else {
            // Partial scroll region: ring-aware copy_within.
            let tr = self.normal.total_rows();
            let vis = self.normal.visible_start();
            let c = self.normal.cols();
            let src_start = ((vis + self.scroll_top + 1) % tr) * c;
            let src_end = ((vis + self.scroll_bottom + 1) % tr) * c;
            let dst = ((vis + self.scroll_top) % tr) * c;
            if src_start <= src_end {
                self.normal.copy_linear_range(src_start, src_end, dst);
            } else {
                let first_len = tr * c - src_start;
                let first_part = src_start..tr * c;
                let second_part = 0..src_end;
                self.normal.copy_cells_within(first_part, dst);
                self.normal.copy_cells_within(second_part, dst + first_len);
            }
            self.normal.fill_row(self.scroll_bottom);
        }
        self.cursor_y = self.scroll_bottom;
        self.cursor_x = 0;
    }

    /// Implements DECSTBM (`CSI r`): set scrolling region.
    ///
    /// `top` and `bottom` are 1-based ANSI coordinates. A value of 0 means "use default"
    /// (top=1, bottom=rows). If `top >= bottom` after clamping, the request is ignored.
    /// Cursor is moved to home (0, 0) on success.
    pub(crate) fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let top = if top == 0 { 1 } else { top };
        let bottom = if bottom == 0 {
            self.normal.rows()
        } else {
            bottom
        };
        let top = top.max(1).min(self.normal.rows());
        let bottom = bottom.min(self.normal.rows());
        if top >= bottom {
            return;
        }
        self.scroll_top = top - 1;
        self.scroll_bottom = bottom - 1;
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Implements CSI `@` (ICH): insert `n` blank characters at the cursor, shifting
    /// existing characters right. Characters past the right margin are lost.
    /// The cursor position does not change.
    pub(crate) fn insert_chars(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        self.mark_row_dirty(self.cursor_y);
        let col = self.cursor_x;
        if col >= self.normal.cols() {
            return;
        }
        let n = n.min(self.normal.cols() - col);
        if n == 0 {
            return;
        }
        let ring_row = self.normal.display_to_ring(self.cursor_y);
        let row_start = ring_row * self.normal.cols();
        // Shift cells in [col, cols - n) right by n.
        let src_start = row_start + col;
        let src_end = row_start + self.normal.cols() - n;
        let dst = row_start + col + n;
        self.normal.copy_linear_range(src_start, src_end, dst);
        // Fill vacated cells with blanks.
        self.normal
            .fill_linear_range(row_start + col, row_start + col + n);
    }

    /// Implements CSI `P` (DCH): delete `n` characters at the cursor, shifting remaining
    /// characters left. Vacated cells at the right margin become blank.
    /// The cursor position does not change.
    pub(crate) fn delete_chars(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        self.mark_row_dirty(self.cursor_y);
        let col = self.cursor_x;
        if col >= self.normal.cols() {
            return;
        }
        let n = n.min(self.normal.cols() - col);
        if n == 0 {
            return;
        }
        let ring_row = self.normal.display_to_ring(self.cursor_y);
        let start = ring_row * self.normal.cols() + col + n;
        let end = ring_row * self.normal.cols() + self.normal.cols();
        let dst = ring_row * self.normal.cols() + col;
        self.normal.copy_linear_range(start, end, dst);
        let blank_start = ring_row * self.normal.cols() + self.normal.cols() - n;
        self.normal.fill_linear_range(blank_start, blank_start + n);
    }
    pub(crate) fn insert_lines(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        if self.cursor_y < self.scroll_top || self.cursor_y > self.scroll_bottom {
            return;
        }
        let max_n = self.scroll_bottom - self.cursor_y + 1;
        let n = n.min(max_n);
        // Mark all affected rows dirty.
        for row in self.cursor_y..=self.scroll_bottom {
            self.mark_row_dirty(row);
        }
        // When n covers all remaining rows in the region, just blank them all.
        if n == max_n {
            for row in self.cursor_y..=self.scroll_bottom {
                self.normal.fill_row(row);
            }
            self.cursor_x = 0;
            return;
        }
        // Shift rows [cursor_y .. scroll_bottom - n + 1] down by n (ring-aware).
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.cursor_y) % tr) * c;
        let src_end = ((vis + self.scroll_bottom - n + 1) % tr) * c;
        let dst = ((vis + self.cursor_y + n) % tr) * c;
        if src_start <= src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        } else {
            let first_len = tr * c - src_start;
            self.normal.copy_linear_range(src_start, tr * c, dst);
            self.normal.copy_linear_range(0, src_end, dst + first_len);
        }
        for i in 0..n {
            self.normal.fill_row(self.cursor_y + i);
        }
        self.cursor_x = 0;
    }

    pub(crate) fn delete_lines(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        if self.cursor_y < self.scroll_top || self.cursor_y > self.scroll_bottom {
            return;
        }
        let max_n = self.scroll_bottom - self.cursor_y + 1;
        let n = n.min(max_n);
        // Mark all affected rows dirty.
        for row in self.cursor_y..=self.scroll_bottom {
            self.mark_row_dirty(row);
        }
        // When n covers all remaining rows in the region, just blank them all.
        if n == max_n {
            for row in self.cursor_y..=self.scroll_bottom {
                self.normal.fill_row(row);
            }
            self.cursor_x = 0;
            return;
        }
        // Shift rows [cursor_y + n ..= scroll_bottom] up by n (ring-aware).
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.cursor_y + n) % tr) * c;
        let src_end = ((vis + self.scroll_bottom + 1) % tr) * c;
        let dst = ((vis + self.cursor_y) % tr) * c;
        if src_start <= src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        } else {
            let first_len = tr * c - src_start;
            let first_part = src_start..tr * c;
            let second_part = 0..src_end;
            self.normal.copy_cells_within(first_part, dst);
            self.normal.copy_cells_within(second_part, dst + first_len);
        }
        for i in 0..n {
            self.normal.fill_row(self.scroll_bottom - i);
        }
        self.cursor_x = 0;
    }

    /// Implements CSI `S` (SU): scroll the scrolling region up by `n` lines.
    /// Top `n` lines of the region are lost; blank lines appear at the bottom.
    /// The cursor does not move.
    pub(crate) fn scroll_up_region(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        let region_height = self.scroll_bottom - self.scroll_top + 1;
        let n = n.min(region_height);
        // Mark only region rows dirty.
        for row in self.scroll_top..=self.scroll_bottom {
            self.mark_row_dirty(row);
        }
        // When n covers the entire region, just blank everything.
        if n == region_height {
            for row in self.scroll_top..=self.scroll_bottom {
                self.normal.fill_row(row);
            }
            return;
        }
        // Shift rows [scroll_top + n ..= scroll_bottom] up by n.
        // Full-screen case handled via scroll_up_full_screen; here handle partial.
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.scroll_top + n) % tr) * c;
        let src_end = ((vis + self.scroll_bottom + 1) % tr) * c;
        let dst = ((vis + self.scroll_top) % tr) * c;
        // Handle wrap-around for copy_within.
        if src_start <= src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        } else {
            let first_part = ((vis + self.scroll_top + n) % tr) * c..tr * c;
            let second_part = 0..src_end;
            let first_len = first_part.len();
            self.normal.copy_cells_within(first_part, dst);
            self.normal.copy_cells_within(second_part, dst + first_len);
        }
        for i in 0..n {
            self.normal.fill_row(self.scroll_bottom - i);
        }
    }

    /// Implements CSI `T` (SD): scroll the scrolling region down by `n` lines.
    /// Bottom `n` lines of the region are lost; blank lines appear at the top.
    /// The cursor does not move.
    pub(crate) fn scroll_down_region(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        let region_height = self.scroll_bottom - self.scroll_top + 1;
        let n = n.min(region_height);
        // Mark only region rows dirty.
        for row in self.scroll_top..=self.scroll_bottom {
            self.mark_row_dirty(row);
        }
        // When n covers the entire region, just blank everything.
        if n == region_height {
            for row in self.scroll_top..=self.scroll_bottom {
                self.normal.fill_row(row);
            }
            return;
        }
        // Shift rows [scroll_top ..= scroll_bottom - n] down by n.
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.scroll_top) % tr) * c;
        let src_end = ((vis + self.scroll_bottom - n + 1) % tr) * c;
        let dst = ((vis + self.scroll_top + n) % tr) * c;
        if src_start <= src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        } else {
            let first_len = tr * c - src_start;
            let first_part = src_start..tr * c;
            let second_part = 0..src_end;
            self.normal.copy_cells_within(first_part, dst);
            self.normal.copy_cells_within(second_part, dst + first_len);
        }
        for i in 0..n {
            self.normal.fill_row(self.scroll_top + i);
        }
    }

    /// Implements DECSC (`ESC 7`): save cursor position and SGR attributes.
    pub(crate) fn save_cursor(&mut self) {
        self.saved_cursor = Some(SavedCursor {
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
            fg: self.current_fg,
            bg: self.current_bg,
            attrs: self.current_attrs,
        });
    }

    /// Implements DECRC (`ESC 8`): restore cursor position and SGR attributes.
    /// If no cursor was previously saved, this is a no-op.
    pub(crate) fn restore_cursor(&mut self) {
        if let Some(saved) = &self.saved_cursor {
            self.cursor_x = saved.cursor_x;
            self.cursor_y = saved.cursor_y;
            self.current_fg = saved.fg;
            self.current_bg = saved.bg;
            self.current_attrs = saved.attrs;
        }
    }

    // ── viewport scroll & alt delegation ────────────────────────────

    pub(crate) fn scroll_up(&mut self, n: usize) {
        self.normal.scroll_up(n);
    }

    pub(crate) fn scroll_down(&mut self, n: usize) {
        self.normal.scroll_down(n);
    }

    pub(crate) fn scroll_to_bottom(&mut self) {
        self.normal.scroll_to_bottom();
    }

    pub(crate) fn is_alt(&self) -> bool {
        self.in_alt
    }

    pub(crate) fn enter_alt(&mut self) {
        if self.in_alt {
            return;
        }
        let state = AltSavedState {
            cells: self.normal.take_cells(),
            dirty_rows: self.normal.take_dirty_rows(),
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
            current_fg: self.current_fg,
            current_bg: self.current_bg,
            current_attrs: self.current_attrs,
            scroll_top: self.scroll_top,
            scroll_bottom: self.scroll_bottom,
            cursor_shape: self.cursor_shape,
            cursor_blink: self.cursor_blink,
            saved_cursor: self.saved_cursor.take(),
            visible_start: self.normal.visible_start(),
            scroll_count: self.normal.scroll_count(),
        };
        self.normal.init_alt_buffer();
        self.alt_saved = Some(state);
        self.in_alt = true;
    }

    pub(crate) fn exit_alt(&mut self) {
        if let Some(state) = self.alt_saved.take() {
            self.normal.restore_cells(state.cells);
            self.normal.restore_dirty_rows(state.dirty_rows);
            self.normal.set_visible_start(state.visible_start);
            self.normal.set_scroll_count(state.scroll_count);
            self.cursor_x = state.cursor_x;
            self.cursor_y = state.cursor_y;
            self.current_fg = state.current_fg;
            self.current_bg = state.current_bg;
            self.current_attrs = state.current_attrs;
            self.scroll_top = state.scroll_top;
            self.scroll_bottom = state.scroll_bottom;
            self.cursor_shape = state.cursor_shape;
            self.cursor_blink = state.cursor_blink;
            self.saved_cursor = state.saved_cursor;
        }
        self.in_alt = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_char_marks_its_row() {
        let mut screen = Screen::new(3, 4);
        // After new, all rows are dirty.
        screen.clear_dirty();
        screen.write_char('x');
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(dirty, vec![0], "write_char at row 0 should mark row 0");
    }

    #[test]
    fn clear_dirty_resets_all() {
        let mut screen = Screen::new(2, 4);
        // All rows initially dirty.
        assert_eq!(screen.dirty_rows().len(), 2);
        screen.clear_dirty();
        assert_eq!(screen.dirty_rows().len(), 0);
    }

    #[test]
    fn scroll_up_marks_all_rows() {
        let mut screen = Screen::new(3, 4);
        // Fill all rows, then scroll up via newline on bottom row.
        screen.clear_dirty();
        screen.cursor_y = 2; // bottom row
        screen.newline(); // triggers scroll_up
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(dirty.len(), 3, "scroll_up should mark all rows");
    }

    #[test]
    fn erase_line_marks_cursor_row() {
        let mut screen = Screen::new(4, 4);
        screen.cursor_y = 2;
        screen.clear_dirty();
        screen.erase_line(2);
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(dirty, vec![2], "erase_line should mark only cursor row");
    }

    #[test]
    fn cursor_movement_does_not_mark_dirty() {
        let mut screen = Screen::new(3, 4);
        screen.clear_dirty();
        screen.cursor_up(1);
        screen.cursor_down(1);
        screen.cursor_left(1);
        screen.cursor_right(1);
        screen.carriage_return();
        screen.set_cursor(2, 2);
        assert_eq!(
            screen.dirty_rows().len(),
            0,
            "cursor-only ops should not mark rows dirty"
        );
    }

    #[test]
    fn erase_display_mode_0_marks_from_cursor_to_end() {
        let mut screen = Screen::new(5, 4);
        screen.cursor_y = 2;
        screen.clear_dirty();
        screen.erase_display(0);
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(
            dirty,
            vec![2, 3, 4],
            "erase_display(0) should mark rows cursor..end"
        );
    }

    #[test]
    fn erase_display_mode_1_marks_from_start_to_cursor() {
        let mut screen = Screen::new(5, 4);
        screen.cursor_y = 2;
        screen.clear_dirty();
        screen.erase_display(1);
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(
            dirty,
            vec![0, 1, 2],
            "erase_display(1) should mark rows 0..=cursor"
        );
    }

    #[test]
    fn erase_display_mode_2_marks_all() {
        let mut screen = Screen::new(5, 4);
        screen.clear_dirty();
        screen.erase_display(2);
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(dirty.len(), 5, "erase_display(2) should mark all rows");
    }

    #[test]
    fn reset_display_marks_all_rows() {
        let mut screen = Screen::new(3, 4);
        screen.clear_dirty();
        screen.reset_display();
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(dirty.len(), 3, "reset_display should mark all rows");
    }

    #[test]
    fn reverse_index_scroll_marks_all_rows() {
        let mut screen = Screen::new(3, 4);
        screen.clear_dirty();
        screen.cursor_y = 0;
        screen.reverse_index(); // triggers scroll
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(
            dirty.len(),
            3,
            "reverse_index with scroll should mark all rows"
        );
    }

    #[test]
    fn backspace_does_not_mark_dirty() {
        let mut screen = Screen::new(1, 4);
        screen.cursor_x = 2;
        screen.clear_dirty();
        screen.backspace();
        assert_eq!(
            screen.dirty_rows().len(),
            0,
            "backspace should not mark rows dirty"
        );
    }

    #[test]
    fn newline_no_scroll_does_not_mark_dirty() {
        let mut screen = Screen::new(3, 4);
        screen.cursor_y = 1;
        screen.clear_dirty();
        screen.newline();
        assert_eq!(
            screen.dirty_rows().len(),
            0,
            "newline without scroll should not mark dirty"
        );
    }

    #[test]
    fn resize_rebuilds_dirty_all_true() {
        let mut screen = Screen::new(2, 4);
        screen.clear_dirty();
        screen.resize(4, 4);
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(
            dirty.len(),
            4,
            "resize should rebuild dirty_rows with all true"
        );
    }
    #[test]
    fn erase_chars_clears_from_cursor_to_right() {
        let mut screen = Screen::new(1, 14);
        // Write "hello world!" without wrapping.
        screen.write_char('h');
        screen.write_char('e');
        screen.write_char('l');
        screen.write_char('l');
        screen.write_char('o');
        screen.write_char(' ');
        screen.write_char('w');
        screen.write_char('o');
        screen.write_char('r');
        screen.write_char('l');
        screen.write_char('d');
        screen.write_char('!');
        assert_eq!(screen.row_text(0).trim_end(), "hello world!");

        // Move cursor back to column 5 (at ' ') and erase 7 chars.
        screen.cursor_x = 5;
        screen.clear_dirty();
        screen.erase_chars(7);
        assert_eq!(screen.row_text(0).trim_end(), "hello");
        assert_eq!(screen.dirty_rows().into_iter().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn erase_chars_clamps_to_row_end() {
        let mut screen = Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        assert_eq!(screen.row_text(0), "ab  ");

        screen.cursor_x = 2;
        screen.erase_chars(10); // more than remaining cols
        assert_eq!(screen.row_text(0), "ab  ");
    }

    #[test]
    fn erase_chars_zero_acts_as_one() {
        let mut screen = Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        assert_eq!(screen.row_text(0), "ab  ");

        screen.cursor_x = 1;
        screen.erase_chars(0);
        assert_eq!(screen.row_text(0), "a   ");
    }

    // ── ICH / DCH ──────────────────────────────────────────────

    #[test]
    fn insert_chars_shifts_right() {
        let mut screen = Screen::new(1, 8);
        for ch in "abcdef".chars() {
            screen.write_char(ch);
        }
        assert_eq!(screen.row_text(0), "abcdef  ");

        screen.cursor_x = 2;
        screen.clear_dirty();
        screen.insert_chars(2);
        assert_eq!(screen.row_text(0), "ab  cdef");
        assert!(screen.dirty_rows().into_iter().any(|r| r == 0));
    }

    #[test]
    fn insert_chars_zero_acts_as_one() {
        let mut screen = Screen::new(1, 6);
        for ch in "abcde".chars() {
            screen.write_char(ch);
        }
        assert_eq!(screen.row_text(0), "abcde ");

        screen.cursor_x = 1;
        screen.insert_chars(0);
        assert_eq!(screen.row_text(0), "a bcde");
    }

    #[test]
    fn insert_chars_clamps_to_row_end() {
        let mut screen = Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        assert_eq!(screen.row_text(0), "ab  ");

        screen.cursor_x = 2;
        screen.insert_chars(10);
        assert_eq!(screen.row_text(0), "ab  ");
    }

    #[test]
    fn delete_chars_shifts_left() {
        let mut screen = Screen::new(1, 8);
        for ch in "abcdef".chars() {
            screen.write_char(ch);
        }
        assert_eq!(screen.row_text(0), "abcdef  ");

        screen.cursor_x = 2;
        screen.clear_dirty();
        screen.delete_chars(2);
        assert_eq!(screen.row_text(0), "abef    ");
        assert!(screen.dirty_rows().into_iter().any(|r| r == 0));
    }

    #[test]
    fn delete_chars_zero_acts_as_one() {
        let mut screen = Screen::new(1, 5);
        for ch in "abcd".chars() {
            screen.write_char(ch);
        }
        screen.cursor_x = 1;
        screen.delete_chars(0);
        assert_eq!(screen.row_text(0), "acd  ");
    }

    #[test]
    fn delete_chars_clamps_to_row_end() {
        let mut screen = Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        assert_eq!(screen.row_text(0), "ab  ");

        screen.cursor_x = 2;
        screen.delete_chars(10);
        assert_eq!(screen.row_text(0), "ab  ");
    }

    // ── IL / DL ─────────────────────────────────────────────────

    #[test]
    fn insert_lines_within_region() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");
        assert_eq!(screen.row_text(3), "dddd");

        screen.cursor_y = 1;
        screen.insert_lines(1);
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "bbbb");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn insert_lines_outside_region_noop() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        screen.cursor_y = 0; // above scroll_top
        screen.insert_lines(1);
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn delete_lines_within_region() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");

        screen.cursor_y = 1;
        screen.delete_lines(1);
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "cccc");
        assert_eq!(screen.row_text(2), "    ");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn delete_lines_outside_region_noop() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        screen.cursor_y = 3; // below scroll_bottom
        screen.delete_lines(1);
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");
    }

    // ── SU / SD ─────────────────────────────────────────────────

    #[test]
    fn scroll_up_region_scrolls() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 0;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");
        assert_eq!(screen.row_text(3), "dddd");

        screen.scroll_up_region(2);
        assert_eq!(screen.row_text(0), "cccc");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "    ");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn scroll_down_region_scrolls() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 0;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }

        screen.scroll_down_region(2);
        assert_eq!(screen.row_text(0), "    ");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "aaaa");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn scroll_up_region_clamps_n() {
        let mut screen = Screen::new(3, 4);
        for row in 0..3 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        screen.scroll_up_region(100);
        assert_eq!(screen.row_text(0), "    ");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "    ");
    }

    // ── DECSTBM ─────────────────────────────────────────────────

    #[test]
    fn set_scroll_region_default() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 2;
        screen.scroll_bottom = 3;
        screen.set_scroll_region(0, 0);
        assert_eq!(screen.scroll_top, 0);
        assert_eq!(screen.scroll_bottom, 3);
        // Cursor homes on success.
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
    }

    #[test]
    fn set_scroll_region_custom() {
        let mut screen = Screen::new(4, 4);
        screen.cursor_x = 3;
        screen.cursor_y = 3;
        screen.set_scroll_region(2, 3);
        assert_eq!(screen.scroll_top, 1);
        assert_eq!(screen.scroll_bottom, 2);
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
    }

    #[test]
    fn set_scroll_region_invalid_ignored() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 0;
        screen.scroll_bottom = 3;
        screen.set_scroll_region(3, 2); // top >= bottom after clamping
        assert_eq!(screen.scroll_top, 0);
        assert_eq!(screen.scroll_bottom, 3);
    }

    // ── Cursor save/restore ──────────────────────────────────────

    #[test]
    fn save_restore_cursor_roundtrips() {
        let mut screen = Screen::new(4, 4);
        screen.save_cursor();
        screen.cursor_x = 2;
        screen.cursor_y = 3;
        screen.current_fg = Color::Named(1);
        screen.current_bg = Color::Named(2);
        screen.current_attrs.set(CellAttrs::BOLD);
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
        assert_eq!(screen.current_fg, Color::Default);
        assert_eq!(screen.current_bg, Color::Default);
        assert_eq!(screen.current_attrs, CellAttrs::default());
    }

    #[test]
    fn restore_cursor_none_is_noop() {
        let mut screen = Screen::new(4, 4);
        screen.cursor_x = 2;
        screen.cursor_y = 3;
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 2);
        assert_eq!(screen.cursor_y, 3);
    }

    // ── Region-aware newline / reverse_index ────────────────────

    #[test]
    fn newline_scrolls_region() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        // Cursor at scroll_bottom, call newline.
        screen.cursor_y = 2;
        screen.cursor_x = 0;
        screen.clear_dirty();
        screen.newline();
        // Region [1,2] scrolled up: row 1 gets old row 2 ("cccc"), row 2 blanked.
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "cccc");
        assert_eq!(screen.row_text(2), "    ");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn reverse_index_scrolls_region() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        // Cursor at scroll_top, call reverse_index.
        screen.cursor_y = 1;
        screen.clear_dirty();
        screen.reverse_index();
        // Region [1,2] scrolled down: row 1 blank, row 2 gets old row 1 ("bbbb").
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "bbbb");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn reverse_index_above_region_no_panic() {
        let mut screen = Screen::new(5, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 3;
        // Cursor at (0, 0) — above the scroll region.
        screen.reverse_index(); // must NOT panic
        // No rows or cursor should have changed (no-op above region).
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
        assert_eq!(screen.row_text(0), "    ");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "    ");
        assert_eq!(screen.row_text(3), "    ");
        assert_eq!(screen.row_text(4), "    ");
    }

    #[test]
    fn newline_below_region_no_panic() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        screen.cursor_y = 3; // below region at last row
        screen.cursor_x = 0;
        screen.newline(); // must NOT panic
        // Cursor should not have wrapped past rows-1.
        assert!(screen.cursor_y < screen.rows());
        assert_eq!(screen.cursor_y, 3);
        assert_eq!(screen.cursor_x, 0);
    }

    #[test]
    fn resize_preserves_saved_cursor() {
        // Save at home, resize smaller, restore → clamped to new bounds.
        let mut screen = Screen::new(5, 10);
        screen.save_cursor();
        // Move cursor away and set SGR.
        screen.cursor_y = 4;
        screen.cursor_x = 7;
        screen.current_fg = Color::Named(1);
        screen.current_bg = Color::Named(2);
        screen.current_attrs.set(CellAttrs::BOLD);
        screen.resize(3, 5); // smaller — saved cursor must be clamped
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 0, "saved x clamped to 0.min(4)");
        assert_eq!(screen.cursor_y, 0, "saved y clamped to 0.min(2)");
        assert_eq!(screen.current_fg, Color::Default);
        assert_eq!(screen.current_bg, Color::Default);
        assert_eq!(screen.current_attrs, CellAttrs::default());

        // Save at non-home, resize larger, restore → original position preserved.
        let mut screen = Screen::new(2, 5);
        screen.cursor_y = 1;
        screen.cursor_x = 3;
        screen.save_cursor();
        screen.resize(10, 20); // larger — no clamping needed
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 3, "original x preserved");
        assert_eq!(screen.cursor_y, 1, "original y preserved");
    }

    // ── selected_text ──────────────────────────────────────────────

    #[test]
    fn selected_text_single_row() {
        let mut screen = Screen::new(3, 11);
        // Fill row 1 with "hello world" (some chars repeated to fill)
        let text: Vec<char> = "hello world".chars().collect();
        for (col, ch) in text.iter().enumerate() {
            screen.cell_mut(1, col).ch = *ch;
        }
        let result = screen.selected_text(SelectionBounds {
            start_row: 1,
            start_col: 0,
            end_row: 1,
            end_col: 10,
        });
        assert_eq!(result, "hello world");
    }

    #[test]
    fn selected_text_multi_row() {
        let mut screen = Screen::new(3, 4);
        let rows = ["ab", "cd", "ef"];
        for (r, line) in rows.iter().enumerate() {
            for (c, ch) in line.chars().enumerate() {
                screen.cell_mut(r, c).ch = ch;
            }
        }
        // Select rows 0-1, full row 0 and partial row 1 (only col 0)
        let result = screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 0,
        });
        assert_eq!(result, "ab\nc");
    }

    #[test]
    fn selected_text_skips_wide_continuation() {
        let mut screen = Screen::new(1, 4);
        // Simulate a double-width character at col 0: set ch at 0, continuation at 1.
        screen.cell_mut(0, 0).ch = 'A';
        screen.cell_mut(0, 1).wide_continuation = true;
        screen.cell_mut(0, 2).ch = 'B';
        screen.cell_mut(0, 3).ch = 'C';
        let result = screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 3,
        });
        assert_eq!(result, "ABC");
    }

    #[test]
    fn selected_text_trims_trailing_whitespace() {
        let mut screen = Screen::new(2, 5);
        screen.cell_mut(0, 0).ch = 'a';
        screen.cell_mut(0, 1).ch = ' ';
        screen.cell_mut(0, 2).ch = ' ';
        screen.cell_mut(1, 0).ch = 'b';
        let result = screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        });
        assert_eq!(result, "a\nb");
    }

    #[test]
    fn selected_text_empty_selection() {
        let screen = Screen::new(3, 10);
        let result = screen.selected_text(SelectionBounds {
            start_row: 1,
            start_col: 2,
            end_row: 1,
            end_col: 1,
        });
        assert_eq!(result, "");
    }
}
