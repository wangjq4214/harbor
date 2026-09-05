use crate::layout::{Point, Size};
use crate::scene::primitive::Color;
use std::error::Error;
use std::fmt;

/// The reason a decoration value was rejected at its public construction boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecorationError {
    /// A scalar component was NaN or infinite.
    NonFinite { field: &'static str },
    /// A scalar that must be non-negative was negative.
    Negative { field: &'static str },
}

impl DecorationError {
    /// Identifies the invalid input component.
    pub const fn field(&self) -> &'static str {
        match self {
            Self::NonFinite { field } | Self::Negative { field } => field,
        }
    }

    /// Describes the validation rule that the input violated.
    pub const fn rule(&self) -> &'static str {
        match self {
            Self::NonFinite { .. } => "finite",
            Self::Negative { .. } => "non-negative",
        }
    }
}

impl fmt::Display for DecorationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} must be {}", self.field(), self.rule())
    }
}

impl Error for DecorationError {}

fn validate_finite(field: &'static str, value: f32) -> Result<(), DecorationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(DecorationError::NonFinite { field })
    }
}

fn validate_non_negative(field: &'static str, value: f32) -> Result<(), DecorationError> {
    validate_finite(field, value)?;
    if value < 0.0 {
        Err(DecorationError::Negative { field })
    } else {
        Ok(())
    }
}

fn validate_color(field: &'static str, color: Color) -> Result<(), DecorationError> {
    if color.is_finite() {
        Ok(())
    } else {
        Err(DecorationError::NonFinite { field })
    }
}

/// Four circular corner radii in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderRadius {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

impl BorderRadius {
    /// Constructs the same validated radius for every corner.
    pub fn all(radius: f32) -> Result<Self, DecorationError> {
        Self::only(radius, radius, radius, radius)
    }

    /// Constructs individually validated radii in clockwise corner order.
    pub fn only(
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    ) -> Result<Self, DecorationError> {
        validate_non_negative("top_left", top_left)?;
        validate_non_negative("top_right", top_right)?;
        validate_non_negative("bottom_right", bottom_right)?;
        validate_non_negative("bottom_left", bottom_left)?;
        Ok(Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        })
    }

    pub const fn top_left(&self) -> f32 {
        self.top_left
    }

    pub const fn top_right(&self) -> f32 {
        self.top_right
    }

    pub const fn bottom_right(&self) -> f32 {
        self.bottom_right
    }

    pub const fn bottom_left(&self) -> f32 {
        self.bottom_left
    }

    /// Returns the radii in clockwise corner order: top-left, top-right,
    /// bottom-right, bottom-left.
    pub const fn as_array(&self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    /// Fits this radius proportionally within a finite, non-negative box.
    pub fn normalize(&self, size: Size) -> Result<NormalizedBorderRadius, DecorationError> {
        validate_non_negative("width", size.width)?;
        validate_non_negative("height", size.height)?;

        Ok(self.normalize_extents(f64::from(size.width), f64::from(size.height)))
    }

    /// Fits this radius within already-validated finite logical extents.
    ///
    /// `RoundedClip` supplies f64 extents here so two finite f32 endpoints
    /// cannot overflow while their distance is being calculated.
    pub(crate) fn normalize_extents(&self, width: f64, height: f64) -> NormalizedBorderRadius {
        debug_assert!(width.is_finite() && width >= 0.0);
        debug_assert!(height.is_finite() && height >= 0.0);

        if width == 0.0 || height == 0.0 {
            return NormalizedBorderRadius::ZERO;
        }

        // Calculate the adjacent sums and scale in f64. Two valid f32 radii
        // can overflow when added as f32, which would otherwise collapse all
        // corners to zero rather than preserving their proportions.
        let ratios = [
            (width, f64::from(self.top_left) + f64::from(self.top_right)),
            (
                width,
                f64::from(self.bottom_left) + f64::from(self.bottom_right),
            ),
            (
                height,
                f64::from(self.top_left) + f64::from(self.bottom_left),
            ),
            (
                height,
                f64::from(self.top_right) + f64::from(self.bottom_right),
            ),
        ];
        let scale = ratios
            .into_iter()
            .filter_map(|(extent, sum)| (sum > 0.0).then_some(extent / sum))
            .fold(1.0_f64, f64::min);

        NormalizedBorderRadius {
            top_left: (f64::from(self.top_left) * scale) as f32,
            top_right: (f64::from(self.top_right) * scale) as f32,
            bottom_right: (f64::from(self.bottom_right) * scale) as f32,
            bottom_left: (f64::from(self.bottom_left) * scale) as f32,
        }
    }
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    }
}

