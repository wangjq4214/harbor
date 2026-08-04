use harbor_text::TextMetrics;
use harbor_types::TerminalSnapshot;
use std::sync::Arc;

use super::gpu::{self, ColoredVertex, GpuContext, UploadMode};
use crate::render::RenderViewport;
use crate::{CellAttrs, DirtyRange};

// ── Vertex builders (free fn, testable without GPU handles) ───────────────────

/// Builds underline vertices for every row.
/// Returns one `ColoredVertex` per grid cell (degenerate for cells without decoration).
pub fn build_underline_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
) -> Vec<ColoredVertex> {
    let (surf_w, surf_h) = viewport.surface_dimensions();
    let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
    for row in 0..snap.rows {
        let (_, cell_y) = viewport.cell_pos(row, 0);
        let u_top = cell_y + metrics.underline_position;
        let u_bottom = u_top + metrics.underline_thickness;
        for col in 0..snap.cols {
            let cell = snap.cell(row, col);
            if cell.attrs.contains(CellAttrs::UNDERLINE) && cell.ch != ' ' {
                let (left, _, right, _) = viewport.cell_bounds(row, col);
                let color = cell.fg.to_rgba();
                verts.extend_from_slice(&ColoredVertex::from_pixel_rect(
                    left, u_top, right, u_bottom, color, surf_w, surf_h,
                ));
            } else {
                verts.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
            }
        }
    }
    verts
}

/// Builds strikethrough vertices for every row.
/// Returns one `ColoredVertex` per grid cell (degenerate for cells without decoration).
pub fn build_strikethrough_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
) -> Vec<ColoredVertex> {
    let (surf_w, surf_h) = viewport.surface_dimensions();
    let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
    for row in 0..snap.rows {
        let (_, cell_y) = viewport.cell_pos(row, 0);
        let s_top = cell_y + metrics.strikethrough_position - metrics.strikethrough_thickness / 2.0;
        let s_bottom = s_top + metrics.strikethrough_thickness;
        for col in 0..snap.cols {
            let cell = snap.cell(row, col);
            if cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' ' {
                let (left, _, right, _) = viewport.cell_bounds(row, col);
                let color = cell.fg.to_rgba();
                verts.extend_from_slice(&ColoredVertex::from_pixel_rect(
                    left, s_top, right, s_bottom, color, surf_w, surf_h,
                ));
            } else {
                verts.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
            }
        }
    }
    verts
}

// ── Decoration ────────────────────────────────────────────────────────────────

/// Underline / strikethrough decoration overlay.
/// Rendered after text so lines draw over glyphs.
pub struct Decoration {
    pipeline: Arc<wgpu::RenderPipeline>,
    underline_buffer: wgpu::Buffer,
    strikethrough_buffer: wgpu::Buffer,
    rows: usize,
    cols: usize,
    cell_width: f32,
    line_height: f32,
    underline_pos: f32,
    underline_thickness: f32,
    strikethrough_pos: f32,
    strikethrough_thickness: f32,
    dirty: bool,
}

impl Decoration {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn new(gpu: &GpuContext, snap: &TerminalSnapshot, metrics: TextMetrics) -> Self {
        let pipeline = gpu.colored_quad_pipeline();

        let rows = snap.rows;
        let cols = snap.cols;
        let max_vertices = rows * cols * 6;
        let empty = vec![ColoredVertex::default(); max_vertices.max(1)];

