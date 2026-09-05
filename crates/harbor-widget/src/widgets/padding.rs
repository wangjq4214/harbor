use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// Insets a single child by padding and optionally draws a background.
#[derive(Clone)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    pub background: Option<Color>,
    child: Option<View>,
}

impl Padding {
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Padding {
            top,
            right,
            bottom,
            left,
            background: None,
            child: None,
        }
    }

    pub fn all(value: f32) -> Self {
        Self::new(value, value, value, value)
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Sets this padding's only child, replacing any previously staged child.
    pub fn child(mut self, child: impl Component + 'static) -> Self {
        self.child = Some(View::deferred(child));
        self
    }
}

impl Component for Padding {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl AnyView for Padding {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        // layout_fiber measures descendants before calling layout_children.
        // This intrinsic fallback therefore represents the child-free case.
        constraints.constrain(Size::new(self.left + self.right, self.top + self.bottom))
    }

    fn child_constraints(&self, constraints: BoxConstraints) -> BoxConstraints {
        constraints.deflate(Size::new(self.left + self.right, self.top + self.bottom))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let child_size = child_sizes.first().copied().unwrap_or(Size::ZERO);
        let own = constraints.constrain(Size::new(
            child_size.width + self.left + self.right,
            child_size.height + self.top + self.bottom,
        ));
        let positions = if child_sizes.is_empty() {
            vec![]
        } else {
            vec![Point::new(self.left, self.top)]
        };
        (own, positions)
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        self.background
            .map(|color| Primitive::Quad {
                rect,
                color,
                corner_radius: 0.0,
            })
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::sized_box::SizedBox;

    #[test]
    fn padding_around_sized_box() {
        let padding =
            Padding::new(10.0, 10.0, 10.0, 10.0).child(SizedBox::new(Size::new(100.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(100.0, 50.0)];
        let (own, positions) = padding.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(own, Size::new(120.0, 70.0));
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], Point::new(10.0, 10.0));
    }

    #[test]
    fn should_set_uniform_insets_when_constructed_with_all() {
        // Arrange
        let inset = 12.0;

        // Act
        let padding = Padding::all(inset);

        // Assert
        assert_eq!(padding.top, inset);
        assert_eq!(padding.right, inset);
        assert_eq!(padding.bottom, inset);
        assert_eq!(padding.left, inset);
    }

    #[test]
    fn should_emit_only_one_child_when_child_is_staged_multiple_times() {
        // Arrange
        let padding = Padding::all(4.0)
            .child(SizedBox::new(Size::new(10.0, 10.0)))
            .child(SizedBox::new(Size::new(20.0, 20.0)));
        let mut cx = BuildCx::stub();

        // Act
        let view = padding.build(&mut cx);

        // Assert
        assert_eq!(view.children.len(), 1);
    }

    #[test]
    fn should_emit_no_children_when_child_is_not_staged() {
        // Arrange
        let padding = Padding::all(4.0);
        let mut cx = BuildCx::stub();

        // Act
        let view = padding.build(&mut cx);

        // Assert
        assert!(view.children.is_empty());
    }

    #[test]
    fn should_deflate_child_constraints_when_padding_is_uniform() {
        // Arrange
        let padding = Padding::all(16.0);
        let constraints = BoxConstraints::tight(Size::new(100.0, 80.0));

        // Act
        let child_constraints = padding.child_constraints(constraints);

        // Assert
        assert_eq!(
            child_constraints,
            BoxConstraints::tight(Size::new(68.0, 48.0))
        );
    }

    #[test]
    fn should_deflate_child_constraints_when_padding_is_asymmetric() {
        // Arrange
        let padding = Padding::new(5.0, 10.0, 15.0, 20.0);
        let constraints = BoxConstraints::loose(Size::new(100.0, 80.0));

        // Act
        let child_constraints = padding.child_constraints(constraints);

        // Assert
        assert_eq!(
            child_constraints,
            BoxConstraints::loose(Size::new(70.0, 60.0))
        );
    }

    #[test]
    fn should_saturate_child_constraints_when_padding_exceeds_parent_bounds() {
        // Arrange
        let padding = Padding::new(8.0, 40.0, 8.0, 40.0);
        let constraints = BoxConstraints::loose(Size::new(60.0, 30.0));

        // Act
        let child_constraints = padding.child_constraints(constraints);

        // Assert
        assert_eq!(
            child_constraints,
            BoxConstraints::loose(Size::new(0.0, 14.0))
        );
    }

    #[test]
    fn padding_with_tight_constraints_clamps_child() {
        let padding =
            Padding::new(10.0, 10.0, 10.0, 10.0).child(SizedBox::new(Size::new(100.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(50.0, 50.0));
        let child_sizes = vec![Size::new(100.0, 50.0)];
        let (own, _positions) = padding.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        // Clamped to tight 50x50
        assert_eq!(own, Size::new(50.0, 50.0));
    }

    #[test]
    fn padding_paint_primitives_with_background() {
        let padding = Padding::new(5.0, 5.0, 5.0, 5.0).background(Color::RED);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        let prims = padding.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(prims.len(), 1);
    }

    #[test]
    fn padding_paint_primitives_without_background() {
        let padding = Padding::new(5.0, 5.0, 5.0, 5.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        let prims = padding.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert!(prims.is_empty());
    }

    #[test]
    fn asymmetric_padding() {
        let padding = Padding::new(5.0, 10.0, 15.0, 20.0);
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(100.0, 50.0)];
        let (own, positions) = padding.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(own, Size::new(130.0, 70.0)); // 100+20+10=130, 50+5+15=70
        assert_eq!(positions[0], Point::new(20.0, 5.0));
    }

    #[test]
    fn padding_zero_is_identity() {
        let padding = Padding::new(0.0, 0.0, 0.0, 0.0).child(SizedBox::new(Size::new(100.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(100.0, 50.0)];
        let (own, positions) = padding.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        // Own size equals child size with zero padding
        assert_eq!(own, Size::new(100.0, 50.0));
        assert_eq!(positions[0], Point::ZERO);
    }

    #[test]
    fn padding_no_child() {
        let padding = Padding::new(10.0, 10.0, 10.0, 10.0);
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (own, positions) =
            padding.layout_children(constraints, &[], &crate::runtime::DEFAULT_TEXT_METRICS);
        // Own size is padding only (0 child + 10 + 10)
        assert_eq!(own.width, 20.0);
        assert_eq!(own.height, 20.0);
        // No children → no positions
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn padding_child_larger_than_constraints() {
        let padding =
            Padding::new(10.0, 10.0, 10.0, 10.0).child(SizedBox::new(Size::new(500.0, 500.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(500.0, 500.0)];
        let (own, _positions) = padding.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        // Clamped to tight 200x200
        assert_eq!(own, Size::new(200.0, 200.0));
    }
}
