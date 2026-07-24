use crate::layout::{Alignment, BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// Horizontal flex container. Stacks children left-to-right.
#[derive(Clone)]
pub struct Row {
    pub cross_axis_alignment: Alignment,
    pub background: Option<Color>,
    children: Vec<View>,
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Row {
    pub fn new() -> Self {
        Row {
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

impl Component for Row {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for Row {
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
        // Row fills available height; width is sum of children but unknown
        // at intrinsic-time (children are laid out bottom-up by layout_fiber).
        constraints.constrain(Size::new(constraints.max.width, constraints.max.height))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        let total_width: f32 = child_sizes.iter().map(|s| s.width).sum();
        let max_height: f32 = child_sizes.iter().map(|s| s.height).fold(0.0, f32::max);
        let own = constraints.constrain(Size::new(total_width, max_height));
        let mut x = 0.0;
        let positions: Vec<Point> = child_sizes
            .iter()
            .map(|s| {
                let y = self.cross_axis_alignment.position(s.height, own.height);
                let pos = Point::new(x, y);
                x += s.width;
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
    fn row_three_children() {
        let row = Row::new()
            .child(SizedBox::new(Size::new(50.0, 50.0)))
            .child(SizedBox::new(Size::new(50.0, 50.0)))
            .child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![
            Size::new(50.0, 50.0),
            Size::new(50.0, 50.0),
            Size::new(50.0, 50.0),
        ];
        let (own, positions) = row.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(150.0, 50.0));
        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], Point::new(0.0, 0.0));
        assert_eq!(positions[1], Point::new(50.0, 0.0));
        assert_eq!(positions[2], Point::new(100.0, 0.0));
    }

    #[test]
    fn row_different_child_sizes() {
        let row = Row::new()
            .child(SizedBox::new(Size::new(100.0, 50.0)))
            .child(SizedBox::new(Size::new(200.0, 75.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(100.0, 50.0), Size::new(200.0, 75.0)];
        let (own, _positions) = row.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(300.0, 75.0));
    }

    #[test]
    fn row_cross_axis_center() {
        let row = Row::new()
            .cross_axis_alignment(Alignment::Center)
            .child(SizedBox::new(Size::new(50.0, 20.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(50.0, 20.0)];
        let (own, positions) = row.layout_children(constraints, &child_sizes);
        assert_eq!(own.height, 20.0);
        // Child centered in cross axis (height is own.height which is 20 = child)
        assert_eq!(positions[0].y, 0.0);
    }

    #[test]
    fn row_cross_axis_center_with_larger_own() {
        let row = Row::new()
            .cross_axis_alignment(Alignment::Center)
            .child(SizedBox::new(Size::new(50.0, 20.0)))
            .child(SizedBox::new(Size::new(50.0, 100.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(50.0, 20.0), Size::new(50.0, 100.0)];
        let (own, positions) = row.layout_children(constraints, &child_sizes);
        assert_eq!(own.height, 100.0);
        // First child (20 tall) centered in 100 height → y = 40
        assert_eq!(positions[0].y, 40.0);
        // Second child (100 tall) centered in 100 height → y = 0
        assert_eq!(positions[1].y, 0.0);
    }

    #[test]
    fn row_empty() {
        let row = Row::new();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (own, positions) = row.layout_children(constraints, &[]);
        assert_eq!(own, Size::ZERO);
        assert!(positions.is_empty());
    }

    #[test]
    fn row_child_wider_than_constraints() {
        let row = Row::new().child(SizedBox::new(Size::new(1000.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let child_sizes = vec![Size::new(1000.0, 50.0)];
        let (own, _positions) = row.layout_children(constraints, &child_sizes);
        // Both dimensions clamped to tight 500 (width 1000 -> 500, height 50 -> 500)
        assert_eq!(own, Size::new(500.0, 500.0));
    }

    #[test]
    fn row_all_children_wider_than_max() {
        let row = Row::new()
            .child(SizedBox::new(Size::new(400.0, 50.0)))
            .child(SizedBox::new(Size::new(400.0, 50.0)));
        // Tight constraint smaller than total
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let child_sizes = vec![Size::new(400.0, 50.0), Size::new(400.0, 50.0)];
        let (own, _positions) = row.layout_children(constraints, &child_sizes);
        // Total width 800 clamped to 500; height 50 clamped to tight min 500 -> 500
        assert_eq!(own, Size::new(500.0, 500.0));
    }

    #[test]
    fn row_cross_axis_end() {
        let row = Row::new()
            .cross_axis_alignment(Alignment::End)
            .child(SizedBox::new(Size::new(50.0, 20.0)))
            .child(SizedBox::new(Size::new(50.0, 100.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(50.0, 20.0), Size::new(50.0, 100.0)];
        let (own, positions) = row.layout_children(constraints, &child_sizes);
        assert_eq!(own.height, 100.0);
        // First child (20 tall) at end of 100 height → y = 100 - 20 = 80
        assert_eq!(positions[0].y, 80.0);
        // Second child (100 tall) at end of 100 height → y = 0
        assert_eq!(positions[1].y, 0.0);
    }

    #[test]
    fn row_cross_axis_stretch() {
        let row = Row::new()
            .cross_axis_alignment(Alignment::Stretch)
            .child(SizedBox::new(Size::new(50.0, 20.0)))
            .child(SizedBox::new(Size::new(50.0, 100.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(50.0, 20.0), Size::new(50.0, 100.0)];
        let (own, positions) = row.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(200.0, 200.0)); // clamped to tight
        // Stretch aligns at y=0
        assert_eq!(positions[0].y, 0.0);
        assert_eq!(positions[1].y, 0.0);
    }

    #[test]
    fn row_intrinsic_size() {
        let row = Row::new().child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = row.intrinsic_size(constraints);
        // Row fills available max in loose mode
        assert_eq!(size, Size::new(800.0, 600.0));
    }

    #[test]
    fn row_background_paint() {
        let row = Row::new().background(Color::BLUE);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 100.0));
        let prims = row.paint_primitives(rect);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::Quad { color, .. } => assert_eq!(*color, Color::BLUE),
            _ => panic!("expected Quad"),
        }
    }

    #[test]
    fn row_no_background_paint() {
        let row = Row::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 100.0));
        let prims = row.paint_primitives(rect);
        assert!(prims.is_empty());
    }
}
