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
    full_rect_buffer: wgpu::Buffer,
    dirty: bool,
    rows: usize,
    cols: usize,
    tint: [f32; 4],
}

impl Background {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Creates the background render pipeline and pre-allocates a vertex buffer
    /// for the full grid (rows × cols × 6 vertices) plus a 6-vertex quad that
    /// covers the whole allocation with the default tint.
    pub fn new(
        gpu: &GpuContext,
        snap: &TerminalSnapshot,
        cell_width: f32,
        line_height: f32,
        tint: [f32; 4],
    ) -> Self {
        let pipeline = gpu.colored_quad_pipeline();

        let rows = snap.rows;
        let cols = snap.cols;
        let max_vertices = rows * cols * 6;
        let vertex_buffer = gpu::create_colored_vertex_buffer(
            gpu.device(),
            &vec![ColoredVertex::default(); max_vertices.max(1)],
        );
        let full_rect_buffer =
            gpu::create_colored_vertex_buffer(gpu.device(), &[ColoredVertex::default(); 6]);

        let mut layer = Self {
            pipeline,
            vertex_buffer,
            full_rect_buffer,
            dirty: true,
            rows,
            cols,
            tint,
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
        layer.upload_full_rect(gpu, &viewport);
        layer.dirty = false;
        layer
    }

    /// Builds one quad covering the whole allocation with the default tint,
    /// including the text padding and leftover grid margin the per-cell quads
    /// cannot reach.
    pub fn build_full_rect_vertices(
        viewport: &RenderViewport,
        tint: [f32; 4],
    ) -> [ColoredVertex; 6] {
        let (origin_x, origin_y) = viewport.allocation_origin;
        let (surf_w, surf_h) = viewport.surface_dimensions();
        ColoredVertex::from_pixel_rect(
            origin_x,
            origin_y,
            origin_x + viewport.allocation_size.0 as f32,
            origin_y + viewport.allocation_size.1 as f32,
            tint,
            surf_w,
            surf_h,
        )
    }

    fn upload_full_rect(&self, gpu: &GpuContext, viewport: &RenderViewport) {
        let full_rect = Self::build_full_rect_vertices(viewport, self.tint);
        gpu.write_buffer(&self.full_rect_buffer, 0, bytemuck::cast_slice(&full_rect));
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
    ///
    /// Default-background cells stay degenerate: the full-allocation tint quad
    /// drawn by [`Background::draw`] already covers them, and painting them a
    /// second time would double-apply the translucent tint over acrylic.
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
                // Default background → degenerate quad; the full-rect tint
                // quad supplies the tint exactly once.
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
        tint: [f32; 4],
    ) {
        let resized = snap.rows != self.rows || snap.cols != self.cols;
        let tint_changed = tint != self.tint;
        if tint_changed {
            self.tint = tint;
        }
        let rebuild_full_rect = resized || self.dirty || tint_changed;
        let bytes_per_cell = 6 * std::mem::size_of::<ColoredVertex>();
        let plan = gpu.upload_plan(
            snap.rows,
            snap.cols,
            bytes_per_cell,
            dirty_ranges,
            resized || self.dirty || tint_changed,
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

        if rebuild_full_rect {
            self.upload_full_rect(gpu, viewport);
        }

        self.dirty = false;
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
        tint: [f32; 4],
    ) {
        if let Some(snap) = snap {
            self.prepare_with_dirty(gpu, snap, &snap.dirty_ranges, viewport, tint);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.full_rect_buffer.slice(..));
        pass.draw(0..6, 0..1);

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

        // Every cell in this row has a default background; all 3×6 vertices
        // must stay degenerate. Only the full-rect quad paints the tint, and
        // painting default cells here too would double-apply it.
        assert_eq!(verts.len(), 18, "three cells × six vertices");
        for vert in &verts {
            assert_eq!(
                vert.position,
                [0.0, 0.0],
                "default bg cell should stay degenerate; the full-rect quad supplies the tint"
            );
            assert_eq!(
                vert.color,
                [0.0, 0.0, 0.0, 0.0],
                "default bg cell should stay degenerate; the full-rect quad supplies the tint"
            );
        }
    }

    #[test]
    fn should_emit_colored_quad_only_for_non_default_cell_in_mixed_row() {
        // Arrange — cell 0 gets a named red background; cells 1–2 stay default.
        let mut terminal = Terminal::new_headless(1, 3);
        terminal.put_str("\x1b[41mA\x1b[0mB");
        let snap = terminal.screen().terminal_snapshot();
        let viewport = RenderViewport::new(10.0, 20.0);

        // Act
        let verts = Background::build_background_range_vertices(0, 0, 3, &snap, &viewport);

        // Assert — the non-default cell emits one positioned, colored quad.
        assert_eq!(verts.len(), 18, "three cells × six vertices");
        let red = Color::Named(1).to_rgba();
        for vert in &verts[..6] {
            assert_eq!(vert.color, red, "non-default cell quad uses its bg color");
            assert_ne!(
                vert.position,
                [0.0, 0.0],
                "non-default cell quad must be positioned"
            );
        }

        // Assert — default neighbors stay degenerate; no per-cell tint is
        // applied, so the translucent full-rect tint is never doubled.
        for vert in &verts[6..] {
            assert_eq!(
                vert.position,
                [0.0, 0.0],
                "default cell must stay degenerate"
            );
            assert_eq!(
                vert.color,
                [0.0, 0.0, 0.0, 0.0],
                "default cell must not be tinted per-cell"
            );
        }
    }

    fn assert_close(actual: [f32; 2], expected: [f32; 2], message: &str) {
        assert!(
            (actual[0] - expected[0]).abs() < 1e-5 && (actual[1] - expected[1]).abs() < 1e-5,
            "{message}: actual {actual:?} != expected {expected:?}"
        );
    }

    #[test]
    fn full_rect_vertices_cover_whole_allocation_with_tint() {
        let viewport = RenderViewport::with_surface(10.0, 20.0, (100, 80), (800, 600));
        let tint = [0.36, 0.20, 0.08, 0.25];

        let verts = Background::build_full_rect_vertices(&viewport, tint);

        for vert in &verts {
            assert_eq!(vert.color, tint, "full-rect quad uses the tint");
        }
        // Allocation 100×80 in a 800×600 surface: spans NDC x [-1.0, -0.75]
        // and NDC y [0.7333, 1.0] (screen y-down → NDC y-up).
        assert_close(verts[0].position, [-1.0, 1.0], "top-left");
        assert_close(verts[1].position, [-1.0, 0.7333334], "bottom-left");
        assert_close(verts[2].position, [-0.75, 0.7333334], "bottom-right");
        assert_close(verts[3].position, [-1.0, 1.0], "top-left duplicate");
        assert_close(
            verts[4].position,
            [-0.75, 0.7333334],
            "bottom-right duplicate",
        );
        assert_close(verts[5].position, [-0.75, 1.0], "top-right");
    }

    #[test]
    fn full_rect_vertices_offset_to_allocation_origin_within_surface() {
        let viewport = RenderViewport {
            cell_width: 10.0,
            line_height: 20.0,
            padding: 0.0,
            allocation_origin: (50.0, 30.0),
            allocation_size: (200, 100),
            surface_size: (800, 600),
        };
        let tint = [0.36, 0.20, 0.08, 0.25];

        let verts = Background::build_full_rect_vertices(&viewport, tint);

        for vert in &verts {
            assert_eq!(vert.color, tint, "full-rect quad uses the tint");
        }
        // Origin (50, 30), size 200×100 → spans (50,30)..(250,130) in NDC.
        assert_close(verts[0].position, [-0.875, 0.9], "top-left");
        assert_close(verts[1].position, [-0.875, 0.5666667], "bottom-left");
        assert_close(verts[2].position, [-0.375, 0.5666667], "bottom-right");
        assert_close(verts[5].position, [-0.375, 0.9], "top-right");
    }

    #[test]
    fn full_rect_covers_padding_and_margin_beyond_last_grid_cell() {
        let viewport = RenderViewport::with_surface(10.0, 20.0, (100, 80), (800, 600));
        let tint = [0.36, 0.20, 0.08, 0.25];
        let grid = viewport.compute_grid_size();

        let verts = Background::build_full_rect_vertices(&viewport, tint);

        // Grid is 6×2 (allocation 100×80 minus 16px padding per edge); the
        // full rect must extend past the last cell on both axes.
        let (_, _, last_cell_right, last_cell_bottom) =
            viewport.cell_bounds(grid.rows - 1, grid.cols - 1);
        let (surf_w, surf_h) = viewport.surface_dimensions();
        let cell_right_ndc = last_cell_right / surf_w * 2.0 - 1.0;
        let cell_bottom_ndc = 1.0 - last_cell_bottom / surf_h * 2.0;
        assert!(
            verts[2].position[0] > cell_right_ndc,
            "full rect right edge ({}) must exceed last cell right ({})",
            verts[2].position[0],
            cell_right_ndc
        );
        assert!(
            verts[2].position[1] < cell_bottom_ndc,
            "full rect bottom edge ({}) must exceed last cell bottom ({})",
            verts[2].position[1],
            cell_bottom_ndc
        );
        assert!(
            verts[2].position[0] > verts[0].position[0]
                && verts[1].position[1] < verts[0].position[1],
            "full rect must have positive extent on both axes"
        );
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
