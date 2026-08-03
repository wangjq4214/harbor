#![cfg(feature = "winit")]

use harbor_widget::input::event::{
    Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::Point;
use harbor_widget::runtime::Runtime;
use harbor_widget::widgets::button::Button;
use harbor_widget::widgets::custom_paint::CustomPaint;
use harbor_widget::widgets::preview_pane::PreviewPane;
use harbor_widget::winit::WinitAdapter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
use winit::keyboard::{Key, ModifiersState};

fn custom_paint_runtime(draw_id: u64) -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_root(CustomPaint::new(draw_id));
    runtime.update(Instant::now());
    assert!(runtime.focus_first_focusable());
    runtime.drain_external_input();
    runtime
}

#[test]
fn should_route_wheel_invalidation_as_a_handled_redraw_effect() {
    // Arrange
    let offset = Arc::new(AtomicUsize::new(2));
    let mut runtime = Runtime::new();
    runtime.set_root(PreviewPane::new(
        (0..20).map(|line| line.to_string()).collect(),
        offset.clone(),
        20.0,
        10,
    ));
    runtime.update(Instant::now());
    let mut adapter = WinitAdapter::new();

    // Act
    let outcome = adapter.handle_event(
        &mut runtime,
        &WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, 1.0),
            phase: winit::event::TouchPhase::Moved,
        },
    );

    // Assert
    assert!(outcome.is_handled());
    assert!(outcome.effects.request_redraw);
    assert_eq!(offset.load(std::sync::atomic::Ordering::Relaxed), 3);
}

#[test]
fn should_quarantine_stale_release_until_a_fresh_pointer_down() {
    // Arrange
    let clicked = Arc::new(AtomicBool::new(false));
    let clicked_clone = Arc::clone(&clicked);
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK").on_click(move |_| {
        clicked_clone.store(true, Ordering::SeqCst);
    }));
    runtime.update(Instant::now());
    let mut adapter = WinitAdapter::new();
    let cursor = WindowEvent::CursorMoved {
        device_id: winit::event::DeviceId::dummy(),
        position: PhysicalPosition::new(4.0, 4.0),
    };
    let down = WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state: ElementState::Pressed,
        button: MouseButton::Left,
    };
    let up = WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state: ElementState::Released,
        button: MouseButton::Left,
    };

    // Act: cancel the capture, then offer the stale release.
    adapter.handle_event(&mut runtime, &cursor);
    adapter.handle_event(&mut runtime, &down);
    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
    let stale_up = adapter.handle_event(&mut runtime, &up);

    // Assert: stale release is handled without activating the button.
    assert!(stale_up.is_handled());
    assert!(!stale_up.effects.request_redraw);
    assert!(!clicked.load(Ordering::SeqCst));

    // A fresh down starts a new capture and restores normal click behavior.
    adapter.handle_event(&mut runtime, &cursor);
    adapter.handle_event(&mut runtime, &down);
    adapter.handle_event(&mut runtime, &up);
    assert!(clicked.load(Ordering::SeqCst));
}

#[test]
fn stale_release_for_canceled_mouse_button_cannot_end_fresh_capture() {
    let clicked = Arc::new(AtomicBool::new(false));
    let clicked_clone = Arc::clone(&clicked);
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK").on_click(move |_| {
        clicked_clone.store(true, Ordering::SeqCst);
    }));
    runtime.update(Instant::now());
    let mut adapter = WinitAdapter::new();
    let cursor = WindowEvent::CursorMoved {
        device_id: winit::event::DeviceId::dummy(),
        position: PhysicalPosition::new(4.0, 4.0),
    };
    let mouse_input = |state, button| WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state,
        button,
    };

    adapter.handle_event(&mut runtime, &cursor);
    adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Pressed, MouseButton::Left),
    );
    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));

    // A new right-button capture must not make the canceled left-button
    // release eligible for the shared widget pointer capture.
    adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Pressed, MouseButton::Right),
    );
    let stale_left = adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Released, MouseButton::Left),
    );
    assert!(stale_left.is_handled());
    assert!(!clicked.load(Ordering::SeqCst));

    adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Released, MouseButton::Right),
    );
    assert!(clicked.load(Ordering::SeqCst));
}

#[test]
fn unsupported_mouse_release_stays_unhandled_during_quarantine() {
    let mut adapter = WinitAdapter::new();
    let mut runtime = Runtime::new();

    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
    let outcome = adapter.handle_event(
        &mut runtime,
        &WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Released,
            button: MouseButton::Other(8),
        },
    );

    assert!(!outcome.is_handled());
    assert!(outcome.effects.is_noop());
}

