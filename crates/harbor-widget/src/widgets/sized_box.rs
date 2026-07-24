use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// A fixed-size widget with optional background color.
///
/// Merged from Phase 0's separate SizedBox + SizedBoxState into a single
/// struct implementing both Component and AnyView.
#[derive(Clone)]
pub struct SizedBox {
    pub size: Size,
    pub color: Option<Color>,
    children: Vec<View>,
}

impl SizedBox {
    pub fn new(size: Size) -> Self {
        SizedBox {
            size,
            color: None,
            children: vec![],
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Component for SizedBox {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for SizedBox {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn build(self: Box<Self>, _cx: &mut BuildCx) -> View {
        View::new(*self, vec![], None)
    }

    fn intrinsic_size(&self, constraints: BoxConstraints) -> Size {
        constraints.constrain(self.size)
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        (
            self.intrinsic_size(constraints),
            vec![Point::ZERO; child_sizes.len()],
        )
    }

    fn paint_primitives(&self, rect: Rect) -> Vec<Primitive> {
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

    #[test]
    fn sized_box_build() {
        let sized_box = SizedBox::new(Size::new(100.0, 50.0));
        let mut cx = BuildCx::stub();
        let view = sized_box.build(&mut cx);
        assert_eq!(view.children.len(), 0);
    }

    #[test]
    fn intrinsic_size() {
        let sb = SizedBox::new(Size::new(100.0, 50.0));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = sb.intrinsic_size(constraints);
        assert_eq!(size, Size::new(100.0, 50.0));
    }

    #[test]
    fn intrinsic_size_clamped() {
        let sb = SizedBox::new(Size::new(1000.0, 50.0));
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let size = sb.intrinsic_size(constraints);
        assert_eq!(size, Size::new(500.0, 500.0));
    }

    #[test]
    fn paint_primitives_with_color() {
        let sb = SizedBox::new(Size::new(100.0, 50.0)).color(Color::RED);
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(100.0, 50.0));
        let prims = sb.paint_primitives(rect);
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
        let sb = SizedBox::new(Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        let prims = sb.paint_primitives(rect);
        assert!(prims.is_empty());
    }

    #[test]
    fn layout_children() {
        let sb = SizedBox::new(Size::new(100.0, 50.0));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (size, positions) = sb.layout_children(constraints, &[]);
        assert_eq!(size, Size::new(100.0, 50.0));
        assert!(positions.is_empty());
    }
}
