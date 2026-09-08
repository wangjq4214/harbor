use harbor_widget::effects::{CursorEffect, CursorShape};
use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::{Point, Size};
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::view::{Component, SizedBox};
use harbor_widget::{
    Actions, Button, Focus, FocusHandle, FocusScope, KeyChord, MouseRegion, Shortcuts, Theme,
    ThemeProvider,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn now() -> Instant {
    Instant::now()
}

fn pointer(position: Point, phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(position, phase, PointerButton::Left, 0))
}

#[derive(Clone)]
struct DisableableButton {
    disabled: harbor_widget::signal::Signal<bool>,
}

impl Component for DisableableButton {
    fn build(&self, cx: &mut harbor_widget::view::BuildCx) -> harbor_widget::view::View {
        cx.track(&self.disabled);
        Button::new("Disableable")
            .disabled(*self.disabled.read())
            .build(cx)
    }
}

#[derive(Clone)]
struct ToggleMouseRegion {
    visible: harbor_widget::signal::Signal<bool>,
}

impl Component for ToggleMouseRegion {
    fn build(&self, cx: &mut harbor_widget::view::BuildCx) -> harbor_widget::view::View {
        cx.track(&self.visible);
        if *self.visible.read() {
            MouseRegion::new()
                .cursor(CursorShape::Text)
                .child(SizedBox::new(Size::new(100.0, 40.0)))
                .build(cx)
        } else {
            SizedBox::new(Size::new(100.0, 40.0)).build(cx)
        }
    }
}

#[derive(Clone)]
struct ToggleFocusedChild {
    show_target: harbor_widget::signal::Signal<bool>,
}

impl Component for ToggleFocusedChild {
    fn build(&self, cx: &mut harbor_widget::view::BuildCx) -> harbor_widget::view::View {
        cx.track(&self.show_target);
        let mut scope =
            FocusScope::new().child(Focus::new(SizedBox::new(Size::new(20.0, 20.0))).order(10));
        if *self.show_target.read() {
            scope = scope.child(Focus::new(SizedBox::new(Size::new(20.0, 20.0))).order(-1));
        }
        scope.build(cx)
    }
}

#[test]
fn interactive_region_cancels_outside_release_and_reactivates_after_reentry() {
    let activations = Arc::new(AtomicUsize::new(0));
    let observed = activations.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(Button::new("OK").on_click(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
    }));
    runtime.update(now());

    runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Down));
    runtime.dispatch(pointer(Point::new(500.0, 500.0), PointerPhase::Move));
    runtime.dispatch(pointer(Point::new(500.0, 500.0), PointerPhase::Up));
    assert_eq!(activations.load(Ordering::SeqCst), 0);
    assert!(runtime.input().captor(0).is_none());

    runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Down));
    runtime.dispatch(pointer(Point::new(500.0, 500.0), PointerPhase::Move));
    runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Move));
    runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Up));
    assert_eq!(activations.load(Ordering::SeqCst), 1);
}

#[test]
fn disabling_a_pressed_control_releases_its_capture_during_rebuild() {
    let disabled = harbor_widget::signal::Signal::new(false);
    let mut runtime = Runtime::new();
    runtime.set_root(DisableableButton {
        disabled: disabled.clone(),
    });
    runtime.update(now());

    runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Down));
    assert!(runtime.input().captor(0).is_some());

    disabled.set(true);
    runtime.update(now());
    assert!(runtime.input().captor(0).is_none());
    assert!(runtime.input().focused().is_none());
}

#[test]
fn mouse_region_emits_boundary_callback_and_cursor_only_on_edges() {
    let entered = Arc::new(AtomicUsize::new(0));
    let observed = entered.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(
        MouseRegion::new()
            .cursor(CursorShape::Text)
            .on_enter(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
            })
            .child(SizedBox::new(Size::new(100.0, 40.0))),
    );
    runtime.update(now());

    let entered_effects = runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Move));
    assert_eq!(entered.load(Ordering::SeqCst), 1);
    assert_eq!(
        entered_effects.cursor,
        Some(CursorEffect::Set(CursorShape::Text))
    );

    let stable_effects = runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Move));
    assert_eq!(entered.load(Ordering::SeqCst), 1);
    assert_eq!(stable_effects.cursor, None);

    let left_effects = runtime.dispatch(pointer(Point::new(150.0, 10.0), PointerPhase::Move));
    assert_eq!(left_effects.cursor, Some(CursorEffect::Reset));
}

