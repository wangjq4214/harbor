use crate::model::TerminalSnapshot;
use harbor_config::Palette;
use harbor_text::TextMetrics;
use std::sync::Arc;

use super::gpu::{self, ColoredVertex, TerminalGpuAccess, UploadMode};
use super::text::glyph_color_with_palette;
use crate::render::RenderViewport;
use crate::{CellAttrs, Color, DirtyRange, UnderlineStyle};

// ── Vertex builders (free fn, testable without GPU handles) ───────────────────

// Fixed-size cell slots keep dirty-range uploads bounded and independent of
// neighboring cells. Curves use 16 strips; other styles use at most that many.
const UNDERLINE_VERTICES_PER_CELL: usize = 16 * 6;

#[inline]
fn strikethrough_bounds(metrics: &TextMetrics, cell_y: f32) -> (f32, f32) {
    let top = cell_y + metrics.strikethrough_position - metrics.strikethrough_thickness / 2.0;
    (top, top + metrics.strikethrough_thickness)
}

fn effective_underline(
    cell: &crate::Cell,
    snap: &TerminalSnapshot,
    row: usize,
    col: usize,
) -> UnderlineStyle {
    let explicit = cell.attrs.underline_style();
    if explicit != UnderlineStyle::Off {
        return explicit;
    }
    // A continuation has a space scalar but belongs to its non-space wide lead.
    let non_space =
        cell.ch != ' ' || (cell.wide_continuation && col > 0 && snap.cell(row, col - 1).ch != ' ');
    if cell.hyperlink.is_some() && non_space {
        UnderlineStyle::Single
    } else {
        UnderlineStyle::Off
    }
}

/// One non-overlapping slot per occupied cell, including wide continuations.
/// Pattern phase is grid-relative, so incremental updates cannot introduce seams.
fn underline_cell_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
    row: usize,
    col: usize,
) -> [ColoredVertex; UNDERLINE_VERTICES_PER_CELL] {
    let mut out = [ColoredVertex::default(); UNDERLINE_VERTICES_PER_CELL];
    let cell = snap.cell(row, col);
    let style = effective_underline(cell, snap, row, col);
    if style == UnderlineStyle::Off {
        return out;
    }
    let color = if cell.underline_color == Color::Default {
        glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs)
    } else {
        palette.resolve(cell.underline_color)
    };
    let (left, cell_y, right, cell_bottom) = viewport.cell_bounds(row, col);
    let scale = viewport.line_height / metrics.line_height;
    let thickness = (metrics.underline_thickness * scale)
        .max(1.0)
        .min(viewport.line_height / 4.0);
    // Reserve room inside this row for two lines or the wave's excursion.
    let band_height = (3.0 * thickness).min(viewport.line_height);
    let top =
        (cell_y + metrics.underline_position * scale).clamp(cell_y, cell_bottom - band_height);
    let (surf_w, surf_h) = viewport.surface_dimensions();
    let mut used = 0;
    let mut rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        if x1 > x0 && y1 > y0 {
            out[used..used + 6].copy_from_slice(&ColoredVertex::from_pixel_rect(
                x0, y0, x1, y1, color, surf_w, surf_h,
            ));
            used += 6;
        }
    };
    match style {
        UnderlineStyle::Single => rect(left, top, right, top + thickness),
        UnderlineStyle::Double => {
            rect(left, top, right, top + thickness);
            rect(left, top + 2.0 * thickness, right, top + 3.0 * thickness);
        }
        UnderlineStyle::Curly => {
            let step = viewport.cell_width / 16.0;
            for segment in 0..16 {
                // One full wave per cell, continuous across adjacent cells.
                let x0 = left + segment as f32 * step;
                let x1 = if segment == 15 {
                    right
                } else {
                    left + (segment + 1) as f32 * step
                };
                let phase = (segment as f32 + 0.5) / 16.0 * std::f32::consts::TAU;
                let y = top + thickness * (1.0 + phase.sin());
                rect(x0, y, x1, y + thickness);
            }
        }
        UnderlineStyle::Dotted | UnderlineStyle::Dashed => {
            let on = if style == UnderlineStyle::Dotted {
                thickness
            } else {
                3.0 * thickness
            };
            // At most eight dots per cell, avoiding unbounded geometry at large sizes.
            let period = (2.0 * on).max(viewport.cell_width / 8.0);
            let phase_left = col as f32 * viewport.cell_width;
            let phase_right = phase_left + viewport.cell_width;
            let mut start = (phase_left / period).floor() * period;
            while start < phase_right {
                let x0 = start.max(phase_left);
                let x1 = (start + on).min(phase_right);
                rect(
                    left + x0 - phase_left,
                    top,
                    left + x1 - phase_left,
                    top + thickness,
                );
                start += period;
            }
        }
        UnderlineStyle::Off => {}
    }
    out
}

#[inline]
fn is_strikethrough_active(cell: &crate::model::Cell) -> bool {
    cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' '
}

