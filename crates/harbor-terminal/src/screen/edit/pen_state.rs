//! Pen state: SGR pen, tab stops, character sets, and erase-cell helper.
//!
//! Owns the SGR pen, horizontal tab stops, and character-set designations.
//! Cell-erase uses the current pen to produce blank cells tinted with the
//! active foreground, background, and attributes.

use crate::model::{Cell, CellAttrs, CharacterProtection, UnderlineStyle};
use harbor_config::Color;
use harbor_parser::Params;

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
    pub(crate) underline_color: Color,
    /// Whether newly written cells are protected (DECSCA).
    pub(crate) protected: bool,
}

impl Pen {
    pub(crate) fn reset() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
            underline_color: Color::Default,
            protected: false,
        }
    }
}

/// Snapshot of pen color + attributes + character sets for DECSC/DECRC save/restore.
#[derive(Debug, Clone, Copy)]
struct SavedPen {
    hyperlink: Option<crate::model::HyperlinkId>,
    fg: Color,
    bg: Color,
    attrs: CellAttrs,
    underline_color: Color,
    charsets: CharacterSets,
}

/// Parse only the approved SGR 58 forms, consuming the entire candidate even
/// when invalid. Colon fields never become independent top-level attributes.
fn underline_color(params: &Params, i: usize) -> (Option<Color>, usize) {
    let len = params.sub_params_len(i).unwrap_or(0);
    if len > 1 {
        let color = match (params.get_sub_param(i, 1), len) {
            (Some(5), 3) => params
                .get_sub_param(i, 2)
                .filter(|&n| n <= 255)
                .map(|n| Color::Indexed(n as u8)),
            (Some(2), 5) => rgb_color(
                params.get_sub_param(i, 2),
                params.get_sub_param(i, 3),
                params.get_sub_param(i, 4),
            ),
            (Some(2), 6) if params.get_sub_param(i, 2).is_none() => rgb_color(
                params.get_sub_param(i, 3),
                params.get_sub_param(i, 4),
                params.get_sub_param(i, 5),
            ),
            _ => None,
        };
        return (color, 0);
    }
    let plain = |index| {
        (params.sub_params_len(index) == Some(1))
            .then(|| params.get(index))
            .flatten()
    };
    let consumed = match plain(i + 1) {
        Some(5) => 2,
        Some(2) => 4,
        _ => return (None, 1),
    };
    let color = match consumed {
        2 => plain(i + 2)
            .filter(|&n| n <= 255)
            .map(|n| Color::Indexed(n as u8)),
        _ => rgb_color(plain(i + 2), plain(i + 3), plain(i + 4)),
    };
    (color, consumed)
}

fn rgb_color(r: Option<usize>, g: Option<usize>, b: Option<usize>) -> Option<Color> {
    match (r, g, b) {
        (Some(r), Some(g), Some(b)) if r <= 255 && g <= 255 && b <= 255 => {
            Some(Color::Rgb(r as u8, g as u8, b as u8))
        }
        _ => None,
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
}

/// Character set single shift target for the immediate next graphic character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SingleShift {
    G2,
    G3,
}

/// Character set state for GL mapping via G0-G3 designation and SS2/SS3 single-shift.
///
/// `g0`, `g1`, `g2`, and `g3` hold the final character of the designation escape
/// (e.g. `b'B'` for US-ASCII, `b'0'` for DEC Special Graphics).
/// `active` selects which set (0 = G0, 1 = G1) maps GL characters.
/// `single_shift` temporarily selects G2 or G3 for the immediate next graphic character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CharacterSets {
    /// Most recently printed character (used by REP / CSI Ps b).
    pub(crate) last_char: Option<char>,
    /// G0 character set designation.
    pub(crate) g0: u8,
    /// G1 character set designation.
    pub(crate) g1: u8,
    /// G2 character set designation.
    pub(crate) g2: u8,
    /// G3 character set designation.
    pub(crate) g3: u8,
    /// Active charset: 0 = G0, 1 = G1.
    pub(crate) active: u8,
    /// Pending single-shift invocation (SS2/SS3) for the next graphic character.
    pub(crate) single_shift: Option<SingleShift>,
}

