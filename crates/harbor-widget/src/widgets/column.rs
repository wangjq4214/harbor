use crate::layout::{Alignment, BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// Vertical flex container. Stacks children top-to-bottom.
#[derive(Clone)]
pub struct Column {
    pub cross_axis_alignment: Alignment,
    pub background: Option<Color>,
    children: Vec<View>,
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Column {
    pub fn new() -> Self {
        Column {
            cross_axis_alignment: Alignment::Start,
            background: None,
            children: vec![],
        }
    }

    pub fn cross_axis_alignment(mut self, alignment: Alignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
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

impl Component for Column {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for Column {
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
        let total_height: f32 = 0.0;
        let max_width: f32 = 0.0;
        constraints.constrain(Size::new(max_width, total_height))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        let max_width: f32 = child_sizes.iter().map(|s| s.width).fold(0.0, f32::max);
        let total_height: f32 = child_sizes.iter().map(|s| s.height).sum();
        let own = constraints.constrain(Size::new(max_width, total_height));
        let mut y = 0.0;
        let positions: Vec<Point> = child_sizes
            .iter()
            .map(|s| {
                let x = self.cross_axis_alignment.position(s.width, own.width);
                let pos = Point::new(x, y);
                y += s.height;
                pos
            })
            .collect();
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
    fn column_three_children() {
        let column = Column::new()
            .child(SizedBox::new(Size::new(50.0, 50.0)))
            .child(SizedBox::new(Size::new(50.0, 50.0)))
            .child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![
            Size::new(50.0, 50.0),
            Size::new(50.0, 50.0),
            Size::new(50.0, 50.0),
        ];
        let (own, positions) = column.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(50.0, 150.0));
        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], Point::new(0.0, 0.0));
        assert_eq!(positions[1], Point::new(0.0, 50.0));
        assert_eq!(positions[2], Point::new(0.0, 100.0));
    }

    #[test]
    fn column_different_child_sizes() {
        let column = Column::new()
            .child(SizedBox::new(Size::new(50.0, 100.0)))
            .child(SizedBox::new(Size::new(75.0, 200.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(50.0, 100.0), Size::new(75.0, 200.0)];
        let (own, _positions) = column.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(75.0, 300.0));
    }

    #[test]
    fn column_cross_axis_center() {
        let column = Column::new()
            .cross_axis_alignment(Alignment::Center)
            .child(SizedBox::new(Size::new(20.0, 50.0)))
            .child(SizedBox::new(Size::new(100.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(20.0, 50.0), Size::new(100.0, 50.0)];
        let (own, positions) = column.layout_children(constraints, &child_sizes);
        assert_eq!(own.width, 100.0);
        // First child (20 wide) centered in 100 width → x = 40
        assert_eq!(positions[0].x, 40.0);
        // Second child (100 wide) centered in 100 width → x = 0
        assert_eq!(positions[1].x, 0.0);
    }

    #[test]
    fn column_empty() {
        let column = Column::new();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (own, positions) = column.layout_children(constraints, &[]);
        assert_eq!(own, Size::ZERO);
        assert!(positions.is_empty());
    }

    #[test]
    fn column_child_taller_than_constraints() {
        let column = Column::new().child(SizedBox::new(Size::new(50.0, 1000.0)));
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let child_sizes = vec![Size::new(50.0, 1000.0)];
        let (own, _positions) = column.layout_children(constraints, &child_sizes);
        // Both dimensions clamped to tight 500 (width 50 -> 500, height 1000 -> 500)
        assert_eq!(own, Size::new(500.0, 500.0));
    }

    #[test]
    fn column_cross_axis_end() {
        let column = Column::new()
            .cross_axis_alignment(Alignment::End)
            .child(SizedBox::new(Size::new(20.0, 50.0)))
            .child(SizedBox::new(Size::new(100.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(20.0, 50.0), Size::new(100.0, 50.0)];
        let (own, positions) = column.layout_children(constraints, &child_sizes);
        assert_eq!(own.width, 100.0);
        // First child (20 wide) at end of 100 → x = 80
        assert_eq!(positions[0].x, 80.0);
        // Second child (100 wide) at end of 100 → x = 0
        assert_eq!(positions[1].x, 0.0);
    }

    #[test]
    fn column_background_paint() {
        let column = Column::new().background(Color::BLUE);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 300.0));
        let prims = column.paint_primitives(rect);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::Quad { color, .. } => assert_eq!(*color, Color::BLUE),
            _ => panic!("expected Quad"),
        }
    }

    #[test]
    fn column_no_background_paint() {
        let column = Column::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 300.0));
        let prims = column.paint_primitives(rect);
        assert!(prims.is_empty());
    }

    #[test]
    fn column_all_children_taller_than_max() {
        let column = Column::new()
            .child(SizedBox::new(Size::new(50.0, 400.0)))
            .child(SizedBox::new(Size::new(50.0, 400.0)));
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let child_sizes = vec![Size::new(50.0, 400.0), Size::new(50.0, 400.0)];
        let (own, _positions) = column.layout_children(constraints, &child_sizes);
        // Total height = 800, clamped to 500
        assert_eq!(own.height, 500.0);
    }
}
