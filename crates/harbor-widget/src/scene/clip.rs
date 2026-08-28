use crate::decoration::{BorderRadius, ClipBehavior, DecorationError, NormalizedBorderRadius};
use crate::layout::{Point, Rect};

/// Retained logical geometry for clipping child content to a rounded rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundedClip {
    rect: Rect,
    radii: NormalizedBorderRadius,
    behavior: ClipBehavior,
}

impl RoundedClip {
    /// Validates finite, non-negative bounds and normalizes the supplied radii
    /// to fit them once at construction.
    pub fn new(
        rect: Rect,
        radius: BorderRadius,
        behavior: ClipBehavior,
    ) -> Result<Self, DecorationError> {
        let (width, height) = validate_rect(rect)?;
        let radii = radius.normalize_extents(width, height);
        Ok(Self {
            rect,
            radii,
            behavior,
        })
    }

    pub const fn rect(&self) -> Rect {
        self.rect
    }

    pub const fn radii(&self) -> NormalizedBorderRadius {
        self.radii
    }

    pub const fn behavior(&self) -> ClipBehavior {
        self.behavior
    }

    /// Returns whether `point` is inside this clip using the HardEdge inclusion rule.
    ///
    /// The axis-aligned bounds use the same half-open rectangle as [`Rect::contains`].
    /// A point in a corner square is inside only when it also lies on or inside that
    /// corner's circular arc (`distance <= 0`). Zero radii therefore match rectangular
    /// hit testing.
    pub fn contains(&self, point: Point) -> bool {
        if !self.rect.contains(point) {
            return false;
        }
        let size = self.rect.size();
        let local_x = f64::from(point.x) - f64::from(self.rect.min.x);
        let local_y = f64::from(point.y) - f64::from(self.rect.min.y);
        let width = f64::from(size.width);
        let height = f64::from(size.height);
        let radii = self.radii.as_array();
        inside_corner(local_x, local_y, f64::from(radii[0]), 0.0, 0.0)
            && inside_corner(local_x, local_y, f64::from(radii[1]), width, 0.0)
            && inside_corner(local_x, local_y, f64::from(radii[2]), width, height)
            && inside_corner(local_x, local_y, f64::from(radii[3]), 0.0, height)
    }
}

fn inside_corner(local_x: f64, local_y: f64, radius: f64, corner_x: f64, corner_y: f64) -> bool {
    if radius <= 0.0 {
        return true;
    }
    let inward_x = if corner_x == 0.0 {
        radius
    } else {
        corner_x - radius
    };
    let inward_y = if corner_y == 0.0 {
        radius
    } else {
        corner_y - radius
    };
    let in_x = if corner_x == 0.0 {
        local_x < radius
    } else {
        local_x > corner_x - radius
    };
    let in_y = if corner_y == 0.0 {
        local_y < radius
    } else {
        local_y > corner_y - radius
    };
    if !(in_x && in_y) {
        return true;
    }
    let dx = local_x - inward_x;
    let dy = local_y - inward_y;
    dx * dx + dy * dy <= radius * radius
}

fn validate_rect(rect: Rect) -> Result<(f64, f64), DecorationError> {
    validate_finite("rect.min.x", rect.min.x)?;
    validate_finite("rect.min.y", rect.min.y)?;
    validate_finite("rect.max.x", rect.max.x)?;
    validate_finite("rect.max.y", rect.max.y)?;

    // Endpoints are finite f32 values, but their distance can exceed f32
    // range. Keep it in f64 through radius normalization.
    let width = f64::from(rect.max.x) - f64::from(rect.min.x);
    let height = f64::from(rect.max.y) - f64::from(rect.min.y);
    if width < 0.0 {
        return Err(DecorationError::Negative {
            field: "rect.width",
        });
    }
    if height < 0.0 {
        return Err(DecorationError::Negative {
            field: "rect.height",
        });
    }
    Ok((width, height))
}