fn build_decoration_layer_vertices(
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
    y_bounds: impl Fn(&TextMetrics, f32) -> (f32, f32),
    is_active: impl Fn(&crate::model::Cell) -> bool,
    metrics: &TextMetrics,
) -> Vec<ColoredVertex> {
    let (surf_w, surf_h) = viewport.surface_dimensions();
    let mut verts = Vec::with_capacity(snap.rows * snap.cols * 6);
    for row in 0..snap.rows {
        let (_, cell_y) = viewport.cell_pos(row, 0);
        let (top, bottom) = y_bounds(metrics, cell_y);
        for col in 0..snap.cols {
            let cell = snap.cell(row, col);
            if is_active(cell) {
                let (left, _, right, _) = viewport.cell_bounds(row, col);
                let color = glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs);
                verts.extend_from_slice(&ColoredVertex::from_pixel_rect(
                    left, top, right, bottom, color, surf_w, surf_h,
                ));
            } else {
                verts.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
            }
        }
    }
    verts
}

/// Builds fixed-size underline slots for every occupied grid cell.
pub fn build_underline_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
) -> Vec<ColoredVertex> {
    let mut vertices = Vec::with_capacity(snap.rows * snap.cols * UNDERLINE_VERTICES_PER_CELL);
    for row in 0..snap.rows {
        for col in 0..snap.cols {
            vertices.extend_from_slice(&underline_cell_vertices(
                metrics, snap, viewport, palette, row, col,
            ));
        }
    }
    vertices
}

/// Builds strikethrough vertices for every row.
/// Returns one `ColoredVertex` per grid cell (degenerate for cells without decoration).
pub fn build_strikethrough_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
) -> Vec<ColoredVertex> {
    build_decoration_layer_vertices(
        snap,
        viewport,
        palette,
        strikethrough_bounds,
        is_strikethrough_active,
        metrics,
    )
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
    metrics: TextMetrics,
    palette: Palette,
    dirty: bool,
}

