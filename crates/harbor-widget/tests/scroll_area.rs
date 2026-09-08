use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::Color;
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{
    Button, Column, ConstrainedBox, Focus, FocusHandle, ScrollArea, ScrollController,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

fn update(runtime: &mut Runtime) -> bool {
    runtime.update(Instant::now()).request_redraw
}

fn wheel(y: f32, dy: f32) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(
        Point::new(5.0, y),
        PointerPhase::WheelLine { dx: 0.0, dy },
        PointerButton::Left,
        1,
    ))
}

fn key(key: Key) -> UiEvent {
    UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
    })
}

fn child_rect(runtime: &Runtime) -> Rect {
    let root = runtime.root_id().unwrap();
    let child = runtime.arena().get(root).unwrap().children()[0];
    runtime.arena().get(child).unwrap().layout_rect().unwrap()
}

#[test]
fn scroll_area_commits_metrics_moves_content_and_settles_at_boundaries() {
    let controller = ScrollController::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(
        ScrollArea::new()
            .controller(controller.clone())
            .line_step(25.0)
            .child(Focus::new(
                SizedBox::new(Size::new(100.0, 100.0)).color(Color::RED),
            )),
    );

    assert!(update(&mut runtime));
    runtime.dispatch(key(Key::Tab));
    assert_eq!(controller.metrics().viewport_extent(), 40.0);
    assert_eq!(controller.metrics().content_extent(), 100.0);
    assert_eq!(controller.metrics().max_scroll_extent(), 60.0);
    assert_eq!(child_rect(&runtime).min.y, 0.0);

    let toward_start_at_top = runtime.dispatch(wheel(10.0, 1.0));
    assert!(!toward_start_at_top.request_redraw);
    assert_eq!(controller.offset(), 0.0);

    let effects = runtime.dispatch(wheel(10.0, -1.0));
    assert!(effects.request_redraw);
    assert_eq!(controller.offset(), 25.0);
    assert!(update(&mut runtime));
    assert_eq!(child_rect(&runtime).min.y, -25.0);

    assert!(runtime.dispatch(key(Key::End)).request_redraw);
    assert!(update(&mut runtime));
    assert_eq!(controller.offset(), 60.0);
    assert_eq!(child_rect(&runtime).min.y, -60.0);

    let boundary = runtime.dispatch(wheel(10.0, -1.0));
    assert!(!boundary.request_redraw);
    assert!(!update(&mut runtime));
}

#[test]
fn pixel_page_home_and_ignored_input_follow_committed_boundaries() {
    let controller = ScrollController::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(
        ScrollArea::new()
            .controller(controller.clone())
            .child(Focus::new(SizedBox::new(Size::new(100.0, 100.0)))),
    );
    update(&mut runtime);
    runtime.dispatch(key(Key::Tab));

    let pixel = UiEvent::Pointer(PointerEvent::new(
        Point::new(5.0, 5.0),
        PointerPhase::WheelPixel { dx: 0.0, dy: -7.5 },
        PointerButton::Left,
        2,
    ));
    assert!(runtime.dispatch(pixel).request_redraw);
    assert_eq!(controller.offset(), 7.5);
    update(&mut runtime);

    assert!(runtime.dispatch(key(Key::PageDown)).request_redraw);
    assert_eq!(controller.offset(), 47.5);
    update(&mut runtime);
    assert!(runtime.dispatch(key(Key::Home)).request_redraw);
    assert_eq!(controller.offset(), 0.0);
    update(&mut runtime);

    let horizontal = UiEvent::Pointer(PointerEvent::new(
        Point::new(5.0, 5.0),
        PointerPhase::WheelLine { dx: 2.0, dy: 0.0 },
        PointerButton::Left,
        2,
    ));
    assert!(!runtime.dispatch(horizontal).request_redraw);
    let modified = UiEvent::Pointer(
        PointerEvent::new(
            Point::new(5.0, 5.0),
            PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            PointerButton::Left,
            2,
        )
        .with_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::default()
        }),
    );
    assert!(!runtime.dispatch(modified).request_redraw);
}

#[test]
fn scroll_area_attaches_hard_viewport_clip_to_descendant_paint() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 30, 1.0));
    runtime
        .set_root(ScrollArea::new().child(SizedBox::new(Size::new(80.0, 90.0)).color(Color::BLUE)));
    update(&mut runtime);

    let item = runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .find(|item| !item.clips.is_empty())
        .expect("painted child inherits the viewport clip");
    assert_eq!(item.clips.len(), 1);
    assert_eq!(item.clips[0].rect().size(), Size::new(80.0, 30.0));
    assert_eq!(
        item.clips[0].behavior(),
        harbor_widget::ClipBehavior::HardEdge
    );
}

