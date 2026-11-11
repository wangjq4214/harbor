use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerBoundaryEvent, PointerBoundaryKind, PointerButton,
    PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::{
    SceneItem,
    primitive::{Color, Primitive},
};
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component};
use harbor_widget::widgets::align::Align;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{
    Alignment2D, Button, Children, Column, FocusHandle, IconButton, LucideIcon, Row,
    ScrollController, ScrollView, TextLabel, Tooltip, WithChildren,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

fn update(runtime: &mut Runtime) {
    runtime.update(Instant::now());
}

fn pointer(runtime: &mut Runtime, point: Point, phase: PointerPhase) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        point,
        phase,
        PointerButton::Left,
        1,
    )));
    update(runtime);
}

// No GPU encoding is done in these mounted tests. Pending deltas are coalesced
// against the empty renderer baseline, so added/modified are the current scene.
fn items(runtime: &Runtime) -> Vec<&SceneItem> {
    runtime
        .pending_delta()
        .into_iter()
        .flat_map(|delta| delta.added.iter().chain(&delta.modified))
        .collect()
}

fn text(runtime: &Runtime) -> Vec<&str> {
    items(runtime)
        .into_iter()
        .filter_map(|item| match &item.primitive {
            Primitive::Text { text, .. } => Some(text.as_ref()),
            _ => None,
        })
        .collect()
}

fn hint_rect(runtime: &Runtime) -> Rect {
    items(runtime)
        .into_iter()
        .find_map(|item| match item.primitive {
            Primitive::Quad {
                rect,
                corner_radius: 4.0,
                ..
            } => Some(rect),
            _ => None,
        })
        .expect("painted tooltip surface")
}

fn red_rect(runtime: &Runtime) -> Rect {
    items(runtime)
        .into_iter()
        .find_map(|item| match item.primitive {
            Primitive::Quad {
                rect,
                color: Color::RED,
                ..
            } => Some(rect),
            _ => None,
        })
        .expect("target paint")
}

fn target() -> SizedBox {
    SizedBox::new(Size::new(30.0, 24.0))
        .unwrap()
        .background(Color::RED)
        .unwrap()
}

fn center(rect: Rect) -> Point {
    Point::new(
        (rect.min.x + rect.max.x) / 2.0,
        (rect.min.y + rect.max.y) / 2.0,
    )
}

#[test]
fn hover_paints_readable_text_to_the_right_without_changing_child_or_sibling_layout() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(400, 160, 1.0));
    runtime.set_root(
        Row::new()
            .push_child(target())
            .push_child(TextLabel::new("sibling")),
    );
    update(&mut runtime);
    let original_target = red_rect(&runtime);
    let original_sibling = items(&runtime)
        .into_iter()
        .find_map(|item| match &item.primitive {
            Primitive::Text { text, origin, .. } if text.as_ref() == "sibling" => Some(*origin),
            _ => None,
        })
        .unwrap();
    let original_root = runtime
        .arena()
        .get(runtime.root_id().unwrap())
        .unwrap()
        .layout_rect();

    runtime.set_root(
        Row::new()
            .push_child(Tooltip::new("Useful help").child(target()))
            .push_child(TextLabel::new("sibling")),
    );
    update(&mut runtime);
    assert!(!text(&runtime).contains(&"Useful help"));
    pointer(&mut runtime, center(original_target), PointerPhase::Move);
    assert!(text(&runtime).contains(&"Useful help"));
    assert_eq!(red_rect(&runtime), original_target);
    assert_eq!(
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .layout_rect(),
        original_root
    );
    assert!(items(&runtime).into_iter().any(|item| matches!(&item.primitive,
        Primitive::Text { text, origin, .. } if text.as_ref() == "sibling" && *origin == original_sibling)));
    let hint = hint_rect(&runtime);
    assert!(hint.min.x > original_target.max.x);
    assert!(hint.max.x <= 400.0 && hint.min.y >= 0.0 && hint.max.y <= 160.0);
    assert!(runtime.input().focused().is_none());

    // Neither post-layout callbacks nor extra settling turns should be required.
    assert!(!runtime.update(Instant::now()).request_redraw);
    assert!(text(&runtime).contains(&"Useful help"));
    pointer(&mut runtime, Point::new(390.0, 150.0), PointerPhase::Move);
    assert!(!text(&runtime).contains(&"Useful help"));
}

