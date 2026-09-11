use std::any::TypeId;
use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use harbor_widget::fiber::{Fiber, FiberId};
use harbor_widget::layout::{LayoutDiagnostic, LayoutError, Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::padding::Padding;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{
    Alignment, Axis, BoxDecoration, Column, ConstrainedBox, DecoratedBox, Expanded, Flex, FlexFit,
    Flexible, MainAxisAlignment, Row, Separator, Spacer,
};

fn size(axis: Axis, main: f32, cross: f32) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

fn point(axis: Axis, main: f32, cross: f32) -> Point {
    let size = size(axis, main, cross);
    Point::new(size.width, size.height)
}

fn main(axis: Axis, size: Size) -> f32 {
    match axis {
        Axis::Horizontal => size.width,
        Axis::Vertical => size.height,
    }
}

fn bounds(axis: Axis, minimum: f32, maximum: f32) -> ConstrainedBox {
    match axis {
        Axis::Horizontal => ConstrainedBox::new().min_width(minimum).max_width(maximum),
        Axis::Vertical => ConstrainedBox::new()
            .min_height(minimum)
            .max_height(maximum),
    }
}

fn tight(size: Size) -> ConstrainedBox {
    ConstrainedBox::new()
        .min_width(size.width)
        .max_width(size.width)
        .min_height(size.height)
        .max_height(size.height)
}

fn mount(component: impl Component + 'static, viewport: Size) -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(
        viewport.width as u32,
        viewport.height as u32,
        1.0,
    ));
    runtime.set_root(component);
    runtime.update(Instant::now());
    runtime
}

fn fiber<'a>(runtime: &'a Runtime, path: &[usize]) -> &'a Fiber {
    let mut id = runtime.root_id().unwrap();
    for &index in path {
        id = runtime.arena().get(id).unwrap().children()[index];
    }
    runtime.arena().get(id).unwrap()
}

fn rect(runtime: &Runtime, path: &[usize]) -> Rect {
    fiber(runtime, path).layout_rect().unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

fn assert_rect(actual: Rect, origin: Point, expected: Size) {
    close(actual.min.x, origin.x);
    close(actual.min.y, origin.y);
    close(actual.size().width, expected.width);
    close(actual.size().height, expected.height);
}

fn assert_finite_tree(runtime: &Runtime) -> usize {
    let mut pending = vec![runtime.root_id().unwrap()];
    let mut count = 0;
    while let Some(id) = pending.pop() {
        let fiber = runtime.arena().get(id).unwrap();
        assert_eq!(fiber.layout_error(), None, "fiber {id:?}");
        let rect = fiber.layout_rect().unwrap();
        for value in [
            rect.min.x,
            rect.min.y,
            rect.max.x,
            rect.max.y,
            rect.size().width,
            rect.size().height,
        ] {
            assert!(value.is_finite() && value >= 0.0, "{rect:?}");
        }
        for diagnostic in fiber.layout_diagnostics() {
            if let LayoutDiagnostic::Overflow { main, cross } = diagnostic {
                assert!(main.is_finite() && cross.is_finite() && *main >= 0.0 && *cross >= 0.0);
            }
        }
        pending.extend_from_slice(fiber.children());
        count += 1;
    }
    count
}

fn rail(axis: Axis, extent: f32) -> Flex {
    let separator = match axis {
        Axis::Horizontal => Separator::vertical(),
        Axis::Vertical => Separator::horizontal(),
    };
    Flex::new(axis)
        .cross_axis_alignment(Alignment::Stretch)
        .child(bounds(axis, extent, extent).child(SizedBox::new(Size::ZERO).color(Color::RED)))
        .child(separator.color(Color::BLACK))
        .child(Expanded::new().child(SizedBox::new(Size::ZERO).color(Color::BLUE)))
}

#[test]
fn real_runtime_rail_matrix_has_exact_bounded_rail_separator_and_residual() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for rail_extent in [56.0_f32, 200.0] {
            for width in [0.0_f32, 1.0, 55.0, 56.0, 57.0, 199.0, 200.0, 201.0, 1000.0] {
                let runtime = mount(rail(axis, rail_extent), size(axis, width, 80.0));
                assert_finite_tree(&runtime);
                let rail_width = rail_extent.min(width);
                let separator_width = 1.0_f32.min(width);
                let content = (width - rail_width - separator_width).max(0.0);
                assert_rect(rect(&runtime, &[]), Point::ZERO, size(axis, width, 80.0));
                assert_rect(
                    rect(&runtime, &[0]),
                    Point::ZERO,
                    size(axis, rail_width, 80.0),
                );
                assert_rect(
                    rect(&runtime, &[1]),
                    point(axis, rail_width, 0.0),
                    size(axis, separator_width, 80.0),
                );
                assert_rect(
                    rect(&runtime, &[2]),
                    point(axis, rail_width + separator_width, 0.0),
                    size(axis, content, 80.0),
                );
                assert_eq!(rect(&runtime, &[2]), rect(&runtime, &[2, 0]));
                let overflow = (rail_width + separator_width - width).max(0.0);
                if overflow > 0.0 {
                    assert_eq!(
                        fiber(&runtime, &[]).layout_diagnostics(),
                        &[LayoutDiagnostic::Overflow {
                            main: overflow,
                            cross: 0.0
                        }]
                    );
                } else {
                    assert!(fiber(&runtime, &[]).layout_diagnostics().is_empty());
                }
            }
        }
    }
}

