use crate::decoration::{BorderRadius, BoxDecoration, ClipBehavior};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::clip::RoundedClip;
use crate::scene::primitive::Primitive;
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, PaintPhase, View};

/// A layout-neutral single-child wrapper that paints a box decoration.
#[derive(Clone)]
pub struct DecoratedBox {
    decoration: BoxDecoration,
    clip_behavior: ClipBehavior,
    child: Option<View>,
}

impl DecoratedBox {
    /// Caps blur input so its three-sigma raster extent remains finite.
    const MAX_BLUR_RADIUS: f32 = f32::MAX / 6.0;

    /// Bounds the visual contribution of a Gaussian-like blur in logical pixels.
    const fn blur_extent(blur_radius: f32) -> f32 {
        blur_radius.min(Self::MAX_BLUR_RADIUS) * 3.0
    }

    /// Keeps every shadow rasterizable around its hard or soft SDF edge.
    ///
    /// The one-pixel floor gives zero and very small blur radii the same
    /// finite derivative-aware raster boundary; the shader still limits soft
    /// coverage to the requested blur extent.
    const fn shadow_raster_extent(blur_radius: f32) -> f32 {
        Self::blur_extent(blur_radius).max(1.0)
    }

    fn finite_f32(value: f64) -> Option<f32> {
        (value.is_finite() && value.abs() <= f64::from(f32::MAX)).then_some(value as f32)
    }

    fn fill_primitive(&self, rect: Rect, radii: [f32; 4]) -> Option<Primitive> {
        let paint = self.decoration.paint().filter(crate::Paint::is_visible)?;
        Some(match paint {
            crate::Paint::Solid(color) => Primitive::RoundedQuad {
                rect,
                color,
                corner_radii: radii,
            },
            _ => Primitive::PaintedQuad {
                rect,
                paint,
                corner_radii: radii,
            },
        })
    }

    fn outer_shadow_primitive(
        &self,
        rect: Rect,
        radius: BorderRadius,
        shadow: &crate::decoration::BoxShadow,
    ) -> Option<Primitive> {
        let color = shadow.color();
        if color.a <= 0.0 {
            return None;
        }
        let spread = f64::from(shadow.spread_radius());
        let offset = shadow.offset();
        if shadow.blur_radius() == 0.0 && spread == 0.0 && offset.x == 0.0 && offset.y == 0.0 {
            return None;
        }
        let min_x = f64::from(rect.min.x) - spread + f64::from(offset.x);
        let min_y = f64::from(rect.min.y) - spread + f64::from(offset.y);
        let max_x = f64::from(rect.max.x) + spread + f64::from(offset.x);
        let max_y = f64::from(rect.max.y) + spread + f64::from(offset.y);
        let shape_width = max_x - min_x;
        let shape_height = max_y - min_y;
        if !shape_width.is_finite()
            || !shape_height.is_finite()
            || shape_width <= 0.0
            || shape_height <= 0.0
            || shape_width > f64::from(f32::MAX)
            || shape_height > f64::from(f32::MAX)
        {
            return None;
        }
        let extent = f64::from(Self::shadow_raster_extent(shadow.blur_radius()));
        let blur_min_x = min_x - extent;
        let blur_min_y = min_y - extent;
        let blur_max_x = max_x + extent;
        let blur_max_y = max_y + extent;
        let blur_width = blur_max_x - blur_min_x;
        let blur_height = blur_max_y - blur_min_y;
        if !blur_width.is_finite()
            || !blur_height.is_finite()
            || blur_width <= 0.0
            || blur_height <= 0.0
            || blur_width > f64::from(f32::MAX)
            || blur_height > f64::from(f32::MAX)
        {
            return None;
        }
        let shape_rect = Rect {
            min: Point::new(Self::finite_f32(min_x)?, Self::finite_f32(min_y)?),
            max: Point::new(Self::finite_f32(max_x)?, Self::finite_f32(max_y)?),
        };
        let blur_bounds = Rect {
            min: Point::new(Self::finite_f32(blur_min_x)?, Self::finite_f32(blur_min_y)?),
            max: Point::new(Self::finite_f32(blur_max_x)?, Self::finite_f32(blur_max_y)?),
        };
        let shape_size = shape_rect.size();
        let blur_size = blur_bounds.size();
        if !shape_size.width.is_finite()
            || !shape_size.height.is_finite()
            || shape_size.width <= 0.0
            || shape_size.height <= 0.0
            || !blur_size.width.is_finite()
            || !blur_size.height.is_finite()
            || blur_size.width <= 0.0
            || blur_size.height <= 0.0
        {
            return None;
        }
        Some(Primitive::OuterShadow {
            rect: blur_bounds,
            shape_rect,
            occluder_rect: rect,
            color,
            corner_radii: radius.normalize(shape_size).ok()?.as_array(),
            occluder_radii: radius.normalize(rect.size()).ok()?.as_array(),
            blur_radius: shadow.blur_radius().min(Self::MAX_BLUR_RADIUS),
        })
    }

