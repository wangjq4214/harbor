use harbor_config::TEXT_PADDING;
use harbor_types::TerminalSize;

/// Backend-neutral primary font measurements.
///
/// Carries the essential metrics derived from the primary face for terminal
/// cell sizing, without any font-parser-specific types.
#[derive(Clone, Copy, Debug)]
pub struct FontMetrics {
    /// Cell width in pixels (advance of 'M' glyph).
    pub cell_width: f32,
    /// Line height in pixels.
    pub line_height: f32,
    /// Ascent from baseline to top of line.
    pub ascent: f32,
    /// Descent from baseline to bottom of line (positive value).
    pub descent: f32,
    /// Extra spacing between lines.
    pub line_gap: f32,
}

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

impl FontMetrics {
    /// Construct metrics after validating the dimensions used for layout.
    pub fn new(
        cell_width: f32,
        line_height: f32,
        ascent: f32,
        descent: f32,
        line_gap: f32,
    ) -> Option<Self> {
        let valid_positive = |value: f32| value.is_finite() && value > 0.0;
        let valid_non_negative = |value: f32| value.is_finite() && value >= 0.0;
        let valid_finite = |value: f32| value.is_finite();
        (valid_positive(cell_width)
            && valid_positive(line_height)
            && valid_positive(ascent)
            && valid_non_negative(descent)
            && valid_finite(line_gap))
        .then_some(Self {
            cell_width,
            line_height,
            ascent,
            descent,
            line_gap,
        })
    }
}

impl TextMetrics {
    /// Construct from backend-neutral font metrics.
    pub fn from_font_metrics(fm: FontMetrics) -> Self {
        let d = fm.descent;
        let underline_position = fm.line_height - d + 1.0;
        let strikethrough_position = (fm.line_height - d) * 0.45;

        Self {
            cell_width: fm.cell_width,
            line_height: fm.line_height,
            ascent: fm.ascent,
            underline_position,
            underline_thickness: 1.5,
            strikethrough_position,
            strikethrough_thickness: 1.5,
        }
    }

    pub fn terminal_size(self, width: u32, height: u32) -> TerminalSize {
        // Keep this boundary total even for legacy callers that construct the
        // public record literal directly instead of using FontMetrics::new.
        let cell_width = positive_dimension_or_one(self.cell_width);
        let line_height = positive_dimension_or_one(self.line_height);
        let text_width = (width as f32 - TEXT_PADDING * 2.0).max(cell_width);
        let text_height = (height as f32 - TEXT_PADDING * 2.0).max(line_height);

        TerminalSize {
            rows: (text_height / line_height).floor().max(1.0) as usize,
            cols: (text_width / cell_width).floor().max(1.0) as usize,
        }
    }
}

fn positive_dimension_or_one(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(cell_width: f32, line_height: f32) -> TextMetrics {
        TextMetrics::from_font_metrics(FontMetrics {
            cell_width,
            line_height,
            ascent: 0.0,
            descent: 2.0,
            line_gap: 0.0,
        })
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
    fn terminal_size_handles_invalid_public_dimensions() {
        let metrics = TextMetrics {
            cell_width: 0.0,
            line_height: f32::NAN,
            ascent: 0.0,
            underline_position: 0.0,
            underline_thickness: 1.5,
            strikethrough_position: 0.0,
            strikethrough_thickness: 1.5,
        };
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

    // ── FontMetrics tests ─────────────────────────────────────────────

    #[test]
    fn should_expose_all_fields_when_constructed() {
        let fm = FontMetrics {
            cell_width: 8.0,
            line_height: 16.0,
            ascent: 14.0,
            descent: 3.0,
            line_gap: 1.0,
        };
        assert_eq!(fm.cell_width, 8.0);
        assert_eq!(fm.line_height, 16.0);
        assert_eq!(fm.ascent, 14.0);
        assert_eq!(fm.descent, 3.0);
        assert_eq!(fm.line_gap, 1.0);
    }

    #[test]
    fn should_be_copy_and_clone() {
        let fm = FontMetrics {
            cell_width: 9.0,
            line_height: 18.0,
            ascent: 15.0,
            descent: 2.5,
            line_gap: 0.5,
        };
        let copy = fm;
        let cloned = fm;
        assert_eq!(copy.cell_width, cloned.cell_width);
        assert_eq!(copy.line_height, cloned.line_height);
        assert_eq!(copy.ascent, cloned.ascent);
        assert_eq!(copy.descent, cloned.descent);
        assert_eq!(copy.line_gap, cloned.line_gap);
    }

    #[test]
    fn should_support_zero_values() {
        let fm = FontMetrics {
            cell_width: 0.0,
            line_height: 0.0,
            ascent: 0.0,
            descent: 0.0,
            line_gap: 0.0,
        };
        assert_eq!(fm.cell_width, 0.0);
        assert_eq!(fm.line_height, 0.0);
    }

    #[test]
    fn should_reject_invalid_dimensions_at_construction() {
        assert!(FontMetrics::new(0.0, 16.0, 14.0, 2.0, 0.0).is_none());
        assert!(FontMetrics::new(8.0, f32::NAN, 14.0, 2.0, 0.0).is_none());
        assert!(FontMetrics::new(8.0, 16.0, 14.0, 2.0, -1.0).is_some());
    }

    #[test]
    fn should_be_usable_with_from_font_metrics() {
        let fm = FontMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            descent: 4.0,
            line_gap: 2.0,
        };
        let tm = TextMetrics::from_font_metrics(fm);
        assert_eq!(tm.cell_width, 10.0);
        assert_eq!(tm.line_height, 20.0);
        assert_eq!(tm.ascent, 16.0);
        // underline_position = line_height - descent + 1.0 = 20.0 - 4.0 + 1.0 = 17.0
        assert_eq!(tm.underline_position, 17.0);
        // strikethrough_position = (line_height - descent) * 0.45 = 16.0 * 0.45 = 7.2
        assert_eq!(tm.strikethrough_position, 7.2);
    }

    #[test]
    fn should_compute_underline_position_from_descent() {
        let fm = FontMetrics {
            cell_width: 8.0,
            line_height: 16.0,
            ascent: 13.0,
            descent: 3.0,
            line_gap: 0.0,
        };
        let tm = TextMetrics::from_font_metrics(fm);
        // underline_position = line_height - descent + 1 = 16 - 3 + 1 = 14
        assert_eq!(tm.underline_position, 14.0);
    }

    #[test]
    fn should_compute_strikethrough_position_from_descent() {
        let fm = FontMetrics {
            cell_width: 8.0,
            line_height: 16.0,
            ascent: 13.0,
            descent: 3.0,
            line_gap: 0.0,
        };
        let tm = TextMetrics::from_font_metrics(fm);
        // strikethrough_position = (line_height - descent) * 0.45 = 13 * 0.45 = 5.85
        assert_eq!(tm.strikethrough_position, 5.85);
    }
}
