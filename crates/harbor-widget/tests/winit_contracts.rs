#![cfg(feature = "winit")]

use harbor_widget::effects::RuntimeEffects;
use harbor_widget::input::event::{
    KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::runtime::Runtime;
use harbor_widget::widgets::button::Button;
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::custom_paint::CustomPaint;
use harbor_widget::winit::{
    FrameError, FrameOutcome, WinitAdapter, WinitEventOutcome, WinitFrameTarget,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use winit::dpi::PhysicalPosition;
use winit::event::{
    ElementState, Ime, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
use winit::keyboard::{Key, KeyLocation, ModifiersState};
use winit::window::Window;

// This fixture is type-checked without constructing an OS window or GPU
// surface. In particular, the target owns no host resources.
fn borrowed_frame_contract<'frame, 'surface>(
    window: &'frame Window,
    surface: &'frame wgpu::Surface<'surface>,
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    configure: &'frame mut dyn FnMut(u32, u32),
    event: &WindowEvent,
) {
    let target = WinitFrameTarget::new(
        window,
        surface,
        device,
        queue,
        configure,
        false,
        wgpu::CompositeAlphaMode::Opaque,
    );
    let _ = target.window();
    let _ = target.surface();
    let _ = target.device();
    let _ = target.queue();
    assert!(!target.backdrop_available());
    assert_eq!(target.alpha_mode(), wgpu::CompositeAlphaMode::Opaque);

    let mut runtime = Runtime::new();
    let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
    let outcome: WinitEventOutcome = adapter.handle_event(&mut runtime, event);
    assert!(outcome.handled);
    assert!(outcome.effects.is_noop());
    let outcome = adapter.render(&mut runtime, target);
    assert!(outcome.effects().is_noop());
}

#[test]
fn should_update_confirmation_viewport_when_resized_event_includes_current_size() {
    // Arrange
    let mut runtime = Runtime::new();
    let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
    let event = WindowEvent::Resized(winit::dpi::PhysicalSize::new(640, 480));

    // Act
    let outcome = adapter.handle_event_with_size(&mut runtime, &event, Some((640, 480)));

    // Assert
    assert!(outcome.handled);
    assert_eq!(
        runtime
            .current_viewport()
            .map(|viewport| viewport.physical_size),
        Some((640, 480))
    );
    assert_eq!(adapter.viewport().physical_size, (640, 480));
}

#[test]
fn should_keep_main_viewport_drawable_when_confirmation_is_resized_to_zero() {
    // Arrange
    let mut main_runtime = Runtime::new();
    let mut confirmation_runtime = Runtime::new();
    let main_adapter = WinitAdapter::with_surface(800, 600, 1.0);
    let mut confirmation_adapter = WinitAdapter::with_surface(600, 500, 1.0);
    main_runtime.set_viewport(main_adapter.viewport().clone());
    confirmation_runtime.set_viewport(confirmation_adapter.viewport().clone());
    let minimized = WindowEvent::Resized(winit::dpi::PhysicalSize::new(0, 0));

    // Act
    let outcome = confirmation_adapter.handle_event_with_size(
        &mut confirmation_runtime,
        &minimized,
        Some((0, 0)),
    );

    // Assert
    assert!(outcome.is_handled());
    assert!(!outcome.effects.request_redraw);
    assert_eq!(
        confirmation_runtime
            .current_viewport()
            .map(|viewport| viewport.physical_size),
        Some((0, 0))
    );
    assert_eq!(confirmation_adapter.viewport().physical_size, (0, 0));
    assert_eq!(
        main_runtime
            .current_viewport()
            .map(|viewport| viewport.physical_size),
        Some((800, 600))
    );
    assert_eq!(main_adapter.viewport().physical_size, (800, 600));
}

#[test]
fn adapters_have_independent_state() {
    let first = WinitAdapter::new();
    let second = WinitAdapter::new();
    assert_ne!(&first as *const _, &second as *const _);
}

#[test]
fn frame_outcomes_classify_variants_and_preserve_effects() {
    // Arrange: one host-visible effect batch is attached to every outcome kind.
    let effects = RuntimeEffects::request_redraw();

    // Act: construct each public frame outcome variant.
    let presented = FrameOutcome::presented(effects.clone());
    let suboptimal = FrameOutcome::presented_suboptimal(effects.clone());
    let skipped = FrameOutcome::skipped(effects.clone());
    let recovery = FrameOutcome::recovery_required(effects.clone());
    let fatal = FrameOutcome::fatal(FrameError::out_of_memory(), effects.clone());

    // Assert: classification and effect extraction are independent of outcome kind.
    assert!(presented.is_presented());
    assert!(!presented.is_suboptimal());
    assert_eq!(presented.effects(), &effects);
    assert!(suboptimal.is_presented());
    assert!(suboptimal.is_suboptimal());
    assert_eq!(suboptimal.effects(), &effects);
    assert!(!skipped.is_presented());
    assert!(!skipped.is_fatal());
    assert_eq!(skipped.effects(), &effects);
    assert!(recovery.is_recovery_required());
    assert_eq!(recovery.effects(), &effects);
    assert!(fatal.is_fatal());
    assert_eq!(fatal.fatal_error(), Some(&FrameError::OutOfMemory));
    assert_eq!(fatal.effects(), &effects);
    assert_eq!(fatal.into_effects(), effects);
}

#[test]
fn frame_error_categories_are_host_inspectable() {
    assert_eq!(FrameError::device_lost(), FrameError::DeviceLost);
    assert_eq!(
        FrameError::validation("invalid pass"),
        FrameError::Validation("invalid pass".into())
    );
    assert_eq!(
        FrameError::presentation("surface"),
        FrameError::Presentation("surface".into())
    );
    assert_eq!(
        FrameError::other("unexpected"),
        FrameError::Other("unexpected".into())
    );
}

#[allow(dead_code)]
fn compile_only_borrowed_frame_contract() {
    let _ = borrowed_frame_contract;
}

fn custom_paint_runtime(draw_id: u64) -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_root(CustomPaint::new(draw_id));
    runtime.update(Instant::now());
    assert!(runtime.focus_first_focusable());
    runtime.drain_external_input();
    runtime
}

#[test]
fn should_return_none_when_frame_appearance_provider_declines() {
    // Arrange
    let provider = Arc::new(|_, _| None);
    let mut runtime = Runtime::new();
    runtime.set_root(CustomPaint::new(8).frame_appearance(provider));

    // Act
    let appearance = runtime.frame_appearance(true);

    // Assert
    assert!(appearance.is_none());
}

#[test]
fn should_retain_first_frame_appearance_provider_for_duplicate_draw_ids() {
    // Arrange
    let first = Arc::new(|_, _| {
        Some(harbor_widget::scene::primitive::ExternalFrameAppearance::new([1.0, 0.0, 0.0, 1.0]))
    });
    let second = Arc::new(|_, _| {
        Some(harbor_widget::scene::primitive::ExternalFrameAppearance::new([0.0, 1.0, 0.0, 1.0]))
    });
    let mut runtime = Runtime::new();
    runtime.set_root(
        Column::new()
            .child(CustomPaint::new(7).frame_appearance(first))
            .child(CustomPaint::new(7).frame_appearance(second)),
    );

    // Act
    let appearance = runtime.frame_appearance(true);

    // Assert
    assert_eq!(
        appearance.map(|value| value.rgba),
        Some([1.0, 0.0, 0.0, 1.0])
    );
}

#[test]
fn adapter_reports_unsupported_events_without_dispatching_them() {
    // Arrange: a focused external-input target and events the adapter does not own.
    let mut runtime = custom_paint_runtime(10);
    let mut adapter = WinitAdapter::new();
    let unsupported = [
        WindowEvent::RedrawRequested,
        WindowEvent::CloseRequested,
        WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Other(8),
        },
    ];

    // Act and assert each unsupported event independently.
    for event in unsupported {
        let outcome = adapter.handle_event(&mut runtime, &event);
        assert_eq!(outcome, WinitEventOutcome::unhandled());
    }
    assert!(runtime.drain_external_input().is_empty());
}

#[test]
fn adapter_dispatches_pointer_events_with_scaled_position_and_latest_cursor_state() {
    // Arrange
    let mut runtime = custom_paint_runtime(11);
    let mut adapter = WinitAdapter::new();
    adapter.set_scale_factor(2.0);

    let moved = WindowEvent::CursorMoved {
        device_id: winit::event::DeviceId::dummy(),
        position: PhysicalPosition::new(80.0, 40.0),
    };
    let pressed = WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state: ElementState::Pressed,
        button: MouseButton::Right,
    };
    let wheel = WindowEvent::MouseWheel {
        device_id: winit::event::DeviceId::dummy(),
        delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(3.0, -4.0)),
        phase: winit::event::TouchPhase::Moved,
    };

    // Act
    assert!(adapter.handle_event(&mut runtime, &moved).is_handled());
    assert!(adapter.handle_event(&mut runtime, &pressed).is_handled());
    assert!(adapter.handle_event(&mut runtime, &wheel).is_handled());
    let events = runtime.drain_external_input();

    // Assert
    assert_eq!(
        events,
        vec![
            (
                11,
                UiEvent::Pointer(PointerEvent::new(
                    harbor_widget::layout::Point::new(40.0, 20.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    0,
                ))
            ),
            (
                11,
                UiEvent::Pointer(PointerEvent::new(
                    harbor_widget::layout::Point::new(40.0, 20.0),
                    PointerPhase::Down,
                    PointerButton::Right,
                    0,
                ))
            ),
            (
                11,
                UiEvent::Pointer(PointerEvent::new(
                    harbor_widget::layout::Point::new(40.0, 20.0),
                    PointerPhase::WheelPixel { dx: 3.0, dy: -4.0 },
                    PointerButton::Left,
                    0,
                ))
            ),
        ]
    );
}

