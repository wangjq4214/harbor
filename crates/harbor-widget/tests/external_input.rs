//! Integration tests for external input deferral through the public
//! Runtime API (dispatch → drain_external_input) and CustomPaint widget.

use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::input::event_ctx::EventHandled;
use harbor_widget::layout::{Point, Size};
use harbor_widget::runtime::Runtime;
use harbor_widget::widgets::custom_paint::CustomPaint;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn now() -> Instant {
    Instant::now()
}

fn pointer_event(
    position: Point,
    phase: PointerPhase,
    button: PointerButton,
    pointer_id: u64,
) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(position, phase, button, pointer_id))
}

fn key_down(key: Key) -> UiEvent {
    UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
    })
}

// ── External input queue (unit-level via public trampoline) ─────────────────

#[test]
fn should_queue_and_drain_external_input_round_trip() {
    // Arrange: CustomPaint queues only while its owning Runtime is dispatching.
    let event = key_down(Key::Enter);
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(42));
    rt.update(now());
    assert!(rt.focus_first_focusable());
    rt.drain_external_input();
    rt.dispatch(event.clone());

    // Act: drain the owning Runtime
    let drained = rt.drain_external_input();

    // Assert: exactly one event with correct id and payload
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, 42);
    assert_eq!(drained[0].1, event);
}

#[test]
fn should_drain_clears_queue() {
    // Arrange: queue two events during one Runtime dispatch sequence.
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(1));
    rt.update(now());
    assert!(rt.focus_first_focusable());
    rt.drain_external_input();
    rt.dispatch(key_down(Key::Tab));
    rt.dispatch(key_down(Key::Space));

    // Act: first drain
    let first = rt.drain_external_input();
    assert_eq!(first.len(), 2);

    // Act: second drain should be empty
    let second = rt.drain_external_input();
    assert!(second.is_empty(), "drain should clear the queue");
}

#[test]
fn should_support_multiple_draw_ids() {
    // Arrange: two Runtime instances queue independently on the same thread.
    let mut first = Runtime::new();
    first.set_root(CustomPaint::new(10));
    first.update(now());
    assert!(first.focus_first_focusable());
    first.drain_external_input();
    first.dispatch(key_down(Key::ArrowUp));
    first.dispatch(key_down(Key::Enter));

    let mut second = Runtime::new();
    second.set_root(CustomPaint::new(20));
    second.update(now());
    assert!(second.focus_first_focusable());
    second.drain_external_input();
    second.dispatch(key_down(Key::ArrowDown));

    // Assert: neither Runtime can drain the other's events.
    let first_events = first.drain_external_input();
    let second_events = second.drain_external_input();
    assert_eq!(first_events.len(), 2);
    assert_eq!(first_events[0].0, 10);
    assert_eq!(first_events[1].0, 10);
    assert_eq!(second_events.len(), 1);
    assert_eq!(second_events[0].0, 20);
}

#[test]
fn should_drain_empty_when_no_events_queued() {
    let rt = Runtime::new();
    let drained = rt.drain_external_input();
    assert!(drained.is_empty());
}

#[test]
fn should_support_pointer_events() {
    let pe = pointer_event(
        Point::new(100.0, 200.0),
        PointerPhase::Down,
        PointerButton::Left,
        7,
    );
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(99));
    rt.update(now());
    assert!(rt.focus_first_focusable());
    rt.drain_external_input();
    rt.dispatch(pe.clone());
    let drained = rt.drain_external_input();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, 99);
    assert_eq!(drained[0].1, pe);
}

// ── CustomPaint → external input integration (via Runtime::dispatch) ─────────

#[test]
fn should_queue_event_when_custom_paint_dispatches_through_runtime() {
    // Arrange: Runtime with CustomPaint root, focused to receive keyboard events
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(77));
    rt.update(now());
    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);
    rt.drain_external_input();

    // Act: dispatch a keyboard event through runtime (CustomPaint handles it,
    // queues to external input trampoline)
    let event = key_down(Key::Escape);
    rt.dispatch(event.clone());

    // Drain
    let drained = rt.drain_external_input();

    // Assert: event was queued with correct draw_id
    assert_eq!(
        drained.len(),
        1,
        "CustomPaint should queue its event via dispatch"
    );
    assert_eq!(drained[0].0, 77);
    assert_eq!(drained[0].1, event);
}

