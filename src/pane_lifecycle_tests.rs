//! Windowless Host/model + real ConPTY lifecycle; not native-desktop acceptance.
use super::*;
use harbor_app::tab_manager::PaneBinding;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

struct NativeResources {
    resources: TerminalTabResources,
    receipt: harbor_pty::ShutdownReceipt,
}

fn native(
    draw: ExternalDrawId,
    source: SessionId,
    events: mpsc::Sender<SessionId>,
) -> NativeResources {
    let command = ShellCommand {
        program: Some("cmd.exe".into()),
        args: vec!["/D".into(), "/Q".into()],
        ..Default::default()
    };
    let endpoints =
        PtyEndpoints::spawn_shell(harbor_pty::TerminalSize { rows: 12, cols: 80 }, &command)
            .unwrap();
    let receipt = endpoints.shutdown_receipt();
    endpoints
        .observe_exit(move || {
            let _ = events.send(source);
        })
        .unwrap();
    let terminal = Terminal::new_headless(12, 80)
        .start_session_from_endpoints(endpoints, || true)
        .unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(terminal));
    let bridge =
        TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
    NativeResources {
        resources: TerminalTabResources::new(terminal, bridge, "cmd"),
        receipt,
    }
}
use std::sync::atomic::AtomicBool;

fn wait_receipt(receipt: &harbor_pty::ShutdownReceipt) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !receipt.is_complete() {
        assert!(
            Instant::now() < deadline,
            "native retirement did not complete"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn shell(tabs: &TabManager, binding: PaneBinding) -> Arc<Mutex<Terminal>> {
    tabs.session_terminal(binding.session).unwrap()
}
fn send(terminal: &Arc<Mutex<Terminal>>, command: &str) {
    terminal
        .lock()
        .unwrap()
        .write_pty(command.as_bytes())
        .unwrap();
}

#[test]
#[cfg(windows)]
#[ignore = "pty: Windows ConPTY with dedicated local cmd children"]
fn exact_shell_exit_collapses_only_its_pane_and_retires_pending_resize_once() {
    let (tx, rx) = mpsc::channel();
    let mut tabs = TabManager::new();
    let mut receipts = Vec::new();
    tabs.create_tab(|source, draw| {
        let native = native(draw, source, tx.clone());
        receipts.push(native.receipt);
        Ok(native.resources)
    })
    .unwrap();
    let a = tabs.active_binding().unwrap();
    let (b, draw) = tabs.reserve_split_ids(a).unwrap();
    let second = native(draw, b.session, tx.clone());
    receipts.push(second.receipt);
    let retained_bridge = second.resources.bridge().clone();
    tabs.attach_split(
        a,
        b,
        draw,
        second.resources,
        harbor_widget::SplitAxis::Horizontal,
    )
    .unwrap();
    let first = shell(&tabs, a);
    let second = shell(&tabs, b);
    let first_identity = first.lock().unwrap().content_version().session;
    let root = tempfile::tempdir().unwrap();
    let report = root.path().join("foreground.txt");
    send(
        &second,
        &format!(
            "cmd /D /Q /C echo foreground-completed>\"{}\"\r",
            report.display()
        ),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while !std::fs::read_to_string(&report).is_ok_and(|text| text.trim() == "foreground-completed")
    {
        tabs.process_output(b.session);
        assert!(Instant::now() < deadline, "foreground child did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        std::fs::read_to_string(&report).unwrap().trim(),
        "foreground-completed"
    );
    assert!(
        rx.try_recv().is_err(),
        "foreground completion is not session exit"
    );
    assert_eq!(
        tabs.close_exited_session(b.session),
        TabActionOutcome::default()
    );
    // OSC 133 command completion likewise cannot close a still-live shell.
    second.lock().unwrap().process_output(b"\x1b]133;D;0\x07");
    tabs.process_output(b.session);
    assert_eq!(
        tabs.close_exited_session(b.session),
        TabActionOutcome::default()
    );

    tabs.focus_pane(b);
    second
        .lock()
        .unwrap()
        .process_output(b"\x1b]52;c;c291cmNl\x07");
    let request = tabs
        .process_output(b.session)
        .clipboard_write
        .expect("source clipboard request");
    assert_eq!(request.source, b.session);
    let mut policy =
        crate::clipboard::ClipboardPolicy::new(harbor_config::Osc52WritePolicy::Confirm);
    policy.retain(request);
    let _ = second
        .lock()
        .unwrap()
        .request_resize(TerminalSize { rows: 20, cols: 91 })
        .unwrap();
    send(&second, "exit\r");
    assert_eq!(rx.recv_timeout(Duration::from_secs(15)).unwrap(), b.session);
    assert!(second.lock().unwrap().shell_process_exited());
    assert!(
        !tabs.active_live(b.session),
        "native exit revokes permission even before model reduction"
    );
    tabs.focus_pane(a);
    assert!(
        policy.resolve(true, tabs.active_live(b.session)).is_none(),
        "pending OSC52 must not take the survivor's foreground permission"
    );
    let closed = tabs.close_exited_session(b.session);
    assert!(!closed.close_window && !closed.active_bridge_changed);
    assert!(!tabs.contains_binding(b));
    assert!(tabs.contains_binding(a));
    assert_eq!(tabs.active_binding(), Some(a));
    assert_eq!(
        first.lock().unwrap().content_version().session,
        first_identity
    );
    assert!(!first.lock().unwrap().shell_process_exited());
    assert!(second.lock().unwrap().is_session_closed());
    assert!(!second.lock().unwrap().resize_pending());
    assert_eq!(tabs.close_pane(b), TabActionOutcome::default());
    assert_eq!(
        tabs.close_exited_session(b.session),
        TabActionOutcome::default()
    );
    assert_eq!(tabs.process_output(b.session), Default::default());
    wait_receipt(&receipts[1]);
    drop(retained_bridge); // retained projection never prolongs native ownership.

    send(&first, "exit\r");
    assert_eq!(rx.recv_timeout(Duration::from_secs(15)).unwrap(), a.session);
    let last = tabs.close_exited_session(a.session);
    assert!(last.close_window && tabs.snapshots().is_empty());
    assert_eq!(
        tabs.close_exited_session(a.session),
        TabActionOutcome::default()
    );
    wait_receipt(&receipts[0]);
}

#[test]
#[cfg(windows)]
#[ignore = "pty: Windows ConPTY with dedicated local cmd children"]
fn explicit_close_releases_exit_observer_and_late_exit_cannot_close_replacement() {
    let (tx, rx) = mpsc::channel();
    let mut tabs = TabManager::new();
    let mut receipt = None;
    tabs.create_tab(|id, draw| {
        let native = native(draw, id, tx.clone());
        receipt = Some(native.receipt);
        Ok(native.resources)
    })
    .unwrap();
    let old = tabs.active_binding().unwrap();
    let engine = shell(&tabs, old);
    let _ = engine
        .lock()
        .unwrap()
        .request_resize(TerminalSize { rows: 19, cols: 87 })
        .unwrap();
    assert!(tabs.close_pane(old).close_window);
    let mut replacement_receipt = None;
    tabs.create_tab(|id, draw| {
        let native = native(draw, id, tx.clone());
        replacement_receipt = Some(native.receipt);
        Ok(native.resources)
    })
    .unwrap();
    let replacement = tabs.active_binding().unwrap();
    assert_ne!(old.session, replacement.session);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(15)).unwrap(),
        old.session
    );
    assert_eq!(
        tabs.close_exited_session(old.session),
        TabActionOutcome::default()
    );
    assert_eq!(tabs.process_output(old.session), Default::default());
    assert!(tabs.contains_binding(replacement));
    assert!(
        !shell(&tabs, replacement)
            .lock()
            .unwrap()
            .shell_process_exited()
    );
    assert!(engine.lock().unwrap().is_session_closed());
    wait_receipt(receipt.as_ref().unwrap());
    tabs.close_pane(replacement);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(15)).unwrap(),
        replacement.session
    );
    wait_receipt(replacement_receipt.as_ref().unwrap());
}

