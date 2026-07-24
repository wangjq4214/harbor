//! Component tree: owns GPU layers and dispatches events in z-order.

use harbor_render::{
    AtlasGlyph, Background, Component, Cursor, Decoration, EventResult, FontBook, GpuContext,
    InteractionResult, Scrollbar, Selection, Text, TextMetrics, WaitResult,
};
use harbor_terminal::TerminalSize;
use harbor_types::{
    CopySelectionResult, DirtyRange, TerminalSnapshot, TerminalUpdate, UpdateDamage,
};
use winit::keyboard::ModifiersState;

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
        state: &TerminalSnapshot,
        _fonts: FontBook,
        metrics: TextMetrics,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            background: Background::new(gpu, state, metrics.cell_width, metrics.line_height),
            text: Text::new(gpu, _fonts, metrics, state)?,
            decoration: Decoration::new(gpu, state, metrics),
            selection: Selection::new(gpu, metrics.cell_width, metrics.line_height),
            cursor: Cursor::new(gpu, metrics),
            scrollbar: Scrollbar::new(gpu, state),
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
    pub(crate) fn ensure_glyphs(&mut self, text: &str, gpu: &GpuContext) {
        self.text.ensure_glyphs(text, gpu);
    }

    /// Uploads dirty GPU resources for all five components.
    pub(crate) fn prepare(&mut self, gpu: &GpuContext, state: &TerminalSnapshot) {
        self.prepare_snapshot(gpu, state, &[]);
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.background.is_dirty()
            || self.text.is_dirty()
            || self.decoration.is_dirty()
            || self.selection.is_dirty()
            || self.cursor.is_dirty()
    }

    /// Applies a complete revisioned update. A revision gap's `FullUpload`
    /// cannot be reduced to the update's local dirty ranges.
    pub(crate) fn prepare_update(&mut self, gpu: &GpuContext, update: &TerminalUpdate) {
        self.prepare_update_damage(gpu, &update.snapshot, &update.damage);
    }

    pub(crate) fn prepare_update_damage(
        &mut self,
        gpu: &GpuContext,
        state: &TerminalSnapshot,
        damage: &UpdateDamage,
    ) {
        let full_ranges;
        let dirty_ranges = match damage {
            UpdateDamage::Ranges(ranges) => ranges,
            UpdateDamage::FullUpload => {
                full_ranges = (0..state.rows)
                    .map(|row| DirtyRange {
                        row,
                        start_col: 0,
                        end_col: state.cols,
                    })
                    .collect::<Vec<_>>();
                &full_ranges
            }
        };
        self.prepare_snapshot(gpu, state, dirty_ranges);
    }

    fn prepare_snapshot(
        &mut self,
        gpu: &GpuContext,
        state: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
    ) {
        if dirty_ranges.is_empty() && !self.is_dirty() {
            return;
        }
        self.background.prepare_with_dirty(gpu, state, dirty_ranges);
        self.text.prepare_with_dirty(gpu, state, dirty_ranges);
        self.decoration.prepare_with_dirty(gpu, state, dirty_ranges);
        self.selection.prepare(gpu, Some(state));
        self.cursor.prepare(gpu, Some(state));
        self.scrollbar.prepare(gpu, Some(state));
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

    pub(crate) fn apply_copy_result(&mut self, result: CopySelectionResult) -> bool {
        self.selection.apply_copy_result(result)
    }
    pub(crate) fn set_copy_pending(&mut self, request_id: u64) {
        self.selection.set_copy_pending(request_id);
    }
    /// Dispatches interaction using the published snapshot and returns concrete app requests.
    pub(crate) fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
        snapshot: &TerminalSnapshot,
        modifiers: ModifiersState,
    ) -> InteractionResult {
        let mut result = self.selection.handle_event(event, snapshot, modifiers);
        if result.event != EventResult::Continue { return result; }
        let scrollbar = self.scrollbar.handle_event(event, snapshot);
        result.requests.extend(scrollbar.requests);
        if scrollbar.event == EventResult::Handled { result.event = EventResult::Handled; return result; }
        let cursor = self.cursor.handle_event(event, snapshot);
        result.requests.extend(cursor.requests);
        result.event = cursor.event;
        result
    }

    /// Collects the earliest component wake deadline and associated requests.
    pub(crate) fn compact_deadline(&mut self, snapshot: &TerminalSnapshot) -> WaitResult {
        let mut result = self.selection.on_about_to_wait(snapshot);
        for other in [self.cursor.on_about_to_wait(snapshot), self.scrollbar.on_about_to_wait()] {
            result.deadline = match (result.deadline, other.deadline) { (Some(a), Some(b)) => Some(a.min(b)), (Some(a), None) => Some(a), (None, deadline) => deadline };
            result.requests.extend(other.requests);
        }
        result
    }

}
