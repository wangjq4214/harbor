use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::{Point, Size};
use harbor_widget::runtime::Runtime;
use harbor_widget::widgets::button::Button;
use harbor_widget::widgets::focus_scope::FocusScope;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::widgets::stack::Stack;
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

/// Build a Button that sets `flag` to true when its onClick fires.
fn test_button(label: &str, flag: Arc<AtomicBool>) -> Button {
    Button::new(label).on_click(move |_ctx| {
        flag.store(true, Ordering::SeqCst);
    })
}

// ── Pointer click routing ────────────────────────────────────────────────────

#[test]
fn should_fire_onclick_on_pointer_up_after_down() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // Dispatch pointer down inside button bounds (~92x32 at origin)
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    assert!(
        !clicked.load(Ordering::SeqCst),
        "onClick should not fire on Down"
    );

    // Dispatch pointer up — onClick should fire now
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Up,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    assert!(
        clicked.load(Ordering::SeqCst),
        "onClick should fire on Up after Down"
    );
}

#[test]
fn should_not_fire_onclick_when_pointer_down_outside_button() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // Dispatch pointer down far outside button bounds
    rt.dispatch(
        pointer_event(
            Point::new(500.0, 500.0),
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    rt.dispatch(
        pointer_event(
            Point::new(500.0, 500.0),
            PointerPhase::Up,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    assert!(
        !clicked.load(Ordering::SeqCst),
        "onClick should not fire when clicking empty space"
    );
}

// ── Pointer capture ──────────────────────────────────────────────────────────

#[test]
fn should_capture_pointer_on_down_and_release_on_up() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // Down: button captures pointer 0
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    assert!(
        rt.input().captor(0).is_some(),
        "pointer should be captured after Down"
    );

    // Up: button releases and fires onClick
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Up,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    assert!(
        rt.input().captor(0).is_none(),
        "pointer should be released after Up"
    );
    assert!(clicked.load(Ordering::SeqCst));
}

#[test]
fn should_deliver_events_to_captor_regardless_of_position() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // Capture the pointer with Down inside the button
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            1,
        ),
        now(),
    );

    // Now dispatch Up at a position far outside — captor still receives it
    rt.dispatch(
        pointer_event(
            Point::new(500.0, 500.0),
            PointerPhase::Up,
            PointerButton::Left,
            1,
        ),
        now(),
    );
    assert!(
        clicked.load(Ordering::SeqCst),
        "captured button should receive Up even when pointer moved outside"
    );
}

// ── Hit testing: overlapping widgets in Stack ────────────────────────────────

#[test]
fn should_route_pointer_to_topmost_overlapping_widget() {
    let top_clicked = Arc::new(AtomicBool::new(false));
    let bottom_clicked = Arc::new(AtomicBool::new(false));

    let stack = Stack::new()
        .child(test_button("Bottom", bottom_clicked.clone()))
        .child(test_button("Top", top_clicked.clone()));

    let mut rt = Runtime::new();
    rt.set_root(stack);
    rt.update(now());

    // Button "Bottom" 6 chars → ~92x32; "Top" 3 chars → ~62x32
    // Both at origin. Click at (50, 16) hits both; topmost (Top, last child) should win.
    let pointer_id = 0;
    rt.dispatch(
        pointer_event(
            Point::new(50.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            pointer_id,
        ),
        now(),
    );
    rt.dispatch(
        pointer_event(
            Point::new(50.0, 16.0),
            PointerPhase::Up,
            PointerButton::Left,
            pointer_id,
        ),
        now(),
    );

    assert!(
        top_clicked.load(Ordering::SeqCst),
        "topmost overlapping button should fire onClick"
    );
    assert!(
        !bottom_clicked.load(Ordering::SeqCst),
        "underlying button should NOT fire when topmost covers it"
    );
}

#[test]
fn should_route_to_underlying_widget_when_point_outside_topmost_bounds() {
    let top_clicked = Arc::new(AtomicBool::new(false));
    let bottom_clicked = Arc::new(AtomicBool::new(false));

    let stack = Stack::new()
        .child(test_button("Bottom", bottom_clicked.clone()))
        .child(test_button("Top", top_clicked.clone()));

    let mut rt = Runtime::new();
    rt.set_root(stack);
    rt.update(now());

    // Top button is ~62px wide, Bottom is ~92px wide.
    // Click at (80, 16) — outside Top bounds but inside Bottom.
    let pointer_id = 1;
    rt.dispatch(
        pointer_event(
            Point::new(80.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            pointer_id,
        ),
        now(),
    );
    rt.dispatch(
        pointer_event(
            Point::new(80.0, 16.0),
            PointerPhase::Up,
            PointerButton::Left,
            pointer_id,
        ),
        now(),
    );

    assert!(
        bottom_clicked.load(Ordering::SeqCst),
        "underlying button should fire when click is outside topmost bounds"
    );
    assert!(
        !top_clicked.load(Ordering::SeqCst),
        "topmost button should NOT fire when click is outside its bounds"
    );
}

// ── Keyboard activation ──────────────────────────────────────────────────────

#[test]
fn should_activate_button_on_enter_when_focused() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // The root IS the button — set focus on it
    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);

    rt.dispatch(key_down(Key::Enter), now());
    assert!(
        clicked.load(Ordering::SeqCst),
        "Enter key should activate focused button"
    );
}

