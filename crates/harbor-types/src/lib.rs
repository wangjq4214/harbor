//! Shared domain types for the Harbor terminal emulator.
//!
//! Zero-dependency crate. Pure data types used across terminal, pty, render, and app layers.

use std::borrow::Cow;

// ── Render values ────────────────────────────────────────────────────────────

/// Backend-neutral RGBA color used by UI and renderer commands.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RgbaColor(pub [f32; 4]);

impl RgbaColor {
    pub const BLACK: Self = Self([0.0, 0.0, 0.0, 1.0]);
    pub const WHITE: Self = Self([1.0, 1.0, 1.0, 1.0]);
}

/// Axis-aligned logical rectangle shared by UI layout and renderer commands.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn intersect(self, other: Self) -> Self {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        Self {
            x: left,
            y: top,
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        }
    }
}

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
}

// ── CursorShape ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    #[default]
    Bar,
}

// ── SelectionBounds ───────────────────────────────────────────────────────────

/// Display-coordinate bounds of a text selection, row-major, inclusive.
/// `start_row` / `end_row` are **generations** (stable scrollback coordinates).
#[derive(Clone, Copy, Debug)]
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

/// Lightweight snapshot of terminal modes that affect input encoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputModes {
    pub application_cursor: bool,
    pub application_keypad: bool,
    pub bracketed_paste: bool,
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
    Enter,
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

// ── RenderSnapshot ─────────────────────────────────────────────────────────────

/// Owned read-only projection of terminal state for rendering.
///
/// Snapshot from [`Screen`] that the GPU layers consume without depending on
/// the full terminal model. All visible cells are copied once per frame.
pub struct RenderSnapshot {
    /// Visible grid rows.
    pub rows: usize,
    /// Visible grid columns.
    pub cols: usize,
    /// Flattened visible cells: `cells[row * cols + col]`.
    pub cells: Vec<Cell>,
    /// Cursor column (0-based).
    pub cursor_x: usize,
    /// Cursor row (0-based display row; set to `rows` when scrolled back).
    pub cursor_y: usize,
    /// Whether the cursor should be drawn.
    pub cursor_visible: bool,
    /// Whether the cursor blinks.
    pub cursor_blink: bool,
    /// Cursor visual style.
    pub cursor_shape: CursorShape,
    /// Total scrollback rows retained.
    pub scroll_count: usize,
    /// Viewport offset from live bottom (0 = at bottom).
    pub view_offset: usize,
    /// Monotonically increasing base generation for scrollback coordinate space.
    pub history_start: u64,
    /// True when alternate screen is active.
    pub is_alt: bool,
    /// Damaged cell ranges for incremental upload.
    pub dirty_ranges: Vec<DirtyRange>,
}

impl RenderSnapshot {
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
}
