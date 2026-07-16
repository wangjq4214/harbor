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
use harbor_gpu::GpuContext;
use harbor_pty::Pty;
use harbor_terminal::{Screen, Terminal};
use harbor_ui::{TerminalVisualState, TextMetrics};
use winit::{event::WindowEvent, keyboard::ModifiersState, window::Window};

/// Shell-owned terminal interaction state and event dispatch.
pub(crate) struct TerminalInteraction {
    selection: Selection,
    cursor: Cursor,
    scrollbar: Scrollbar,
}

impl TerminalInteraction {
    pub(crate) fn new(gpu: &GpuContext, screen: &Screen, metrics: TextMetrics) -> Self {
        Self {
            selection: Selection::new(metrics.cell_width, metrics.line_height),
            cursor: Cursor::new(gpu, metrics),
            scrollbar: Scrollbar::new(gpu, &screen.snapshot()),
        }
    }

    pub(crate) fn visual_state(&self) -> TerminalVisualState {
        TerminalVisualState {
            selection: self.selection.selection_bounds(),
            cursor_visible: self.cursor.is_visible(),
            scrollbar_visible: self.scrollbar.is_visible(),
        }
    }

    pub(crate) fn prepare(&mut self, gpu: &GpuContext, screen: &Screen) {
        let snapshot = screen.snapshot();
        self.cursor.prepare(gpu, Some(&snapshot));
        self.scrollbar.prepare(gpu, Some(&snapshot));
    }

    pub(crate) fn handle_event(
        &mut self,
        event: &WindowEvent,
        terminal: &mut Terminal,
        window: &Window,
        gpu: &GpuContext,
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
        if self.scrollbar.handle_event(
            event,
            &ScrollbarContext {
                terminal,
                gpu,
                window,
            },
        ) == EventResult::Handled
        {
            return EventResult::Handled;
        }
        if self
            .cursor
            .handle_event(event, &CursorContext { terminal, gpu })
            == EventResult::Handled
        {
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