/// Corner radii that are known to fit one finite logical box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedBorderRadius {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

impl NormalizedBorderRadius {
    const ZERO: Self = Self {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    pub const fn top_left(&self) -> f32 {
        self.top_left
    }

    pub const fn top_right(&self) -> f32 {
        self.top_right
    }

    pub const fn bottom_right(&self) -> f32 {
        self.bottom_right
    }

    pub const fn bottom_left(&self) -> f32 {
        self.bottom_left
    }

    /// Returns the radii in clockwise corner order: top-left, top-right,
    /// bottom-right, bottom-left.
    pub const fn as_array(&self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

/// A uniform four-edge outline in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    color: Color,
    width: f32,
}

impl Border {
    /// Creates a finite-color, non-negative-width border.
    pub fn all(color: Color, width: f32) -> Result<Self, DecorationError> {
        validate_color("color", color)?;
        validate_non_negative("width", width)?;
        Ok(Self { color, width })
    }

    pub const fn color(&self) -> Color {
        self.color
    }

    pub const fn width(&self) -> f32 {
        self.width
    }
}

impl Default for Border {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            width: 0.0,
        }
    }
}

/// One layout-neutral outer-shadow configuration in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    color: Color,
    offset: Point,
    blur_radius: f32,
    spread_radius: f32,
}

impl BoxShadow {
    /// Starts with the no-op shadow value.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces this shadow's color after validating all color components.
    pub fn try_color(mut self, color: Color) -> Result<Self, DecorationError> {
        validate_color("color", color)?;
        self.color = color;
        Ok(self)
    }

    /// Replaces this shadow's offset after validating both components.
    pub fn try_offset(mut self, offset: Point) -> Result<Self, DecorationError> {
        validate_finite("offset.x", offset.x)?;
        validate_finite("offset.y", offset.y)?;
        self.offset = offset;
        Ok(self)
    }

    /// Replaces this shadow's non-negative blur radius.
    pub fn try_blur_radius(mut self, blur_radius: f32) -> Result<Self, DecorationError> {
        validate_non_negative("blur_radius", blur_radius)?;
        self.blur_radius = blur_radius;
        Ok(self)
    }

    /// Replaces this shadow's finite signed spread radius.
    pub fn try_spread_radius(mut self, spread_radius: f32) -> Result<Self, DecorationError> {
        validate_finite("spread_radius", spread_radius)?;
        self.spread_radius = spread_radius;
        Ok(self)
    }

    pub const fn color(&self) -> Color {
        self.color
    }

    pub const fn offset(&self) -> Point {
        self.offset
    }

    pub const fn blur_radius(&self) -> f32 {
        self.blur_radius
    }

    pub const fn spread_radius(&self) -> f32 {
        self.spread_radius
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            offset: Point::ZERO,
            blur_radius: 0.0,
            spread_radius: 0.0,
        }
    }
}

/// Quality policy for clipping child content to a rounded box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipBehavior {
    /// Do not clip child content.
    #[default]
    None,
    /// Clip to the nearest pixel edge.
    HardEdge,
    /// Clip with anti-aliased edges.
    AntiAlias,
}

/// Immutable, layout-neutral optional box paint values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxDecoration {
    color: Option<Color>,
    border: Option<Border>,
    border_radius: Option<BorderRadius>,
    shadows: Vec<BoxShadow>,
}

impl BoxDecoration {
    /// Starts with no fill, border, radius, or shadows.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces the fill color after validating it.
    pub fn try_color(mut self, color: Color) -> Result<Self, DecorationError> {
        validate_color("color", color)?;
        self.color = Some(color);
        Ok(self)
    }

