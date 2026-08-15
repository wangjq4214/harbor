use harbor_types::{SelectionBounds, TerminalSnapshot};
use std::sync::Arc;

use super::gpu::{self, ColoredVertex, GpuContext};
use crate::render::RenderViewport;
use harbor_config::SELECTION_COLOR;

// ── Selection (outer — GPU) ──────────────────────────────────────

pub struct Selection {
    pipeline: Arc<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    /// Number of vertices to draw (0 when no selection).
    vertex_count: u32,
    /// Current vertex buffer capacity (rows * cols * 6).
    vertex_cap: usize,
    /// Current terminal-owned selection bounds.
    bounds: Option<SelectionBounds>,
    /// Last projection inputs used for the vertex upload.
    last_projection: Option<(u64, usize, usize, usize, usize, RenderViewport)>,
    /// Whether vertex buffer needs re-upload.
    dirty: bool,
}

impl Selection {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn new(gpu: &GpuContext, _cell_width: f32, _line_height: f32) -> Self {
        let pipeline = gpu.colored_quad_pipeline();
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &[]);
        Self {
            pipeline,
            vertex_buffer,
            vertex_count: 0,
            vertex_cap: 0,
            bounds: None,
            last_projection: None,
            dirty: true,
        }
    }

    /// Ensures the vertex buffer capacity can hold `rows * cols * 6` vertices.
    fn ensure_capacity(&mut self, gpu: &GpuContext, rows: usize, cols: usize) {
        let required = rows * cols * 6;
        if required > self.vertex_cap {
            let cap = required.max(64);
            self.vertex_buffer = gpu::create_colored_vertex_buffer(
                gpu.device(),
                &vec![ColoredVertex::default(); cap],
            );
            self.vertex_cap = cap;
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
                    SELECTION_COLOR,
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

    pub fn set_bounds(&mut self, bounds: Option<SelectionBounds>) {
        if self.bounds != bounds {
            self.bounds = bounds;
            self.dirty = true;
        }
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
    ) {
        let Some(snap) = snap else {
            self.vertex_count = 0;
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
            self.ensure_capacity(gpu, rows, cols);

            let verts = self.build_vertices(snap, viewport, bounds);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.vertex_count = verts.len() as u32;
        } else {
            self.vertex_count = 0;
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}
