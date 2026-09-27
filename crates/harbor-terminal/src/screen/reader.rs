//! Read-only queries over terminal screen state.

use super::Screen;
use crate::model::{SelectionBounds, TerminalSnapshot};

/// Façade for read-only screen queries — snapshots, text extraction, etc.
#[derive(Clone, Copy, Debug)]
pub struct ScreenReader<'a> {
    screen: &'a Screen,
}

impl<'a> ScreenReader<'a> {
    pub(crate) const fn new(screen: &'a Screen) -> Self {
        Self { screen }
    }

    /// Builds a full `TerminalSnapshot` for the UI/update contract.
    pub fn terminal_snapshot(&self) -> TerminalSnapshot {
        self.screen.terminal_snapshot()
    }

    /// Extracts logical text between two inclusive generation/column coordinates.
    pub fn selected_text(&self, bounds: SelectionBounds) -> String {
        self.screen.selected_text(bounds)
    }
}
