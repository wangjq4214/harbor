use super::*;

#[test]
fn notification_preserves_terminal_focus_allocation_and_routes_dismissal_as_host_intent() {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        std::io::empty(),
        Writer(bytes.clone()),
        || true,
    )));
    let bridge = TerminalWidgetBridge::new(1, engine, Arc::new(AtomicBool::new(false)));
    let controller = WorkspaceUiController::new(vec![snapshot(1, true, false)], bridge, 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    let focused = runtime.input().focused();
    let allocation = controller.latest_terminal_allocation().unwrap();
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("Invalid settings; previously accepted configuration remains effective".into()),
    );
    runtime.update(Instant::now());
    assert_eq!(runtime.input().focused(), focused);
    assert_eq!(controller.latest_terminal_allocation(), Some(allocation));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('a'),
        modifiers: Modifiers::default(),
    }));
    assert_eq!(
        &*bytes.lock().unwrap(),
        b"a",
        "notification must not inject PTY bytes or gate typing"
    );
    // Empty overlay space still permits normal rail interaction.
    let row = tab_rows(&runtime)[0];
    let rect = runtime.arena().get(row).unwrap().layout_rect().unwrap();
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(rect.min.x + 8.0, rect.min.y + 8.0),
            phase,
            PointerButton::Left,
            7,
        )));
    }
    assert_eq!(
        controller.drain_actions()[0].command,
        AppCommand::Tab(TabCommand::Activate(TabId(1)))
    );
    runtime.request_focus(&controller.terminal_focus());
    for (width, height, scale) in [(1000, 500, 1.0), (1200, 800, 2.0)] {
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        let before = controller.latest_terminal_allocation();
        let focused = runtime.input().focused();
        let delta = runtime.pending_delta().unwrap();
        let close = delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| match &item.primitive {
                Primitive::Icon {
                    icon: LucideIcon::X,
                    rect,
                    ..
                } => Some(*rect),
                _ => None,
            })
            .max_by(|a, b| a.max.x.total_cmp(&b.max.x))
            .expect("visible vector close control");
        assert!((close.max.x - (width as f32 / scale - 21.0)).abs() < 1.0);
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(close.min.x + 9.0, close.min.y + 9.0),
                phase,
                PointerButton::Left,
                8,
            )));
        }
        let actions = controller.drain_actions();
        assert_eq!(actions.len(), 1);
        let AppCommand::DismissNotification(revision) = actions[0].command else {
            panic!("dismiss action");
        };
        assert_eq!(actions[0].focus, TabFocusPolicy::PreserveRail);
        assert!(
            controller.store.state().read().notices.message().is_some(),
            "routing never mutates Host state"
        );
        controller.dismiss_notification(revision);
        runtime.update(Instant::now());
        assert!(controller.store.state().read().notices.message().is_none());
        assert_eq!(runtime.input().focused(), focused);
        assert_eq!(controller.latest_terminal_allocation(), before);
        controller.publish_configuration_notice(
            ConfigurationNoticeSource::Reload,
            Some(format!("A different error after {scale}")),
        );
        runtime.update(Instant::now());
    }
}

#[test]
fn idle_notification_expiry_removes_glass_without_changing_terminal_focus_or_allocation() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    let allocation = controller.latest_terminal_allocation();
    let focused = runtime.input().focused();
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Resource,
        Some("Unrecovered resource failure".into()),
    );
    runtime.update(Instant::now());
    let (expired, deadline) = controller.advance_notifications(Instant::now());
    assert!(!expired);
    let deadline = deadline.expect("idle host receives a wake deadline");
    assert!(
        runtime
            .pending_delta()
            .unwrap()
            .added
            .iter()
            .any(|item| matches!(item.primitive, Primitive::BackdropMaterial { .. }))
    );
    assert_eq!(
        controller.advance_notifications(deadline - std::time::Duration::from_millis(1)),
        (false, Some(deadline))
    );
    assert_eq!(controller.advance_notifications(deadline), (true, None));
    runtime.update(deadline);
    let delta = runtime.pending_delta().unwrap();
    // Unsubmitted added primitives coalesce away when their overlay is removed.
    assert!(
        !delta
            .added
            .iter()
            .chain(&delta.modified)
            .any(|item| matches!(item.primitive, Primitive::BackdropMaterial { .. }))
    );
    assert_eq!(controller.latest_terminal_allocation(), allocation);
    assert_eq!(runtime.input().focused(), focused);
    assert!(controller.drain_actions().is_empty());
    assert_eq!(controller.advance_notifications(deadline), (false, None));
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Resource,
        Some("Unrecovered resource failure".into()),
    );
    assert_eq!(controller.store.state().read().notices.message(), None);
    assert_eq!(
        controller.advance_notifications(Instant::now()),
        (false, None)
    );
}