#[test]
fn removing_a_hovered_mouse_region_resets_the_cursor_during_update() {
    let visible = harbor_widget::signal::Signal::new(true);
    let mut runtime = Runtime::new();
    runtime.set_root(ToggleMouseRegion {
        visible: visible.clone(),
    });
    runtime.update(now());

    let entered = runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Move));
    assert_eq!(entered.cursor, Some(CursorEffect::Set(CursorShape::Text)));

    visible.set(false);
    let effects = runtime.update(now());

    assert_eq!(effects.cursor, Some(CursorEffect::Reset));
}

#[test]
fn focus_scope_uses_explicit_order_and_focus_handles() {
    let first = FocusHandle::new();
    let second = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.set_root(
        FocusScope::new()
            .child(
                Focus::new(SizedBox::new(Size::new(20.0, 20.0)))
                    .order(10)
                    .handle(second),
            )
            .child(
                Focus::new(SizedBox::new(Size::new(20.0, 20.0)))
                    .order(-1)
                    .handle(first),
            ),
    );
    runtime.update(now());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    let first_fiber = runtime.input().focused().expect("first focus target");

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    let second_fiber = runtime.input().focused().expect("second focus target");
    assert_ne!(
        first_fiber, second_fiber,
        "explicit order changes traversal order"
    );

    let effects = runtime.request_focus(&first);
    assert!(effects.request_redraw);
    assert_eq!(runtime.input().focused(), Some(first_fiber));
}

#[test]
fn focus_wrapper_delegates_keyboard_activation_to_its_child() {
    let activations = Arc::new(AtomicUsize::new(0));
    let observed = activations.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(Focus::new(Button::new("Wrapped").on_click(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
    })));
    runtime.update(now());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    }));

    assert_eq!(activations.load(Ordering::SeqCst), 1);
}

#[test]
fn focus_wrapper_contributes_one_logical_tab_stop() {
    let mut runtime = Runtime::new();
    runtime.set_root(Focus::new(Button::new("Wrapped")).order(-10));
    runtime.update(now());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    let first = runtime.input().focused().expect("wrapper receives focus");
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));

    assert_eq!(runtime.input().focused(), Some(first));
}

#[test]
fn disabled_focus_wrapper_suppresses_focusable_descendants() {
    let mut runtime = Runtime::new();
    runtime.set_root(Focus::new(Button::new("Disabled wrapper")).enabled(false));
    runtime.update(now());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));

    assert!(runtime.input().focused().is_none());
}

#[test]
fn root_traversal_enters_a_nested_focus_scope() {
    let mut runtime = Runtime::new();
    runtime.set_root(
        ThemeProvider::new(Theme::default()).child(FocusScope::new().child(Button::new("Nested"))),
    );
    runtime.update(now());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));

    assert!(runtime.input().focused().is_some());
}

#[test]
fn pending_focus_handle_restores_keyboard_visible_focus() {
    let handle = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.request_focus(&handle);
    runtime.set_root(Focus::new(Button::new("Restored")).handle(handle));
    runtime.update(now());
    assert!(runtime.input().focused().is_some());

    let focus_borders = runtime.pending_delta().map_or(0, |delta| {
        delta
            .added
            .iter()
            .filter(|item| matches!(item.primitive, Primitive::Border { width, .. } if width > 1.0))
            .count()
    });
    assert_eq!(focus_borders, 1);
}

