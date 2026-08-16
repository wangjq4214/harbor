//! Shared domain types for the Harbor terminal emulator.
//!
//! Zero-dependency crate. Pure data types used across terminal, pty, render, and app layers.

use std::borrow::Cow;

// ── Color ────────────────────────────────────────────────────────────────────

/// Terminal color value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
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
    pub fn to_rgba(self) -> [f32; 4] {
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

// ── CellAttrs ─────────────────────────────────────────────────────────────────

/// Text style attributes, stored as a compact bitset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellAttrs(u8);

impl CellAttrs {
    pub const BOLD: u8 = 1 << 0;
    pub const DIM: u8 = 1 << 1;
    pub const ITALIC: u8 = 1 << 2;
    pub const UNDERLINE: u8 = 1 << 3;
    pub const BLINK: u8 = 1 << 4;
    pub const INVERSE: u8 = 1 << 5;
    pub const STRIKETHROUGH: u8 = 1 << 6;

    #[allow(dead_code)]
    pub fn contains(self, bits: u8) -> bool {
        self.0 & bits != 0
    }
    pub fn set(&mut self, bits: u8) {
        self.0 |= bits;
    }
    pub fn toggle(&mut self, bits: u8) {
        self.0 ^= bits;
    }
    pub fn clear(&mut self, bits: u8) {
        self.0 &= !bits;
    }
    #[allow(dead_code)]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

// ── Cell ──────────────────────────────────────────────────────────────────────

/// One visible terminal grid cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    /// Character currently displayed in this cell.
    pub ch: char,
    /// True when this cell is the hidden trailing half of a double-width character.
    pub wide_continuation: bool,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Text style attributes.
    pub attrs: CellAttrs,
    /// True if this character is protected against selective erasure (DECSCA).
    pub protected: bool,
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
    pub fn set(&mut self, ch: char, fg: Color, bg: Color, attrs: CellAttrs, protected: bool) {
        self.ch = ch;
        self.wide_continuation = false;
        self.fg = fg;
        self.bg = bg;
        self.attrs = attrs;
        self.protected = protected;
    }

    /// Applies a single SGR (Select Graphic Rendition) code to this cell,
    /// mutating foreground, background, attributes, and protection in place.
    pub fn apply_sgr(&mut self, code: usize) {
        match code {
            0 => {
                self.fg = Color::Default;
                self.bg = Color::Default;
                self.attrs = CellAttrs::default();
                self.protected = false;
            }
            1 => self.attrs.set(CellAttrs::BOLD),
            2 => self.attrs.set(CellAttrs::DIM),
            3 => self.attrs.set(CellAttrs::ITALIC),
            4 => self.attrs.set(CellAttrs::UNDERLINE),
            5 => self.attrs.set(CellAttrs::BLINK),
            7 => self.attrs.set(CellAttrs::INVERSE),
            9 => self.attrs.set(CellAttrs::STRIKETHROUGH),
            22 => self.attrs.clear(CellAttrs::BOLD | CellAttrs::DIM),
            23 => self.attrs.clear(CellAttrs::ITALIC),
            24 => self.attrs.clear(CellAttrs::UNDERLINE),
            25 => self.attrs.clear(CellAttrs::BLINK),
            27 => self.attrs.clear(CellAttrs::INVERSE),
            29 => self.attrs.clear(CellAttrs::STRIKETHROUGH),
            30..=37 => self.fg = Color::Named((code - 30) as u8),
            40..=47 => self.bg = Color::Named((code - 40) as u8),
            39 => self.fg = Color::Default,
            49 => self.bg = Color::Default,
            90..=97 => self.fg = Color::Bright((code - 90) as u8),
            100..=107 => self.bg = Color::Bright((code - 100) as u8),
            _ => {}
        }
    }

    /// Toggles a single SGR attribute on this cell (used by DECRARA).
    /// Only supports attribute codes (bold, dim, italic, underline, blink,
    /// inverse, strikethrough); color codes are silently ignored.
    pub fn toggle_sgr(&mut self, code: usize) {
        match code {
            1 => self.attrs.toggle(CellAttrs::BOLD),
            2 => self.attrs.toggle(CellAttrs::DIM),
            3 => self.attrs.toggle(CellAttrs::ITALIC),
            4 => self.attrs.toggle(CellAttrs::UNDERLINE),
            5 => self.attrs.toggle(CellAttrs::BLINK),
            7 => self.attrs.toggle(CellAttrs::INVERSE),
            9 => self.attrs.toggle(CellAttrs::STRIKETHROUGH),
            _ => {}
        }
    }
}

// ── CursorShape ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

/// Parameter for DECSCUSR (CSI Ps SP q) — cursor style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorStyleArg {
    /// Ps = 0 or 1: blinking block (default).
    #[default]
    BlinkingBlock,
    /// Ps = 2: steady (non-blinking) block.
    SteadyBlock,
    /// Ps = 3: blinking underline.
    BlinkingUnderline,
    /// Ps = 4: steady underline.
    SteadyUnderline,
    /// Ps = 5: blinking bar.
    BlinkingBar,
    /// Ps = 6: steady bar.
    SteadyBar,
}

