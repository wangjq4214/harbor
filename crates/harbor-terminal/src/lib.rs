#![allow(unused_imports)]

mod damage;
mod normal_buf;
mod parser;
mod screen;
pub mod selection_model;
#[cfg(test)]
mod terminal_tests;

// Re-exports for the main crate.
pub use damage::DirtyRange;
pub use normal_buf::NormalBuf;
pub use parser::TerminalParser;
pub use screen::{AltScreenAction, Cell, CellAttrs, Color, CursorShape, Screen, SelectionBounds};
pub use selection_model::{
    AutoScroll, SelectionGranularity, SelectionModel, SelectionOutcome, SelectionRange,
};

pub use harbor_types::should_confirm_multiline;
pub use harbor_types::{
    CopySelectionResult, InputKey, InputModes, InputModifiers, InputRequest, PasteDisposition,
    RevisionedUpdateReceiver, TerminalCommand, TerminalSize, TerminalSnapshot, TerminalUpdate,
    UpdateDamage, WorkerStatus, safe_preview_line,
};

/// Stateful terminal model: a byte-stream parser plus the visible screen it mutates.
pub struct Terminal {
    /// Incremental ANSI/VT parser.
    parser: TerminalParser,
    /// Screen (primary buffer; alt screen handled internally via in_alt).
    normal: Screen,
    /// When true, `process_output` skips the scroll-to-bottom snap
    /// (active while the user is dragging a text selection).
    suppress_scroll_snap: bool,
}

impl Terminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            parser: TerminalParser::default(),
            normal: Screen::new(rows, cols),
            suppress_scroll_snap: false,
        }
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.normal.resize(rows, cols);
        self.suppress_scroll_snap = false;
    }

    pub fn put_str(&mut self, text: &str) {
        self.put_bytes(text.as_bytes());
    }

    /// Feeds raw PTY bytes through the streaming parser.
    ///
    /// This is the low-level parser ingestion path — no viewport side effects.
    /// Callers that want "scroll to bottom on new output" should use
    /// [`process_output`](Self::process_output) instead.
    pub fn put_bytes(&mut self, bytes: &[u8]) {
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
    pub fn process_output(&mut self, output: &[u8]) {
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
    pub fn screen(&self) -> &Screen {
        &self.normal
    }

    /// Returns the GPU-independent terminal state for the UI/update contract.
    pub fn snapshot(&self) -> TerminalSnapshot {
        self.normal.terminal_snapshot()
    }

    /// Mutable screen access for tests.
    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.normal
    }
    /// Resets the screen's dirty-row tracking (called after layers consume the dirt).
    pub fn clear_screen_dirty(&mut self) {
        self.normal.clear_dirty();
    }

    pub fn row_text(&self, row: usize) -> String {
        self.screen().row_text(row)
    }

    /// Resizes the terminal grid when the window surface changes.
    /// Returns `true` when dimensions actually changed.
    pub fn resize_terminal_if_changed(&mut self, new_size: TerminalSize) -> bool {
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
    pub fn scroll_viewport_up(&mut self, n: usize) {
        self.normal.scroll_up(n);
    }

    /// Scroll the viewport down by `n` rows (toward live content).
    pub fn scroll_viewport_down(&mut self, n: usize) {
        self.normal.scroll_down(n);
    }

    /// Snaps viewport to the oldest available scrollback row.
    pub fn scroll_viewport_to_top(&mut self) {
        let scroll_count = self.normal.scroll_count();
        self.normal.scroll_up(scroll_count);
    }

    /// Snaps viewport to live bottom.
    pub fn scroll_viewport_to_bottom(&mut self) {
        self.normal.scroll_to_bottom();
    }

    /// Returns `true` when the alternate screen is active.
    pub fn is_alt_screen(&self) -> bool {
        self.normal.is_alt()
    }

    /// Suppress or re-enable the scroll-to-bottom snap on PTY output (used during selection drag).
    pub fn set_suppress_scroll_snap(&mut self, suppress: bool) {
        self.suppress_scroll_snap = suppress;
    }
}
