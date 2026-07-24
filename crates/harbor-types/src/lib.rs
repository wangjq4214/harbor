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
}

/// Damage carried by a complete update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateDamage {
    /// The listed ranges are sufficient for an incremental renderer upload.
    Ranges(Vec<DirtyRange>),
    /// The renderer must upload the complete visible grid.
    FullUpload,
}
/// Logical key data sent to the terminal worker for mode-sensitive encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputKey {
    Character(String),
    Enter,
    Backspace,
    Tab,
    Escape,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowRight,
    ArrowLeft,
    Home,
    End,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Insert,
    Delete,
    PageUp,
    PageDown,
}

/// Modifier state captured by the UI and interpreted by the worker.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputModifiers(pub u8);

impl InputModifiers {
    pub const SHIFT: u8 = 1;
    pub const ALT: u8 = 2;
    pub const CONTROL: u8 = 4;
    pub const SUPER: u8 = 8;

    pub fn shift(self) -> bool {
        self.0 & Self::SHIFT != 0
    }

    pub fn alt(self) -> bool {
        self.0 & Self::ALT != 0
    }

    pub fn control(self) -> bool {
        self.0 & Self::CONTROL != 0
    }

    pub fn super_key(self) -> bool {
        self.0 & Self::SUPER != 0
    }
}

/// A logical keyboard event whose bytes are encoded against worker-owned modes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputRequest {
    pub key: InputKey,
    pub text: Option<String>,
    pub modifiers: InputModifiers,
    pub is_numpad: bool,
}

/// Result of an asynchronous selection-copy request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopySelectionResult {
    pub request_id: u64,
    pub text: String,
}

/// Commands accepted by the terminal worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalCommand {
    /// Raw PTY output. Bytes must be consumed in this exact order.
    PtyOutputBytes(Vec<u8>),
    /// Logical keyboard input; the worker encodes it using authoritative modes.
    Input(InputRequest),
    /// Raw paste text; the worker applies bracketed-paste framing from its modes.
    PasteText(String),
    Resize {
        request_id: u64,
        size: TerminalSize,
    },
    ScrollViewport {
        request_id: u64,
        rows: isize,
    },
    ScrollToTop {
        request_id: u64,
    },
    ScrollToBottom {
        request_id: u64,
    },
    SetSelectionDragActive(bool),
    CopySelection {
        request_id: u64,
        bounds: SelectionBounds,
    },
    RequestSnapshot {
        request_id: u64,
    },
    Shutdown,
}

/// Worker health visible to the UI without exposing worker internals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerStatus {
    Ready,
    Processing,
    Idle,
    Failed { message: String },
    Stopped,
}

/// Complete, revisioned state published by the terminal worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalUpdate {
    pub revision: u64,
    pub snapshot: TerminalSnapshot,
    pub damage: UpdateDamage,
    /// Snapshot-producing command request acknowledged by this update.
    pub acknowledged_request_id: Option<u64>,
}

impl TerminalUpdate {
    pub fn from_snapshot(revision: u64, snapshot: TerminalSnapshot) -> Self {
        Self::with_acknowledgement(revision, snapshot, None)
    }

    pub fn with_acknowledgement(
        revision: u64,
        snapshot: TerminalSnapshot,
        acknowledged_request_id: Option<u64>,
    ) -> Self {
        Self {
            revision,
            damage: UpdateDamage::Ranges(snapshot.dirty_ranges.clone()),
            snapshot,
            acknowledged_request_id,
        }
    }
}

/// Accepts monotonically increasing updates and converts revision gaps into
/// an explicit full-upload requirement.
#[derive(Debug, Default)]
pub struct RevisionedUpdateReceiver {
    last_revision: Option<u64>,
}

impl RevisionedUpdateReceiver {
    pub fn last_revision(&self) -> Option<u64> {
        self.last_revision
    }

    pub fn accept(&mut self, mut update: TerminalUpdate) -> Option<TerminalUpdate> {
        if self
            .last_revision
            .is_some_and(|last| update.revision <= last)
        {
            return None;
        }
        if self
            .last_revision
            .is_some_and(|last| update.revision > last.saturating_add(1))
        {
            update.damage = UpdateDamage::FullUpload;
        }
        self.last_revision = Some(update.revision);
        Some(update)
    }
}

#[cfg(test)]
mod worker_contract_tests {
    use super::*;

    fn snapshot(dirty_ranges: Vec<DirtyRange>) -> TerminalSnapshot {
        TerminalSnapshot {
            rows: 1,
            cols: 2,
            cells: vec![Cell::default(); 2],
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            cursor_blink: false,
            cursor_shape: CursorShape::default(),
            scroll_count: 0,
            view_offset: 0,
            history_start: 0,
            is_alt: false,
            input_modes: InputModes::default(),
            dirty_ranges,
        }
    }

    #[test]
    fn stale_updates_cannot_replace_newer_state() {
        let mut receiver = RevisionedUpdateReceiver::default();
        let current = TerminalUpdate::from_snapshot(2, snapshot(Vec::new()));
        assert!(receiver.accept(current).is_some());
        assert!(
            receiver
                .accept(TerminalUpdate::from_snapshot(1, snapshot(Vec::new())))
                .is_none()
        );
        assert_eq!(receiver.last_revision(), Some(2));
    }

    #[test]
    fn skipped_revision_requires_full_upload() {
        let mut receiver = RevisionedUpdateReceiver::default();
        assert!(
            receiver
                .accept(TerminalUpdate::from_snapshot(
                    4,
                    snapshot(vec![DirtyRange {
                        row: 0,
                        start_col: 1,
                        end_col: 2,
                    }])
                ))
                .is_some()
        );

        let accepted = receiver
            .accept(TerminalUpdate::from_snapshot(
                6,
                snapshot(vec![DirtyRange {
                    row: 0,
                    start_col: 0,
                    end_col: 1,
                }]),
            ))
            .expect("new revision is accepted");
        assert_eq!(accepted.damage, UpdateDamage::FullUpload);
    }

    #[test]
    fn output_command_preserves_raw_ordered_bytes() {
        let commands = [
            TerminalCommand::PtyOutputBytes(vec![0x1b, b'[']),
            TerminalCommand::PtyOutputBytes(vec![b'2', b'J']),
        ];
        let mut bytes = Vec::new();
        for command in commands {
            let TerminalCommand::PtyOutputBytes(chunk) = command else {
                unreachable!("test only contains output commands");
            };
            bytes.extend(chunk);
        }
        assert_eq!(bytes, b"\x1b[2J");
    }
}
