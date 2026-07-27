//! Bidirectional key translation: winit ↔ widget key ↔ winit modifiers.
//!
//! Extracted from `app.rs` so changes to key mappings are isolated in one file.

use winit::{
    event::{ElementState, MouseScrollDelta, WindowEvent},
    keyboard::{Key, ModifiersState, NamedKey},
};

use harbor_widget::input::event::{
    Key as WidgetKey, KeyboardEvent as WidgetKbEvent, Modifiers as WidgetModifiers, PointerButton,
    PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::Point as WidgetPoint;

/// Converts a winit `WindowEvent` into a widget-framework `UiEvent`, or `None`
/// for events the widget framework does not consume.
pub(crate) fn winit_to_uievent(
    event: &WindowEvent,
    scale_factor: f32,
    modifiers: ModifiersState,
) -> Option<UiEvent> {
    match event {
        WindowEvent::KeyboardInput {
            device_id: _,
            event,
            is_synthetic: _,
        } => {
            let key = match &event.logical_key {
                Key::Named(named) => named_to_widget_key(named)?,
                Key::Character(ch) => WidgetKey::Character(ch.chars().next().unwrap_or('\0')),
                _ => return None,
            };
            let modifiers = modifiers_to_widget(modifiers);
            match event.state {
                ElementState::Pressed => {
                    Some(UiEvent::Keyboard(WidgetKbEvent::KeyDown { key, modifiers }))
                }
                ElementState::Released => {
                    Some(UiEvent::Keyboard(WidgetKbEvent::KeyUp { key, modifiers }))
                }
            }
        }
        WindowEvent::CursorMoved {
            device_id: _,
            position,
        } => {
            let pos = WidgetPoint::new(
                position.x as f32 / scale_factor,
                position.y as f32 / scale_factor,
            );
            Some(UiEvent::Pointer(PointerEvent::new(
                pos,
                PointerPhase::Move,
                PointerButton::Left,
                0,
            )))
        }
        WindowEvent::MouseInput {
            device_id: _,
            state,
            button,
        } => {
            let phase = match state {
                ElementState::Pressed => PointerPhase::Down,
                ElementState::Released => PointerPhase::Up,
            };
            let btn = match button {
                winit::event::MouseButton::Left => PointerButton::Left,
                winit::event::MouseButton::Right => PointerButton::Right,
                winit::event::MouseButton::Middle => PointerButton::Middle,
                _ => return None,
            };
            Some(UiEvent::Pointer(PointerEvent::new(
                WidgetPoint::ZERO,
                phase,
                btn,
                0,
            )))
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let (dx, dy) = match delta {
                MouseScrollDelta::LineDelta(x, y) => (*x, *y),
                MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
            };
            Some(UiEvent::Pointer(PointerEvent::new(
                WidgetPoint::ZERO,
                PointerPhase::Wheel { dx, dy },
                PointerButton::Left,
                0,
            )))
        }
        _ => None,
    }
}

/// Maps a winit `NamedKey` to a widget-framework `WidgetKey`.
pub(crate) fn named_to_widget_key(named: &NamedKey) -> Option<WidgetKey> {
    match named {
        NamedKey::Tab => Some(WidgetKey::Tab),
        NamedKey::Enter => Some(WidgetKey::Enter),
        NamedKey::Space => Some(WidgetKey::Space),
        NamedKey::Escape => Some(WidgetKey::Escape),
        NamedKey::Backspace => Some(WidgetKey::Backspace),
        NamedKey::Delete => Some(WidgetKey::Delete),
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

/// Converts winit modifier state into widget-framework modifier flags.
pub(crate) fn modifiers_to_widget(mods: ModifiersState) -> WidgetModifiers {
    WidgetModifiers {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key(),
        meta: mods.super_key(),
    }
}

/// Converts a widget `Key` back to a winit `Key` and optional character text.
pub(crate) fn widget_key_to_winit(key: &WidgetKey) -> (Key, Option<String>) {
    match key {
        WidgetKey::Tab => (Key::Named(NamedKey::Tab), None),
        WidgetKey::Enter => (Key::Named(NamedKey::Enter), Some("\r".into())),
        WidgetKey::Space => (Key::Character(" ".into()), Some(" ".into())),
        WidgetKey::Escape => (Key::Named(NamedKey::Escape), None),
        WidgetKey::Backspace => (Key::Named(NamedKey::Backspace), None),
        WidgetKey::Delete => (Key::Named(NamedKey::Delete), None),
        WidgetKey::ArrowUp => (Key::Named(NamedKey::ArrowUp), None),
        WidgetKey::ArrowDown => (Key::Named(NamedKey::ArrowDown), None),
        WidgetKey::ArrowLeft => (Key::Named(NamedKey::ArrowLeft), None),
        WidgetKey::ArrowRight => (Key::Named(NamedKey::ArrowRight), None),
        WidgetKey::Home => (Key::Named(NamedKey::Home), None),
        WidgetKey::End => (Key::Named(NamedKey::End), None),
        WidgetKey::PageUp => (Key::Named(NamedKey::PageUp), None),
        WidgetKey::PageDown => (Key::Named(NamedKey::PageDown), None),
        WidgetKey::Character(c) => {
            let s: String = c.to_string();
            (Key::Character(s.clone().into()), Some(s))
        }
    }
}

/// Converts widget-framework modifier flags back to winit modifier state.
pub(crate) fn widget_to_winit_mods(m: WidgetModifiers) -> winit::keyboard::ModifiersState {
    use winit::keyboard::ModifiersState;
    let mut state = ModifiersState::empty();
    if m.shift {
        state |= ModifiersState::SHIFT;
    }
    if m.ctrl {
        state |= ModifiersState::CONTROL;
    }
    if m.alt {
        state |= ModifiersState::ALT;
    }
    if m.meta {
        state |= ModifiersState::SUPER;
    }
    state
}
