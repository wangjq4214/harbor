use crate::model::TerminalSnapshot;
use harbor_config::Palette;
use harbor_text::TextMetrics;
use std::sync::Arc;

use super::gpu::{self, ColoredVertex, TerminalGpuAccess};
use super::text::glyph_color_with_palette;
use crate::render::RenderViewport;
use crate::{CellAttrs, Color, DirtyRange, UnderlineStyle};

// ── Vertex builders (free fn, testable without GPU handles) ───────────────────

// Bounded per-cell builder scratch; retained GPU/CPU geometry is compacted by row.
// Curves use 16 strips; other styles use at most that many.
const UNDERLINE_VERTICES_PER_CELL: usize = 16 * 6;

/// Reject malformed public metric records before projecting or publishing resources.
/// Finite offsets may lie outside the row; placement clamps them back into it.
pub(super) fn valid_metrics(metrics: &TextMetrics) -> bool {
    [
        metrics.cell_width,
        metrics.line_height,
        metrics.ascent,
        metrics.underline_thickness,
        metrics.strikethrough_thickness,
    ]
    .into_iter()
    .all(|v| v.is_finite() && v > 0.0)
        && metrics.underline_position.is_finite()
        && metrics.strikethrough_position.is_finite()
}

/// Row-local geometry in f64: finite f32 metrics can overflow during ratios,
/// multiplication or offset addition, even when the final visible rect is small.
struct DecorationCell<'a> {
    view: &'a RenderViewport,
    bounds: [f64; 4],
    scale: f64,
}

impl<'a> DecorationCell<'a> {
    fn new(
        metrics: &TextMetrics,
        view: &'a RenderViewport,
        row: usize,
        col: usize,
    ) -> Option<Self> {
        if !valid_metrics(metrics) || !view.is_drawable() {
            return None;
        }
        let left = f64::from(view.allocation_origin.0)
            + f64::from(view.padding)
            + col as f64 * f64::from(view.cell_width);
        let top = f64::from(view.allocation_origin.1)
            + f64::from(view.padding)
            + row as f64 * f64::from(view.line_height);
        let right = left + f64::from(view.cell_width);
        let bottom = top + f64::from(view.line_height);
        (right > left && bottom > top).then_some(Self {
            view,
            bounds: [left, top, right, bottom],
            scale: f64::from(view.line_height) / f64::from(metrics.line_height),
        })
    }

    fn thickness(&self, thickness: f32) -> f64 {
        (f64::from(thickness) * self.scale)
            .max(1.0)
            .min(f64::from(self.view.line_height) / 4.0)
    }

    fn top(&self, offset: f64, height: f64) -> f64 {
        let [_, top, _, bottom] = self.bounds;
        (top + offset).clamp(top, (bottom - height).max(top))
    }

    fn rect(&self, rect: [f64; 4], color: [f32; 4]) -> [ColoredVertex; 6] {
        let [left, top, right, bottom] = self.bounds;
        let (ox, oy) = self.view.allocation_origin;
        let [x0, y0, x1, y1] = rect;
        let x0 = x0.max(left).max(f64::from(ox)).max(0.0);
        let y0 = y0.max(top).max(f64::from(oy)).max(0.0);
        let x1 = x1
            .min(right)
            .min(f64::from(ox) + f64::from(self.view.allocation_size.0))
            .min(f64::from(self.view.surface_size.0));
        let y1 = y1
            .min(bottom)
            .min(f64::from(oy) + f64::from(self.view.allocation_size.1))
            .min(f64::from(self.view.surface_size.1));
        if x1 <= x0 || y1 <= y0 {
            return [ColoredVertex::default(); 6];
        }
        let (w, h) = self.view.surface_dimensions();
        ColoredVertex::from_pixel_rect(x0 as f32, y0 as f32, x1 as f32, y1 as f32, color, w, h)
    }
}

