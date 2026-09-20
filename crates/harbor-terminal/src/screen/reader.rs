//! Read-only queries over terminal screen state.
//!
//! `ScreenReader` takes a shared reference to `Screen` and produces
//! snapshots and extracted text without mutation. This separates
//! the read path from the mutation methods on `Screen`.

use crate::content_anchor::ContentProjection;
use crate::logical_content::DecodeError;
use crate::model::{SelectionBounds, TerminalSnapshot};

use super::Screen;

/// Façade for read-only screen queries — snapshots, text extraction, etc.
///
/// Created via `Screen::reader()`. All methods are `&self` (no mutation).
pub struct ScreenReader<'a> {
    screen: &'a Screen,
}

impl<'a> ScreenReader<'a> {
    pub(crate) fn new(screen: &'a Screen) -> Self {
        Self { screen }
    }

    /// Builds a full `TerminalSnapshot` for the UI/update contract.
    pub fn terminal_snapshot(&self) -> TerminalSnapshot {
        let rows = self.screen.rows();
        let cols = self.screen.cols();
        let mut cells = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                cells.push(*self.screen.cell(r, c));
            }
        }
        TerminalSnapshot {
            rows,
            cols,
            cells,
            cursor_x: self.screen.cursor_x(),
            cursor_y: self.screen.cursor_y(),
            cursor_visible: self.screen.cursor_visible(),
            cursor_blink: self.screen.cursor_blink(),
            cursor_shape: self.screen.cursor_shape(),
            scroll_count: self.screen.scroll_count(),
            view_offset: self.screen.view_offset(),
            history_start: self.screen.history_start(),
            wrapped: (0..rows).map(|row| self.screen.is_wrapped(row)).collect(),
            is_alt: self.screen.is_alt(),
            input_modes: self.screen.input_modes(),
            dirty_ranges: self.screen.dirty_ranges(),
        }
    }

    pub(crate) fn content_projection(&self) -> Result<ContentProjection, DecodeError> {
        self.screen.content_projection()
    }

    /// Extracts logical text between two inclusive generation/column coordinates.
    pub fn selected_text(&self, bounds: SelectionBounds) -> String {
        match crate::logical_content::selected_text(&self.screen.normal, bounds) {
            Ok(text) => text,
            Err(error) => {
                tracing::error!(generation = error.generation, column = error.column, kind = ?error.kind, "selected text decode failed");
                String::new()
            }
        }
    }
}
