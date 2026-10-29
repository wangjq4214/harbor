use super::*;
use std::sync::atomic::AtomicBool;

fn resources(id: ExternalDrawId) -> TerminalTabResources {
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
    let bridge =
        TerminalWidgetBridge::new(id, Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    TerminalTabResources::new(terminal, bridge, "test-shell")
}

fn create(manager: &mut TabManager) -> (TabId, ExternalDrawId) {
    manager
        .create_tab(|_, draw_id| Ok(resources(draw_id)))
        .unwrap();
    let snapshot = manager.snapshots().pop().unwrap();
    assert_eq!(snapshot.draw_id, manager.active_bridge().unwrap().draw_id());
    (snapshot.id, snapshot.draw_id)
}

fn terminal(manager: &TabManager, id: TabId) -> Arc<Mutex<Terminal>> {
    manager
        .tabs
        .iter()
        .find(|tab| tab.id == id)
        .map(|tab| Arc::clone(&tab.terminal))
        .unwrap()
}

#[test]
fn clipboard_output_preserves_sources_and_unrelated_events() {
    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    let (b, _) = create(&mut manager);
    terminal(&manager, a).lock().unwrap().put_bytes(
        b"\x1b]52;c;Zmlyc3Q=\x07\x1b]2;source title\x07\x1b]7;file:///source\x07\x1b]133;A\x07\x1b]52;c;c2Vjb25k\x07",
    );
    terminal(&manager, b)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;;YWN0aXZl\x1b\\");

    let output = manager.process_output(a);
    assert!(output.title_changed);
    assert!(output.metadata_changed);
    assert!(!output.active_title_changed);
    assert!(!output.request_active_invalidation);
    assert!(!output.unread_changed);
    let write = output.clipboard_write.unwrap();
    assert_eq!(write.source, a);
    assert_eq!(write.contents.as_str(), "second");
    assert!(!format!("{write:?}").contains("second"));
    assert!(!manager.active_live(write.source));
    assert_eq!(manager.source_label(a), "source title");
    assert_eq!(manager.active_title(), Some("test-shell"));
    let snapshot = &manager.snapshots()[0];
    assert_eq!(snapshot.working_directory().unwrap().path(), "/source");
    assert_eq!(
        snapshot.shell_integration(),
        Some(ShellIntegrationMarker::PromptStart)
    );
    assert!(manager.process_output(a).clipboard_write.is_none());

    let write = manager.process_output(b).clipboard_write.unwrap();
    assert_eq!(write.source, b);
    assert_eq!(write.contents.into_text(), "active");
    assert!(manager.active_live(b));
}

#[test]
fn clipboard_source_eligibility_rejects_invalid_removed_and_changed_sessions() {
    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    assert!(manager.active_live(a));
    assert!(!manager.active_live(TabId(u64::MAX)));
    terminal(&manager, a)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07");
    let write = manager.process_output(a).clipboard_write.unwrap();
    let (b, _) = create(&mut manager);
    assert!(!manager.active_live(write.source));
    manager.close(a);
    assert!(!manager.active_live(write.source));
    assert_eq!(manager.process_output(a), TabOutputOutcome::default());
    assert_eq!(
        manager.process_output(TabId(u64::MAX)),
        TabOutputOutcome::default()
    );
    assert!(manager.active_live(b));
    assert_eq!(manager.source_label(a), format!("Session {a}"));
    manager.close(b);
    let (c, _) = create(&mut manager);
    assert_ne!(c, a);
    assert_ne!(c, b);
    assert!(!manager.active_live(a));
    assert!(!manager.active_live(b));
}

#[test]
fn clipboard_delivery_settings_apply_to_current_and_future_sessions() {
    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    manager.set_clipboard_delivery(ClipboardDelivery::First);
    let (b, _) = create(&mut manager);
    for id in [a, b] {
        terminal(&manager, id)
            .lock()
            .unwrap()
            .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07\x1b]52;c;c2Vjb25k\x07");
        assert_eq!(
            manager
                .process_output(id)
                .clipboard_write
                .unwrap()
                .contents
                .as_str(),
            "first"
        );
    }
    manager.set_clipboard_delivery(ClipboardDelivery::Latest);
    for id in [a, b] {
        terminal(&manager, id)
            .lock()
            .unwrap()
            .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07\x1b]52;c;c2Vjb25k\x07\x1b]52;c;invalid!\x07");
        assert_eq!(
            manager
                .process_output(id)
                .clipboard_write
                .unwrap()
                .contents
                .as_str(),
            "second"
        );
    }
    manager.set_clipboard_delivery(ClipboardDelivery::Discard);
    let (c, _) = create(&mut manager);
    for id in [a, b, c] {
        terminal(&manager, id)
            .lock()
            .unwrap()
            .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07\x1b]2;still works\x07");
        let output = manager.process_output(id);
        assert!(output.clipboard_write.is_none());
        assert!(output.title_changed);
    }
}