    pub fn new(decoration: BoxDecoration) -> Self {
        Self {
            decoration,
            clip_behavior: ClipBehavior::None,
            child: None,
        }
    }

    /// Replaces the staged child. A DecoratedBox always owns at most one child.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }

    /// Selects the child clipping policy applied to paint and hit testing.
    ///
    /// `None` leaves descendants unclipped. `HardEdge` and `AntiAlias` clip
    /// descendants to this box's rounded shape without clipping this wrapper's
    /// own shadow, fill, or border.
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

impl crate::WithChildren for DecoratedBox {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("DecoratedBox", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for DecoratedBox {
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
        metrics: &TextMetrics,
    ) -> Vec<Primitive> {
        self.paint_primitives_with_slots_for_phase(phase, rect, metrics)
            .into_iter()
            .map(|(_, primitive)| primitive)
            .collect()
    }

    fn paint_primitives_with_slots_for_phase(
        &self,
        phase: PaintPhase,
        rect: Rect,
        _metrics: &TextMetrics,
    ) -> Vec<(u32, Primitive)> {
        let radius = self.decoration.border_radius_value().unwrap_or_default();
        let Ok(radii) = radius.normalize(rect.size()) else {
            return Vec::new();
        };
        let radii = radii.as_array();
        match phase {
            PaintPhase::BeforeChildren => {
                let mut primitives = self
                    .decoration
                    .shadows()
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, shadow)| {
                        self.outer_shadow_primitive(rect, radius, shadow)
                            .map(|primitive| (slot as u32, primitive))
                    })
                    .collect::<Vec<_>>();
                if let Some(fill) = self.fill_primitive(rect, radii) {
                    primitives.push((u32::MAX, fill));
                }
                primitives
            }
            PaintPhase::AfterChildren => self
                .decoration
                .border_value()
                .filter(|border| border.width() > 0.0 && border.paint().is_visible())
                .map(|border| {
                    let primitive = match border.paint() {
                        crate::Paint::Solid(color) => Primitive::RoundedBorder {
                            rect,
                            width: border.width(),
                            color,
                            corner_radii: radii,
                        },
                        paint => Primitive::PaintedBorder {
                            rect,
                            width: border.width(),
                            paint,
                            corner_radii: radii,
                        },
                    };
                    (0, primitive)
                })
                .into_iter()
                .collect(),
        }
    }

