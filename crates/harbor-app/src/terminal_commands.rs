//! Host-owned terminal operation reduction, shared by every command source.
use crate::{command::AppCommand, tab_manager::TabManager};
use harbor_terminal::TerminalEventOutcome;

impl TabManager {
    /// Effects still belong to the native Host; this performs no clipboard/window I/O.
    pub fn dispatch_terminal_command(
        &self,
        command: AppCommand,
    ) -> Result<TerminalEventOutcome, &'static str> {
        let terminal = self.active_terminal().ok_or("No active terminal")?;
        let mut terminal = terminal.lock().map_err(|_| "Terminal lock unavailable")?;
        match command {
            AppCommand::Copy | AppCommand::CopyOrInterrupt => Ok(terminal.command_copy_selection()),
            AppCommand::PageUp => Ok(terminal.command_page_up()),
            AppCommand::PageDown => Ok(terminal.command_page_down()),
            AppCommand::ScrollToTop => Ok(terminal.command_scroll_to_top()),
            AppCommand::ScrollToBottom => Ok(terminal.command_scroll_to_bottom()),
            _ => Err("Not a terminal operation"),
        }
    }
}
