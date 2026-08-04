use harbor_types::TerminalSize;

use harbor_text::TextMetrics;
use harbor_widget::scene::primitive::ExternalDrawContext;

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

    pub fn from_external(context: &ExternalDrawContext, metrics: &TextMetrics) -> Self {
        let (origin_x, origin_y, alloc_w, alloc_h) = context.physical_allocation();
        Self {
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            padding: harbor_config::TEXT_PADDING,
            allocation_origin: (origin_x, origin_y),
            allocation_size: (alloc_w, alloc_h),
            surface_size: context.surface_size(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::layout::{Point, Rect};
    use harbor_widget::renderer::Viewport;

    fn external_context(logical: Rect, physical: (u32, u32), scale: f32) -> ExternalDrawContext {
        ExternalDrawContext::new(logical, Viewport::new(physical.0, physical.1, scale))
    }

    #[test]
    fn compute_grid_size_from_allocation_not_full_surface() {
        let metrics = TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 16.0,
            underline_thickness: 2.0,
            strikethrough_position: 10.0,
            strikethrough_thickness: 2.0,
        };
        let context = external_context(
            Rect::from_min_size(
                Point::new(10.0, 5.0),
                harbor_widget::layout::Size::new(200.0, 100.0),
            ),
            (800, 600),
            1.0,
        );
        let viewport = RenderViewport::from_external(&context, &metrics);
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
    fn should_derive_physical_allocation_from_external_context_at_2x_scale() {
        // Arrange
        let metrics = TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 16.0,
            underline_thickness: 2.0,
            strikethrough_position: 10.0,
            strikethrough_thickness: 2.0,
        };
        let context = external_context(
            Rect::from_min_size(
                Point::new(10.0, 5.0),
                harbor_widget::layout::Size::new(200.0, 100.0),
            ),
            (1600, 1200),
            2.0,
        );

        // Act
        let viewport = RenderViewport::from_external(&context, &metrics);
        let grid = viewport.compute_grid_size();

        // Assert: logical 200×100 at 2× → physical 400×200 allocation.
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
    fn should_round_fractional_external_allocation_before_grid_sizing() {
        // Arrange
        let metrics = TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 16.0,
            underline_thickness: 2.0,
            strikethrough_position: 10.0,
            strikethrough_thickness: 2.0,
        };
        let context = external_context(
            Rect::from_min_size(
                Point::new(0.4, 0.6),
                harbor_widget::layout::Size::new(100.2, 50.4),
            ),
            (800, 600),
            1.0,
        );

        // Act
        let viewport = RenderViewport::from_external(&context, &metrics);

        // Assert: floor(0.4)=0, ceil(100.6)=101; floor(0.6)=0, ceil(51.0)=51
        assert_eq!(viewport.allocation_origin, (0.0, 0.0));
        assert_eq!(viewport.allocation_size, (101, 51));
    }
}
