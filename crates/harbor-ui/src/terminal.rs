//! Component tree: owns GPU layers and dispatches events in z-order.

use crate::{
    Component, TerminalOverlays,
    background::Background,
    decoration::Decoration,
    font::FontBook,
    metrics::TextMetrics,
    text::{AtlasGlyph, Text},
};
use harbor_gpu::GpuContext;
use harbor_terminal::{Screen, TerminalSize};

/// Terminal renderer layers. Interactive layer state remains in the application shell.
pub struct UiRoot {
    background: Background,
    text: Text,
    decoration: Decoration,
}

impl UiRoot {
    /// Creates terminal rendering layers from the GPU context and font metrics.
    pub fn new(
        gpu: &GpuContext,
        screen: &Screen,
        fonts: FontBook,
        metrics: TextMetrics,
    ) -> anyhow::Result<Self> {
        let snap = screen.snapshot();
        Ok(Self {
            background: Background::new(gpu, &snap, metrics.cell_width, metrics.line_height),
            text: Text::new(gpu, fonts, metrics, &snap)?,
            decoration: Decoration::new(gpu, &snap, metrics),
        })
    }

    /// Returns the cell dimensions (rows × cols) for the current surface size.
    pub fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        self.text.terminal_size(gpu)
    }

    /// Font metrics (cell dimensions, ascent, etc.).
    pub fn text_metrics(&self) -> &TextMetrics {
        self.text.metrics()
    }

    /// Looks up a glyph in the CPU-side atlas.
    pub fn text_glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.text.glyph(ch)
    }

    /// The text render pipeline.
    pub fn text_pipeline(&self) -> &wgpu::RenderPipeline {
        self.text.text_pipeline()
    }

    /// The bind group holding the glyph atlas texture and sampler.
    pub fn text_bind_group(&self) -> &wgpu::BindGroup {
        self.text.text_bind_group()
    }

    /// Ensures dialog text characters are rasterized.
    pub fn ensure_glyphs(&mut self, text: &str, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.text.ensure_glyphs(text, device, queue);
    }

    /// Uploads terminal layers and shell-owned overlays.
    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        screen: &Screen,
        overlays: &mut impl TerminalOverlays,
    ) {
        let snap = screen.snapshot();
        let dirty_ranges = snap.dirty_ranges.clone();
        self.background
            .prepare_with_dirty(gpu, &snap, &dirty_ranges);
        self.text.prepare_with_dirty(gpu, &snap, &dirty_ranges);
        self.decoration
            .prepare_with_dirty(gpu, &snap, &dirty_ranges);
        overlays.prepare(gpu, &snap);
    }

    /// Issues draw calls in terminal z-order.
    pub fn draw(&self, pass: &mut wgpu::RenderPass, overlays: &impl TerminalOverlays) {
        self.background.draw(pass);
        self.text.draw(pass);
        self.decoration.draw(pass);
        overlays.draw(pass);
    }

    /// Marks terminal layers and shell-owned overlays dirty after a surface resize.
    pub fn resize(
        &mut self,
        gpu: &GpuContext,
        size: (u32, u32),
        overlays: &mut impl TerminalOverlays,
    ) {
        Component::resize(&mut self.background, gpu, size);
        Component::resize(&mut self.text, gpu, size);
        Component::resize(&mut self.decoration, gpu, size);
        overlays.resize(gpu, size);
    }
}
