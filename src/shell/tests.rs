#[path = "tests/architecture.rs"]
mod architecture;
mod configuration_notice_tests;

use super::*;

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn startup_terminal_focus_routes_the_first_key_without_a_native_focus_event() {
    use harbor_app::{tab_manager::TerminalTabResources, terminal_view::TerminalWidgetBridge};
    use harbor_widget::{
        input::event::{Key, KeyboardEvent, UiEvent},
        renderer::Viewport,
        runtime::Runtime,
    };
    use std::sync::Mutex;

    let pty = tempfile::NamedTempFile::new().unwrap();
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| {
        let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            8,
            40,
            std::io::empty(),
            pty.reopen().unwrap(),
            || true,
        )));
        let bridge =
            TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
        Ok(TerminalTabResources::new(terminal, bridge, "startup"))
    })
    .unwrap();
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    ui.publish_panes(&tabs);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(1000, 500, 1.0));
    runtime.set_root(harbor_app::workspace::tab_workspace(ui.clone(), false));
    runtime.update(Instant::now());

    // On Windows the main HWND may already have focus without a Focused(true)
    // event. Route the first key using the same logical target as Shell startup.
    runtime.request_focus(&ui.terminal_focus());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('x'),
        modifiers: Default::default(),
    }));
    assert_eq!(std::fs::read(pty.path()).unwrap(), b"x");
}

#[test]
fn shared_gpu_exit_cancels_pending_actions_before_exit() {
    use harbor_widget::winit::{GpuFault, GpuFaultKind};
    for kind in [
        GpuFaultKind::OutOfMemory,
        GpuFaultKind::DeviceLost,
        GpuFaultKind::Validation,
        GpuFaultKind::Internal,
    ] {
        let gate = Arc::new(AtomicBool::new(true));
        let mut confirmations =
            ConfirmationCoordinator::new(gate.clone(), harbor_config::Osc52WritePolicy::Confirm);
        confirmations.retain_clipboard_for_test(
            1,
            harbor_app::tab_manager::SessionClipboardWrite {
                source: harbor_app::tab_manager::SessionId(7),
                contents: harbor_terminal::ClipboardWrite::new("never approve".into()).unwrap(),
            },
        );
        let mut exits = 0;
        apply_gpu_fatal_policy(
            &GpuFault {
                kind,
                message: "policy sentinel".into(),
            },
            &mut confirmations,
            || {
                assert!(!gate.load(std::sync::atomic::Ordering::Acquire));
                exits += 1;
            },
        );
        assert_eq!(exits, 1);
        assert!(!confirmations.is_active());
        assert!(confirmations.clipboard_generation().is_none());
    }
}

/// Keep the synthetic reader live until the test releases it; EOF retires the session.
struct HeldReader(std::sync::mpsc::Receiver<()>);
impl std::io::Read for HeldReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.0.recv();
        Ok(0)
    }
}

#[allow(clippy::arc_with_non_send_sync)]
fn test_workspace() -> (
    TabManager,
    WorkspaceUiController,
    Arc<AtomicBool>,
    tempfile::NamedTempFile,
    std::sync::mpsc::Sender<()>,
) {
    use harbor_app::{tab_manager::TerminalTabResources, terminal_view::TerminalWidgetBridge};
    let pty = tempfile::NamedTempFile::new().unwrap();
    let gate = Arc::new(AtomicBool::new(false));
    let (reader_lifetime, reader) = std::sync::mpsc::channel();
    let mut reader = Some(reader);
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| {
        let terminal = Arc::new(std::sync::Mutex::new(Terminal::new_headless_with_io(
            8,
            40,
            HeldReader(reader.take().expect("single test tab")),
            pty.reopen().unwrap(),
            || true,
        )));
        let bridge = TerminalWidgetBridge::new(draw, terminal.clone(), Arc::clone(&gate));
        Ok(TerminalTabResources::new(terminal, bridge, "test"))
    })
    .unwrap();
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    ui.publish_panes(&tabs);
    (tabs, ui, gate, pty, reader_lifetime)
}

