//! Headless evidence for the T0001 increment, not native Windows acceptance.
use super::*;
use crate::{
    command::{CommandSource, command_availability, validate_keybindings},
    tab_manager::{TabManager, TerminalTabResources},
};
use harbor_widget::{
    TextEditingController,
    effects::ClipboardEffect,
    input::event::{FocusEvent, ImePreedit},
};

type Capture = Arc<Mutex<Vec<u8>>>;

#[allow(clippy::arc_with_non_send_sync)]
fn resources(draw: ExternalDrawId, bytes: &Capture) -> TerminalTabResources {
    let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        std::io::empty(),
        Writer(bytes.clone()),
        || true,
    )));
    let bridge = TerminalWidgetBridge::new(draw, engine.clone(), Arc::new(AtomicBool::new(false)));
    TerminalTabResources::new(engine, bridge, "test")
}

fn fixture() -> (TabManager, WorkspaceUiController, Runtime, Capture) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap();
    let controller =
        WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    (tabs, controller, runtime, bytes)
}

fn down(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
}
fn up(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
}
fn tap(runtime: &mut Runtime, key: Key) {
    down(runtime, key, Modifiers::default());
    up(runtime, key, Modifiers::default());
}
fn open(controller: &WorkspaceUiController, runtime: &mut Runtime) {
    let mods = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::default()
    };
    down(runtime, Key::Character('P'), mods);
    let actions = controller.drain_actions();
    assert_eq!(
        actions,
        [AppCommandRequest::shortcut(AppCommand::OpenCommandPalette)]
    );
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    let effects = runtime.update(Instant::now());
    assert_eq!(
        effects.ime.expect("query IME destination").allowed,
        Some(true)
    );
    up(runtime, Key::Character('P'), mods);
    assert!(runtime.input().focused().is_some());
}
fn input(controller: &WorkspaceUiController) -> TextEditingController {
    controller
        .store
        .state()
        .read()
        .palette
        .as_ref()
        .unwrap()
        .input
        .clone()
}
fn replace(runtime: &mut Runtime, text: &str) {
    down(runtime, Key::Character('a'), ctrl());
    up(runtime, Key::Character('a'), ctrl());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(text.into())));
    runtime.update(Instant::now());
}
fn painted(runtime: &Runtime) -> String {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .filter_map(|item| match &item.primitive {
            Primitive::Text { text, .. } => Some(text.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn take_one(controller: &WorkspaceUiController) -> AppCommandRequest {
    let actions = controller.drain_actions();
    assert_eq!(actions.len(), 1, "exactly one typed request");
    actions[0]
}

#[test]
fn palette_filters_navigates_and_executes_one_real_tab_reduction_without_pty_input() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    assert!(painted(&runtime).contains("Reload configuration"));
    assert!(painted(&runtime).contains("No other tab"));
    tap(&mut runtime, Key::ArrowDown);
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .palette
            .as_ref()
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
    assert!(painted(&runtime).contains("Tab does not exist"));
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
    assert!(painted(&runtime).contains("Ctrl+T"));
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
    assert!(text.contains("Ctrl+N, Alt+N"));
    assert!(!text.contains("Ctrl+T"));
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
fn palette_ime_preedit_confirmation_escape_and_clipboard_are_editor_owned() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "app.new-tab");
    let editor = input(&controller);
    let initial_selected = controller
        .store
        .state()
        .read()
        .palette
        .as_ref()
        .unwrap()
        .selected;
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
    tap(&mut runtime, Key::ArrowDown);
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .palette
            .as_ref()
            .unwrap()
            .selected,
        initial_selected
    );
    assert_eq!(editor.text(), "app.new-tab");
    down(&mut runtime, Key::Enter, Modifiers::default());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    down(&mut runtime, Key::Enter, Modifiers::default());
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert_eq!(editor.text(), "app.new-tab界");
    assert!(
        controller.drain_actions().is_empty(),
        "IME confirmation never executes a candidate"
    );
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("文", None)));
    tap(&mut runtime, Key::Escape);
    assert!(editor.value().composition.is_none());
    assert!(controller.drain_actions().is_empty());
    assert!(controller.palette_is_open());
    down(&mut runtime, Key::Character('a'), ctrl());
    let copied = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('c'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(copied)) = copied.clipboard else {
        panic!("editor copy")
    };
    runtime.complete_clipboard(copied.id, Ok(String::new()));
    let cut = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('x'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(cut)) = cut.clipboard else {
        panic!("editor cut")
    };
    runtime.complete_clipboard(cut.id, Ok(String::new()));
    assert_eq!(editor.text(), "");
    let pasted = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(pasted)) = pasted.clipboard else {
        panic!("editor paste")
    };
    runtime.complete_clipboard(pasted.id, Ok("New tab".into()));
    assert_eq!(editor.text(), "New tab");
    down(&mut runtime, Key::Character('z'), ctrl());
    assert_eq!(editor.text(), "");
    assert!(
        controller.drain_actions().is_empty(),
        "query clipboard never queues terminal actions"
    );
    tap(&mut runtime, Key::Escape);
    let request = take_one(&controller);
    let AppCommand::CloseCommandPalette(generation) = request.command else {
        panic!("close")
    };
    assert!(controller.close_palette(generation));
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    up(&mut runtime, Key::Escape, Modifiers::default());
    assert!(bytes.lock().unwrap().is_empty());
    tap(&mut runtime, Key::Character('q'));
    assert_eq!(&*bytes.lock().unwrap(), b"q");
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
fn palette_close_focus_loss_and_root_replacement_reject_late_clipboard_and_ime() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "query");
    let old = input(&controller);
    let pasted = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(pasted)) = pasted.clipboard else {
        panic!("paste")
    };
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
    runtime.dispatch(UiEvent::Focus(FocusEvent::Lost));
    runtime.complete_clipboard(pasted.id, Ok("late".into()));
    assert_eq!(old.text(), "query");
    assert!(old.value().composition.is_none());
    runtime.dispatch(UiEvent::Focus(FocusEvent::Gained));
    runtime.request_focus(&controller.open_palette());
    let late_close = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(late_close)) = late_close.clipboard else {
        panic!("pending close paste")
    };
    let generation = controller
        .store
        .state()
        .read()
        .palette
        .as_ref()
        .unwrap()
        .generation;
    controller.close_palette(generation);
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    runtime.complete_clipboard(late_close.id, Ok("late close".into()));
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("文", None)));
    let editor = input(&controller);
    let pending = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    // Composition consumes Ctrl+V rather than starting clipboard I/O.
    assert!(pending.clipboard.is_none());
    runtime.set_root(tab_workspace(controller.clone(), false));
    runtime.update(Instant::now());
    assert!(editor.value().composition.is_none());
    runtime.request_focus(&controller.open_palette());
    let late_root = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(late_root)) = late_root.clipboard else {
        panic!("pending replacement paste")
    };
    runtime.set_root(tab_workspace(controller.clone(), false));
    runtime.update(Instant::now());
    runtime.request_focus(&controller.open_palette());
    runtime.complete_clipboard(late_root.id, Ok("wrong lifetime".into()));
    assert_eq!(editor.text(), "");
    assert!(controller.drain_actions().is_empty());
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