fn validate_finite(field: &'static str, value: f32) -> Result<(), DecorationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(DecorationError::NonFinite { field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Point, Size};

    #[test]
    fn rounded_clip_normalizes_radii_and_retains_behavior() {
        let clip = RoundedClip::new(
            Rect::from_min_size(Point::new(1.0, 2.0), Size::new(100.0, 20.0)),
            BorderRadius::all(50.0).unwrap(),
            ClipBehavior::AntiAlias,
        )
        .unwrap();

        assert_eq!(clip.radii().as_array(), [10.0; 4]);
        assert_eq!(clip.behavior(), ClipBehavior::AntiAlias);
    }

    #[test]
    fn rounded_clip_rejects_malformed_bounds() {
        let radius = BorderRadius::default();
        assert_eq!(
            RoundedClip::new(
                Rect::from_min_size(Point::ZERO, Size::new(-1.0, 1.0)),
                radius,
                ClipBehavior::HardEdge,
            ),
            Err(DecorationError::Negative {
                field: "rect.width"
            })
        );
        assert_eq!(
            RoundedClip::new(
                Rect::from_min_size(Point::new(f32::NAN, 0.0), Size::new(1.0, 1.0)),
                radius,
                ClipBehavior::HardEdge,
            ),
            Err(DecorationError::NonFinite {
                field: "rect.min.x"
            })
        );
    }

    #[test]
    fn rounded_clip_accepts_finite_endpoints_with_an_extreme_extent() {
        let rect = Rect {
            min: Point::new(-f32::MAX, 0.0),
            max: Point::new(f32::MAX, 1.0),
        };

        let clip = RoundedClip::new(rect, BorderRadius::default(), ClipBehavior::HardEdge)
            .expect("finite ordered endpoints must form a valid clip");

        assert_eq!(clip.rect(), rect);
        assert_eq!(clip.radii().as_array(), [0.0; 4]);
    }

    fn hard_clip(rect: Rect, radius: BorderRadius) -> RoundedClip {
        RoundedClip::new(rect, radius, ClipBehavior::HardEdge).unwrap()
    }

    #[test]
    fn should_match_rect_contains_when_radii_are_zero() {
        // Arrange
        let rect = Rect::from_min_size(Point::ZERO, Size::new(10.0, 8.0));
        let clip = hard_clip(rect, BorderRadius::default());

        // Act / Assert
        for point in [
            Point::new(0.0, 0.0),
            Point::new(9.0, 7.0),
            Point::new(10.0, 4.0),
            Point::new(5.0, 8.0),
        ] {
            assert_eq!(clip.contains(point), rect.contains(point));
        }
    }

    #[test]
    fn should_include_interior_point_when_radii_are_nonzero() {
        // Arrange
        let clip = hard_clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            BorderRadius::all(8.0).unwrap(),
        );

        // Act
        let inside = clip.contains(Point::new(10.0, 10.0));

        // Assert
        assert!(inside);
    }

    #[test]
    fn should_reject_point_when_inside_corner_cutout() {
        // Arrange
        let clip = hard_clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            BorderRadius::all(8.0).unwrap(),
        );

        // Act
        let in_cutout = clip.contains(Point::new(0.0, 0.0));

        // Assert
        assert!(!in_cutout);
    }

    #[test]
    fn should_include_point_when_exactly_on_corner_arc() {
        // Arrange
        let clip = RoundedClip::new(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            BorderRadius::all(8.0).unwrap(),
            ClipBehavior::AntiAlias,
        )
        .unwrap();

        // Act
        // 3-4-5 from the top-left corner center (8, 8) lands on the arc.
        let on_arc = clip.contains(Point::new(5.0, 4.0));

        // Assert
        assert!(on_arc);
    }

    #[test]
    fn should_include_point_when_on_straight_edge_outside_corner_square() {
        // Arrange
        let clip = hard_clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            BorderRadius::all(8.0).unwrap(),
        );

        // Act
        let on_edge = clip.contains(Point::new(8.0, 0.0));

        // Assert
        assert!(on_edge);
    }

    #[test]
    fn should_contain_nothing_when_rect_is_empty() {
        // Arrange
        let clip = hard_clip(
            Rect::from_min_size(Point::new(4.0, 6.0), Size::ZERO),
            BorderRadius::all(4.0).unwrap(),
        );

        // Act
        let origin = clip.contains(Point::new(4.0, 6.0));
        let nearby = clip.contains(Point::new(4.1, 6.1));

        // Assert
        assert!(!origin);
        assert!(!nearby);
    }

    #[test]
    fn should_use_normalized_radii_when_requested_radii_exceed_box() {
        // Arrange
        let clip = hard_clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 10.0)),
            BorderRadius::all(50.0).unwrap(),
        );

        // Act
        let cutout = clip.contains(Point::new(0.0, 0.0));
        let along_top = clip.contains(Point::new(5.0, 0.0));

        // Assert
        assert_eq!(clip.radii().as_array(), [5.0; 4]);
        assert!(!cutout);
        assert!(along_top);
    }

    #[test]
    fn should_reject_only_rounded_corners_when_radii_are_independent() {
        // Arrange
        let clip = hard_clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            BorderRadius::only(8.0, 0.0, 0.0, 0.0).unwrap(),
        );

        // Act
        let top_left = clip.contains(Point::new(0.0, 0.0));
        let top_right = clip.contains(Point::new(19.0, 0.0));
        let bottom_left = clip.contains(Point::new(0.0, 19.0));
        let on_arc = clip.contains(Point::new(5.0, 4.0));

        // Assert
        assert!(!top_left);
        assert!(top_right);
        assert!(bottom_left);
        assert!(on_arc);
    }
}
