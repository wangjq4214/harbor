use super::{LayoutError, Size};

/// A raw layout scalar was not finite and nonnegative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfigError {
    pub field: &'static str,
    pub value: f32,
}

impl std::fmt::Display for LayoutConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} must be finite and nonnegative; got {}",
            self.field, self.value
        )
    }
}

impl std::error::Error for LayoutConfigError {}

pub(crate) fn validate_scalar(field: &'static str, value: f32) -> Result<(), LayoutConfigError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(LayoutConfigError { field, value })
    }
}

/// Plain input shared by Padding and Align. Consuming components validate sides.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Insets {
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub(crate) fn validate(self) -> Result<(), LayoutConfigError> {
        for (field, value) in [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
        ] {
            validate_scalar(field, value)?;
        }
        Ok(())
    }

    pub(crate) fn total(self) -> Size {
        Size::new(self.left + self.right, self.top + self.bottom)
    }

    /// Check derived sums before any parent clamp can conceal overflow.
    pub(crate) fn checked_inflate(self, size: Size) -> Result<Size, LayoutError> {
        let width = f64::from(size.width) + f64::from(self.left) + f64::from(self.right);
        let height = f64::from(size.height) + f64::from(self.top) + f64::from(self.bottom);
        if !size.is_valid() || width > f64::from(f32::MAX) || height > f64::from(f32::MAX) {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Size::new(width as f32, height as f32))
    }
}

/// Position along one axis; unlike flex cross alignment this never stretches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionAlignment {
    #[default]
    Start,
    Center,
    End,
}

impl PositionAlignment {
    pub(crate) fn position(self, child: f32, available: f32) -> f32 {
        let free = (available - child).max(0.0);
        match self {
            Self::Start => 0.0,
            Self::Center => free * 0.5,
            Self::End => free,
        }
    }
}

/// Independent horizontal and vertical positioning, without Stretch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Alignment2D {
    pub horizontal: PositionAlignment,
    pub vertical: PositionAlignment,
}

impl Alignment2D {
    pub const TOP_LEFT: Self = Self::new(PositionAlignment::Start, PositionAlignment::Start);
    pub const TOP_RIGHT: Self = Self::new(PositionAlignment::End, PositionAlignment::Start);
    pub const CENTER: Self = Self::new(PositionAlignment::Center, PositionAlignment::Center);
    pub const BOTTOM_RIGHT: Self = Self::new(PositionAlignment::End, PositionAlignment::End);

    pub const fn new(horizontal: PositionAlignment, vertical: PositionAlignment) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }
}