#[test]
fn should_activate_button_on_space_when_focused() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);

    rt.dispatch(key_down(Key::Space), now());
    assert!(
        clicked.load(Ordering::SeqCst),
        "Space key should activate focused button"
    );
}

#[test]
fn should_not_activate_button_on_key_when_not_focused() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // No focus set — keyboard events have no target
    rt.dispatch(key_down(Key::Enter), now());
    assert!(
        !clicked.load(Ordering::SeqCst),
        "Enter should not activate unfocused button"
    );
}

// ── FocusScope: modal integration via Runtime ────────────────────────────────

/// Helper: build a FocusScope wrapping a Button (for tree integration testing).
fn focus_scope_with_button(modal: bool, button_label: &str, flag: Arc<AtomicBool>) -> FocusScope {
    let btn = Button::new(button_label).on_click(move |_ctx| {
        flag.store(true, Ordering::SeqCst);
    });
    FocusScope::new().modal(modal).child(btn)
}

#[test]
fn should_deliver_pointer_events_inside_non_modal_focus_scope() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(focus_scope_with_button(false, "OK", clicked.clone()));
    rt.update(now());

    // Click inside the FocusScope/Button bounds
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Up,
            PointerButton::Left,
            0,
        ),
        now(),
    );

    assert!(
        clicked.load(Ordering::SeqCst),
        "non-modal FocusScope should not block pointer events to its child"
    );
}

// ── Multiple buttons independence ────────────────────────────────────────────

#[test]
fn should_only_fire_targeted_button_in_stack() {
    let top_clicked = Arc::new(AtomicBool::new(false));
    let bottom_clicked = Arc::new(AtomicBool::new(false));

    let stack = Stack::new()
        .child(test_button("First", bottom_clicked.clone()))
        .child(test_button("Second", top_clicked.clone()));

    let mut rt = Runtime::new();
    rt.set_root(stack);
    rt.update(now());

    // Click inside Second only (~62px) — should only fire Second
    rt.dispatch(
        pointer_event(
            Point::new(40.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            10,
        ),
        now(),
    );
    rt.dispatch(
        pointer_event(
            Point::new(40.0, 16.0),
            PointerPhase::Up,
            PointerButton::Left,
            10,
        ),
        now(),
    );

    assert!(top_clicked.load(Ordering::SeqCst));
    assert!(!bottom_clicked.load(Ordering::SeqCst));
}

// ── Edge cases ───────────────────────────────────────────────────────────────

#[test]
fn should_not_crash_dispatch_with_no_root() {
    let mut rt = Runtime::new();
    // Dispatch before any root is set
    let req = rt.dispatch(
        pointer_event(Point::ZERO, PointerPhase::Down, PointerButton::Left, 0),
        now(),
    );
    assert!(!req.needs_redraw);
}

#[test]
fn should_return_needs_redraw_false_when_no_handler_requests_paint() {
    let mut rt = Runtime::new();
    // Use SizedBox (no event handlers at all) instead of Button
    rt.set_root(SizedBox::new(Size::new(100.0, 32.0)));
    rt.update(now());

    // Dispatch a move event — SizedBox ignores all events
    let req = rt.dispatch(
        pointer_event(
            Point::new(50.0, 16.0),
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    assert!(
        !req.needs_redraw,
        "Move event on SizedBox (ignores all events) should not trigger redraw"
    );
}

#[test]
fn should_honor_pointer_id_isolation() {
    let clicked = Arc::new(AtomicBool::new(false));

    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // Pointer 0: Down captures, no Up
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ),
        now(),
    );

    // Pointer 1: Down+Up in same position → fires onClick (separate capture context)
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            1,
        ),
        now(),
    );
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Up,
            PointerButton::Left,
            1,
        ),
        now(),
    );

    // Pointer 1's onClick fired; pointer 0 is still captured
    assert!(clicked.load(Ordering::SeqCst));
    assert!(rt.input().captor(0).is_some());
    assert!(rt.input().captor(1).is_none());
}

