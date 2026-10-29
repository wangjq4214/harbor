use super::palette_support::*;
use super::*;

#[test]
fn palette_build_queries_context_once_for_visible_rows_and_never_for_no_matches() {
    let (tabs, controller, mut runtime, _) = fixture();
    let bridge = tabs.active_bridge().unwrap();
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    let before = bridge.alt_screen_queries();
    runtime.update(Instant::now());
    assert!(
        crate::command::command_registry()
            .iter()
            .filter(|entry| entry.palette_visible)
            .count()
            > 5
    );
    assert_eq!(
        bridge.alt_screen_queries() - before,
        1,
        "one context snapshot for all result rows"
    );

    let before = bridge.alt_screen_queries();
    assert!(input(&controller).set_text("definitely-not-a-command"));
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("No matching commands"));
    assert_eq!(
        bridge.alt_screen_queries(),
        before,
        "empty results do not drain the PTY"
    );
}

#[test]
fn palette_disabled_titles_and_shortcuts_are_muted_without_reason_text() {
    for binding_count in 0..=2 {
        let (_, controller, mut runtime, bytes) = fixture();
        let mut raw = harbor_config::RawKeybindings::default();
        raw.entries.insert(
            "app.next-tab".into(),
            ["ctrl", "alt"]
                .into_iter()
                .take(binding_count)
                .map(|modifier| harbor_config::RawKeybinding {
                    modifiers: vec![modifier.into()],
                    key: "n".into(),
                })
                .collect(),
        );
        controller.publish_keybindings(validate_keybindings(&raw).unwrap());
        open(&controller, &mut runtime);
        replace(&mut runtime, "app.next-tab");
        let theme = harbor_widget::Theme::default().button;
        assert_ne!(theme.disabled.foreground, theme.normal.foreground);
        let (_, row) = palette_geometry(&runtime);
        let text = row_text(&runtime, row, theme.disabled.foreground);
        let expected: Vec<&str> = match binding_count {
            0 => vec!["Next tab", "Unbound"],
            1 => vec!["Next tab", "Ctrl", "N"],
            _ => vec!["Next tab", "Ctrl", "N", "Alt", "N"],
        };
        assert_eq!(
            text, expected,
            "only the title and shortcuts, never a reason"
        );
        assert!(!painted(&runtime).contains("No other tab"));

        replace(&mut runtime, "app.new-tab");
        let (_, row) = palette_geometry(&runtime);
        assert_eq!(
            row_text(&runtime, row, theme.normal.foreground),
            ["New tab", "Ctrl", "T"],
            "available commands keep their normal color",
        );
        tap(&mut runtime, Key::Enter);
        runtime.update(Instant::now());
        let (_, row) = palette_geometry(&runtime);
        assert_eq!(
            row_text(&runtime, row, theme.disabled.foreground),
            ["New tab", "Ctrl", "T"],
            "pending execution also disables and mutes the row",
        );
        assert_eq!(
            take_one(&controller).command,
            AppCommand::Tab(TabCommand::New)
        );
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn palette_filters_navigates_and_executes_one_real_tab_reduction_without_pty_input() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    assert!(painted(&runtime).contains("Reload configuration"));
    assert!(!painted(&runtime).contains("No other tab"));
    tap(&mut runtime, Key::ArrowDown);
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .popups
            .get::<palette::PaletteState>()
            .unwrap()
            .selected,
        1
    );
    tap(&mut runtime, Key::ArrowUp);
    replace(&mut runtime, "NeW TaB");
    assert!(painted(&runtime).contains("New tab"));
    replace(&mut runtime, "APP.NEW-TAB");
    down(&mut runtime, Key::Enter, Modifiers::default());
    down(&mut runtime, Key::Enter, Modifiers::default()); // repeat while Host has not drained
    let request = take_one(&controller);
    assert!(matches!(request.source, CommandSource::Palette { .. }));
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    let AppCommand::Tab(command) = request.command else {
        panic!("tab request")
    };
    let mut executions = 0;
    let resulting_bytes = Arc::new(Mutex::new(Vec::new()));
    let outcome = tabs
        .execute_command(command, |_, draw| {
            executions += 1;
            Ok(resources(draw, &resulting_bytes))
        })
        .unwrap();
    assert_eq!(executions, 1);
    assert!(outcome.active_bridge_changed);
    assert_eq!(tabs.snapshots().len(), 2);
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .is_err()
    );
    controller
        .tabs()
        .sync(tabs.snapshots(), tabs.active_bridge(), 1000.0);
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    down(&mut runtime, Key::Enter, Modifiers::default()); // held invocation after resulting-tab focus
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert!(controller.drain_actions().is_empty());
    assert!(
        bytes.lock().unwrap().is_empty(),
        "opener, filter, navigation, Enter and releases are consumed"
    );
    assert!(
        resulting_bytes.lock().unwrap().is_empty(),
        "invocation repeats/releases do not reach the new terminal"
    );
    tap(&mut runtime, Key::Character('x'));
    assert!(
        bytes.lock().unwrap().is_empty(),
        "old terminal never receives post-command input"
    );
    assert_eq!(
        &*resulting_bytes.lock().unwrap(),
        b"x",
        "new terminal receives resumed input only"
    );
}

