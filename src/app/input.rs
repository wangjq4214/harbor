//! Keyboard → PTY byte mapping.

use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Maps a logical key + optional text + modifier state to the byte sequence
/// to write to the PTY.  Named control/navigation keys are dispatched by
/// `logical_key` first.  When Ctrl is held and the key is a single ASCII
/// letter (a–z / A–Z), the corresponding control character (0x01–0x1A) is
/// emitted regardless of what winit places in `text`.
pub(super) fn keyboard_input_bytes(
    logical_key: &Key,
    text: Option<&str>,
    modifiers: ModifiersState,
) -> Option<Vec<u8>> {
    // Ctrl+letter → control character (0x01–0x1A).
    if modifiers.control_key()
        && let Key::Character(ch) = logical_key
        && let Some(ctrl_byte) = ctrl_letter_to_byte(ch)
    {
        return Some(vec![ctrl_byte]);
    }

    // If it's some other character with Ctrl held, fall through —
    // winit may have placed a control character in `text` already.
    match logical_key {
        Key::Named(NamedKey::Enter) => Some(b"\r".to_vec()),
        Key::Named(NamedKey::Backspace) => Some(b"\x7f".to_vec()),
        Key::Named(NamedKey::Tab) => Some(b"\t".to_vec()),
        Key::Named(NamedKey::Escape) => Some(b"\x1b".to_vec()),
        // Arrow keys → standard VT100 escape sequences.
        Key::Named(NamedKey::ArrowUp) => Some(b"\x1b[A".to_vec()),
        Key::Named(NamedKey::ArrowDown) => Some(b"\x1b[B".to_vec()),
        Key::Named(NamedKey::ArrowRight) => Some(b"\x1b[C".to_vec()),
        Key::Named(NamedKey::ArrowLeft) => Some(b"\x1b[D".to_vec()),
        // For everything else, send the UTF-8 text if present.
        _ => {
            let t = text?;
            if t.is_empty() {
                None
            } else {
                Some(t.as_bytes().to_vec())
            }
        }
    }
}

/// Converts a single-character `SmolStr` to its control-character byte
/// (`letter & 0x1F`).  Returns `None` for multi-codepoint strings or
/// non-ASCII letters.
fn ctrl_letter_to_byte(ch: &str) -> Option<u8> {
    let mut chars = ch.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None; // more than one codepoint — not a simple letter
    }
    match c {
        'a'..='z' => Some((c as u8) - b'a' + 1),
        'A'..='Z' => Some((c as u8) - b'A' + 1),
        _ => None,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    fn k(name: NamedKey) -> Key {
        Key::Named(name)
    }

    fn mods() -> ModifiersState {
        ModifiersState::default()
    }

    fn ctrl() -> ModifiersState {
        ModifiersState::CONTROL
    }

    #[test]
    fn backspace_with_unexpected_text_still_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some("\x17"), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_no_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), None, mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_empty_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some(""), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn enter_sends_cr() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Enter), None, mods()),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn escape_sends_esc() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Escape), None, mods()),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn arrow_up() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::ArrowUp), None, mods()),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn printable_character() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("a".into()), Some("a"), mods()),
            Some(b"a".to_vec())
        );
    }

    #[test]
    fn unrecognized_named_key_no_text_ignored() {
        assert_eq!(keyboard_input_bytes(&k(NamedKey::F1), None, mods()), None);
    }

    #[test]
    fn empty_text_ignored() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("".into()), Some(""), mods()),
            None
        );
    }

    // ── Ctrl+letter → control character ───────────────────────────────

    #[test]
    fn ctrl_c_sends_etx_with_text() {
        // Ctrl held + 'c' → 0x03, even if winit puts plain "c" in text.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), Some("c"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_c_sends_etx_without_text() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), None, ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_d_sends_eot() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("d".into()), Some("d"), ctrl()),
            Some(b"\x04".to_vec())
        );
    }

    #[test]
    fn ctrl_shift_c_still_sends_etx() {
        // Ctrl+Shift+C = Ctrl+C → 0x03.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("C".into()), Some("C"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn c_without_ctrl_is_plain_text() {
        // 'c' without Ctrl held → normal text.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), Some("c"), mods()),
            Some(b"c".to_vec())
        );
    }

    #[test]
    fn c_without_text_or_ctrl_is_none() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), None, mods()),
            None
        );
    }

    #[test]
    fn ctrl_non_letter_falls_through_to_text() {
        // Ctrl+1 is not a letter — should use winit's text if provided.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("1".into()), Some("1"), ctrl()),
            Some(b"1".to_vec())
        );
    }
}
