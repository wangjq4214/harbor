//! Spec 0038: measure real child fibers, not precomputed child sizes.
use crate::fiber::{Fiber, FiberArena, FiberId, layout_fiber};
use crate::layout::{BoxConstraints, LayoutError, Point, Rect, Size};
use crate::runtime::DEFAULT_TEXT_METRICS;
use crate::text::TextMetrics;
use crate::view::AnyView;
use crate::{
    Align, Alignment2D, Axis, Column, CrossAxisAlignment, Flex, FlexOptions, Insets,
    MainAxisAlignment, Padding, Row, SizedBox,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

struct Probe {
    requests: Rc<RefCell<Vec<BoxConstraints>>>,
    natural: Rc<Cell<Size>>,
}
impl AnyView for Probe {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        self.requests.borrow_mut().push(constraints);
        constraints.constrain(self.natural.get())
    }
}
fn insert(arena: &mut FiberArena, view: impl AnyView, children: Vec<FiberId>) -> FiberId {
    let mut fiber = Fiber::new(None, view.widget_type(), Some(Arc::new(view)));
    fiber.children = children.clone();
    let id = arena.insert(fiber);
    for child in children {
        arena.get_mut(child).unwrap().parent = Some(id);
    }
    id
}
fn rect(arena: &FiberArena, id: FiberId) -> Rect {
    arena.get(id).unwrap().layout_rect().unwrap()
}
fn run(arena: &mut FiberArena, id: FiberId, constraints: BoxConstraints) -> bool {
    layout_fiber(arena, id, constraints, Point::ZERO, &DEFAULT_TEXT_METRICS).committed
}

#[test]
fn sized_axes_enforce_parent_and_remeasure_content_without_replacing_fibers() {
    for horizontal in [true, false] {
        let mut arena = FiberArena::new();
        let requests = Rc::new(RefCell::new(Vec::new()));
        let natural = Rc::new(Cell::new(Size::new(31.0, 27.0)));
        let child = insert(
            &mut arena,
            Probe {
                requests: requests.clone(),
                natural: natural.clone(),
            },
            vec![],
        );
        let sized = if horizontal {
            SizedBox::width(440.0)
        } else {
            SizedBox::height(440.0)
        }
        .unwrap();
        assert_eq!(sized.get_requested_width(), horizontal.then_some(440.0));
        assert_eq!(sized.get_requested_height(), (!horizontal).then_some(440.0));
        let root = insert(&mut arena, sized, vec![child]);
        let axis = if horizontal {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };
        for (parent, fixed) in [
            (BoxConstraints::loose(axis.size(300.0, 90.0)), 300.0),
            (
                BoxConstraints {
                    min: axis.size(500.0, 0.0),
                    max: axis.size(500.0, 90.0),
                },
                500.0,
            ),
            (BoxConstraints::loose(axis.size(700.0, 90.0)), 440.0),
            (
                BoxConstraints::loose(axis.size(f32::INFINITY, f32::INFINITY)),
                440.0,
            ),
        ] {
            requests.borrow_mut().clear();
            assert!(run(&mut arena, root, parent));
            let expected = BoxConstraints {
                min: axis.size(fixed, 0.0),
                max: axis.size(fixed, axis.cross(parent.max)),
            };
            assert_eq!(*requests.borrow(), [expected]);
            assert_eq!(
                rect(&arena, root).size(),
                axis.size(fixed, axis.cross(natural.get()))
            );
            assert_eq!(rect(&arena, child), rect(&arena, root));
        }
        natural.set(Size::new(45.0, 53.0));
        assert!(run(
            &mut arena,
            root,
            BoxConstraints::loose(Size::new(800.0, 600.0))
        ));
        assert_eq!(
            rect(&arena, root).size(),
            axis.size(440.0, axis.cross(natural.get()))
        );
        assert_eq!(arena.get(root).unwrap().children(), &[child]);
    }
    for sized in [
        SizedBox::width(440.0).unwrap(),
        SizedBox::height(440.0).unwrap(),
        SizedBox::new(Size::new(440.0, 440.0)).unwrap(),
    ] {
        let mut arena = FiberArena::new();
        let expected = Size::new(
            sized.get_requested_width().unwrap_or(17.0),
            sized.get_requested_height().unwrap_or(19.0),
        );
        let root = insert(&mut arena, sized, vec![]);
        assert!(run(
            &mut arena,
            root,
            BoxConstraints {
                min: Size::new(17.0, 19.0),
                max: Size::new(800.0, 600.0)
            }
        ));
        assert_eq!(rect(&arena, root).size(), expected);
    }
}

#[test]
fn named_insets_preserve_actual_deflation_positioning_and_unbounded_extent() {
    let insets = Insets {
        top: 5.0,
        right: 10.0,
        bottom: 15.0,
        left: 20.0,
    };
    for align in [false, true] {
        for parent in [
            BoxConstraints::loose(Size::new(200.0, 100.0)),
            BoxConstraints::tight(Size::new(200.0, 100.0)),
            BoxConstraints::loose(Size::new(10.0, 8.0)),
            BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)),
        ] {
            for has_child in [false, true] {
                let mut arena = FiberArena::new();
                let requests = Rc::new(RefCell::new(Vec::new()));
                let child = has_child.then(|| {
                    insert(
                        &mut arena,
                        Probe {
                            requests: requests.clone(),
                            natural: Rc::new(Cell::new(Size::new(40.0, 20.0))),
                        },
                        vec![],
                    )
                });
                let children = child.into_iter().collect();
                let root = if align {
                    insert(
                        &mut arena,
                        Align::new(Alignment2D::TOP_RIGHT).insets(insets).unwrap(),
                        children,
                    )
                } else {
                    insert(&mut arena, Padding::new(insets).unwrap(), children)
                };
                assert!(run(&mut arena, root, parent));
                let deflated = parent.deflate(Size::new(30.0, 20.0));
                let natural = if has_child {
                    deflated.constrain(Size::new(40.0, 20.0))
                } else {
                    Size::ZERO
                };
                let inflated = Size::new(natural.width + 30.0, natural.height + 20.0);
                let own = if align {
                    parent.fill_bounded(inflated)
                } else {
                    parent.constrain(inflated)
                };
                assert_eq!(rect(&arena, root).size(), own);
                if let Some(child) = child {
                    assert_eq!(*requests.borrow(), [deflated]);
                    let x = if align {
                        20.0 + (own.width - 30.0 - natural.width).max(0.0)
                    } else {
                        20.0
                    };
                    assert_eq!(
                        rect(&arena, child),
                        Rect::from_min_size(Point::new(x, 5.0), natural)
                    );
                }
            }
        }
    }
}