#[test]
fn zero_height_viewport_keeps_descendants_clipped_and_unhittable() {
    let activated = Arc::new(AtomicBool::new(false));
    let observed = activated.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 0, 1.0));
    runtime
        .set_root(ScrollArea::new().child(
            Button::new("hidden").on_click(move |_| observed.store(true, Ordering::SeqCst)),
        ));
    update(&mut runtime);

    let clipped = runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .find(|item| !item.clips.is_empty())
        .expect("zero viewport clip remains attached to descendants");
    assert_eq!(clipped.clips[0].rect().size(), Size::new(100.0, 0.0));

    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(10.0, 10.0),
            phase,
            PointerButton::Left,
            10,
        )));
    }
    assert!(!activated.load(Ordering::SeqCst));
}

#[test]
fn nested_scroll_area_bubbles_to_outer_only_after_inner_reaches_boundary() {
    let outer = ScrollController::new();
    let inner = ScrollController::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(
        ScrollArea::new()
            .controller(outer.clone())
            .line_step(10.0)
            .child(
                Column::new()
                    .child(
                        ConstrainedBox::new()
                            .min_height(20.0)
                            .max_height(20.0)
                            .child(
                                ScrollArea::new()
                                    .controller(inner.clone())
                                    .line_step(10.0)
                                    .child(SizedBox::new(Size::new(100.0, 60.0))),
                            ),
                    )
                    .child(SizedBox::new(Size::new(100.0, 80.0))),
            ),
    );
    update(&mut runtime);

    for expected in [10.0, 20.0, 30.0, 40.0] {
        assert!(runtime.dispatch(wheel(10.0, -1.0)).request_redraw);
        update(&mut runtime);
        assert_eq!(inner.offset(), expected);
        assert_eq!(outer.offset(), 0.0);
    }
    assert!(runtime.dispatch(wheel(10.0, -1.0)).request_redraw);
    assert_eq!(inner.offset(), 40.0);
    assert_eq!(outer.offset(), 10.0);
}

#[test]
fn focus_navigation_reveals_offscreen_descendant_with_minimal_offset() {
    let controller = ScrollController::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(
        ScrollArea::new().controller(controller.clone()).child(
            Column::new()
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0)))),
        ),
    );
    update(&mut runtime);

    runtime.dispatch(key(Key::Tab));
    assert_eq!(controller.offset(), 0.0);
    runtime.dispatch(key(Key::Tab));
    assert_eq!(controller.offset(), 20.0);
    update(&mut runtime);
    runtime.dispatch(key(Key::Tab));
    assert_eq!(controller.offset(), 50.0);
    update(&mut runtime);
    assert_eq!(controller.metrics().offset(), 50.0);
    assert!(!update(&mut runtime));
}

#[test]
fn pending_focus_handle_reveals_target_after_first_successful_layout() {
    let controller = ScrollController::new();
    let target = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.request_focus(&target);
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(
        ScrollArea::new().controller(controller.clone()).child(
            Column::new()
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))).handle(target)),
        ),
    );

    assert!(update(&mut runtime));
    assert_eq!(controller.metrics().offset(), 50.0);
    assert!(!update(&mut runtime));
}

#[test]
fn pending_focus_reveal_retries_after_zero_viewport_restores() {
    let controller = ScrollController::new();
    let target = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.request_focus(&target);
    runtime.set_viewport(Viewport::new(100, 0, 1.0));
    runtime.set_root(
        ScrollArea::new().controller(controller.clone()).child(
            Column::new()
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))).handle(target)),
        ),
    );

    assert!(update(&mut runtime));
    assert_eq!(controller.offset(), 0.0);

    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    assert!(update(&mut runtime));
    assert_eq!(controller.offset(), 50.0);
    assert!(update(&mut runtime));
    assert_eq!(controller.metrics().offset(), 50.0);
    assert!(!update(&mut runtime));
}

