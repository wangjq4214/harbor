use harbor_text::TextMetrics;
use harbor_types::TerminalSnapshot;
use std::sync::Arc;

use super::gpu::{self, ColoredVertex, GpuContext, UploadMode};
use crate::{CellAttrs, DirtyRange};
use harbor_config::TEXT_PADDING;

// ── Vertex builders (free fn, testable without GPU handles) ───────────────────

/// Builds underline vertices for every row.
/// Returns one `ColoredVertex` per grid cell (degenerate for cells without decoration).
pub fn build_underline_vertices(
    cell_width: f32,
    line_height: f32,
    underline_pos: f32,
    underline_thickness: f32,
    snap: &TerminalSnapshot,
    surf_w: f32,
    surf_h: f32,
) -> Vec<ColoredVertex> {
    let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
    for row in 0..snap.rows {
        let cell_top = TEXT_PADDING + row as f32 * line_height;
        let u_top = cell_top + underline_pos;
        let u_bottom = u_top + underline_thickness;
        for col in 0..snap.cols {
            let cell = snap.cell(row, col);
            if cell.attrs.contains(CellAttrs::UNDERLINE) && cell.ch != ' ' {
                let left = TEXT_PADDING + col as f32 * cell_width;
                let right = TEXT_PADDING + (col + 1) as f32 * cell_width;
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
    cell_width: f32,
    line_height: f32,
    strikethrough_pos: f32,
    strikethrough_thickness: f32,
    snap: &TerminalSnapshot,
    surf_w: f32,
    surf_h: f32,
) -> Vec<ColoredVertex> {
    let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
    for row in 0..snap.rows {
        let cell_top = TEXT_PADDING + row as f32 * line_height;
        let s_top = cell_top + strikethrough_pos - strikethrough_thickness / 2.0;
        let s_bottom = s_top + strikethrough_thickness;
        for col in 0..snap.cols {
            let cell = snap.cell(row, col);
            if cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' ' {
                let left = TEXT_PADDING + col as f32 * cell_width;
                let right = TEXT_PADDING + (col + 1) as f32 * cell_width;
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

        let (surf_w, surf_h) = gpu.surface_size();
        let u = build_underline_vertices(
            metrics.cell_width,
            metrics.line_height,
            metrics.underline_position,
            metrics.underline_thickness,
            snap,
            surf_w as f32,
            surf_h as f32,
        );
        let s = build_strikethrough_vertices(
            metrics.cell_width,
            metrics.line_height,
            metrics.strikethrough_position,
            metrics.strikethrough_thickness,
            snap,
            surf_w as f32,
            surf_h as f32,
        );
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

    pub fn prepare_with_dirty(
        &mut self,
        gpu: &GpuContext,
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
    ) {
        let (surf_w, surf_h) = gpu.surface_size();
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
            let u = build_underline_vertices(
                self.cell_width,
                self.line_height,
                self.underline_pos,
                self.underline_thickness,
                snap,
                surf_w as f32,
                surf_h as f32,
            );
            let s = build_strikethrough_vertices(
                self.cell_width,
                self.line_height,
                self.strikethrough_pos,
                self.strikethrough_thickness,
                snap,
                surf_w as f32,
                surf_h as f32,
            );
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
            let u = build_underline_vertices(
                self.cell_width,
                self.line_height,
                self.underline_pos,
                self.underline_thickness,
                snap,
                surf_w as f32,
                surf_h as f32,
            );
            let s = build_strikethrough_vertices(
                self.cell_width,
                self.line_height,
                self.strikethrough_pos,
                self.strikethrough_thickness,
                snap,
                surf_w as f32,
                surf_h as f32,
            );
            gpu.write_buffer(&self.underline_buffer, 0, bytemuck::cast_slice(&u));
            gpu.write_buffer(&self.strikethrough_buffer, 0, bytemuck::cast_slice(&s));
        } else {
            tracing::trace!("rebuilding decoration draw batch (incremental)");
            for range in dirty_ranges {
                let cell_top = TEXT_PADDING + range.row as f32 * self.line_height;
                let u_top = cell_top + self.underline_pos;
                let u_bottom = u_top + self.underline_thickness;
                let s_top = cell_top + self.strikethrough_pos - self.strikethrough_thickness / 2.0;
                let s_bottom = s_top + self.strikethrough_thickness;

                let mut u_row = Vec::with_capacity((range.end_col - range.start_col) * 6);
                let mut s_row = Vec::with_capacity((range.end_col - range.start_col) * 6);
                for col in range.start_col..range.end_col {
                    let cell = snap.cell(range.row, col);
                    if cell.attrs.contains(CellAttrs::UNDERLINE) && cell.ch != ' ' {
                        let left = TEXT_PADDING + col as f32 * self.cell_width;
                        let right = TEXT_PADDING + (col + 1) as f32 * self.cell_width;
                        let color = cell.fg.to_rgba();
                        u_row.extend_from_slice(&ColoredVertex::from_pixel_rect(
                            left,
                            u_top,
                            right,
                            u_bottom,
                            color,
                            surf_w as f32,
                            surf_h as f32,
                        ));
                    } else {
                        u_row.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
                    }

                    if cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' ' {
                        let left = TEXT_PADDING + col as f32 * self.cell_width;
                        let right = TEXT_PADDING + (col + 1) as f32 * self.cell_width;
                        let color = cell.fg.to_rgba();
                        s_row.extend_from_slice(&ColoredVertex::from_pixel_rect(
                            left,
                            s_top,
                            right,
                            s_bottom,
                            color,
                            surf_w as f32,
                            surf_h as f32,
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

    pub fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
        if let Some(snap) = snap {
            self.prepare_with_dirty(gpu, snap, &snap.dirty_ranges);
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

    #[test]
    fn decoration_layer_generates_underline_vertices() {
        let mut terminal = Terminal::new_headless(2, 4);
        terminal.put_str("\x1b[4mtest\x1b[0m");
        let snap = terminal.screen().terminal_snapshot();

        let u_verts = build_underline_vertices(10.0, 20.0, 16.0, 2.0, &snap, 800.0, 600.0);
        assert_eq!(u_verts.len(), 2 * 4 * 6);
        assert_ne!(
            u_verts[0].position,
            [0.0, 0.0],
            "first cell underline should not be degenerate"
        );

        let s_verts = build_strikethrough_vertices(10.0, 20.0, 10.0, 2.0, &snap, 800.0, 600.0);
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

        let s_verts = build_strikethrough_vertices(10.0, 20.0, 10.0, 2.0, &snap, 800.0, 600.0);
        assert_eq!(s_verts.len(), 2 * 4 * 6);
        assert_ne!(
            s_verts[0].position,
            [0.0, 0.0],
            "strikethrough should not be degenerate"
        );
    }
}
