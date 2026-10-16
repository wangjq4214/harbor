//! Layout-independent decoration paints. Colors use the same linear RGB space as `Color`.
use crate::{layout::Point, scene::primitive::Color};
use std::{error::Error, fmt};

/// A color at a position along a gradient. Validated by `LinearGradient::new`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    offset: f32,
    color: Color,
}

impl GradientStop {
    pub const fn new(offset: f32, color: Color) -> Self {
        Self { offset, color }
    }

    pub const fn offset(&self) -> f32 {
        self.offset
    }
    pub const fn color(&self) -> Color {
        self.color
    }
}

/// Invalid gradient construction input; no partially valid gradient is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GradientError {
    field: &'static str,
    rule: &'static str,
}

impl GradientError {
    pub const fn field(&self) -> &'static str {
        self.field
    }
    pub const fn rule(&self) -> &'static str {
        self.rule
    }
}

impl fmt::Display for GradientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} must be {}", self.field, self.rule)
    }
}
impl Error for GradientError {}

/// A bounded, immutable linear gradient in normalized box coordinates.
///
/// `(0, 0)` is the top-left and `(1, 1)` the bottom-right of the painted box.
/// Projection is evaluated in that normalized coordinate space, so resizing
/// scales the paint with the box. Samples outside the stop interval clamp to
/// its endpoint colors. Stops interpolate in premultiplied linear RGBA, avoiding
/// color fringes next to transparent stops. This is a paint, not a layout/filter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    start: Point,
    end: Point,
    stops: [GradientStop; Self::MAX_STOPS],
    count: u32,
}

impl LinearGradient {
    /// Bounds both retained data and fragment sampling work.
    pub const MAX_STOPS: usize = 8;

    /// Accepts 2–8 strictly increasing offsets in [0, 1], finite colors with
    /// alpha in [0, 1], and distinct normalized endpoints with a representable
    /// projection. Endpoints and stops are validated once at construction.
    pub fn new(start: Point, end: Point, stops: &[GradientStop]) -> Result<Self, GradientError> {
        let invalid = |field, rule| GradientError { field, rule };
        for value in [start.x, start.y, end.x, end.y] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(invalid("gradient.endpoints", "finite and in [0, 1]"));
            }
        }
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx * dx + dy * dy;
        // WGSL backends may flush subnormal squared lengths to zero.
        if length_squared < f32::MIN_POSITIVE {
            return Err(invalid("gradient.direction", "nondegenerate"));
        }
        if !(2..=Self::MAX_STOPS).contains(&stops.len()) {
            return Err(invalid("gradient.stops", "between 2 and 8 stops"));
        }
        let mut previous = -1.0;
        for stop in stops {
            if !stop.offset.is_finite()
                || !(0.0..=1.0).contains(&stop.offset)
                || stop.offset <= previous
            {
                return Err(invalid(
                    "gradient.offsets",
                    "strictly increasing and in [0, 1]",
                ));
            }
            if !stop.color.is_finite() || !(0.0..=1.0).contains(&stop.color.a) {
                return Err(invalid("gradient.colors", "finite with alpha in [0, 1]"));
            }
            previous = stop.offset;
        }
        let mut values = [GradientStop::new(0.0, Color::TRANSPARENT); Self::MAX_STOPS];
        values[..stops.len()].copy_from_slice(stops);
        Ok(Self {
            start,
            end,
            stops: values,
            count: stops.len() as u32,
        })
    }

    pub const fn start(&self) -> Point {
        self.start
    }
    pub const fn end(&self) -> Point {
        self.end
    }
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops[..self.count as usize]
    }
}

/// Shared paint input for decoration fills and border strokes.
/// Solid colors retain the existing finite-color validation at decoration APIs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    Solid(Color),
    LinearGradient(LinearGradient),
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}
impl From<LinearGradient> for Paint {
    fn from(gradient: LinearGradient) -> Self {
        Self::LinearGradient(gradient)
    }
}

impl Paint {
    pub const fn solid_color(&self) -> Option<Color> {
        match self {
            Self::Solid(color) => Some(*color),
            Self::LinearGradient(_) => None,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        match self {
            Self::Solid(color) => color.a > 0.0,
            Self::LinearGradient(gradient) => {
                gradient.stops().iter().any(|stop| stop.color.a > 0.0)
            }
        }
    }

    pub(crate) const fn first_color(&self) -> Color {
        match self {
            Self::Solid(color) => *color,
            Self::LinearGradient(gradient) => gradient.stops[0].color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_gradient_contract_without_silently_sorting_or_truncating() {
        let stops = [
            GradientStop::new(0.0, Color::RED),
            GradientStop::new(1.0, Color::BLUE),
        ];
        let a = Point::ZERO;
        let b = Point::new(1.0, 1.0);
        let gradient = LinearGradient::new(a, b, &stops).unwrap();
        assert_eq!(gradient.stops(), stops);
        assert_eq!(gradient.start(), a);
        assert_eq!(gradient.end(), b);
        for end in [
            a,
            Point::new(f32::NAN, 0.0),
            Point::new(2.0, 0.0),
            Point::new(f32::MIN_POSITIVE, 0.0),
            Point::new(1.0e-19, 0.0),
        ] {
            assert!(LinearGradient::new(a, end, &stops).is_err());
        }
        for invalid in [
            vec![],
            vec![stops[0]],
            vec![stops[0]; 9],
            vec![stops[1], stops[0]],
            vec![stops[0], stops[0]],
        ] {
            assert!(LinearGradient::new(a, b, &invalid).is_err());
        }
        for offset in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(
                LinearGradient::new(a, b, &[stops[0], GradientStop::new(offset, Color::BLUE)])
                    .is_err()
            );
        }
        for color in [
            Color {
                r: f32::NAN,
                ..Color::WHITE
            },
            Color {
                a: 1.1,
                ..Color::WHITE
            },
            Color {
                a: -0.1,
                ..Color::WHITE
            },
        ] {
            assert!(LinearGradient::new(a, b, &[stops[0], GradientStop::new(1.0, color)]).is_err());
        }
        let max =
            std::array::from_fn::<_, 8, _>(|i| GradientStop::new(i as f32 / 7.0, Color::WHITE));
        assert_eq!(LinearGradient::new(a, b, &max).unwrap().stops().len(), 8);
    }

    #[test]
    fn paint_visibility_checks_all_stops() {
        let transparent = GradientStop::new(0.0, Color::TRANSPARENT);
        let gradient = LinearGradient::new(
            Point::ZERO,
            Point::new(0.0, 1.0),
            &[transparent, GradientStop::new(1.0, Color::WHITE)],
        )
        .unwrap();
        assert!(Paint::from(gradient).is_visible());
        assert_eq!(Paint::from(gradient).solid_color(), None);
        assert_eq!(Paint::from(Color::RED).solid_color(), Some(Color::RED));
        let empty = LinearGradient::new(
            Point::ZERO,
            Point::new(1.0, 0.0),
            &[transparent, GradientStop::new(1.0, Color::TRANSPARENT)],
        )
        .unwrap();
        assert!(!Paint::from(empty).is_visible());
    }
}
