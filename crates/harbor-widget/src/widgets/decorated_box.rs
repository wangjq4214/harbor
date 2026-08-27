use crate::decoration::{BorderRadius, BoxDecoration, ClipBehavior};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::Primitive;
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key, PaintPhase, View};

/// A layout-neutral single-child wrapper that paints a box decoration.
#[derive(Clone)]
pub struct DecoratedBox {
    decoration: BoxDecoration,
    clip_behavior: ClipBehavior,
    child: Option<View>,
}

impl DecoratedBox {
    pub fn new(decoration: BoxDecoration) -> Self {
        Self {
            decoration,
            clip_behavior: ClipBehavior::None,
            child: None,
        }
    }

    /// Replaces the staged child. A DecoratedBox always owns at most one child.
    pub fn child(mut self, child: impl Component + 'static) -> Self {
        self.child = Some(View::deferred(child));
        self
    }

    /// Selects the intended clipping policy for a future rounded-mask backend.
    ///
    /// This slice preserves the policy as decoration configuration but does not
    /// propagate it into SceneItems: the current external-draw API can only
    /// honor rectangular scissors, so advertising rounded clipping would be a
    /// false contract. A provider-owned mask/stencil boundary is required
    /// before this setting can affect child rendering.
    pub fn clip_behavior(mut self, clip_behavior: ClipBehavior) -> Self {
        self.clip_behavior = clip_behavior;
        self
    }

    pub fn decoration(&self) -> &BoxDecoration {
        &self.decoration
    }

    pub fn clip_behavior_value(&self) -> ClipBehavior {
        self.clip_behavior
    }
}

impl Component for DecoratedBox {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl AnyView for DecoratedBox {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(child_sizes.first().copied().unwrap_or(Size::ZERO));
        if child_sizes.is_empty() {
            (size, Vec::new())
        } else {
            (size, vec![Point::ZERO])
        }
    }

    fn paint_primitives_for_phase(
        &self,
        phase: PaintPhase,
        rect: Rect,
        _metrics: &TextMetrics,
    ) -> Vec<Primitive> {
        let Some(radii) = self
            .decoration
            .border_radius_value()
            .unwrap_or_else(BorderRadius::default)
            .normalize(rect.size())
            .ok()
            .map(|radii| radii.as_array())
        else {
            return Vec::new();
        };

        match phase {
            PaintPhase::BeforeChildren => self
                .decoration
                .color()
                .filter(|color| color.a > 0.0)
                .map(|color| Primitive::RoundedQuad {
                    rect,
                    color,
                    corner_radii: radii,
                })
                .into_iter()
                .collect(),
            PaintPhase::AfterChildren => self
                .decoration
                .border_value()
                .filter(|border| border.width() > 0.0 && border.color().a > 0.0)
                .map(|border| Primitive::RoundedBorder {
                    rect,
                    width: border.width(),
                    color: border.color(),
                    corner_radii: radii,
                })
                .into_iter()
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::primitive::Color;
    use crate::widgets::sized_box::SizedBox;

    #[test]
    fn layout_preserves_child_size_and_constraints() {
        let widget =
            DecoratedBox::new(BoxDecoration::default()).child(SizedBox::new(Size::new(20.0, 10.0)));
        let constraints = BoxConstraints::loose(Size::new(100.0, 100.0));
        let (size, positions) = widget.layout_children(
            constraints,
            &[Size::new(20.0, 10.0)],
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert_eq!(size, Size::new(20.0, 10.0));
        assert_eq!(positions, vec![Point::ZERO]);
    }

    #[test]
    fn emits_fill_before_and_border_after_with_same_radii() {
        let radius = BorderRadius::only(8.0, 4.0, 2.0, 6.0).unwrap();
        let border = crate::Border::all(Color::BLUE, 2.0).unwrap();
        let widget = DecoratedBox::new(
            BoxDecoration::new()
                .try_color(Color::RED)
                .unwrap()
                .border(border)
                .border_radius(radius),
        );
        let rect = Rect::from_min_size(Point::ZERO, Size::new(10.0, 10.0));
        let before = widget.paint_primitives_for_phase(
            PaintPhase::BeforeChildren,
            rect,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        let after = widget.paint_primitives_for_phase(
            PaintPhase::AfterChildren,
            rect,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert!(matches!(before[0], Primitive::RoundedQuad { .. }));
        assert!(matches!(after[0], Primitive::RoundedBorder { .. }));
        if let (
            Primitive::RoundedQuad {
                corner_radii: fill, ..
            },
            Primitive::RoundedBorder {
                corner_radii: outline,
                ..
            },
        ) = (&before[0], &after[0])
        {
            assert_eq!(fill, outline);
        }
    }

    #[test]
    fn malformed_rect_does_not_panic_when_painting_with_active_clip_policy() {
        let wrapper = DecoratedBox::new(BoxDecoration::new().try_color(Color::RED).unwrap())
            .clip_behavior(ClipBehavior::HardEdge);
        let rect = Rect {
            min: Point::ZERO,
            max: Point::new(f32::NAN, 10.0),
        };

        let primitives = wrapper.paint_primitives_for_phase(
            PaintPhase::BeforeChildren,
            rect,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        assert!(primitives.is_empty());
    }

    #[test]
    fn omits_transparent_effects_and_supports_empty_child() {
        let widget = DecoratedBox::new(BoxDecoration::new().try_color(Color::TRANSPARENT).unwrap())
            .child(SizedBox::new(Size::ZERO));
        let constraints = BoxConstraints::loose(Size::new(100.0, 100.0));
        let (size, positions) =
            widget.layout_children(constraints, &[], &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(size, Size::ZERO);
        assert!(positions.is_empty());
        assert!(
            widget
                .paint_primitives_for_phase(
                    PaintPhase::BeforeChildren,
                    Rect::from_min_size(Point::ZERO, Size::new(1.0, 1.0)),
                    &crate::runtime::DEFAULT_TEXT_METRICS
                )
                .is_empty()
        );
    }
}