#[test]
fn adapter_keeps_valid_scale_and_pointer_state_when_invalid_scale_is_offered() {
    // Arrange
    let mut runtime = custom_paint_runtime(12);
    let mut adapter = WinitAdapter::new();
    adapter.set_scale_factor(2.0);
    adapter.set_scale_factor(0.0);
    adapter.set_scale_factor(f32::NAN);
    adapter.set_scale_factor(f32::INFINITY);

    // Act
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: PhysicalPosition::new(20.0, 10.0),
        },
    );
    let events = runtime.drain_external_input();

    // Assert: rejected values do not reset or poison the last valid scale.
    assert_eq!(
        events,
        vec![(
            12,
            UiEvent::Pointer(PointerEvent::new(
                harbor_widget::layout::Point::new(10.0, 5.0),
                PointerPhase::Move,
                PointerButton::Left,
                0,
            )),
        )]
    );
}

#[test]
fn adapter_deduplicates_ime_composition_and_forwards_only_nonempty_commit() {
    // Arrange
    let mut runtime = custom_paint_runtime(13);
    let mut adapter = WinitAdapter::new();

    // Act: enable/preedit are state updates; only a non-empty commit is input.
    assert!(
        adapter
            .handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled))
            .is_handled()
    );
    assert!(
        adapter
            .handle_event(
                &mut runtime,
                &WindowEvent::Ime(Ime::Preedit("draft".into(), Some((0, 5)))),
            )
            .is_handled()
    );
    assert!(runtime.drain_external_input().is_empty());

    assert!(
        adapter
            .handle_event(&mut runtime, &WindowEvent::Ime(Ime::Commit("語".into())))
            .is_handled()
    );
    assert!(
        adapter
            .handle_event(&mut runtime, &WindowEvent::Ime(Ime::Commit(String::new())))
            .is_handled()
    );

    // Assert: composition text is delivered once, and empty commits are ignored.
    assert_eq!(
        runtime.drain_external_input(),
        vec![(13, UiEvent::Keyboard(KeyboardEvent::Ime("語".into())),)]
    );
}