    fn descendant_clip(&self, rect: Rect) -> Option<RoundedClip> {
        if self.clip_behavior == ClipBehavior::None || rect.size().is_empty() {
            return None;
        }
        let radius = self.decoration.border_radius_value().unwrap_or_default();
        RoundedClip::new(rect, radius, self.clip_behavior).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::primitive::Color;
    use crate::widgets::sized_box::SizedBox;

    #[test]
    fn phase_paint_preserves_sparse_shadow_slots_fill_and_border_order() {
        let shadow = crate::BoxShadow::new()
            .try_color(Color::BLACK)
            .unwrap()
            .try_blur_radius(2.0)
            .unwrap();
        let widget = DecoratedBox::new(
            BoxDecoration::new()
                .shadow(crate::BoxShadow::default())
                .shadow(shadow)
                .try_color(Color::RED)
                .unwrap()
                .border(crate::Border::all(Color::BLUE, 1.0).unwrap()),
        );
        let metrics = &crate::runtime::DEFAULT_TEXT_METRICS;
        let rect = Rect::from_min_size(Point::ZERO, Size::new(20.0, 10.0));
        for phase in [PaintPhase::BeforeChildren, PaintPhase::AfterChildren] {
            let slotted = widget.paint_primitives_with_slots_for_phase(phase, rect, metrics);
            let slots: Vec<_> = slotted.iter().map(|(slot, _)| *slot).collect();
            assert_eq!(
                slots,
                if phase == PaintPhase::BeforeChildren {
                    vec![1, u32::MAX]
                } else {
                    vec![0]
                }
            );
            assert_eq!(
                widget.paint_primitives_for_phase(phase, rect, metrics),
                slotted
                    .into_iter()
                    .map(|(_, primitive)| primitive)
                    .collect::<Vec<_>>()
            );
            let invalid = Rect {
                min: Point::ZERO,
                max: Point::new(f32::NAN, 10.0),
            };
            assert!(
                widget
                    .paint_primitives_for_phase(phase, invalid, metrics)
                    .is_empty()
            );
            assert!(
                widget
                    .paint_primitives_with_slots_for_phase(phase, invalid, metrics)
                    .is_empty()
            );
        }
        let before = widget.paint_primitives_for_phase(PaintPhase::BeforeChildren, rect, metrics);
        assert!(matches!(before[0], Primitive::OuterShadow { .. }));
        assert!(matches!(
            before[1],
            Primitive::RoundedQuad {
                color: Color::RED,
                ..
            }
        ));
        assert!(matches!(
            widget.paint_primitives_for_phase(PaintPhase::AfterChildren, rect, metrics)[0],
            Primitive::RoundedBorder {
                color: Color::BLUE,
                ..
            }
        ));
    }

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

    fn layout_size(clip_behavior: ClipBehavior) -> Size {
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(clip_behavior)
            .child(SizedBox::new(Size::new(20.0, 10.0)))
            .layout_children(
                BoxConstraints::loose(Size::new(100.0, 100.0)),
                &[Size::new(20.0, 10.0)],
                &crate::runtime::DEFAULT_TEXT_METRICS,
            )
            .0
    }

    #[test]
    fn should_return_none_when_clip_behavior_is_none() {
        // Arrange
        let widget =
            DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
                .clip_behavior(ClipBehavior::None);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0));

        // Act
        let clip = widget.descendant_clip(rect);

        // Assert
        assert!(clip.is_none());
    }

    #[test]
    fn should_return_none_when_allocation_is_empty() {
        // Arrange
        let widget = DecoratedBox::new(BoxDecoration::new()).clip_behavior(ClipBehavior::HardEdge);

        // Act
        let clip = widget.descendant_clip(Rect::from_min_size(Point::ZERO, Size::ZERO));

        // Assert
        assert!(clip.is_none());
    }

    #[test]
    fn should_return_rounded_clip_when_hard_edge_policy_is_active() {
        // Arrange
        let widget =
            DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
                .clip_behavior(ClipBehavior::HardEdge);
        let rect = Rect::from_min_size(Point::new(2.0, 3.0), Size::new(20.0, 16.0));

        // Act
        let clip = widget.descendant_clip(rect).expect("active clip");

        // Assert
        assert_eq!(clip.rect(), rect);
        assert_eq!(clip.behavior(), ClipBehavior::HardEdge);
        assert_eq!(clip.radii().as_array(), [8.0; 4]);
    }

    #[test]
    fn should_keep_layout_size_when_clip_policy_is_active() {
        // Arrange / Act
        let none = layout_size(ClipBehavior::None);
        let hard = layout_size(ClipBehavior::HardEdge);
        let anti_alias = layout_size(ClipBehavior::AntiAlias);

        // Assert
        assert_eq!(none, Size::new(20.0, 10.0));
        assert_eq!(hard, none);
        assert_eq!(anti_alias, none);
    }
}
