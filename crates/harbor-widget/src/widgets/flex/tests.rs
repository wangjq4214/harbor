use super::*;
use crate::layout::{FlexParentData, ParentData};
use std::num::NonZeroU32;

type ChildSpec = (f32, f32, Option<(u32, FlexFit)>);

#[derive(Clone)]
struct CorrectionLeaf {
    axis: Axis,
    measurements: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Component for CorrectionLeaf {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), Vec::new(), None)
    }
}

impl AnyView for CorrectionLeaf {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        self.measurements.set(self.measurements.get() + 1);
        let main = if self.axis.cross(constraints.min) >= 20.0 {
            80.0
        } else {
            50.0
        };
        constraints.constrain(self.axis.size(main, 10.0))
    }
}

fn layout_tree(
    component: impl Component + 'static,
    constraints: BoxConstraints,
) -> (crate::fiber::FiberArena, crate::fiber::FiberId) {
    let mut arena = crate::fiber::FiberArena::new();
    // Use the production deferred reconciler, then detach its temporary
    // staging parent so layout observes the actual unbounded root.
    let staging = arena.insert(crate::fiber::Fiber::new(
        None,
        std::any::TypeId::of::<()>(),
        None,
    ));
    let root = crate::fiber::reconcile_children_with_externals(
        &mut arena,
        staging,
        &[],
        vec![View::deferred(component)],
        &mut crate::view::ExternalRegistrations::default(),
    )[0];
    arena.remove(staging);
    arena.get_mut(root).unwrap().parent = None;
    crate::fiber::layout_fiber(
        &mut arena,
        root,
        constraints,
        Point::ZERO,
        &crate::runtime::DEFAULT_TEXT_METRICS,
    );
    (arena, root)
}

fn assert_committed_tree(arena: &crate::fiber::FiberArena, root: crate::fiber::FiberId) {
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        let fiber = arena.get(id).unwrap();
        assert_eq!(fiber.layout_error(), None, "fiber {id:?}");
        let rect = fiber.layout_rect().unwrap();
        assert!(rect.min.x.is_finite() && rect.min.y.is_finite());
        assert!(rect.max.x.is_finite() && rect.max.y.is_finite());
        assert!(rect.size().width >= 0.0 && rect.size().height >= 0.0);
        pending.extend_from_slice(fiber.children());
    }
}

#[test]
fn final_traversal_retains_correction_sensitive_descendants_through_alternating_axes() {
    use crate::widgets::sized_box::SizedBox;
    use crate::{Expanded, Flexible};
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let other_axis = match axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        for maximum in [100.0, f32::INFINITY] {
            for flexible in [false, true] {
                let measurements = std::rc::Rc::new(std::cell::Cell::new(0));
                let opposite =
                    Flex::new(other_axis).push_child(Expanded::new().child(CorrectionLeaf {
                        axis,
                        measurements: measurements.clone(),
                    }));
                let root = Flex::new(axis).cross_axis_alignment(CrossAxisAlignment::Stretch);
                let root = if flexible {
                    root.push_child(Flexible::new().child(opposite))
                } else {
                    root.push_child(opposite)
                };
                let root = root.push_child(SizedBox::new(axis.size(20.0, 20.0)).unwrap());
                let (arena, root) = layout_tree(
                    root,
                    BoxConstraints::loose(axis.size(maximum, f32::INFINITY)),
                );
                assert_committed_tree(&arena, root);
                let root_fiber = arena.get(root).unwrap();
                assert_eq!(
                    root_fiber.layout_rect().unwrap().size(),
                    axis.size(100.0, 20.0)
                );
                let opposite_id = root_fiber.children()[0];
                assert_eq!(
                    arena
                        .get(opposite_id)
                        .unwrap()
                        .layout_rect()
                        .unwrap()
                        .size(),
                    axis.size(80.0, 20.0)
                );
                let sibling = arena.get(root_fiber.children()[1]).unwrap();
                assert_eq!(sibling.layout_rect().unwrap().min, axis.point(80.0, 0.0));
                assert_eq!(
                    measurements.get(),
                    2,
                    "natural discovery plus final geometry"
                );
                let mut pending = vec![opposite_id];
                while let Some(id) = pending.pop() {
                    let fiber = arena.get(id).unwrap();
                    assert!(
                        !fiber
                            .layout_diagnostics()
                            .contains(&LayoutDiagnostic::UnboundedFlex),
                        "provisional diagnostic leaked"
                    );
                    if fiber.widget_type() == std::any::TypeId::of::<CorrectionLeaf>() {
                        assert_eq!(fiber.layout_rect().unwrap().size(), axis.size(80.0, 20.0));
                    }
                    pending.extend_from_slice(fiber.children());
                }
            }
        }
    }
}