#[test]
fn adapter_modifier_state_is_per_window_and_does_not_leak_between_runtimes() {
    // Arrange
    let mut first_runtime = custom_paint_runtime(14);
    let second_runtime = custom_paint_runtime(15);
    let mut first_adapter = WinitAdapter::new();
    let second_adapter = WinitAdapter::new();

    // Act
    let outcome = first_adapter.handle_event(
        &mut first_runtime,
        &WindowEvent::ModifiersChanged(ModifiersState::SHIFT.into()),
    );

    // Assert
    assert!(outcome.is_handled());
    assert_eq!(first_adapter.modifiers(), ModifiersState::SHIFT);
    assert_eq!(second_adapter.modifiers(), ModifiersState::empty());
    assert!(first_runtime.drain_external_input().is_empty());
    assert!(second_runtime.drain_external_input().is_empty());
}

#[test]
fn main_and_confirmation_adapters_route_only_events_offered_to_each_window() {
    // Arrange: each native window owns a distinct Runtime and adapter.
    let mut main_runtime = custom_paint_runtime(16);
    let mut confirmation_runtime = custom_paint_runtime(17);
    let mut main_adapter = WinitAdapter::new();
    let mut confirmation_adapter = WinitAdapter::new();
    let main_event = WindowEvent::Ime(Ime::Commit("main".into()));
    let confirmation_event = WindowEvent::CursorMoved {
        device_id: winit::event::DeviceId::dummy(),
        position: PhysicalPosition::new(4.0, 6.0),
    };

    // Act and assert each host route before offering the next window's event.
    main_adapter.handle_event(&mut main_runtime, &main_event);
    assert_eq!(
        main_runtime.drain_external_input(),
        vec![(16, UiEvent::Keyboard(KeyboardEvent::Ime("main".into())),)]
    );
    assert!(confirmation_runtime.drain_external_input().is_empty());

    confirmation_adapter.handle_event(&mut confirmation_runtime, &confirmation_event);
    assert_eq!(
        confirmation_runtime.drain_external_input(),
        vec![(
            17,
            UiEvent::Pointer(PointerEvent::new(
                harbor_widget::layout::Point::new(4.0, 6.0),
                PointerPhase::Move,
                PointerButton::Left,
                0,
            )),
        )]
    );
    assert!(main_runtime.drain_external_input().is_empty());
}

