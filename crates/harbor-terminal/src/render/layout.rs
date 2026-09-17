use crate::model::TerminalSize;

use crate::types::{Preedit, RenderTarget};
use harbor_text::TextMetrics;
use unicode_width::UnicodeWidthChar;

/// Centralizes grid geometry, layout margins, and cell-to-pixel coordinate projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderViewport {
    pub cell_width: f32,
    pub line_height: f32,
    pub padding: f32,
    /// Physical origin of the allocation within the full surface (pixels).
    pub allocation_origin: (f32, f32),
    /// Physical size of the render allocation (pixels).
    pub allocation_size: (u32, u32),
    /// Full surface dimensions used for NDC normalization.
    pub surface_size: (u32, u32),
}

impl RenderViewport {
    pub fn new(cell_width: f32, line_height: f32) -> Self {
        Self::with_surface(cell_width, line_height, (0, 0), (0, 0))
    }

    pub fn with_surface(
        cell_width: f32,
        line_height: f32,
        allocation_size: (u32, u32),
        surface_size: (u32, u32),
    ) -> Self {
        Self {
            cell_width,
            line_height,
            padding: harbor_config::TEXT_PADDING,
            allocation_origin: (0.0, 0.0),
            allocation_size,
            surface_size,
        }
    }

    pub fn with_padding(cell_width: f32, line_height: f32, padding: f32) -> Self {
        Self {
            cell_width,
            line_height,
            padding,
            allocation_origin: (0.0, 0.0),
            allocation_size: (0, 0),
            surface_size: (0, 0),
        }
    }

    /// Builds a viewport from a terminal-owned render target and text metrics.
    pub fn from_target(target: RenderTarget, metrics: &TextMetrics) -> Self {
        Self {
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            padding: harbor_config::TEXT_PADDING,
            allocation_origin: target.allocation_origin,
            allocation_size: target.allocation_size,
            surface_size: target.surface_size,
        }
    }

    pub fn surface_dimensions(&self) -> (f32, f32) {
        (self.surface_size.0 as f32, self.surface_size.1 as f32)
    }

    /// Maps grid (row, col) to top-left pixel position (x, y).
    pub fn cell_pos(&self, row: usize, col: usize) -> (f32, f32) {
        (
            self.allocation_origin.0 + self.padding + col as f32 * self.cell_width,
            self.allocation_origin.1 + self.padding + row as f32 * self.line_height,
        )
    }

    /// Returns bounding box (x_min, y_min, x_max, y_max) for a cell.
    pub fn cell_bounds(&self, row: usize, col: usize) -> (f32, f32, f32, f32) {
        let (x, y) = self.cell_pos(row, col);
        (x, y, x + self.cell_width, y + self.line_height)
    }

    /// Calculates grid dimensions that fit inside the current allocation.
    pub fn compute_grid_size(&self) -> TerminalSize {
        let (alloc_w, alloc_h) = self.allocation_size;
        let available_width = (alloc_w as f32 - 2.0 * self.padding).max(0.0);
        let available_height = (alloc_h as f32 - 2.0 * self.padding).max(0.0);
        TerminalSize {
            rows: ((available_height / self.line_height).floor() as usize).max(1),
            cols: ((available_width / self.cell_width).floor() as usize).max(1),
        }
    }
}
/// One visible glyph position in the transient preedit overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreeditGlyph {
    pub ch: char,
    pub row: usize,
    pub col: usize,
}

/// Visible preedit glyphs and the clamped candidate-window caret cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreeditLayout {
    pub glyphs: Vec<PreeditGlyph>,
    pub caret: (usize, usize),
}