impl CursorStyleArg {
    /// Convert from a DECSCUSR Ps parameter, falling back to `BlinkingBlock`.
    pub fn from_param(ps: usize) -> Self {
        match ps {
            0 | 1 => Self::BlinkingBlock,
            2 => Self::SteadyBlock,
            3 => Self::BlinkingUnderline,
            4 => Self::SteadyUnderline,
            5 => Self::BlinkingBar,
            6 => Self::SteadyBar,
            _ => Self::default(),
        }
    }
}

/// Parameter for DECSCA (CSI Ps " q) — character protection mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CharacterProtection {
    /// Ps = 0 or 2: subsequent characters are NOT protected.
    #[default]
    Unprotected,
    /// Ps = 1: subsequent characters are protected from selective erase.
    Protected,
}

impl CharacterProtection {
    /// Convert from a DECSCA Ps parameter, falling back to `Unprotected`.
    pub fn from_param(ps: usize) -> Self {
        match ps {
            1 => Self::Protected,
            _ => Self::Unprotected,
        }
    }
}

// ── SelectionBounds ───────────────────────────────────────────────────────────

/// Display-coordinate bounds of a text selection, row-major, inclusive.
/// `start_row` / `end_row` are **generations** (stable scrollback coordinates).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionBounds {
    pub start_row: u64,
    pub start_col: usize,
    pub end_row: u64,
    pub end_col: usize,
}

// ── DirtyRange ────────────────────────────────────────────────────────────────

/// Dirty cell range in character-cell coordinate space.
///
/// `start_col` is inclusive, `end_col` is exclusive: `[start_col, end_col)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRange {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
}

// ── TerminalSize ──────────────────────────────────────────────────────────────

/// Terminal dimensions in character cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    /// Number of visible terminal rows.
    pub rows: usize,
    /// Number of visible terminal columns.
    pub cols: usize,
}

// ── InputModes ────────────────────────────────────────────────────────────────

/// VT mouse-reporting mode selected by DEC private modes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MouseTrackingMode {
    #[default]
    Disabled,
    Button,
    ButtonMotion,
    AnyMotion,
}

/// Lightweight snapshot of terminal modes that affect input encoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputModes {
    pub application_cursor: bool,
    pub application_keypad: bool,
    pub bracketed_paste: bool,
    pub mouse_tracking: MouseTrackingMode,
    pub mouse_sgr: bool,
}

impl InputModes {
    /// Returns raw pasted bytes, framed with bracketed-paste markers when enabled.
    pub fn paste<'a>(&self, text: &'a [u8]) -> Cow<'a, [u8]> {
        if !self.bracketed_paste {
            return Cow::Borrowed(text);
        }
        let mut bytes = Vec::with_capacity(text.len() + b"\x1b[200~".len() + b"\x1b[201~".len());
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(text);
        bytes.extend_from_slice(b"\x1b[201~");
        Cow::Owned(bytes)
    }
}

// ── PasteDisposition ──────────────────────────────────────────────────────────

/// Disposition of a paste operation after checking multi-line and bracketed-paste state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PasteDisposition {
    /// Send directly to PTY — no confirmation needed.
    SendDirect,
    /// Confirmation required; holds the raw paste text.
    Confirm { raw_text: String },
}

impl PasteDisposition {
    /// Determines the paste disposition from InputModes and clipboard text.
    ///
    /// Returns `SendDirect` when bracketed paste is ON or the text has at most
    /// one meaningful line (after trimming trailing newlines). Returns `Confirm`
    /// when bracketed paste is OFF and the text contains real newlines.
    pub fn decide(modes: InputModes, text: &str) -> Self {
        if modes.bracketed_paste || !should_confirm_multiline(text) {
            PasteDisposition::SendDirect
        } else {
            PasteDisposition::Confirm {
                raw_text: text.to_owned(),
            }
        }
    }
}

// ── AltScreenAction ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AltScreenAction {
    Enter { clear: bool },
    Exit,
}

// ── Utility functions ─────────────────────────────────────────────────────────

/// Returns `true` when `text` contains at least one newline after recursively
/// trimming all trailing newline sequences (`\r\n`, `\n`, `\r`).
///
/// Single-line text (with or without trailing newlines) and text that becomes
/// empty after trimming are not multi-line.
pub fn should_confirm_multiline(text: &str) -> bool {
    let trimmed = trim_trailing_newlines(text);
    trimmed.contains('\n') || trimmed.contains('\r')
}