#[test]
fn should_not_request_redraw_when_wheel_is_clamped_at_scroll_boundary() {
    // Arrange
    let offset = Arc::new(AtomicUsize::new(0));
    let mut runtime = Runtime::new();
    runtime.set_root(PreviewPane::new(
        vec!["only line".to_string()],
        offset.clone(),
        20.0,
        10,
    ));
    runtime.update(Instant::now());
    let mut adapter = WinitAdapter::new();

    // Act
    let outcome = adapter.handle_event(
        &mut runtime,
        &WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, -1.0),
            phase: winit::event::TouchPhase::Moved,
        },
    );

    // Assert
    assert!(outcome.is_handled());
    assert!(!outcome.effects.request_redraw);
    assert_eq!(offset.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn should_keep_window_scale_modifiers_and_external_input_isolated() {
    // Arrange
    let mut first_runtime = custom_paint_runtime(101);
    let mut second_runtime = custom_paint_runtime(202);
    let mut first_adapter = WinitAdapter::new();
    let mut second_adapter = WinitAdapter::new();
    first_adapter.set_scale_factor(2.0);
    first_adapter.handle_event(
        &mut first_runtime,
        &WindowEvent::ModifiersChanged(ModifiersState::SHIFT.into()),
    );

    // Act
    for (adapter, runtime) in [
        (&mut first_adapter, &mut first_runtime),
        (&mut second_adapter, &mut second_runtime),
    ] {
        adapter.handle_event(
            runtime,
            &WindowEvent::CursorMoved {
                device_id: winit::event::DeviceId::dummy(),
                position: PhysicalPosition::new(20.0, 10.0),
            },
        );
        adapter.handle_keyboard_input(
            runtime,
            &Key::Character("x".into()),
            ElementState::Pressed,
            winit::keyboard::KeyLocation::Standard,
        );
    }

    // Assert
    assert_eq!(
        first_runtime.drain_external_input(),
        vec![
            (
                101,
                UiEvent::Pointer(PointerEvent::new(
                    Point::new(10.0, 5.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    0,
                )),
            ),
            (
                101,
                UiEvent::Keyboard(KeyboardEvent::KeyDown {
                    key: WidgetKey::Character('x'),
                    modifiers: Modifiers {
                        shift: true,
                        ..Modifiers::default()
                    },
                }),
            ),
        ]
    );
    assert_eq!(
        second_runtime.drain_external_input(),
        vec![
            (
                202,
                UiEvent::Pointer(PointerEvent::new(
                    Point::new(20.0, 10.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    0,
                )),
            ),
            (
                202,
                UiEvent::Keyboard(KeyboardEvent::KeyDown {
                    key: WidgetKey::Character('x'),
                    modifiers: Modifiers::default(),
                }),
            ),
        ]
    );
}

#[test]
fn should_reset_modifiers_for_keyboard_input_after_focus_loss() {
    // Arrange
    let mut runtime = custom_paint_runtime(303);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
    );
    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
    runtime.drain_external_input();

    // Act
    let outcome = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("x".into()),
        ElementState::Pressed,
        winit::keyboard::KeyLocation::Standard,
    );

    // Assert
    assert!(outcome.is_handled());
    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            303,
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: WidgetKey::Character('x'),
                modifiers: Modifiers::default(),
            }),
        ),]
    );
}

#[test]
fn should_quarantine_stale_touch_end_until_a_fresh_touch_down() {
    // Arrange
    let clicks = Arc::new(AtomicUsize::new(0));
    let clicks_clone = Arc::clone(&clicks);
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK").on_click(move |_| {
        clicks_clone.fetch_add(1, Ordering::SeqCst);
    }));
    runtime.update(Instant::now());
    let mut adapter = WinitAdapter::new();
    let touch = |phase, id| {
        WindowEvent::Touch(Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(4.0, 4.0),
            force: None,
            id,
        })
    };

    // Act: focus loss cancels the active contact; its end is stale.
    adapter.handle_event(&mut runtime, &touch(TouchPhase::Started, 41));
    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
    let stale_end = adapter.handle_event(&mut runtime, &touch(TouchPhase::Ended, 41));

    // Assert: the stale end is consumed without activation.
    assert!(stale_end.is_handled());
    assert_eq!(clicks.load(Ordering::SeqCst), 0);

    // A new Down exits quarantine and permits a complete new click.
    adapter.handle_event(&mut runtime, &touch(TouchPhase::Started, 42));
    adapter.handle_event(&mut runtime, &touch(TouchPhase::Ended, 42));
    assert_eq!(clicks.load(Ordering::SeqCst), 1);
}