#[test]
fn alternating_stretch_axes_finalize_deep_trees_under_unbounded_root_constraints() {
    use crate::Expanded;
    use crate::widgets::sized_box::SizedBox;
    for root_axis in [Axis::Horizontal, Axis::Vertical] {
        for maximum in [1000.0, f32::INFINITY] {
            for depth in [8, 32] {
                let mut tree =
                    Flex::new(root_axis).push_child(SizedBox::new(Size::new(1.0, 1.0)).unwrap());
                for level in 0..depth {
                    let axis = if level % 2 == 0 {
                        root_axis
                    } else {
                        match root_axis {
                            Axis::Horizontal => Axis::Vertical,
                            Axis::Vertical => Axis::Horizontal,
                        }
                    };
                    tree = Flex::new(axis)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .push_child(Expanded::new().child(tree))
                        .push_child(SizedBox::new(axis.size(1.0, level as f32 + 2.0)).unwrap());
                }
                let axis = tree.axis;
                let (arena, root) = layout_tree(
                    tree,
                    BoxConstraints::loose(axis.size(maximum, f32::INFINITY)),
                );
                assert_committed_tree(&arena, root);
                let size = arena.get(root).unwrap().layout_rect().unwrap().size();
                assert!(axis.main(size) > 0.0 && axis.cross(size) > 0.0);
            }
        }
    }
}

struct Children {
    axis: Axis,
    sizes: Vec<Size>,
    data: Vec<ParentData>,
    requests: Vec<Vec<BoxConstraints>>,
    order: Vec<usize>,
    grow_on_stretch: bool,
    provisional: bool,
    provisional_calls: Vec<usize>,
}

impl Children {
    fn new(axis: Axis, children: &[ChildSpec]) -> Self {
        Self {
            axis,
            sizes: children
                .iter()
                .map(|&(main, cross, _)| axis.size(main, cross))
                .collect(),
            data: children
                .iter()
                .map(|&(_, _, flex)| ParentData {
                    flex: flex.map(|(factor, fit)| FlexParentData {
                        factor: NonZeroU32::new(factor).unwrap(),
                        fit,
                    }),
                })
                .collect(),
            requests: vec![Vec::new(); children.len()],
            order: Vec::new(),
            grow_on_stretch: false,
            provisional: false,
            provisional_calls: Vec::new(),
        }
    }
}

impl ChildMeasurer for Children {
    fn is_provisional(&self) -> bool {
        self.provisional
    }

    fn measure_provisional(
        &mut self,
        index: usize,
        constraints: BoxConstraints,
    ) -> Result<Size, LayoutError> {
        self.provisional_calls.push(index);
        self.measure(index, constraints)
    }

    fn len(&self) -> usize {
        self.sizes.len()
    }
    fn parent_data(&self, index: usize) -> Result<ParentData, LayoutError> {
        Ok(self.data[index])
    }
    fn measure(&mut self, index: usize, constraints: BoxConstraints) -> Result<Size, LayoutError> {
        constraints.validate()?;
        self.order.push(index);
        self.requests[index].push(constraints);
        assert!(self.requests[index].len() <= 2, "more than one correction");
        let mut size = self.sizes[index];
        if self.grow_on_stretch && self.axis.cross(constraints.min) > self.axis.cross(size) {
            size = self
                .axis
                .size(self.axis.main(size) + 30.0, self.axis.cross(size));
        }
        Ok(constraints.constrain(size))
    }
}

fn engine(axis: Axis) -> FlexLayout {
    FlexLayout {
        axis,
        gap: 0.0,
        main_axis_alignment: MainAxisAlignment::Start,
        cross_axis_alignment: CrossAxisAlignment::Start,
    }
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.0001,
        "{actual} != {expected}"
    );
}

