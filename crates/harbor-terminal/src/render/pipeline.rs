use crate::damage::DirtyRange;
use crate::model::{TerminalSnapshot, UpdateDamage};
use crate::render::{
    Background, Cursor, Decoration, RenderViewport, Scrollbar, Selection, TerminalGpuAccess, Text,
};
use harbor_config::Palette;
use harbor_text::{FontBook, TextMetrics};
use std::sync::Arc;
use std::time::Instant;

/// Encapsulates the GPU rendering pipeline components for the terminal.
pub struct TerminalRenderPipeline {
    viewport: RenderViewport,
    _colored_quad_pipeline: Arc<wgpu::RenderPipeline>,
    pub background: Background,
    pub text: Text,
    pub decoration: Decoration,
    pub selection: Selection,
    pub cursor: Cursor,
    pub scrollbar: Scrollbar,
    palette: Palette,
}

impl TerminalRenderPipeline {
    pub fn new(
        gpu: TerminalGpuAccess<'_>,
        initial_surface_size: (u32, u32),
        font_book: FontBook,
        metrics: TextMetrics,
        snap: &TerminalSnapshot,
        tint: [f32; 4],
        palette: Palette,
    ) -> anyhow::Result<Self> {
        let initial_surface_size = (initial_surface_size.0.max(1), initial_surface_size.1.max(1));
        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            initial_surface_size,
            initial_surface_size,
        );
        let colored_quad_pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
            gpu.device(),
            gpu.format(),
            "terminal colored-quad pipeline",
        ));
        let background = Background::new(
            gpu,
            Arc::clone(&colored_quad_pipeline),
            initial_surface_size,
            snap,
            metrics.cell_width,
            metrics.line_height,
            tint,
            palette,
        );
        let text = Text::new(gpu, font_book, metrics, snap, &viewport, palette)?;
        let decoration = Decoration::new(
            gpu,
            Arc::clone(&colored_quad_pipeline),
            initial_surface_size,
            snap,
            metrics,
            palette,
        );
        let selection = Selection::new(gpu, Arc::clone(&colored_quad_pipeline), palette.selection);
        let cursor = Cursor::new(gpu, metrics, palette.cursor);
        let scrollbar = Scrollbar::new(gpu, snap, &viewport);

        Ok(Self {
            viewport,
            _colored_quad_pipeline: colored_quad_pipeline,
            background,
            text,
            decoration,
            selection,
            cursor,
            scrollbar,
            palette,
        })
    }
    pub fn sync_palette(&mut self, palette: Palette) -> bool {
        if self.palette == palette {
            return false;
        }
        self.palette = palette;
        self.background.set_palette(palette);
        self.text.set_palette(palette);
        self.decoration.set_palette(palette);
        self.cursor.set_color(palette.cursor);
        true
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
        gpu: TerminalGpuAccess<'_>,
        snap: &TerminalSnapshot,
        damage: Option<&UpdateDamage>,
        now: Instant,
        selection_bounds: Option<crate::model::SelectionBounds>,
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
}