#[test]
fn decorated_padded_root_deflates_before_flex_allocation() {
    let runtime = mount(
        DecoratedBox::new(BoxDecoration::default())
            .child(Padding::all(8.0).child(rail(Axis::Horizontal, 200.0))),
        Size::new(1016.0, 616.0),
    );
    assert_finite_tree(&runtime);
    assert_rect(
        rect(&runtime, &[0, 0, 0]),
        Point::new(8.0, 8.0),
        Size::new(200.0, 600.0),
    );
    assert_rect(
        rect(&runtime, &[0, 0, 1]),
        Point::new(208.0, 8.0),
        Size::new(1.0, 600.0),
    );
    assert_rect(
        rect(&runtime, &[0, 0, 2]),
        Point::new(209.0, 8.0),
        Size::new(799.0, 600.0),
    );
}

#[test]
fn main_and_cross_alignment_matrix_uses_actual_occupied_extent_plus_minimum_gaps() {
    let alignments = [
        (MainAxisAlignment::Start, [0.0, 15.0]),
        (MainAxisAlignment::Center, [32.5, 47.5]),
        (MainAxisAlignment::End, [65.0, 80.0]),
        (MainAxisAlignment::SpaceBetween, [0.0, 80.0]),
        (MainAxisAlignment::SpaceAround, [16.25, 63.75]),
        (
            MainAxisAlignment::SpaceEvenly,
            [65.0 / 3.0, 15.0 + 130.0 / 3.0],
        ),
    ];
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for (alignment, offsets) in alignments {
            for cross in [
                Alignment::Start,
                Alignment::Center,
                Alignment::End,
                Alignment::Stretch,
            ] {
                let runtime = mount(
                    tight(size(axis, 100.0, 40.0)).child(
                        Flex::new(axis)
                            .gap(5.0)
                            .main_axis_alignment(alignment)
                            .cross_axis_alignment(cross)
                            .child(SizedBox::new(size(axis, 10.0, 10.0)))
                            .child(SizedBox::new(size(axis, 20.0, 20.0))),
                    ),
                    size(axis, 100.0, 40.0),
                );
                assert_finite_tree(&runtime);
                for (index, &offset) in offsets.iter().enumerate() {
                    let natural = (index + 1) as f32 * 10.0;
                    let (cross_offset, extent) = match cross {
                        Alignment::Start => (0.0, natural),
                        Alignment::Center => ((40.0 - natural) / 2.0, natural),
                        Alignment::End => (40.0 - natural, natural),
                        Alignment::Stretch => (0.0, 40.0),
                    };
                    assert_rect(
                        rect(&runtime, &[0, index]),
                        point(axis, offset, cross_offset),
                        size(axis, natural, extent),
                    );
                }
            }
        }
    }
}

#[test]
fn loose_fit_keeps_unused_share_and_spacer_consumes_its_tight_share() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let runtime = mount(
            Flex::new(axis)
                .main_axis_alignment(MainAxisAlignment::End)
                .child(Flexible::new().child(SizedBox::new(size(axis, 10.0, 5.0))))
                .child(Spacer::new())
                .child(Expanded::new().child(SizedBox::new(size(axis, 2.0, 5.0)))),
            size(axis, 90.0, 20.0),
        );
        assert_finite_tree(&runtime);
        assert_rect(
            rect(&runtime, &[0]),
            point(axis, 20.0, 0.0),
            size(axis, 10.0, 5.0),
        );
        assert_rect(
            rect(&runtime, &[1]),
            point(axis, 30.0, 0.0),
            size(axis, 30.0, 0.0),
        );
        assert_rect(
            rect(&runtime, &[2]),
            point(axis, 60.0, 0.0),
            size(axis, 30.0, 5.0),
        );
    }
}