#[test]
fn palette_empty_results_disabled_rows_and_filter_changes_cannot_execute_phantom_selection() {
    let (tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    tap(&mut runtime, Key::ArrowUp); // selects last numeric row
    replace(&mut runtime, "does not match");
    assert!(painted(&runtime).contains("No matching commands"));
    for key in [Key::ArrowUp, Key::ArrowDown, Key::Enter] {
        tap(&mut runtime, key);
    }
    assert!(controller.drain_actions().is_empty());
    replace(&mut runtime, "app.select-tab-9");
    assert!(!painted(&runtime).contains("Tab does not exist"));
    tap(&mut runtime, Key::Enter);
    assert!(controller.drain_actions().is_empty());
    assert_eq!(tabs.snapshots().len(), 1);
    assert!(
        controller
            .store
            .state()
            .read()
            .notices
            .message()
            .unwrap()
            .contains("Rejected")
    );
    replace(&mut runtime, "app.new-tab");
    tap(&mut runtime, Key::Enter);
    assert_eq!(
        take_one(&controller).command,
        AppCommand::Tab(TabCommand::New)
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_binding_overrides_multiple_unbinding_and_rejected_reload_display() {
    let (_, controller, mut runtime, _) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "app.new-tab");
    assert!(painted(&runtime).contains("Ctrl\nT"));
    let mut raw = harbor_config::RawKeybindings::default();
    raw.entries.insert(
        "app.new-tab".into(),
        vec![
            harbor_config::RawKeybinding {
                modifiers: vec!["ctrl".into()],
                key: "n".into(),
            },
            harbor_config::RawKeybinding {
                modifiers: vec!["alt".into()],
                key: "n".into(),
            },
        ],
    );
    controller.publish_keybindings(validate_keybindings(&raw).unwrap());
    runtime.update(Instant::now());
    let text = painted(&runtime);
    assert!(text.contains("Ctrl\nN\nAlt\nN"), "{text:?}");
    assert!(!text.contains("Ctrl\nT"));
    raw.entries.insert("app.new-tab".into(), vec![]);
    controller.publish_keybindings(validate_keybindings(&raw).unwrap());
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("Unbound"));
    let accepted = controller.store.state().read().keybindings.clone();
    raw.entries.insert("unknown".into(), vec![]);
    assert!(validate_keybindings(&raw).is_err());
    // Failed candidates never reach Host publication.
    assert_eq!(controller.store.state().read().keybindings, accepted);
    controller.report_palette_result(
        AppCommand::ReloadConfiguration,
        Err("Failed: invalid candidate"),
    );
    assert_eq!(controller.store.state().read().keybindings, accepted);
}

#[test]
fn palette_rechecks_host_context_disposes_stale_requests_and_preserves_new_editor() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "app.new-tab");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&controller);
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap(); // authoritative context changes before drain
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap_err()
            .contains("changed")
    );
    assert!(!controller.palette_is_open());
    controller
        .tabs()
        .sync(tabs.snapshots(), tabs.active_bridge(), 1000.0);
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    let new_editor = input(&controller);
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .is_err()
    );
    assert!(
        controller.palette_is_open(),
        "old requests cannot close the replacement editor"
    );
    assert_eq!(new_editor.text(), "");
    assert_eq!(tabs.snapshots().len(), 2);
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_tab_navigation_numeric_bounds_and_final_close_use_existing_model_semantics() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap();
    for query in [
        "app.previous-tab",
        "app.next-tab",
        "app.select-tab-1",
        "app.close-active-tab",
        "app.close-active-tab",
    ] {
        controller
            .tabs()
            .sync(tabs.snapshots(), tabs.active_bridge(), 1000.0);
        runtime.request_focus(&controller.terminal_focus());
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        replace(&mut runtime, query);
        tap(&mut runtime, Key::Enter);
        let request = take_one(&controller);
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap();
        let AppCommand::Tab(command) = request.command else {
            panic!("tab")
        };
        let outcome = tabs
            .execute_command(command, |_, _| panic!("no creation"))
            .unwrap();
        if tabs.snapshots().is_empty() {
            assert!(outcome.close_window);
            break;
        }
        assert!(!outcome.close_window);
        assert!(tabs.active_id().is_some());
    }
    assert!(tabs.snapshots().is_empty());
    assert!(
        command_availability(
            AppCommand::Tab(TabCommand::Numeric(TabIndex::from_valid_u8(9))),
            &tabs.snapshots(),
            crate::command::CommandContext::default(),
        )
        .is_err()
    );
    assert!(bytes.lock().unwrap().is_empty());
}