#[test]
fn overlay_never_flickers_or_steals_clicks_capture_focus_or_keyboard_activation() {
    let clicks = Rc::new(Cell::new(0));
    let observed = clicks.clone();
    let handle = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(96, 90, 1.0));
    runtime.set_root(
        Align::new(Alignment2D::CENTER).child(
            Tooltip::new("Help overlaps the control in this small window").child(
                IconButton::new(LucideIcon::Plus, "metadata only")
                    .focus_handle(handle)
                    .on_click(move |_| observed.set(observed.get() + 1)),
            ),
        ),
    );
    update(&mut runtime);
    runtime.request_focus(&handle);
    update(&mut runtime);
    let focused = runtime.input().focused();
    let point = Point::new(48.0, 45.0);
    pointer(&mut runtime, point, PointerPhase::Move);
    let hint = hint_rect(&runtime);
    assert!(
        hint.contains(point),
        "exercise a hint overlapping its own target"
    );
    let hovered = runtime.input().hovered();
    for _ in 0..3 {
        pointer(&mut runtime, point, PointerPhase::Move);
        assert!(!text(&runtime).is_empty(), "no self-induced hover exit");
        assert_eq!(runtime.input().hovered(), hovered);
        assert_eq!(runtime.input().focused(), focused);
    }
    assert_eq!(clicks.get(), 0);
    pointer(&mut runtime, point, PointerPhase::Down);
    assert!(
        text(&runtime).is_empty(),
        "press dismisses help without consuming activation"
    );
    pointer(&mut runtime, point, PointerPhase::Up);
    assert_eq!(clicks.get(), 1);
    assert_eq!(runtime.input().focused(), focused);
    assert!(runtime.input().captor(1).is_none());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    }));
    update(&mut runtime);
    assert_eq!(clicks.get(), 2);

    pointer(&mut runtime, point, PointerPhase::Move);
    pointer(&mut runtime, point, PointerPhase::Down);
    pointer(&mut runtime, point, PointerPhase::Cancel);
    assert!(text(&runtime).is_empty());
    assert!(runtime.input().captor(1).is_none());
    assert_eq!(clicks.get(), 2);
}

#[test]
fn portal_escapes_scroll_clip_and_target_scroll_hides_help_in_the_same_update() {
    let controller = ScrollController::new();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(420, 180, 1.0));
    runtime.set_root(
        SizedBox::new(Size::new(42.0, 40.0)).unwrap().child(
            ScrollView::new().controller(controller.clone()).child(
                Column::new()
                    .push_child(Tooltip::new("Outside the scroll viewport").child(target()))
                    .push_child(SizedBox::new(Size::new(30.0, 160.0)).unwrap()),
            ),
        ),
    );
    update(&mut runtime);
    pointer(&mut runtime, Point::new(10.0, 10.0), PointerPhase::Move);
    assert!(text(&runtime).contains(&"Outside the scroll viewport"));
    assert!(hint_rect(&runtime).max.x > 42.0);
    let hint = items(&runtime)
        .into_iter()
        .find(|item| {
            matches!(&item.primitive,
        Primitive::Text { text, .. } if text.as_ref() == "Outside the scroll viewport")
        })
        .unwrap();
    assert_eq!(hint.clips.len(), 1, "only the portal viewport clip remains");
    assert_eq!(
        hint.clips[0].rect(),
        Rect::from_min_size(Point::ZERO, Size::new(420.0, 180.0))
    );
    let target_item = items(&runtime)
        .into_iter()
        .find(|item| {
            matches!(
                &item.primitive,
                Primitive::Quad {
                    color: Color::RED,
                    ..
                }
            )
        })
        .unwrap();
    assert!(
        !target_item.clips.is_empty(),
        "test really is under scroll clipping"
    );

    assert!(controller.jump_to(30.0));
    update(&mut runtime);
    assert!(!text(&runtime).contains(&"Outside the scroll viewport"));
    assert!(red_rect(&runtime).min.y < 0.0);
    assert!(
        !runtime.update(Instant::now()).request_redraw,
        "no observer follow-up turn"
    );
    assert!(controller.jump_to(0.0));
    update(&mut runtime);
    pointer(&mut runtime, Point::new(10.0, 10.0), PointerPhase::Move);
    assert!(text(&runtime).contains(&"Outside the scroll viewport"));
    pointer(
        &mut runtime,
        Point::new(10.0, 10.0),
        PointerPhase::WheelPixel { dx: 0.0, dy: -8.0 },
    );
    assert!(!text(&runtime).contains(&"Outside the scroll viewport"));
    assert_eq!(
        controller.metrics().offset(),
        8.0,
        "Tooltip must not consume scrolling"
    );
}