#[inline]
pub(crate) fn is_supported_charset(charset: u8) -> bool {
    matches!(charset, b'B' | b'0')
}

impl Default for CharacterSets {
    fn default() -> Self {
        Self {
            last_char: None,
            g0: b'B',
            g1: b'B',
            g2: b'B',
            g3: b'B',
            active: 0,
            single_shift: None,
        }
    }
}

/// Owns pen state, tab stops, character-set designations, and saved-pen snapshot.
#[derive(Debug, Clone)]
pub(crate) struct PenState {
    pub(crate) pen: Pen,
    pub(crate) tab_stops: TabStops,
    pub(crate) charsets: CharacterSets,
    pub(crate) active_hyperlink: Option<crate::model::HyperlinkId>,
    saved_pen: Option<SavedPen>,
}

impl PenState {
    pub(crate) fn new(cols: usize) -> Self {
        Self {
            pen: Pen::reset(),
            tab_stops: TabStops::new(cols),
            charsets: CharacterSets::default(),
            active_hyperlink: None,
            saved_pen: None,
        }
    }

    pub(crate) fn prepare_resize(
        &self,
        cols: usize,
    ) -> Result<Self, crate::primary_reflow::PreparationError> {
        use crate::primary_reflow::PreparationError;

        let mut stops = Vec::new();
        stops
            .try_reserve_exact(cols)
            .map_err(|_| PreparationError::AllocationFailed)?;
        stops.resize(cols, false);
        let retained = self.tab_stops.0.len().min(cols);
        stops[..retained].copy_from_slice(&self.tab_stops.0[..retained]);
        for (col, stop) in stops.iter_mut().enumerate().skip(retained) {
            if col % 8 == 0 {
                *stop = true;
            }
        }
        Ok(Self {
            pen: self.pen,
            tab_stops: TabStops(stops),
            charsets: self.charsets,
            active_hyperlink: self.active_hyperlink,
            saved_pen: self.saved_pen,
        })
    }

    /// Resets pen, charsets, tab-stops, and saved-pen snapshot to defaults (RIS).
    pub(crate) fn reset(&mut self, cols: usize) {
        self.pen = Pen::reset();
        self.charsets = CharacterSets::default();
        self.tab_stops = TabStops::new(cols);
        self.active_hyperlink = None;
        self.saved_pen = None;
    }

    /// Soft reset (DECSTR): resets pen, charsets.last_char, and charsets.single_shift,
    /// but leaves tab-stops and G0-G3 / active charset designations intact.
    pub(crate) fn soft_reset(&mut self) {
        self.pen = Pen::reset();
        self.charsets.last_char = None;
        self.charsets.single_shift = None;
        self.saved_pen = None;
    }

    /// Returns a blank cell tinted with the current SGR attributes, for erase ops.
    pub(crate) fn erase_cell(&self) -> Cell {
        Cell {
            ch: ' ',
            suffix: String::new(),
            width: 1,
            isolated_mark: false,
            wide_continuation: false,
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
            underline_color: self.pen.underline_color,
            protected: false,
            hyperlink: None,
        }
    }

    /// Saves the current pen colors + attributes (DECSC).
    pub(crate) fn save_pen(&mut self) {
        self.saved_pen = Some(SavedPen {
            hyperlink: self.active_hyperlink,
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
            underline_color: self.pen.underline_color,
            charsets: self.charsets,
        });
    }

    /// Restores the saved pen colors + attributes and character set designations (DECRC).
    pub(crate) fn restore_pen(&mut self) {
        if let Some(saved) = self.saved_pen {
            self.active_hyperlink = saved.hyperlink;
            self.pen.fg = saved.fg;
            self.pen.bg = saved.bg;
            self.pen.attrs = saved.attrs;
            self.pen.underline_color = saved.underline_color;
            self.charsets.g0 = saved.charsets.g0;
            self.charsets.g1 = saved.charsets.g1;
            self.charsets.g2 = saved.charsets.g2;
            self.charsets.g3 = saved.charsets.g3;
            self.charsets.active = saved.charsets.active;
            self.charsets.single_shift = None;
        }
    }

