//! Terminal/UI boundary used by the synchronous U1 backend.
//!
//! The adapter keeps the current `Terminal` and `Pty` ownership unchanged while
//! exposing the same narrow capability boundary that a future worker backend
//! will implement.

use crate::pty::Pty;
use harbor_render::{PtyFacade, TerminalFacade};
use harbor_terminal::Terminal;
use harbor_types::TerminalView;

pub(crate) struct SyncUiFacade<'a> {
    terminal: &'a mut Terminal,
    pty: &'a mut Pty,
}

pub(crate) enum SyncCommandResponse {
    None,
    Snapshot(harbor_types::TerminalSnapshot),
    SelectedText(String),
    ShutdownRequested,
}

impl<'a> SyncUiFacade<'a> {
    pub(crate) fn new(terminal: &'a mut Terminal, pty: &'a mut Pty) -> Self {
        Self { terminal, pty }
    }
    pub(crate) fn dispatch(
        &mut self,
        command: harbor_types::TerminalCommand,
    ) -> SyncCommandResponse {
        use harbor_types::TerminalCommand;

        match command {
            TerminalCommand::PtyOutputBytes(bytes) => {
                self.terminal.process_output(&bytes);
                SyncCommandResponse::None
            }
            TerminalCommand::WritePtyInput(bytes) => {
                self.pty.write(&bytes);
                SyncCommandResponse::None
            }
            TerminalCommand::Resize(size) => {
                self.resize(size);
                SyncCommandResponse::None
            }
            TerminalCommand::ScrollViewport { rows } if rows >= 0 => {
                self.terminal.scroll_viewport_down(rows as usize);
                SyncCommandResponse::None
            }
            TerminalCommand::ScrollViewport { rows } => {
                self.terminal.scroll_viewport_up(rows.unsigned_abs());
                SyncCommandResponse::None
            }
            TerminalCommand::ScrollToTop => {
                self.terminal.scroll_viewport_to_top();
                SyncCommandResponse::None
            }
            TerminalCommand::ScrollToBottom => {
                self.terminal.scroll_viewport_to_bottom();
                SyncCommandResponse::None
            }
            TerminalCommand::SetSelectionDragActive(active) => {
                self.terminal.set_suppress_scroll_snap(active);
                SyncCommandResponse::None
            }
            TerminalCommand::CopySelection { bounds, .. } => {
                SyncCommandResponse::SelectedText(self.terminal.screen().selected_text(bounds))
            }
            TerminalCommand::RequestSnapshot { .. } => {
                SyncCommandResponse::Snapshot(self.terminal.snapshot())
            }
            TerminalCommand::Shutdown => SyncCommandResponse::ShutdownRequested,
        }
    }

    pub(crate) fn resize(&mut self, size: harbor_types::TerminalSize) -> bool {
        let changed = self.terminal.resize_terminal_if_changed(size);
        if changed {
            self.pty.resize(size);
        }
        changed
    }

    pub(crate) fn write_input(&mut self, bytes: &[u8]) {
        self.pty.write(bytes);
    }
}

impl TerminalFacade for SyncUiFacade<'_> {
    fn view(&self) -> &dyn TerminalView {
        self.terminal.screen()
    }

    fn render_snapshot(&self) -> harbor_types::RenderSnapshot {
        self.terminal.screen().snapshot()
    }

    fn selected_text(&self, bounds: harbor_types::SelectionBounds) -> String {
        self.terminal.screen().selected_text(bounds)
    }

    fn scroll_viewport_up(&mut self, n: usize) {
        self.terminal.scroll_viewport_up(n);
    }

    fn scroll_viewport_down(&mut self, n: usize) {
        self.terminal.scroll_viewport_down(n);
    }

    fn scroll_viewport_to_top(&mut self) {
        self.terminal.scroll_viewport_to_top();
    }

    fn scroll_viewport_to_bottom(&mut self) {
        self.terminal.scroll_viewport_to_bottom();
    }

    fn set_suppress_scroll_snap(&mut self, active: bool) {
        self.terminal.set_suppress_scroll_snap(active);
    }

    fn is_alt_screen(&self) -> bool {
        self.terminal.is_alt_screen()
    }
}

impl PtyFacade for SyncUiFacade<'_> {
    fn write(&mut self, bytes: &[u8]) {
        self.pty.write(bytes);
    }
}