#[test]
fn measures_inflexible_first_and_uses_final_residue_in_both_axes() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut children = Children::new(
            axis,
            &[
                (0.0, 10.0, Some((1, FlexFit::Tight))),
                (10.0, 10.0, None),
                (0.0, 10.0, Some((2, FlexFit::Tight))),
                (0.0, 10.0, Some((3, FlexFit::Tight))),
            ],
        );
        let mut engine = engine(axis);
        engine.gap = 1.0;
        let result = engine
            .layout(
                BoxConstraints::loose(axis.size(100.25, 20.0)),
                &mut children,
            )
            .unwrap();
        assert_eq!(children.order, [1, 0, 2, 3]);
        assert_eq!(axis.main(result.size), 100.25);
        let shares: Vec<_> = [0, 2, 3]
            .map(|i| axis.main(children.requests[i][0].max))
            .to_vec();
        close(shares[0], 87.25 / 6.0);
        close(shares[1], 87.25 / 3.0);
        close(shares.iter().sum(), 87.25);
        for index in [0, 2, 3] {
            assert_eq!(
                axis.main(children.requests[index][0].min),
                axis.main(children.requests[index][0].max)
            );
        }
    }
}

#[test]
fn maximum_integer_factors_do_not_overflow_and_loose_fit_is_not_redistributed() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut children = Children::new(
            axis,
            &[
                (10.0, 5.0, Some((u32::MAX, FlexFit::Loose))),
                (0.0, 5.0, Some((u32::MAX, FlexFit::Tight))),
            ],
        );
        let mut engine = engine(axis);
        engine.main_axis_alignment = MainAxisAlignment::End;
        let result = engine
            .layout(BoxConstraints::loose(axis.size(100.0, 20.0)), &mut children)
            .unwrap();
        assert_eq!(children.order, [0, 1]);
        assert_eq!(axis.main(children.requests[0][0].min), 0.0);
        assert_eq!(axis.main(children.requests[1][0].min), 50.0);
        assert_eq!(
            result.placements,
            [(0, axis.point(40.0, 0.0)), (1, axis.point(50.0, 0.0))]
        );
    }
}

#[test]
fn unbounded_flex_is_finite_content_driven_and_spacer_is_zero() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut children = Children::new(
            axis,
            &[
                (12.0, 5.0, Some((1, FlexFit::Tight))),
                (17.0, 7.0, Some((1, FlexFit::Loose))),
                (0.0, 0.0, Some((1, FlexFit::Tight))),
            ],
        );
        let result = engine(axis)
            .layout(
                BoxConstraints::loose(axis.size(f32::INFINITY, f32::INFINITY)),
                &mut children,
            )
            .unwrap();
        assert_eq!(result.size, axis.size(29.0, 7.0));
        assert_eq!(result.diagnostics, [LayoutDiagnostic::UnboundedFlex]);
        for requests in children.requests {
            assert_eq!(axis.main(requests[0].min), 0.0);
            assert_eq!(axis.main(requests[0].max), f32::INFINITY);
        }
    }
}

#[test]
fn unbounded_stretch_corrects_once_and_uses_final_main_extent_and_overflow() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut children = Children::new(axis, &[(50.0, 10.0, None), (70.0, 20.0, None)]);
        children.grow_on_stretch = true;
        let mut engine = engine(axis);
        engine.cross_axis_alignment = CrossAxisAlignment::Stretch;
        let result = engine
            .layout(
                BoxConstraints::loose(axis.size(100.0, f32::INFINITY)),
                &mut children,
            )
            .unwrap();
        assert_eq!(children.order, [0, 1, 0, 1]);
        assert_eq!(children.provisional_calls, [0, 1]);
        assert_eq!(children.requests[1][1].min, axis.size(0.0, 20.0));
        assert_eq!(children.requests[0][1].min, axis.size(0.0, 20.0));
        assert_eq!(children.requests[0][1].max, axis.size(100.0, 20.0));
        assert_eq!(result.size, axis.size(100.0, 20.0));
        assert_eq!(result.placements[1].1, axis.point(80.0, 0.0));
        assert_eq!(
            result.diagnostics,
            [LayoutDiagnostic::Overflow {
                main: 50.0,
                cross: 0.0
            }]
        );
    }
}

#[test]
fn provisional_unbounded_stretch_measures_every_fit_naturally_without_correction() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut children = Children::new(
            axis,
            &[
                (10.0, 5.0, Some((1, FlexFit::Loose))),
                (20.0, 10.0, None),
                (0.0, 20.0, Some((1, FlexFit::Tight))),
            ],
        );
        children.provisional = true;
        let mut engine = engine(axis);
        engine.cross_axis_alignment = CrossAxisAlignment::Stretch;
        let result = engine
            .layout(
                BoxConstraints::loose(axis.size(100.0, f32::INFINITY)),
                &mut children,
            )
            .unwrap();
        assert_eq!(children.order, [1, 0, 2]);
        assert_eq!(children.provisional_calls, children.order);
        assert_eq!(result.size, axis.size(100.0, 20.0));
        assert!(children.requests.iter().all(|requests| requests.len() == 1));
    }
}