#[test]
fn should_focus_custom_paint_at_startup_and_after_pointer_down() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(12));
    rt.update(now());
    let root_id = rt.root_id().unwrap();

    assert!(rt.focus_first_focusable());
    assert_eq!(rt.input().focused(), Some(root_id));
    rt.drain_external_input();
    rt.dispatch(key_down(Key::Enter));
    assert_eq!(rt.drain_external_input().len(), 1);

    rt.clear_focus();
    rt.dispatch(pointer_event(
        Point::new(400.0, 300.0),
        PointerPhase::Down,
        PointerButton::Left,
        0,
    ));
    assert_eq!(rt.input().focused(), Some(root_id));
    rt.drain_external_input();

    rt.dispatch(key_down(Key::Escape));
    let drained = rt.drain_external_input();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, 12);
}

#[test]
fn should_queue_multiple_custom_paint_events_across_dispatch() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(5));
    rt.update(now());
    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);
    rt.drain_external_input();

    // Act: dispatch events via Runtime (CustomPaint queues them)
    rt.dispatch(key_down(Key::Enter));
    rt.dispatch(key_down(Key::ArrowDown));
    rt.dispatch(key_down(Key::ArrowUp));

    // Drain
    let drained = rt.drain_external_input();

    // Assert: all three events queued with draw_id=5
    assert_eq!(drained.len(), 3);
    for (id, event) in &drained {
        assert_eq!(*id, 5);
        assert!(
            matches!(event, UiEvent::Keyboard(_)),
            "all events should be keyboard events"
        );
    }
}

#[test]
fn should_queue_pointer_events_from_custom_paint() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(3));
    rt.update(now());

    // Dispatch pointer down + up
    rt.dispatch(pointer_event(
        Point::new(400.0, 300.0),
        PointerPhase::Down,
        PointerButton::Left,
        0,
    ));
    rt.drain_external_input();
    rt.dispatch(pointer_event(
        Point::new(400.0, 300.0),
        PointerPhase::Up,
        PointerButton::Left,
        0,
    ));

    let drained = rt.drain_external_input();
    assert_eq!(drained.len(), 1);
    for (id, _event) in &drained {
        assert_eq!(*id, 3);
    }
}

#[test]
fn should_not_queue_when_no_custom_paint_in_tree() {
    // Arrange: Runtime with SizedBox (no CustomPaint, no external input handlers)
    use harbor_widget::widgets::sized_box::SizedBox;
    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
    rt.update(now());

    // Act: dispatch events
    rt.dispatch(key_down(Key::Enter));
    rt.dispatch(pointer_event(
        Point::new(50.0, 25.0),
        PointerPhase::Down,
        PointerButton::Left,
        0,
    ));

    // Assert: nothing queued (SizedBox doesn't implement external input)
    let drained = rt.drain_external_input();
    assert!(
        drained.is_empty(),
        "no external input should be queued without CustomPaint"
    );
}

#[test]
fn should_drain_empty_between_dispatches_when_already_drained() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(1));
    rt.update(now());
    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);
    rt.drain_external_input();

    // First dispatch
    rt.dispatch(key_down(Key::Tab));
    let first = rt.drain_external_input();
    assert_eq!(first.len(), 1);

    // No new events dispatched since last drain
    let second = rt.drain_external_input();
    assert!(
        second.is_empty(),
        "drain should be empty when no new events occurred"
    );
}

#[test]
fn should_queue_wheel_events_from_custom_paint() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(8));
    rt.update(now());

    let wheel = UiEvent::Pointer(PointerEvent::new(
        Point::ZERO,
        PointerPhase::WheelLine { dx: 0.0, dy: 10.0 },
        PointerButton::Left,
        0,
    ));
    rt.dispatch(wheel.clone());

    let drained = rt.drain_external_input();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, 8);
    assert_eq!(drained[0].1, wheel);
}