/// Escapes C0 control characters (U+0000–U+001F) and DEL (U+007F) in a single
/// line to visible Unicode markers. Tab is rendered as `→`. All other C0 chars
/// use the Unicode Control Pictures block (U+2400 + byte value). DEL uses U+2421.
///
/// LF (`\n`) and CR (`\r`) pass through unchanged — the caller is responsible
/// for line splitting; this function receives individual lines that should not
/// contain line-break characters.
pub fn safe_preview_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '\t' => out.push('\u{2192}'), // →
            // LF and CR pass through (caller splits lines, not us)
            '\n' | '\r' => out.push(ch),
            c if (c as u32) <= 0x1F => {
                out.push(char::from_u32(0x2400 + c as u32).unwrap_or('?'));
            }
            '\x7F' => out.push('\u{2421}'), // ␡
            other => out.push(other),
        }
    }
    out
}

/// Trims any trailing newline sequences (`\r\n`, `\n`, `\r`) from the input,
/// returning the remaining prefix.
fn trim_trailing_newlines(text: &str) -> &str {
    let mut end = text.len();
    loop {
        if end >= 2 && text.as_bytes()[end - 2..end] == *b"\r\n" {
            end -= 2;
        } else if end >= 1 {
            let last = text.as_bytes()[end - 1];
            if last == b'\n' || last == b'\r' {
                end -= 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    &text[..end]
}

// ── Terminal worker contract ────────────────────────────────────────────────

/// Complete terminal state exchanged between the terminal model and the UI.
///
/// This is intentionally a domain snapshot. It contains no GPU handles, UVs,
/// or buffer offsets; the renderer derives its own projection from this data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalSnapshot {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<Cell>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub cursor_visible: bool,
    pub cursor_blink: bool,
    pub cursor_shape: CursorShape,
    pub scroll_count: usize,
    pub view_offset: usize,
    pub history_start: u64,
    /// Soft-wrap marker for each currently displayed row.
    pub wrapped: Vec<bool>,
    pub is_alt: bool,
    pub input_modes: InputModes,
    pub dirty_ranges: Vec<DirtyRange>,
}

impl TerminalSnapshot {
    /// Returns a reference to the cell at `(row, col)` in display coordinates.
    #[inline]
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        &self.cells[row * self.cols + col]
    }

    /// Returns the character at `(row, col)` in display coordinates.
    #[inline]
    pub fn cell_char(&self, row: usize, col: usize) -> char {
        self.cells[row * self.cols + col].ch
    }

    /// Returns the cell at the given scrollback generation and column.
    #[inline]
    pub fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
        let visible_start =
            self.history_start + self.scroll_count.saturating_sub(self.view_offset) as u64;
        let row = generation.checked_sub(visible_start)? as usize;
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.cells.get(row * self.cols + col)
    }

    /// Returns whether a visible generation continues the logical line above.
    pub fn is_wrapped_at_generation(&self, generation: u64) -> bool {
        let visible_start =
            self.history_start + self.scroll_count.saturating_sub(self.view_offset) as u64;
        let row = generation
            .checked_sub(visible_start)
            .map(|row| row as usize);
        row.and_then(|row| self.wrapped.get(row))
            .copied()
            .unwrap_or(false)
    }
}

/// Damage carried by a complete update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateDamage {
    /// The listed ranges are sufficient for an incremental renderer upload.
    Ranges(Vec<DirtyRange>),
    /// The renderer must upload the complete visible grid.
    FullUpload,
}

#[cfg(test)]
mod tests {
    use super::{CursorShape, CursorStyleArg};

    #[test]
    fn cursor_shape_default_is_block() {
        assert_eq!(CursorShape::default(), CursorShape::Block);
    }

    #[test]
    fn cursor_style_from_param_maps_canonical_values() {
        assert_eq!(CursorStyleArg::from_param(0), CursorStyleArg::BlinkingBlock);
        assert_eq!(CursorStyleArg::from_param(1), CursorStyleArg::BlinkingBlock);
        assert_eq!(CursorStyleArg::from_param(2), CursorStyleArg::SteadyBlock);
        assert_eq!(
            CursorStyleArg::from_param(3),
            CursorStyleArg::BlinkingUnderline
        );
        assert_eq!(
            CursorStyleArg::from_param(4),
            CursorStyleArg::SteadyUnderline
        );
        assert_eq!(CursorStyleArg::from_param(5), CursorStyleArg::BlinkingBar);
        assert_eq!(CursorStyleArg::from_param(6), CursorStyleArg::SteadyBar);
        assert_eq!(
            CursorStyleArg::from_param(99),
            CursorStyleArg::BlinkingBlock
        );
    }
}