#[test]
fn proportional_share_rounding_does_not_emit_phantom_overflow() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut children = Children::new(
            axis,
            &[
                (0.0, 5.0, Some((1, FlexFit::Tight))),
                (0.0, 5.0, Some((2, FlexFit::Tight))),
            ],
        );
        let engine = engine(axis);
        let result = engine
            .layout(BoxConstraints::loose(axis.size(100.0, 20.0)), &mut children)
            .unwrap();
        assert!(result.diagnostics.is_empty());
        let position = result.placements[1].1;
        assert_eq!(
            axis.main(Size::new(position.x, position.y)) + axis.main(children.requests[1][0].min),
            100.0
        );
        let overflow = engine
            .finish(
                BoxConstraints::loose(axis.size(100.0, 20.0)),
                &[axis.size(50.0, 5.0), axis.size(50.01, 5.0)],
                false,
                vec![],
            )
            .unwrap();
        assert!(
            matches!(overflow.diagnostics.as_slice(), [LayoutDiagnostic::Overflow { main, .. }] if *main > 0.009)
        );
    }
}

#[test]
fn finite_stretch_is_supplied_on_the_first_measurement() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut children = Children::new(axis, &[(10.0, 1.0, None), (10.0, 3.0, None)]);
        let mut engine = engine(axis);
        engine.cross_axis_alignment = CrossAxisAlignment::Stretch;
        let result = engine
            .layout(BoxConstraints::loose(axis.size(100.0, 20.0)), &mut children)
            .unwrap();
        assert_eq!(result.size, axis.size(20.0, 20.0));
        assert_eq!(children.order, [0, 1]);
        for requests in children.requests {
            assert_eq!(axis.cross(requests[0].min), 20.0);
            assert_eq!(axis.cross(requests[0].max), 20.0);
        }
    }
}

#[test]
fn extreme_finite_shares_including_subnormal_residue_remain_finite() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for maximum in [0.0, f32::from_bits(1), f32::MIN_POSITIVE, 0.125, f32::MAX] {
            let mut children = Children::new(axis, &[(0.0, 0.0, Some((u32::MAX, FlexFit::Tight)))]);
            let result = engine(axis)
                .layout(
                    BoxConstraints::loose(axis.size(maximum, 1.0)),
                    &mut children,
                )
                .unwrap();
            assert_eq!(result.size, axis.size(maximum, 0.0));
            assert_eq!(result.placements, [(0, Point::ZERO)]);
            assert_eq!(axis.main(children.requests[0][0].min), maximum);
        }
    }
}

#[test]
fn negative_nan_and_infinite_gaps_and_arithmetic_overflow_are_errors() {
    for gap in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut engine = engine(Axis::Horizontal);
        engine.gap = gap;
        let mut children = Children::new(Axis::Horizontal, &[]);
        assert!(matches!(
            engine.layout(BoxConstraints::loose(Size::new(10.0, 10.0)), &mut children),
            Err(LayoutError::InvalidConstraints)
        ));
    }
    let mut engine = engine(Axis::Horizontal);
    engine.gap = f32::MAX;
    let mut children = Children::new(Axis::Horizontal, &[(0.0, 0.0, None); 3]);
    assert!(matches!(
        engine.layout(BoxConstraints::loose(Size::new(10.0, 10.0)), &mut children),
        Err(LayoutError::InvalidGeometry)
    ));
    let engine = self::engine(Axis::Horizontal);
    assert!(matches!(
        engine.finish(
            BoxConstraints::loose(Size::new(f32::INFINITY, 10.0)),
            &[Size::new(f32::MAX, 1.0); 2],
            false,
            vec![]
        ),
        Err(LayoutError::InvalidGeometry)
    ));
}

#[test]
fn overflowing_children_align_from_start_and_report_both_axes() {
    let mut engine = engine(Axis::Vertical);
    engine.main_axis_alignment = MainAxisAlignment::End;
    engine.cross_axis_alignment = CrossAxisAlignment::Center;
    let result = engine
        .finish(
            BoxConstraints::tight(Size::new(10.0, 20.0)),
            &[Size::new(15.0, 30.0)],
            false,
            vec![],
        )
        .unwrap();
    assert_eq!(result.placements, [(0, Point::ZERO)]);
    assert_eq!(
        result.diagnostics,
        [LayoutDiagnostic::Overflow {
            main: 10.0,
            cross: 5.0
        }]
    );
}
