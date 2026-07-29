use harbor_types::TerminalSize;

/// Centralizes grid geometry, layout margins, and cell-to-pixel coordinate projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderViewport {
    pub cell_width: f32,
    pub line_height: f32,
    pub padding: f32,
}

impl RenderViewport {
    pub fn new(cell_width: f32, line_height: f32) -> Self {
        Self {
            cell_width,
            line_height,
            padding: harbor_config::TEXT_PADDING,
        }
    }

    pub fn with_padding(cell_width: f32, line_height: f32, padding: f32) -> Self {
        Self {
            cell_width,
            line_height,
            padding,
        }
    }

    /// Maps grid (row, col) to top-left pixel position (x, y).
    pub fn cell_pos(&self, row: usize, col: usize) -> (f32, f32) {
        (
            self.padding + col as f32 * self.cell_width,
            self.padding + row as f32 * self.line_height,
        )
    }

    /// Returns bounding box (x_min, y_min, x_max, y_max) for a cell.
    pub fn cell_bounds(&self, row: usize, col: usize) -> (f32, f32, f32, f32) {
        let (x, y) = self.cell_pos(row, col);
        (x, y, x + self.cell_width, y + self.line_height)
    }

    /// Calculates grid dimensions that fit inside the given surface dimensions.
    pub fn compute_grid_size(&self, surface_width: u32, surface_height: u32) -> TerminalSize {
        let available_width = (surface_width as f32 - 2.0 * self.padding).max(0.0);
        let available_height = (surface_height as f32 - 2.0 * self.padding).max(0.0);
        TerminalSize {
            rows: ((available_height / self.line_height).floor() as usize).max(1),
            cols: ((available_width / self.cell_width).floor() as usize).max(1),
        }
    }
}