    /// Adds or replaces the uniform border.
    pub fn border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }

    /// Adds or replaces the corner radii.
    pub fn border_radius(mut self, border_radius: BorderRadius) -> Self {
        self.border_radius = Some(border_radius);
        self
    }

    /// Appends an outer shadow, preserving caller paint order.
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    pub const fn color(&self) -> Option<Color> {
        self.color
    }

    pub const fn border_value(&self) -> Option<Border> {
        self.border
    }

    pub const fn border_radius_value(&self) -> Option<BorderRadius> {
        self.border_radius
    }

    pub fn shadows(&self) -> &[BoxShadow] {
        &self.shadows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_no_op_values() {
        assert_eq!(BoxDecoration::default(), BoxDecoration::new());
        assert_eq!(BoxDecoration::default().color(), None);
        assert_eq!(BoxDecoration::default().border_value(), None);
        assert_eq!(BoxDecoration::default().border_radius_value(), None);
        assert!(BoxDecoration::default().shadows().is_empty());
        assert_eq!(Border::default().color(), Color::TRANSPARENT);
        assert_eq!(Border::default().width(), 0.0);
        assert_eq!(BoxShadow::default().offset(), Point::ZERO);
        assert_eq!(ClipBehavior::default(), ClipBehavior::None);
    }

    #[test]
    fn values_reject_invalid_inputs_but_accept_signed_spread() {
        assert_eq!(
            Border::all(Color::WHITE, -1.0),
            Err(DecorationError::Negative { field: "width" })
        );
        assert_eq!(
            BoxShadow::default().try_blur_radius(f32::NAN),
            Err(DecorationError::NonFinite {
                field: "blur_radius"
            })
        );
        assert_eq!(
            BorderRadius::only(1.0, 2.0, f32::INFINITY, 4.0),
            Err(DecorationError::NonFinite {
                field: "bottom_right"
            })
        );
        assert_eq!(
            BoxShadow::default()
                .try_spread_radius(-4.0)
                .unwrap()
                .spread_radius(),
            -4.0
        );
    }

    #[test]
    fn each_public_validation_boundary_reports_its_field_and_rule() {
        assert_eq!(
            BorderRadius::all(-1.0),
            Err(DecorationError::Negative { field: "top_left" })
        );
        assert_eq!(
            BoxShadow::default().try_color(Color {
                b: f32::NAN,
                ..Color::WHITE
            }),
            Err(DecorationError::NonFinite { field: "color" })
        );
        assert_eq!(
            BoxShadow::default().try_offset(Point::new(f32::INFINITY, 0.0)),
            Err(DecorationError::NonFinite { field: "offset.x" })
        );
        assert_eq!(
            BoxShadow::default().try_blur_radius(-1.0),
            Err(DecorationError::Negative {
                field: "blur_radius"
            })
        );
        assert_eq!(
            BoxShadow::default().try_spread_radius(f32::NEG_INFINITY),
            Err(DecorationError::NonFinite {
                field: "spread_radius"
            })
        );
        let error = BoxDecoration::default()
            .try_color(Color {
                a: f32::NAN,
                ..Color::WHITE
            })
            .unwrap_err();
        assert_eq!(error.field(), "color");
        assert_eq!(error.rule(), "finite");
        assert_eq!(
            BorderRadius::default().normalize(Size::new(-1.0, 1.0)),
            Err(DecorationError::Negative { field: "width" })
        );
    }

    #[test]
    fn box_decoration_replaces_single_values_and_preserves_shadow_order() {
        let first = BoxShadow::default().try_spread_radius(-2.0).unwrap();
        let second = BoxShadow::default().try_spread_radius(3.0).unwrap();
        let decoration = BoxDecoration::default()
            .try_color(Color::RED)
            .unwrap()
            .border(Border::all(Color::GREEN, 1.5).unwrap())
            .border_radius(BorderRadius::all(4.0).unwrap())
            .shadow(first)
            .shadow(second);

        assert_eq!(decoration.color(), Some(Color::RED));
        assert_eq!(decoration.border_value().unwrap().width(), 1.5);
        assert_eq!(decoration.border_radius_value().unwrap().top_left(), 4.0);
        assert_eq!(decoration.shadows(), &[first, second]);
    }

    #[test]
    fn radius_normalization_uses_one_proportional_scale_factor() {
        let radii = BorderRadius::only(80.0, 40.0, 60.0, 20.0).unwrap();
        let normalized = radii.normalize(Size::new(100.0, 60.0)).unwrap();

        assert_eq!(normalized.as_array(), [48.0, 24.0, 36.0, 12.0]);
        assert_eq!(
            BorderRadius::all(5.0)
                .unwrap()
                .normalize(Size::ZERO)
                .unwrap()
                .as_array(),
            [0.0; 4]
        );

        let extreme = BorderRadius::all(f32::MAX)
            .unwrap()
            .normalize(Size::new(f32::MAX, f32::MAX))
            .unwrap();
        assert_eq!(extreme.as_array(), [f32::MAX / 2.0; 4]);
    }
}
