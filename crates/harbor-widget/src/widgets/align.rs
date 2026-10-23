use crate::layout::{Alignment, BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Single-child positioner within parent bounds according to Alignment.
#[derive(Clone)]
pub struct Align {
    pub alignment: Alignment,
    pub background: Option<Color>,
    child: Option<View>,
    vertical_alignment: Option<Alignment>,
    insets: [f32; 4],
}

impl Align {
    pub fn new(alignment: Alignment) -> Self {
        Align {
            alignment,
            background: None,
            vertical_alignment: None,
            insets: [0.0; 4],
            child: None,
        }
    }

    /// Overrides vertical placement while preserving the constructor's horizontal alignment.
    pub fn vertical_alignment(mut self, alignment: Alignment) -> Self {
        self.vertical_alignment = Some(alignment);
        self
    }

    /// Insets the available alignment bounds (top, right, bottom, left).
    pub fn insets(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.insets = [top, right, bottom, left];
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Replaces the child slot.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for Align {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for Align {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("Align", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for Align {
    // Alignment space is not an input surface (notably inside a Portal).
    fn hit_test(&self, _: Point, _: Rect) -> bool {
        false
    }
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        // Fill bounded axes; an unbounded axis has no space to fill.
        constraints.fill_bounded(Size::ZERO)
    }

    fn child_constraints(&self, constraints: BoxConstraints) -> BoxConstraints {
        let [top, right, bottom, left] = self.insets;
        constraints.deflate(Size::new(left + right, top + bottom))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let [top, right, bottom, left] = self.insets;
        let natural = child_sizes.first().copied().unwrap_or(Size::ZERO);
        let own = constraints.fill_bounded(Size::new(
            natural.width + left + right,
            natural.height + top + bottom,
        ));
        if child_sizes.is_empty() {
            return (own, vec![]);
        }
        let child_size = child_sizes[0];
        let x = left
            + self
                .alignment
                .position(child_size.width, (own.width - left - right).max(0.0));
        let y = top
            + self
                .vertical_alignment
                .unwrap_or(self.alignment)
                .position(child_size.height, (own.height - top - bottom).max(0.0));
        (own, vec![Point::new(x, y)])
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
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
    fn align_top_right_overrides_only_vertical_placement() {
        let align = Align::new(Alignment::End).vertical_alignment(Alignment::Start);
        let (size, positions) = align.layout_children(
            BoxConstraints::tight(Size::new(200.0, 100.0)),
            &[Size::new(50.0, 20.0)],
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(size, Size::new(200.0, 100.0));
        assert_eq!(positions, vec![Point::new(150.0, 0.0)]);
        assert!(!align.hit_test(
            Point::new(10.0, 10.0),
            Rect::from_min_size(Point::ZERO, size)
        ));
    }

    #[test]
    fn align_insets_reserve_space_without_creating_an_input_surface() {
        let align = Align::new(Alignment::End)
            .vertical_alignment(Alignment::Start)
            .insets(10.0, 20.0, 30.0, 40.0);
        let constraints = BoxConstraints::loose(Size::new(200.0, 100.0));
        assert_eq!(
            align.child_constraints(constraints),
            BoxConstraints::loose(Size::new(140.0, 60.0))
        );
        let (size, positions) = align.layout_children(
            constraints,
            &[Size::new(50.0, 20.0)],
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(size, Size::new(200.0, 100.0));
        assert_eq!(positions, vec![Point::new(130.0, 10.0)]);
        assert!(!align.hit_test(
            Point::new(190.0, 5.0),
            Rect::from_min_size(Point::ZERO, size)
        ));
    }
    #[test]
    fn should_use_natural_extent_on_unbounded_axes() {
        let align = Align::new(Alignment::Center);
        let metrics = &crate::runtime::DEFAULT_TEXT_METRICS;
        let constraints = BoxConstraints {
            min: Size::new(10.0, 5.0),
            max: Size::new(f32::INFINITY, 80.0),
        };
        assert_eq!(
            align.intrinsic_size(constraints, metrics),
            Size::new(10.0, 80.0)
        );
        let (size, positions) =
            align.layout_children(constraints, &[Size::new(30.0, 20.0)], metrics);
        assert_eq!(size, Size::new(30.0, 80.0));
        assert_eq!(positions, vec![Point::new(0.0, 30.0)]);
        assert_eq!(
            align
                .layout_children(
                    BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)),
                    &[],
                    metrics
                )
                .0,
            Size::ZERO,
        );
    }

    #[test]
    fn align_center() {
        let align = Align::new(Alignment::Center).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (own, positions) = align.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(own, Size::new(200.0, 200.0));
        assert_eq!(positions[0], Point::new(75.0, 75.0));
    }

    #[test]
    fn align_start() {
        let align = Align::new(Alignment::Start).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (_own, positions) = align.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(positions[0], Point::ZERO);
    }

    #[test]
    fn align_end() {
        let align = Align::new(Alignment::End).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (_own, positions) = align.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(positions[0], Point::new(150.0, 150.0));
    }

    #[test]
    fn align_empty() {
        let align = Align::new(Alignment::Center);
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let (own, positions) =
            align.layout_children(constraints, &[], &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(own, Size::new(200.0, 200.0));
        assert!(positions.is_empty());
    }

    #[test]
    fn align_child_larger_than_parent() {
        let align = Align::new(Alignment::Center).child(SizedBox::new(Size::new(300.0, 300.0)));
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let child_sizes = vec![Size::new(300.0, 300.0)];
        let (own, positions) = align.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(own, Size::new(200.0, 200.0));
        // Child larger than own: center offset clamped to 0
        assert_eq!(positions[0], Point::ZERO);
    }

    #[test]
    fn align_background_paint() {
        let align = Align::new(Alignment::Center).background(Color::BLUE);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        let prims = align.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
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
        let prims = align.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert!(prims.is_empty());
    }

    #[test]
    fn align_with_loose_constraints_fills_max() {
        let align = Align::new(Alignment::Start).child(SizedBox::new(Size::new(50.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(50.0, 50.0)];
        let (own, positions) = align.layout_children(
            constraints,
            &child_sizes,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        // With loose constraints, Align fills max
        assert_eq!(own, Size::new(800.0, 600.0));
        assert_eq!(positions[0], Point::ZERO);
    }
}
