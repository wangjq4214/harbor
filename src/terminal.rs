mod damage;
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
