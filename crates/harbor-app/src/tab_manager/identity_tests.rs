//! Identity/lifetime contracts, deliberately using non-coincident numeric IDs.
use super::*;
use std::sync::atomic::AtomicBool;

fn create(manager: &mut TabManager) -> PaneBinding {
    manager
        .create_tab(|_, draw| {
            #[allow(clippy::arc_with_non_send_sync)]
            let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
            let bridge =
                TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
            Ok(TerminalTabResources::new(
                terminal,
                bridge,
                "identity-shell",
            ))
        })
        .unwrap();
    manager.active_binding().unwrap()
}

fn manager() -> TabManager {
    TabManager {
        next_tab_id: 11,
        next_pane_id: 101,
        next_session_id: 1001,
        next_draw_id: ExternalDrawId::new(10001),
        ..TabManager::new()
    }
}

#[test]
fn qualified_sources_survive_focus_changes_but_not_removal_or_replacement() {
    let mut manager = manager();
    let a = create(&mut manager);
    let terminal = manager.session_terminal(a.session).unwrap();
    let version = terminal.lock().unwrap().content_version();
    let b = create(&mut manager);
    terminal
        .lock()
        .unwrap()
        .put_bytes(b"\x1b]2;original\x07\x1b]7;file:///original\x07\x1b]52;c;c291cmNl\x07");
    let output = manager.process_output(a.session);
    assert_eq!(output.clipboard_write.unwrap().source, a.session);
    assert!(!output.active_title_changed);
    assert_eq!(manager.source_label(a.session), "original");
    assert_eq!(
        manager.session_working_directory(a.session).unwrap().path(),
        "/original"
    );
    assert!(manager.contains_binding(a));
    assert_eq!(manager.binding_for_session(a.session), Some(a));
    assert!(!manager.contains_binding(PaneBinding {
        session: b.session,
        ..a
    }));
    assert!(manager.session_terminal(SessionId(a.workspace.0)).is_none());
    assert_eq!(
        manager.process_output(SessionId(a.workspace.0)),
        TabOutputOutcome::default()
    );
    assert!(Arc::ptr_eq(
        &terminal,
        &manager.session_terminal(a.session).unwrap()
    ));
    // Host routing migration does not replace the engine read-model identity.
    assert_eq!(
        terminal.lock().unwrap().content_version().session,
        version.session
    );

    manager.close(a.workspace);
    assert!(!manager.contains_binding(a));
    assert!(manager.session_terminal(a.session).is_none());
    let c = create(&mut manager);
    assert_ne!(a.session, c.session);
    terminal.lock().unwrap().put_bytes(b"\x1b]2;late\x07");
    assert_eq!(
        manager.process_output(a.session),
        TabOutputOutcome::default()
    );
    assert_eq!(manager.active_title(), Some("identity-shell"));
    assert!(manager.active_live(c.session));
    assert!(manager.contains_binding(b));
}

#[test]
fn failed_attempt_burns_every_identity_and_cannot_route_a_late_wake() {
    let mut manager = manager();
    let mut failed_source = None;
    let result = manager.create_tab(|source, _| {
        failed_source = Some(source);
        Err(anyhow!("synthetic spawn failure"))
    });
    assert!(result.is_err());
    assert!(manager.configuration_targets().is_empty());
    let binding = create(&mut manager);
    assert_eq!(binding.workspace, TabId(12));
    assert_eq!(binding.pane, PaneId(102));
    assert_eq!(binding.session, SessionId(1002));
    assert_eq!(
        manager.active_bridge().unwrap().draw_id(),
        ExternalDrawId::new(10002)
    );
    assert_eq!(
        manager.process_output(failed_source.unwrap()),
        TabOutputOutcome::default()
    );
    assert!(manager.active_live(binding.session));
}

#[test]
fn configuration_and_foreground_eligibility_use_sessions_not_workspace_count() {
    let mut manager = manager();
    let a = create(&mut manager);
    let b = create(&mut manager);
    // Model-only fixture for the next tickets, NOT production split admission.
    let second = manager.tabs.pop().unwrap();
    manager.tabs[0].panes.extend(second.panes);
    manager.tabs[0].focused = b.pane;
    manager.active = Some(a.workspace);
    let bound_b = PaneBinding {
        workspace: a.workspace,
        ..b
    };
    assert_eq!(manager.active_binding(), Some(bound_b));
    assert!(manager.contains_binding(a));
    assert!(manager.contains_binding(bound_b));
    assert!(!manager.contains_binding(b));
    assert_eq!(
        manager
            .configuration_targets()
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<Vec<_>>(),
        [a.session, b.session]
    );
    assert!(!manager.active_live(a.session));
    assert!(manager.active_live(b.session));
    let a_terminal = manager.session_terminal(a.session).unwrap();
    let b_terminal = manager.session_terminal(b.session).unwrap();
    assert!(manager.close(a.workspace).close_window);
    assert!(manager.configuration_targets().is_empty());
    assert!(a_terminal.lock().unwrap().is_session_closed());
    assert!(b_terminal.lock().unwrap().is_session_closed());
    assert_eq!(manager.close(a.workspace), TabActionOutcome::default());
}

#[test]
fn retirement_drops_io_even_with_retained_ui_and_engine_clones() {
    use std::io::{Cursor, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Writer(Arc<AtomicUsize>);
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Drop for Writer {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let mut manager = manager();
    manager
        .create_tab(|_, draw| {
            #[allow(clippy::arc_with_non_send_sync)]
            let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                4,
                20,
                Cursor::new(Vec::<u8>::new()),
                Writer(drops.clone()),
                || true,
            )));
            let bridge =
                TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
            Ok(TerminalTabResources::new(terminal, bridge, "io"))
        })
        .unwrap();
    let binding = manager.active_binding().unwrap();
    let bridge = manager.active_bridge().unwrap();
    let terminal = manager.session_terminal(binding.session).unwrap();
    let weak = Arc::downgrade(&terminal);
    manager.close(binding.workspace);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "UI references must not own IO shutdown"
    );
    assert!(terminal.lock().unwrap().is_session_closed());
    manager.close(binding.workspace);
    terminal.lock().unwrap().retire_session();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(bridge);
    drop(terminal);
    assert!(weak.upgrade().is_none());
}

#[test]
fn failed_resource_validation_and_owner_drop_retire_retained_clones() {
    let mut manager = manager();
    let binding = create(&mut manager);
    let survivor = manager.session_terminal(binding.session).unwrap();
    let mut failed = None;
    assert!(
        manager
            .create_tab(|_, draw| {
                #[allow(clippy::arc_with_non_send_sync)]
                let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
                failed = Some(terminal.clone());
                let bridge = TerminalWidgetBridge::new(
                    draw.checked_add(1).unwrap(),
                    terminal.clone(),
                    Arc::new(AtomicBool::new(false)),
                );
                Ok(TerminalTabResources::new(terminal, bridge, "invalid"))
            })
            .is_err()
    );
    assert!(failed.unwrap().lock().unwrap().is_session_closed());
    assert!(manager.active_live(binding.session));
    let bridge = manager.active_bridge().unwrap();
    drop(manager);
    assert!(survivor.lock().unwrap().is_session_closed());
    drop(bridge);
}