#[test]
fn should_request_redraw_when_programmatic_focus_changes() {
    // Arrange
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK"));
    runtime.update(Instant::now());
    let root_id = runtime.root_id().expect("button root");

    // Act
    runtime.set_focus(root_id);
    let gained = runtime.update(Instant::now());
    runtime.clear_focus();
    let lost = runtime.update(Instant::now());

    // Assert
    assert_eq!(runtime.input().focused, None);
    assert!(gained.request_redraw);
    assert!(lost.request_redraw);
}

#[test]
fn programmatic_focus_events_and_effects_stay_with_the_owning_runtime() {
    let mut owner = custom_paint_runtime(601);
    let other = custom_paint_runtime(602);
    let owner_id = owner.root_id().expect("owner root");

    owner.clear_focus();
    assert!(owner.take_pending_effects().request_redraw);
    assert!(
        owner
            .drain_external_input()
            .iter()
            .any(|(_, event)| matches!(
                event,
                UiEvent::Focus(harbor_widget::input::event::FocusEvent::Lost)
            ))
    );
    assert!(other.drain_external_input().is_empty());

    owner.set_focus(owner_id);
    assert!(owner.take_pending_effects().request_redraw);
    assert!(
        owner
            .drain_external_input()
            .iter()
            .any(|(_, event)| matches!(
                event,
                UiEvent::Focus(harbor_widget::input::event::FocusEvent::Gained)
            ))
    );
    assert!(other.drain_external_input().is_empty());
}

#[test]
fn mixed_mouse_and_touch_quarantine_is_scoped_to_each_pointer_source() {
    let touch = |phase, id| {
        WindowEvent::Touch(Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(4.0, 4.0),
            force: None,
            id,
        })
    };

    // A fresh mouse press must not release a quarantined touch contact.
    let mut touch_runtime = custom_paint_runtime(603);
    let mut touch_adapter = WinitAdapter::new();
    touch_adapter.handle_event(&mut touch_runtime, &touch(TouchPhase::Started, 41));
    touch_runtime.drain_external_input();
    touch_adapter.handle_event(&mut touch_runtime, &WindowEvent::Focused(false));
    touch_runtime.drain_external_input();
    touch_adapter.handle_event(
        &mut touch_runtime,
        &WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        },
    );
    touch_runtime.drain_external_input();
    touch_adapter.handle_event(&mut touch_runtime, &touch(TouchPhase::Ended, 41));
    assert!(touch_runtime.drain_external_input().is_empty());
    touch_adapter.handle_event(&mut touch_runtime, &touch(TouchPhase::Cancelled, 41));
    assert!(touch_runtime.drain_external_input().is_empty());

    // A fresh contact from the same source exits only that source's
    // quarantine and retains the normal Down -> Up behavior.
    touch_adapter.handle_event(&mut touch_runtime, &touch(TouchPhase::Started, 41));
    touch_runtime.drain_external_input();
    touch_adapter.handle_event(&mut touch_runtime, &touch(TouchPhase::Ended, 41));
    assert!(!touch_runtime.drain_external_input().is_empty());

    // A fresh contact must not release a quarantined mouse button. The fresh
    // touch still receives its complete Down -> Up sequence.
    let mut mouse_runtime = custom_paint_runtime(604);
    let mut mouse_adapter = WinitAdapter::new();
    mouse_adapter.handle_event(
        &mut mouse_runtime,
        &WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        },
    );
    mouse_adapter.handle_event(&mut mouse_runtime, &touch(TouchPhase::Started, 42));
    mouse_runtime.drain_external_input();
    mouse_adapter.handle_event(&mut mouse_runtime, &WindowEvent::Focused(false));
    mouse_runtime.drain_external_input();

    mouse_adapter.handle_event(&mut mouse_runtime, &touch(TouchPhase::Started, 43));
    mouse_runtime.drain_external_input();
    mouse_adapter.handle_event(
        &mut mouse_runtime,
        &WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Released,
            button: MouseButton::Left,
        },
    );
    assert!(mouse_runtime.drain_external_input().is_empty());

    mouse_adapter.handle_event(&mut mouse_runtime, &touch(TouchPhase::Ended, 43));
    let fresh_touch_events = mouse_runtime.drain_external_input();
    assert!(fresh_touch_events.iter().any(|(_, event)| matches!(
        event,
        UiEvent::Pointer(pointer) if pointer.phase == PointerPhase::Up
    )));
    mouse_adapter.handle_event(&mut mouse_runtime, &touch(TouchPhase::Ended, 42));
    assert!(mouse_runtime.drain_external_input().is_empty());
    mouse_adapter.handle_event(&mut mouse_runtime, &touch(TouchPhase::Cancelled, 42));
    assert!(mouse_runtime.drain_external_input().is_empty());
}