#[test]
fn tracked_external_signals_unsubscribe_when_their_component_unmounts() {
    let disabled = harbor_widget::signal::Signal::new(false);
    let mut runtime = Runtime::new();
    runtime.set_root(DisableableButton {
        disabled: disabled.clone(),
    });
    runtime.update(now());
    runtime.set_root(SizedBox::new(Size::new(10.0, 10.0)));
    runtime.update(now());

    disabled.set(true);

    assert!(!runtime.update(now()).request_redraw);
}

#[test]
fn removing_the_focused_child_uses_a_fallback_in_the_same_scope() {
    let show_target = harbor_widget::signal::Signal::new(true);
    let mut runtime = Runtime::new();
    runtime.set_root(ToggleFocusedChild {
        show_target: show_target.clone(),
    });
    runtime.update(now());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    let removed_target = runtime.input().focused().expect("target is focused");

    show_target.set(false);
    runtime.update(now());

    let fallback = runtime
        .input()
        .focused()
        .expect("scope fallback is focused");
    assert_ne!(fallback, removed_target);
    assert!(runtime.arena().get(fallback).is_some());
}

#[derive(Clone)]
enum TestAction {
    NewTab,
    NextTab,
}

#[test]
fn shortcuts_claim_exact_chords_and_invoke_nearest_actions_provider() {
    let invocations = Arc::new(AtomicUsize::new(0));
    let observed = invocations.clone();
    let chord = KeyChord::new(
        Key::Character('t'),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    );
    let mut runtime = Runtime::new();
    runtime.set_root(Actions::new(
        Shortcuts::new(Button::new("Target")).bind(chord, TestAction::NewTab),
        move |action| {
            assert!(matches!(action, TestAction::NewTab));
            observed.fetch_add(1, Ordering::SeqCst);
        },
    ));
    runtime.update(now());
    assert!(runtime.input().focused().is_none());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('t'),
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    }));
    assert_eq!(invocations.load(Ordering::SeqCst), 1);

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('t'),
        modifiers: Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        },
    }));
    assert_eq!(
        invocations.load(Ordering::SeqCst),
        1,
        "modifiers match exactly"
    );
}

#[test]
fn keyboard_shortcut_restores_focus_visible_modality() {
    let chord = KeyChord::new(
        Key::Character('t'),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    );
    let mut runtime = Runtime::new();
    runtime.set_root(Actions::new(
        Shortcuts::new(Button::new("Target")).bind(chord, TestAction::NewTab),
        |_: TestAction| {},
    ));
    runtime.update(now());
    runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Down));
    runtime.dispatch(pointer(Point::new(10.0, 10.0), PointerPhase::Up));
    runtime.update(now());

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('t'),
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    }));
    runtime.update(now());

    assert!(runtime.pending_delta().is_some_and(|delta| {
        delta
            .added
            .iter()
            .any(|item| matches!(item.primitive, Primitive::Border { width, .. } if width > 1.0))
    }));
}

#[test]
fn ctrl_tab_prefers_the_exact_shortcut_over_focus_traversal() {
    let invocations = Arc::new(AtomicUsize::new(0));
    let observed = invocations.clone();
    let chord = KeyChord::new(
        Key::Tab,
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    );
    let mut runtime = Runtime::new();
    runtime.set_root(Actions::new(
        Shortcuts::new(
            FocusScope::new()
                .child(Button::new("First"))
                .child(Button::new("Second")),
        )
        .bind(chord, TestAction::NextTab),
        move |action| {
            assert!(matches!(action, TestAction::NextTab));
            observed.fetch_add(1, Ordering::SeqCst);
        },
    ));
    runtime.update(now());
    assert!(runtime.focus_first_focusable());
    let focused = runtime.input().focused();

    let effects = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    }));

    assert_eq!(invocations.load(Ordering::SeqCst), 1);
    assert_eq!(runtime.input().focused(), focused);
    assert!(effects.request_redraw, "handled actions wake the host");
}

#[derive(Clone)]
enum TabCommand {
    New,
    Close,
    Next,
    Previous,
    Select(usize),
}