#[test]
fn nested_focus_reveal_accounts_for_inner_scroll_before_outer_scroll() {
    let outer = ScrollController::new();
    let inner = ScrollController::new();
    let target = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.request_focus(&target);
    runtime.set_viewport(Viewport::new(100, 100, 1.0));
    runtime.set_root(
        ScrollArea::new().controller(outer.clone()).child(
            Column::new()
                .child(SizedBox::new(Size::new(100.0, 80.0)))
                .child(
                    ConstrainedBox::new()
                        .min_height(40.0)
                        .max_height(40.0)
                        .child(
                            ScrollArea::new().controller(inner.clone()).child(
                                Column::new()
                                    .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                                    .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                                    .child(
                                        Focus::new(SizedBox::new(Size::new(100.0, 30.0)))
                                            .handle(target),
                                    ),
                            ),
                        ),
                )
                .child(SizedBox::new(Size::new(100.0, 80.0))),
        ),
    );

    assert!(update(&mut runtime));
    assert_eq!(inner.metrics().offset(), 50.0);
    assert_eq!(outer.metrics().offset(), 20.0);
    assert!(!update(&mut runtime));
}

#[test]
fn content_shrink_clamps_geometry_in_the_same_commit_then_converges() {
    #[derive(Clone)]
    struct DynamicContent {
        height: Signal<f32>,
        controller: ScrollController,
    }

    impl Component for DynamicContent {
        fn build(&self, cx: &mut BuildCx) -> View {
            cx.track(&self.height);
            ScrollArea::new()
                .controller(self.controller.clone())
                .child(SizedBox::new(Size::new(100.0, *self.height.read())))
                .build(cx)
        }
    }

    let height = Signal::new_distinct(100.0);
    let controller = ScrollController::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(DynamicContent {
        height: height.clone(),
        controller: controller.clone(),
    });
    update(&mut runtime);
    controller.jump_to(60.0);
    update(&mut runtime);
    assert_eq!(controller.metrics().offset(), 60.0);

    height.set(20.0);
    assert!(update(&mut runtime));
    assert_eq!(controller.metrics().offset(), 0.0);
    assert_eq!(child_rect(&runtime).min.y, 0.0);
    assert!(update(&mut runtime));
    assert!(!update(&mut runtime));
}

#[test]
fn failed_layout_defers_focus_reveal_until_fresh_geometry_commits() {
    #[derive(Clone)]
    struct FallibleScroll {
        invalid: Signal<bool>,
        controller: ScrollController,
        target: FocusHandle,
    }

    impl Component for FallibleScroll {
        fn build(&self, cx: &mut BuildCx) -> View {
            cx.track(&self.invalid);
            let constrained = if *self.invalid.read() {
                ConstrainedBox::new().min_width(100.0).max_width(50.0)
            } else {
                ConstrainedBox::new()
                    .min_width(100.0)
                    .max_width(100.0)
                    .min_height(40.0)
                    .max_height(40.0)
            };
            constrained
                .child(
                    ScrollArea::new().controller(self.controller.clone()).child(
                        Column::new()
                            .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                            .child(Focus::new(SizedBox::new(Size::new(100.0, 30.0))))
                            .child(
                                Focus::new(SizedBox::new(Size::new(100.0, 30.0)))
                                    .handle(self.target),
                            ),
                    ),
                )
                .build(cx)
        }
    }

    let invalid = Signal::new_distinct(false);
    let controller = ScrollController::new();
    let target = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(FallibleScroll {
        invalid: invalid.clone(),
        controller: controller.clone(),
        target,
    });
    update(&mut runtime);

    invalid.set(true);
    assert!(update(&mut runtime));
    runtime.request_focus(&target);
    assert_eq!(controller.offset(), 0.0);

    invalid.set(false);
    assert!(update(&mut runtime));
    assert_eq!(controller.offset(), 50.0);
    assert!(update(&mut runtime));
    assert_eq!(controller.metrics().offset(), 50.0);
    assert!(!update(&mut runtime));
}

#[test]
fn descendant_hit_testing_is_clipped_to_the_scroll_viewport() {
    let activated = Arc::new(AtomicBool::new(false));
    let observed = activated.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 40, 1.0));
    runtime.set_root(
        ScrollArea::new().child(Column::new().child(Button::new("first")).child(
            Button::new("second").on_click(move |_| {
                observed.store(true, Ordering::SeqCst);
            }),
        )),
    );
    update(&mut runtime);

    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(10.0, 50.0),
            phase,
            PointerButton::Left,
            9,
        )));
    }
    assert!(!activated.load(Ordering::SeqCst));
}
