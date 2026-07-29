use harbor_types::TerminalSnapshot;
use std::sync::Arc;

use super::gpu::{self, ColoredVertex, GpuContext, UploadMode};
use crate::render::RenderViewport;
use crate::{CellAttrs, Color, DirtyRange};

// ── BackgroundLayer ───────────────────────────────────────────────────────────

/// Draws a solid-color rectangle behind each cell with a non-default background.
/// Rendered before the text layer so glyphs appear on top.
pub struct Background {
    pipeline: Arc<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    dirty: bool,
    rows: usize,
    cols: usize,
    cell_width: f32,
    line_height: f32,
}

impl Background {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Creates the background render pipeline and pre-allocates a vertex buffer
    /// for the full grid (rows × cols × 6 vertices).
    pub fn new(
        gpu: &GpuContext,
        snap: &TerminalSnapshot,
        cell_width: f32,
        line_height: f32,
    ) -> Self {
        let pipeline = gpu.colored_quad_pipeline();

        let rows = snap.rows;
        let cols = snap.cols;
        let max_vertices = rows * cols * 6;
        let vertex_buffer = gpu::create_colored_vertex_buffer(
            gpu.device(),
            &vec![ColoredVertex::default(); max_vertices.max(1)],
        );

        let mut layer = Self {
            pipeline,
            vertex_buffer,
            dirty: true,
            rows,
            cols,
            cell_width,
            line_height,
        };

        // Build initial vertex data and upload.
        let (surf_w, surf_h) = gpu.surface_size();
        let verts = layer.build_all_vertices(snap, surf_w as f32, surf_h as f32);
        gpu.write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;
        layer
    }

    /// Builds background vertices for a single row's cells.
    pub fn build_background_row_vertices(
        cell_width: f32,
        line_height: f32,
        row: usize,
        snap: &TerminalSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        Self::build_background_range_vertices(
            cell_width,
            line_height,
            row,
            0,
            snap.cols,
            snap,
            surf_w,
            surf_h,
        )
    }

    /// Builds background vertices for a slice of columns in a single row `[start_col, end_col)`.
    #[allow(clippy::too_many_arguments)]
    pub fn build_background_range_vertices(
        cell_width: f32,
        line_height: f32,
        row: usize,
        start_col: usize,
        end_col: usize,
        snap: &TerminalSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity((end_col - start_col) * 6);
        let viewport = RenderViewport::new(cell_width, line_height);
        for col in start_col..end_col {
            let cell = snap.cell(row, col);
            let inverse = cell.attrs.contains(CellAttrs::INVERSE);
            if cell.bg != Color::Default || (inverse && cell.fg != Color::Default) {
                let (left, top, right, bottom) = viewport.cell_bounds(row, col);

                let color = if inverse {
                    cell.fg.to_rgba()
                } else {
                    cell.bg.to_rgba()
                };

                verts.extend_from_slice(&ColoredVertex::from_pixel_rect(
                    left, top, right, bottom, color, surf_w, surf_h,
                ));
            } else {
                // Default background → degenerate quad.
                verts.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
            }
        }
        verts
    }

    /// Builds vertices for every row in the full grid.
    fn build_all_vertices(
        &self,
        snap: &TerminalSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
        for row in 0..snap.rows {
            verts.extend(Self::build_background_row_vertices(
                self.cell_width,
                self.line_height,
                row,
                snap,
                surf_w,
                surf_h,
            ));
        }
        verts
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
        if plan.mode == UploadMode::None {
            return;
        }

        if resized {
            tracing::trace!(
                rows = snap.rows,
                cols = snap.cols,
                "background layer resize"
            );
            let new_cap = snap.rows * snap.cols * 6;
            let old_cap = self.rows * self.cols * 6;
            if new_cap > old_cap {
                self.vertex_buffer = gpu::create_colored_vertex_buffer(
                    gpu.device(),
                    &vec![ColoredVertex::default(); new_cap.max(1)],
                );
            }
            let verts = self.build_all_vertices(snap, surf_w as f32, surf_h as f32);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = snap.rows;
            self.cols = snap.cols;
        } else if plan.mode == UploadMode::Full {
            tracing::trace!("rebuilding background draw batch (full)");
            let verts = self.build_all_vertices(snap, surf_w as f32, surf_h as f32);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding background draw batch (incremental)");
            for range in dirty_ranges {
                let range_verts = Self::build_background_range_vertices(
                    self.cell_width,
                    self.line_height,
                    range.row,
                    range.start_col,
                    range.end_col,
                    snap,
                    surf_w as f32,
                    surf_h as f32,
                );
                let offset = (range.row * snap.cols + range.start_col)
                    * 6
                    * std::mem::size_of::<ColoredVertex>();
                gpu.write_buffer(
                    &self.vertex_buffer,
                    offset as u64,
                    bytemuck::cast_slice(&range_verts),
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
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
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
    fn inverse_background_rect_uses_fg_color() {
        let mut terminal = Terminal::new_headless(2, 3);
        terminal.put_str("\x1b[7;31mX\x1b[0m  ");
        let screen = terminal.screen();
        let cell = screen.cell(0, 0);
        assert!(
            cell.attrs.contains(CellAttrs::INVERSE),
            "cell should have INVERSE attr"
        );
        assert_eq!(cell.fg, Color::Named(1), "fg should be red (ANSI 31)");

        let snap = screen.terminal_snapshot();
        let verts = Background::build_background_row_vertices(10.0, 20.0, 0, &snap, 800.0, 600.0);

        let expected = Color::Named(1).to_rgba();
        assert_eq!(verts[0].color, expected, "inverse bg rect uses fg color");
    }

    #[test]
    fn sgr_strikethrough_stored() {
        let mut terminal = Terminal::new_headless(2, 6);
        terminal.put_str("\x1b[9mstrike\x1b[0m");
        let snap = terminal.screen();
        assert!(
            snap.cell(0, 0).attrs.contains(CellAttrs::STRIKETHROUGH),
            "cell 0 should have STRIKETHROUGH attr"
        );
    }

    #[test]
    fn sgr_underline_stored() {
        let mut terminal = Terminal::new_headless(2, 6);
        terminal.put_str("\x1b[4munder\x1b[0m");
        let snap = terminal.screen();
        assert!(
            snap.cell(0, 0).attrs.contains(CellAttrs::UNDERLINE),
            "cell 0 should have UNDERLINE attr"
        );
    }
}
