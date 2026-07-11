//! Component tree: owns GPU layers and dispatches events in z-order.

use crate::{
    pty::Pty,
    render::{
        Background, Component, Cursor, CursorContext, CursorInput, CursorWaitContext, Decoration,
        EventResult, FontBook, GpuContext, Scrollbar, ScrollbarContext, ScrollbarInput,
        ScrollbarWaitContext, Selection, SelectionContext, SelectionInput, Text, TextMetrics,
    },
    terminal::{Screen, Terminal, TerminalSize},
};
use winit::keyboard::ModifiersState;
use winit::window::Window;

/// Container for all UI components. Owns GPU resources and delegates
/// render / event calls to each component in z-order.
pub(crate) struct UiRoot {
    /// Solid-color background behind each non-default cell.
    background: Background,
    /// Text rendering: glyph atlas + vertex buffer for every grid cell.
    text: Text,
    /// Underline / strikethrough decoration overlay.
    decoration: Decoration,
    /// Text selection: mouse-drag highlight overlay.
    selection: Selection,
    /// Cursor rendering + blink timer.
    cursor: Cursor,
    /// Scrollbar: visibility state machine + GPU thumb.
    scrollbar: Scrollbar,
}

impl UiRoot {
    /// Creates all five UI components from the GPU context and font metrics.
    /// The `screen` provides the initial grid state for atlas construction.
    /// `_fonts` is consumed by `Text::new`.
    pub(crate) fn new(
        gpu: &GpuContext,
        screen: &Screen,
        _fonts: FontBook,
        metrics: TextMetrics,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            background: Background::new(gpu, screen, metrics.cell_width, metrics.line_height),
            text: Text::new(gpu, _fonts, metrics, screen)?,
            decoration: Decoration::new(gpu, screen, metrics),
            selection: Selection::new(gpu, metrics.cell_width, metrics.line_height),
            cursor: Cursor::new(gpu, metrics),
            scrollbar: Scrollbar::new(gpu, screen),
        })
    }

    /// Returns the cell dimensions (rows × cols) for the current surface size.
    pub(crate) fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        self.text.terminal_size(gpu)
    }

    /// Uploads dirty GPU resources for all five components.
    /// Called after terminal content changes or resize.
    pub(crate) fn prepare(&mut self, gpu: &GpuContext, screen: &Screen) {
        self.background.prepare(gpu, Some(screen));
        self.text.prepare(gpu, Some(screen));
        self.decoration.prepare(gpu, Some(screen));
        self.selection.prepare(gpu, Some(screen));
        self.cursor.prepare(gpu, Some(screen));
        self.scrollbar.prepare(gpu, Some(screen));
    }

    /// Issues draw calls for all five components in z-order (back to front).
    /// Binds pipelines and vertex buffers; no GPU allocation.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.background.draw(pass);
        self.text.draw(pass);
        self.decoration.draw(pass);
        self.selection.draw(pass);
        self.cursor.draw(pass);
        self.scrollbar.draw(pass);
    }

    /// Called when the window surface is resized. Forwards to all components
    /// so they can mark their GPU resources as needing re-upload.
    pub(crate) fn resize(&mut self, gpu: &GpuContext, size: (u32, u32)) {
        Component::resize(&mut self.background, gpu, size);
        Component::resize(&mut self.text, gpu, size);
        Component::resize(&mut self.decoration, gpu, size);
        Component::resize(&mut self.selection, gpu, size);
        Component::resize(&mut self.cursor, gpu, size);
        Component::resize(&mut self.scrollbar, gpu, size);
    }

    /// Dispatches to interactive layers only, each with the rights it needs.
    /// Selection first — scrollbar always returns Handled on CursorMoved,
    /// which would block selection drag updates.
    pub(crate) fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
        terminal: &Terminal,
        window: &Window,
        gpu: &GpuContext,
        pty: &mut Pty,
        modifiers: ModifiersState,
    ) -> EventResult {
        if self.selection.handle_event(
            event,
            &mut SelectionContext {
                terminal,
                window,
                pty,
                modifiers,
            },
        ) == EventResult::Handled
        {
            return EventResult::Handled;
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

    /// Collects the next wake deadline from interactive components
    /// (cursor blink or scrollbar auto-hide).
    pub(crate) fn compact_deadline(
        &mut self,
        terminal: &Terminal,
        window: &Window,
    ) -> Option<std::time::Instant> {
        let mut deadline: Option<std::time::Instant> = None;
        if let Some(d) = self
            .cursor
            .on_about_to_wait(&CursorWaitContext { terminal, window })
        {
            deadline = Some(deadline.map_or(d, |cur| cur.min(d)));
        }
        if let Some(d) = self
            .scrollbar
            .on_about_to_wait(&ScrollbarWaitContext { window })
        {
            deadline = Some(deadline.map_or(d, |cur| cur.min(d)));
        }
        deadline
    }
}
