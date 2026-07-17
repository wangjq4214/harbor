//! Keyboard event dispatch: semantic decisions about copy, scrollback
//! navigation, scroll-to-bottom, and PTY forwarding.

use crate::app::input::InputEncoder;
use crate::pty::Pty;
use crate::terminal::Terminal;
use harbor_types::InputModes;
use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};
use winit::window::Window;

use super::EventResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScrollbackNavigation {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

pub(super) fn scrollback_navigation(
    logical_key: &Key,
    modifiers: ModifiersState,
    is_alt_screen: bool,
) -> Option<ScrollbackNavigation> {
    if is_alt_screen
        || modifiers.shift_key()
        || modifiers.control_key()
        || modifiers.alt_key()
        || modifiers.super_key()
    {
        return None;
    }
    match logical_key {
        Key::Named(NamedKey::PageUp) => Some(ScrollbackNavigation::PageUp),
        Key::Named(NamedKey::PageDown) => Some(ScrollbackNavigation::PageDown),
        Key::Named(NamedKey::Home) => Some(ScrollbackNavigation::Top),
        Key::Named(NamedKey::End) => Some(ScrollbackNavigation::Bottom),
        _ => None,
    }
}

pub(super) struct KeyboardDispatch {
    pub scroll_to_bottom: bool,
    pub scrollback: Option<ScrollbackNavigation>,
    pub pty_bytes: Option<Vec<u8>>,
    pub needs_redraw: bool,
}

impl KeyboardDispatch {
    pub(super) fn none() -> Self {
        Self {
            scroll_to_bottom: false,
            scrollback: None,
            pty_bytes: None,
            needs_redraw: false,
        }
    }

    pub(super) fn decide(
        logical_key: &Key,
        text: Option<&str>,
        location: KeyLocation,
        modifiers: ModifiersState,
        is_alt_screen: bool,
        input_modes: InputModes,
        interaction_result: &EventResult,
    ) -> Self {
        let is_copy = modifiers.control_key()
            && matches!(logical_key, Key::Character(ch) if ch == "c" || ch == "C");
        let scroll_to_bottom =
            text.is_some() && !(*interaction_result == EventResult::Handled && is_copy);
        let needs_redraw = true;
        if *interaction_result == EventResult::Handled {
            return Self {
                scroll_to_bottom,
                scrollback: None,
                pty_bytes: None,
                needs_redraw,
            };
        }
        let scrollback = scrollback_navigation(logical_key, modifiers, is_alt_screen);
        if scrollback.is_some() {
            return Self {
                scroll_to_bottom,
                scrollback,
                pty_bytes: None,
                needs_redraw,
            };
        }
        let is_numpad = location == KeyLocation::Numpad;
        let pty_bytes = InputEncoder::key(logical_key, text, modifiers, input_modes, is_numpad)
            .map(|cow| cow.into_owned());
        Self {
            scroll_to_bottom,
            scrollback: None,
            pty_bytes,
            needs_redraw,
        }
    }

    pub(super) fn apply(self, terminal: &mut Terminal, pty: &mut Pty, window: &Window) {
        if self.scroll_to_bottom {
            terminal.scroll_viewport_to_bottom();
        }
        if let Some(nav) = self.scrollback {
            let page_rows = terminal.screen().rows();
            match nav {
                ScrollbackNavigation::PageUp => terminal.scroll_viewport_up(page_rows),
                ScrollbackNavigation::PageDown => terminal.scroll_viewport_down(page_rows),
                ScrollbackNavigation::Top => terminal.scroll_viewport_to_top(),
                ScrollbackNavigation::Bottom => terminal.scroll_viewport_to_bottom(),
            }
            window.request_redraw();
            return;
        }
        if self.needs_redraw {
            window.request_redraw();
        }
        if let Some(bytes) = self.pty_bytes {
            pty.write(&bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_with_text_suppresses_scroll() {
        // Ctrl+C with text and selection (Handled) — must not scroll or forward
        let d = KeyboardDispatch::decide(
            &Key::Character("c".into()),
            Some("c"),
            KeyLocation::Standard,
            ModifiersState::CONTROL,
            false,
            InputModes::default(),
            &EventResult::Handled,
        );
        assert!(!d.scroll_to_bottom, "copy must not scroll to bottom");
        assert!(d.scrollback.is_none());
        assert!(d.pty_bytes.is_none(), "copy must not forward to PTY");
        assert!(d.needs_redraw);
    }

    #[test]
    fn ctrl_c_without_selection_forwards() {
        let d = KeyboardDispatch::decide(
            &Key::Character("c".into()),
            None,
            KeyLocation::Standard,
            ModifiersState::CONTROL,
            false,
            InputModes::default(),
            &EventResult::Continue,
        );
        assert!(!d.scroll_to_bottom);
        assert_eq!(d.pty_bytes.as_deref(), Some(&[0x03u8][..]));
        assert!(d.needs_redraw);
    }

    #[test]
    fn text_key_scrolls_and_forwards() {
        let d = KeyboardDispatch::decide(
            &Key::Character("a".into()),
            Some("a"),
            KeyLocation::Standard,
            ModifiersState::default(),
            false,
            InputModes::default(),
            &EventResult::Continue,
        );
        assert!(d.scroll_to_bottom);
        assert_eq!(d.pty_bytes.as_deref(), Some(b"a".as_slice()));
        assert!(d.needs_redraw);
    }

    #[test]
    fn paste_scrolls_but_no_pty() {
        let d = KeyboardDispatch::decide(
            &Key::Character("v".into()),
            Some("v"),
            KeyLocation::Standard,
            ModifiersState::CONTROL,
            false,
            InputModes::default(),
            &EventResult::Handled,
        );
        assert!(d.scroll_to_bottom);
        assert!(d.pty_bytes.is_none());
        assert!(d.needs_redraw);
    }

    #[test]
    fn page_up_is_scrollback_nav() {
        let d = KeyboardDispatch::decide(
            &Key::Named(NamedKey::PageUp),
            None,
            KeyLocation::Standard,
            ModifiersState::default(),
            false,
            InputModes::default(),
            &EventResult::Continue,
        );
        assert!(!d.scroll_to_bottom);
        assert_eq!(d.scrollback, Some(ScrollbackNavigation::PageUp));
        assert!(d.pty_bytes.is_none());
        assert!(d.needs_redraw);
    }

    #[test]
    fn page_up_in_alt_screen_is_forwarded() {
        let d = KeyboardDispatch::decide(
            &Key::Named(NamedKey::PageUp),
            None,
            KeyLocation::Standard,
            ModifiersState::default(),
            true,
            InputModes::default(),
            &EventResult::Continue,
        );
        assert!(!d.scroll_to_bottom);
        assert!(d.scrollback.is_none());
        assert!(d.pty_bytes.is_some());
        assert!(d.needs_redraw);
    }
}