#[test]
fn clipboard_delivery_changes_preserve_or_release_pending_candidates() {
    let mut manager = TabManager::new();
    let (id, _) = create(&mut manager);
    terminal(&manager, id)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07");
    manager.set_clipboard_delivery(ClipboardDelivery::First);
    terminal(&manager, id)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;c2Vjb25k\x07");
    assert_eq!(
        manager
            .process_output(id)
            .clipboard_write
            .unwrap()
            .contents
            .as_str(),
        "first"
    );

    terminal(&manager, id)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07");
    manager.set_clipboard_delivery(ClipboardDelivery::Latest);
    terminal(&manager, id)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;c2Vjb25k\x07");
    assert_eq!(
        manager
            .process_output(id)
            .clipboard_write
            .unwrap()
            .contents
            .as_str(),
        "second"
    );

    terminal(&manager, id)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07\x1b]2;retained title\x07");
    manager.set_clipboard_delivery(ClipboardDelivery::Discard);
    let output = manager.process_output(id);
    assert!(output.clipboard_write.is_none());
    assert!(output.title_changed);
}

#[test]
fn clipboard_factory_delivery_is_preserved_without_manager_override() {
    let mut manager = TabManager::new();
    manager
        .create_tab(|_, draw_id| {
            let resources = resources(draw_id);
            resources
                .terminal
                .lock()
                .unwrap()
                .set_clipboard_delivery(ClipboardDelivery::First);
            Ok(resources)
        })
        .unwrap();
    let id = manager.active_id().unwrap();
    terminal(&manager, id)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07\x1b]52;c;c2Vjb25k\x07");
    assert_eq!(
        manager
            .process_output(id)
            .clipboard_write
            .unwrap()
            .contents
            .as_str(),
        "first"
    );
}

#[test]
fn closed_reader_session_is_ineligible_even_while_tab_remains_active() {
    use std::io::Cursor;
    use std::time::{Duration, Instant};

    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut manager = TabManager::new();
    manager
        .create_tab(|_, draw_id| {
            #[allow(clippy::arc_with_non_send_sync)]
            let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                4,
                20,
                Cursor::new(Vec::<u8>::new()),
                std::io::sink(),
                move || wake_tx.send(()).is_ok(),
            )));
            let bridge = TerminalWidgetBridge::new(
                draw_id,
                Arc::clone(&terminal),
                Arc::new(AtomicBool::new(false)),
            );
            Ok(TerminalTabResources::new(terminal, bridge, "closed-shell"))
        })
        .unwrap();
    let id = manager.active_id().unwrap();
    let terminal = terminal(&manager, id);
    // An EOF reader may already have finished before this UI turn; no live precondition.
    terminal
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]52;c;Zmlyc3Q=\x07\x1b]2;closing title\x07");
    wake_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    // The EOF wake is posted just before the reader drops its sender.
    loop {
        terminal.lock().unwrap().drain_pty();
        if terminal.lock().unwrap().is_session_closed() {
            break;
        }
        assert!(Instant::now() < deadline, "reader EOF was not observed");
        std::thread::yield_now();
    }
    assert_eq!(manager.active_id(), Some(id));
    assert!(!manager.active_live(id));
    let output = manager.process_output(id);
    assert!(output.clipboard_write.is_none());
    assert!(output.title_changed);
    assert_eq!(manager.active_title(), Some("closing title"));
}