#[test]
fn valid_scalar_combinations_overflow_in_layout_and_keep_committed_geometry() {
    let mut arena = FiberArena::new();
    let child = insert(
        &mut arena,
        SizedBox::new(Size::new(20.0, 10.0)).unwrap(),
        vec![],
    );
    let root = insert(&mut arena, Padding::all(1.0).unwrap(), vec![child]);
    let parent = BoxConstraints::loose(Size::new(100.0, 100.0));
    assert!(run(&mut arena, root, parent));
    let previous = [rect(&arena, root), rect(&arena, child)];
    let huge = Insets {
        left: f32::MAX,
        right: f32::MAX,
        ..Default::default()
    };
    #[expect(
        clippy::arc_with_non_send_sync,
        reason = "retained Fiber views require Arc even for UI-local widgets"
    )]
    let candidates: Vec<Arc<dyn AnyView>> = vec![
        Arc::new(Padding::new(huge).unwrap()),
        Arc::new(Align::new(Alignment2D::CENTER).insets(huge).unwrap()),
    ];
    for candidate in candidates {
        arena.get_mut(root).unwrap().view = Some(candidate);
        assert!(!run(&mut arena, root, parent));
        assert_eq!(
            arena.get(root).unwrap().layout_error(),
            Some(LayoutError::InvalidGeometry)
        );
        assert_eq!([rect(&arena, root), rect(&arena, child)], previous);
    }
    // A gap is valid without knowing the number of children. Two gaps overflow.
    let third = insert(&mut arena, SizedBox::new(Size::ZERO).unwrap(), vec![]);
    let second = insert(&mut arena, SizedBox::new(Size::ZERO).unwrap(), vec![]);
    let flex = insert(
        &mut arena,
        Row::with_options(FlexOptions {
            gap: f32::MAX,
            ..Default::default()
        })
        .unwrap(),
        vec![child, second, third],
    );
    assert!(!run(&mut arena, flex, parent));
    assert_eq!(
        arena.get(flex).unwrap().layout_error(),
        Some(LayoutError::InvalidGeometry)
    );
    assert_eq!(rect(&arena, flex).size(), Size::ZERO);
    assert_eq!(rect(&arena, child), previous[1]);
}

#[test]
fn options_and_builders_share_mounted_geometry_and_diagnostics() {
    let options = FlexOptions {
        gap: 7.0,
        main_axis_alignment: MainAxisAlignment::SpaceBetween,
        cross_axis_alignment: CrossAxisAlignment::Stretch,
    };
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for parent in [
            BoxConstraints::loose(Size::new(90.0, 80.0)),
            BoxConstraints::tight(Size::new(50.0, 30.0)),
            BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)),
        ] {
            #[expect(
                clippy::arc_with_non_send_sync,
                reason = "retained Fiber views require Arc even for UI-local widgets"
            )]
            let alternatives: Vec<Arc<dyn AnyView>> = if axis == Axis::Horizontal {
                vec![
                    Arc::new(Flex::with_options(axis, options).unwrap()),
                    Arc::new(Row::with_options(options).unwrap()),
                    Arc::new(
                        Row::new()
                            .main_axis_alignment(options.main_axis_alignment)
                            .cross_axis_alignment(options.cross_axis_alignment)
                            .gap(7.0)
                            .unwrap(),
                    ),
                ]
            } else {
                vec![
                    Arc::new(Flex::with_options(axis, options).unwrap()),
                    Arc::new(Column::with_options(options).unwrap()),
                    Arc::new(
                        Column::new()
                            .main_axis_alignment(options.main_axis_alignment)
                            .cross_axis_alignment(options.cross_axis_alignment)
                            .gap(7.0)
                            .unwrap(),
                    ),
                ]
            };
            let mut arena = FiberArena::new();
            let a = insert(
                &mut arena,
                SizedBox::new(Size::new(40.0, 20.0)).unwrap(),
                vec![],
            );
            let b = insert(
                &mut arena,
                SizedBox::new(Size::new(30.0, 10.0)).unwrap(),
                vec![],
            );
            let root = insert(&mut arena, Flex::new(axis), vec![a, b]);
            let mut reference = None;
            for candidate in alternatives {
                arena.get_mut(root).unwrap().view = Some(candidate);
                assert!(run(&mut arena, root, parent));
                let result = (
                    [rect(&arena, root), rect(&arena, a), rect(&arena, b)],
                    arena.get(root).unwrap().layout_diagnostics().to_vec(),
                );
                if let Some(reference) = &reference {
                    assert_eq!(&result, reference);
                } else {
                    reference = Some(result);
                }
            }
        }
    }
}