fn effective_underline(
    cell: &crate::Cell,
    snap: &TerminalSnapshot,
    row: usize,
    col: usize,
) -> UnderlineStyle {
    if cell.attrs.contains(CellAttrs::CONCEAL) {
        return UnderlineStyle::Off;
    }
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
    let Some(projection) = DecorationCell::new(metrics, viewport, row, col) else {
        return out;
    };
    let [left, cell_y, right, cell_bottom] = projection.bounds;
    let thickness = projection.thickness(metrics.underline_thickness);
    // Reserve room inside this row for two lines or the wave's excursion.
    let band_height = (3.0 * thickness).min(cell_bottom - cell_y);
    let top = projection.top(
        f64::from(metrics.underline_position) * projection.scale,
        band_height,
    );
    let mut used = 0;
    let mut rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
        if x1 > x0 && y1 > y0 {
            out[used..used + 6].copy_from_slice(&projection.rect([x0, y0, x1, y1], color));
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
            let step = f64::from(viewport.cell_width) / 16.0;
            for segment in 0..16 {
                // One full wave per cell, continuous across adjacent cells.
                let x0 = left + segment as f64 * step;
                let x1 = if segment == 15 {
                    right
                } else {
                    left + (segment + 1) as f64 * step
                };
                let phase = (segment as f64 + 0.5) / 16.0 * std::f64::consts::TAU;
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
            // At most nine clipped intervals, including the partial leading one.
            // Relative phase and fixed iteration count remain safe when col is
            // large enough that adding a period to an absolute f32 stops advancing.
            let width = f64::from(viewport.cell_width);
            let period = (2.0 * on).max(width / 8.0);
            let phase = (col as f64 * width).rem_euclid(period);
            for segment in 0..9 {
                let start = -phase + segment as f64 * period;
                if start >= width {
                    break;
                }
                rect(
                    left + start.max(0.0),
                    top,
                    left + (start + on).min(width),
                    top + thickness,
                );
            }
        }
        UnderlineStyle::Off => {}
    }
    out
}

#[inline]
fn is_strikethrough_active(cell: &crate::model::Cell) -> bool {
    cell.attrs.contains(CellAttrs::STRIKETHROUGH)
        && !cell.attrs.contains(CellAttrs::CONCEAL)
        && cell.ch != ' '
}

fn strikethrough_cell_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
    row: usize,
    col: usize,
) -> [ColoredVertex; 6] {
    let cell = snap.cell(row, col);
    if is_strikethrough_active(cell)
        && let Some(projection) = DecorationCell::new(metrics, viewport, row, col)
    {
        let [left, _, right, _] = projection.bounds;
        let thickness = projection.thickness(metrics.strikethrough_thickness);
        let top = projection.top(
            f64::from(metrics.strikethrough_position) * projection.scale - thickness / 2.0,
            thickness,
        );
        let color = glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs);
        return projection.rect([left, top, right, top + thickness], color);
    }
    [ColoredVertex::default(); 6]
}

fn append_strikethrough_range(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
    range: &DirtyRange,
    vertices: &mut Vec<ColoredVertex>,
) {
    for col in range.start_col..range.end_col {
        vertices.extend_from_slice(&strikethrough_cell_vertices(
            metrics, snap, viewport, palette, range.row, col,
        ));
    }
}

/// Builds fixed-size underline slots for every occupied grid cell.
pub fn build_underline_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
) -> Vec<ColoredVertex> {
    let Some(cells) = slot_count(snap, u64::MAX) else {
        return Vec::new();
    };
    let mut vertices = Vec::with_capacity(cells * UNDERLINE_VERTICES_PER_CELL);
    for row in 0..snap.rows {
        for col in 0..snap.cols {
            vertices.extend_from_slice(&underline_cell_vertices(
                metrics, snap, viewport, palette, row, col,
            ));
        }
    }
    vertices
}

/// Top-of-row overline, one non-overlapping slot per occupied cell.
fn overline_cell_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
    row: usize,
    col: usize,
) -> [ColoredVertex; 6] {
    let cell = snap.cell(row, col);
    if !cell.attrs.contains(CellAttrs::OVERLINE) || cell.attrs.contains(CellAttrs::CONCEAL) {
        return [ColoredVertex::default(); 6];
    }
    let Some(projection) = DecorationCell::new(metrics, viewport, row, col) else {
        return [ColoredVertex::default(); 6];
    };
    let [left, top, right, _] = projection.bounds;
    let thickness = projection.thickness(metrics.underline_thickness);
    let color = glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs);
    projection.rect([left, top, right, top + thickness], color)
}

/// Builds fixed-size overline slots for the occupied grid, including spaces.
pub fn build_overline_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
) -> Vec<ColoredVertex> {
    let Some(cells) = slot_count(snap, u64::MAX) else {
        return Vec::new();
    };
    let mut vertices = Vec::with_capacity(cells * 6);
    for row in 0..snap.rows {
        for col in 0..snap.cols {
            vertices.extend_from_slice(&overline_cell_vertices(
                metrics, snap, viewport, palette, row, col,
            ));
        }
    }
    vertices
}