#[test]
fn notification_dismissal_suppression_recovery_and_independent_sources() {
    use ConfigurationNoticeSource as Source;
    let mut state = ConfigurationNotices::default();
    state.publish(Source::Reload, Some("invalid TOML".into()));
    let old_revision = state.revision;
    state.dismiss(old_revision);
    state.publish(Source::Reload, Some("invalid TOML".into()));
    assert!(
        state.message().is_none(),
        "identical watcher failure stays dismissed"
    );
    state.publish(Source::Reload, Some("invalid color".into()));
    state.dismiss(old_revision);
    assert_eq!(
        state.message().as_deref(),
        Some("invalid color"),
        "stale dismissal cannot hide newer feedback"
    );
    state.publish(
        Source::Observation,
        Some("watching unavailable; manual reload remains available".into()),
    );
    state.publish(Source::Reload, None);
    assert!(
        state.message().unwrap().contains("manual reload"),
        "manual success cannot claim observation recovery"
    );
    state.publish(Source::Observation, None);
    assert!(
        state.message().is_none(),
        "recovery removes obsolete failures without a success toast"
    );
    state.publish(Source::Resource, Some("session 1 needs recovery".into()));
    state.publish(Source::Reload, None); // a later missing file is not resource recovery
    assert_eq!(state.message().as_deref(), Some("session 1 needs recovery"));
    state.publish(Source::Resource, None); // successful application transaction
    state.publish(Source::Reload, Some("invalid TOML".into()));
    assert_eq!(
        state.message().as_deref(),
        Some("invalid TOML"),
        "failure after recovery is a new episode"
    );
}

#[test]
fn reload_rebinding_keeps_consumed_press_repeat_and_release_out_of_terminal() {
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let engine = std::sync::Arc::new(std::sync::Mutex::new(
        harbor_terminal::Terminal::new_headless_with_io(
            4,
            20,
            std::io::empty(),
            Writer(bytes.clone()),
            || true,
        ),
    ));
    let test_bridge = crate::terminal_view::TerminalWidgetBridge::new(
        1,
        engine,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut runtime = mount(controller.clone(), 1000, 320);
    controller.tabs().sync(
        vec![snapshot(1, true, false)],
        Some(test_bridge.clone()),
        1000.0,
    );
    runtime.update(Instant::now());
    runtime.request_focus(&controller.terminal_focus());
    let modifiers = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::default()
    };
    let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('R'),
        modifiers,
    });
    assert!(runtime.dispatch(event.clone()).request_redraw);
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        controller.drain_actions()[0].command,
        AppCommand::ReloadConfiguration
    );
    let mut raw = harbor_config::RawKeybindings::default();
    raw.entries.insert(
        "app.reload-configuration".to_owned(),
        vec![harbor_config::RawKeybinding {
            modifiers: vec!["ctrl".to_owned(), "shift".to_owned()],
            key: "q".to_owned(),
        }],
    );
    controller.publish_keybindings(crate::command::validate_keybindings(&raw).unwrap());
    controller.report_configuration_result("Configuration reloaded".to_owned());
    runtime.update(Instant::now());
    assert!(
        runtime
            .dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Character('r'),
                modifiers: Modifiers::default()
            }))
            .request_redraw
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        controller.drain_actions()[0].command,
        AppCommand::ReloadConfiguration
    );
    assert!(
        runtime
            .dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
                key: Key::Character('r'),
                modifiers: Modifiers::default()
            }))
            .request_redraw
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert!(controller.drain_actions().is_empty());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('q'),
        modifiers,
    }));
    assert_eq!(
        controller.drain_actions()[0].command,
        AppCommand::ReloadConfiguration
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
        key: Key::Character('q'),
        modifiers,
    }));
    controller
        .tabs()
        .sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
    assert!(controller.store.state().read().keybindings.is_some());
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
        Some("Configuration reloaded")
    );
    runtime.update(Instant::now());
    runtime.request_focus(&controller.terminal_focus());
    assert!(bytes.lock().unwrap().is_empty());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('r'),
        modifiers,
    }));
    assert!(controller.drain_actions().is_empty());
    assert_eq!(
        *bytes.lock().unwrap(),
        b"\x12",
        "released old chord must return to the PTY writer"
    );
}
