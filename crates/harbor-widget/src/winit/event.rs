//! Winit window-event → UiEvent conversion helpers.

use crate::input::event::{Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, UiEvent};
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};

pub(super) fn ime_suppresses_keyboard(
    logical_key: &Key,
    state: ElementState,
    location: KeyLocation,
    modifiers: ModifiersState,
    ime_enabled: bool,
) -> bool {
    state == ElementState::Pressed
        && ime_enabled
        && location != KeyLocation::Numpad
        && !modifiers.control_key()
        && !modifiers.alt_key()
        && !modifiers.super_key()
        && matches!(logical_key, Key::Character(_))
}

/// Prefer the host-supplied physical size on DPI change; fall back to the
/// adapter's last known size only when the host omitted a hint (headless tests).
pub(super) fn physical_size_for_scale_change(
    host: Option<(u32, u32)>,
    current: (u32, u32),
) -> (u32, u32) {
    host.unwrap_or(current)
}

pub(super) fn mouse_button_index(button: MouseButton) -> Option<usize> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Right => Some(1),
        MouseButton::Middle => Some(2),
        _ => None,
    }
}

pub(super) fn mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Left),
        MouseButton::Right => Some(PointerButton::Right),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

/// Maps a winit named key to the platform-independent widget key.
pub(super) fn named_to_widget_key(named: &NamedKey) -> Option<WidgetKey> {
    match named {
        NamedKey::Tab => Some(WidgetKey::Tab),
        NamedKey::Enter => Some(WidgetKey::Enter),
        NamedKey::Space => Some(WidgetKey::Space),
        NamedKey::Escape => Some(WidgetKey::Escape),
        NamedKey::Backspace => Some(WidgetKey::Backspace),
        NamedKey::Insert => Some(WidgetKey::Insert),
        NamedKey::Delete => Some(WidgetKey::Delete),
        NamedKey::F1 => Some(WidgetKey::F1),
        NamedKey::F2 => Some(WidgetKey::F2),
        NamedKey::F3 => Some(WidgetKey::F3),
        NamedKey::F4 => Some(WidgetKey::F4),
        NamedKey::F5 => Some(WidgetKey::F5),
        NamedKey::F6 => Some(WidgetKey::F6),
        NamedKey::F7 => Some(WidgetKey::F7),
        NamedKey::F8 => Some(WidgetKey::F8),
        NamedKey::F9 => Some(WidgetKey::F9),
        NamedKey::F10 => Some(WidgetKey::F10),
        NamedKey::F11 => Some(WidgetKey::F11),
        NamedKey::F12 => Some(WidgetKey::F12),
        NamedKey::ArrowUp => Some(WidgetKey::ArrowUp),
        NamedKey::ArrowDown => Some(WidgetKey::ArrowDown),
        NamedKey::ArrowLeft => Some(WidgetKey::ArrowLeft),
        NamedKey::ArrowRight => Some(WidgetKey::ArrowRight),
        NamedKey::Home => Some(WidgetKey::Home),
        NamedKey::End => Some(WidgetKey::End),
        NamedKey::PageUp => Some(WidgetKey::PageUp),
        NamedKey::PageDown => Some(WidgetKey::PageDown),
        _ => None,
    }
}

pub(super) fn modifiers_to_widget(modifiers: ModifiersState) -> Modifiers {
    Modifiers {
        shift: modifiers.shift_key(),
        ctrl: modifiers.control_key(),
        alt: modifiers.alt_key(),
        meta: modifiers.super_key(),
    }
}

pub(super) fn keyboard_to_uievent(
    logical_key: &Key,
    state: ElementState,
    location: KeyLocation,
    modifiers: ModifiersState,
    ime_enabled: bool,
) -> Option<UiEvent> {
    let is_numpad = location == KeyLocation::Numpad;
    if state == ElementState::Pressed
        && ime_enabled
        && !is_numpad
        && !modifiers.control_key()
        && !modifiers.alt_key()
        && !modifiers.super_key()
        && matches!(logical_key, Key::Character(_))
    {
        return None;
    }

    let key = match logical_key {
        Key::Named(NamedKey::Enter) if is_numpad => WidgetKey::NumpadEnter,
        Key::Named(named) => named_to_widget_key(named)?,
        Key::Character(text) if is_numpad => WidgetKey::NumpadCharacter(text.chars().next()?),
        Key::Character(text) => WidgetKey::Character(text.chars().next()?),
        _ => return None,
    };
    let modifiers = modifiers_to_widget(modifiers);
    let event = match state {
        ElementState::Pressed => KeyboardEvent::KeyDown { key, modifiers },
        ElementState::Released => KeyboardEvent::KeyUp { key, modifiers },
    };
    Some(UiEvent::Keyboard(event))
}