        let underline_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);
        let strikethrough_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);

        let (surface_w, surface_h) = gpu.surface_size();
        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            (surface_w, surface_h),
            (surface_w, surface_h),
        );
        let u = build_underline_vertices(&metrics, snap, &viewport);
        let s = build_strikethrough_vertices(&metrics, snap, &viewport);
        gpu.write_buffer(&underline_buffer, 0, bytemuck::cast_slice(&u));
        gpu.write_buffer(&strikethrough_buffer, 0, bytemuck::cast_slice(&s));

        Self {
            pipeline,
            underline_buffer,
            strikethrough_buffer,
            rows,
            cols,
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            underline_pos: metrics.underline_position,
            underline_thickness: metrics.underline_thickness,
            strikethrough_pos: metrics.strikethrough_position,
            strikethrough_thickness: metrics.strikethrough_thickness,
            dirty: false,
        }
    }

    pub fn invalidate_projection(&mut self) {
        self.dirty = true;
    }

    pub fn prepare_with_dirty(
        &mut self,
        gpu: &GpuContext,
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
        viewport: &RenderViewport,
    ) {
        let (surf_w, surf_h) = viewport.surface_dimensions();
        let metrics = TextMetrics {
            cell_width: self.cell_width,
            line_height: self.line_height,
            ascent: self.line_height,
            underline_position: self.underline_pos,
            underline_thickness: self.underline_thickness,
            strikethrough_position: self.strikethrough_pos,
            strikethrough_thickness: self.strikethrough_thickness,
        };
        let resized = snap.rows != self.rows || snap.cols != self.cols;
        let bytes_per_cell = 6 * std::mem::size_of::<ColoredVertex>();
        let plan = gpu.upload_plan(
            snap.rows,
            snap.cols,
            bytes_per_cell,
            dirty_ranges,
            resized || self.dirty,
        );

        if resized {
            tracing::trace!(
                rows = snap.rows,
                cols = snap.cols,
                "decoration layer resize"
            );
            let new_cap = snap.rows * snap.cols * 6;
            let old_cap = self.rows * self.cols * 6;
            if new_cap > old_cap {
                let empty = vec![ColoredVertex::default(); new_cap.max(1)];
                self.underline_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);
                self.strikethrough_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);
            }
            let u = build_underline_vertices(&metrics, snap, viewport);
            let s = build_strikethrough_vertices(&metrics, snap, viewport);
            gpu.write_buffer(&self.underline_buffer, 0, bytemuck::cast_slice(&u));
            gpu.write_buffer(&self.strikethrough_buffer, 0, bytemuck::cast_slice(&s));
            self.rows = snap.rows;
            self.cols = snap.cols;
            self.dirty = false;
            return;
        }

        if plan.mode == UploadMode::None {
            return;
        }

        if plan.mode == UploadMode::Full {
            tracing::trace!("rebuilding decoration draw batch (full)");
            let u = build_underline_vertices(&metrics, snap, viewport);
            let s = build_strikethrough_vertices(&metrics, snap, viewport);
            gpu.write_buffer(&self.underline_buffer, 0, bytemuck::cast_slice(&u));
            gpu.write_buffer(&self.strikethrough_buffer, 0, bytemuck::cast_slice(&s));
        } else {
            tracing::trace!("rebuilding decoration draw batch (incremental)");
            for range in dirty_ranges {
                let (_, cell_y) = viewport.cell_pos(range.row, 0);
                let u_top = cell_y + self.underline_pos;
                let u_bottom = u_top + self.underline_thickness;
                let s_top = cell_y + self.strikethrough_pos - self.strikethrough_thickness / 2.0;
                let s_bottom = s_top + self.strikethrough_thickness;

                let mut u_row = Vec::with_capacity((range.end_col - range.start_col) * 6);
                let mut s_row = Vec::with_capacity((range.end_col - range.start_col) * 6);
                for col in range.start_col..range.end_col {
                    let cell = snap.cell(range.row, col);
                    if cell.attrs.contains(CellAttrs::UNDERLINE) && cell.ch != ' ' {
                        let (left, _, right, _) = viewport.cell_bounds(range.row, col);
                        let color = cell.fg.to_rgba();
                        u_row.extend_from_slice(&ColoredVertex::from_pixel_rect(
                            left, u_top, right, u_bottom, color, surf_w, surf_h,
                        ));
                    } else {
                        u_row.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
                    }

                    if cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' ' {
                        let (left, _, right, _) = viewport.cell_bounds(range.row, col);
                        let color = cell.fg.to_rgba();
                        s_row.extend_from_slice(&ColoredVertex::from_pixel_rect(
                            left, s_top, right, s_bottom, color, surf_w, surf_h,
                        ));
                    } else {
                        s_row.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
                    }
                }

                let offset = ((range.row * snap.cols + range.start_col)
                    * 6
                    * std::mem::size_of::<ColoredVertex>()) as u64;
                gpu.write_buffer(&self.underline_buffer, offset, bytemuck::cast_slice(&u_row));
                gpu.write_buffer(
                    &self.strikethrough_buffer,
                    offset,
                    bytemuck::cast_slice(&s_row),
                );
            }
        }

        self.dirty = false;
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
    ) {
        if let Some(snap) = snap {
            self.prepare_with_dirty(gpu, snap, &snap.dirty_ranges, viewport);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.underline_buffer.slice(..));
        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
            pass.set_vertex_buffer(0, self.strikethrough_buffer.slice(..));
            pass.draw(0..vertex_count, 0..1);
        }
    }

    pub fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terminal;

    fn test_viewport() -> RenderViewport {
        RenderViewport::with_surface(10.0, 20.0, (800, 600), (800, 600))
    }

    fn test_metrics() -> TextMetrics {
        TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 16.0,
            underline_thickness: 2.0,
            strikethrough_position: 10.0,
            strikethrough_thickness: 2.0,
        }
    }

    #[test]
    fn decoration_layer_generates_underline_vertices() {
        let mut terminal = Terminal::new_headless(2, 4);
        terminal.put_str("\x1b[4mtest\x1b[0m");
        let snap = terminal.screen().terminal_snapshot();
        let viewport = test_viewport();
        let metrics = test_metrics();

        let u_verts = build_underline_vertices(&metrics, &snap, &viewport);
        assert_eq!(u_verts.len(), 2 * 4 * 6);
        assert_ne!(
            u_verts[0].position,
            [0.0, 0.0],
            "first cell underline should not be degenerate"
        );

        let s_verts = build_strikethrough_vertices(&metrics, &snap, &viewport);
        assert_eq!(s_verts.len(), 2 * 4 * 6);
        assert_eq!(
            s_verts[0].position,
            [0.0, 0.0],
            "no strikethrough expected, should be degenerate"
        );
    }

    #[test]
    fn decoration_layer_generates_strikethrough_vertices() {
        let mut terminal = Terminal::new_headless(2, 4);
        terminal.put_str("\x1b[9mstrike\x1b[0m");
        let snap = terminal.screen().terminal_snapshot();
        let viewport = test_viewport();
        let metrics = test_metrics();

        let s_verts = build_strikethrough_vertices(&metrics, &snap, &viewport);
        assert_eq!(s_verts.len(), 2 * 4 * 6);
        assert_ne!(
            s_verts[0].position,
            [0.0, 0.0],
            "strikethrough should not be degenerate"
        );
    }
}