#[test]
fn poisoned_clipboard_source_is_ineligible() {
    let mut manager = TabManager::new();
    let (id, _) = create(&mut manager);
    let terminal = terminal(&manager, id);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = terminal.lock().unwrap();
        panic!("synthetic lock poison");
    }));
    assert!(!manager.active_live(id));
    assert_eq!(manager.process_output(id), TabOutputOutcome::default());
}

#[test]
fn clipboard_source_label_is_bounded_without_changing_the_title() {
    let mut manager = TabManager::new();
    let (id, _) = create(&mut manager);
    let title = "界".repeat(100);
    terminal(&manager, id)
        .lock()
        .unwrap()
        .put_bytes(format!("\x1b]2;{title}\x07").as_bytes());
    manager.process_output(id);
    assert_eq!(manager.source_label(id).chars().count(), 80);
    assert_eq!(manager.active_title(), Some(title.as_str()));
}

#[test]
fn create_allocates_ordered_distinct_ids_and_activates_new_tab() {
    let mut manager = TabManager::new();
    let (a, draw_a) = create(&mut manager);
    let (b, draw_b) = create(&mut manager);

    assert_ne!(a, b);
    assert_ne!(draw_a, draw_b);
    assert_eq!(manager.active_id(), Some(b));
    assert_eq!(
        manager
            .snapshots()
            .iter()
            .map(|tab| tab.title.as_str())
            .collect::<Vec<_>>(),
        vec!["test-shell", "test-shell"]
    );
}

#[test]
fn active_tab_changes_clear_the_previous_terminal_preedit() {
    use harbor_terminal::{Preedit, TerminalEvent};

    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    let terminal_a = terminal(&manager, a);
    terminal_a
        .lock()
        .unwrap()
        .handle_event(TerminalEvent::Preedit(Preedit::new("first", None)))
        .unwrap();

    let (b, _) = create(&mut manager);
    assert!(terminal_a.lock().unwrap().preedit().is_none());

    let terminal_b = terminal(&manager, b);
    terminal_b
        .lock()
        .unwrap()
        .handle_event(TerminalEvent::Preedit(Preedit::new("second", None)))
        .unwrap();
    manager.activate(a);

    assert!(terminal_b.lock().unwrap().preedit().is_none());
}

#[test]
fn activation_wraps_supports_numeric_lookup_and_clears_unread() {
    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    let (b, _) = create(&mut manager);
    let (c, _) = create(&mut manager);
    manager
        .tabs
        .iter_mut()
        .find(|tab| tab.id == a)
        .unwrap()
        .unread = true;

    assert!(manager.activate_next().active_bridge_changed);
    assert_eq!(manager.active_id(), Some(a));
    assert!(!manager.snapshots()[0].unread);
    assert!(manager.activate_previous().active_bridge_changed);
    assert_eq!(manager.active_id(), Some(c));
    assert!(
        manager
            .activate_numeric(TabIndex::from_valid_u8(2))
            .active_bridge_changed
    );
    assert_eq!(manager.active_id(), Some(b));
    assert_eq!(
        manager.activate_numeric(TabIndex::from_valid_u8(9)),
        TabActionOutcome::default()
    );
}

#[test]
fn shell_osc_titles_use_the_same_short_display_name_as_the_fallback() {
    assert_eq!(
        normalize_shell_title(r"C:\Windows\System32\cmd.exe".to_owned(), "cmd",),
        "cmd"
    );
    assert_eq!(normalize_shell_title("PWsh.EXE".to_owned(), "pwsh"), "pwsh");
    assert_eq!(
        normalize_shell_title(r"C:\project\README.md".to_owned(), "cmd"),
        r"C:\project\README.md"
    );
}

