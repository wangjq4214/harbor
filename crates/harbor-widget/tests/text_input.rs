use harbor_widget::{
    ActionOutcome, Actions, EditableText, FocusHandle, FocusScope, KeyChord, Shortcuts,
    TextEditingController, TextField, TextSelection,
    effects::{ClipboardEffect, ClipboardEffectError},
    input::event::{
        FocusEvent, ImePreedit, Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent,
        PointerPhase, UiEvent,
    },
    layout::Point,
    renderer::Viewport,
    runtime::Runtime,
    signal::Signal,
    view::{BuildCx, Component, View},
};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

fn key(key: Key, modifiers: Modifiers) -> UiEvent {
    UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers })
}
fn ctrl(ch: char) -> UiEvent {
    key(
        Key::Character(ch),
        Modifiers {
            ctrl: true,
            ..Default::default()
        },
    )
}
fn press(runtime: &mut Runtime, key_value: Key) {
    runtime.dispatch(key(key_value, Modifiers::default()));
}
fn update(runtime: &mut Runtime) {
    runtime.update(Instant::now());
}
fn mount(component: impl Component + 'static) -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_root(component);
    update(&mut runtime);
    assert!(runtime.focus_first_focusable());
    update(&mut runtime);
    runtime
}
fn request(runtime: &mut Runtime, event: UiEvent) -> u64 {
    let effects = runtime.dispatch(event);
    match effects.clipboard {
        Some(ClipboardEffect::Editor(request)) => request.id,
        other => panic!("missing tagged clipboard operation: {other:?}"),
    }
}
fn pointer(x: f32, y: f32, phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(
        Point::new(x, y),
        phase,
        PointerButton::Left,
        1,
    ))
}

#[test]
fn keyboard_replacement_history_and_notifications_are_editor_local() {
    let c = TextEditingController::new("abcd");
    let changes = Rc::new(Cell::new(0));
    let observed = changes.clone();
    let mut runtime =
        mount(TextField::new(c.clone()).on_changed(move |_| observed.set(observed.get() + 1)));
    press(&mut runtime, Key::Home);
    press(&mut runtime, Key::ArrowRight);
    press(&mut runtime, Key::Character('X'));
    assert_eq!(c.text(), "aXbcd");
    runtime.dispatch(key(
        Key::ArrowRight,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    ));
    press(&mut runtime, Key::Character('Y'));
    assert_eq!(c.text(), "aXYcd");
    runtime.dispatch(ctrl('z'));
    assert_eq!(c.text(), "aXbcd");
    runtime.dispatch(ctrl('y'));
    assert_eq!(c.text(), "aXYcd");
    assert_eq!(changes.get(), 4);
    c.set_text("programmatic");
    update(&mut runtime);
    assert_eq!(changes.get(), 4);
    runtime.dispatch(ctrl('a'));
    press(&mut runtime, Key::Delete);
    assert_eq!(c.text(), "");
    assert!(runtime.drain_external_input().is_empty());
}

#[test]
fn tagged_clipboard_cut_waits_for_success_and_paste_rejects_stale_revisions() {
    let c = TextEditingController::new("abc");
    let mut runtime = mount(EditableText::new(c.clone()));
    c.set_selection(TextSelection {
        anchor: 1,
        extent: 2,
    });
    let cut = request(&mut runtime, ctrl('x'));
    assert_eq!(c.text(), "abc");
    runtime.complete_clipboard(cut, Err(ClipboardEffectError::WriteFailed));
    assert_eq!(c.text(), "abc");
    let cut = request(&mut runtime, ctrl('x'));
    runtime.complete_clipboard(cut, Ok(String::new()));
    assert_eq!(c.text(), "ac");
    let paste = request(&mut runtime, ctrl('v'));
    runtime.complete_clipboard(paste, Ok("你".into()));
    assert_eq!(c.text(), "a你c");
    let stale = request(&mut runtime, ctrl('v'));
    press(&mut runtime, Key::Home);
    runtime.complete_clipboard(stale, Ok("wrong".into()));
    assert_eq!(c.text(), "a你c");
    let failed = request(&mut runtime, ctrl('v'));
    runtime.complete_clipboard(failed, Err(ClipboardEffectError::ReadFailed));
    assert_eq!(c.text(), "a你c");
}

#[test]
fn clipboard_completion_cannot_follow_focus_or_remounted_controller() {
    let first = TextEditingController::new("first");
    let second = TextEditingController::new("second");
    let a = FocusHandle::new();
    let b = FocusHandle::new();
    let mut runtime = mount(
        FocusScope::new()
            .child(TextField::new(first.clone()).focus_handle(a))
            .child(TextField::new(second.clone()).focus_handle(b)),
    );
    let request_id = request(&mut runtime, ctrl('v'));
    runtime.request_focus(&b);
    runtime.complete_clipboard(request_id, Ok("wrong".into()));
    assert_eq!(first.text(), "first");
    assert_eq!(second.text(), "second");
    runtime.request_focus(&a);
    let removed = request(&mut runtime, ctrl('v'));
    runtime.set_root(TextField::new(second.clone()).focus_handle(a));
    update(&mut runtime);
    runtime.request_focus(&a);
    runtime.complete_clipboard(removed, Ok("wrong".into()));
    assert_eq!(second.text(), "second");
}

