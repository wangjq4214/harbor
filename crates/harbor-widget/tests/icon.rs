use harbor_widget::input::event::{Key, KeyboardEvent, UiEvent};
use harbor_widget::layout::Size;
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::signal::Signal;
use harbor_widget::{Children, Column, Icon, LucideIcon, Theme, ThemeProvider, WithChildren};
use std::time::Instant;

#[test]
fn icons_inherit_theme_and_retain_identity_on_theme_updates() {
    let mut theme = Theme::default();
    theme.icon_button.normal.foreground = Color::RED;
    let theme = Signal::new(theme);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(80, 80, 1.0));
    runtime.set_root(
        ThemeProvider::from_signal(theme.clone()).child(
            Column::new()
                .with_children(Children::from_iter([
                    Icon::new(LucideIcon::Terminal),
                    Icon::new(LucideIcon::Plus).color(Color::BLUE),
                ]))
                .unwrap(),
        ),
    );
    runtime.update(Instant::now());
    let delta = runtime.pending_delta().unwrap();
    assert_eq!(delta.added.len(), 2);
    let inherited_id = delta
        .added
        .iter()
        .find(|item| {
            matches!(
                item.primitive,
                Primitive::Icon {
                    icon: LucideIcon::Terminal,
                    color: Color::RED,
                    ..
                }
            )
        })
        .unwrap()
        .id;
    assert!(delta.added.iter().any(|item| matches!(
        item.primitive,
        Primitive::Icon {
            icon: LucideIcon::Plus,
            color: Color::BLUE,
            ..
        }
    )));
    let mut next = theme.read().clone();
    next.icon_button.normal.foreground = Color::GREEN;
    theme.set(next);
    runtime.update(Instant::now());
    let delta = runtime.pending_delta().unwrap();
    assert!(delta.removed.is_empty() && delta.modified.is_empty());
    // Pending, unencoded additions coalesce to their newest contents.
    assert_eq!(delta.added.len(), 2);
    let inherited = delta
        .added
        .iter()
        .find(|item| item.id == inherited_id)
        .unwrap();
    assert!(matches!(
        inherited.primitive,
        Primitive::Icon {
            color: Color::GREEN,
            ..
        }
    ));
    assert!(delta.added.iter().any(|item| matches!(
        item.primitive,
        Primitive::Icon {
            icon: LucideIcon::Plus,
            color: Color::BLUE,
            ..
        }
    )));
}

#[test]
fn icon_is_a_public_macro_leaf_without_keyboard_activation() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(40, 40, 1.0));
    runtime.set_root(|cx: &mut harbor_widget::view::BuildCx| {
        harbor_widget::view! { Icon::new(LucideIcon::X).size(16.0); }
    });
    runtime.update(Instant::now());
    assert!(
        matches!(runtime.pending_delta().unwrap().added[0].primitive,
        Primitive::Icon { icon: LucideIcon::X, rect, .. } if rect.size() == Size::new(16.0, 16.0))
    );
    let effects = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Enter,
        modifiers: Default::default(),
    }));
    assert!(!effects.request_redraw);
    assert!(runtime.input().focused().is_none());
}