#[test]
fn should_consume_programmatic_focus_redraw_effect_once_at_runtime_boundary() {
    // Arrange: focus changes are made while the runtime is otherwise idle.
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK"));
    runtime.update(Instant::now());
    let root_id = runtime.root_id().expect("button root");

    // Act: the host consumes the immediate pending effect, then the runtime
    // update consumes any dirty-tree work without duplicating that effect.
    runtime.set_focus(root_id);
    let host_effects = runtime.take_pending_effects();
    let update_effects = runtime.update(Instant::now());
    let idle_effects = runtime.update(Instant::now());

    // Assert: the owning host sees the redraw request, and no later idle turn
    // repeats it merely because the programmatic focus event was delivered.
    assert!(host_effects.request_redraw);
    assert!(update_effects.request_redraw);
    assert!(!idle_effects.request_redraw);
}

#[test]
fn should_quarantine_each_supported_mouse_release_without_claiming_unsupported_buttons() {
    // Arrange: both supported buttons are held when the window loses focus.
    let mut runtime = custom_paint_runtime(605);
    let mut adapter = WinitAdapter::new();
    let mouse_input = |state, button| WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state,
        button,
    };
    adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Pressed, MouseButton::Left),
    );
    adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Pressed, MouseButton::Right),
    );
    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
    runtime.drain_external_input();

    // Act: an unsupported release remains outside the adapter's quarantine,
    // while each supported stale release is consumed independently.
    let unsupported = adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Released, MouseButton::Other(99)),
    );
    let stale_left = adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Released, MouseButton::Left),
    );
    let stale_right = adapter.handle_event(
        &mut runtime,
        &mouse_input(ElementState::Released, MouseButton::Right),
    );

    // Assert: quarantine is per supported button and does not turn an
    // otherwise unsupported native event into a handled event.
    assert!(!unsupported.is_handled());
    assert!(unsupported.effects.is_noop());
    assert!(stale_left.is_handled());
    assert!(stale_left.effects.is_noop());
    assert!(stale_right.is_handled());
    assert!(stale_right.effects.is_noop());
    assert!(runtime.drain_external_input().is_empty());
}

fn focused_button_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK"));
    runtime.update(Instant::now());
    assert!(runtime.focus_first_focusable());
    assert!(runtime.take_pending_effects().request_redraw);
    runtime.update(Instant::now());
    runtime
}

#[test]
fn main_and_confirmation_focus_redraw_effects_are_owned_by_their_runtimes() {
    // Arrange: model the main and confirmation windows with independent
    // Runtime/adapter pairs, including their initial focus lifecycle.
    let mut main_runtime = focused_button_runtime();
    let mut confirmation_runtime = focused_button_runtime();
    let mut main_adapter = WinitAdapter::new();
    let mut confirmation_adapter = WinitAdapter::new();

    // Act: deliver focus loss to each native-window boundary independently.
    let main_loss = main_adapter.handle_event(&mut main_runtime, &WindowEvent::Focused(false));
    let confirmation_loss =
        confirmation_adapter.handle_event(&mut confirmation_runtime, &WindowEvent::Focused(false));

    // Assert: each focus transition produces a redraw for its owner only.
    assert!(main_loss.is_handled());
    assert!(main_loss.effects.request_redraw);
    assert!(confirmation_loss.is_handled());
    assert!(confirmation_loss.effects.request_redraw);
    assert_eq!(main_runtime.input().focused, main_runtime.root_id());
    assert_eq!(
        confirmation_runtime.input().focused,
        confirmation_runtime.root_id()
    );

    let main_update = main_runtime.update(Instant::now());
    let confirmation_update = confirmation_runtime.update(Instant::now());
    assert!(main_update.request_redraw);
    assert!(confirmation_update.request_redraw);
    assert!(!main_runtime.update(Instant::now()).request_redraw);
    assert!(!confirmation_runtime.update(Instant::now()).request_redraw);
}

#[test]
fn lifecycle_update_consumes_focus_effects_before_the_next_encode_turn() {
    // Arrange: both initial layout and focus setup have completed.
    let mut runtime = focused_button_runtime();
    let root_id = runtime.root_id().expect("button root");

    // Act: clear focus, consume the immediate host effect, then run the
    // update turn that precedes encoding the next frame.
    runtime.clear_focus();
    let host_effects = runtime.take_pending_effects();
    let update_effects = runtime.update(Instant::now());
    let idle_effects = runtime.update(Instant::now());

    // Assert: the lifecycle boundary preserves the transition once, then
    // returns to idle; restoring focus repeats the same owner-local handoff.
    assert!(host_effects.request_redraw);
    assert!(update_effects.request_redraw);
    assert!(!idle_effects.request_redraw);
    runtime.set_focus(root_id);
    assert!(runtime.take_pending_effects().request_redraw);
    assert!(runtime.update(Instant::now()).request_redraw);
    assert!(!runtime.update(Instant::now()).request_redraw);
}