#[test]
fn should_clear_capture_when_set_root_replaces_tree() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("Old", clicked.clone()));
    rt.update(now());

    // Capture pointer on old tree
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            5,
        ),
        now(),
    );
    assert!(rt.input().captor(5).is_some());

    // Replace root — capture cleared since old fiber is gone
    rt.set_root(Button::new("New"));
    rt.update(now());
    assert!(
        rt.input().captor(5).is_none(),
        "capture should be cleared when tree is replaced"
    );
}

// ── Modal FocusScope blocking ───────────────────────────────────────────────

#[test]
fn should_block_keyboard_events_when_modal_is_active_and_nothing_focused() {
    // When a modal FocusScope is active and nothing is focused,
    // keyboard events should not reach non-modal siblings.
    let inside_key = Arc::new(AtomicBool::new(false));
    let outside_key = Arc::new(AtomicBool::new(false));

    let modal_scope = FocusScope::new()
        .modal(true)
        .child(test_button("Inside", inside_key.clone()));
    let outside_button = test_button("Outside", outside_key.clone());

    // Stack both at origin: modal covers everything
    let root = Stack::new().child(outside_button).child(modal_scope);

    let mut rt = Runtime::new();
    rt.set_root(root);
    rt.update(now());

    // Keyboard event with no focused widget —
    // path is [root_id], and modal blocks it
    rt.dispatch(key_down(Key::Enter), now());
    assert!(
        !inside_key.load(Ordering::SeqCst),
        "Enter should not reach inside button when nothing is focused under modal"
    );
    assert!(
        !outside_key.load(Ordering::SeqCst),
        "Enter should not reach outside button when modal is active"
    );
}

#[test]
fn should_detect_modal_in_tree() {
    let mut rt = Runtime::new();
    rt.set_root(Button::new("OK"));
    rt.update(now());
    assert!(!rt.has_modal());

    // Add modal scope
    rt.set_root(FocusScope::new().modal(true).child(Button::new("OK")));
    rt.update(now());
    assert!(rt.has_modal());

    // Non-modal scope
    rt.set_root(FocusScope::new().child(Button::new("OK")));
    rt.update(now());
    assert!(!rt.has_modal());
}

// ── FocusEvent routing ──────────────────────────────────────────────────────

#[test]
fn should_deliver_focus_gained_to_focused_button() {
    let focus_gained = Arc::new(AtomicBool::new(false));

    // Build a button that captures click (for identification)
    let btn = Button::new("OK").on_click(move |_ctx| {
        focus_gained.store(true, Ordering::SeqCst);
    });

    let mut rt = Runtime::new();
    rt.set_root(btn);
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);

    // Deliver FocusEvent::Gained — should not crash
    use harbor_widget::input::event::FocusEvent;
    rt.dispatch(UiEvent::Focus(FocusEvent::Gained), now());
    // Button handles FocusEvent but does not request repaint
}

#[test]
fn should_deliver_focus_lost_to_button() {
    let mut rt = Runtime::new();
    rt.set_root(Button::new("OK"));
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);

    // Deliver FocusEvent::Lost
    use harbor_widget::input::event::FocusEvent;
    rt.dispatch(UiEvent::Focus(FocusEvent::Lost), now());
    // Should not crash; button transitions to Normal
}

// ── Pointer Cancel ──────────────────────────────────────────────────────────