#[test]
fn factor_rounding_keeps_exact_committed_edge_without_phantom_overflow() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let runtime = mount(
            Flex::new(axis)
                .child(Expanded::new().child(SizedBox::new(Size::ZERO)))
                .child(
                    Expanded::new()
                        .factor(NonZeroU32::new(2).unwrap())
                        .child(SizedBox::new(Size::ZERO)),
                ),
            size(axis, 100.0, 20.0),
        );
        assert_finite_tree(&runtime);
        assert!(fiber(&runtime, &[]).layout_diagnostics().is_empty());
        let edge = rect(&runtime, &[1]).max;
        assert_eq!(main(axis, Size::new(edge.x, edge.y)), 100.0);
    }
}

#[test]
fn factors_and_fit_builders_allocate_proportional_sizes() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut flex = Flex::new(axis);
        for factor in [1, 2, 3] {
            flex = flex.child(
                Flexible::new()
                    .factor(NonZeroU32::new(factor).unwrap())
                    .fit(FlexFit::Tight)
                    .child(SizedBox::new(Size::ZERO)),
            );
        }
        let runtime = mount(flex, size(axis, 120.0, 20.0));
        assert_finite_tree(&runtime);
        for (index, expected) in [20.0, 40.0, 60.0].into_iter().enumerate() {
            close(main(axis, rect(&runtime, &[index]).size()), expected);
        }
    }
    assert!(
        NonZeroU32::new(0).is_none(),
        "zero allocation factors are unrepresentable"
    );
}

#[test]
fn empty_single_child_and_no_flex_preserve_shrink_wrap_and_parent_minimum() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for alignment in [
            MainAxisAlignment::Start,
            MainAxisAlignment::Center,
            MainAxisAlignment::End,
            MainAxisAlignment::SpaceBetween,
            MainAxisAlignment::SpaceAround,
            MainAxisAlignment::SpaceEvenly,
        ] {
            let runtime = mount(
                Flex::new(axis).gap(20.0).main_axis_alignment(alignment),
                size(axis, 100.0, 40.0),
            );
            assert_eq!(rect(&runtime, &[]).size(), Size::ZERO);
            let runtime = mount(
                bounds(axis, 50.0, 100.0).child(
                    Flex::new(axis)
                        .gap(20.0)
                        .main_axis_alignment(alignment)
                        .child(SizedBox::new(size(axis, 10.0, 5.0))),
                ),
                size(axis, 100.0, 40.0),
            );
            assert_finite_tree(&runtime);
            assert_eq!(rect(&runtime, &[0]).size(), size(axis, 50.0, 5.0));
            let offset = match alignment {
                MainAxisAlignment::Start | MainAxisAlignment::SpaceBetween => 0.0,
                MainAxisAlignment::End => 40.0,
                _ => 20.0,
            };
            assert_eq!(rect(&runtime, &[0, 0]).min, point(axis, offset, 0.0));
        }
    }
}

#[test]
fn constrained_box_caps_without_preferring_max_and_parent_minimum_wins() {
    let runtime = mount(
        ConstrainedBox::new()
            .max_width(200.0)
            .child(SizedBox::new(Size::new(40.0, 10.0))),
        Size::new(1000.0, 100.0),
    );
    assert_eq!(rect(&runtime, &[]).size(), Size::new(40.0, 10.0));
    let runtime = mount(
        tight(Size::new(80.0, 50.0)).child(
            ConstrainedBox::new()
                .max_width(20.0)
                .max_height(10.0)
                .child(SizedBox::new(Size::ZERO)),
        ),
        Size::new(1000.0, 100.0),
    );
    assert_finite_tree(&runtime);
    assert_eq!(rect(&runtime, &[0, 0]).size(), Size::new(80.0, 50.0));
}

#[test]
fn facades_preserve_type_identity_existing_backgrounds_and_shared_flex_behavior() {
    let runtime = mount(
        Row::new()
            .background(Color::RED)
            .gap(1.0)
            .main_axis_alignment(MainAxisAlignment::End)
            .child(SizedBox::new(Size::new(10.0, 5.0)))
            .child(Expanded::new().child(SizedBox::new(Size::ZERO))),
        Size::new(100.0, 40.0),
    );
    assert_eq!(fiber(&runtime, &[]).widget_type(), TypeId::of::<Row>());
    assert_eq!(rect(&runtime, &[1]).size(), Size::new(89.0, 0.0));
    assert!(
        runtime
            .pending_delta()
            .unwrap()
            .added
            .iter()
            .any(|item| matches!(
                item.primitive,
                Primitive::Quad {
                    color: Color::RED,
                    ..
                }
            ))
    );
    let runtime = mount(
        Column::new()
            .background(Color::BLUE)
            .gap(1.0)
            .main_axis_alignment(MainAxisAlignment::Start)
            .child(SizedBox::new(Size::new(5.0, 10.0)))
            .child(Expanded::new().child(SizedBox::new(Size::ZERO))),
        Size::new(40.0, 100.0),
    );
    assert_eq!(fiber(&runtime, &[]).widget_type(), TypeId::of::<Column>());
    assert_eq!(rect(&runtime, &[1]).size(), Size::new(0.0, 89.0));
}

