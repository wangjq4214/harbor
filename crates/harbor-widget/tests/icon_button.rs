use harbor_widget::effects::{CursorEffect, CursorShape};
use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::signal::Signal;
use harbor_widget::{Children, Icon, IconButton, LucideIcon, Theme, ThemeProvider, WithChildren};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn pointer(position: Point, phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(position, phase, PointerButton::Left, 0))
}

fn icon_primitive(runtime: &Runtime) -> Option<(LucideIcon, Rect, Color)> {
    let delta = runtime.pending_delta()?;
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match &item.primitive {
            Primitive::Icon { icon, rect, color } => Some((*icon, *rect, *color)),
            _ => None,
        })
}

fn painted_text(runtime: &Runtime) -> bool {
    runtime.pending_delta().is_some_and(|delta| {
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .any(|item| matches!(&item.primitive, Primitive::Text { .. }))
    })
}

#[test]
fn icon_button_paints_only_its_icon_with_the_theme_size_and_color() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(IconButton::new(LucideIcon::Plus, "New terminal"));
    runtime.update(Instant::now());

    let theme = Theme::default();
    let (icon, rect, color) = icon_primitive(&runtime).expect("icon primitive");
    assert_eq!(icon, LucideIcon::Plus);
    assert_eq!(
        rect.size(),
        Size::new(theme.icon_button.icon_size, theme.icon_button.icon_size)
    );
    assert_eq!(color, theme.icon_button.normal.foreground);
    assert!(!painted_text(&runtime), "the label is never painted");
    assert_eq!(
        IconButton::new(LucideIcon::Plus, "New terminal").label(),
        "New terminal"
    );
}

#[test]
fn icon_button_tint_follows_pointer_interaction_states() {
    let mut theme = Theme::default();
    theme.icon_button.normal.foreground = Color::RED;
    theme.icon_button.hovered.foreground = Color::GREEN;
    theme.icon_button.pressed.foreground = Color::BLUE;
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(ThemeProvider::new(theme).child(IconButton::new(LucideIcon::X, "Close")));
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::RED);

    runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Move));
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::GREEN);

    runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Down));
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::BLUE);

    runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Up));
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::GREEN);
}

#[test]
fn disabled_icon_button_uses_the_disabled_tint_and_does_not_activate() {
    let activations = Arc::new(AtomicUsize::new(0));
    let observed = activations.clone();
    let mut theme = Theme::default();
    theme.icon_button.normal.foreground = Color::RED;
    theme.icon_button.disabled.foreground = Color::WHITE;
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(
        ThemeProvider::new(theme).child(
            IconButton::new(LucideIcon::X, "Close")
                .disabled(true)
                .on_click(move |_| {
                    observed.fetch_add(1, Ordering::SeqCst);
                }),
        ),
    );
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::WHITE);

    let effects = runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Move));
    assert_eq!(
        effects.cursor,
        Some(CursorEffect::Set(CursorShape::Default))
    );
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(pointer(Point::new(8.0, 8.0), phase));
        runtime.update(Instant::now());
    }
    assert_eq!(activations.load(Ordering::SeqCst), 0);
}

#[test]
fn enabled_icon_button_activates_from_keyboard_and_pointer() {
    let activations = Arc::new(AtomicUsize::new(0));
    let observed = activations.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(IconButton::new(LucideIcon::Plus, "New").on_click(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
    }));
    runtime.update(Instant::now());

    assert!(runtime.focus_first_focusable());
    runtime.update(Instant::now());
    for key in [Key::Enter, Key::Space] {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
        }));
    }
    assert_eq!(activations.load(Ordering::SeqCst), 2);

    runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Down));
    runtime.dispatch(pointer(Point::new(8.0, 8.0), PointerPhase::Up));
    runtime.update(Instant::now());
    assert_eq!(activations.load(Ordering::SeqCst), 3);
}

#[test]
fn color_with_overrides_resolution_and_observes_focus() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(IconButton::new(LucideIcon::X, "Close").color_with(|state| {
        if state.focused {
            Color::RED
        } else {
            Color::BLUE
        }
    }));
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::BLUE);

    assert!(runtime.focus_first_focusable());
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::RED);
}

#[test]
fn style_and_icon_size_overrides_win_over_the_theme() {
    let mut expected = Theme::default();
    expected.icon_button.icon_size = 24.0;

    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(ThemeProvider::new(expected).child(IconButton::new(LucideIcon::Plus, "New")));
    runtime.update(Instant::now());
    let (_, rect, _) = icon_primitive(&runtime).unwrap();
    assert_eq!(rect.size(), Size::new(24.0, 24.0));

    let mut style = Theme::default().icon_button.clone();
    style.icon_size = 30.0;
    style.normal.foreground = Color::GREEN;
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(
        IconButton::new(LucideIcon::Plus, "New")
            .style(style)
            .icon_size(12.0),
    );
    runtime.update(Instant::now());
    let (_, rect, color) = icon_primitive(&runtime).unwrap();
    assert_eq!(rect.size(), Size::new(12.0, 12.0));
    assert_eq!(color, Color::GREEN);
}

#[test]
fn icon_button_tint_follows_theme_updates() {
    let mut theme = Theme::default();
    theme.icon_button.normal.foreground = Color::RED;
    let theme = Signal::new(theme);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(
        ThemeProvider::from_signal(theme.clone()).child(IconButton::new(LucideIcon::Dot, "Unread")),
    );
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::RED);

    let mut next = theme.read().clone();
    next.icon_button.normal.foreground = Color::GREEN;
    theme.set(next);
    runtime.update(Instant::now());
    assert_eq!(icon_primitive(&runtime).unwrap().2, Color::GREEN);
}

#[test]
fn icon_button_is_a_public_macro_leaf() {
    assert!(
        IconButton::new(LucideIcon::X, "Close")
            .with_children(Children::one(Icon::new(LucideIcon::Plus)))
            .is_err()
    );

    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(40, 40, 1.0));
    runtime.set_root(|cx: &mut harbor_widget::view::BuildCx| {
        harbor_widget::view! { IconButton::new(LucideIcon::X, "Close"); }
    });
    runtime.update(Instant::now());
    assert!(icon_primitive(&runtime).is_some_and(|(icon, _, _)| icon == LucideIcon::X));
}