/// Lays out transient IME text from the live cursor without touching terminal cells.
pub fn layout_preedit(
    preedit: &Preedit,
    cursor: (usize, usize),
    rows: usize,
    cols: usize,
) -> PreeditLayout {
    let rows = rows.max(1);
    let cols = cols.max(1);
    let caret_offset = preedit
        .cursor_range
        .filter(|(start, end)| {
            start <= end
                && *end <= preedit.text.len()
                && preedit.text.is_char_boundary(*start)
                && preedit.text.is_char_boundary(*end)
        })
        .map_or(preedit.text.len(), |(_, end)| end);

    let mut row = cursor.1.min(rows - 1);
    let mut col = cursor.0.min(cols);
    let mut glyphs = Vec::new();
    let mut previous_glyph_cell = None;
    let mut rejected_base = false;
    let mut caret = None;

    for (byte_index, ch) in preedit.text.char_indices() {
        if byte_index == caret_offset {
            caret = Some((row.min(rows - 1), col.min(cols)));
        }
        if row >= rows {
            row = rows - 1;
            col = cols;
            break;
        }
        if ch == '\n' {
            row = row.saturating_add(1);
            col = 0;
            previous_glyph_cell = None;
            rejected_base = false;
            continue;
        }

        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width > cols {
            col = cols;
            previous_glyph_cell = None;
            rejected_base = true;
            continue;
        }
        if width > 0 && col > 0 && col.saturating_add(width) > cols {
            row = row.saturating_add(1);
            col = 0;
        }
        if row >= rows {
            row = rows - 1;
            col = cols;
            break;
        }

        if width == 0 && rejected_base {
            continue;
        }
        let glyph_cell = if width == 0 {
            previous_glyph_cell.unwrap_or((row, col.saturating_sub(1)))
        } else {
            (row, col)
        };
        glyphs.push(PreeditGlyph {
            ch,
            row: glyph_cell.0,
            col: glyph_cell.1.min(cols - 1),
        });

        if width > 0 {
            previous_glyph_cell = Some(glyph_cell);
            rejected_base = false;
        }
        if width > 0 {
            col = col.saturating_add(width);
            if col >= cols {
                if row + 1 < rows {
                    row += 1;
                    col = 0;
                } else {
                    col = cols;
                }
            }
        }
    }

    if caret.is_none() && caret_offset == preedit.text.len() {
        caret = Some((row.min(rows - 1), col.min(cols)));
    }

    PreeditLayout {
        glyphs,
        caret: caret.unwrap_or((rows - 1, cols)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RenderTarget;

    fn sample_metrics() -> TextMetrics {
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
    fn compute_grid_size_from_allocation_not_full_surface() {
        let metrics = sample_metrics();
        let target = RenderTarget::new((10.0, 5.0), (200, 100), (800, 600));
        let viewport = RenderViewport::from_target(target, &metrics);
        let grid = viewport.compute_grid_size();
        // Allocation is 200×100; TEXT_PADDING is applied on each edge.
        let pad = harbor_config::TEXT_PADDING;
        let expected_cols = (((200.0 - 2.0 * pad) / metrics.cell_width).floor() as usize).max(1);
        let expected_rows = (((100.0 - 2.0 * pad) / metrics.line_height).floor() as usize).max(1);
        assert_eq!(grid.cols, expected_cols);
        assert_eq!(grid.rows, expected_rows);
        assert_eq!(viewport.allocation_origin, (10.0, 5.0));
        assert_eq!(viewport.allocation_size, (200, 100));
        assert_eq!(viewport.surface_size, (800, 600));
    }

    #[test]
    fn cell_bounds_include_allocation_origin() {
        let viewport = RenderViewport {
            cell_width: 10.0,
            line_height: 20.0,
            padding: 0.0,
            allocation_origin: (50.0, 30.0),
            allocation_size: (200, 100),
            surface_size: (800, 600),
        };
        let (x, y, right, bottom) = viewport.cell_bounds(1, 2);
        assert_eq!((x, y), (70.0, 50.0));
        assert_eq!((right, bottom), (80.0, 70.0));
    }

    #[test]
    fn should_return_minimum_one_by_one_grid_for_tiny_allocation() {
        let viewport = RenderViewport::with_surface(10.0, 20.0, (1, 1), (800, 600));
        let grid = viewport.compute_grid_size();
        assert_eq!(grid.rows, 1);
        assert_eq!(grid.cols, 1);
    }

    #[test]
    fn should_expose_surface_dimensions_as_floats_for_ndc_projection() {
        // Arrange
        let viewport = RenderViewport::with_surface(10.0, 20.0, (200, 100), (800, 600));

        // Act
        let dims = viewport.surface_dimensions();

        // Assert
        assert_eq!(dims, (800.0, 600.0));
    }

    #[test]
    fn should_map_cell_pos_relative_to_allocation_origin() {
        // Arrange
        let viewport = RenderViewport {
            cell_width: 10.0,
            line_height: 20.0,
            padding: 4.0,
            allocation_origin: (100.0, 50.0),
            allocation_size: (200, 100),
            surface_size: (800, 600),
        };

        // Act
        let (x, y) = viewport.cell_pos(2, 3);

        // Assert
        assert_eq!((x, y), (134.0, 94.0));
    }

    #[test]
    fn should_derive_grid_from_physical_target_at_2x_scale_allocation() {
        // Arrange: already-physical values as produced by a 2× scale adaptation.
        let metrics = sample_metrics();
        let target = RenderTarget::new((20.0, 10.0), (400, 200), (1600, 1200));

        // Act
        let viewport = RenderViewport::from_target(target, &metrics);
        let grid = viewport.compute_grid_size();

        // Assert
        assert_eq!(viewport.allocation_origin, (20.0, 10.0));
        assert_eq!(viewport.allocation_size, (400, 200));
        assert_eq!(viewport.surface_size, (1600, 1200));
        let pad = harbor_config::TEXT_PADDING;
        assert_eq!(
            grid.cols,
            (((400.0 - 2.0 * pad) / metrics.cell_width).floor() as usize).max(1)
        );
        assert_eq!(
            grid.rows,
            (((200.0 - 2.0 * pad) / metrics.line_height).floor() as usize).max(1)
        );
    }

    #[test]
    fn should_preserve_supplied_physical_allocation_exactly() {
        // Arrange: physical values already rounded by the bridge adaptation.
        let metrics = sample_metrics();
        let target = RenderTarget::new((0.0, 0.0), (101, 51), (800, 600));

        // Act
        let viewport = RenderViewport::from_target(target, &metrics);

        // Assert
        assert_eq!(viewport.allocation_origin, (0.0, 0.0));
        assert_eq!(viewport.allocation_size, (101, 51));
    }

    #[test]
    fn should_copy_metrics_and_surface_when_building_from_target() {
        // Arrange
        let metrics = sample_metrics();
        let target = RenderTarget::new((12.0, 8.0), (320, 160), (1280, 720));

        // Act
        let viewport = RenderViewport::from_target(target, &metrics);

        // Assert
        assert_eq!(viewport.cell_width, metrics.cell_width);
        assert_eq!(viewport.line_height, metrics.line_height);
        assert_eq!(viewport.padding, harbor_config::TEXT_PADDING);
        assert_eq!(viewport.allocation_origin, (12.0, 8.0));
        assert_eq!(viewport.allocation_size, (320, 160));
        assert_eq!(viewport.surface_size, (1280, 720));
    }

    #[test]
    fn should_return_minimum_grid_when_from_target_has_zero_allocation() {
        // Arrange
        let metrics = sample_metrics();
        let target = RenderTarget::new((0.0, 0.0), (0, 0), (800, 600));

        // Act
        let viewport = RenderViewport::from_target(target, &metrics);
        let grid = viewport.compute_grid_size();

        // Assert
        assert_eq!(grid.rows, 1);
        assert_eq!(grid.cols, 1);
    }

    #[test]
    fn preedit_wraps_from_live_cursor_and_places_fallback_caret_after_text() {
        let layout = layout_preedit(&Preedit::new("ab", None), (3, 0), 3, 4);

        assert_eq!(
            layout.glyphs,
            vec![
                PreeditGlyph {
                    ch: 'a',
                    row: 0,
                    col: 3
                },
                PreeditGlyph {
                    ch: 'b',
                    row: 1,
                    col: 0
                },
            ]
        );
        assert_eq!(layout.caret, (1, 1));
    }

    #[test]
    fn preedit_wraps_wide_glyphs_and_keeps_combining_marks_with_previous_cell() {
        let layout = layout_preedit(&Preedit::new("你a\u{301}", None), (3, 0), 3, 4);

        assert_eq!(
            layout.glyphs[0],
            PreeditGlyph {
                ch: '你',
                row: 1,
                col: 0
            }
        );
        assert_eq!(
            layout.glyphs[1],
            PreeditGlyph {
                ch: 'a',
                row: 1,
                col: 2
            }
        );
        assert_eq!(
            layout.glyphs[2],
            PreeditGlyph {
                ch: '\u{301}',
                row: 1,
                col: 2
            }
        );
        assert_eq!(layout.caret, (1, 3));

        let wide_combining = layout_preedit(&Preedit::new("你\u{301}", None), (0, 0), 2, 4);
        assert_eq!(wide_combining.glyphs[0].row, 0);
        assert_eq!(wide_combining.glyphs[0].col, 0);
        assert_eq!(wide_combining.glyphs[1].row, 0);
        assert_eq!(wide_combining.glyphs[1].col, 0);

        let edge_combining = layout_preedit(&Preedit::new("a\u{301}", None), (3, 0), 2, 4);
        assert_eq!(edge_combining.glyphs[0].row, 0);
        assert_eq!(edge_combining.glyphs[0].col, 3);
        assert_eq!(edge_combining.glyphs[1].row, 0);
        assert_eq!(edge_combining.glyphs[1].col, 3);
    }

    #[test]
    fn preedit_does_not_split_a_wide_glyph_in_a_one_column_grid() {
        let layout = layout_preedit(&Preedit::new("你\u{301}", None), (0, 0), 2, 1);

        assert!(layout.glyphs.is_empty());
        assert_eq!(layout.caret, (0, 1));
    }
    #[test]
    fn preedit_uses_valid_utf8_caret_and_falls_back_for_invalid_ranges() {
        let valid = layout_preedit(&Preedit::new("a你", Some((1, 1))), (0, 0), 2, 4);
        let reversed = layout_preedit(&Preedit::new("a你", Some((4, 1))), (0, 0), 2, 4);
        let split_codepoint = layout_preedit(&Preedit::new("a你", Some((2, 2))), (0, 0), 2, 4);

        assert_eq!(valid.caret, (0, 1));
        assert_eq!(reversed.caret, (0, 3));
        assert_eq!(split_codepoint.caret, (0, 3));
    }

    #[test]
    fn preedit_clips_beyond_visible_bottom_and_clamps_caret_to_right_edge() {
        let layout = layout_preedit(&Preedit::new("ab", None), (3, 0), 1, 4);

        assert_eq!(
            layout.glyphs,
            vec![PreeditGlyph {
                ch: 'a',
                row: 0,
                col: 3
            }]
        );
        assert_eq!(layout.caret, (0, 4));
    }
}
