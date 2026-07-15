//! Component tree: owns GPU layers and dispatches events in z-order.

use harbor_pty::Pty;
use harbor_render::{
    AtlasGlyph, Background, Component, Cursor, CursorContext, CursorInput, CursorWaitContext,
    Decoration, EventResult, FontBook, GpuContext, Scrollbar, ScrollbarContext, ScrollbarInput,
    ScrollbarWaitContext, Selection, SelectionContext, SelectionInput, SelectionWaitContext, Text,
    TextMetrics,
};
use harbor_terminal::{Screen, Terminal, TerminalSize};
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
        let snap = screen.snapshot();
        Ok(Self {
            background: Background::new(gpu, &snap, metrics.cell_width, metrics.line_height),
            text: Text::new(gpu, _fonts, metrics, &snap)?,
            decoration: Decoration::new(gpu, &snap, metrics),
            selection: Selection::new(gpu, metrics.cell_width, metrics.line_height),
            cursor: Cursor::new(gpu, metrics),
            scrollbar: Scrollbar::new(gpu, &snap),
        })
    }

    /// Returns the cell dimensions (rows × cols) for the current surface size.
    pub(crate) fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        self.text.terminal_size(gpu)
    }

    /// Font metrics (cell dimensions, ascent, etc.).
    pub(crate) fn text_metrics(&self) -> &TextMetrics {
        self.text.metrics()
    }

    /// Looks up a glyph in the CPU-side atlas.
    pub(crate) fn text_glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.text.glyph(ch)
    }

    /// The text render pipeline.
    pub(crate) fn text_pipeline(&self) -> &wgpu::RenderPipeline {
        self.text.text_pipeline()
    }

    /// The bind group holding the glyph atlas texture and sampler.
    pub(crate) fn text_bind_group(&self) -> &wgpu::BindGroup {
        self.text.text_bind_group()
    }

    /// Ensures dialog text characters are rasterized.
    pub(crate) fn ensure_glyphs(&mut self, text: &str, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.text.ensure_glyphs(text, device, queue);
    }

    /// Uploads dirty GPU resources for all five components.
    pub(crate) fn prepare(&mut self, gpu: &GpuContext, screen: &Screen) {
        let snap = screen.snapshot();
        let dirty_ranges = snap.dirty_ranges.clone();
        self.background
            .prepare_with_dirty(gpu, &snap, &dirty_ranges);
        self.text.prepare_with_dirty(gpu, &snap, &dirty_ranges);
        self.decoration
            .prepare_with_dirty(gpu, &snap, &dirty_ranges);
        self.selection.prepare(gpu, Some(&snap));
        self.cursor.prepare(gpu, Some(&snap));
        self.scrollbar.prepare(gpu, Some(&snap));
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
        terminal: &mut Terminal,
        window: &Window,
        gpu: &GpuContext,
        pty: &mut Pty,
        modifiers: ModifiersState,
    ) -> EventResult {
        let sel_result = self.selection.handle_event(
            event,
            &mut SelectionContext {
                terminal: &mut *terminal,
                window,
                pty,
                modifiers,
            },
        );
        // Propagate Handled or ConfirmPaste; only Continue falls through.
        if sel_result != EventResult::Continue {
            return sel_result;
        }
        // After SelectionContext is dropped, terminal reborrow is released.
        if self.scrollbar.handle_event(
            event,
            &ScrollbarContext {
                terminal, // &mut Terminal auto-reborrows to &Terminal
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
    /// (cursor blink, scrollbar auto-hide, or selection auto-scroll).
    pub(crate) fn compact_deadline(
        &mut self,
        terminal: &mut Terminal,
        window: &Window,
    ) -> Option<std::time::Instant> {
        let mut deadline: Option<std::time::Instant> = None;

        // Selection auto-scroll — needs &mut Terminal for ScrollAccess.
        if let Some(d) = self.selection.on_about_to_wait(&mut SelectionWaitContext {
            terminal: &mut *terminal,
            window,
        }) {
            deadline = Some(deadline.map_or(d, |cur| cur.min(d)));
        }

        // Cursor blink — reborrows terminal as &Terminal (no conflict after selection).
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