#[test]
fn osc52_confirmation_opens_in_window_displaces_search_gates_pty_and_resolves_once() {
    use crate::clipboard::Admission;
    use harbor_app::confirmation::ConfirmationKind;
    use std::sync::atomic::Ordering;

    let (tabs, ui, gate, pty, _reader_lifetime) = test_workspace();
    let binding = tabs.active_binding().unwrap();
    let terminal = tabs.active_terminal().unwrap();
    let mut confirmations =
        ConfirmationCoordinator::new(Arc::clone(&gate), harbor_config::Osc52WritePolicy::Confirm);

    // Open search first so OSC 52 confirmation displacement is verified.
    ui.open_search(binding.session);
    assert!(ui.search_query().is_some());

    let request = harbor_app::tab_manager::SessionClipboardWrite {
        source: binding.session,
        contents: harbor_terminal::ClipboardWrite::new("copied\tpayload\nline2".into()).unwrap(),
    };
    assert_eq!(confirmations.clipboard_admission(true), Admission::Confirm);
    let focus = confirmations.open_clipboard_confirmation(&ui, true, 42, request);
    assert!(focus.is_some());
    assert_eq!(confirmations.clipboard_epoch(), 42);
    assert!(ui.search_query().is_none(), "confirmation displaces Search");
    assert_eq!(
        ui.confirmation_kind(),
        Some(ConfirmationKind::ClipboardWrite)
    );
    let generation = confirmations
        .clipboard_generation()
        .expect("clipboard generation");
    assert_eq!(ui.confirmation_generation(), Some(generation));
    assert!(gate.load(Ordering::Acquire), "confirmation gates PTY input");

    // Mount into Runtime and verify PTY keystrokes remain suppressed while open.
    let mut runtime = harbor_widget::runtime::Runtime::new();
    runtime.set_viewport(harbor_widget::renderer::Viewport::new(1000, 700, 1.0));
    runtime.set_root(harbor_app::workspace::tab_workspace(ui.clone(), false));
    runtime.update(Instant::now());
    runtime.request_focus(&ui.terminal_focus());
    runtime.dispatch(harbor_widget::input::event::UiEvent::Keyboard(
        harbor_widget::input::event::KeyboardEvent::KeyDown {
            key: harbor_widget::input::event::Key::Character('z'),
            modifiers: Default::default(),
        },
    ));
    assert!(std::fs::read(pty.path()).unwrap().is_empty());

    // Confirming with the current generation resolves once without creating a permanent grant.
    let stale = confirmations.resolve_clipboard(generation + 1, true, true, &ui);
    assert!(stale.is_none());
    assert!(confirmations.is_active());

    let approved = confirmations
        .resolve_clipboard(generation, true, true, &ui)
        .expect("matching generation resolves approved payload");
    assert_eq!(approved.contents.as_str(), "copied\tpayload\nline2");
    assert!(!confirmations.is_active());
    assert!(!ui.confirmation_is_open());
    assert!(!gate.load(Ordering::Acquire));
    assert_eq!(confirmations.clipboard_admission(true), Admission::Confirm);
    drop(terminal);
}

#[test]
fn osc52_confirmation_cancels_on_focus_loss_or_ineligibility_with_zero_writes() {
    use std::sync::atomic::Ordering;

    let (tabs, ui, gate, _pty, _reader_lifetime) = test_workspace();
    let binding = tabs.active_binding().unwrap();
    let mut confirmations =
        ConfirmationCoordinator::new(Arc::clone(&gate), harbor_config::Osc52WritePolicy::Confirm);

    let request = harbor_app::tab_manager::SessionClipboardWrite {
        source: binding.session,
        contents: harbor_terminal::ClipboardWrite::new("secret".into()).unwrap(),
    };
    let _ = confirmations
        .open_clipboard_confirmation(&ui, true, 7, request)
        .expect("opened confirmation");
    let generation = confirmations.clipboard_generation().unwrap();
    assert!(confirmations.cancel_clipboard(&ui));
    assert!(!confirmations.is_active());
    assert!(!ui.confirmation_is_open());
    assert!(!gate.load(Ordering::Acquire));
    assert!(
        confirmations
            .resolve_clipboard(generation, true, true, &ui)
            .is_none(),
        "stale resolution after focus-loss cancellation cannot execute"
    );

    // Denial or failed revalidation also yields zero payload writes.
    let request = harbor_app::tab_manager::SessionClipboardWrite {
        source: binding.session,
        contents: harbor_terminal::ClipboardWrite::new("denied".into()).unwrap(),
    };
    let _ = confirmations
        .open_clipboard_confirmation(&ui, true, 8, request)
        .expect("opened confirmation");
    let generation = confirmations.clipboard_generation().unwrap();
    assert!(
        confirmations
            .resolve_clipboard(generation, true, false, &ui)
            .is_none()
    );
    assert!(!ui.confirmation_is_open());
    assert!(!gate.load(Ordering::Acquire));
}