#[test]
fn adapter_dispatch_reaches_runtime_custom_paint_with_public_event_outcome() {
    // Arrange
    let mut runtime = custom_paint_runtime(18);
    let mut adapter = WinitAdapter::new();

    // Act
    let outcome = adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Commit("terminal input".into())),
    );

    // Assert: the adapter dispatches a platform-independent event through Runtime.
    assert!(outcome.is_handled());
    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            18,
            UiEvent::Keyboard(KeyboardEvent::Ime("terminal input".into())),
        )]
    );
}

#[test]
fn ime_enabled_without_preedit_does_not_suppress_character_keydown() {
    let mut runtime = custom_paint_runtime(19);
    let mut adapter = WinitAdapter::new();
    assert!(
        adapter
            .handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled))
            .is_handled()
    );

    // When IME is enabled but not composing (no preedit), normal character KeyDown is dispatched.
    let outcome = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("a".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );

    assert!(outcome.is_handled());
    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            19,
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: harbor_widget::input::event::Key::Character('a'),
                modifiers: Default::default(),
            }),
        )]
    );

    // When Preedit is active, KeyDown IS suppressed.
    assert!(
        adapter
            .handle_event(
                &mut runtime,
                &WindowEvent::Ime(Ime::Preedit("draft".into(), Some((0, 5)))),
            )
            .is_handled()
    );
    let outcome_composing = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("a".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    assert!(outcome_composing.is_handled());
    assert!(outcome_composing.effects.is_noop());
    assert!(runtime.drain_external_input().is_empty());
}

#[test]
fn disabled_ime_restores_character_key_dispatch_from_active_preedit() {
    let mut runtime = custom_paint_runtime(20);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Preedit("draft".into(), Some((0, 5)))),
    );
    // Transitioning to Disabled from active preedit restores direct typing
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Disabled));

    assert!(
        adapter
            .handle_keyboard_input(
                &mut runtime,
                &Key::Character("a".into()),
                ElementState::Pressed,
                KeyLocation::Standard,
            )
            .is_handled()
    );
    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            20,
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: harbor_widget::input::event::Key::Character('a'),
                modifiers: Default::default(),
            }),
        )]
    );

    let empty = adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Commit(String::new())));
    assert!(empty.is_handled());
    assert!(empty.effects.is_noop());
    assert!(runtime.drain_external_input().is_empty());
}

#[test]
fn disabled_ime_restores_character_key_dispatch_and_empty_commit_is_a_handled_noop() {
    let mut runtime = custom_paint_runtime(20);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Disabled));

    assert!(
        adapter
            .handle_keyboard_input(
                &mut runtime,
                &Key::Character("a".into()),
                ElementState::Pressed,
                KeyLocation::Standard,
            )
            .is_handled()
    );
    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            20,
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: harbor_widget::input::event::Key::Character('a'),
                modifiers: Default::default(),
            }),
        )]
    );

    let empty = adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Commit(String::new())));
    assert!(empty.is_handled());
    assert!(empty.effects.is_noop());
    assert!(runtime.drain_external_input().is_empty());
}

