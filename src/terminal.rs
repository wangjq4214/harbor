mod damage;
use crate::pty::Pty;
mod normal_buf;
mod parser;
mod screen;
#[cfg(test)]
mod tests;

pub(crate) use damage::DirtyRange;
pub(crate) use normal_buf::NormalBuf;
use parser::TerminalParser;
pub(crate) use screen::AltScreenAction;
pub(crate) use screen::CellAttrs;
pub(crate) use screen::Color;
pub(crate) use screen::CursorShape;
pub(crate) use screen::Screen;
pub(crate) use screen::SelectionBounds;
use std::borrow::Cow;

/// Terminal dimensions in character cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSize {
    /// Number of visible terminal rows.
    pub(crate) rows: usize,
    /// Number of visible terminal columns.
    pub(crate) cols: usize,
}

/// Lightweight snapshot of terminal modes that affect input encoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct InputModes {
    pub(crate) application_cursor: bool,
    pub(crate) application_keypad: bool,
    pub(crate) bracketed_paste: bool,
}

impl InputModes {
    /// Returns raw pasted bytes, framed with bracketed-paste markers when enabled.
    pub(crate) fn paste<'a>(&self, text: &'a [u8]) -> Cow<'a, [u8]> {
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

/// Disposition of a paste operation after checking multi-line and bracketed-paste state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PasteDisposition {
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
    pub(crate) fn decide(modes: InputModes, text: &str) -> Self {
        if modes.bracketed_paste || !should_confirm_multiline(text) {
            PasteDisposition::SendDirect
        } else {
            PasteDisposition::Confirm {
                raw_text: text.to_owned(),
            }
        }
    }
}

/// Returns `true` when `text` contains at least one newline after recursively
/// trimming all trailing newline sequences (`\r\n`, `\n`, `\r`).
///
/// Single-line text (with or without trailing newlines) and text that becomes
/// empty after trimming are not multi-line.
pub(crate) fn should_confirm_multiline(text: &str) -> bool {
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
pub(crate) fn safe_preview_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '\t' => out.push('\u{2192}'),              // →
            // LF and CR pass through (caller splits lines, not us)
            '\n' | '\r' => out.push(ch),
            c if (c as u32) <= 0x1F => {
                out.push(char::from_u32(0x2400 + c as u32).unwrap_or('?'));
            }
            '\x7F' => out.push('\u{2421}'),            // ␡
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

/// Delivers paste text to the PTY, framing with bracketed-paste markers when
/// the mode is enabled.  This is the single entry point for paste delivery;
/// callers should not reach for `Pty::write` directly for paste payloads.
pub(crate) fn send_paste(modes: InputModes, text: &str, pty: &mut Pty) {
    pty.write(&modes.paste(text.as_bytes()));
}

/// Stateful terminal model: a byte-stream parser plus the visible screen it mutates.
pub(crate) struct Terminal {
    /// Incremental ANSI/VT parser.
    parser: TerminalParser,
    /// Screen (primary buffer; alt screen handled internally via in_alt).
    normal: Screen,
    /// When true, `process_output` skips the scroll-to-bottom snap
    /// (active while the user is dragging a text selection).
    suppress_scroll_snap: bool,
}

impl Terminal {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        Self {
            parser: TerminalParser::default(),
            normal: Screen::new(rows, cols),
            suppress_scroll_snap: false,
        }
    }

    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        self.normal.resize(rows, cols);
        self.suppress_scroll_snap = false;
    }

    #[cfg(test)]
    pub(crate) fn put_str(&mut self, text: &str) {
        self.put_bytes(text.as_bytes());
    }

    /// Feeds raw PTY bytes through the streaming parser.
    ///
    /// This is the low-level parser ingestion path — no viewport side effects.
    /// Callers that want "scroll to bottom on new output" should use
    /// [`process_output`](Self::process_output) instead.
    pub(crate) fn put_bytes(&mut self, bytes: &[u8]) {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let result = {
                let parser = &mut self.parser;
                parser.put_bytes(&mut self.normal, remaining)
            };
            remaining = &remaining[result.consumed..];
            if let Some(action) = result.alt_request {
                // Consume the alt request flag and handle it.
                self.normal.take_alt_request();
                // Clear selection-drag suppression so normal-mode output
                // always snaps to bottom after any alt-screen transition.
                self.suppress_scroll_snap = false;
                match action {
                    AltScreenAction::Enter => self.normal.enter_alt(),
                    AltScreenAction::Exit => self.normal.exit_alt(),
                }
            }
        }
    }

    /// Feeds raw PTY bytes into the terminal parser (render refresh handled by caller).
    pub(crate) fn process_output(&mut self, output: &[u8]) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }
        // Snap back to live bottom on new output unless in alt screen or dragging a selection.
        if !self.normal.is_alt() && !self.suppress_scroll_snap {
            self.normal.scroll_to_bottom();
        }
        // Feed bytes into the terminal parser (updates screen cells and cursor).
        self.put_bytes(output);
    }

    /// Returns the renderable screen snapshot owned by this terminal.
    pub(crate) fn screen(&self) -> &Screen {
        &self.normal
    }

    /// Mutable screen access for tests.
    #[cfg(test)]
    pub(crate) fn screen_mut(&mut self) -> &mut Screen {
        &mut self.normal
    }
    /// Resets the screen's dirty-row tracking (called after layers consume the dirt).
    pub(crate) fn clear_screen_dirty(&mut self) {
        self.normal.clear_dirty();
    }

    #[cfg(test)]
    pub(crate) fn row_text(&self, row: usize) -> String {
        self.screen().row_text(row)
    }

    /// Resizes the terminal grid when the window surface changes.
    /// Returns `true` when dimensions actually changed.
    pub(crate) fn resize_terminal_if_changed(&mut self, new_size: TerminalSize) -> bool {
        let current = TerminalSize {
            rows: self.screen().rows(),
            cols: self.screen().cols(),
        };

        if new_size != current {
            self.resize(new_size.rows, new_size.cols);
            true
        } else {
            false
        }
    }
    pub(crate) fn scroll_viewport_up(&mut self, n: usize) {
        self.normal.scroll_up(n);
    }

    /// Scroll the viewport down by `n` rows (toward live content).
    pub(crate) fn scroll_viewport_down(&mut self, n: usize) {
        self.normal.scroll_down(n);
    }

    /// Snaps viewport to the oldest available scrollback row.
    pub(crate) fn scroll_viewport_to_top(&mut self) {
        let scroll_count = self.normal.scroll_count();
        self.normal.scroll_up(scroll_count);
    }

    /// Snaps viewport to live bottom.
    pub(crate) fn scroll_viewport_to_bottom(&mut self) {
        self.normal.scroll_to_bottom();
    }

    /// Returns `true` when the alternate screen is active.
    pub(crate) fn is_alt_screen(&self) -> bool {
        self.normal.is_alt()
    }

    /// Suppress or re-enable the scroll-to-bottom snap on PTY output (used during selection drag).
    pub(crate) fn set_suppress_scroll_snap(&mut self, suppress: bool) {
        self.suppress_scroll_snap = suppress;
    }
}
