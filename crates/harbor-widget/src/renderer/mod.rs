pub mod quad;

use crate::layout::{Rect, Size};

// ── Viewport ─────────────────────────────────────────────────────────────────

/// DPI-aware viewport descriptor for converting logical-pixel coordinates to NDC.
#[derive(Clone, Debug)]
pub struct Viewport {
    /// Logical (dp) size of the surface.
    pub logical_size: Size,
    /// Physical pixel dimensions.
    pub physical_size: (u32, u32),
    /// Device pixel ratio (physical / logical).
    pub scale_factor: f32,
}

impl Viewport {
    /// Creates a new Viewport with the given physical size and scale factor.
    pub fn new(physical_width: u32, physical_height: u32, scale_factor: f32) -> Self {
        let logical_width = physical_width as f32 / scale_factor;
        let logical_height = physical_height as f32 / scale_factor;
        Viewport {
            logical_size: Size::new(logical_width, logical_height),
            physical_size: (physical_width, physical_height),
            scale_factor,
        }
    }

    /// Converts a dp Rect to NDC coordinates.
    /// Returns [x_ndc, y_ndc, w_ndc, h_ndc] where NDC y goes from -1 (bottom) to 1 (top),
    /// but we flip y so top-left origin maps to the top of NDC space.
    pub fn dp_rect_to_ndc(&self, rect: &Rect) -> [f32; 4] {
        if self.logical_size.width <= 0.0 || self.logical_size.height <= 0.0 {
            return [0.0, 0.0, 0.0, 0.0];
        }

        // Normalize to [0, 1] in logical space
        let nx = rect.min.x / self.logical_size.width;
        let ny = rect.min.y / self.logical_size.height;
        let nw = rect.size().width / self.logical_size.width;
        let nh = rect.size().height / self.logical_size.height;

        // Map to NDC: x from [0,1] to [-1,1], y from [0,1] to [1,-1] (flip)
        let x = 2.0 * nx - 1.0;
        let y = 1.0 - 2.0 * ny;
        let w = 2.0 * nw;
        let h = -2.0 * nh; // negative because NDC y is flipped

        [x, y, w, h]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Point;

    #[test]
    fn viewport_at_1x_scale() {
        let vp = Viewport::new(800, 600, 1.0);
        assert_eq!(vp.logical_size, Size::new(800.0, 600.0));
        assert_eq!(vp.scale_factor, 1.0);
    }

    #[test]
    fn viewport_at_2x_scale() {
        let vp = Viewport::new(1600, 1200, 2.0);
        assert_eq!(vp.logical_size, Size::new(800.0, 600.0));
    }

    #[test]
    fn dp_rect_to_ndc_full_screen() {
        let vp = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(800.0, 600.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // Full screen should map to NDC full screen
        // x: 0->*2-1=-1, y: 1-0*2=1, w: 2*1=2, h: -2*1=-2
        assert!((ndc[0] + 1.0).abs() < 0.001); // x = -1
        assert!((ndc[1] - 1.0).abs() < 0.001); // y = 1
        assert!((ndc[2] - 2.0).abs() < 0.001); // w = 2
        assert!((ndc[3] + 2.0).abs() < 0.001); // h = -2
    }

    #[test]
    fn dp_rect_to_ndc_top_left_quadrant() {
        let vp = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(400.0, 300.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // Top-left quadrant: x in [-1,0], y in [0,1] (top half in NDC)
        assert!((ndc[0] + 1.0).abs() < 0.001); // x = -1
        assert!((ndc[1] - 1.0).abs() < 0.001); // y = 1
        assert!((ndc[2] - 1.0).abs() < 0.001); // w = 1
        assert!((ndc[3] + 1.0).abs() < 0.001); // h = -1
    }

    #[test]
    fn dp_rect_to_ndc_at_nonzero_origin() {
        let vp = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(200.0, 150.0), Size::new(100.0, 50.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // x = 200/800 * 2 - 1 = 0.25*2-1 = -0.5
        // y = 1 - 150/600 * 2 = 1 - 0.25*2 = 0.5
        // w = 100/800 * 2 = 0.125*2 = 0.25
        // h = -(50/600 * 2) = -(0.0833*2) = -0.1667
        assert!((ndc[0] - (-0.5)).abs() < 0.01);
        assert!((ndc[1] - 0.5).abs() < 0.01);
        assert!((ndc[2] - 0.25).abs() < 0.01);
        assert!((ndc[3] - (-0.1666)).abs() < 0.01);
    }

    #[test]
    fn dp_rect_to_ndc_zero_viewport() {
        let vp = Viewport::new(0, 0, 1.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // Zero viewport → return zeros (guard against division by zero)
        assert_eq!(ndc, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn dp_rect_to_ndc_negative_size() {
        let vp = Viewport::new(800, 600, 1.0);
        // Create a rect with negative width/height via inverted min/max
        let rect = Rect::from_min_size(Point::new(0.0, 0.0), Size::new(-10.0, -10.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // NDC w/h will be negative (reflecting the negative dp size)
        assert!(ndc[2] < 0.0, "width in NDC should be negative");
        assert!(
            ndc[3] > 0.0,
            "height in NDC should be positive (flipped from negative)"
        );
    }

    #[test]
    fn dp_rect_to_ndc_zero_size_rect() {
        let vp = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(200.0, 150.0), Size::ZERO);
        let ndc = vp.dp_rect_to_ndc(&rect);
        // Zero-size rect produces zero w, h in NDC
        assert!((ndc[2] - 0.0).abs() < 0.001);
        assert!((ndc[3] - 0.0).abs() < 0.001);
    }

    #[test]
    fn dp_rect_to_ndc_partially_offscreen() {
        let vp = Viewport::new(800, 600, 1.0);
        // Rect starts offscreen left/top, extends partially inside
        let rect = Rect::from_min_size(Point::new(-100.0, -50.0), Size::new(300.0, 200.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // x should be < -1 (offscreen left)
        assert!(ndc[0] < -1.0, "x should be offscreen left");
        // y should be > 1 (offscreen top; y flipped)
        assert!(ndc[1] > 1.0, "y should be offscreen top");
    }

    #[test]
    fn dp_rect_to_ndc_at_2x_scale() {
        let vp = Viewport::new(1600, 1200, 2.0);
        // Logical size is 800x600. A rect at logical (200,150) size (100,50)
        let rect = Rect::from_min_size(Point::new(200.0, 150.0), Size::new(100.0, 50.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // Same logical coordinates as the nonzero_origin test but with 2x scale
        assert!((ndc[0] - (-0.5)).abs() < 0.01);
        assert!((ndc[1] - 0.5).abs() < 0.01);
        assert!((ndc[2] - 0.25).abs() < 0.01);
        assert!((ndc[3] - (-0.1666)).abs() < 0.01);
    }

    #[test]
    fn dp_rect_to_ndc_bottom_right_corner() {
        let vp = Viewport::new(800, 600, 1.0);
        // Rect at bottom-right corner
        let rect = Rect::from_min_size(Point::new(700.0, 500.0), Size::new(100.0, 100.0));
        let ndc = vp.dp_rect_to_ndc(&rect);
        // x = 700/800*2-1 = 0.875*2-1 = 0.75
        assert!((ndc[0] - 0.75).abs() < 0.01);
        // y = 1 - 500/600*2 = 1 - 0.8333*2 = 1 - 1.6667 = -0.6667
        assert!((ndc[1] - (-0.6667)).abs() < 0.01);
    }
}