#[test]
fn modifier_changes_are_applied_to_dispatched_keyboard_events() {
    // Arrange: update the adapter's per-window modifier snapshot.
    let mut runtime = custom_paint_runtime(26);
    let mut adapter = WinitAdapter::new();
    let modifiers = ModifiersState::SHIFT | ModifiersState::CONTROL;
    assert!(
        adapter
            .handle_event(
                &mut runtime,
                &WindowEvent::ModifiersChanged(modifiers.into()),
            )
            .is_handled()
    );

    // Act: dispatch a character key through the public keyboard boundary.
    let outcome = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("x".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );

    // Assert: the runtime receives the adapter's current modifier state.
    assert!(outcome.is_handled());
    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            26,
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: harbor_widget::input::event::Key::Character('x'),
                modifiers: Modifiers {
                    shift: true,
                    ctrl: true,
                    ..Modifiers::default()
                },
            }),
        )]
    );
}

#[test]
fn unsupported_keyboard_keys_are_unhandled_without_external_input() {
    // Arrange: keys with no widget mapping, including an empty character string.
    let mut runtime = custom_paint_runtime(27);
    let mut adapter = WinitAdapter::new();

    // Act and assert each malformed/unsupported key independently.
    let empty_character = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    let unsupported_named = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Named(winit::keyboard::NamedKey::CapsLock),
        ElementState::Pressed,
        KeyLocation::Standard,
    );

    assert_eq!(empty_character, WinitEventOutcome::unhandled());
    assert_eq!(unsupported_named, WinitEventOutcome::unhandled());
    assert!(runtime.drain_external_input().is_empty());
}

#[test]
fn focus_loss_cancels_all_touch_captures_before_later_touch_ends() {
    // Arrange: two independent touch contacts press the same button.
    let clicks = Arc::new(AtomicUsize::new(0));
    let clicks_clone = Arc::clone(&clicks);
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK").on_click(move |_| {
        clicks_clone.fetch_add(1, Ordering::SeqCst);
    }));
    runtime.update(Instant::now());
    let mut adapter = WinitAdapter::new();

    for (index, source_id) in [31, 47].into_iter().enumerate() {
        let outcome = adapter.handle_event(
            &mut runtime,
            &WindowEvent::Touch(Touch {
                device_id: winit::event::DeviceId::dummy(),
                phase: TouchPhase::Started,
                location: PhysicalPosition::new(4.0, 4.0),
                force: None,
                id: source_id,
            }),
        );
        assert!(outcome.is_handled());
        assert!(
            runtime
                .input()
                .captor((1 << 63) | (index as u64 + 1))
                .is_some()
        );
    }

    // Act: losing the native window focus cancels every captured pointer.
    let focus_loss = adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));

    // Assert: captures are released, and stale touch endings cannot activate the button.
    // Touch starts already own the outstanding redraw edge; focus-loss work coalesces.
    assert!(focus_loss.is_handled());
    assert!(!focus_loss.effects.request_redraw);
    assert_eq!(runtime.input().captor((1 << 63) | 1), None);
    assert_eq!(runtime.input().captor((1 << 63) | 2), None);
    for pointer_id in [31, 47] {
        let outcome = adapter.handle_event(
            &mut runtime,
            &WindowEvent::Touch(Touch {
                device_id: winit::event::DeviceId::dummy(),
                phase: TouchPhase::Ended,
                location: PhysicalPosition::new(500.0, 500.0),
                force: None,
                id: pointer_id,
            }),
        );
        assert!(outcome.is_handled());
    }
    assert_eq!(clicks.load(Ordering::SeqCst), 0);
}

#[test]
fn ime_suppresses_only_character_keydown_during_preedit_and_keeps_keyup_handled() {
    // Arrange: composition (preedit) is active for this window.
    let mut runtime = custom_paint_runtime(22);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Preedit("draft".into(), Some((0, 5)))),
    );

    // Act: a character press during preedit is suppressed, while its release remains a key event.
    let press = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("a".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    let release = adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("a".into()),
        ElementState::Released,
        KeyLocation::Standard,
    );

    // Assert: both are handled, but only the release reaches the widget runtime.
    assert!(press.is_handled());
    assert!(press.effects.is_noop());
    assert!(release.is_handled());
    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            22,
            UiEvent::Keyboard(KeyboardEvent::KeyUp {
                key: harbor_widget::input::event::Key::Character('a'),
                modifiers: Default::default(),
            }),
        )]
    );
}

