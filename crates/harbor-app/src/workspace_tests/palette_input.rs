use super::palette_support::*;
use super::*;

#[test]
fn palette_hover_is_lighter_transient_glass_and_preserves_keyboard_selection_and_focus() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let focus = runtime.input().focused();
    let (panel, selected) = palette_geometry(&runtime);
    let second = Point::new(
        selected.max.x - 2.0,
        selected.max.y + 4.0 + selected.size().height / 2.0,
    );
    row_pointer(&mut runtime, second, PointerPhase::Move);
    let hovered = row_surface(&runtime, 0.06).expect("hover is lighter than selection");
    assert!(hovered.contains(second));
    assert_eq!(row_surface(&runtime, 0.10), Some(selected));
    let delta = runtime.pending_delta().unwrap();
    assert!(
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .any(|item| matches!(
                item.primitive, Primitive::BackdropMaterial { rect, corner_radii, .. }
                    if rect == hovered && corner_radii == [12.0; 4]
            ))
    );
    assert_eq!(runtime.input().focused(), focus);
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .popups
            .get::<palette::PaletteState>()
            .unwrap()
            .selected,
        0
    );
    assert!(
        controller.drain_actions().is_empty(),
        "hover never executes"
    );
    row_pointer(
        &mut runtime,
        Point::new(panel.min.x + 1.0, panel.min.y + 1.0),
        PointerPhase::Move,
    );
    assert!(
        row_surface(&runtime, 0.06).is_none(),
        "hover disappears on leave"
    );
    assert_eq!(row_surface(&runtime, 0.10), Some(selected));
    tap(&mut runtime, Key::ArrowDown);
    runtime.update(Instant::now());
    assert_eq!(runtime.input().focused(), focus);
    tap(&mut runtime, Key::Enter);
    assert_eq!(
        take_one(&controller).command,
        AppCommand::Tab(TabCommand::New)
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_single_click_runs_the_clicked_command_once_instead_of_the_keyboard_selection() {
    // Labels, keycaps and row padding are all part of the same click target.
    for fraction in [0.0, 0.5, 1.0] {
        let (mut tabs, controller, mut runtime, bytes) = fixture();
        open(&controller, &mut runtime);
        let focus = runtime.input().focused();
        let (_, first) = palette_geometry(&runtime);
        let point = Point::new(
            first.min.x + 2.0 + (first.size().width - 4.0) * fraction,
            first.max.y + 4.0 + first.size().height / 2.0,
        );
        row_pointer(&mut runtime, point, PointerPhase::Move);
        row_pointer(&mut runtime, point, PointerPhase::Down);
        assert!(
            controller.drain_actions().is_empty(),
            "execute on release, not press"
        );
        assert_eq!(runtime.input().focused(), focus);
        row_pointer(&mut runtime, point, PointerPhase::Up);
        // Repeated clicks and Enter before the Host drains cannot enqueue duplicates.
        row_pointer(&mut runtime, point, PointerPhase::Down);
        row_pointer(&mut runtime, point, PointerPhase::Up);
        tap(&mut runtime, Key::Enter);
        let request = take_one(&controller);
        assert_eq!(request.command, AppCommand::Tab(TabCommand::New));
        assert!(matches!(request.source, CommandSource::Palette { .. }));
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap();
        let mut executions = 0;
        tabs.execute_command(TabCommand::New, |_, draw| {
            executions += 1;
            Ok(resources(draw, &bytes))
        })
        .unwrap();
        assert_eq!(executions, 1);
        assert_eq!(tabs.snapshots().len(), 2);
        assert!(!controller.palette_is_open());
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn palette_cancelled_click_and_disabled_row_leave_the_editor_usable_without_executing() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let focus = runtime.input().focused();
    let (panel, first) = palette_geometry(&runtime);
    let point = Point::new(
        first.min.x + 30.0,
        first.max.y + 4.0 + first.size().height / 2.0,
    );
    let outside = Point::new(panel.min.x - 2.0, panel.min.y);
    row_pointer(&mut runtime, point, PointerPhase::Down);
    row_pointer(&mut runtime, outside, PointerPhase::Move);
    row_pointer(&mut runtime, outside, PointerPhase::Up);
    assert!(runtime.input().captor(42).is_none());
    assert_eq!(runtime.input().focused(), focus);
    assert!(controller.drain_actions().is_empty());
    replace(&mut runtime, "app.next-tab");
    assert!(!painted(&runtime).contains("No other tab"));
    let (_, disabled) = palette_geometry(&runtime);
    let point = Point::new(disabled.min.x + 30.0, disabled.min.y + 10.0);
    row_pointer(&mut runtime, point, PointerPhase::Move);
    assert_eq!(row_surface(&runtime, 0.10), Some(disabled));
    assert!(row_surface(&runtime, 0.06).is_none());
    assert!(row_surface(&runtime, 0.12).is_none());
    assert!(!painted(&runtime).contains("No other tab"));
    row_pointer(&mut runtime, point, PointerPhase::Down);
    assert!(row_surface(&runtime, 0.14).is_none());
    assert!(runtime.input().captor(42).is_none());
    row_pointer(&mut runtime, point, PointerPhase::Up);
    assert_eq!(runtime.input().focused(), focus);
    assert!(controller.drain_actions().is_empty());
    replace(&mut runtime, "app.new-tab");
    tap(&mut runtime, Key::Enter);
    assert_eq!(
        take_one(&controller).command,
        AppCommand::Tab(TabCommand::New)
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_filtering_during_a_press_never_executes_a_replacement_row() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let (_, first) = palette_geometry(&runtime);
    let point = Point::new(
        first.min.x + 30.0,
        first.max.y + 4.0 + first.size().height / 2.0,
    );
    row_pointer(&mut runtime, point, PointerPhase::Down);
    replace(&mut runtime, "terminal.copy");
    row_pointer(&mut runtime, point, PointerPhase::Up);
    assert!(controller.drain_actions().is_empty());
    assert!(runtime.input().captor(42).is_none());
    assert!(bytes.lock().unwrap().is_empty());
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
        .popups
        .get::<palette::PaletteState>()
        .unwrap()
        .selected;
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
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
        .popups
        .get::<palette::PaletteState>()
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
