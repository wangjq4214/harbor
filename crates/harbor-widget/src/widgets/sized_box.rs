use crate::layout::configuration::validate_scalar;
use crate::layout::{BoxConstraints, LayoutConfigError, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Optional single-child sizing. Requested axes are tight within parent bounds;
/// omitted axes inherit the parent interval and follow content (or zero).
#[derive(Clone)]
pub struct SizedBox {
    requested_width: Option<f32>,
    requested_height: Option<f32>,
    pub color: Option<Color>,
    child: Option<View>,
}

impl SizedBox {
    pub fn new(size: Size) -> Result<Self, LayoutConfigError> {
        Self::from_axes(Some(size.width), Some(size.height))
    }

    pub fn width(width: f32) -> Result<Self, LayoutConfigError> {
        Self::from_axes(Some(width), None)
    }

    pub fn height(height: f32) -> Result<Self, LayoutConfigError> {
        Self::from_axes(None, Some(height))
    }

    fn from_axes(width: Option<f32>, height: Option<f32>) -> Result<Self, LayoutConfigError> {
        if let Some(value) = width {
            validate_scalar("width", value)?;
        }
        if let Some(value) = height {
            validate_scalar("height", value)?;
        }
        Ok(Self {
            requested_width: width,
            requested_height: height,
            color: None,
            child: None,
        })
    }

    pub fn requested_width(&self) -> Option<f32> {
        self.requested_width
    }
    pub fn requested_height(&self) -> Option<f32> {
        self.requested_height
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Replaces the optional content slot.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for SizedBox {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for SizedBox {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("SizedBox", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for SizedBox {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        self.child_constraints(constraints).constrain(Size::ZERO)
    }

    fn child_constraints(&self, mut constraints: BoxConstraints) -> BoxConstraints {
        if let Some(width) = self.requested_width {
            let width = width.clamp(constraints.min.width, constraints.max.width);
            constraints.min.width = width;
            constraints.max.width = width;
        }
        if let Some(height) = self.requested_height {
            let height = height.clamp(constraints.min.height, constraints.max.height);
            constraints.min.height = height;
            constraints.max.height = height;
        }
        constraints
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = self
            .child_constraints(constraints)
            .constrain(child_sizes.first().copied().unwrap_or(Size::ZERO));
        (size, vec![Point::ZERO; child_sizes.len()])
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        match self.color {
            Some(color) => vec![Primitive::Quad {
                rect,
                color,
                corner_radius: 0.0,
            }],
            None => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Point;

    #[test]
    fn sized_box_build() {
        let sized_box = SizedBox::new(Size::new(100.0, 50.0)).unwrap();
        let mut cx = BuildCx::stub();
        let view = sized_box.build(&mut cx);
        assert_eq!(view.children.len(), 0);
    }

    #[test]
    fn intrinsic_size() {
        let sb = SizedBox::new(Size::new(100.0, 50.0)).unwrap();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = sb.intrinsic_size(constraints, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(size, Size::new(100.0, 50.0));
    }

    #[test]
    fn intrinsic_size_clamped() {
        let sb = SizedBox::new(Size::new(1000.0, 50.0)).unwrap();
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let size = sb.intrinsic_size(constraints, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(size, Size::new(500.0, 500.0));
    }

    #[test]
    fn paint_primitives_with_color() {
        let sb = SizedBox::new(Size::new(100.0, 50.0))
            .unwrap()
            .color(Color::RED);
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(100.0, 50.0));
        let prims = sb.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::Quad {
                rect: r,
                color: c,
                corner_radius,
            } => {
                assert_eq!(*r, rect);
                assert_eq!(*c, Color::RED);
                assert_eq!(*corner_radius, 0.0);
            }
            _ => panic!("expected Quad primitive"),
        }
    }

    #[test]
    fn paint_primitives_without_color() {
        let sb = SizedBox::new(Size::new(100.0, 50.0)).unwrap();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        let prims = sb.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert!(prims.is_empty());
    }

    #[test]
    fn layout_children() {
        let sb = SizedBox::new(Size::new(100.0, 50.0)).unwrap();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (size, positions) =
            sb.layout_children(constraints, &[], &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(size, Size::new(100.0, 50.0));
        assert!(positions.is_empty());
    }
}
