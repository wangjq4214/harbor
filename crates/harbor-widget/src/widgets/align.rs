use crate::layout::{Alignment, BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// Single-child positioner within parent bounds according to Alignment.
#[derive(Clone)]
pub struct Align {
    pub alignment: Alignment,
    pub background: Option<Color>,
    children: Vec<View>,
}

impl Align {
    pub fn new(alignment: Alignment) -> Self {
        Align {
            alignment,
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

impl Component for Align {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for Align {
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
        // Align fills available space
        constraints.max
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        let own = Size::new(
            constraints.max.width.max(constraints.min.width),
            constraints.max.height.max(constraints.min.height),
        );
        let own = constraints.constrain(own);
        if child_sizes.is_empty() {
            return (own, vec![]);
        }
        let child_size = child_sizes[0];
        let x = self.alignment.position(child_size.width, own.width);
        let y = self.alignment.position(child_size.height, own.height);
        (own, vec![Point::new(x, y)])
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
    fn align_center() {
        let align = Align::new(Alignment::Center).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (own, positions) = align.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(200.0, 200.0));
        assert_eq!(positions[0], Point::new(75.0, 75.0));
    }

    #[test]
    fn align_start() {
        let align = Align::new(Alignment::Start).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (_own, positions) = align.layout_children(constraints, &child_sizes);
        assert_eq!(positions[0], Point::ZERO);
    }

    #[test]
    fn align_end() {
        let align = Align::new(Alignment::End).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (_own, positions) = align.layout_children(constraints, &child_sizes);
        assert_eq!(positions[0], Point::new(150.0, 150.0));
    }

    #[test]
    fn align_empty() {
        let align = Align::new(Alignment::Center);
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let (own, positions) = align.layout_children(constraints, &[]);
        assert_eq!(own, Size::new(200.0, 200.0));
        assert!(positions.is_empty());
    }

    #[test]
    fn align_child_larger_than_parent() {
        let align = Align::new(Alignment::Center).child(SizedBox::new(Size::new(300.0, 300.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(300.0, 300.0)];
        let (own, positions) = align.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(200.0, 200.0));
        // Child larger than own: center offset clamped to 0
        assert_eq!(positions[0], Point::ZERO);
    }

    #[test]
    fn align_background_paint() {
        let align = Align::new(Alignment::Center).background(Color::BLUE);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        let prims = align.paint_primitives(rect);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::Quad { color, .. } => assert_eq!(*color, Color::BLUE),
            _ => panic!("expected Quad"),
        }
    }

    #[test]
    fn align_no_background_paint() {
        let align = Align::new(Alignment::Center);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        let prims = align.paint_primitives(rect);
        assert!(prims.is_empty());
    }

    #[test]
    fn align_with_loose_constraints_fills_max() {
        let align = Align::new(Alignment::Start).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (own, positions) = align.layout_children(constraints, &child_sizes);
        // With loose constraints, Align fills max
        assert_eq!(own, Size::new(800.0, 600.0));
        assert_eq!(positions[0], Point::ZERO);
    }
}