#[test]
fn should_handle_pointer_cancel_and_release_capture() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    // Down: capture pointer 0
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    assert!(rt.input().captor(0).is_some());

    // Cancel: should release capture and NOT fire onClick
    rt.dispatch(
        UiEvent::Pointer(PointerEvent::new(
            Point::new(46.0, 16.0),
            PointerPhase::Cancel,
            PointerButton::Left,
            0,
        )),
        now(),
    );
    assert!(
        rt.input().captor(0).is_none(),
        "pointer should be released on Cancel"
    );
    assert!(
        !clicked.load(Ordering::SeqCst),
        "onClick should not fire on Cancel"
    );
}

// ── Right/Middle click ignored ──────────────────────────────────────────────

#[test]
fn should_fire_onclick_on_right_click_since_button_does_not_filter_by_button_type() {
    let clicked = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new();
    rt.set_root(test_button("OK", clicked.clone()));
    rt.update(now());

    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Down,
            PointerButton::Right,
            0,
        ),
        now(),
    );
    rt.dispatch(
        pointer_event(
            Point::new(46.0, 16.0),
            PointerPhase::Up,
            PointerButton::Right,
            0,
        ),
        now(),
    );
    // Current behavior: Button fires onClick for any button type
    assert!(
        clicked.load(Ordering::SeqCst),
        "Right click currently fires onClick (Button does not filter by button type)"
    );
}

// ── Wheel events ────────────────────────────────────────────────────────────

#[test]
fn should_not_crash_on_wheel_event() {
    let mut rt = Runtime::new();
    rt.set_root(Button::new("OK"));
    rt.update(now());

    let req = rt.dispatch(
        UiEvent::Pointer(PointerEvent::new(
            Point::new(46.0, 16.0),
            PointerPhase::Wheel { dx: 0.0, dy: 10.0 },
            PointerButton::Left,
            0,
        )),
        now(),
    );
    assert!(
        !req.needs_redraw,
        "Wheel event (ignored by Button) should not trigger redraw"
    );
}

// ── Move event routing ──────────────────────────────────────────────────────

#[test]
fn should_route_move_event_to_button_without_crashing() {
    let mut rt = Runtime::new();
    rt.set_root(Button::new("OK"));
    rt.update(now());

    // Move inside the button bounds — button handles it (sets hover state)
    let req = rt.dispatch(
        pointer_event(
            Point::new(50.0, 16.0),
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ),
        now(),
    );
    // Move inside the button bounds — button handles it (sets hover, requests paint)
    assert!(
        req.needs_redraw,
        "Move should request redraw for hover state"
    );
}

// ── Keyboard dispatch with no focused widget ────────────────────────────────

#[test]
fn should_not_crash_on_keyboard_with_no_focused_widget() {
    let mut rt = Runtime::new();
    rt.set_root(Button::new("OK"));
    rt.update(now());

    // No focus set — keyboard events go to root via keyboard path
    let req = rt.dispatch(key_down(Key::Tab), now());
    // Tab on Button (not FocusScope) is ignored
    assert!(!req.needs_redraw);
}

// ── Viewport change triggers re-layout ──────────────────────────────────────

#[test]
fn should_trigger_relayout_on_viewport_change() {
    use harbor_widget::layout::Size;
    use harbor_widget::renderer::Viewport;

    let mut rt = Runtime::new();
    rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
    let req1 = rt.update(now());
    assert!(req1.needs_redraw);

    // Second update without changes
    let req2 = rt.update(now());
    assert!(!req2.needs_redraw);

    // Change viewport — should mark root dirty
    rt.set_viewport(Viewport::new(1024, 768, 1.0));
    let req3 = rt.update(now());
    assert!(
        req3.needs_redraw,
        "viewport change should trigger relayout and redraw"
    );
}

// ── Clear focus programmatically ────────────────────────────────────────────

#[test]
fn should_clear_focus_programmatically() {
    let mut rt = Runtime::new();
    rt.set_root(Button::new("OK"));
    rt.update(now());

    let root_id = rt.root_id().unwrap();
    rt.set_focus(root_id);
    assert!(rt.input().focused.is_some());

    rt.clear_focus();
    assert!(rt.input().focused.is_none());
}

// ── Runtime with no root edge cases ─────────────────────────────────────────

#[test]
fn should_ignore_update_with_no_root() {
    let mut rt = Runtime::new();
    let req = rt.update(now());
    assert!(!req.needs_redraw);
}