#[test]
fn paste_and_osc52_confirmations_are_mutually_exclusive() {
    use harbor_app::confirmation::ConfirmationKind;

    let (tabs, ui, gate, _pty, _reader_lifetime) = test_workspace();
    let binding = tabs.active_binding().unwrap();
    let terminal = tabs.active_terminal().unwrap();
    let mut confirmations =
        ConfirmationCoordinator::new(Arc::clone(&gate), harbor_config::Osc52WritePolicy::Confirm);

    // 1. Active OSC 52 rejects incoming paste without changing the overlay.
    let request = harbor_app::tab_manager::SessionClipboardWrite {
        source: binding.session,
        contents: harbor_terminal::ClipboardWrite::new("osc52".into()).unwrap(),
    };
    let _ = confirmations
        .open_clipboard_confirmation(&ui, true, 10, request)
        .expect("osc52 confirmation opened");
    let osc_gen = ui.confirmation_generation().unwrap();
    let err = confirmations
        .paste_text(
            &ui,
            Some(&terminal),
            Some(binding),
            "line1\nline2".into(),
            false,
        )
        .unwrap_err();
    assert_eq!(err, "Rejected: paste confirmation is active");
    assert_eq!(ui.confirmation_generation(), Some(osc_gen));
    assert_eq!(
        ui.confirmation_kind(),
        Some(ConfirmationKind::ClipboardWrite)
    );

    assert!(confirmations.cancel_clipboard(&ui));

    // 2. Active paste rejects incoming OSC 52 confirmation without changing the overlay.
    confirmations
        .paste_text(
            &ui,
            Some(&terminal),
            Some(binding),
            "line1\nline2".into(),
            false,
        )
        .expect("paste confirmation opens");
    let paste_gen = ui.confirmation_generation().unwrap();
    assert_eq!(
        confirmations.clipboard_admission(true),
        crate::clipboard::Admission::Denied
    );
    assert_eq!(ui.confirmation_generation(), Some(paste_gen));
    assert_eq!(ui.confirmation_kind(), Some(ConfirmationKind::Paste));
}

#[test]
fn presented_frame_shows_only_a_pending_window() {
    let outcome = HostFrameOutcome::Presented { wait: None };
    assert_eq!(
        main_frame_policy(&outcome, true),
        MainFramePolicy {
            presented: true,
            show_window: true,
            schedule_hidden_retry: false,
            fatal: false,
        }
    );
    assert!(!main_frame_policy(&outcome, false).show_window);
}

#[test]
fn skipped_hidden_frame_retries_without_showing() {
    let outcome = HostFrameOutcome::Skipped { wait: None };
    let policy = main_frame_policy(&outcome, true);
    assert!(!policy.presented);
    assert!(!policy.show_window);
    assert!(policy.schedule_hidden_retry);
    assert!(!policy.fatal);
}

#[test]
fn recovery_waits_for_host_and_fatal_is_delegated_to_shell() {
    let recovery = HostFrameOutcome::RecoveryScheduled { wait: None };
    assert_eq!(
        main_frame_policy(&recovery, true),
        MainFramePolicy::default()
    );

    let fatal = HostFrameOutcome::Fatal {
        error: harbor_widget::winit::FrameError::out_of_memory(),
        wait: None,
    };
    assert!(main_frame_policy(&fatal, true).fatal);
}
