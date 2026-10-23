use harbor_widget::effects::{CursorEffect, CursorShape};
use harbor_widget::input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent};
use harbor_widget::layout::{Point, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::Primitive;
use harbor_widget::view::{Component, SizedBox};
use harbor_widget::{Button, IconButton, InteractiveRegion, LucideIcon, MouseRegion, Notification};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn pointer(position: Point, phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(position, phase, PointerButton::Left, 0))
}

fn assert_arrow(control: impl Component + 'static) {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(240, 120, 1.0));
    runtime.set_root(control);
    runtime.update(Instant::now());
    let effects = runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Move));
    assert_eq!(
        effects.cursor,
        Some(CursorEffect::Set(CursorShape::Default))
    );
    for phase in [PointerPhase::Down, PointerPhase::Up, PointerPhase::Move] {
        let effects = runtime.dispatch(pointer(Point::new(8.0, 8.0), phase));
        assert!(matches!(
            effects.cursor,
            None | Some(CursorEffect::Set(CursorShape::Default))
        ));
        runtime.update(Instant::now());
    }
}

#[test]
fn ordinary_controls_keep_the_arrow_when_enabled_or_disabled() {
    for disabled in [false, true] {
        assert_arrow(Button::text("Command").disabled(disabled));
        assert_arrow(IconButton::new(LucideIcon::X, "Close").disabled(disabled));
        assert_arrow(
            InteractiveRegion::new()
                .disabled(disabled)
                .child(SizedBox::new(Size::new(80.0, 40.0))),
        );
    }
}

#[test]
fn ordinary_controls_override_an_ancestor_specialized_cursor() {
    assert_arrow(
        MouseRegion::new()
            .cursor(CursorShape::Text)
            .child(Button::text("Command")),
    );
}

#[test]
fn explicit_link_text_resize_and_drag_cursors_remain_available() {
    for shape in [
        CursorShape::Pointer,
        CursorShape::Text,
        CursorShape::ResizeHorizontal,
        CursorShape::ResizeVertical,
        CursorShape::Grab,
        CursorShape::Grabbing,
    ] {
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(240, 120, 1.0));
        runtime.set_root(
            InteractiveRegion::new()
                .cursor(shape)
                .child(SizedBox::new(Size::new(80.0, 40.0))),
        );
        runtime.update(Instant::now());
        let effects = runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Move));
        assert_eq!(effects.cursor, Some(CursorEffect::Set(shape)));
        let effects = runtime.dispatch(pointer(Point::new(500.0, 500.0), PointerPhase::Move));
        assert_eq!(effects.cursor, Some(CursorEffect::Reset));
    }
}

#[test]
fn notification_close_keeps_the_arrow_and_still_dismisses_without_focus_theft() {
    let dismissed = Arc::new(AtomicUsize::new(0));
    let observed = dismissed.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(240, 120, 1.0));
    runtime.set_root(Notification::new("Notice", move || {
        observed.fetch_add(1, Ordering::SeqCst);
    }));
    runtime.update(Instant::now());
    let close_rect = runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::Icon {
                icon: LucideIcon::X,
                rect,
                ..
            } => Some(rect),
            _ => None,
        })
        .expect("notification close icon");
    let close = Point::new(
        (close_rect.min.x + close_rect.max.x) / 2.0,
        (close_rect.min.y + close_rect.max.y) / 2.0,
    );
    let effects = runtime.dispatch(pointer(close, PointerPhase::Move));
    assert_eq!(
        effects.cursor,
        Some(CursorEffect::Set(CursorShape::Default))
    );
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        let effects = runtime.dispatch(pointer(close, phase));
        assert!(matches!(
            effects.cursor,
            None | Some(CursorEffect::Set(CursorShape::Default))
        ));
        runtime.update(Instant::now());
    }
    assert_eq!(dismissed.load(Ordering::SeqCst), 1);
    assert!(runtime.input().focused().is_none());
    assert!(runtime.input().captor(0).is_none());
}