impl Decoration {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn new(
        gpu: TerminalGpuAccess<'_>,
        pipeline: Arc<wgpu::RenderPipeline>,
        initial_surface_size: (u32, u32),
        snap: &TerminalSnapshot,
        metrics: TextMetrics,
        palette: Palette,
    ) -> Self {
        let rows = snap.rows;
        let cols = snap.cols;
        let empty_u =
            vec![ColoredVertex::default(); (rows * cols * UNDERLINE_VERTICES_PER_CELL).max(1)];
        let empty_s = vec![ColoredVertex::default(); (rows * cols * 6).max(1)];
        let underline_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty_u);
        let strikethrough_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty_s);

        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            initial_surface_size,
            initial_surface_size,
        );
        let u = build_underline_vertices(&metrics, snap, &viewport, &palette);
        let s = build_strikethrough_vertices(&metrics, snap, &viewport, &palette);
        gpu.write_buffer(&underline_buffer, 0, bytemuck::cast_slice(&u));
        gpu.write_buffer(&strikethrough_buffer, 0, bytemuck::cast_slice(&s));

        Self {
            pipeline,
            underline_buffer,
            strikethrough_buffer,
            rows,
            cols,
            metrics,
            palette,
            dirty: false,
        }
    }

    pub fn invalidate_projection(&mut self) {
        self.dirty = true;
    }

    pub(crate) fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
        self.dirty = true;
    }

    pub fn prepare_with_dirty(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: &TerminalSnapshot,
        dirty_ranges: &[DirtyRange],
        viewport: &RenderViewport,
    ) {
        let (surf_w, surf_h) = viewport.surface_dimensions();
        let resized = snap.rows != self.rows || snap.cols != self.cols;
        let bytes_per_cell =
            (UNDERLINE_VERTICES_PER_CELL + 6) * std::mem::size_of::<ColoredVertex>();
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
            if snap.rows * snap.cols > self.rows * self.cols {
                let empty_u = vec![
                    ColoredVertex::default();
                    (snap.rows * snap.cols * UNDERLINE_VERTICES_PER_CELL).max(1)
                ];
                let empty_s = vec![ColoredVertex::default(); (snap.rows * snap.cols * 6).max(1)];
                self.underline_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty_u);
                self.strikethrough_buffer =
                    gpu::create_colored_vertex_buffer(gpu.device(), &empty_s);
            }
            let u = build_underline_vertices(&self.metrics, snap, viewport, &self.palette);
            let s = build_strikethrough_vertices(&self.metrics, snap, viewport, &self.palette);
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
            let u = build_underline_vertices(&self.metrics, snap, viewport, &self.palette);
            let s = build_strikethrough_vertices(&self.metrics, snap, viewport, &self.palette);
            gpu.write_buffer(&self.underline_buffer, 0, bytemuck::cast_slice(&u));
            gpu.write_buffer(&self.strikethrough_buffer, 0, bytemuck::cast_slice(&s));
        } else {
            tracing::trace!("rebuilding decoration draw batch (incremental)");
            for range in dirty_ranges {
                let (_, cell_y) = viewport.cell_pos(range.row, 0);
                let (s_top, s_bottom) = strikethrough_bounds(&self.metrics, cell_y);

                let mut u_row = Vec::with_capacity(
                    (range.end_col - range.start_col) * UNDERLINE_VERTICES_PER_CELL,
                );
                let mut s_row = Vec::with_capacity((range.end_col - range.start_col) * 6);
                for col in range.start_col..range.end_col {
                    let cell = snap.cell(range.row, col);
                    let (left, _, right, _) = viewport.cell_bounds(range.row, col);
                    let color =
                        glyph_color_with_palette(&self.palette, cell.fg, cell.bg, cell.attrs);

                    u_row.extend_from_slice(&underline_cell_vertices(
                        &self.metrics,
                        snap,
                        viewport,
                        &self.palette,
                        range.row,
                        col,
                    ));

                    if is_strikethrough_active(cell) {
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
                let u_offset = ((range.row * snap.cols + range.start_col)
                    * UNDERLINE_VERTICES_PER_CELL
                    * std::mem::size_of::<ColoredVertex>()) as u64;
                gpu.write_buffer(
                    &self.underline_buffer,
                    u_offset,
                    bytemuck::cast_slice(&u_row),
                );
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
        gpu: TerminalGpuAccess<'_>,
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
            pass.draw(
                0..(self.rows * self.cols * UNDERLINE_VERTICES_PER_CELL) as u32,
                0..1,
            );
            pass.set_vertex_buffer(0, self.strikethrough_buffer.slice(..));
            pass.draw(0..vertex_count, 0..1);
        }
    }
}

#[cfg(test)]
#[path = "decoration_tests.rs"]
mod modern_tests;

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

        let u_verts = build_underline_vertices(&metrics, &snap, &viewport, &Palette::default());
        assert_eq!(u_verts.len(), 2 * 4 * UNDERLINE_VERTICES_PER_CELL);
        assert_ne!(
            u_verts[0].position,
            [0.0, 0.0],
            "first cell underline should not be degenerate"
        );

        let s_verts = build_strikethrough_vertices(&metrics, &snap, &viewport, &Palette::default());
        assert_eq!(s_verts.len(), 2 * 4 * 6);
        assert_eq!(
            s_verts[0].position,
            [0.0, 0.0],
            "no strikethrough expected, should be degenerate"
        );
    }

    #[test]
    fn osc8_hyperlink_cells_are_underlined_until_close() {
        let mut terminal = Terminal::new_headless(1, 4);
        terminal.put_str("\x1b]8;;https://example.test\x1b\\a b\x1b]8;;\x1b\\x");
        let snap = terminal.screen().terminal_snapshot();

        let vertices = build_underline_vertices(
            &test_metrics(),
            &snap,
            &test_viewport(),
            &Palette::default(),
        );

        assert_eq!(vertices.len(), 4 * UNDERLINE_VERTICES_PER_CELL);
        assert_ne!(
            vertices[0].position,
            [0.0, 0.0],
            "linked character should have an underline"
        );
        assert_eq!(
            vertices[UNDERLINE_VERTICES_PER_CELL].position,
            [0.0, 0.0],
            "linked spaces should remain undecorated"
        );
        assert_ne!(
            vertices[2 * UNDERLINE_VERTICES_PER_CELL].position,
            [0.0, 0.0],
            "linked character after a space should have an underline"
        );
        assert_eq!(
            vertices[3 * UNDERLINE_VERTICES_PER_CELL].position,
            [0.0, 0.0],
            "character after OSC 8 close should remain undecorated"
        );
    }

    #[test]
    fn decoration_layer_generates_strikethrough_vertices() {
        let mut terminal = Terminal::new_headless(2, 4);
        terminal.put_str("\x1b[9mstrike\x1b[0m");
        let snap = terminal.screen().terminal_snapshot();
        let viewport = test_viewport();
        let metrics = test_metrics();

        let s_verts = build_strikethrough_vertices(&metrics, &snap, &viewport, &Palette::default());
        assert_eq!(s_verts.len(), 2 * 4 * 6);
        assert_ne!(
            s_verts[0].position,
            [0.0, 0.0],
            "strikethrough should not be degenerate"
        );
    }

    #[test]
    fn inverse_decorations_use_effective_glyph_color() {
        let mut terminal = Terminal::new_headless(1, 1);
        terminal.put_str("\x1b[4;9;7mX");
        let snap = terminal.screen().terminal_snapshot();
        let palette = Palette {
            background: harbor_config::Rgba::from_rgba8(10, 20, 30, 64),
            ..Palette::default()
        };
        let expected = [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 1.0];

        let underline =
            build_underline_vertices(&test_metrics(), &snap, &test_viewport(), &palette);
        let strikethrough =
            build_strikethrough_vertices(&test_metrics(), &snap, &test_viewport(), &palette);

        assert_eq!(underline[0].color, expected);
        assert_eq!(strikethrough[0].color, expected);
    }
}
