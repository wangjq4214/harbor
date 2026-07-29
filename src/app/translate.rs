//! Bidirectional key translation: winit ↔ widget key ↔ winit modifiers.
//!
//! Extracted from `app.rs` so changes to key mappings are isolated in one file.

use winit::{
    event::{ElementState, Ime, MouseScrollDelta, WindowEvent},
    keyboard::{Key, ModifiersState, NamedKey},
};

use harbor_widget::input::event::{
    Key as WidgetKey, KeyboardEvent as WidgetKbEvent, Modifiers as WidgetModifiers, PointerButton,
    PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::Point as WidgetPoint;

/// Tracks whether winit has enabled composition input for a window.
///
/// Winit 0.30 sends committed composition text through `Ime::Commit`; while
/// composition is enabled, forwarding an unmodified character KeyDown as well
/// would insert that text twice. Physical shortcuts, navigation, and keypad
/// keys remain KeyDown-driven.
#[derive(Default)]
pub(crate) struct ImeState {
    enabled: bool,
}

impl ImeState {
    fn observe(&mut self, event: &Ime) {
        match event {
            Ime::Enabled => self.enabled = true,
            Ime::Disabled => self.enabled = false,
            Ime::Preedit(_, _) | Ime::Commit(_) => {}
        }
    }

    fn suppresses_character_key(
        &self,
        key: &Key,
        modifiers: ModifiersState,
        is_numpad: bool,
    ) -> bool {
        self.enabled
            && !is_numpad
            && !modifiers.control_key()
            && !modifiers.alt_key()
            && !modifiers.super_key()
            && matches!(key, Key::Character(_))
    }
}

fn ime_to_uievent(event: &Ime, state: &mut ImeState) -> Option<UiEvent> {
    state.observe(event);
    match event {
        Ime::Commit(text) if !text.is_empty() => {
            Some(UiEvent::Keyboard(WidgetKbEvent::Ime(text.clone())))
        }
        _ => None,
    }
}

/// Converts a winit `WindowEvent` without retaining composition state.
///
/// Secondary windows use this compatibility form; the terminal window uses
/// [`winit_to_uievent_with_ime`] so composition commits are de-duplicated.
pub(crate) fn winit_to_uievent(
    event: &WindowEvent,
    scale_factor: f32,
    modifiers: ModifiersState,
) -> Option<UiEvent> {
    winit_to_uievent_with_ime(event, scale_factor, modifiers, &mut ImeState::default())
}

/// Converts a winit `WindowEvent` while tracking composition state for a window.
pub(crate) fn winit_to_uievent_with_ime(
    event: &WindowEvent,
    scale_factor: f32,
    modifiers: ModifiersState,
    ime: &mut ImeState,
) -> Option<UiEvent> {
    match event {
        WindowEvent::KeyboardInput {
            device_id: _,
            event,
            is_synthetic: _,
        } => {
            let is_numpad = event.location == winit::keyboard::KeyLocation::Numpad;
            if event.state == ElementState::Pressed
                && ime.suppresses_character_key(&event.logical_key, modifiers, is_numpad)
            {
                return None;
            }
            let key = match &event.logical_key {
                Key::Named(NamedKey::Enter) if is_numpad => WidgetKey::NumpadEnter,
                Key::Named(named) => named_to_widget_key(named)?,
                Key::Character(ch) if is_numpad => {
                    WidgetKey::NumpadCharacter(ch.chars().next().unwrap_or('\0'))
                }
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
        WindowEvent::Ime(event) => ime_to_uievent(event, ime),
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

/// Converts winit modifier state into widget-framework modifier flags.
pub(crate) fn modifiers_to_widget(mods: ModifiersState) -> WidgetModifiers {
    WidgetModifiers {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key(),
        meta: mods.super_key(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_commit_is_the_only_composition_text_event() {
        let mut state = ImeState::default();
        assert!(ime_to_uievent(&Ime::Enabled, &mut state).is_none());
        assert!(state.suppresses_character_key(
            &Key::Character("a".into()),
            ModifiersState::default(),
            false,
        ));
        assert_eq!(
            ime_to_uievent(&Ime::Commit("語".into()), &mut state),
            Some(UiEvent::Keyboard(WidgetKbEvent::Ime("語".into())))
        );
    }

    #[test]
    fn ime_does_not_block_physical_shortcuts_navigation_or_keypad() {
        let mut state = ImeState::default();
        state.observe(&Ime::Enabled);

        assert!(!state.suppresses_character_key(
            &Key::Character("c".into()),
            ModifiersState::CONTROL,
            false,
        ));
        assert!(!state.suppresses_character_key(
            &Key::Named(NamedKey::ArrowUp),
            ModifiersState::default(),
            false,
        ));
        assert!(!state.suppresses_character_key(
            &Key::Character("1".into()),
            ModifiersState::default(),
            true,
        ));

        state.observe(&Ime::Disabled);
        assert!(!state.suppresses_character_key(
            &Key::Character("a".into()),
            ModifiersState::default(),
            false,
        ));
    }

    #[test]
    fn insert_and_function_keys_round_trip_through_widget_keys() {
        let expected = [
            (NamedKey::Insert, WidgetKey::Insert),
            (NamedKey::F1, WidgetKey::F1),
            (NamedKey::F2, WidgetKey::F2),
            (NamedKey::F3, WidgetKey::F3),
            (NamedKey::F4, WidgetKey::F4),
            (NamedKey::F5, WidgetKey::F5),
            (NamedKey::F6, WidgetKey::F6),
            (NamedKey::F7, WidgetKey::F7),
            (NamedKey::F8, WidgetKey::F8),
            (NamedKey::F9, WidgetKey::F9),
            (NamedKey::F10, WidgetKey::F10),
            (NamedKey::F11, WidgetKey::F11),
            (NamedKey::F12, WidgetKey::F12),
        ];

        for (named, widget) in expected {
            assert_eq!(named_to_widget_key(&named), Some(widget));
        }
    }
}