    pub(crate) fn hyperlink_ids(&self) -> impl Iterator<Item = crate::model::HyperlinkId> + '_ {
        [
            self.active_hyperlink,
            self.saved_pen.and_then(|saved| saved.hyperlink),
        ]
        .into_iter()
        .flatten()
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
                4 => {
                    let style = if sub_params_len == 1 {
                        Some(UnderlineStyle::Single)
                    } else if sub_params_len == 2 {
                        params
                            .get_sub_param(i, 1)
                            .and_then(UnderlineStyle::from_sgr)
                    } else {
                        None
                    };
                    if let Some(style) = style {
                        self.pen.attrs.set_underline_style(style);
                    }
                }
                58 => {
                    let (color, consumed) = underline_color(params, i);
                    if let Some(color) = color {
                        self.pen.underline_color = color;
                    }
                    i += consumed;
                }
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
                _ => crate::model::apply_scalar_sgr(
                    n,
                    &mut self.pen.fg,
                    &mut self.pen.bg,
                    &mut self.pen.attrs,
                    &mut self.pen.underline_color,
                ),
            }
            i += 1;
        }
    }

    pub(crate) fn set_sgr_slice(&mut self, slice: &[Option<usize>]) {
        self.set_sgr(&Params::from(slice));
    }

    // ── character sets ────────────────────────────────────────────

    pub(crate) fn designate_g0(&mut self, charset: u8) {
        if is_supported_charset(charset) {
            self.charsets.g0 = charset;
        }
    }

    pub(crate) fn designate_g1(&mut self, charset: u8) {
        if is_supported_charset(charset) {
            self.charsets.g1 = charset;
        }
    }

    pub(crate) fn designate_g2(&mut self, charset: u8) {
        if is_supported_charset(charset) {
            self.charsets.g2 = charset;
        }
    }

    pub(crate) fn designate_g3(&mut self, charset: u8) {
        if is_supported_charset(charset) {
            self.charsets.g3 = charset;
        }
    }

    pub(crate) fn single_shift_2(&mut self) {
        self.charsets.single_shift = Some(SingleShift::G2);
    }

    pub(crate) fn single_shift_3(&mut self) {
        self.charsets.single_shift = Some(SingleShift::G3);
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
            0 if cursor_x < self.tab_stops.0.len() => {
                self.tab_stops.0[cursor_x] = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_sgr_matches_cell_styling_but_preserves_pen_protection_and_hyperlinks() {
        let hyperlink =
            crate::model::HyperlinkId::from_nonzero(std::num::NonZeroU32::new(7).unwrap());
        for code in (0..=110).chain([usize::MAX]) {
            let mut cell = Cell {
                fg: Color::Rgb(1, 2, 3),
                bg: Color::Bright(5),
                underline_color: Color::Indexed(6),
                protected: true,
                hyperlink: Some(hyperlink),
                ..Cell::default()
            };
            cell.attrs.set(CellAttrs::BOLD | CellAttrs::CONCEAL);
            cell.attrs.set_underline_style(UnderlineStyle::Curly);
            let mut state = PenState::new(8);
            state.pen = Pen {
                fg: cell.fg,
                bg: cell.bg,
                attrs: cell.attrs,
                underline_color: cell.underline_color,
                protected: cell.protected,
            };
            state.active_hyperlink = cell.hyperlink;

            cell.apply_sgr(code);
            state.set_sgr_slice(&[Some(code)]);
            assert_eq!(
                (cell.fg, cell.bg, cell.attrs, cell.underline_color),
                (
                    state.pen.fg,
                    state.pen.bg,
                    state.pen.attrs,
                    state.pen.underline_color
                ),
                "SGR {code}",
            );
            assert!(state.pen.protected, "SGR {code} must not clear DECSCA");
            assert_eq!(cell.protected, code != 0);
            assert_eq!(cell.hyperlink, Some(hyperlink));
            assert_eq!(state.active_hyperlink, Some(hyperlink));
        }
    }
}