/// Builds strikethrough vertices for every row.
/// Returns six `ColoredVertex` values per grid cell (degenerate for cells without decoration).
pub fn build_strikethrough_vertices(
    metrics: &TextMetrics,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    palette: &Palette,
) -> Vec<ColoredVertex> {
    let Some(cells) = slot_count(snap, u64::MAX) else {
        return Vec::new();
    };
    let mut vertices = Vec::with_capacity(cells * 6);
    for row in 0..snap.rows {
        append_strikethrough_range(
            metrics,
            snap,
            viewport,
            palette,
            &DirtyRange {
                row,
                start_col: 0,
                end_col: snap.cols,
            },
            &mut vertices,
        );
    }
    vertices
}

/// The largest decoration slot also bounds strike/overline counts and offsets.
pub(super) fn slot_count(snap: &TerminalSnapshot, max_buffer_size: u64) -> Option<usize> {
    let cells = snap.rows.checked_mul(snap.cols)?;
    if cells == 0 || snap.cells.len() < cells {
        return None;
    }
    let vertices = cells.checked_mul(UNDERLINE_VERTICES_PER_CELL)?;
    let bytes = vertices.checked_mul(std::mem::size_of::<ColoredVertex>())?;
    (vertices <= u32::MAX as usize && bytes as u64 <= max_buffer_size).then_some(cells)
}

// ── Decoration ────────────────────────────────────────────────────────────────

/// Underline / strikethrough / overline decoration overlay.
/// Rendered after text so lines draw over glyphs.
pub struct Decoration {
    pipeline: Arc<wgpu::RenderPipeline>,
    underline: gpu::ColoredRows,
    strikethrough: gpu::ColoredRows,
    overline: gpu::ColoredRows,
    rows: usize,
    cols: usize,
    metrics: TextMetrics,
    palette: Palette,
    dirty: bool,
    // Retain a valid projection through invalidation, but not a rejected preparation.
    viewport: Option<RenderViewport>,
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
        let viewport = RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            initial_surface_size,
            initial_surface_size,
        );
        let mut layer = Self {
            pipeline,
            underline: gpu::ColoredRows::default(),
            strikethrough: gpu::ColoredRows::default(),
            overline: gpu::ColoredRows::default(),
            rows: 0,
            cols: 0,
            metrics,
            palette,
            dirty: true,
            viewport: None,
        };
        layer.prepare_with_dirty(gpu, snap, &[], &viewport);
        layer
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
        if !valid_metrics(&self.metrics)
            || !viewport.is_drawable()
            || slot_count(snap, gpu.device().limits().max_buffer_size).is_none()
        {
            self.viewport = None;
            self.dirty = true;
            return;
        }
        let full = self.dirty
            || (snap.rows, snap.cols) != (self.rows, self.cols)
            || self.viewport != Some(*viewport)
            || dirty_ranges
                .iter()
                .any(|r| r.row >= snap.rows || r.start_col > r.end_col || r.end_col > snap.cols);
        self.underline
            .prepare(gpu, snap.rows, dirty_ranges, full, |row, vertices| {
                for col in 0..snap.cols {
                    if effective_underline(snap.cell(row, col), snap, row, col)
                        != UnderlineStyle::Off
                    {
                        gpu::ColoredRows::append(
                            vertices,
                            &underline_cell_vertices(
                                &self.metrics,
                                snap,
                                viewport,
                                &self.palette,
                                row,
                                col,
                            ),
                        );
                    }
                }
            });
        self.strikethrough
            .prepare(gpu, snap.rows, dirty_ranges, full, |row, vertices| {
                for col in 0..snap.cols {
                    if is_strikethrough_active(snap.cell(row, col)) {
                        gpu::ColoredRows::append(
                            vertices,
                            &strikethrough_cell_vertices(
                                &self.metrics,
                                snap,
                                viewport,
                                &self.palette,
                                row,
                                col,
                            ),
                        );
                    }
                }
            });
        self.overline
            .prepare(gpu, snap.rows, dirty_ranges, full, |row, vertices| {
                for col in 0..snap.cols {
                    let cell = snap.cell(row, col);
                    if cell.attrs.contains(CellAttrs::OVERLINE)
                        && !cell.attrs.contains(CellAttrs::CONCEAL)
                    {
                        gpu::ColoredRows::append(
                            vertices,
                            &overline_cell_vertices(
                                &self.metrics,
                                snap,
                                viewport,
                                &self.palette,
                                row,
                                col,
                            ),
                        );
                    }
                }
            });
        self.rows = snap.rows;
        self.cols = snap.cols;
        self.dirty = false;
        self.viewport = Some(*viewport);
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
        if self.viewport.is_some() {
            self.underline.draw(pass, &self.pipeline);
            self.strikethrough.draw(pass, &self.pipeline);
            self.overline.draw(pass, &self.pipeline);
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
