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
        };

        // Build initial vertex data and upload.
        let (surface_w, surface_h) = gpu.surface_size();
        let viewport = RenderViewport::with_surface(
            cell_width,
            line_height,
            (surface_w, surface_h),
            (surface_w, surface_h),
        );
        let verts = layer.build_all_vertices(snap, &viewport);
        gpu.write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;
        layer
    }

    /// Builds background vertices for a single row's cells.
    pub fn build_background_row_vertices(
        row: usize,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
    ) -> Vec<ColoredVertex> {
        Self::build_background_range_vertices(row, 0, snap.cols, snap, viewport)
    }

    /// Builds background vertices for a slice of columns in a single row `[start_col, end_col)`.
    pub fn build_background_range_vertices(
        row: usize,
        start_col: usize,
        end_col: usize,
        snap: &TerminalSnapshot,
        viewport: &RenderViewport,
    ) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity((end_col - start_col) * 6);
        let (surf_w, surf_h) = viewport.surface_dimensions();
        for col in start_col..end_col {
            let cell = snap.cell(row, col);
            let inverse = cell.attrs.contains(CellAttrs::INVERSE);
            if cell.bg != Color::Default || inverse {
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
        viewport: &RenderViewport,
    ) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
        for row in 0..snap.rows {
            verts.extend(Self::build_background_row_vertices(row, snap, viewport));
        }
        verts
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
            let verts = self.build_all_vertices(snap, viewport);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = snap.rows;
            self.cols = snap.cols;
        } else if plan.mode == UploadMode::Full {
            tracing::trace!("rebuilding background draw batch (full)");
            let verts = self.build_all_vertices(snap, viewport);
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding background draw batch (incremental)");
            for range in dirty_ranges {
                let range_verts = Self::build_background_range_vertices(
                    range.row,
                    range.start_col,
                    range.end_col,
                    snap,
                    viewport,
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
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terminal;

    #[test]
    fn inverse_default_background_fills_with_default_foreground() {
        let mut terminal = Terminal::new_headless(2, 3);
        terminal.put_str("\x1b[7mX\x1b[0m  ");
        let screen = terminal.screen();
        let cell = screen.cell(0, 0);
        assert!(
            cell.attrs.contains(CellAttrs::INVERSE),
            "cell should have INVERSE attr"
        );
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Default);

        let snap = screen.terminal_snapshot();
        let viewport = RenderViewport::new(10.0, 20.0);
        let verts = Background::build_background_row_vertices(0, &snap, &viewport);

        let expected = Color::Default.to_rgba();
        assert_eq!(
            verts[0].color, expected,
            "inverse default-default bg rect uses default foreground"
        );
    }

    #[test]
    fn default_background_cell_skips_fill() {
        let mut terminal = Terminal::new_headless(2, 3);
        terminal.put_str("X  ");
        let snap = terminal.screen().terminal_snapshot();
        let viewport = RenderViewport::new(10.0, 20.0);
        let verts = Background::build_background_row_vertices(0, &snap, &viewport);

        for vert in &verts[..6] {
            assert_eq!(
                vert.position,
                [0.0, 0.0],
                "default bg cell should skip fill"
            );
            assert_eq!(
                vert.color,
                [0.0, 0.0, 0.0, 0.0],
                "default bg cell should skip fill"
            );
        }
    }

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
        let viewport = RenderViewport::new(10.0, 20.0);
        let verts = Background::build_background_row_vertices(0, &snap, &viewport);

        let expected = Color::Named(1).to_rgba();
        assert_eq!(verts[0].color, expected, "inverse bg rect uses fg color");
    }

    #[test]
    fn inverse_named_bg_default_fg_rect_uses_default_foreground() {
        let mut terminal = Terminal::new_headless(2, 3);
        terminal.put_str("\x1b[41;7mX\x1b[0m  ");
        let screen = terminal.screen();
        let cell = screen.cell(0, 0);
        assert!(
            cell.attrs.contains(CellAttrs::INVERSE),
            "cell should have INVERSE attr"
        );
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Named(1), "bg should be red (ANSI 41)");

        let snap = screen.terminal_snapshot();
        let viewport = RenderViewport::new(10.0, 20.0);
        let verts = Background::build_background_row_vertices(0, &snap, &viewport);

        let expected = Color::Default.to_rgba();
        assert_eq!(
            verts[0].color, expected,
            "inverse named-default bg rect uses default foreground"
        );
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
