use super::NormalBuf;
use super::normal_buf::CellsIter;
use super::parser::params::Params;

use unicode_width::UnicodeWidthChar;

#[cfg(test)]
mod tests;

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
    pub(crate) wide_continuation: bool,
    /// Foreground color.
    pub(crate) fg: Color,
    /// Background color.
    pub(crate) bg: Color,
    /// Text style attributes.
    pub(crate) attrs: CellAttrs,
    /// True if this character is protected against selective erasure (DECSCA).
    pub(crate) protected: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            wide_continuation: false,
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
            protected: false,
        }
    }
}

impl Cell {
    /// Sets all fields atomically (ensures no field is forgotten on add).
    pub(crate) fn set(
        &mut self,
        ch: char,
        fg: Color,
        bg: Color,
        attrs: CellAttrs,
        protected: bool,
    ) {
        self.ch = ch;
        self.wide_continuation = false;
        self.fg = fg;
        self.bg = bg;
        self.attrs = attrs;
        self.protected = protected;
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
    origin_mode: bool,
    autowrap: bool,
    pending_wrap: bool,
}

/// Cursor position, appearance, and saved-state (DECSC/DECRC).
///
/// All coordinates are 0-based. The saved cursor snapshot persists until
/// overwritten by a subsequent DECSC, a reset, or alt-screen entry.
#[derive(Debug, Clone)]
struct CursorState {
    /// 0-based column.
    x: usize,
    /// 0-based row.
    y: usize,
    /// Current cursor shape (DECSCUSR).
    shape: CursorShape,
    /// Whether the cursor blinks (DECSCUSR).
    blink: bool,
    /// Whether the cursor is visible (DECTCEM).
    visible: bool,
    /// Saved cursor snapshot from DECSC, or `None` before any save.
    saved: Option<SavedCursor>,
}

impl CursorState {
    fn new() -> Self {
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

/// Current SGR pen state — the active foreground, background, attributes,
/// and protection flag applied to each newly written character.
#[derive(Debug, Clone, Copy)]
struct Pen {
    /// Foreground color (SGR 30–39, 90–97, 38).
    fg: Color,
    /// Background color (SGR 40–49, 100–107, 48).
    bg: Color,
    /// Active text attributes (bold, italic, underline, etc.).
    attrs: CellAttrs,
    /// Whether newly written cells are protected (DECSCA).
    protected: bool,
}

impl Pen {
    fn reset() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
            protected: false,
        }
    }
}

/// Vertical scrolling region (DECSTBM).  Both boundaries are 0-based and
/// inclusive: `top=0, bottom=rows-1` covers the full screen.
#[derive(Debug, Clone, Copy)]
struct ScrollRegion {
    /// Top boundary of the scrolling region, inclusive.
    top: usize,
    /// Bottom boundary of the scrolling region, inclusive.
    bottom: usize,
}

impl ScrollRegion {
    fn full(rows: usize) -> Self {
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
struct Margins {
    /// Whether DECLRMM is active.
    enabled: bool,
    /// Left margin column, inclusive.
    left: usize,
    /// Right margin column, inclusive.
    right: usize,
}

impl Margins {
    fn full(cols: usize) -> Self {
        Self {
            enabled: false,
            left: 0,
            right: cols.saturating_sub(1),
        }
    }

    /// Clamps both margin boundaries into `[0, cols-1]` without reordering.
    fn clamp(&mut self, cols: usize) {
        let rightmost = cols.saturating_sub(1);
        self.left = self.left.min(rightmost);
        self.right = self.right.min(rightmost);
    }
}

/// Set of binary terminal modes — each maps to a DEC private or standard
/// mode whose state is stored directly in the screen.
#[derive(Debug, Clone, Copy)]
struct TerminalModes {
    /// DECAWM: autowrap at the right margin.
    autowrap: bool,
    /// Internal flag: true when the cursor reached the right margin and the
    /// next printable character should wrap before printing.
    pending_wrap: bool,
    /// DECOM: cursor positioning is relative to the scrolling region.
    origin: bool,
    /// IRM (standard mode 4): insert characters instead of overwriting.
    insert: bool,
    /// LNM (standard mode 20): line feed also performs a carriage return.
    line_feed: bool,
    /// DECCKM: application cursor keys send SS3-style sequences.
    application_cursor: bool,
    /// DECKPAM: application keypad sends SS3-style sequences.
    application_keypad: bool,
}

impl TerminalModes {
    fn default() -> Self {
        Self {
            autowrap: true,
            pending_wrap: false,
            origin: false,
            insert: false,
            line_feed: false,
            application_cursor: false,
            application_keypad: false,
        }
    }
}

/// Horizontal tab stops.  `true` at column `c` means a tab stop is set.
/// Default stops are at every 8th column.
#[derive(Debug, Clone)]
struct TabStops(Vec<bool>);

impl TabStops {
    fn new(cols: usize) -> Self {
        let mut stops = vec![false; cols];
        for (col, stop) in stops.iter_mut().enumerate() {
            if col % 8 == 0 {
                *stop = true;
            }
        }
        Self(stops)
    }

    fn resize(&mut self, cols: usize) {
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
struct CharacterSets {
    /// Most recently printed character (used by REP / CSI Ps b).
    last_char: Option<char>,
    /// G0 character set designation.
    g0: u8,
    /// G1 character set designation.
    g1: u8,
    /// Active charset: 0 = G0, 1 = G1.
    active: u8,
}

impl CharacterSets {
    fn default() -> Self {
        Self {
            last_char: None,
            g0: b'B',
            g1: b'B',
            active: 0,
        }
    }

    fn reset(&mut self) {
        self.last_char = None;
        self.g0 = b'B';
        self.g1 = b'B';
        self.active = 0;
    }
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
    /// Cursor position, appearance, and saved-state (DECSC/DECRC).
    cursor: CursorState,
    /// Current SGR pen — foreground, background, attributes, protection.
    pen: Pen,
    /// Pending alt-screen request set by the parser, consumed by Terminal.
    alt_request: Option<AltScreenAction>,
    /// Vertical scrolling region (DECSTBM).
    scroll_region: ScrollRegion,
    /// Saved normal-screen state while the alternate screen is active.
    alt_saved: Option<Box<Screen>>,
    /// Horizontal left/right margins (DECLRMM).
    margins: Margins,
    /// Horizontal tab stops (every 8 columns by default).
    tab_stops: TabStops,
    /// Binary terminal modes (DECAWM, DECOM, IRM, LNM, DECCKM, DECKPAM, …).
    modes: TerminalModes,
    /// Character set designations (G0, G1) and last printed character.
    charsets: CharacterSets,
}

struct Rect {
    top: usize,
    left: usize,
    bottom: usize,
    right: usize,
}

impl Screen {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        assert!(rows > 0, "terminal rows must be non-zero");
        assert!(cols > 0, "terminal cols must be non-zero");
        Self {
            normal: NormalBuf::new(rows, cols),
            in_alt: false,
            cursor: CursorState::new(),
            pen: Pen::reset(),
            alt_request: None,
            scroll_region: ScrollRegion::full(rows),
            alt_saved: None,
            margins: Margins::full(cols),
            tab_stops: TabStops::new(cols),
            modes: TerminalModes::default(),
            charsets: CharacterSets::default(),
        }
    }

    pub(crate) fn rows(&self) -> usize {
        self.normal.rows()
    }
    pub(crate) fn cols(&self) -> usize {
        self.normal.cols()
    }
    pub(crate) fn scroll_count(&self) -> usize {
        self.normal.scroll_count()
    }
    pub(crate) fn view_offset(&self) -> usize {
        self.normal.view_offset()
    }
    pub(crate) fn visible_rows(&self) -> usize {
        self.normal.rows()
    }
    pub(crate) fn cursor_x(&self) -> usize {
        self.cursor.x
    }
    pub(crate) fn cursor_y(&self) -> usize {
        if self.normal.view_offset() > 0 {
            // Force cursor off-screen when viewing scrollback.
            self.normal.rows()
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
    pub(crate) fn set_cursor_style(&mut self, ps: usize) {
        let (shape, blink) = match ps {
            0 => (CursorShape::Bar, true),
            1 => (CursorShape::Block, true),
            2 => (CursorShape::Block, false),
            3 => (CursorShape::Underline, true),
            4 => (CursorShape::Underline, false),
            5 => (CursorShape::Bar, true),
            6 => (CursorShape::Bar, false),
            _ => (CursorShape::default(), true),
        };
        self.cursor.shape = shape;
        self.cursor.blink = blink;
    }

    pub(crate) fn application_cursor(&self) -> bool {
        self.modes.application_cursor
    }

    pub(crate) fn application_keypad(&self) -> bool {
        self.modes.application_keypad
    }

    pub(crate) fn margin_mode(&self) -> bool {
        self.margins.enabled
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
    pub(crate) fn set_sgr(&mut self, params: &Params) {
        let mut i = 0usize;
        while i < params.len {
            let p = params.get_param(i).expect("index is bounded by params.len");
            let n = p.get(0).unwrap_or_default();
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
                    // Check if this parameter has colon sub-parameters
                    if p.len > 1 {
                        let sub = p.get(1).unwrap_or_default();
                        match sub {
                            5 => {
                                if let Some(val) = p.get(2)
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
                                let (r_idx, g_idx, b_idx) =
                                    if p.len >= 6 { (3, 4, 5) } else { (2, 3, 4) };
                                if let (Some(r), Some(g), Some(b)) =
                                    (p.get(r_idx), p.get(g_idx), p.get(b_idx))
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
                            }
                            _ => {}
                        }
                    } else {
                        // Semicolon fallback: read subsequent parameters
                        if i + 1 >= params.len {
                            break;
                        }
                        let next_p = &params.values[i + 1];
                        let sub = next_p.get(0).unwrap_or_default();
                        match sub {
                            5 => {
                                if i + 2 >= params.len {
                                    break;
                                }
                                if let Some(val) = params.values[i + 2].get(0)
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
                                if i + 4 >= params.len {
                                    break;
                                }
                                if let (Some(r), Some(g), Some(b)) = (
                                    params.values[i + 2].get(0),
                                    params.values[i + 3].get(0),
                                    params.values[i + 4].get(0),
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

    #[cfg(test)]
    pub(crate) fn set_sgr_slice(&mut self, slice: &[Option<usize>]) {
        self.set_sgr(&Params::from(slice));
    }

    /// Resizes the visible grid while preserving the top-left rectangle of existing cells.
    ///
    /// Resize does not touch parser state. Newly exposed cells are blank, and the cursor is
    /// clamped into the new bounds.
    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        self.normal.resize(rows, cols);
        self.cursor.y = self.cursor.y.min(rows.saturating_sub(1));
        self.cursor.x = self.cursor.x.min(cols.saturating_sub(1));
        self.margins.clamp(cols);
        self.tab_stops.resize(cols);
        self.scroll_region = ScrollRegion::full(rows);
        if let Some(saved) = &mut self.alt_saved {
            saved.resize(rows, cols);
        }
        if let Some(ref mut saved) = self.cursor.saved {
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
        self.modes.pending_wrap = false;
        self.cursor.x = if self.margins.enabled {
            self.margins.left
        } else {
            0
        };
    }

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

    pub(crate) fn cursor_down(&mut self, n: usize) {
        self.modes.pending_wrap = false;
        let limit = if self.modes.origin
            || (self.cursor.y >= self.scroll_region.top
                && self.cursor.y <= self.scroll_region.bottom)
        {
            self.scroll_region.bottom
        } else {
            self.normal.rows() - 1
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

    pub(crate) fn cursor_right(&mut self, n: usize) {
        self.modes.pending_wrap = false;
        let limit = if self.margins.enabled
            && self.cursor.x >= self.margins.left
            && self.cursor.x <= self.margins.right
        {
            self.margins.right
        } else {
            self.normal.cols() - 1
        };
        self.cursor.x = self.cursor.x.saturating_add(n).min(limit);
    }

    /// Expands horizontal tab into spaces up to the next 8-column tab stop or row end.
    pub(crate) fn horizontal_tab(&mut self) {
        self.modes.pending_wrap = false;
        let right_limit = if self.margins.enabled {
            self.margins.right
        } else {
            self.normal.cols()
        };
        let mut target = right_limit;
        for col in (self.cursor.x + 1)..=right_limit {
            if col < self.tab_stops.0.len() && self.tab_stops.0[col] {
                target = col;
                break;
            }
        }
        if target > self.cursor.x {
            let spaces = target - self.cursor.x;
            for _ in 0..spaces {
                self.write_char(' ');
            }
        }
    }

    /// Writes one already-decoded printable character at the cursor and advances by its terminal
    /// cell width.
    pub(crate) fn write_char(&mut self, ch: char) {
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
        if self.modes.autowrap && self.modes.pending_wrap {
            self.newline();
            self.modes.pending_wrap = false;
        }

        let right_limit = if self.margins.enabled {
            self.margins.right
        } else {
            self.normal.cols().saturating_sub(1)
        };

        // 2. Clamp cursor if autowrap is off to prevent overflow
        if !self.modes.autowrap && self.cursor.x >= right_limit {
            self.cursor.x = right_limit;
        }

        // 3. If a wide character cannot fit, wrap only when DECAWM is enabled.
        if width == 2 && self.cursor.x + 1 > right_limit {
            if !self.modes.autowrap {
                return;
            }
            self.newline();
            self.modes.pending_wrap = false;
        }
        if self.modes.insert {
            self.insert_chars(width);
        }

        self.mark_row_dirty(self.cursor.y);

        let index = self.normal.display_to_ring(self.cursor.y) * self.normal.cols() + self.cursor.x;
        self.clear_cell_for_write(index);
        if width == 2 && self.cursor.x < right_limit {
            self.clear_cell_for_write(index + 1);
        }

        let cell = self.normal.live_cell_mut(self.cursor.y, self.cursor.x);
        cell.set(
            ch,
            self.pen.fg,
            self.pen.bg,
            self.pen.attrs,
            self.pen.protected,
        );

        if width == 2 && self.cursor.x < right_limit {
            *self.normal.cell_linear_mut(index + 1) = Cell {
                ch: ' ',
                wide_continuation: true,
                fg: self.pen.fg,
                bg: self.pen.bg,
                attrs: self.pen.attrs,
                protected: self.pen.protected,
            };
        }

        // 4. Advance cursor and handle autowrap boundaries
        self.cursor.x += width;
        if self.cursor.x > right_limit {
            self.cursor.x = right_limit;
            if self.modes.autowrap {
                self.modes.pending_wrap = true;
            }
        }
        self.charsets.last_char = Some(ch);
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
        self.carriage_return();
        self.index();
    }

    pub(crate) fn line_feed(&mut self) {
        if self.modes.line_feed {
            self.carriage_return();
        }
        self.index();
    }

    /// Implements VT Index (IND): move down one row, scrolling at the bottom margin,
    /// without changing the cursor column.
    pub(crate) fn index(&mut self) {
        if self.cursor.y >= self.scroll_region.top && self.cursor.y <= self.scroll_region.bottom {
            if self.cursor.y == self.scroll_region.bottom {
                self.scroll_region_up_one();
            } else {
                self.cursor.y += 1;
            }
        } else if self.cursor.y + 1 < self.normal.rows() {
            self.cursor.y += 1;
        }
        self.modes.pending_wrap = false;
    }

    /// VT non-destructive backspace: move cursor left, skipping wide-continuation cells.
    pub(crate) fn backspace(&mut self) {
        self.modes.pending_wrap = false;
        if self.cursor.x == 0 {
            return;
        }
        self.cursor.x -= 1;

        // Step past the continuation half of a wide glyph so the cursor
        // lands at the start column rather than sitting mid-glyph.
        let index = self.normal.display_to_ring(self.cursor.y) * self.normal.cols() + self.cursor.x;
        if self.normal.cell_linear(index).wide_continuation && self.cursor.x > 0 {
            self.cursor.x -= 1;
        }
    }

    /// Positions the cursor from 1-based ANSI coordinates, clamped to the visible grid.
    pub(crate) fn set_cursor_position(&mut self, row_1_based: usize, col_1_based: usize) {
        self.modes.pending_wrap = false;
        if self.modes.origin {
            let relative_row = row_1_based.saturating_sub(1);
            let absolute_row = self.scroll_region.top.saturating_add(relative_row);
            self.cursor.y = absolute_row.clamp(self.scroll_region.top, self.scroll_region.bottom);

            let relative_col = col_1_based.saturating_sub(1);
            let absolute_col = self.margins.left.saturating_add(relative_col);
            self.cursor.x = absolute_col.clamp(self.margins.left, self.margins.right);
        } else {
            let row = row_1_based.saturating_sub(1).min(self.normal.rows() - 1);
            let col = col_1_based.saturating_sub(1).min(self.normal.cols() - 1);
            self.cursor.y = row;
            self.cursor.x = col;
        }
    }

    pub(crate) fn set_cursor_col(&mut self, col_1_based: usize) {
        self.modes.pending_wrap = false;
        if self.modes.origin {
            let relative_col = col_1_based.saturating_sub(1);
            let absolute_col = self.margins.left.saturating_add(relative_col);
            self.cursor.x = absolute_col.clamp(self.margins.left, self.margins.right);
        } else {
            self.cursor.x = col_1_based.saturating_sub(1).min(self.normal.cols() - 1);
        }
    }

    pub(crate) fn set_cursor_row(&mut self, row_1_based: usize) {
        self.modes.pending_wrap = false;
        if self.modes.origin {
            let relative_row = row_1_based.saturating_sub(1);
            let absolute_row = self.scroll_region.top.saturating_add(relative_row);
            self.cursor.y = absolute_row.clamp(self.scroll_region.top, self.scroll_region.bottom);
        } else {
            self.cursor.y = row_1_based.saturating_sub(1).min(self.normal.rows() - 1);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_cursor(&mut self, row_1_based: usize, col_1_based: usize) {
        self.set_cursor_position(row_1_based, col_1_based);
    }

    /// Returns a cell with the current SGR attributes for erase operations (EL/ED/ECH).
    fn erase_cell(&self) -> Cell {
        Cell {
            ch: ' ',
            wide_continuation: false,
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
            protected: false,
        }
    }

    /// Implements CSI `J` erase-display modes that affect visible cells.
    pub(crate) fn erase_display(&mut self, mode: usize) {
        if self.margins.enabled
            && (self.cursor.x < self.margins.left || self.cursor.x > self.margins.right)
        {
            return;
        }
        self.modes.pending_wrap = false;
        let cell = self.erase_cell();
        let cols = self.normal.cols();
        let (left_col, right_col) = if self.margins.enabled {
            (self.margins.left, self.margins.right)
        } else {
            (0, cols - 1)
        };

        match mode {
            0 => {
                self.mark_row_dirty(self.cursor.y);
                let ring_row = self.normal.display_to_ring(self.cursor.y);
                let start = ring_row * cols + self.cursor.x;
                let end = ring_row * cols + right_col + 1;
                self.normal.fill_linear_range_with(start, end, cell);
                for row in self.cursor.y + 1..self.normal.rows() {
                    self.mark_row_dirty(row);
                    let r_row = self.normal.display_to_ring(row);
                    self.normal.fill_linear_range_with(
                        r_row * cols + left_col,
                        r_row * cols + right_col + 1,
                        cell,
                    );
                }
            }
            1 => {
                for row in 0..self.cursor.y {
                    self.mark_row_dirty(row);
                    let r_row = self.normal.display_to_ring(row);
                    self.normal.fill_linear_range_with(
                        r_row * cols + left_col,
                        r_row * cols + right_col + 1,
                        cell,
                    );
                }
                self.mark_row_dirty(self.cursor.y);
                let ring_row = self.normal.display_to_ring(self.cursor.y);
                let start = ring_row * cols + left_col;
                let end = ring_row * cols + self.cursor.x + 1;
                self.normal.fill_linear_range_with(start, end, cell);
            }
            2 => {
                for row in 0..self.normal.rows() {
                    self.mark_row_dirty(row);
                    let r_row = self.normal.display_to_ring(row);
                    self.normal.fill_linear_range_with(
                        r_row * cols + left_col,
                        r_row * cols + right_col + 1,
                        cell,
                    );
                }
                self.home_cursor();
                self.mark_all_dirty();
            }
            _ => {}
        }
    }

    /// Implements CSI `K` erase-line modes for the current row.
    pub(crate) fn erase_line(&mut self, mode: usize) {
        if self.margins.enabled
            && (self.cursor.x < self.margins.left || self.cursor.x > self.margins.right)
        {
            return;
        }
        self.modes.pending_wrap = false;
        let cell = self.erase_cell();
        let ring_row = self.normal.display_to_ring(self.cursor.y);
        let cols = self.normal.cols();
        let (left_col, right_col) = if self.margins.enabled {
            (self.margins.left, self.margins.right)
        } else {
            (0, cols - 1)
        };
        let start = ring_row * cols + left_col;
        let cursor = ring_row * cols + self.cursor.x;
        let end = ring_row * cols + right_col + 1;
        self.mark_row_dirty(self.cursor.y);
        match mode {
            0 => self.normal.fill_linear_range_with(cursor, end, cell),
            1 => self.normal.fill_linear_range_with(start, cursor + 1, cell),
            2 => self.normal.fill_linear_range_with(start, end, cell),
            _ => {}
        }
    }

    /// Implements CSI `X` (ECH): erase `n` characters from the cursor rightward.
    ///
    /// Replaces cells with default (space) characters without moving the cursor.
    /// The default parameter (0) acts as 1.
    pub(crate) fn erase_chars(&mut self, n: usize) {
        if self.margins.enabled
            && (self.cursor.x < self.margins.left || self.cursor.x > self.margins.right)
        {
            return;
        }
        self.modes.pending_wrap = false;
        let cell = self.erase_cell();
        let n = if n == 0 { 1 } else { n };
        self.mark_row_dirty(self.cursor.y);
        let ring_row = self.normal.display_to_ring(self.cursor.y);
        let cols = self.normal.cols();
        let right_col = if self.margins.enabled {
            self.margins.right
        } else {
            cols - 1
        };
        let start = ring_row * cols + self.cursor.x;
        let end = (start + n).min(ring_row * cols + right_col + 1);
        self.normal.fill_linear_range_with(start, end, cell);
    }

    pub(crate) fn selective_erase_display(&mut self, mode: usize) {
        if self.margins.enabled
            && (self.cursor.x < self.margins.left || self.cursor.x > self.margins.right)
        {
            return;
        }
        let erase = self.erase_cell();
        let cols = self.normal.cols();
        let (left_col, right_col) = if self.margins.enabled {
            (self.margins.left, self.margins.right)
        } else {
            (0, cols - 1)
        };

        match mode {
            0 => {
                self.mark_row_dirty(self.cursor.y);
                let ring_row = self.normal.display_to_ring(self.cursor.y);
                let start_idx = ring_row * cols + self.cursor.x;
                let row_end = ring_row * cols + right_col + 1;
                for idx in start_idx..row_end {
                    let cell = self.normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
                for row in self.cursor.y + 1..self.normal.rows() {
                    self.mark_row_dirty(row);
                    let r_row = self.normal.display_to_ring(row);
                    let r_start = r_row * cols + left_col;
                    let r_end = r_row * cols + right_col + 1;
                    for idx in r_start..r_end {
                        let cell = self.normal.cell_linear_mut(idx);
                        if !cell.protected {
                            *cell = erase;
                        }
                    }
                }
            }
            1 => {
                for row in 0..self.cursor.y {
                    self.mark_row_dirty(row);
                    let r_row = self.normal.display_to_ring(row);
                    let r_start = r_row * cols + left_col;
                    let r_end = r_row * cols + right_col + 1;
                    for idx in r_start..r_end {
                        let cell = self.normal.cell_linear_mut(idx);
                        if !cell.protected {
                            *cell = erase;
                        }
                    }
                }
                self.mark_row_dirty(self.cursor.y);
                let ring_row = self.normal.display_to_ring(self.cursor.y);
                let start_idx = ring_row * cols + left_col;
                let end_idx = ring_row * cols + self.cursor.x + 1;
                for idx in start_idx..end_idx {
                    let cell = self.normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            2 => {
                for row in 0..self.normal.rows() {
                    self.mark_row_dirty(row);
                    let r_row = self.normal.display_to_ring(row);
                    let r_start = r_row * cols + left_col;
                    let r_end = r_row * cols + right_col + 1;
                    for idx in r_start..r_end {
                        let cell = self.normal.cell_linear_mut(idx);
                        if !cell.protected {
                            *cell = erase;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn selective_erase_line(&mut self, mode: usize) {
        if self.margins.enabled
            && (self.cursor.x < self.margins.left || self.cursor.x > self.margins.right)
        {
            return;
        }
        let erase = self.erase_cell();
        let ring_row = self.normal.display_to_ring(self.cursor.y);
        let cols = self.normal.cols();
        let (left_col, right_col) = if self.margins.enabled {
            (self.margins.left, self.margins.right)
        } else {
            (0, cols - 1)
        };
        let start_idx = ring_row * cols + left_col;
        let cursor_idx = ring_row * cols + self.cursor.x;
        let end_idx = ring_row * cols + right_col + 1;
        self.mark_row_dirty(self.cursor.y);
        match mode {
            0 => {
                for idx in cursor_idx..end_idx {
                    let cell = self.normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            1 => {
                for idx in start_idx..=cursor_idx {
                    let cell = self.normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            2 => {
                for idx in start_idx..end_idx {
                    let cell = self.normal.cell_linear_mut(idx);
                    if !cell.protected {
                        *cell = erase;
                    }
                }
            }
            _ => {}
        }
    }

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

    /// Clears all visible cells, homes the cursor, and resets most state
    /// groups to power-on defaults (RIS / `ESC c`).  Preserves cursor shape
    /// and blink.
    pub(crate) fn reset_display(&mut self) {
        self.in_alt = false;
        self.alt_saved = None;

        self.normal.fill_all();
        self.cursor.x = 0;
        self.cursor.y = 0;
        self.cursor.visible = true;
        self.pen = Pen::reset();
        self.scroll_region = ScrollRegion::full(self.normal.rows());
        self.margins = Margins::full(self.normal.cols());
        self.modes = TerminalModes::default();
        self.charsets.reset();
        self.tab_stops = TabStops::new(self.normal.cols());
        self.cursor.saved = None;
        self.mark_all_dirty();
    }

    /// Soft terminal reset (DECSTR / `CSI ! p`): resets pen, scroll region,
    /// margins, modes, and saved cursor.  Preserves character-set designations,
    /// tab stops, cell contents, cursor shape, and blink.
    pub(crate) fn soft_reset(&mut self) {
        self.pen = Pen::reset();
        self.scroll_region = ScrollRegion::full(self.normal.rows());
        self.margins = Margins::full(self.normal.cols());
        self.modes = TerminalModes::default();

        self.cursor.saved = None;
        self.charsets.last_char = None;
        self.cursor.x = 0;
        self.cursor.y = 0;
        self.cursor.visible = true;
    }

    fn scroll_margin_rect_up(&mut self, top: usize, bottom: usize, n: usize) {
        let height = bottom - top + 1;
        if n < height {
            for dst_row in top..=(bottom - n) {
                let src_row = dst_row + n;
                for col in self.margins.left..=self.margins.right {
                    let cell = *self.normal.cell(src_row, col);
                    *self.normal.cell_mut(dst_row, col) = cell;
                }
            }
        }
        let blank = self.erase_cell();
        for row in (bottom + 1 - n)..=bottom {
            for col in self.margins.left..=self.margins.right {
                *self.normal.cell_mut(row, col) = blank;
            }
        }
    }

    fn scroll_margin_rect_down(&mut self, top: usize, bottom: usize, n: usize) {
        let height = bottom - top + 1;
        if n < height {
            for dst_row in ((top + n)..=bottom).rev() {
                let src_row = dst_row - n;
                for col in self.margins.left..=self.margins.right {
                    let cell = *self.normal.cell(src_row, col);
                    *self.normal.cell_mut(dst_row, col) = cell;
                }
            }
        }
        let blank = self.erase_cell();
        for row in top..(top + n) {
            for col in self.margins.left..=self.margins.right {
                *self.normal.cell_mut(row, col) = blank;
            }
        }
    }

    /// Implements reverse index (`ESC M`): move up, or scroll the scrolling region down
    /// when already at the top of the region. When the cursor is above the region,
    /// moves up if not already at the top of the full screen.
    pub(crate) fn reverse_index(&mut self) {
        tracing::debug!(
            cursor_y = self.cursor.y,
            scroll_top = self.scroll_region.top,
            scroll_bottom = self.scroll_region.bottom,
            full_screen = (self.scroll_region.top == 0
                && self.scroll_region.bottom == self.normal.rows() - 1),
            "reverse_index"
        );

        if self.cursor.y == self.scroll_region.top && self.cursor.y <= self.scroll_region.bottom {
            // Mark only region rows dirty.
            for row in self.scroll_region.top..=self.scroll_region.bottom {
                self.mark_row_dirty(row);
            }
            if self.margins.enabled {
                self.scroll_margin_rect_down(self.scroll_region.top, self.scroll_region.bottom, 1);
            } else if self.scroll_region.top == 0
                && self.scroll_region.bottom == self.normal.rows() - 1
            {
                // Full-screen reverse: use scroll_up_full_screen in reverse direction.
                // Shift down by 1: blank top, everything moves down.
                let tr = self.normal.total_rows();
                let vis = self.normal.visible_start();
                let c = self.normal.cols();
                let src_start = ((vis + self.scroll_region.top) % tr) * c;
                let src_end = ((vis + self.scroll_region.bottom) % tr) * c;
                let dst = ((vis + self.scroll_region.top + 1) % tr) * c;
                if src_start <= src_end {
                    self.normal.copy_linear_range(src_start, src_end, dst);
                } else {
                    let first_part = src_start..tr * c;
                    let second_part = 0..src_end;
                    let first_len = first_part.len();
                    self.normal.copy_cells_within(first_part, dst);
                    self.normal.copy_cells_within(second_part, dst + first_len);
                }
                self.normal
                    .fill_row_with(self.scroll_region.top, self.erase_cell());
            } else {
                // Partial region: ring-aware copy_within down.
                let tr = self.normal.total_rows();
                let vis = self.normal.visible_start();
                let c = self.normal.cols();
                let src_start = ((vis + self.scroll_region.top) % tr) * c;
                let src_end = ((vis + self.scroll_region.bottom) % tr) * c;
                let dst = ((vis + self.scroll_region.top + 1) % tr) * c;
                if src_start <= src_end {
                    self.normal.copy_linear_range(src_start, src_end, dst);
                } else {
                    let first_part = src_start..tr * c;
                    let second_part = 0..src_end;
                    let first_len = first_part.len();
                    self.normal.copy_cells_within(first_part, dst);
                    self.normal.copy_cells_within(second_part, dst + first_len);
                }
                self.normal
                    .fill_row_with(self.scroll_region.top, self.erase_cell());
            }
        } else if self.cursor.y > 0 {
            self.cursor.y -= 1;
        }
    }

    /// Scrolls the scrolling region up by one row: shifts rows within `[scroll_top, scroll_bottom]`
    /// upward (row N → N-1) and fills the newly exposed bottom row of the region with blank
    /// cells. The caller controls the cursor column.
    fn scroll_region_up_one(&mut self) {
        tracing::debug!(
            scroll_top = self.scroll_region.top,
            scroll_bottom = self.scroll_region.bottom,
            visible_rows = self.normal.rows(),
            full_screen = (self.scroll_region.top == 0
                && self.scroll_region.bottom == self.normal.rows() - 1),
            "scroll_region_up_one"
        );

        for row in self.scroll_region.top..=self.scroll_region.bottom {
            self.mark_row_dirty(row);
        }
        if self.margins.enabled {
            self.scroll_margin_rect_up(self.scroll_region.top, self.scroll_region.bottom, 1);
        } else if self.scroll_region.top == 0 && self.scroll_region.bottom == self.normal.rows() - 1
        {
            // Full-screen: O(1) ring-buffer advance.
            self.normal.scroll_up_full_screen(1, self.erase_cell());
        } else {
            // Partial scroll region: ring-aware copy_within.
            let tr = self.normal.total_rows();
            let vis = self.normal.visible_start();
            let c = self.normal.cols();
            let src_start = ((vis + self.scroll_region.top + 1) % tr) * c;
            let src_end = ((vis + self.scroll_region.bottom + 1) % tr) * c;
            let dst = ((vis + self.scroll_region.top) % tr) * c;
            if src_start <= src_end {
                self.normal.copy_linear_range(src_start, src_end, dst);
            } else {
                let first_len = tr * c - src_start;
                let first_part = src_start..tr * c;
                let second_part = 0..src_end;
                self.normal.copy_cells_within(first_part, dst);
                self.normal.copy_cells_within(second_part, dst + first_len);
            }
            self.normal
                .fill_row_with(self.scroll_region.bottom, self.erase_cell());
        }
        self.cursor.y = self.scroll_region.bottom;
    }

    /// Implements DECSTBM (`CSI r`): set scrolling region.
    ///
    /// `top` and `bottom` are 1-based ANSI coordinates. A value of 0 means "use default"
    /// (top=1, bottom=rows). If `top >= bottom` after clamping, the request is ignored.
    /// Cursor is moved to the active home position on success.
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
        self.scroll_region.top = top - 1;
        self.scroll_region.bottom = bottom - 1;
        self.home_cursor();
    }

    pub(crate) fn set_left_right_margins(&mut self, left: usize, right: usize) {
        let left = if left == 0 { 1 } else { left };
        let right = if right == 0 {
            self.normal.cols()
        } else {
            right
        };
        let left = left.max(1).min(self.normal.cols());
        let right = right.min(self.normal.cols());
        if left < right {
            self.margins.left = left - 1;
            self.margins.right = right - 1;
        }
        self.home_cursor();
    }

    /// Implements CSI `@` (ICH): insert `n` blank characters at the cursor, shifting
    /// existing characters right. Characters past the right margin are lost.
    /// The cursor position does not change.
    pub(crate) fn insert_chars(&mut self, n: usize) {
        self.modes.pending_wrap = false;
        let n = if n == 0 { 1 } else { n };
        self.mark_row_dirty(self.cursor.y);
        let col = self.cursor.x;
        let (left, right) = if self.margins.enabled {
            (self.margins.left, self.margins.right)
        } else {
            (0, self.normal.cols() - 1)
        };
        if col < left || col > right {
            return;
        }
        let n = n.min(right - col + 1);
        if n == 0 {
            return;
        }
        let ring_row = self.normal.display_to_ring(self.cursor.y);
        let row_start = ring_row * self.normal.cols();

        // Shift cells in [col, right - n + 1] right by n
        let src_start = row_start + col;
        let src_end = row_start + right - n + 1;
        let dst = row_start + col + n;
        if src_start < src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        }

        // Fill vacated cells with blanks
        self.normal
            .fill_linear_range_with(row_start + col, row_start + col + n, self.erase_cell());
    }

    /// Implements CSI `P` (DCH): delete `n` characters at the cursor, shifting remaining
    /// characters left. Vacated cells at the right margin become blank.
    /// The cursor position does not change.
    pub(crate) fn delete_chars(&mut self, n: usize) {
        self.modes.pending_wrap = false;
        let n = if n == 0 { 1 } else { n };
        self.mark_row_dirty(self.cursor.y);
        let col = self.cursor.x;
        let (left, right) = if self.margins.enabled {
            (self.margins.left, self.margins.right)
        } else {
            (0, self.normal.cols() - 1)
        };
        if col < left || col > right {
            return;
        }
        let n = n.min(right - col + 1);
        if n == 0 {
            return;
        }
        let ring_row = self.normal.display_to_ring(self.cursor.y);
        let row_start = ring_row * self.normal.cols();

        // Shift cells in [col + n, right] left by n
        let src_start = row_start + col + n;
        let src_end = row_start + right + 1;
        let dst = row_start + col;
        if src_start < src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        }

        // Fill vacated cells at the right margin
        let blank_start = row_start + right + 1 - n;
        self.normal
            .fill_linear_range_with(blank_start, blank_start + n, self.erase_cell());
    }
    pub(crate) fn insert_lines(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        if self.cursor.y < self.scroll_region.top || self.cursor.y > self.scroll_region.bottom {
            return;
        }
        let max_n = self.scroll_region.bottom - self.cursor.y + 1;
        let n = n.min(max_n);
        // Mark all affected rows dirty.
        for row in self.cursor.y..=self.scroll_region.bottom {
            self.mark_row_dirty(row);
        }
        if self.margins.enabled {
            self.scroll_margin_rect_down(self.cursor.y, self.scroll_region.bottom, n);
            self.cursor.x = 0;
            return;
        }
        // When n covers all remaining rows in the region, just blank them all.
        if n == max_n {
            for row in self.cursor.y..=self.scroll_region.bottom {
                self.normal.fill_row_with(row, self.erase_cell());
            }
            self.cursor.x = 0;
            return;
        }
        // Shift rows [cursor_y .. scroll_bottom - n + 1] down by n (ring-aware).
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.cursor.y) % tr) * c;
        let src_end = ((vis + self.scroll_region.bottom - n + 1) % tr) * c;
        let dst = ((vis + self.cursor.y + n) % tr) * c;
        if src_start <= src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        } else {
            let first_len = tr * c - src_start;
            self.normal.copy_linear_range(src_start, tr * c, dst);
            self.normal.copy_linear_range(0, src_end, dst + first_len);
        }
        for i in 0..n {
            self.normal
                .fill_row_with(self.cursor.y + i, self.erase_cell());
        }
        self.cursor.x = 0;
    }

    pub(crate) fn delete_lines(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        if self.cursor.y < self.scroll_region.top || self.cursor.y > self.scroll_region.bottom {
            return;
        }
        let max_n = self.scroll_region.bottom - self.cursor.y + 1;
        let n = n.min(max_n);
        // Mark all affected rows dirty.
        for row in self.cursor.y..=self.scroll_region.bottom {
            self.mark_row_dirty(row);
        }
        if self.margins.enabled {
            self.scroll_margin_rect_up(self.cursor.y, self.scroll_region.bottom, n);
            self.cursor.x = 0;
            return;
        }
        if n == max_n {
            for row in self.cursor.y..=self.scroll_region.bottom {
                self.normal.fill_row_with(row, self.erase_cell());
            }
            self.cursor.x = 0;
            return;
        }
        // Shift rows [cursor_y + n ..= scroll_bottom] up by n (ring-aware).
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.cursor.y + n) % tr) * c;
        let src_end = ((vis + self.scroll_region.bottom + 1) % tr) * c;
        let dst = ((vis + self.cursor.y) % tr) * c;
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
            self.normal
                .fill_row_with(self.scroll_region.bottom - i, self.erase_cell());
        }
        self.cursor.x = 0;
    }

    /// Implements CSI `S` (SU): scroll the scrolling region up by `n` lines.
    /// Top `n` lines of the region are lost; blank lines appear at the bottom.
    /// The cursor does not move.
    pub(crate) fn scroll_up_region(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        let region_height = self.scroll_region.bottom - self.scroll_region.top + 1;
        let n = n.min(region_height);
        // Mark only region rows dirty.
        for row in self.scroll_region.top..=self.scroll_region.bottom {
            self.mark_row_dirty(row);
        }
        if self.margins.enabled {
            self.scroll_margin_rect_up(self.scroll_region.top, self.scroll_region.bottom, n);
            return;
        }
        // When n covers the entire region, just blank everything.
        if n == region_height {
            for row in self.scroll_region.top..=self.scroll_region.bottom {
                self.normal.fill_row_with(row, self.erase_cell());
            }
            return;
        }
        // Shift rows [scroll_top + n ..= scroll_bottom] up by n.
        // Full-screen case handled via scroll_up_full_screen; here handle partial.
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.scroll_region.top + n) % tr) * c;
        let src_end = ((vis + self.scroll_region.bottom + 1) % tr) * c;
        let dst = ((vis + self.scroll_region.top) % tr) * c;
        // Handle wrap-around for copy_within.
        if src_start <= src_end {
            self.normal.copy_linear_range(src_start, src_end, dst);
        } else {
            let first_part = ((vis + self.scroll_region.top + n) % tr) * c..tr * c;
            let second_part = 0..src_end;
            let first_len = first_part.len();
            self.normal.copy_cells_within(first_part, dst);
            self.normal.copy_cells_within(second_part, dst + first_len);
        }
        for i in 0..n {
            self.normal
                .fill_row_with(self.scroll_region.bottom - i, self.erase_cell());
        }
    }

    /// Implements CSI `T` (SD): scroll the scrolling region down by `n` lines.
    /// Bottom `n` lines of the region are lost; blank lines appear at the top.
    /// The cursor does not move.
    pub(crate) fn scroll_down_region(&mut self, n: usize) {
        let n = if n == 0 { 1 } else { n };
        let region_height = self.scroll_region.bottom - self.scroll_region.top + 1;
        let n = n.min(region_height);
        // Mark only region rows dirty.
        for row in self.scroll_region.top..=self.scroll_region.bottom {
            self.mark_row_dirty(row);
        }
        if self.margins.enabled {
            self.scroll_margin_rect_down(self.scroll_region.top, self.scroll_region.bottom, n);
            return;
        }
        // When n covers the entire region, just blank everything.
        if n == region_height {
            for row in self.scroll_region.top..=self.scroll_region.bottom {
                self.normal.fill_row_with(row, self.erase_cell());
            }
            return;
        }
        // Shift rows [scroll_top ..= scroll_bottom - n] down by n.
        let tr = self.normal.total_rows();
        let vis = self.normal.visible_start();
        let c = self.normal.cols();
        let src_start = ((vis + self.scroll_region.top) % tr) * c;
        let src_end = ((vis + self.scroll_region.bottom - n + 1) % tr) * c;
        let dst = ((vis + self.scroll_region.top + n) % tr) * c;
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
            self.normal
                .fill_row_with(self.scroll_region.top + i, self.erase_cell());
        }
    }

    pub(crate) fn set_private_mode(&mut self, param: usize, enabled: bool) {
        match param {
            1 => {
                self.modes.application_cursor = enabled;
            }
            66 => {
                self.modes.application_keypad = enabled;
            }
            6 => {
                self.modes.origin = enabled;
                self.home_cursor();
            }
            7 => {
                self.modes.autowrap = enabled;
            }
            25 => {
                self.cursor.visible = enabled;
            }
            69 => {
                self.margins.enabled = enabled;
                if !enabled {
                    self.margins.left = 0;
                    self.margins.right = self.normal.cols().saturating_sub(1);
                }
                self.home_cursor();
            }
            1049 => {
                if enabled {
                    self.request_alt_enter();
                } else {
                    self.request_alt_exit();
                }
            }
            _ => {
                tracing::warn!("unsupported private mode: ?{}", param);
            }
        }
    }

    pub(crate) fn set_standard_mode(&mut self, param: usize, enabled: bool) {
        match param {
            4 => {
                self.modes.insert = enabled;
            }
            20 => {
                self.modes.line_feed = enabled;
            }
            _ => {
                tracing::warn!("unsupported standard mode: {}", param);
            }
        }
    }

    pub(crate) fn set_application_keypad(&mut self, enabled: bool) {
        self.modes.application_keypad = enabled;
    }

    pub(crate) fn designate_g0(&mut self, charset: u8) {
        self.charsets.g0 = charset;
    }

    pub(crate) fn designate_g1(&mut self, charset: u8) {
        self.charsets.g1 = charset;
    }

    pub(crate) fn set_active_charset(&mut self, active: u8) {
        self.charsets.active = active;
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

    pub(crate) fn set_tab_stop(&mut self) {
        if self.cursor.x < self.tab_stops.0.len() {
            self.tab_stops.0[self.cursor.x] = true;
        }
    }

    pub(crate) fn clear_tab_stops(&mut self, mode: usize) {
        match mode {
            0 => {
                if self.cursor.x < self.tab_stops.0.len() {
                    self.tab_stops.0[self.cursor.x] = false;
                }
            }
            3 => {
                self.tab_stops.0.fill(false);
            }
            _ => {}
        }
    }

    pub(crate) fn repeat_char(&mut self, n: usize) {
        if let Some(ch) = self.charsets.last_char {
            let n = if n == 0 { 1 } else { n };
            let count = n.min(self.normal.cols());
            for _ in 0..count {
                self.write_char(ch);
            }
        }
    }

    fn resolve_rect(&self, top: usize, left: usize, bottom: usize, right: usize) -> Option<Rect> {
        let (top_bound, bottom_bound, left_bound, right_bound) = if self.modes.origin {
            (
                self.scroll_region.top,
                self.scroll_region.bottom,
                self.margins.left,
                self.margins.right,
            )
        } else {
            (0, self.normal.rows() - 1, 0, self.normal.cols() - 1)
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

    pub(crate) fn decera(&mut self, params: &Params) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = self.resolve_rect(top, left, bottom, right)
        else {
            return;
        };
        let erase = self.erase_cell();
        for row in t..=b {
            self.mark_row_dirty(row);
            for col in l..=r {
                *self.normal.cell_mut(row, col) = erase;
            }
        }
    }

    pub(crate) fn decsera(&mut self, params: &Params) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = self.resolve_rect(top, left, bottom, right)
        else {
            return;
        };
        let erase = self.erase_cell();
        for row in t..=b {
            self.mark_row_dirty(row);
            for col in l..=r {
                let cell = self.normal.cell_mut(row, col);
                if !cell.protected {
                    *cell = erase;
                }
            }
        }
    }

    pub(crate) fn decfra(&mut self, params: &Params) {
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
        }) = self.resolve_rect(top, left, bottom, right)
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
            self.mark_row_dirty(row);
            for col in l..=r {
                *self.normal.cell_mut(row, col) = cell;
            }
        }
    }

    pub(crate) fn deccra(&mut self, params: &Params) {
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
        }) = self.resolve_rect(src_top, src_left, src_bottom, src_right)
        else {
            return;
        };

        let dt_start = if self.modes.origin {
            let r = dest_top.saturating_sub(1);
            self.scroll_region.top + r
        } else {
            dest_top.saturating_sub(1)
        };
        let dl_start = if self.modes.origin {
            let c = dest_left.saturating_sub(1);
            self.margins.left + c
        } else {
            dest_left.saturating_sub(1)
        };

        let height = sb - st + 1;
        let width = sr - sl + 1;

        let mut temp = Vec::with_capacity(height * width);
        for row in st..=sb {
            for col in sl..=sr {
                temp.push(*self.normal.cell(row, col));
            }
        }

        let max_rows = self.normal.rows();
        let max_cols = self.normal.cols();

        for h in 0..height {
            let dest_row = dt_start + h;
            if dest_row >= max_rows {
                break;
            }
            self.mark_row_dirty(dest_row);
            for w in 0..width {
                let dest_col = dl_start + w;
                if dest_col >= max_cols {
                    break;
                }
                let src_cell = temp[h * width + w];
                *self.normal.cell_mut(dest_row, dest_col) = src_cell;
            }
        }
    }

    fn apply_sgr_to_cell(cell: &mut Cell, code: usize) {
        match code {
            0 => {
                cell.fg = Color::Default;
                cell.bg = Color::Default;
                cell.attrs = CellAttrs::default();
                cell.protected = false;
            }
            1 => cell.attrs.set(CellAttrs::BOLD),
            2 => cell.attrs.set(CellAttrs::DIM),
            3 => cell.attrs.set(CellAttrs::ITALIC),
            4 => cell.attrs.set(CellAttrs::UNDERLINE),
            5 => cell.attrs.set(CellAttrs::BLINK),
            7 => cell.attrs.set(CellAttrs::INVERSE),
            9 => cell.attrs.set(CellAttrs::STRIKETHROUGH),
            22 => cell.attrs.clear(CellAttrs::BOLD | CellAttrs::DIM),
            23 => cell.attrs.clear(CellAttrs::ITALIC),
            24 => cell.attrs.clear(CellAttrs::UNDERLINE),
            25 => cell.attrs.clear(CellAttrs::BLINK),
            27 => cell.attrs.clear(CellAttrs::INVERSE),
            29 => cell.attrs.clear(CellAttrs::STRIKETHROUGH),
            30..=37 => cell.fg = Color::Named((code - 30) as u8),
            40..=47 => cell.bg = Color::Named((code - 40) as u8),
            39 => cell.fg = Color::Default,
            49 => cell.bg = Color::Default,
            90..=97 => cell.fg = Color::Bright((code - 90) as u8),
            100..=107 => cell.bg = Color::Bright((code - 100) as u8),
            _ => {}
        }
    }

    pub(crate) fn deccara(&mut self, params: &Params) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = self.resolve_rect(top, left, bottom, right)
        else {
            return;
        };

        for row in t..=b {
            self.mark_row_dirty(row);
            for col in l..=r {
                let cell = self.normal.cell_mut(row, col);
                for idx in 4..params.len {
                    if let Some(code) = params.values[idx].get(0) {
                        Self::apply_sgr_to_cell(cell, code);
                    }
                }
            }
        }
    }

    fn toggle_sgr_on_cell(cell: &mut Cell, code: usize) {
        match code {
            1 => cell.attrs.0 ^= CellAttrs::BOLD,
            2 => cell.attrs.0 ^= CellAttrs::DIM,
            3 => cell.attrs.0 ^= CellAttrs::ITALIC,
            4 => cell.attrs.0 ^= CellAttrs::UNDERLINE,
            5 => cell.attrs.0 ^= CellAttrs::BLINK,
            7 => cell.attrs.0 ^= CellAttrs::INVERSE,
            9 => cell.attrs.0 ^= CellAttrs::STRIKETHROUGH,
            _ => {}
        }
    }

    pub(crate) fn decrara(&mut self, params: &Params) {
        let top = params.get_or(0, 0);
        let left = params.get_or(1, 0);
        let bottom = params.get_or(2, 0);
        let right = params.get_or(3, 0);

        let Some(Rect {
            top: t,
            left: l,
            bottom: b,
            right: r,
        }) = self.resolve_rect(top, left, bottom, right)
        else {
            return;
        };

        for row in t..=b {
            self.mark_row_dirty(row);
            for col in l..=r {
                let cell = self.normal.cell_mut(row, col);
                for idx in 4..params.len {
                    if let Some(code) = params.values[idx].get(0) {
                        Self::toggle_sgr_on_cell(cell, code);
                    }
                }
            }
        }
    }

    /// Implements DECSC (`ESC 7`): save cursor position and SGR attributes.
    pub(crate) fn save_cursor(&mut self) {
        self.cursor.saved = Some(SavedCursor {
            cursor_x: self.cursor.x,
            cursor_y: self.cursor.y,
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
            origin_mode: self.modes.origin,
            autowrap: self.modes.autowrap,
            pending_wrap: self.modes.pending_wrap,
        });
    }

    /// Implements DECRC (`ESC 8`): restore cursor position and SGR attributes.
    /// If no cursor was previously saved, this is a no-op.
    pub(crate) fn restore_cursor(&mut self) {
        if let Some(saved) = &self.cursor.saved {
            self.cursor.x = saved.cursor_x;
            self.cursor.y = saved.cursor_y;
            self.pen.fg = saved.fg;
            self.pen.bg = saved.bg;
            self.pen.attrs = saved.attrs;
            self.modes.origin = saved.origin_mode;
            self.modes.autowrap = saved.autowrap;
            self.modes.pending_wrap = saved.pending_wrap;
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
        let rows = self.rows();
        let cols = self.cols();
        let saved = std::mem::replace(self, Self::new(rows, cols));
        self.alt_saved = Some(Box::new(saved));
        self.in_alt = true;
    }

    pub(crate) fn exit_alt(&mut self) {
        if let Some(saved) = self.alt_saved.take() {
            *self = *saved;
        }
        self.in_alt = false;
    }
}

fn map_dec_graphics(ch: char) -> char {
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
