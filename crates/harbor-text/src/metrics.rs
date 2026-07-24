use crate::font::FontBook;
use harbor_config::TEXT_PADDING;
use harbor_types::TerminalSize;

/// Fixed measurements used to map window pixels to terminal cells.
#[derive(Clone, Copy)]
pub struct TextMetrics {
    pub cell_width: f32,
    pub line_height: f32,
    pub ascent: f32,
    /// Distance from cell top to underline top edge (px).
    pub underline_position: f32,
    pub underline_thickness: f32,
    /// Distance from cell top to strikethrough center (px).
    pub strikethrough_position: f32,
    pub strikethrough_thickness: f32,
}

impl TextMetrics {
    pub fn new(fonts: &FontBook) -> Self {
        let (cell_width, line_height, ascent) = fonts.terminal_metrics();
        let (underline_position, strikethrough_position) = fonts
            .primary_horizontal_line_metrics(harbor_config::FONT_SIZE)
            .map(|lm| {
                let d = lm.descent.abs();
                (line_height - d + 1.0, (line_height - d) * 0.45)
            })
            .unwrap_or((line_height * 0.8, line_height * 0.45));

        Self {
            cell_width,
            line_height,
            ascent,
            underline_position,
            underline_thickness: 1.5,
            strikethrough_position,
            strikethrough_thickness: 1.5,
        }
    }

    pub fn terminal_size(self, width: u32, height: u32) -> TerminalSize {
        let text_width = (width as f32 - TEXT_PADDING * 2.0).max(self.cell_width);
        let text_height = (height as f32 - TEXT_PADDING * 2.0).max(self.line_height);

        TerminalSize {
            rows: (text_height / self.line_height).floor().max(1.0) as usize,
            cols: (text_width / self.cell_width).floor().max(1.0) as usize,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(cell_width: f32, line_height: f32) -> TextMetrics {
        TextMetrics {
            cell_width,
            line_height,
            ascent: 0.0,
            underline_position: 0.0,
            underline_thickness: 1.5,
            strikethrough_position: 0.0,
            strikethrough_thickness: 1.5,
        }
    }

    #[test]
    fn terminal_size_with_typical_dimensions() {
        let metrics = make_metrics(10.0, 20.0);
        // width=100, height=200 → text area = (100-32, 200-32) = (68, 168)
        // cols = floor(68/10) = 6, rows = floor(168/20) = 8
        let size = metrics.terminal_size(100, 200);
        assert_eq!(size.cols, 6);
        assert_eq!(size.rows, 8);
    }

    #[test]
    fn terminal_size_exactly_divisible() {
        let metrics = make_metrics(8.0, 16.0);
        // width=128 → text_width = 128-32 = 96 → 96/8 = 12 cols
        // height=256 → text_height = 256-32 = 224 → 224/16 = 14 rows
        let size = metrics.terminal_size(128, 256);
        assert_eq!(size.cols, 12);
        assert_eq!(size.rows, 14);
    }

    #[test]
    fn terminal_size_clamps_to_minimum_one() {
        let metrics = make_metrics(1000.0, 1000.0);
        // Small window: 10x10. text_width = max(10-32, 1000) = 1000.
        // cols = floor(1000/1000) = 1, rows = floor(1000/1000) = 1
        let size = metrics.terminal_size(10, 10);
        assert_eq!(size.cols, 1);
        assert_eq!(size.rows, 1);
    }

    #[test]
    fn terminal_size_handles_zero_window() {
        let metrics = make_metrics(10.0, 20.0);
        // Zero window → text area clamped to cell_width / line_height.
        let size = metrics.terminal_size(0, 0);
        assert_eq!(size.cols, 1);
        assert_eq!(size.rows, 1);
    }

    #[test]
    fn terminal_size_with_large_window() {
        let metrics = make_metrics(8.0, 16.0);
        // Typical 1920×1080 with 16px padding → (1888, 1048)
        // cols = floor(1888/8) = 236, rows = floor(1048/16) = 65
        let size = metrics.terminal_size(1920, 1080);
        assert!(size.cols > 100, "should fit many columns");
        assert!(size.rows > 20, "should fit many rows");
    }
}
