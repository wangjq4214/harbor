use crate::layout::{
    Alignment2D, BoxConstraints, ChildMeasurer, Insets, LayoutConfigError, LayoutError,
    ParentLayout, Point, Rect, Size,
};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Single-child positioner within parent bounds according to Alignment.
#[derive(Clone)]
pub struct Align {
    pub alignment: Alignment2D,
    background: Option<Color>,
    child: Option<View>,
    insets: Insets,
}

impl Align {
    pub fn new(alignment: Alignment2D) -> Self {
        Align {
            alignment,
            background: None,
            insets: Insets::all(0.0),
            child: None,
        }
    }

    /// Insets the available alignment bounds. Each side is validated independently.
    pub fn insets(mut self, insets: Insets) -> Result<Self, LayoutConfigError> {
        insets.validate()?;
        self.insets = insets;
        Ok(self)
    }

    pub fn get_insets(&self) -> Insets {
        self.insets
    }

    pub fn background(mut self, color: Color) -> Result<Self, crate::theme::ColorConfigError> {
        crate::theme::validate_color("Align.background", color)?;
        self.background = Some(color);
        Ok(self)
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
    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        metrics: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        constraints.validate()?;
        self.insets.checked_inflate(Size::ZERO)?;
        let child_constraints = self.child_constraints(constraints);
        let child_sizes = (0..children.len())
            .map(|index| children.measure(index, child_constraints))
            .collect::<Result<Vec<_>, _>>()?;
        self.insets
            .checked_inflate(child_sizes.first().copied().unwrap_or(Size::ZERO))?;
        let (size, origins) = self.layout_children(constraints, &child_sizes, metrics);
        Ok(ParentLayout {
            size,
            placements: origins.into_iter().enumerate().collect(),
            diagnostics: vec![],
        })
    }

    // Alignment space is not an input surface (notably inside a Portal).
    fn hit_test(&self, _: Point, _: Rect) -> bool {
        false
    }
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        // Fill bounded axes; an unbounded axis has no space to fill.
        constraints.fill_bounded(Size::ZERO)
    }

    fn child_constraints(&self, constraints: BoxConstraints) -> BoxConstraints {
        let Insets {
            top,
            right,
            bottom,
            left,
        } = self.insets;
        constraints.deflate(Size::new(left + right, top + bottom))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let Insets {
            top,
            right,
            bottom,
            left,
        } = self.insets;
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
                .horizontal
                .position(child_size.width, (own.width - left - right).max(0.0));
        let y = top
            + self
                .alignment
                .vertical
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
        let align = Align::new(crate::Alignment2D::TOP_RIGHT);
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
        let align = Align::new(crate::Alignment2D::TOP_RIGHT)
            .insets(crate::Insets {
                top: 10.0,
                right: 20.0,
                bottom: 30.0,
                left: 40.0,
            })
            .unwrap();
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
        let align = Align::new(crate::Alignment2D::CENTER);
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
        let align = Align::new(crate::Alignment2D::CENTER)
            .child(SizedBox::new(Size::new(50.0, 50.0)).unwrap());
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
        let align = Align::new(crate::Alignment2D::TOP_LEFT)
            .child(SizedBox::new(Size::new(50.0, 50.0)).unwrap());
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
        let align = Align::new(crate::Alignment2D::BOTTOM_RIGHT)
            .child(SizedBox::new(Size::new(50.0, 50.0)).unwrap());
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
        let align = Align::new(crate::Alignment2D::CENTER);
        let constraints = BoxConstraints::tight(Size::new(200.0, 200.0));
        let (own, positions) =
            align.layout_children(constraints, &[], &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(own, Size::new(200.0, 200.0));
        assert!(positions.is_empty());
    }

    #[test]
    fn align_child_larger_than_parent() {
        let align = Align::new(crate::Alignment2D::CENTER)
            .child(SizedBox::new(Size::new(300.0, 300.0)).unwrap());
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
        let align = Align::new(crate::Alignment2D::CENTER)
            .background(Color::BLUE)
            .expect("finite background");
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
        let align = Align::new(crate::Alignment2D::CENTER);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        let prims = align.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert!(prims.is_empty());
    }

    #[test]
    fn align_with_loose_constraints_fills_max() {
        let align = Align::new(crate::Alignment2D::TOP_LEFT)
            .child(SizedBox::new(Size::new(50.0, 50.0)).unwrap());
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