#[test]
fn placement_clamps_right_bottom_and_small_viewports_at_multiple_scales() {
    for (width, height, scale) in [
        (400, 180, 1.0),
        (600, 270, 1.5),
        (800, 360, 2.0),
        (96, 64, 1.0),
        (96, 24, 1.0),
    ] {
        let mut runtime = Runtime::new();
        let viewport = Viewport::new(width, height, scale);
        let size = viewport.logical_size;
        runtime.set_viewport(viewport);
        runtime.set_root(
            Align::new(Alignment2D::BOTTOM_RIGHT).child(
                Tooltip::new(
                    "A long readable hint that wraps at the window edge and stays above the bottom",
                )
                .child(target()),
            ),
        );
        update(&mut runtime);
        let target = red_rect(&runtime);
        pointer(&mut runtime, center(target), PointerPhase::Move);
        let hint = hint_rect(&runtime);
        assert!(hint.min.x >= 0.0 && hint.min.y >= 0.0, "{hint:?}");
        assert!(
            hint.max.x <= size.width && hint.max.y <= size.height,
            "{hint:?} in {size:?}"
        );
        if size.width >= 400.0 {
            assert!(
                hint.max.x < target.min.x,
                "prefer adjacent left when the right cannot fit"
            );
        }
        let lines = text(&runtime);
        assert!(!lines.is_empty());
        if size.width < 100.0 {
            assert!(
                lines.last().unwrap().contains('…'),
                "height-capped help signals overflow"
            );
        }
        let metrics = runtime.text_metrics();
        for item in items(&runtime) {
            if let Primitive::Text {
                text,
                origin,
                color,
            } = &item.primitive
            {
                assert!(hint.contains(*origin));
                assert!(
                    origin.x + text.chars().count() as f32 * metrics.cell_width
                        <= hint.max.x + 0.01
                );
                assert!(origin.y + metrics.line_height <= hint.max.y + 0.01);
                assert!(color.a > 0.0);
            }
        }
    }
}

#[test]
fn leave_unmount_remount_and_target_moves_remove_stale_help() {
    let moved = Signal::new(false);
    let show = Signal::new(true);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(400, 160, 1.0));
    let movement = moved.clone();
    let mounted = show.clone();
    runtime.set_root(move |cx: &mut BuildCx| {
        cx.track(&movement);
        cx.track(&mounted);
        if !*mounted.read() {
            return target().build(cx);
        }
        Align::new(if *movement.read() {
            Alignment2D::BOTTOM_RIGHT
        } else {
            Alignment2D::TOP_LEFT
        })
        .child(Tooltip::new("Lifecycle hint").child(target()))
        .build(cx)
    });
    update(&mut runtime);
    pointer(&mut runtime, Point::new(10.0, 10.0), PointerPhase::Move);
    assert!(text(&runtime).contains(&"Lifecycle hint"));
    runtime.dispatch(UiEvent::PointerBoundary(PointerBoundaryEvent {
        pointer_id: 1,
        position: None,
        kind: PointerBoundaryKind::Leave,
    }));
    update(&mut runtime);
    assert!(!text(&runtime).contains(&"Lifecycle hint"));
    pointer(&mut runtime, Point::new(10.0, 10.0), PointerPhase::Move);
    assert!(text(&runtime).contains(&"Lifecycle hint"));
    moved.set(true);
    update(&mut runtime);
    assert!(
        !text(&runtime).contains(&"Lifecycle hint"),
        "hide on geometry commit, not a later pointer event"
    );
    let point = center(red_rect(&runtime));
    pointer(&mut runtime, point, PointerPhase::Move);
    assert!(text(&runtime).contains(&"Lifecycle hint"));
    show.set(false);
    update(&mut runtime);
    assert!(!text(&runtime).contains(&"Lifecycle hint"));
    show.set(true);
    update(&mut runtime);
    assert!(
        !text(&runtime).contains(&"Lifecycle hint"),
        "remount does not inherit hover"
    );
    pointer(&mut runtime, point, PointerPhase::Move);
    assert!(text(&runtime).contains(&"Lifecycle hint"));
    runtime.set_viewport(Viewport::new(90, 64, 1.0));
    update(&mut runtime);
    assert!(
        !text(&runtime).contains(&"Lifecycle hint"),
        "resize moved the target"
    );
}

