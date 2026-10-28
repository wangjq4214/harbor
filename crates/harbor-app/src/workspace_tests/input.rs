use super::*;

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn canceled_paste_press_cannot_replay_after_gate_focus_loss_or_surface_suspend() {
    use harbor_widget::winit::{NativeKeyboardMetadata, WindowPresenter, WinitAdapter};
    use winit::{
        event::{ElementState, WindowEvent},
        keyboard::{Key as NativeKey, KeyLocation, ModifiersState},
    };
    for (boundary, release_dropped) in [
        ("gate", true),
        ("focus", true),
        ("focus", false),
        ("surface", true),
    ] {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            4,
            20,
            std::io::empty(),
            Writer(bytes.clone()),
            || true,
        )));
        let test_bridge = TerminalWidgetBridge::new(1, engine, Arc::new(AtomicBool::new(false)));
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        controller
            .tabs()
            .sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
        runtime.update(Instant::now());
        runtime.request_focus(&controller.terminal_focus());
        let mut adapter = WinitAdapter::new();
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
        );
        let key = NativeKey::Character("v".into());
        adapter.handle_keyboard_input(
            &mut runtime,
            &key,
            ElementState::Pressed,
            KeyLocation::Standard,
        );
        assert_eq!(controller.drain_actions()[0].command, AppCommand::Paste);
        assert!(bytes.lock().unwrap().is_empty());
        match boundary {
            "gate" => {
                runtime.cancel_active_input_ownership(Point::ZERO);
            }
            "focus" => {
                adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
            }
            "surface" => {
                let mut presenter = WindowPresenter::new(1000, 320, 1.0);
                presenter.handle_event_with_size(
                    &mut adapter,
                    &mut runtime,
                    &WindowEvent::Resized(winit::dpi::PhysicalSize::new(0, 0)),
                    Some((0, 0)),
                );
                presenter.handle_event_with_size(
                    &mut adapter,
                    &mut runtime,
                    &WindowEvent::Resized(winit::dpi::PhysicalSize::new(1000, 320)),
                    Some((1000, 320)),
                );
            }
            _ => unreachable!(),
        }
        // Windows focus transitions synthesize release/gain-press events
        // with repeat=false. Neither may clear canceled ownership or replay
        // the chord; exercise the exact metadata route used by WindowEvent.
        adapter.handle_keyboard_input_with_metadata(
            &mut runtime,
            &key,
            ElementState::Released,
            KeyLocation::Standard,
            NativeKeyboardMetadata {
                repeat: false,
                is_synthetic: true,
            },
        );
        adapter.handle_event(&mut runtime, &WindowEvent::Focused(true));
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
        );
        adapter.handle_keyboard_input_with_metadata(
            &mut runtime,
            &key,
            ElementState::Pressed,
            KeyLocation::Standard,
            NativeKeyboardMetadata {
                repeat: false,
                is_synthetic: true,
            },
        );
        adapter.handle_keyboard_input_with_repeat(
            &mut runtime,
            &key,
            ElementState::Pressed,
            KeyLocation::Standard,
            true,
        );
        assert!(
            controller.drain_actions().is_empty(),
            "synthetic lifecycle: {boundary}"
        );
        assert!(
            bytes.lock().unwrap().is_empty(),
            "synthetic lifecycle: {boundary}"
        );
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::ModifiersChanged(ModifiersState::empty().into()),
        );
        // A still-held repeat remains consumed, but the canceled action must
        // not execute. The confirmation gate then drops the physical key-up.
        adapter.handle_keyboard_input_with_repeat(
            &mut runtime,
            &key,
            ElementState::Pressed,
            KeyLocation::Standard,
            true,
        );
        assert!(controller.drain_actions().is_empty(), "{boundary}");
        assert!(bytes.lock().unwrap().is_empty(), "{boundary}");
        adapter.handle_event(&mut runtime, &WindowEvent::Focused(true));
        if !release_dropped {
            adapter.handle_keyboard_input(
                &mut runtime,
                &key,
                ElementState::Released,
                KeyLocation::Standard,
            );
            assert!(controller.drain_actions().is_empty());
            assert!(bytes.lock().unwrap().is_empty());
        }
        // A genuinely new native press must be ordinary input, with or without a lost release.
        adapter.handle_keyboard_input(
            &mut runtime,
            &key,
            ElementState::Pressed,
            KeyLocation::Standard,
        );
        adapter.handle_keyboard_input(
            &mut runtime,
            &key,
            ElementState::Released,
            KeyLocation::Standard,
        );
        assert!(controller.drain_actions().is_empty(), "{boundary}");
        assert_eq!(*bytes.lock().unwrap(), b"v", "{boundary}");
    }
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn programmatic_terminal_focus_reaches_ime_provider_and_preedit_input() {
    let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
    let controller = WorkspaceUiController::new(
        vec![snapshot(1, true, false)],
        TerminalWidgetBridge::new(1, Arc::clone(&terminal), Arc::new(AtomicBool::new(false))),
        1000.0,
    );
    let mut runtime = mount(controller.clone(), 1000, 320);

    let focus = runtime.request_focus(&controller.terminal_focus());
    assert!(focus.request_redraw);
    let effects = runtime.update(Instant::now());
    let ime = effects.ime.expect("focused terminal IME effect");
    assert_eq!(ime.allowed, Some(true));

    runtime.dispatch(UiEvent::ImePreedit(
        harbor_widget::input::event::ImePreedit::new("draft", Some((0, 5))),
    ));
    assert_eq!(
        terminal
            .lock()
            .unwrap()
            .preedit()
            .map(|value| value.text.as_str()),
        Some("draft")
    );
}