#[test]
fn palette_native_ime_confirmation_and_close_gestures_do_not_replay_into_terminal() {
    use harbor_widget::winit::WinitAdapter;
    use winit::{
        event::{ElementState, Ime, WindowEvent},
        keyboard::{Key as NativeKey, KeyLocation, NamedKey},
    };
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Preedit("New tab".into(), None)),
    );
    let enter = NativeKey::Named(NamedKey::Enter);
    adapter.handle_keyboard_input(
        &mut runtime,
        &enter,
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Commit("New tab".into())),
    );
    adapter.handle_keyboard_input_with_repeat(
        &mut runtime,
        &enter,
        ElementState::Pressed,
        KeyLocation::Standard,
        true,
    );
    adapter.handle_keyboard_input(
        &mut runtime,
        &enter,
        ElementState::Released,
        KeyLocation::Standard,
    );
    assert!(
        controller.drain_actions().is_empty(),
        "native IME confirmation repeat cannot execute a candidate"
    );
    assert_eq!(input(&controller).text(), "New tab");
    let escape = NativeKey::Named(NamedKey::Escape);
    adapter.handle_keyboard_input(
        &mut runtime,
        &escape,
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    let AppCommand::CloseCommandPalette(generation) = take_one(&controller).command else {
        panic!("close")
    };
    controller.close_palette(generation);
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    adapter.handle_keyboard_input_with_repeat(
        &mut runtime,
        &escape,
        ElementState::Pressed,
        KeyLocation::Standard,
        true,
    );
    adapter.handle_keyboard_input(
        &mut runtime,
        &escape,
        ElementState::Released,
        KeyLocation::Standard,
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "held close gesture never becomes terminal Escape"
    );
    adapter.handle_keyboard_input(
        &mut runtime,
        &NativeKey::Character("z".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    assert_eq!(&*bytes.lock().unwrap(), b"z");
}

#[path = "palette_terminal_tests.rs"]
mod terminal_actions;