#[test]
fn single_child_builders_and_macro_replace_clear_and_reject_extra_children() {
    fn single<C: WithChildren<Capability = harbor_widget::SingleChild>>(_: &C) {}
    single(&Tooltip::new("hint"));
    let mut runtime = Runtime::new();
    runtime.set_root(|_: &mut BuildCx| {
        harbor_widget::view! {
            Tooltip::new("Macro help") => { TextLabel::new("arbitrary child"); }
        }
    });
    update(&mut runtime);
    pointer(&mut runtime, Point::new(3.0, 3.0), PointerPhase::Move);
    assert!(text(&runtime).contains(&"arbitrary child"));
    assert!(text(&runtime).contains(&"Macro help"));
    runtime.set_root(
        Tooltip::new("cleared")
            .child(Button::text("old"))
            .with_children(Children::one(TextLabel::new("replacement")))
            .unwrap(),
    );
    update(&mut runtime);
    assert_eq!(text(&runtime), ["replacement"]);
    runtime.set_root(
        Tooltip::new("cleared")
            .child(target())
            .with_children(Children::new())
            .unwrap(),
    );
    update(&mut runtime);
    assert!(items(&runtime).is_empty());
    assert_eq!(
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .layout_rect()
            .unwrap()
            .size(),
        Size::ZERO
    );
    let children = [target(), target()].into_iter().collect::<Children>();
    assert!(Tooltip::new("too many").with_children(children).is_err());
}

#[test]
fn transparent_control_theme_still_produces_opaque_readable_help() {
    let mut theme = harbor_widget::Theme::default();
    theme.button.normal.background = Color::TRANSPARENT;
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(400, 160, 1.0));
    runtime.set_root(
        harbor_widget::ThemeProvider::new(theme)
            .unwrap()
            .child(Tooltip::new("Readable hint").child(target())),
    );
    update(&mut runtime);
    pointer(&mut runtime, Point::new(10.0, 10.0), PointerPhase::Move);
    let colors = harbor_widget::Theme::default().button.normal;
    assert!(
        items(&runtime)
            .into_iter()
            .any(|item| matches!(&item.primitive,
        Primitive::Quad { color, .. } if *color == colors.background && color.a == 1.0))
    );
    assert!(items(&runtime).into_iter().any(|item| matches!(&item.primitive,
        Primitive::Text { text, color, .. } if text.as_ref() == "Readable hint" && *color == colors.foreground)));
}

#[test]
fn empty_text_and_a_window_smaller_than_one_line_do_not_paint_overflow() {
    let mut runtime = Runtime::new();
    runtime.set_root(Tooltip::new("").child(target()));
    update(&mut runtime);
    pointer(&mut runtime, Point::new(3.0, 3.0), PointerPhase::Move);
    assert!(text(&runtime).is_empty());
    runtime.set_viewport(Viewport::new(8, 8, 1.0));
    runtime.set_root(Tooltip::new("Cannot fit").child(target()));
    update(&mut runtime);
    pointer(&mut runtime, Point::new(3.0, 3.0), PointerPhase::Move);
    assert!(text(&runtime).is_empty());
    assert_eq!(items(&runtime).len(), 1, "only the clamped target paints");
}

#[test]
fn cancelling_hover_without_a_press_or_capture_removes_help() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(400, 160, 1.0));
    runtime.set_root(Tooltip::new("Cancelled hover").child(target()));
    update(&mut runtime);
    let point = Point::new(10.0, 10.0);
    pointer(&mut runtime, point, PointerPhase::Move);
    assert!(text(&runtime).contains(&"Cancelled hover"));
    assert!(runtime.input().captor(1).is_none());
    pointer(&mut runtime, point, PointerPhase::Cancel);
    assert!(!text(&runtime).contains(&"Cancelled hover"));
    assert!(runtime.input().hovered().is_none());
    pointer(&mut runtime, point, PointerPhase::Move);
    assert!(text(&runtime).contains(&"Cancelled hover"));
    runtime.cancel_active_input_ownership(point);
    update(&mut runtime);
    assert!(!text(&runtime).contains(&"Cancelled hover"));
    assert!(runtime.input().hovered().is_none());
}
