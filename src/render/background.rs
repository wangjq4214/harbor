use std::sync::Arc;

use crate::{
    config::TEXT_PADDING,
    render::{
        Component,
        gpu::{self, ColoredVertex, GpuContext},
    },
    terminal::{CellAttrs, Color, DirtyRange, Screen},
};

// ── BackgroundLayer ───────────────────────────────────────────────────────────

/// Draws a solid-color rectangle behind each cell with a non-default background.
/// Rendered before the text layer so glyphs appear on top.
pub(crate) struct Background {
    pipeline: Arc<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    dirty: bool,
    rows: usize,
    cols: usize,
    cell_width: f32,
    line_height: f32,
}

impl Background {
    /// Creates the background render pipeline and pre-allocates a vertex buffer
    /// for the full grid (rows × cols × 6 vertices).
    pub(crate) fn new(
        gpu: &GpuContext,
        screen: &Screen,
        cell_width: f32,
        line_height: f32,
    ) -> Self {
        let pipeline = gpu.colored_quad_pipeline();

        let rows = screen.rows();
        let cols = screen.cols();
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
        let verts = layer.build_all_vertices(screen, surf_w as f32, surf_h as f32);
        gpu.queue()
            .write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;

        layer
    }

    /// Builds the 6 × cols vertices for one row, using `cell_width` and `line_height`
    /// for positioning. Cells with `bg == Color::Default` (and not inverse) produce
    /// degenerate quads skipped by the rasterizer. Inverse cells use `fg` for the
    /// background rect color.
    pub(crate) fn build_background_row_vertices(
        cell_width: f32,
        line_height: f32,
        row: usize,
        screen: &Screen,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        Self::build_background_range_vertices(
            cell_width,
            line_height,
            row,
            0,
            screen.cols(),
            screen,
            surf_w,
            surf_h,
        )
    }

    pub(crate) fn build_background_range_vertices(
        cell_width: f32,
        line_height: f32,
        row: usize,
        start_col: usize,
        end_col: usize,
        screen: &Screen,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity((end_col - start_col) * 6);
        for col in start_col..end_col {
            let cell = screen.cell(row, col);
            let inverse = cell.attrs.contains(CellAttrs::INVERSE);
            if cell.bg != Color::Default || (inverse && cell.fg != Color::Default) {
                let left = TEXT_PADDING + col as f32 * cell_width;
                let right = TEXT_PADDING + (col + 1) as f32 * cell_width;
                let top = TEXT_PADDING + row as f32 * line_height;
                let bottom = TEXT_PADDING + (row + 1) as f32 * line_height;

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
    fn build_all_vertices(&self, screen: &Screen, surf_w: f32, surf_h: f32) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity(screen.rows() * screen.cols() * 6);
        for row in 0..screen.rows() {
            verts.extend(Self::build_background_row_vertices(
                self.cell_width,
                self.line_height,
                row,
                screen,
                surf_w,
                surf_h,
            ));
        }
        verts
    }
}

impl Background {
    pub(crate) fn prepare_with_dirty(
        &mut self,
        gpu: &GpuContext,
        screen: &Screen,
        dirty_ranges: &[DirtyRange],
    ) {
        let (surf_w, surf_h) = gpu.surface_size();

        // Detect resize: dimensions changed → reallocate and full rebuild.
        if screen.rows() != self.rows || screen.cols() != self.cols {
            tracing::trace!(
                rows = screen.rows(),
                cols = screen.cols(),
                "background layer resize"
            );
            let new_cap = screen.rows() * screen.cols() * 6;
            let old_cap = self.rows * self.cols * 6;
            if new_cap > old_cap {
                self.vertex_buffer = gpu::create_colored_vertex_buffer(
                    gpu.device(),
                    &vec![ColoredVertex::default(); new_cap.max(1)],
                );
            }
            let verts = self.build_all_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = screen.rows();
            self.cols = screen.cols();
            self.dirty = false;
            return;
        }

        // Dirty check: skip upload if nothing changed.
        if !self.dirty && dirty_ranges.is_empty() {
            return;
        }

        if self.dirty {
            tracing::trace!("rebuilding background draw batch (full)");
            let verts = self.build_all_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding background draw batch (incremental)");
            for range in dirty_ranges {
                let range_verts = Self::build_background_range_vertices(
                    self.cell_width,
                    self.line_height,
                    range.row,
                    range.start_col,
                    range.end_col,
                    screen,
                    surf_w as f32,
                    surf_h as f32,
                );
                let offset = (range.row * screen.cols() + range.start_col)
                    * 6
                    * std::mem::size_of::<ColoredVertex>();
                gpu.queue().write_buffer(
                    &self.vertex_buffer,
                    offset as u64,
                    bytemuck::cast_slice(&range_verts),
                );
            }
        }

        self.dirty = false;
    }
}

impl Component for Background {
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        if let Some(screen) = screen {
            self.prepare_with_dirty(gpu, screen, &screen.dirty_ranges());
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
        }
    }

    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Terminal;

    #[test]
    fn inverse_background_rect_uses_fg_color() {
        let mut terminal = Terminal::new(2, 3);
        terminal.put_str("\x1b[7;31mX\x1b[0m  ");
        let screen = terminal.screen();
        let cell = screen.cell(0, 0);
        assert!(
            cell.attrs.contains(CellAttrs::INVERSE),
            "cell should have INVERSE attr"
        );
        assert_eq!(cell.fg, Color::Named(1), "fg should be red (ANSI 31)");

        let verts = Background::build_background_row_vertices(10.0, 20.0, 0, screen, 800.0, 600.0);

        let expected = Color::Named(1).to_rgba();
        assert_eq!(verts[0].color, expected, "inverse bg rect uses fg color");
    }

    #[test]
    fn sgr_strikethrough_stored() {
        let mut terminal = Terminal::new(2, 6);
        terminal.put_str("\x1b[9mstrike\x1b[0m");
        let screen = terminal.screen();
        assert!(
            screen.cell(0, 0).attrs.contains(CellAttrs::STRIKETHROUGH),
            "cell 0 should have STRIKETHROUGH attr"
        );
    }

    #[test]
    fn sgr_underline_stored() {
        let mut terminal = Terminal::new(2, 6);
        terminal.put_str("\x1b[4munder\x1b[0m");
        let screen = terminal.screen();
        assert!(
            screen.cell(0, 0).attrs.contains(CellAttrs::UNDERLINE),
            "cell 0 should have UNDERLINE attr"
        );
    }
}