#[derive(Clone)]
struct CountedLeaf(Rc<Cell<usize>>);

impl Component for CountedLeaf {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.0.set(self.0.get() + 1);
        SizedBox::new(Size::new(10.0, 5.0)).build(cx)
    }
}

#[test]
fn wrappers_defer_build_and_resizing_does_not_rebuild_components() {
    let builds = Rc::new(Cell::new(0));
    let component = Row::new()
        .child(Expanded::new().child(Padding::all(2.0).child(CountedLeaf(builds.clone()))));
    assert_eq!(builds.get(), 0);
    let mut runtime = mount(component, Size::new(100.0, 50.0));
    assert_eq!(builds.get(), 1);
    let id: FiberId = fiber(&runtime, &[]).children()[0];
    runtime.set_viewport(Viewport::new(200, 50, 1.0));
    runtime.update(Instant::now());
    assert_eq!(builds.get(), 1);
    assert_eq!(fiber(&runtime, &[]).children()[0], id);
    assert_eq!(rect(&runtime, &[0]).size().width, 200.0);
    assert_finite_tree(&runtime);
}

#[test]
fn metadata_does_not_tunnel_and_conflicting_immediate_wrappers_are_diagnosed() {
    let runtime = mount(
        Row::new().child(
            Padding::all(0.0).child(Expanded::new().child(SizedBox::new(Size::new(10.0, 5.0)))),
        ),
        Size::new(100.0, 50.0),
    );
    assert_finite_tree(&runtime);
    assert_eq!(rect(&runtime, &[]).size().width, 10.0);
    assert!(
        fiber(&runtime, &[0])
            .layout_diagnostics()
            .contains(&LayoutDiagnostic::InvalidFlexParent)
    );
    let runtime = mount(
        Row::new().child(
            Expanded::new().child(Flexible::new().child(SizedBox::new(Size::new(10.0, 5.0)))),
        ),
        Size::new(100.0, 50.0),
    );
    assert_finite_tree(&runtime);
    let conflict = [fiber(&runtime, &[0]), fiber(&runtime, &[0, 0])]
        .iter()
        .any(|fiber| {
            fiber
                .layout_diagnostics()
                .contains(&LayoutDiagnostic::ConflictingFlexParentData)
        });
    assert!(conflict);
}

#[test]
fn invalid_widget_numeric_values_abort_without_invalid_geometry_or_retry_loop() {
    for value in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut runtime = mount(
            Row::new().gap(value).child(SizedBox::new(Size::ZERO)),
            Size::new(100.0, 50.0),
        );
        assert_eq!(
            fiber(&runtime, &[]).layout_error(),
            Some(LayoutError::InvalidConstraints)
        );
        assert_eq!(rect(&runtime, &[]).size(), Size::ZERO);
        assert!(!runtime.update(Instant::now()).request_redraw);
        let runtime = mount(
            Separator::vertical().thickness(value),
            Size::new(100.0, 50.0),
        );
        assert_eq!(
            fiber(&runtime, &[]).layout_error(),
            Some(LayoutError::InvalidConstraints)
        );
    }
    let runtime = mount(
        ConstrainedBox::new().min_width(200.0).max_width(100.0),
        Size::new(1000.0, 50.0),
    );
    assert_eq!(
        fiber(&runtime, &[]).layout_error(),
        Some(LayoutError::InvalidConstraints)
    );
}

#[test]
fn fractional_dpi_keeps_logical_rail_and_fractional_residual_without_rounding() {
    let mut runtime = Runtime::new();
    runtime.set_root(rail(Axis::Horizontal, 56.0));
    for scale in [1.0, 1.25, 1.5, 2.0] {
        runtime.set_viewport(Viewport::new(1001, 603, scale));
        runtime.update(Instant::now());
        assert_finite_tree(&runtime);
        close(rect(&runtime, &[0]).size().width, 56.0);
        close(rect(&runtime, &[1]).size().width, 1.0);
        close(rect(&runtime, &[2]).size().width, 1001.0 / scale - 57.0);
        close(rect(&runtime, &[2]).max.x, 1001.0 / scale);
    }
}

