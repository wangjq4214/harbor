use crate::damage::DirtyRange;
use crate::render::{
    Background, Cursor, Decoration, GpuContext, RenderViewport, Scrollbar, Selection, Text,
};
use harbor_text::{AtlasGlyph, FontBook, TextMetrics};
use harbor_types::{TerminalSnapshot, UpdateDamage};
use std::time::Instant;

/// Encapsulates the GPU rendering pipeline components for the terminal.
pub struct TerminalRenderPipeline {
    viewport: RenderViewport,
    pub background: Background,
    pub text: Text,
    pub decoration: Decoration,
    pub selection: Selection,
    pub cursor: Cursor,
    pub scrollbar: Scrollbar,
}

impl TerminalRenderPipeline {
    pub fn new(
        gpu: &GpuContext,
        font_book: FontBook,
        metrics: TextMetrics,
        snap: &TerminalSnapshot,
        tint: [f32; 4],
    ) -> anyhow::Result<Self> {
        let (surface_w, surface_h) = gpu.surface_size();
        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            (surface_w, surface_h),
            (surface_w, surface_h),
        );
        let background = Background::new(gpu, snap, metrics.cell_width, metrics.line_height, tint);
        let text = Text::new(gpu, font_book, metrics, snap, &viewport)?;
        let decoration = Decoration::new(gpu, snap, metrics);
        let selection = Selection::new(gpu, metrics.cell_width, metrics.line_height);
        let cursor = Cursor::new(gpu, metrics);
        let scrollbar = Scrollbar::new(gpu, snap, &viewport);

        Ok(Self {
            viewport,
            background,
            text,
            decoration,
            selection,
            cursor,
            scrollbar,
        })
    }

    pub fn sync_viewport(&mut self, viewport: RenderViewport, grid_changed: bool) {
        let viewport_changed = self.viewport != viewport;
        if viewport_changed || grid_changed {
            self.viewport = viewport;
            self.background.invalidate_projection();
            self.text.invalidate_projection();
            self.decoration.invalidate_projection();
            self.selection.invalidate_projection();
            self.cursor.invalidate_projection();
            self.scrollbar.invalidate_projection();
        }
    }

    pub fn viewport(&self) -> RenderViewport {
        self.viewport
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        snap: &TerminalSnapshot,
        damage: Option<&UpdateDamage>,
        now: Instant,
        selection_bounds: Option<harbor_types::SelectionBounds>,
        tint: [f32; 4],
    ) {
        let viewport = self.viewport;
        if let Some(damage) = damage {
            let full_ranges;
            let dirty_ranges = match damage {
                UpdateDamage::Ranges(ranges) => ranges,
                UpdateDamage::FullUpload => {
                    full_ranges = (0..snap.rows)
                        .map(|row| DirtyRange {
                            row,
                            start_col: 0,
                            end_col: snap.cols,
                        })
                        .collect::<Vec<_>>();
                    &full_ranges
                }
            };
            self.background
                .prepare_with_dirty(gpu, snap, dirty_ranges, &viewport, tint);
            self.text
                .prepare_with_dirty(gpu, snap, dirty_ranges, &viewport);
            self.decoration
                .prepare_with_dirty(gpu, snap, dirty_ranges, &viewport);
        } else {
            self.background.prepare(gpu, Some(snap), &viewport, tint);
            self.text.prepare(gpu, Some(snap), &viewport);
            self.decoration.prepare(gpu, Some(snap), &viewport);
        }
        self.selection.set_bounds(selection_bounds);
        self.selection.prepare(gpu, Some(snap), &viewport);
        self.cursor.prepare(gpu, Some(snap), &viewport, now);
        self.scrollbar.prepare(gpu, Some(snap), &viewport);
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.background.draw(pass);
        self.text.draw(pass);
        self.decoration.draw(pass);
        self.selection.draw(pass);
        self.cursor.draw(pass);
        self.scrollbar.draw(pass);
    }

    pub fn metrics(&self) -> &TextMetrics {
        self.text.metrics()
    }

    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.text.glyph(ch)
    }

    pub fn text_bind_group(&self) -> &wgpu::BindGroup {
        self.text.text_bind_group()
    }

    pub fn text_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.text.text_bind_group_layout()
    }

    pub fn ensure_glyphs(&mut self, text: &str, gpu: &GpuContext) {
        self.text.ensure_glyphs(text, gpu);
    }
}
