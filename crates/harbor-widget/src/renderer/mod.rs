pub mod quad;
pub mod text_renderer;

use crate::decoration::ClipBehavior;
use crate::layout::{Point, Rect, Size};
use crate::scene::clip::RoundedClip;

/// Renderer-ready rounded clip geometry in physical pixels.
///
/// Bounds intentionally use plain floating-point coordinates rather than the
/// logical [`Rect`] type, making the unit conversion boundary explicit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalRoundedClip {
    min: (f32, f32),
    max: (f32, f32),
    radii: [f32; 4],
    behavior: ClipBehavior,
}

impl PhysicalRoundedClip {
    pub const fn min(&self) -> (f32, f32) {
        self.min
    }

    pub const fn max(&self) -> (f32, f32) {
        self.max
    }

    /// Returns physical radii clockwise from the top-left corner.
    pub const fn radii(&self) -> [f32; 4] {
        self.radii
    }

    pub const fn behavior(&self) -> ClipBehavior {
        self.behavior
    }
}

/// Two GPU clip slots packed relative to an instance origin in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PackedClips {
    pub clip_rect0: [f32; 4],
    pub clip_radii0: [f32; 4],
    pub clip_rect1: [f32; 4],
    pub clip_radii1: [f32; 4],
    pub clip_meta: [u32; 4],
}

pub(crate) fn finite_difference(lhs: f32, rhs: f32) -> f32 {
    (f64::from(lhs) - f64::from(rhs)).clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32
}

fn clip_behavior_code(behavior: ClipBehavior) -> u32 {
    match behavior {
        ClipBehavior::None => 0,
        ClipBehavior::HardEdge => 1,
        ClipBehavior::AntiAlias => 2,
    }
}

fn exact_clip_slot(ancestor: &RoundedClip, origin: Point) -> ([f32; 4], [f32; 4], u32) {
    let bounds = ancestor.rect();
    (
        [
            finite_difference(bounds.min.x, origin.x),
            finite_difference(bounds.min.y, origin.y),
            finite_difference(bounds.max.x, bounds.min.x),
            finite_difference(bounds.max.y, bounds.min.y),
        ],
        ancestor.radii().as_array(),
        clip_behavior_code(ancestor.behavior()),
    )
}

/// Packs ancestor clips into the two portable instance slots.
///
/// More than two active clips collapse remaining ancestors into an inscribed
/// hard rectangle so content cannot escape an authoritative ancestor.
fn collapse_ancestor_insets(
    ancestor: &RoundedClip,
    min_x: &mut f32,
    min_y: &mut f32,
    max_x: &mut f32,
    max_y: &mut f32,
) {
    let bounds = ancestor.rect();
    let inset = ancestor.radii().as_array().into_iter().fold(0.0, f32::max);
    let inset_min_x = (f64::from(bounds.min.x) + f64::from(inset))
        .clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32;
    let inset_min_y = (f64::from(bounds.min.y) + f64::from(inset))
        .clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32;
    let inset_max_x = (f64::from(bounds.max.x) - f64::from(inset))
        .clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32;
    let inset_max_y = (f64::from(bounds.max.y) - f64::from(inset))
        .clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32;
    *min_x = (*min_x).max(inset_min_x);
    *min_y = (*min_y).max(inset_min_y);
    *max_x = (*max_x).min(inset_max_x);
    *max_y = (*max_y).min(inset_max_y);
}