#[test]
fn title_events_update_tabs_and_only_active_window_metadata() {
    let mut manager = TabManager::new();
    let (background, _) = create(&mut manager);
    let (active, _) = create(&mut manager);

    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]2;background title\x07");
    let background_outcome = manager.process_output(background);
    assert!(background_outcome.title_changed);
    assert!(!background_outcome.active_title_changed);
    assert_eq!(manager.active_title(), Some("test-shell"));
    assert_eq!(manager.snapshots()[0].title, "background title");

    assert!(manager.activate(background).active_title_changed);
    assert_eq!(manager.active_title(), Some("background title"));
    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]2;\x1b\\");
    let reset = manager.process_output(background);
    assert!(reset.active_title_changed);
    assert_eq!(manager.active_title(), Some("test-shell"));
    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]2;test-shell\x07");
    let duplicate = manager.process_output(background);
    assert!(!duplicate.title_changed);
    assert!(!duplicate.active_title_changed);

    assert_eq!(
        manager.process_output(TabId(u64::MAX)),
        TabOutputOutcome::default()
    );
    assert_eq!(manager.active_id(), Some(background));
    assert_ne!(manager.active_id(), Some(active));
}
#[test]
fn working_directory_metadata_is_retained_per_tab_and_reports_real_changes() {
    let mut manager = TabManager::new();
    let (background, _) = create(&mut manager);
    let (active, _) = create(&mut manager);

    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]7;file://host/home/one\x07");
    let changed = manager.process_output(background);
    assert!(changed.metadata_changed);
    assert!(!changed.title_changed);
    assert!(!changed.active_title_changed);
    assert!(!changed.request_active_invalidation);
    let snapshots = manager.snapshots();
    let background_cwd = snapshots[0].working_directory().unwrap();
    assert_eq!(background_cwd.host(), Some("host"));
    assert_eq!(background_cwd.path(), "/home/one");
    assert_eq!(snapshots[1].working_directory(), None);

    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]7;file://host/home/one\x07\x1b]7;invalid\x07");
    assert!(!manager.process_output(background).metadata_changed);

    terminal(&manager, active)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]7;file:///active\x07");
    let active_changed = manager.process_output(active);
    assert!(active_changed.metadata_changed);
    assert!(!active_changed.title_changed);
    assert!(!active_changed.active_title_changed);
    let snapshots = manager.snapshots();
    let active_cwd = snapshots[1].working_directory().unwrap();
    assert_eq!(active_cwd.host(), None);
    assert_eq!(active_cwd.path(), "/active");

    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]7;\x07");
    assert!(manager.process_output(background).metadata_changed);
    assert_eq!(manager.snapshots()[0].working_directory, None);
    assert_eq!(
        manager.process_output(TabId(u64::MAX)),
        TabOutputOutcome::default()
    );
}

#[test]
fn shell_integration_metadata_is_retained_per_tab_and_reports_real_changes() {
    let mut manager = TabManager::new();
    let (background, _) = create(&mut manager);
    let (active, _) = create(&mut manager);

    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]133;A\x07");
    let changed = manager.process_output(background);
    assert!(changed.metadata_changed);
    assert!(!changed.title_changed);
    assert!(!changed.active_title_changed);
    assert!(!changed.request_active_invalidation);
    let snapshots = manager.snapshots();
    assert_eq!(
        snapshots[0].shell_integration(),
        Some(ShellIntegrationMarker::PromptStart)
    );
    assert_eq!(snapshots[1].shell_integration(), None);

    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]133;unknown\x07");
    assert!(!manager.process_output(background).metadata_changed);
    assert_eq!(
        manager.snapshots()[0].shell_integration(),
        Some(ShellIntegrationMarker::PromptStart)
    );

    // Oversized sequence leaves prior state unchanged
    let oversized = format!("\x1b]133;D;{}\x07", "9".repeat(4096));
    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(oversized.as_bytes());
    assert!(!manager.process_output(background).metadata_changed);
    assert_eq!(
        manager.snapshots()[0].shell_integration(),
        Some(ShellIntegrationMarker::PromptStart)
    );
    terminal(&manager, active)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]133;D;0\x07");
    let active_changed = manager.process_output(active);
    assert!(active_changed.metadata_changed);
    let snapshots = manager.snapshots();
    assert_eq!(
        snapshots[1].shell_integration(),
        Some(ShellIntegrationMarker::CommandFinished(Some(0)))
    );

    terminal(&manager, background)
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]133;\x07");
    assert!(manager.process_output(background).metadata_changed);
    assert_eq!(manager.snapshots()[0].shell_integration(), None);

    // RIS clears shell integration
    terminal(&manager, active)
        .lock()
        .unwrap()
        .put_bytes(b"\x1bc");
    assert!(manager.process_output(active).metadata_changed);
    assert_eq!(manager.snapshots()[1].shell_integration(), None);
}

