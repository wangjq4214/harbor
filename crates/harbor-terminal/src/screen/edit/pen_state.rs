//! Pen state: SGR pen, tab stops, character sets, and erase-cell helper.
//!
//! Owns the SGR pen, horizontal tab stops, and character-set designations.
//! Cell-erase uses the current pen to produce blank cells tinted with the
//! active foreground, background, and attributes.

use harbor_parser::Params;
use harbor_types::{Cell, CellAttrs, CharacterProtection, Color};

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

/// Snapshot of pen color + attributes for DECSC/DECRC save/restore.
#[derive(Debug, Clone, Copy)]
struct SavedPen {
    fg: Color,
    bg: Color,
    attrs: CellAttrs,
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

/// Owns pen state, tab stops, character-set designations, and saved-pen snapshot.
#[derive(Debug)]
pub(crate) struct PenState {
    pub(crate) pen: Pen,
    pub(crate) tab_stops: TabStops,
    pub(crate) charsets: CharacterSets,
    saved_pen: Option<SavedPen>,
}

impl PenState {
    pub(crate) fn new(cols: usize) -> Self {
        Self {
            pen: Pen::reset(),
            tab_stops: TabStops::new(cols),
            charsets: CharacterSets::default(),
            saved_pen: None,
        }
    }

    /// Resets pen, charsets, tab-stops, and saved-pen snapshot to defaults (RIS).
    pub(crate) fn reset(&mut self, cols: usize) {
        self.pen = Pen::reset();
        self.charsets.reset();
        self.tab_stops = TabStops::new(cols);
        self.saved_pen = None;
    }

    /// Soft reset (DECSTR): resets pen and charsets.last_char, but leaves
    /// tab-stops and G0/G1/active charset designations intact.
    pub(crate) fn soft_reset(&mut self) {
        self.pen = Pen::reset();
        self.charsets.last_char = None;
        self.saved_pen = None;
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

    /// Saves the current pen colors + attributes (DECSC).
    pub(crate) fn save_pen(&mut self) {
        self.saved_pen = Some(SavedPen {
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
        });
    }

    /// Restores the saved pen colors + attributes (DECRC).
    pub(crate) fn restore_pen(&mut self) {
        if let Some(saved) = self.saved_pen {
            self.pen.fg = saved.fg;
            self.pen.bg = saved.bg;
            self.pen.attrs = saved.attrs;
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
}

/// Maps the DEC Special Graphics character set (designator `'0'`).
pub(crate) fn map_dec_graphics(ch: char) -> char {
    match ch {
        '`' => '\u{25c6}',
        'a' => '\u{2592}',
        'f' => '\u{00b0}',
        'g' => '\u{00b1}',
        'j' => '\u{2518}',
        'k' => '\u{2510}',
        'l' => '\u{250c}',
        'm' => '\u{2514}',
        'n' => '\u{253c}',
        'o' => '\u{23ba}',
        'p' => '\u{23bb}',
        'q' => '\u{2500}',
        'r' => '\u{23bc}',
        's' => '\u{23bd}',
        't' => '\u{251c}',
        'u' => '\u{2524}',
        'v' => '\u{2534}',
        'w' => '\u{252c}',
        'x' => '\u{2502}',
        'y' => '\u{2264}',
        'z' => '\u{2265}',
        '{' => '\u{03c0}',
        '|' => '\u{2260}',
        '}' => '\u{00a3}',
        '~' => '\u{00b7}',
        _ => ch,
    }
}