#[test]
fn ime_composition_does_not_notify_or_double_submit_and_loss_clears_it() {
    let c = TextEditingController::new("ab");
    let changed = Rc::new(Cell::new(0));
    let submits = Rc::new(Cell::new(0));
    let (changes, submitted) = (changed.clone(), submits.clone());
    let mut runtime = mount(
        EditableText::new(c.clone())
            .on_changed(move |_| changes.set(changes.get() + 1))
            .on_submitted(move |_| submitted.set(submitted.get() + 1)),
    );
    c.set_selection(TextSelection::collapsed(1));
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("ni", Some((0, 2)))));
    assert_eq!(c.text(), "ab");
    assert_eq!(changed.get(), 0);
    press(&mut runtime, Key::Enter);
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("你".into())));
    press(&mut runtime, Key::Enter); // repeat of the IME-owned physical press
    assert_eq!(submits.get(), 0);
    assert_eq!(c.text(), "a你b");
    assert_eq!(changed.get(), 1);
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
        key: Key::Enter,
        modifiers: Default::default(),
    }));
    press(&mut runtime, Key::Enter);
    assert_eq!(submits.get(), 1);
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("draft", None)));
    runtime.dispatch(UiEvent::Focus(FocusEvent::Lost));
    assert!(c.value().composition.is_none());
    assert_eq!(c.text(), "a你b");
    assert_eq!(
        runtime.update(Instant::now()).ime.unwrap().allowed,
        Some(false)
    );
}

#[test]
fn pointer_selection_and_capture_use_current_scrolled_geometry() {
    let c = TextEditingController::new("abcdefghij");
    let mut runtime = mount(EditableText::new(c.clone()));
    runtime.set_viewport(Viewport::new(42, 25, 1.0));
    update(&mut runtime);
    press(&mut runtime, Key::Home);
    update(&mut runtime);
    runtime.dispatch(pointer(10.0, 5.0, PointerPhase::Down));
    runtime.dispatch(pointer(30.0, 5.0, PointerPhase::Move));
    assert_eq!(c.selected_text(), "bc");
    runtime.dispatch(pointer(500.0, 5.0, PointerPhase::Up));
    assert!(runtime.input().captor(1).is_none());
    press(&mut runtime, Key::End);
    update(&mut runtime);
    let effect = runtime.update(Instant::now()).ime.unwrap();
    assert_eq!(effect.allowed, Some(true));
    assert!(effect.position.unwrap().x <= 42.0);
    runtime.dispatch(pointer(20.0, 5.0, PointerPhase::Down));
    assert!(c.value().selection.extent >= 6);
    runtime.dispatch(pointer(20.0, 5.0, PointerPhase::Cancel));
    assert!(runtime.input().captor(1).is_none());
    runtime.set_viewport(Viewport::new(84, 50, 2.0));
    update(&mut runtime);
    assert!(
        runtime
            .update(Instant::now())
            .ime
            .unwrap()
            .position
            .unwrap()
            .x
            <= 42.0
    );
}

#[derive(Clone)]
struct Modes {
    controller: TextEditingController,
    mode: Signal<u8>,
}
impl Component for Modes {
    fn build(&self, cx: &mut BuildCx) -> View {
        cx.track(&self.mode);
        TextField::new(self.controller.clone())
            .enabled(*self.mode.read() != 2)
            .read_only(*self.mode.read() == 1)
            .build(cx)
    }
}

#[test]
fn readonly_and_disabled_modes_block_mutation_and_inflight_paste() {
    let c = TextEditingController::new("fixture");
    let mode = Signal::new(0);
    let mut runtime = mount(Modes {
        controller: c.clone(),
        mode: mode.clone(),
    });
    runtime.dispatch(ctrl('a'));
    let paste = request(&mut runtime, ctrl('v'));
    mode.set(1);
    update(&mut runtime);
    runtime.complete_clipboard(paste, Ok("wrong".into()));
    press(&mut runtime, Key::Delete);
    runtime.dispatch(ctrl('x'));
    assert_eq!(c.text(), "fixture");
    let copied = runtime.dispatch(ctrl('c'));
    assert!(matches!(copied.clipboard, Some(ClipboardEffect::Editor(_))));
    mode.set(2);
    update(&mut runtime);
    press(&mut runtime, Key::Character('x'));
    assert_eq!(c.text(), "fixture");
    assert!(!runtime.focus_first_focusable());
}

#[test]
fn editor_chords_bypass_terminal_like_actions_but_other_shortcuts_still_work() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let c = TextEditingController::new("abc");
    let shortcuts = Shortcuts::empty()
        .bind(
            KeyChord::new(
                Key::Character('c'),
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            1_u8,
        )
        .bind(KeyChord::new(Key::F3, Default::default()), 2_u8);
    let actions = Actions::handler(move |_: u8| {
        observed.fetch_add(1, Ordering::SeqCst);
        ActionOutcome::Consumed
    });
    let mut runtime = mount(move |cx: &mut BuildCx| {
        harbor_widget::view! { cx; actions.clone() => {
            shortcuts.clone() => { TextField::new(c.clone()); }
        } }
    });
    runtime.dispatch(ctrl('a'));
    let effect = runtime.dispatch(ctrl('c'));
    assert!(matches!(effect.clipboard, Some(ClipboardEffect::Editor(_))));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    press(&mut runtime, Key::F3);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn unicode_deletion_through_runtime_and_rebuild_preserves_history() {
    let c = TextEditingController::new("a👨‍👩‍👧‍👦e\u{301}界");
    let mode = Signal::new(0);
    let mut runtime = mount(Modes {
        controller: c.clone(),
        mode: mode.clone(),
    });
    press(&mut runtime, Key::Backspace);
    press(&mut runtime, Key::Backspace);
    assert_eq!(c.text(), "a👨‍👩‍👧‍👦");
    mode.set(0); // force an unrelated root rebuild
    update(&mut runtime);
    runtime.dispatch(ctrl('z'));
    assert_eq!(c.text(), "a👨‍👩‍👧‍👦e\u{301}");
    assert!(runtime.input().captor(1).is_none());
}
