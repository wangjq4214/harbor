use super::flex::Axis;
use super::sized_box::SizedBox;
use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// A line painted using the existing box widget, defaulting to one logical pixel.
///
/// The long axis fills a finite maximum, or uses zero (subject to the parent
/// minimum) when unbounded. There is no device-pixel rounding or implicit clip.
#[derive(Clone)]
pub struct Separator {
    axis: Axis,
    thickness: f32,
    color: Color,
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

impl Separator {
    /// Creates a horizontal separator.
    pub fn new() -> Self {
        Self::horizontal()
    }

    pub fn horizontal() -> Self {
        Self {
            axis: Axis::Horizontal,
            thickness: 1.0,
            color: Color::BLACK,
        }
    }

    pub fn vertical() -> Self {
        Self {
            axis: Axis::Vertical,
            ..Self::horizontal()
        }
    }

    /// Sets finite, nonnegative logical-pixel thickness; invalid values cause a
    /// layout error.
    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    fn size(&self, constraints: BoxConstraints) -> Size {
        let maximum = self.axis.main(constraints.max);
        let long = if maximum.is_finite() { maximum } else { 0.0 };
        constraints.constrain(self.axis.size(long, self.thickness))
    }
}

impl Component for Separator {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), Vec::new(), None)
    }
}

impl crate::WithChildren for Separator {
    fn with_children(
        self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.ensure_leaf("Separator")?;
        Ok(self)
    }
}

impl AnyView for Separator {

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        self.size(constraints)
    }

    fn layout(
        &self,
        constraints: BoxConstraints,
        _children: &mut dyn ChildMeasurer,
        _metrics: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        constraints.validate()?;
        if !self.thickness.is_finite() || self.thickness < 0.0 {
            return Err(LayoutError::InvalidConstraints);
        }
        Ok(ParentLayout {
            size: self.size(constraints),
            placements: Vec::new(),
            diagnostics: Vec::new(),
        })
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        SizedBox::new(rect.size())
            .color(self.color)
            .paint_primitives(rect, metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Point;
    use crate::runtime::DEFAULT_TEXT_METRICS;

    #[test]
    fn unbounded_long_axis_uses_zero_or_parent_minimum() {
        for separator in [Separator::horizontal(), Separator::vertical()] {
            let axis = separator.axis;
            let constraints = BoxConstraints::loose(axis.size(f32::INFINITY, 20.0));
            assert_eq!(
                separator.intrinsic_size(constraints, &DEFAULT_TEXT_METRICS),
                axis.size(0.0, 1.0)
            );
            let constraints = BoxConstraints {
                min: axis.size(7.0, 0.0),
                ..constraints
            };
            assert_eq!(
                separator.intrinsic_size(constraints, &DEFAULT_TEXT_METRICS),
                axis.size(7.0, 1.0)
            );
        }
    }

    #[test]
    fn fractional_thickness_uses_existing_box_paint_without_rounding() {
        let separator = Separator::vertical().thickness(1.25).color(Color::RED);
        let size = separator.intrinsic_size(
            BoxConstraints::loose(Size::new(100.0, 80.5)),
            &DEFAULT_TEXT_METRICS,
        );
        assert_eq!(size, Size::new(1.25, 80.5));
        let rect = Rect::from_min_size(Point::new(3.5, 2.25), size);
        assert_eq!(
            separator.paint_primitives(rect, &DEFAULT_TEXT_METRICS),
            SizedBox::new(size)
                .color(Color::RED)
                .paint_primitives(rect, &DEFAULT_TEXT_METRICS)
        );
    }
}