#[test]
fn inactive_output_marks_unread_only_when_bytes_are_ingested() {
    use std::io::Cursor;
    use std::time::Duration;

    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut manager = TabManager::new();
    manager
        .create_tab(|_, draw_id| {
            #[allow(clippy::arc_with_non_send_sync)]
            let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                4,
                20,
                Cursor::new(b"background".to_vec()),
                std::io::sink(),
                move || wake_tx.send(()).is_ok(),
            )));
            let bridge = TerminalWidgetBridge::new(
                draw_id,
                Arc::clone(&terminal),
                Arc::new(AtomicBool::new(false)),
            );
            Ok(TerminalTabResources::new(
                terminal,
                bridge,
                "background-shell",
            ))
        })
        .unwrap();
    let background = manager.active_id().unwrap();
    create(&mut manager);
    wake_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("reader should announce queued output");

    assert!(manager.process_output(background).unread_changed);
    assert!(!manager.process_output(background).unread_changed);
    assert!(
        manager
            .snapshots()
            .iter()
            .find(|tab| tab.id == background)
            .unwrap()
            .unread
    );
}

#[test]
fn close_active_selects_right_then_left_and_final_requests_window_close() {
    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    let (b, _) = create(&mut manager);
    let (c, _) = create(&mut manager);
    manager.activate(b);

    assert_eq!(manager.neighbor_for_close(b), Some(c));
    assert!(manager.close(b).active_bridge_changed);
    assert_eq!(manager.active_id(), Some(c));
    assert_eq!(manager.neighbor_for_close(c), Some(a));
    assert!(manager.close(c).active_bridge_changed);
    assert_eq!(manager.active_id(), Some(a));
    assert_eq!(manager.neighbor_for_close(a), None);
    assert_eq!(manager.neighbor_for_close(TabId(u64::MAX)), None);
    let final_outcome = manager.close(a);
    assert!(final_outcome.close_window);
    assert_eq!(manager.active_id(), None);
}

#[test]
fn close_inactive_requests_redraw_while_stale_ids_are_noops() {
    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    let (b, _) = create(&mut manager);
    assert_eq!(
        manager.close(a),
        TabActionOutcome {
            request_redraw: true,
            ..TabActionOutcome::default()
        }
    );
    assert_eq!(manager.active_id(), Some(b));
    assert_eq!(manager.close(a), TabActionOutcome::default());
    assert_eq!(manager.process_output(a), TabOutputOutcome::default());
}

#[test]
fn terminals_keep_independent_screen_state() {
    let mut manager = TabManager::new();
    let (a, _) = create(&mut manager);
    let terminal_a = manager
        .tabs
        .iter()
        .find(|tab| tab.id == a)
        .unwrap()
        .terminal
        .clone();
    terminal_a.lock().unwrap().process_output(b"alpha");
    let (b, _) = create(&mut manager);
    let terminal_b = manager
        .tabs
        .iter()
        .find(|tab| tab.id == b)
        .unwrap()
        .terminal
        .clone();
    terminal_b.lock().unwrap().process_output(b"beta");

    assert!(terminal_a.lock().unwrap().row_text(0).contains("alpha"));
    assert!(!terminal_a.lock().unwrap().row_text(0).contains("beta"));
    assert!(terminal_b.lock().unwrap().row_text(0).contains("beta"));
}

