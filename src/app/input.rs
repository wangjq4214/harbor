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
    application_cursor: bool,
    application_keypad: bool,
    is_numpad: bool,
) -> Option<Vec<u8>> {
    // Application Keypad mode
    if application_keypad && is_numpad {
        let keypad_seq = match logical_key {
            Key::Character(ch) => match ch.as_str() {
                "0" => Some(b"\x1bOp".to_vec()),
                "1" => Some(b"\x1bOq".to_vec()),
                "2" => Some(b"\x1bOr".to_vec()),
                "3" => Some(b"\x1bOs".to_vec()),
                "4" => Some(b"\x1bOt".to_vec()),
                "5" => Some(b"\x1bOu".to_vec()),
                "6" => Some(b"\x1bOv".to_vec()),
                "7" => Some(b"\x1bOw".to_vec()),
                "8" => Some(b"\x1bOx".to_vec()),
                "9" => Some(b"\x1bOy".to_vec()),
                "." => Some(b"\x1bOn".to_vec()),
                "-" => Some(b"\x1bOm".to_vec()),
                "+" => Some(b"\x1bOk".to_vec()),
                "/" => Some(b"\x1bOo".to_vec()),
                "*" => Some(b"\x1bOj".to_vec()),
                "," => Some(b"\x1bOl".to_vec()),
                "=" => Some(b"\x1bOX".to_vec()),
                _ => None,
            },
            Key::Named(NamedKey::Enter) => Some(b"\x1bOM".to_vec()),
            _ => None,
        };
        if keypad_seq.is_some() {
            return keypad_seq;
        }
    }

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

        // Arrow keys
        Key::Named(NamedKey::ArrowUp) => {
            if application_cursor {
                Some(b"\x1bOA".to_vec())
            } else {
                Some(b"\x1b[A".to_vec())
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            if application_cursor {
                Some(b"\x1bOB".to_vec())
            } else {
                Some(b"\x1b[B".to_vec())
            }
        }
        Key::Named(NamedKey::ArrowRight) => {
            if application_cursor {
                Some(b"\x1bOC".to_vec())
            } else {
                Some(b"\x1b[C".to_vec())
            }
        }
        Key::Named(NamedKey::ArrowLeft) => {
            if application_cursor {
                Some(b"\x1bOD".to_vec())
            } else {
                Some(b"\x1b[D".to_vec())
            }
        }

        // Home / End
        Key::Named(NamedKey::Home) => {
            if application_cursor {
                Some(b"\x1bOH".to_vec())
            } else {
                Some(b"\x1b[H".to_vec())
            }
        }
        Key::Named(NamedKey::End) => {
            if application_cursor {
                Some(b"\x1bOF".to_vec())
            } else {
                Some(b"\x1b[F".to_vec())
            }
        }

        // Function keys
        Key::Named(NamedKey::F1) => Some(b"\x1bOP".to_vec()),
        Key::Named(NamedKey::F2) => Some(b"\x1bOQ".to_vec()),
        Key::Named(NamedKey::F3) => Some(b"\x1bOR".to_vec()),
        Key::Named(NamedKey::F4) => Some(b"\x1bOS".to_vec()),
        Key::Named(NamedKey::F5) => Some(b"\x1b[15~".to_vec()),
        Key::Named(NamedKey::F6) => Some(b"\x1b[17~".to_vec()),
        Key::Named(NamedKey::F7) => Some(b"\x1b[18~".to_vec()),
        Key::Named(NamedKey::F8) => Some(b"\x1b[19~".to_vec()),
        Key::Named(NamedKey::F9) => Some(b"\x1b[20~".to_vec()),
        Key::Named(NamedKey::F10) => Some(b"\x1b[21~".to_vec()),
        Key::Named(NamedKey::F11) => Some(b"\x1b[23~".to_vec()),
        Key::Named(NamedKey::F12) => Some(b"\x1b[24~".to_vec()),

        // Editing keys
        Key::Named(NamedKey::Insert) => Some(b"\x1b[2~".to_vec()),
        Key::Named(NamedKey::Delete) => Some(b"\x1b[3~".to_vec()),
        Key::Named(NamedKey::PageUp) => Some(b"\x1b[5~".to_vec()),
        Key::Named(NamedKey::PageDown) => Some(b"\x1b[6~".to_vec()),

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
    use crate::terminal::Terminal;
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

    fn test_bytes(key: &Key, text: Option<&str>, m: ModifiersState) -> Option<Vec<u8>> {
        keyboard_input_bytes(key, text, m, false, false, false)
    }

    #[test]
    fn backspace_with_unexpected_text_still_sends_del() {
        assert_eq!(
            test_bytes(&k(NamedKey::Backspace), Some("\x17"), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_no_text_sends_del() {
        assert_eq!(
            test_bytes(&k(NamedKey::Backspace), None, mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_empty_text_sends_del() {
        assert_eq!(
            test_bytes(&k(NamedKey::Backspace), Some(""), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn enter_sends_cr() {
        assert_eq!(
            test_bytes(&k(NamedKey::Enter), None, mods()),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn escape_sends_esc() {
        assert_eq!(
            test_bytes(&k(NamedKey::Escape), None, mods()),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn arrow_up() {
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, mods()),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn printable_character() {
        assert_eq!(
            test_bytes(&Key::Character("a".into()), Some("a"), mods()),
            Some(b"a".to_vec())
        );
    }

    #[test]
    fn unrecognized_named_key_no_text_ignored() {
        assert_eq!(
            test_bytes(&k(NamedKey::AudioVolumeMute), None, mods()),
            None
        );
    }

    #[test]
    fn empty_text_ignored() {
        assert_eq!(
            test_bytes(&Key::Character("".into()), Some(""), mods()),
            None
        );
    }

    // ── Ctrl+letter → control character ───────────────────────────────

    #[test]
    fn ctrl_c_sends_etx_with_text() {
        // Ctrl held + 'c' → 0x03, even if winit puts plain "c" in text.
        assert_eq!(
            test_bytes(&Key::Character("c".into()), Some("c"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_c_sends_etx_without_text() {
        assert_eq!(
            test_bytes(&Key::Character("c".into()), None, ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_d_sends_eot() {
        assert_eq!(
            test_bytes(&Key::Character("d".into()), Some("d"), ctrl()),
            Some(b"\x04".to_vec())
        );
    }

    #[test]
    fn ctrl_shift_c_still_sends_etx() {
        // Ctrl+Shift+C = Ctrl+C → 0x03.
        assert_eq!(
            test_bytes(&Key::Character("C".into()), Some("C"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn c_without_ctrl_is_plain_text() {
        // 'c' without Ctrl held → normal text.
        assert_eq!(
            test_bytes(&Key::Character("c".into()), Some("c"), mods()),
            Some(b"c".to_vec())
        );
    }

    #[test]
    fn c_without_text_or_ctrl_is_none() {
        assert_eq!(test_bytes(&Key::Character("c".into()), None, mods()), None);
    }

    #[test]
    fn ctrl_non_letter_falls_through_to_text() {
        // Ctrl+1 is not a letter — should use winit's text if provided.
        assert_eq!(
            test_bytes(&Key::Character("1".into()), Some("1"), ctrl()),
            Some(b"1".to_vec())
        );
    }

    #[test]
    fn test_keypad_mode_input_encoding() {
        use winit::keyboard::{Key, ModifiersState, NamedKey};

        let key_up = Key::Named(NamedKey::ArrowUp);
        let key_enter = Key::Named(NamedKey::Enter);
        let key_1 = Key::Character("1".into());

        let mods = ModifiersState::default();

        // Standard/default: application modes off
        assert_eq!(
            keyboard_input_bytes(&key_up, None, mods, false, false, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            keyboard_input_bytes(&key_1, Some("1"), mods, false, false, true),
            Some(b"1".to_vec())
        );
        assert_eq!(
            keyboard_input_bytes(&key_enter, None, mods, false, false, true),
            Some(b"\r".to_vec())
        );

        // Application Cursor Keys on
        assert_eq!(
            keyboard_input_bytes(&key_up, None, mods, true, false, false),
            Some(b"\x1bOA".to_vec())
        );

        // Application Keypad on, but is_numpad false
        assert_eq!(
            keyboard_input_bytes(&key_1, Some("1"), mods, false, true, false),
            Some(b"1".to_vec())
        );

        // Application Keypad on and is_numpad true
        assert_eq!(
            keyboard_input_bytes(&key_1, Some("1"), mods, false, true, true),
            Some(b"\x1bOq".to_vec())
        );
        assert_eq!(
            keyboard_input_bytes(&key_enter, None, mods, false, true, true),
            Some(b"\x1bOM".to_vec())
        );
    }

    #[test]
    fn resets_restore_normal_cursor_and_keypad_encodings() {
        let key_up = Key::Named(NamedKey::ArrowUp);
        let key_1 = Key::Character("1".into());

        for (name, reset) in [
            ("RIS", b"\x1bc".as_slice()),
            ("DECSTR", b"\x1b[!p".as_slice()),
        ] {
            let mut terminal = Terminal::new(3, 3);
            terminal.put_bytes(b"\x1b[?1h\x1b=");

            assert_eq!(
                keyboard_input_bytes(
                    &key_up,
                    None,
                    mods(),
                    terminal.screen().application_cursor,
                    terminal.screen().application_keypad,
                    false,
                ),
                Some(b"\x1bOA".to_vec()),
                "{name} setup did not enable application cursor mode"
            );
            assert_eq!(
                keyboard_input_bytes(
                    &key_1,
                    Some("1"),
                    mods(),
                    terminal.screen().application_cursor,
                    terminal.screen().application_keypad,
                    true,
                ),
                Some(b"\x1bOq".to_vec()),
                "{name} setup did not enable application keypad mode"
            );

            terminal.put_bytes(reset);

            assert_eq!(
                keyboard_input_bytes(
                    &key_up,
                    None,
                    mods(),
                    terminal.screen().application_cursor,
                    terminal.screen().application_keypad,
                    false,
                ),
                Some(b"\x1b[A".to_vec()),
                "{name} did not reset application cursor mode"
            );
            assert_eq!(
                keyboard_input_bytes(
                    &key_1,
                    Some("1"),
                    mods(),
                    terminal.screen().application_cursor,
                    terminal.screen().application_keypad,
                    true,
                ),
                Some(b"1".to_vec()),
                "{name} did not reset application keypad mode"
            );
        }
    }
}
