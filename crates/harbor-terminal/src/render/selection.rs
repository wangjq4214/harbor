use crate::model::{SelectionBounds, TerminalSnapshot};
use harbor_config::Rgba;
use std::sync::Arc;

use super::gpu::{ColoredVertex, TerminalGpuAccess, VertexBuffer};
use crate::render::RenderViewport;

// ── Selection (outer — GPU) ──────────────────────────────────────

pub struct Selection {
    pipeline: Arc<wgpu::RenderPipeline>,
    quads: VertexBuffer<ColoredVertex>,
    /// Current terminal-owned selection bounds.
    bounds: Option<SelectionBounds>,
    /// Last projection inputs used for the vertex upload.
    last_projection: Option<(u64, usize, usize, usize, usize, RenderViewport)>,
    /// Selection color, applied when the quads are rebuilt.
    color: Rgba,
    /// Whether the retained quads need rebuilding before the next draw.
    dirty: bool,
}

impl Selection {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn new(
        gpu: TerminalGpuAccess<'_>,
        pipeline: Arc<wgpu::RenderPipeline>,
        color: Rgba,
    ) -> Self {
        let quads = VertexBuffer::new(gpu.device());
        Self {
            pipeline,
            quads,
            bounds: None,
            last_projection: None,
            color,
            dirty: true,
        }
    }

    /// Builds solid-color quad vertices for all cells covered by the selection.
    fn build_vertices(
        &self,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
        bounds: SelectionBounds,
    ) -> Vec<ColoredVertex> {
        let (surf_w, surf_h) = viewport.surface_dimensions();
        let sg = bounds.start_row;
        let sc = bounds.start_col;
        let eg = bounds.end_row;
        let ec = bounds.end_col;

        let history_start = snap.history_start;
        let scroll_count = snap.scroll_count;
        let view_offset = snap.view_offset;
        let rows = snap.rows;
        let cols = snap.cols;

        let view_start = history_start + (scroll_count.saturating_sub(view_offset)) as u64;
        let view_end = view_start + rows as u64 - 1;

        let loop_start = sg.max(view_start);
        let loop_end = eg.min(view_end);

        let mut verts = if loop_start <= loop_end {
            let visible_rows = (loop_end - loop_start + 1) as usize;
            Vec::with_capacity(visible_rows * cols * 6)
        } else {
            return Vec::new();
        };

        for g in loop_start..=loop_end {
            let display_row = (g - view_start) as usize;
            let col_start = if g == sg { sc } else { 0 };
            let col_end = if g == eg { ec } else { cols.saturating_sub(1) };

            for col in col_start..=col_end {
                let (left, top, right, bottom) = viewport.cell_bounds(display_row, col);
                let quad = ColoredVertex::from_pixel_rect(
                    left,
                    top,
                    right,
                    bottom,
                    self.color.components(),
                    surf_w,
                    surf_h,
                );
                verts.extend_from_slice(&quad);
            }
        }
        verts
    }

    pub fn invalidate_projection(&mut self) {
        self.dirty = true;
    }

    pub(crate) fn set_color(&mut self, color: Rgba) {
        if self.color != color {
            self.color = color;
            self.dirty = true;
        }
    }

    pub fn set_bounds(&mut self, bounds: Option<SelectionBounds>) {
        if self.bounds != bounds {
            self.bounds = bounds;
            self.dirty = true;
        }
    }

    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
    ) {
        let Some(snap) = snap else {
            self.quads.clear();
            self.last_projection = None;
            self.dirty = false;
            return;
        };
        let projection = (
            snap.history_start,
            snap.scroll_count,
            snap.view_offset,
            snap.rows,
            snap.cols,
            *viewport,
        );
        if self.last_projection != Some(projection) {
            self.last_projection = Some(projection);
            self.dirty = true;
        }
        if !self.dirty {
            return;
        }
        self.dirty = false;

        if let Some(bounds) = self.bounds {
            let rows = snap.rows;
            let cols = snap.cols;
            self.quads.reserve(gpu.device(), (rows * cols * 6).max(64));

            let verts = self.build_vertices(snap, viewport, bounds);
            self.quads.write(gpu, &verts);
        } else {
            self.quads.clear();
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.quads.draw(pass, &self.pipeline, None);
    }
}