#[test]
fn closing_tab_releases_terminal_and_bridge_ownership() {
    let mut manager = TabManager::new();
    let mut weak = None;
    manager
        .create_tab(|_, draw_id| {
            let made = resources(draw_id);
            weak = Some(Arc::downgrade(&made.terminal));
            Ok(made)
        })
        .unwrap();
    let id = manager.active_id().unwrap();

    assert!(manager.close(id).close_window);
    assert!(weak.unwrap().upgrade().is_none());
}

#[test]
fn runtime_mount_and_schedule_follow_only_the_selected_bridge() {
    use harbor_widget::scene::primitive::Primitive;
    use harbor_widget::widgets::sized_box::SizedBox;

    fn mounted_external_ids(runtime: &harbor_widget::runtime::Runtime) -> Vec<ExternalDrawId> {
        runtime
            .pending_delta()
            .expect("root update produces a scene delta")
            .added
            .iter()
            .filter_map(|item| match item.primitive {
                Primitive::External { draw, .. } => Some(draw),
                _ => None,
            })
            .collect()
    }

    let mut manager = TabManager::new();
    let (a, draw_a) = create(&mut manager);
    manager
        .tabs
        .iter()
        .find(|tab| tab.id == a)
        .unwrap()
        .terminal
        .lock()
        .unwrap()
        .process_output(b"\x1b[?2026hhidden");
    let (b, draw_b) = create(&mut manager);
    let mut runtime = harbor_widget::runtime::Runtime::new();
    runtime.set_root(crate::terminal_view::terminal_widget(
        manager.active_bridge().unwrap(),
    ));
    let active_b = runtime.update(std::time::Instant::now());
    assert_eq!(mounted_external_ids(&runtime), [draw_b]);
    assert!(!active_b.has_deferred_externals);

    manager.activate(a);
    runtime.set_root(crate::terminal_view::terminal_widget(
        manager.active_bridge().unwrap(),
    ));
    let active_a = runtime.update(std::time::Instant::now());
    assert_eq!(mounted_external_ids(&runtime), [draw_a]);
    assert!(active_a.has_deferred_externals);

    assert!(manager.close(a).active_bridge_changed);
    runtime.set_root(crate::terminal_view::terminal_widget(
        manager.active_bridge().unwrap(),
    ));
    let back_to_b = runtime.update(std::time::Instant::now());
    assert_eq!(mounted_external_ids(&runtime), [draw_b]);
    assert!(!back_to_b.has_deferred_externals);

    assert!(manager.close(b).close_window);
    runtime.set_root(SizedBox::new(harbor_widget::layout::Size::ZERO).unwrap());
    runtime.update(std::time::Instant::now());
    assert!(!runtime.has_external_draws());
}

#[test]
fn resize_broadcast_updates_all_live_tabs_and_deduplicates_grid() {
    let mut manager = TabManager::new();
    create(&mut manager);
    create(&mut manager);
    let requested = TerminalSize { rows: 12, cols: 42 };

    assert!(!manager.resize_all_if_changed(requested));
    assert_eq!(manager.last_broadcast_size(), None);
    assert_eq!(manager.advance_resizes().attempted, 2);
    assert_eq!(manager.last_broadcast_size(), Some(requested));
    for tab in &manager.tabs {
        let snapshot = tab.terminal.lock().unwrap().drain_and_snapshot();
        assert_eq!((snapshot.rows, snapshot.cols), (12, 42));
    }
    assert!(!manager.resize_all_if_changed(requested));
    assert!(!manager.resize_all_if_changed(TerminalSize { rows: 0, cols: 42 }));
    assert_eq!(manager.last_broadcast_size(), Some(requested));
}

fn dimensions(manager: &TabManager) -> Vec<TerminalSize> {
    manager
        .tabs
        .iter()
        .map(|tab| {
            let terminal = tab.terminal.lock().unwrap();
            TerminalSize {
                rows: terminal.screen().rows(),
                cols: terminal.screen().cols(),
            }
        })
        .collect()
}

fn settle_resize(manager: &mut TabManager) {
    for _ in 0..manager.tabs.len() + 1 {
        if !manager.advance_resizes().ready_remaining {
            return;
        }
    }
    panic!("ready resize backlog failed to converge");
}

