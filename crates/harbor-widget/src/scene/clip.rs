use crate::decoration::{BorderRadius, ClipBehavior, DecorationError, NormalizedBorderRadius};
use crate::layout::Rect;

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
}
