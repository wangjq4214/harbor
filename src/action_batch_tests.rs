//! Exercises the admission step used by Shell::drain_app_commands with the real UI inbox.
//! Native window/clipboard effects deliberately remain outside this headless seam.
use super::*;
use harbor_app::command::{AppCommand, AppCommandRequest, CommandSource};
use harbor_widget::{
    input::event::{Key, KeyboardEvent, Modifiers, UiEvent},
    renderer::Viewport,
    runtime::Runtime,
};
use std::{io::Write, sync::atomic::AtomicBool, time::Instant};

type Bytes = Arc<Mutex<Vec<u8>>>;
struct Writer(Bytes);
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn resources(draw: ExternalDrawId, bytes: &Bytes) -> TerminalTabResources {
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        8,
        40,
        std::io::empty(),
        Writer(bytes.clone()),
        || true,
    )));
    let bridge =
        TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
    TerminalTabResources::new(terminal, bridge, "batch-test")
}

fn fixture() -> (TabManager, WorkspaceUiController, Runtime, Bytes) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap();
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(1000, 500, 1.0));
    runtime.set_root(harbor_app::workspace::tab_workspace(ui.clone(), false));
    runtime.update(Instant::now());
    runtime.request_focus(&ui.terminal_focus());
    (tabs, ui, runtime, bytes)
}

fn key(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
}
fn shortcut(runtime: &mut Runtime, key_char: char, shift: bool) {
    key(
        runtime,
        Key::Character(key_char),
        Modifiers {
            ctrl: true,
            shift,
            ..Modifiers::default()
        },
    );
}
fn submit(ui: &WorkspaceUiController, runtime: &mut Runtime, query: &str) {
    let focus = ui.open_palette();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(query.into())));
    runtime.update(Instant::now());
    key(runtime, Key::Enter, Modifiers::default());
}
fn execute(admission: AppActionAdmission) -> (AppCommandRequest, bool) {
    match admission {
        AppActionAdmission::Execute {
            request,
            clear_palette_focus,
        } => (request, clear_palette_focus),
        AppActionAdmission::Rejected { reason, .. } => panic!("unexpected rejection: {reason}"),
    }
}

#[test]
fn real_inbox_preserves_fifo_source_and_tab_focus_disposition() {
    let (mut tabs, ui, mut runtime, bytes) = fixture();
    shortcut(&mut runtime, 't', false);
    shortcut(&mut runtime, 'r', true);
    shortcut(&mut runtime, 'w', false);
    let mut actions = ui.drain_actions().into_iter();
    let mut seen = Vec::new();
    while let Some(admission) = next_app_action(&mut actions, &ui, &tabs, false) {
        let (request, clear) = execute(admission);
        assert!(!clear);
        assert_eq!(request.source, CommandSource::Direct);
        assert_eq!(request.focus, TabFocusPolicy::Terminal);
        seen.push(request.command);
        if let AppCommand::Tab(command) = request.command {
            let result = tabs
                .execute_command(command, |_, draw| Ok(resources(draw, &bytes)))
                .unwrap();
            assert!(!result.close_window);
        }
    }
    assert_eq!(
        seen,
        [
            AppCommand::Tab(TabCommand::New),
            AppCommand::ReloadConfiguration,
            AppCommand::Tab(TabCommand::CloseActive)
        ]
    );
    assert_eq!(tabs.snapshots().len(), 1);
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());

    let rail = AppCommandRequest::rail(TabCommand::Next);
    let (request, clear) =
        execute(next_app_action(&mut [rail].into_iter(), &ui, &tabs, false).unwrap());
    assert_eq!(request.focus, TabFocusPolicy::PreserveRail);
    assert!(!clear);
}

#[test]
fn gate_raised_after_paste_discards_direct_actions_but_retires_palette_request() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    shortcut(&mut runtime, 'v', false);
    shortcut(&mut runtime, 't', false);
    submit(&ui, &mut runtime, "app.reload-configuration");
    let mut actions = ui.drain_actions().into_iter();
    assert_eq!(actions.len(), 3);
    let (paste, clear) = execute(next_app_action(&mut actions, &ui, &tabs, false).unwrap());
    assert_eq!(paste.command, AppCommand::Paste);
    assert!(!clear);
    // Native Paste raises the gate in production; the next production admission observes it.
    match next_app_action(&mut actions, &ui, &tabs, true).unwrap() {
        AppActionAdmission::Rejected {
            request,
            reason,
            restore_terminal_focus,
        } => {
            assert_eq!(request.command, AppCommand::ReloadConfiguration);
            assert!(reason.contains("confirmation"));
            assert!(
                !restore_terminal_focus,
                "never focus main over the confirmation window"
            );
        }
        _ => panic!("gated palette command executed"),
    }
    assert!(!ui.palette_is_open());
    assert!(next_app_action(&mut actions, &ui, &tabs, true).is_none());
    assert!(next_app_action(&mut actions, &ui, &tabs, false).is_none());
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn admission_rechecks_context_and_stale_requests_never_steal_new_editor_focus() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    submit(&ui, &mut runtime, "terminal.page-up");
    let requests = ui.drain_actions();
    assert_eq!(requests.len(), 1);
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str("\x1b[?1049h");
    match next_app_action(&mut requests.clone().into_iter(), &ui, &tabs, false).unwrap() {
        AppActionAdmission::Rejected {
            restore_terminal_focus,
            ..
        } => assert!(restore_terminal_focus),
        _ => panic!("unavailable alternate-screen action executed"),
    }
    assert!(!ui.palette_is_open());
    let focus = ui.open_palette();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    match next_app_action(&mut requests.into_iter(), &ui, &tabs, false).unwrap() {
        AppActionAdmission::Rejected {
            restore_terminal_focus,
            ..
        } => assert!(!restore_terminal_focus),
        _ => panic!("expired request executed"),
    }
    assert!(
        ui.palette_is_open(),
        "old admission must not dispose a newer editor"
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_admission_is_one_shot_and_keeps_command_specific_focus_destination() {
    let (mut tabs, ui, mut runtime, bytes) = fixture();
    for command in [AppCommand::SearchOpen, AppCommand::SearchClose] {
        let query = if command == AppCommand::SearchOpen {
            "terminal.search"
        } else {
            "terminal.search-close"
        };
        submit(&ui, &mut runtime, query);
        let requests = ui.drain_actions();
        assert_eq!(requests.len(), 1);
        let (request, clear) =
            execute(next_app_action(&mut requests.clone().into_iter(), &ui, &tabs, false).unwrap());
        assert_eq!(request.command, command);
        assert!(
            clear,
            "Shell must clear the disposed palette's native focus before execution"
        );
        assert!(!ui.palette_is_open());
        let outcome = tabs.execute_search_command(request.command, &ui);
        let focus = outcome.focus_for_source(request.source, &ui).unwrap();
        assert_eq!(
            focus,
            if command == AppCommand::SearchOpen {
                ui.search_focus()
            } else {
                ui.terminal_focus()
            }
        );
        assert!(matches!(
            next_app_action(&mut requests.into_iter(), &ui, &tabs, false),
            Some(AppActionAdmission::Rejected { .. })
        ));
    }
    assert!(bytes.lock().unwrap().is_empty());
}