#[test]
fn resize_burst_yields_with_one_shared_bound_despite_output_input_and_draws() {
    use harbor_terminal::{RenderTarget, TerminalEvent, TerminalFocusEvent, TextMetrics};
    let mut manager = TabManager::new();
    for _ in 0..7 {
        create(&mut manager);
    }
    let target = TerminalSize { rows: 6, cols: 30 };
    let initial = dimensions(&manager);
    manager.resize_all_if_changed(target);
    let metrics = TextMetrics {
        cell_width: 8.0,
        line_height: 16.0,
        ascent: 12.0,
        underline_position: 14.0,
        underline_thickness: 1.0,
        strikethrough_position: 8.0,
        strikethrough_thickness: 1.0,
    };
    // None of these entry paths may create its own transaction budget.
    for (id, terminal, bridge) in manager.configuration_targets() {
        manager.process_output(id);
        bridge.is_alt_screen();
        let mut terminal = terminal.lock().unwrap();
        terminal.frame_demand(std::time::Instant::now());
        terminal
            .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
            .unwrap();
        terminal.prepare_render_frame(
            RenderTarget::new((0.0, 0.0), (800, 600), (800, 600)),
            &metrics,
            std::time::Instant::now(),
        );
    }
    manager.resize_all_if_changed(target);
    assert_eq!(
        dimensions(&manager),
        initial,
        "fan-out/callbacks must not commit"
    );
    let batch = manager.advance_resizes();
    assert_eq!(batch.attempted, RESIZE_BATCH_LIMIT);
    assert!(batch.active_changed && batch.ready_remaining);
    assert_eq!(
        dimensions(&manager)
            .iter()
            .filter(|s| **s == target)
            .count(),
        RESIZE_BATCH_LIMIT
    );
    assert_eq!(manager.last_broadcast_size(), None);
    // An ordinary model/UI action runs before the entire ready backlog.
    let next_active = manager.tabs[3].id;
    assert!(manager.activate(next_active).active_bridge_changed);
    manager.advance_resizes();
    assert_eq!(
        terminal(&manager, next_active)
            .lock()
            .unwrap()
            .screen()
            .cols(),
        30
    );
    settle_resize(&mut manager); // no additional bytes, draws or activation
    assert!(dimensions(&manager).iter().all(|s| *s == target));
    assert_eq!(manager.last_broadcast_size(), Some(target));
    assert!(manager.snapshots().iter().all(|tab| !tab.unread));
    assert_eq!(manager.advance_resizes().attempted, 0);
}

#[test]
fn resize_active_priority_still_rotates_background_under_continuous_targets() {
    let mut manager = TabManager::new();
    for _ in 0..8 {
        create(&mut manager);
    }
    let mut served = std::collections::HashSet::new();
    for turn in 0..8 {
        let target = TerminalSize {
            rows: 4,
            cols: 21 + turn,
        };
        manager.resize_all_if_changed(target);
        let batch = manager.advance_resizes();
        assert_eq!(batch.attempted, RESIZE_BATCH_LIMIT);
        assert_eq!(*dimensions(&manager).last().unwrap(), target);
        for (tab, size) in manager.tabs.iter().zip(dimensions(&manager)) {
            if size == target {
                served.insert(tab.id);
            }
        }
    }
    assert_eq!(
        served.len(),
        manager.tabs.len(),
        "active changes cannot starve background"
    );
}

#[test]
fn resize_return_to_completed_a_cancels_b_with_and_without_partial_commits() {
    for partially_commit in [false, true] {
        let mut manager = TabManager::new();
        for _ in 0..6 {
            create(&mut manager);
        }
        let a = TerminalSize { rows: 4, cols: 20 };
        let b = TerminalSize { rows: 5, cols: 31 };
        assert!(manager.resize_all_if_changed(a));
        manager.resize_all_if_changed(b);
        assert_eq!(manager.last_broadcast_size(), None);
        if partially_commit {
            manager.advance_resizes();
        }
        manager.resize_all_if_changed(a);
        settle_resize(&mut manager);
        assert_eq!(manager.last_broadcast_size(), Some(a));
        assert!(dimensions(&manager).iter().all(|s| *s == a));
        assert!(
            manager
                .tabs
                .iter()
                .all(|tab| !tab.terminal.lock().unwrap().resize_pending())
        );
        assert_eq!(manager.advance_resizes().attempted, 0);
    }
}