#[test]
fn standard_tab_chords_map_to_typed_actions() {
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    let ctrl_shift = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::default()
    };
    let mut shortcuts = Shortcuts::new(Button::new("Target"))
        .bind(KeyChord::new(Key::Character('t'), ctrl), TabCommand::New)
        .bind(KeyChord::new(Key::Character('w'), ctrl), TabCommand::Close)
        .bind(KeyChord::new(Key::Tab, ctrl), TabCommand::Next)
        .bind(KeyChord::new(Key::Tab, ctrl_shift), TabCommand::Previous);
    for index in 1..=9 {
        shortcuts = shortcuts.bind(
            KeyChord::new(
                Key::Character(char::from_digit(index as u32, 10).unwrap()),
                ctrl,
            ),
            TabCommand::Select(index),
        );
    }

    let observed = Arc::new(AtomicUsize::new(usize::MAX));
    let callback_observed = observed.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(Actions::new(shortcuts, move |action| {
        let code = match action {
            TabCommand::New => 0,
            TabCommand::Close => 1,
            TabCommand::Next => 2,
            TabCommand::Previous => 3,
            TabCommand::Select(index) => 10 + index,
        };
        callback_observed.store(code, Ordering::SeqCst);
    }));
    runtime.update(now());

    let mut cases = vec![
        (Key::Character('t'), ctrl, 0),
        (Key::Character('w'), ctrl, 1),
        (Key::Tab, ctrl, 2),
        (Key::Tab, ctrl_shift, 3),
    ];
    cases.extend((1..=9).map(|index| {
        (
            Key::Character(char::from_digit(index as u32, 10).unwrap()),
            ctrl,
            10 + index,
        )
    }));

    for (key, modifiers, expected) in cases {
        let effects =
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
        assert_eq!(observed.load(Ordering::SeqCst), expected);
        assert!(effects.request_redraw);
    }
}
#[test]
fn theme_provider_overrides_button_primitives() {
    let mut theme = Theme::default();
    theme.button.normal.background = Color::RED;
    let mut runtime = Runtime::new();
    runtime.set_root(ThemeProvider::new(theme).child(Button::new("Themed")));
    runtime.update(now());

    let delta = runtime.pending_delta().expect("initial scene delta");
    assert!(delta.added.iter().any(|item| {
        matches!(
            item.primitive,
            Primitive::Quad {
                color: Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0
                },
                ..
            }
        )
    }));
}

#[test]
fn disabled_button_uses_the_resolved_theme_foreground() {
    let mut theme = Theme::default();
    theme.button.normal.foreground = Color::WHITE;
    theme.button.disabled.foreground = Color::RED;
    let mut runtime = Runtime::new();
    runtime.set_root(ThemeProvider::new(theme).child(Button::new("Disabled").disabled(true)));
    runtime.update(now());

    assert!(runtime.pending_delta().is_some_and(|delta| {
        delta.added.iter().any(|item| {
            matches!(
                item.primitive,
                Primitive::Text {
                    color: Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0
                    },
                    ..
                }
            )
        })
    }));
}

#[test]
fn signal_theme_changes_update_visuals_once_then_return_to_idle() {
    let theme = harbor_widget::signal::Signal::new_distinct(Theme::default());
    let mut runtime = Runtime::new();
    runtime.set_root(ThemeProvider::from_signal(theme.clone()).child(Button::new("Dynamic theme")));
    runtime.update(now());

    let mut changed = Theme::default();
    changed.button.normal.background = Color::RED;
    theme.set(changed);
    let effects = runtime.update(now());
    assert!(effects.request_redraw);
    assert!(runtime.pending_delta().is_some_and(|delta| {
        delta.added.iter().any(|item| {
            matches!(
                item.primitive,
                Primitive::Quad {
                    color: Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0
                    },
                    ..
                }
            )
        })
    }));

    let same_theme = theme.read().clone();
    theme.set(same_theme);
    let equal_update = runtime.update(now());
    assert!(!equal_update.request_redraw);

    let idle = runtime.update(now());
    assert!(!idle.request_redraw);
    assert!(idle.control_flow.is_none());
}