#[test]
#[ignore = "pty: Windows ConPTY with three dedicated local cmd children"]
fn mounted_native_mixed_panes_route_markers_under_output_resize_and_close() {
    use crate::split_launch::{AttemptLaunch, Cleanup, LaunchKind, SplitOperation, SplitPoll};
    use harbor_widget::{
        SplitAxis,
        input::event::{KeyboardEvent, PointerButton, PointerEvent, PointerPhase, UiEvent},
        layout::Point,
        renderer::Viewport,
        runtime::Runtime,
    };
    let (tx, rx) = mpsc::channel();
    let mut tabs = TabManager::new();
    let mut receipts = Vec::new();
    tabs.create_tab(|id, draw| {
        let child = native(draw, id, tx.clone());
        receipts.push(child.receipt);
        Ok(child.resources)
    })
    .unwrap();
    let a = tabs.active_binding().unwrap();
    let command = ShellCommand {
        program: Some("cmd.exe".into()),
        args: vec!["/D".into(), "/Q".into()],
        ..Default::default()
    };
    // Consume the same operation-qualified default-readiness + atomic model
    // attachment boundary as production; actual desktop Host admission is T0005.
    let mut bindings = vec![a];
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        let mut operation = SplitOperation::begin(&mut tabs, command.clone()).unwrap();
        let (target, draw) = operation.target();
        let poll = operation.poll(&tabs, Instant::now(), |actual, probe| {
            assert_eq!(actual, &command);
            assert!(probe.is_none());
            let child = native(draw, target.session, tx.clone());
            receipts.push(child.receipt.clone());
            AttemptLaunch {
                resources: Ok(child.resources),
                cleanup: Some(Cleanup::Native(child.receipt)),
            }
        });
        let SplitPoll::Ready(ready) = poll else {
            panic!("default readiness");
        };
        let prepared = ready.claim(&tabs, operation.id()).unwrap();
        assert_eq!(prepared.kind, LaunchKind::DefaultUnknownContext);
        tabs.attach_split(
            prepared.source,
            prepared.target,
            prepared.draw,
            prepared.resources,
            axis,
        )
        .unwrap();
        tabs.focus_pane(target);
        bindings.push(target);
    }
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    ui.sync_panes(&tabs, 1000.0);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(1000, 600, 1.0));
    runtime.set_root(harbor_app::workspace::tab_workspace(ui.clone(), false));
    runtime.update(Instant::now());
    let root = tempfile::tempdir().unwrap();
    let reports: Vec<_> = (0..3)
        .map(|index| root.path().join(format!("pane-{index}.txt")))
        .collect();
    let metrics = TextMetrics {
        cell_width: 8.0,
        line_height: 16.0,
        ..harbor_widget::runtime::DEFAULT_TEXT_METRICS
    };
    ui.publish_pane_input_metrics(1.0, (1000, 600), metrics);
    tabs.observe_pane_allocations(&ui.pane_allocations(), 1.0, (1000, 600), &metrics);
    for (index, binding) in bindings.iter().enumerate() {
        let rect = ui
            .pane_allocations()
            .into_iter()
            .find(|allocation| allocation.binding == *binding)
            .unwrap()
            .rect;
        let p = Point::new(rect.min.x + 24.0, rect.min.y + 24.0);
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            p,
            PointerPhase::Down,
            PointerButton::Left,
            71,
        )));
        for request in ui.drain_actions() {
            if let harbor_app::command::AppCommand::Pane(command) = request.command {
                tabs.execute_pane_command(command);
            }
        }
        ui.sync_panes(&tabs, 1000.0);
        runtime.update(Instant::now());
        assert_eq!(tabs.active_binding(), Some(*binding));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(format!(
            "echo MARKER_{index}>\"{}\"\rfor /L %i in (1,1,200) do @echo PRODUCER_{index}_%i\r",
            reports[index].display()
        ))));
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            p,
            PointerPhase::Up,
            PointerButton::Left,
            71,
        )));
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        for binding in &bindings {
            tabs.process_output(binding.session);
        }
        let batch = tabs.advance_resizes();
        assert!(batch.attempted <= 2);
        let reports_ready = reports.iter().enumerate().all(|(index, path)| {
            std::fs::read_to_string(path).is_ok_and(|text| text.trim() == format!("MARKER_{index}"))
        });
        let producers_ready = bindings.iter().enumerate().all(|(index, binding)| {
            let terminal = shell(&tabs, *binding);
            let terminal = terminal.lock().unwrap();
            let screen = terminal.screen();
            let text = screen.selected_text(harbor_terminal::SelectionBounds {
                start_row: screen.history_start(),
                start_col: 0,
                end_row: screen.history_start() + (screen.scroll_count() + screen.rows()) as u64
                    - 1,
                end_col: screen.cols() - 1,
            });
            text.contains(&format!("PRODUCER_{index}_200"))
                && (0..3)
                    .filter(|other| *other != index)
                    .all(|other| !text.contains(&format!("PRODUCER_{other}_200")))
        });
        if reports_ready
            && producers_ready
            && !batch.ready_remaining
            && bindings
                .iter()
                .all(|binding| !shell(&tabs, *binding).lock().unwrap().resize_pending())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native marker/resize did not converge"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let survivor = shell(&tabs, a);
    let identity = survivor.lock().unwrap().content_version().session;
    assert!(!tabs.close_pane(bindings[1]).close_window);
    ui.sync_panes(&tabs, 1000.0);
    runtime.request_focus(&ui.terminal_focus());
    runtime.update(Instant::now());
    assert_eq!(survivor.lock().unwrap().content_version().session, identity);
    assert!(!survivor.lock().unwrap().shell_process_exited());
    wait_receipt(&receipts[1]);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(15)).unwrap(),
        bindings[1].session
    );
    assert_eq!(
        tabs.close_exited_session(bindings[1].session),
        TabActionOutcome::default()
    );
    assert!(tabs.close_active().close_window);
    for receipt in &receipts {
        wait_receipt(receipt);
    }
}