#[test]
fn resize_failure_attempts_are_bounded_retryable_and_closure_cancels_queued_work() {
    let mut manager = TabManager::new();
    for _ in 0..7 {
        create(&mut manager);
    }
    let impossible = TerminalSize {
        rows: usize::MAX,
        cols: usize::MAX,
    };
    manager.resize_all_if_changed(impossible);
    for _ in 0..3 {
        let batch = manager.advance_resizes();
        assert_eq!(batch.attempted, RESIZE_BATCH_LIMIT);
        assert!(batch.ready_remaining);
        assert_eq!(manager.last_broadcast_size(), None);
        assert!(dimensions(&manager).iter().all(|s| s.cols == 20));
        manager.resize_all_if_changed(impossible);
    }
    let target = TerminalSize { rows: 5, cols: 28 };
    manager.resize_all_if_changed(target);
    let doomed = manager.tabs[2].id;
    let retained = terminal(&manager, doomed);
    manager.close(doomed);
    assert!(!retained.lock().unwrap().resize_pending());
    let (replacement, _) = create(&mut manager);
    assert_ne!(replacement, doomed);
    manager.resize_all_if_changed(target);
    settle_resize(&mut manager);
    assert_eq!(
        retained.lock().unwrap().screen().cols(),
        20,
        "obsolete tab work never commits"
    );
    assert!(dimensions(&manager).iter().all(|s| *s == target));
    assert_eq!(manager.last_broadcast_size(), Some(target));
}

#[test]
fn resize_partial_request_failure_never_claims_aggregate_completion() {
    let mut manager = TabManager::new();
    for _ in 0..4 {
        create(&mut manager);
    }
    manager
        .create_tab(|_, draw| {
            // This reader has no interrupt capability: acquisition must reject,
            // while other sessions remain able to finish the shared allocation.
            #[allow(clippy::arc_with_non_send_sync)]
            let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                4,
                20,
                std::io::empty(),
                std::io::sink(),
                || true,
            )));
            let bridge =
                TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
            Ok(TerminalTabResources::new(
                terminal,
                bridge,
                "unsupported-reader",
            ))
        })
        .unwrap();
    let failed = manager.active_id().unwrap();
    let target = TerminalSize { rows: 5, cols: 30 };
    manager.resize_all_if_changed(target);
    settle_resize(&mut manager);
    assert_eq!(
        dimensions(&manager)
            .iter()
            .filter(|size| **size == target)
            .count(),
        4
    );
    assert_eq!(manager.last_broadcast_size(), None);
    manager.resize_all_if_changed(target);
    assert_eq!(
        manager.advance_resizes().attempted,
        0,
        "successful sessions are not retried"
    );
    assert_eq!(manager.last_broadcast_size(), None);
    manager.close(failed);
    manager.advance_resizes();
    assert_eq!(manager.last_broadcast_size(), Some(target));
}

#[test]
fn failed_factory_burns_ids_without_mutating_tabs() {
    let mut manager = TabManager::new();
    let failure = manager.create_tab(|_, _| Err(anyhow!("spawn failed")));
    assert!(failure.is_err());
    assert!(manager.snapshots().is_empty());

    let (id, draw_id) = create(&mut manager);
    assert_eq!(id.get(), 2);
    assert_eq!(draw_id, 2);
}

#[test]
fn exhausted_id_space_returns_error_instead_of_wrapping() {
    let mut manager = TabManager {
        next_tab_id: u64::MAX,
        ..TabManager::new()
    };
    assert!(
        manager
            .create_tab(|_, draw_id| Ok(resources(draw_id)))
            .is_err()
    );
    assert!(manager.tabs.is_empty());
}
