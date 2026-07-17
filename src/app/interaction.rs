use std::time::Instant;

use super::{
    EventResult,
    caps::{
        CursorContext, CursorInput, CursorWaitContext, ScrollbarContext, ScrollbarInput,
        ScrollbarWaitContext, SelectionContext, SelectionInput, SelectionWaitContext,
    },
    cursor::Cursor,
    scrollbar::Scrollbar,
    selection::Selection,
};
use harbor_pty::Pty;
use harbor_terminal::Terminal;
use harbor_ui::TerminalVisualState;
use winit::{event::WindowEvent, keyboard::ModifiersState, window::Window};

/// Shell-owned terminal interaction state and event dispatch.
pub(crate) struct TerminalInteraction {
    selection: Selection,
    cursor: Cursor,
    scrollbar: Scrollbar,
}

impl TerminalInteraction {
    pub(crate) fn new(cell_width: f32, line_height: f32) -> Self {
        Self {
            selection: Selection::new(cell_width, line_height),
            cursor: Cursor::new(),
            scrollbar: Scrollbar::new(),
        }
    }

    pub(crate) fn visual_state(&self) -> TerminalVisualState {
        TerminalVisualState {
            selection: self.selection.selection_bounds(),
            cursor_visible: self.cursor.is_visible(),
            scrollbar_visible: self.scrollbar.is_visible(),
        }
    }

    pub(crate) fn handle_event(
        &mut self,
        event: &WindowEvent,
        terminal: &mut Terminal,
        window: &Window,
        pty: &mut Pty,
        modifiers: ModifiersState,
    ) -> EventResult {
        let selection = self.selection.handle_event(
            event,
            &mut SelectionContext {
                terminal: &mut *terminal,
                window,
                pty,
                modifiers,
            },
        );
        if selection != EventResult::Continue {
            return selection;
        }
        if self
            .scrollbar
            .handle_event(event, &ScrollbarContext { terminal, window })
            == EventResult::Handled
        {
            return EventResult::Handled;
        }
        if self.cursor.handle_event(event, &CursorContext { terminal }) == EventResult::Handled {
            return EventResult::Handled;
        }
        EventResult::Continue
    }

    pub(crate) fn deadline(&mut self, terminal: &mut Terminal, window: &Window) -> Option<Instant> {
        let mut deadline = None;
        if let Some(next) = self.selection.on_about_to_wait(&mut SelectionWaitContext {
            terminal: &mut *terminal,
            window,
        }) {
            deadline = Some(deadline.map_or(next, |current: Instant| current.min(next)));
        }
        if let Some(next) = self
            .cursor
            .on_about_to_wait(&CursorWaitContext { terminal, window })
        {
            deadline = Some(deadline.map_or(next, |current: Instant| current.min(next)));
        }
        if let Some(next) = self
            .scrollbar
            .on_about_to_wait(&ScrollbarWaitContext { window })
        {
            deadline = Some(deadline.map_or(next, |current: Instant| current.min(next)));
        }
        deadline
    }
}
