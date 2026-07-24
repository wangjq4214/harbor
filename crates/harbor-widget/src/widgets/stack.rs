use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// Overlay container. Positions all children at the same origin (0,0).
#[derive(Clone)]
pub struct Stack {
    pub background: Option<Color>,
    children: Vec<View>,
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl Stack {
    pub fn new() -> Self {
        Stack {
            background: None,
            children: vec![],
        }
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn child(mut self, child: impl Component + 'static) -> Self {
        let mut cx = BuildCx::stub();
        self.children.push(child.build(&mut cx));
        self
    }
}

impl Component for Stack {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for Stack {
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
        constraints.constrain(Size::ZERO)
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        let max_width = child_sizes.iter().map(|s| s.width).fold(0.0, f32::max);
        let max_height = child_sizes.iter().map(|s| s.height).fold(0.0, f32::max);
        let own = constraints.constrain(Size::new(max_width, max_height));
        let positions = vec![Point::ZERO; child_sizes.len()];
        (own, positions)
    }

    fn paint_primitives(&self, rect: Rect) -> Vec<Primitive> {
        self.background
            .map(|c| Primitive::Quad {
                rect,
                color: c,
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
    fn stack_two_children() {
        let stack = Stack::new()
            .child(SizedBox::new(Size::new(100.0, 50.0)))
            .child(SizedBox::new(Size::new(80.0, 40.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(100.0, 50.0), Size::new(80.0, 40.0)];
        let (own, positions) = stack.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(100.0, 50.0));
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], Point::ZERO);
        assert_eq!(positions[1], Point::ZERO);
    }

    #[test]
    fn stack_empty() {
        let stack = Stack::new();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (own, positions) = stack.layout_children(constraints, &[]);
        assert_eq!(own, Size::ZERO);
        assert!(positions.is_empty());
    }

    #[test]
    fn stack_background_paint() {
        let stack = Stack::new().background(Color::GREEN);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 100.0));
        let prims = stack.paint_primitives(rect);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::Quad { color, .. } => assert_eq!(*color, Color::GREEN),
            _ => panic!("expected Quad"),
        }
    }

    #[test]
    fn stack_no_background_paint() {
        let stack = Stack::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 100.0));
        let prims = stack.paint_primitives(rect);
        assert!(prims.is_empty());
    }

    #[test]
    fn stack_child_larger_than_constraints() {
        let stack = Stack::new().child(SizedBox::new(Size::new(1000.0, 1000.0)));
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let child_sizes = vec![Size::new(1000.0, 1000.0)];
        let (own, positions) = stack.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(500.0, 500.0));
        assert_eq!(positions[0], Point::ZERO);
    }

    #[test]
    fn stack_three_children_all_at_origin() {
        let stack = Stack::new()
            .child(SizedBox::new(Size::new(100.0, 100.0)))
            .child(SizedBox::new(Size::new(80.0, 80.0)))
            .child(SizedBox::new(Size::new(60.0, 60.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![
            Size::new(100.0, 100.0),
            Size::new(80.0, 80.0),
            Size::new(60.0, 60.0),
        ];
        let (own, positions) = stack.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(100.0, 100.0)); // max child size
        assert_eq!(positions.len(), 3);
        // All at origin
        for pos in &positions {
            assert_eq!(*pos, Point::ZERO);
        }
    }
}