#[test]
fn should_queue_focus_events_from_custom_paint() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(2));
    rt.update(now());
    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);
    rt.drain_external_input();

    let focus = UiEvent::Focus(harbor_widget::input::event::FocusEvent::Gained);
    rt.dispatch(focus.clone());

    let drained = rt.drain_external_input();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, 2);
    assert_eq!(drained[0].1, focus);
}

#[test]
fn should_queue_move_events_from_custom_paint() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(9));
    rt.update(now());

    let move_evt = pointer_event(
        Point::new(300.0, 200.0),
        PointerPhase::Move,
        PointerButton::Left,
        1,
    );
    rt.dispatch(move_evt);

    let drained = rt.drain_external_input();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, 9);
}

// ── CustomPaint public API tests ────────────────────────────────────────────

#[test]
fn should_clone_and_maintain_draw_id() {
    let cp1 = CustomPaint::new(99);
    let cp2 = cp1.clone();
    assert_eq!(cp2.draw_id, 99);
    assert_eq!(cp2.draw_id, cp1.draw_id);
}

#[test]
fn should_support_child_widgets_in_runtime() {
    use harbor_widget::widgets::sized_box::SizedBox;
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(7).child(SizedBox::new(Size::new(50.0, 30.0))));
    rt.update(now());

    // Verify the tree built without panicking and has expected structure
    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let root_fiber = arena.get(root_id).unwrap();

    // CustomPaint with one child
    assert_eq!(
        root_fiber.children().len(),
        1,
        "CustomPaint should have one child fiber"
    );

    // Child should have layout_rect
    let child = arena.get(root_fiber.children()[0]).unwrap();
    assert!(
        child.layout_rect().is_some(),
        "child should have a layout rect"
    );
}

#[test]
fn should_handle_custom_paint_without_children() {
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(1));
    let req = rt.update(now());

    assert!(req.request_redraw, "first update should request redraw");

    let root_id = rt.root_id().unwrap();
    let arena = rt.arena();
    let fiber = arena.get(root_id).unwrap();
    assert!(
        fiber.layout_rect().is_some(),
        "CustomPaint should have a layout rect"
    );
}

#[test]
fn should_only_queue_for_focused_custom_paint() {
    // When a CustomPaint is focused, keyboard events target it directly
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(11));
    rt.update(now());

    // Set focus on the root (which is CustomPaint)
    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);
    rt.drain_external_input();

    // Keyboard event: should route to focused CustomPaint, which queues
    rt.dispatch(key_down(Key::Space));

    let drained = rt.drain_external_input();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].0, 11);
}

#[test]
fn should_leave_external_queue_empty_when_on_input_is_set() {
    // Arrange: adapter consumes the event in-tree; nothing should be deferred.
    let called = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&called);
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(55).on_input(Arc::new(move |_event, _ctx| {
        flag.store(true, Ordering::Relaxed);
        EventHandled::Handled
    })));
    rt.update(now());
    assert!(rt.focus_first_focusable());
    rt.drain_external_input();

    // Act
    rt.dispatch(key_down(Key::Enter));
    let drained = rt.drain_external_input();

    // Assert
    assert!(called.load(Ordering::Relaxed));
    assert!(
        drained.is_empty(),
        "on_input must bypass queue_external_input"
    );
}

#[test]
fn should_request_focus_on_pointer_down_when_on_input_is_set() {
    // Arrange
    let mut rt = Runtime::new();
    rt.set_root(CustomPaint::new(12).on_input(Arc::new(|_event, _ctx| EventHandled::Handled)));
    rt.update(now());
    let root_id = rt.root_id().unwrap();
    rt.clear_focus();
    assert_eq!(rt.input().focused(), None);

    // Act
    rt.dispatch(pointer_event(
        Point::new(400.0, 300.0),
        PointerPhase::Down,
        PointerButton::Left,
        0,
    ));

    // Assert: focus request still happens before the adapter runs.
    assert_eq!(rt.input().focused(), Some(root_id));
    assert!(rt.drain_external_input().is_empty());
}