#[test]
fn deterministic_finite_inputs_keep_all_allocations_and_diagnostics_finite() {
    let mut seed = 17_u32;
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for _ in 0..128 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let width = (seed % 2048) as f32;
            let gap = (seed % 29) as f32 / 4.0;
            let fixed = (seed % 256) as f32;
            let flex = Flex::new(axis)
                .gap(gap)
                .cross_axis_alignment(Alignment::Stretch)
                .child(bounds(axis, fixed, fixed))
                .child(
                    Flexible::new()
                        .factor(NonZeroU32::new(seed.max(1)).unwrap())
                        .child(SizedBox::new(size(axis, 17.25, 1.0))),
                )
                .child(
                    Expanded::new()
                        .factor(NonZeroU32::new(u32::MAX).unwrap())
                        .child(SizedBox::new(Size::ZERO)),
                );
            let runtime = mount(flex, size(axis, width, 31.0));
            assert_finite_tree(&runtime);
            close(main(axis, rect(&runtime, &[]).size()), width);
            assert!(main(axis, rect(&runtime, &[2]).size()) >= 0.0);
        }
    }
}

#[test]
fn excessive_gaps_and_inflexible_siblings_leave_zero_shares_and_clear_overflow_after_resize() {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let flex = Flex::new(axis)
            .gap(8.0)
            .main_axis_alignment(MainAxisAlignment::End)
            .child(SizedBox::new(size(axis, 40.0, 5.0)))
            .child(SizedBox::new(size(axis, 40.0, 5.0)))
            .child(Expanded::new());
        let mut runtime = mount(flex, size(axis, 10.0, 20.0));
        assert_finite_tree(&runtime);
        assert_eq!(
            fiber(&runtime, &[]).layout_diagnostics(),
            &[LayoutDiagnostic::Overflow {
                main: 6.0,
                cross: 0.0
            }]
        );
        for (index, offset) in [0.0, 8.0, 16.0].into_iter().enumerate() {
            assert_eq!(rect(&runtime, &[index]).min, point(axis, offset, 0.0));
            assert_eq!(main(axis, rect(&runtime, &[index]).size()), 0.0);
        }
        let viewport = size(axis, 100.0, 20.0);
        runtime.set_viewport(Viewport::new(
            viewport.width as u32,
            viewport.height as u32,
            1.0,
        ));
        runtime.update(Instant::now());
        assert_finite_tree(&runtime);
        assert!(fiber(&runtime, &[]).layout_diagnostics().is_empty());
        assert_eq!(main(axis, rect(&runtime, &[2]).size()), 4.0);

        let runtime = mount(
            Flex::new(axis)
                .main_axis_alignment(MainAxisAlignment::Center)
                .child(SizedBox::new(size(axis, 80.0, 5.0)))
                .child(SizedBox::new(size(axis, 80.0, 5.0)))
                .child(Expanded::new()),
            size(axis, 100.0, 20.0),
        );
        assert_finite_tree(&runtime);
        assert_eq!(
            fiber(&runtime, &[]).layout_diagnostics(),
            &[LayoutDiagnostic::Overflow {
                main: 60.0,
                cross: 0.0
            }]
        );
        assert_eq!(rect(&runtime, &[1]).min, point(axis, 80.0, 0.0));
        assert_eq!(rect(&runtime, &[2]).min, point(axis, 160.0, 0.0));
        assert_eq!(main(axis, rect(&runtime, &[2]).size()), 0.0);
    }
}

#[test]
fn wide_and_nested_flex_trees_complete_within_measurement_budget() {
    let mut wide = Row::new()
        .cross_axis_alignment(Alignment::Stretch)
        .gap(0.125);
    for index in 0..2048 {
        wide = wide.child(
            Expanded::new()
                .factor(NonZeroU32::new(index + 1).unwrap())
                .child(SizedBox::new(Size::ZERO)),
        );
    }
    let runtime = mount(wide, Size::new(4096.0, 100.0));
    assert!(assert_finite_tree(&runtime) > 4096);

    let mut nested = Flex::new(Axis::Horizontal).child(SizedBox::new(Size::new(1.0, 1.0)));
    for level in 0..24 {
        let axis = if level % 2 == 0 {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };
        nested = Flex::new(axis)
            .cross_axis_alignment(Alignment::Stretch)
            .child(SizedBox::new(size(axis, 1.0, 1.0)))
            .child(Expanded::new().child(nested));
    }
    let runtime = mount(nested, Size::new(512.0, 512.0));
    assert!(assert_finite_tree(&runtime) > 70);
}