#[test]
fn adapter_routes_focus_loss_to_custom_paint() {
    let mut runtime = custom_paint_runtime(25);
    let mut adapter = WinitAdapter::new();
    assert!(
        adapter
            .handle_event(&mut runtime, &WindowEvent::Focused(false))
            .is_handled()
    );

    assert_eq!(
        runtime.drain_external_input(),
        vec![(
            25,
            UiEvent::Focus(harbor_widget::input::event::FocusEvent::Lost),
        )]
    );
}

#[test]
fn adapter_routes_right_and_middle_button_down_up_events() {
    let mut runtime = custom_paint_runtime(23);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: PhysicalPosition::new(4.0, 4.0),
        },
    );

    for button in [MouseButton::Right, MouseButton::Middle] {
        for state in [ElementState::Pressed, ElementState::Released] {
            assert!(
                adapter
                    .handle_event(
                        &mut runtime,
                        &WindowEvent::MouseInput {
                            device_id: winit::event::DeviceId::dummy(),
                            state,
                            button,
                        },
                    )
                    .is_handled()
            );
        }
    }

    let events = runtime.drain_external_input();
    assert_eq!(events.len(), 5); // cursor move plus two button pairs
    assert!(matches!(
        events[1].1,
        UiEvent::Pointer(PointerEvent {
            button: PointerButton::Right,
            phase: PointerPhase::Down,
            pointer_id: 0,
            ..
        })
    ));
    assert!(matches!(
        events[2].1,
        UiEvent::Pointer(PointerEvent {
            button: PointerButton::Right,
            phase: PointerPhase::Up,
            pointer_id: 0,
            ..
        })
    ));
    assert!(matches!(
        events[3].1,
        UiEvent::Pointer(PointerEvent {
            button: PointerButton::Middle,
            phase: PointerPhase::Down,
            pointer_id: 0,
            ..
        })
    ));
    assert!(matches!(
        events[4].1,
        UiEvent::Pointer(PointerEvent {
            button: PointerButton::Middle,
            phase: PointerPhase::Up,
            pointer_id: 0,
            ..
        })
    ));
}

#[test]
fn focus_loss_clears_ime_suppression_before_refocus() {
    let mut runtime = custom_paint_runtime(24);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
    );

    // A modifier change during composition permits the shortcut exactly once.
    adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("a".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    let shortcut_events = runtime.drain_external_input();
    assert_eq!(shortcut_events.len(), 1);
    assert!(matches!(
        shortcut_events[0].1,
        UiEvent::Keyboard(KeyboardEvent::KeyDown { .. })
    ));

    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
    adapter.handle_event(&mut runtime, &WindowEvent::Focused(true));
    // Focus loss clears enablement, so refocus does not inherit suppression.
    adapter.handle_keyboard_input(
        &mut runtime,
        &Key::Character("b".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    let focus_events = runtime.drain_external_input();
    assert!(focus_events.iter().any(|(_, event)| matches!(
        event,
        UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: harbor_widget::input::event::Key::Character('b'),
            ..
        })
    )));
}

#[test]
fn touch_events_map_to_pointer_phases_ids_and_scaled_positions() {
    let mut runtime = custom_paint_runtime(21);
    let mut adapter = WinitAdapter::new();
    adapter.set_scale_factor(2.0);
    for (phase, location) in [
        (TouchPhase::Started, PhysicalPosition::new(20.0, 10.0)),
        (TouchPhase::Moved, PhysicalPosition::new(22.0, 12.0)),
        (TouchPhase::Ended, PhysicalPosition::new(24.0, 14.0)),
    ] {
        let outcome = adapter.handle_event(
            &mut runtime,
            &WindowEvent::Touch(Touch {
                device_id: winit::event::DeviceId::dummy(),
                phase,
                location,
                force: None,
                id: 77,
            }),
        );
        assert!(outcome.is_handled());
    }

    let events = runtime.drain_external_input();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].0, 21);
    assert_eq!(events[0].1.pointer_id(), Some((1 << 63) | 1));
    assert_eq!(
        events[0].1,
        UiEvent::Pointer(PointerEvent::new(
            harbor_widget::layout::Point::new(10.0, 5.0),
            PointerPhase::Down,
            PointerButton::Left,
            (1 << 63) | 1,
        ))
    );
    assert!(events[1].1.is_pointer_phase(PointerPhase::Move));
    assert!(events[2].1.is_pointer_phase(PointerPhase::Up));
}