pub(crate) fn pack_active_clips(clips: &[RoundedClip], origin: Point) -> PackedClips {
    let mut active = clips.iter().filter(|clip| clip.behavior() != ClipBehavior::None);
    let Some(first) = active.next() else {
        return PackedClips::default();
    };

    let mut packed = PackedClips::default();
    (packed.clip_rect0, packed.clip_radii0, packed.clip_meta[1]) =
        exact_clip_slot(first, origin);

    let Some(second) = active.next() else {
        packed.clip_meta[0] = 1;
        return packed;
    };

    packed.clip_meta[0] = 2;
    let Some(third) = active.next() else {
        (packed.clip_rect1, packed.clip_radii1, packed.clip_meta[2]) =
            exact_clip_slot(second, origin);
        return packed;
    };

    let mut min_x = -f32::MAX;
    let mut min_y = -f32::MAX;
    let mut max_x = f32::MAX;
    let mut max_y = f32::MAX;

    collapse_ancestor_insets(second, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
    collapse_ancestor_insets(third, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
    for ancestor in active {
        collapse_ancestor_insets(ancestor, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
    }

    packed.clip_rect1 = [
        finite_difference(min_x, origin.x),
        finite_difference(min_y, origin.y),
        finite_difference(max_x.max(min_x), min_x),
        finite_difference(max_y.max(min_y), min_y),
    ];
    packed.clip_meta[2] = if max_x <= min_x || max_y <= min_y {
        3
    } else {
        1
    };
    packed
}

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

impl PartialEq for Viewport {
    fn eq(&self, other: &Self) -> bool {
        self.physical_size == other.physical_size
            && self.scale_factor.to_bits() == other.scale_factor.to_bits()
    }
}

impl Eq for Viewport {}

impl Viewport {
    /// Creates a new Viewport with the given physical size and scale factor.
    ///
    /// Non-finite or non-positive scale factors are normalized to `1.0`.
    pub fn new(physical_width: u32, physical_height: u32, scale_factor: f32) -> Self {
        let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        let logical_width = physical_width as f32 / scale_factor;
        let logical_height = physical_height as f32 / scale_factor;
        Viewport {
            logical_size: Size::new(logical_width, logical_height),
            physical_size: (physical_width, physical_height),
            scale_factor,
        }
    }

    /// Converts retained logical rounded-clip geometry to physical pixels
    /// without introducing raster rounding or GPU state.
    ///
    /// The public logical and viewport values are finite, but their product
    /// can exceed `f32` range. Saturation preserves a finite renderer boundary
    /// for a later GPU encoder while leaving all representable products exact.
    pub fn to_physical_clip(&self, clip: &RoundedClip) -> PhysicalRoundedClip {
        let rect = clip.rect();
        let radii = clip
            .radii()
            .as_array()
            .map(|radius| scale_to_physical(radius, self.scale_factor));
        PhysicalRoundedClip {
            min: (
                scale_to_physical(rect.min.x, self.scale_factor),
                scale_to_physical(rect.min.y, self.scale_factor),
            ),
            max: (
                scale_to_physical(rect.max.x, self.scale_factor),
                scale_to_physical(rect.max.y, self.scale_factor),
            ),
            radii,
            behavior: clip.behavior(),
        }
    }

    /// Returns true when both physical dimensions are non-zero.
    pub fn is_drawable(&self) -> bool {
        self.physical_size.0 > 0 && self.physical_size.1 > 0
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

fn scale_to_physical(value: f32, scale_factor: f32) -> f32 {
    let scaled = f64::from(value) * f64::from(scale_factor);
    scaled.clamp(f64::from(-f32::MAX), f64::from(f32::MAX)) as f32
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
    fn physical_rounded_clip_preserves_fractional_scaled_geometry() {
        use crate::decoration::{BorderRadius, ClipBehavior};
        use crate::scene::clip::RoundedClip;

        let clip = RoundedClip::new(
            Rect::from_min_size(Point::new(1.25, 2.5), Size::new(10.0, 8.0)),
            BorderRadius::only(1.0, 2.0, 3.0, 4.0).unwrap(),
            ClipBehavior::AntiAlias,
        )
        .unwrap();
        let physical = Viewport::new(600, 400, 1.5).to_physical_clip(&clip);

        assert_eq!(physical.min(), (1.875, 3.75));
        assert_eq!(physical.max(), (16.875, 15.75));
        assert_eq!(physical.radii(), [1.5, 3.0, 4.5, 6.0]);
        assert_eq!(physical.behavior(), ClipBehavior::AntiAlias);
    }

    #[test]
    fn physical_rounded_clip_scales_at_one_and_two_times_without_rounding() {
        use crate::decoration::{BorderRadius, ClipBehavior};
        use crate::scene::clip::RoundedClip;

        let clip = RoundedClip::new(
            Rect::from_min_size(Point::new(1.25, 2.5), Size::new(4.0, 5.0)),
            BorderRadius::all(1.25).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap();

        let at_one = Viewport::new(100, 100, 1.0).to_physical_clip(&clip);
        let at_two = Viewport::new(200, 200, 2.0).to_physical_clip(&clip);

        assert_eq!(at_one.min(), (1.25, 2.5));
        assert_eq!(at_one.max(), (5.25, 7.5));
        assert_eq!(at_one.radii(), [1.25; 4]);
        assert_eq!(at_two.min(), (2.5, 5.0));
        assert_eq!(at_two.max(), (10.5, 15.0));
        assert_eq!(at_two.radii(), [2.5; 4]);
    }

    #[test]
    fn physical_rounded_clip_saturates_extreme_finite_products() {
        use crate::decoration::{BorderRadius, ClipBehavior};
        use crate::scene::clip::RoundedClip;

        let clip = RoundedClip::new(
            Rect {
                min: Point::ZERO,
                max: Point::new(f32::MAX, 1.0),
            },
            BorderRadius::all(1.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap();
        let physical = Viewport::new(1, 1, 2.0).to_physical_clip(&clip);

        assert_eq!(physical.max().0, f32::MAX);
        assert!(
            [
                physical.min().0,
                physical.min().1,
                physical.max().0,
                physical.max().1,
                physical.radii()[0],
                physical.radii()[1],
                physical.radii()[2],
                physical.radii()[3],
            ]
            .into_iter()
            .all(f32::is_finite)
        );
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
    fn should_report_drawable_only_when_both_physical_dimensions_are_non_zero() {
        // Arrange
        let drawable = Viewport::new(800, 600, 1.0);
        let zero_width = Viewport::new(0, 600, 1.0);
        let zero_height = Viewport::new(800, 0, 1.0);

        // Assert
        assert!(drawable.is_drawable());
        assert!(!zero_width.is_drawable());
        assert!(!zero_height.is_drawable());
    }

    #[test]
    fn should_normalize_invalid_scale_factor_to_one() {
        // Arrange / Act
        let viewport = Viewport::new(800, 600, f32::NAN);

        // Assert
        assert_eq!(viewport.scale_factor, 1.0);
        assert_eq!(viewport.logical_size, Size::new(800.0, 600.0));
    }

    #[test]
    fn should_compare_viewports_by_physical_size_and_scale_only() {
        // Arrange
        let first = Viewport::new(800, 600, 2.0);
        let same = Viewport::new(800, 600, 2.0);
        let different_scale = Viewport::new(800, 600, 1.0);

        // Assert
        assert_eq!(first, same);
        assert_ne!(first, different_scale);
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

    fn test_clip(origin: Point, size: Size, radius: f32, behavior: ClipBehavior) -> RoundedClip {
        RoundedClip::new(
            Rect::from_min_size(origin, size),
            crate::decoration::BorderRadius::all(radius).unwrap(),
            behavior,
        )
        .unwrap()
    }

    #[test]
    fn should_filter_none_clips_when_packing_active_slots() {
        // Arrange
        let clips = [
            test_clip(Point::ZERO, Size::new(20.0, 20.0), 4.0, ClipBehavior::None),
            test_clip(
                Point::new(1.0, 2.0),
                Size::new(10.0, 8.0),
                3.0,
                ClipBehavior::HardEdge,
            ),
        ];

        // Act
        let packed = pack_active_clips(&clips, Point::ZERO);

        // Assert
        assert_eq!(packed.clip_meta[0], 1);
        assert_eq!(packed.clip_meta[1], 1);
        assert_eq!(packed.clip_rect0, [1.0, 2.0, 10.0, 8.0]);
        assert_eq!(packed.clip_radii0, [3.0; 4]);
        assert_eq!(packed.clip_meta[2], 0);
    }

    #[test]
    fn should_pack_two_exact_slots_when_two_clips_are_active() {
        // Arrange
        let clips = [
            test_clip(
                Point::ZERO,
                Size::new(32.0, 32.0),
                8.0,
                ClipBehavior::HardEdge,
            ),
            test_clip(
                Point::new(4.0, 4.0),
                Size::new(24.0, 24.0),
                2.0,
                ClipBehavior::AntiAlias,
            ),
        ];

        // Act
        let packed = pack_active_clips(&clips, Point::new(1.0, 1.0));

        // Assert
        assert_eq!(packed.clip_meta[0], 2);
        assert_eq!(packed.clip_meta[1], 1);
        assert_eq!(packed.clip_meta[2], 2);
        assert_eq!(packed.clip_rect0, [-1.0, -1.0, 32.0, 32.0]);
        assert_eq!(packed.clip_radii0, [8.0; 4]);
        assert_eq!(packed.clip_rect1, [3.0, 3.0, 24.0, 24.0]);
        assert_eq!(packed.clip_radii1, [2.0; 4]);
    }

    #[test]
    fn should_collapse_remaining_ancestors_to_inscribed_hard_rect_when_more_than_two_clips() {
        // Arrange
        let clips = [
            test_clip(
                Point::ZERO,
                Size::new(32.0, 32.0),
                8.0,
                ClipBehavior::HardEdge,
            ),
            test_clip(
                Point::new(4.0, 4.0),
                Size::new(24.0, 24.0),
                4.0,
                ClipBehavior::AntiAlias,
            ),
            test_clip(
                Point::new(8.0, 8.0),
                Size::new(16.0, 16.0),
                2.0,
                ClipBehavior::HardEdge,
            ),
        ];

        // Act
        let packed = pack_active_clips(&clips, Point::ZERO);

        // Assert
        assert_eq!(packed.clip_meta[0], 2);
        assert_eq!(packed.clip_meta[1], 1);
        assert_eq!(packed.clip_meta[2], 1);
        assert_eq!(packed.clip_rect0, [0.0, 0.0, 32.0, 32.0]);
        assert_eq!(packed.clip_rect1, [10.0, 10.0, 12.0, 12.0]);
        assert_eq!(packed.clip_radii1, [0.0; 4]);
    }

    #[test]
    fn should_encode_empty_coverage_when_collapsed_intersection_is_empty() {
        // Arrange
        let clips = [
            test_clip(
                Point::ZERO,
                Size::new(20.0, 20.0),
                0.0,
                ClipBehavior::HardEdge,
            ),
            test_clip(
                Point::ZERO,
                Size::new(20.0, 20.0),
                12.0,
                ClipBehavior::HardEdge,
            ),
            test_clip(
                Point::new(16.0, 16.0),
                Size::new(4.0, 4.0),
                2.0,
                ClipBehavior::HardEdge,
            ),
        ];

        // Act
        let packed = pack_active_clips(&clips, Point::ZERO);

        // Assert
        assert_eq!(packed.clip_meta[0], 2);
        assert_eq!(packed.clip_meta[2], 3);
    }
}
