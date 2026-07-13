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
    // Calculate ANSI modifier code:
    // Base is 1. Shift: +1, Alt: +2, Ctrl: +4, Super: +8.
    let mut modifier_code = 1;
    if modifiers.shift_key() {
        modifier_code += 1;
    }
    if modifiers.alt_key() {
        modifier_code += 2;
    }
    if modifiers.control_key() {
        modifier_code += 4;
    }
    if modifiers.super_key() {
        modifier_code += 8;
    }

    // Application Keypad mode (only when no modifiers are pressed)
    if application_keypad && is_numpad && modifier_code == 1 {
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
    // Prepend ESC (0x1b) if Alt is also pressed.
    if modifiers.control_key()
        && let Key::Character(ch) = logical_key
        && let Some(ctrl_byte) = ctrl_letter_to_byte(ch)
    {
        if modifiers.alt_key() {
            return Some(vec![0x1b, ctrl_byte]);
        } else {
            return Some(vec![ctrl_byte]);
        }
    }

    // If it's some other character with Ctrl held, fall through —
    // winit may have placed a control character in `text` already.
    match logical_key {
        Key::Named(NamedKey::Enter) => Some(b"\r".to_vec()),
        Key::Named(NamedKey::Backspace) => Some(b"\x7f".to_vec()),
        Key::Named(NamedKey::Tab) => {
            if modifiers.shift_key() {
                Some(b"\x1b[Z".to_vec())
            } else {
                Some(b"\t".to_vec())
            }
        }
        Key::Named(NamedKey::Escape) => Some(b"\x1b".to_vec()),

        // Arrow keys
        Key::Named(NamedKey::ArrowUp) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}A", modifier_code).into_bytes())
            } else if application_cursor {
                Some(b"\x1bOA".to_vec())
            } else {
                Some(b"\x1b[A".to_vec())
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}B", modifier_code).into_bytes())
            } else if application_cursor {
                Some(b"\x1bOB".to_vec())
            } else {
                Some(b"\x1b[B".to_vec())
            }
        }
        Key::Named(NamedKey::ArrowRight) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}C", modifier_code).into_bytes())
            } else if application_cursor {
                Some(b"\x1bOC".to_vec())
            } else {
                Some(b"\x1b[C".to_vec())
            }
        }
        Key::Named(NamedKey::ArrowLeft) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}D", modifier_code).into_bytes())
            } else if application_cursor {
                Some(b"\x1bOD".to_vec())
            } else {
                Some(b"\x1b[D".to_vec())
            }
        }

        // Home / End
        Key::Named(NamedKey::Home) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}H", modifier_code).into_bytes())
            } else if application_cursor {
                Some(b"\x1bOH".to_vec())
            } else {
                Some(b"\x1b[H".to_vec())
            }
        }
        Key::Named(NamedKey::End) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}F", modifier_code).into_bytes())
            } else if application_cursor {
                Some(b"\x1bOF".to_vec())
            } else {
                Some(b"\x1b[F".to_vec())
            }
        }

        // Function keys F1-F4: shift from SS3 to CSI when parameterized
        Key::Named(NamedKey::F1) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}P", modifier_code).into_bytes())
            } else {
                Some(b"\x1bOP".to_vec())
            }
        }
        Key::Named(NamedKey::F2) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}Q", modifier_code).into_bytes())
            } else {
                Some(b"\x1bOQ".to_vec())
            }
        }
        Key::Named(NamedKey::F3) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}R", modifier_code).into_bytes())
            } else {
                Some(b"\x1bOR".to_vec())
            }
        }
        Key::Named(NamedKey::F4) => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}S", modifier_code).into_bytes())
            } else {
                Some(b"\x1bOS".to_vec())
            }
        }

        // Function keys F5-F12
        Key::Named(NamedKey::F5) => {
            if modifier_code > 1 {
                Some(format!("\x1b[15;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[15~".to_vec())
            }
        }
        Key::Named(NamedKey::F6) => {
            if modifier_code > 1 {
                Some(format!("\x1b[17;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[17~".to_vec())
            }
        }
        Key::Named(NamedKey::F7) => {
            if modifier_code > 1 {
                Some(format!("\x1b[18;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[18~".to_vec())
            }
        }
        Key::Named(NamedKey::F8) => {
            if modifier_code > 1 {
                Some(format!("\x1b[19;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[19~".to_vec())
            }
        }
        Key::Named(NamedKey::F9) => {
            if modifier_code > 1 {
                Some(format!("\x1b[20;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[20~".to_vec())
            }
        }
        Key::Named(NamedKey::F10) => {
            if modifier_code > 1 {
                Some(format!("\x1b[21;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[21~".to_vec())
            }
        }
        Key::Named(NamedKey::F11) => {
            if modifier_code > 1 {
                Some(format!("\x1b[23;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[23~".to_vec())
            }
        }
        Key::Named(NamedKey::F12) => {
            if modifier_code > 1 {
                Some(format!("\x1b[24;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[24~".to_vec())
            }
        }

        // Editing keys
        Key::Named(NamedKey::Insert) => {
            if modifier_code > 1 {
                Some(format!("\x1b[2;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[2~".to_vec())
            }
        }
        Key::Named(NamedKey::Delete) => {
            if modifier_code > 1 {
                Some(format!("\x1b[3;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[3~".to_vec())
            }
        }
        Key::Named(NamedKey::PageUp) => {
            if modifier_code > 1 {
                Some(format!("\x1b[5;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[5~".to_vec())
            }
        }
        Key::Named(NamedKey::PageDown) => {
            if modifier_code > 1 {
                Some(format!("\x1b[6;{}~", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[6~".to_vec())
            }
        }

        _ => {
            let t = if let Some(t) = text {
                t
            } else if modifiers.alt_key()
                && let Key::Character(ch) = logical_key
            {
                ch.as_str()
            } else {
                ""
            };

            if t.is_empty() {
                None
            } else {
                let bytes = t.as_bytes().to_vec();
                if modifiers.alt_key() {
                    let mut result = vec![0x1b];
                    result.extend_from_slice(&bytes);
                    Some(result)
                } else {
                    Some(bytes)
                }
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

    #[test]
    fn test_phase6_modifiers_matrix() {
        let shift = ModifiersState::SHIFT;
        let alt = ModifiersState::ALT;
        let ctrl = ModifiersState::CONTROL;
        let alt_ctrl = ModifiersState::ALT | ModifiersState::CONTROL;
        let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;

        // 1. ArrowUp:
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, mods()),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, shift),
            Some(b"\x1b[1;2A".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, alt),
            Some(b"\x1b[1;3A".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, ctrl),
            Some(b"\x1b[1;5A".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, ctrl_shift),
            Some(b"\x1b[1;6A".to_vec())
        );

        // 2. Home / End:
        assert_eq!(
            test_bytes(&k(NamedKey::Home), None, mods()),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::Home), None, ctrl),
            Some(b"\x1b[1;5H".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::End), None, ctrl),
            Some(b"\x1b[1;5F".to_vec())
        );

        // 3. Insert / PageUp:
        assert_eq!(
            test_bytes(&k(NamedKey::Insert), None, mods()),
            Some(b"\x1b[2~".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::Insert), None, ctrl),
            Some(b"\x1b[2;5~".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::PageUp), None, ctrl),
            Some(b"\x1b[5;5~".to_vec())
        );

        // 4. F1 (SS3 -> CSI when parameterized):
        assert_eq!(
            test_bytes(&k(NamedKey::F1), None, mods()),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::F1), None, ctrl),
            Some(b"\x1b[1;5P".to_vec())
        );

        // 5. F5:
        assert_eq!(
            test_bytes(&k(NamedKey::F5), None, mods()),
            Some(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::F5), None, ctrl),
            Some(b"\x1b[15;5~".to_vec())
        );

        // 6. Shift+Tab -> \x1b[Z
        assert_eq!(
            test_bytes(&k(NamedKey::Tab), None, shift),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::Tab), None, mods()),
            Some(b"\t".to_vec())
        );

        // 7. Alt + printable characters
        assert_eq!(
            test_bytes(&Key::Character("a".into()), Some("a"), alt),
            Some(b"\x1ba".to_vec())
        );
        assert_eq!(
            test_bytes(&Key::Character("a".into()), None, alt),
            Some(b"\x1ba".to_vec())
        );
        assert_eq!(
            test_bytes(&Key::Character("语".into()), Some("语"), alt),
            Some([vec![0x1b], "语".as_bytes().to_vec()].concat())
        );

        // 8. Alt + Ctrl + letter (Alt + Ctrl + C -> \x1b\x03)
        assert_eq!(
            test_bytes(&Key::Character("c".into()), None, alt_ctrl),
            Some(vec![0x1b, 0x03])
        );
    }

    #[test]
    fn test_numpad_modifier_and_numlock() {
        let ctrl = ModifiersState::CONTROL;
        let mods = ModifiersState::default();

        // 1. NumLock ON (represented by Key::Character)
        // 1a. application_keypad = false, no modifiers -> standard character "8"
        assert_eq!(
            keyboard_input_bytes(
                &Key::Character("8".into()),
                Some("8"),
                mods,
                false,
                false,
                true
            ),
            Some(b"8".to_vec())
        );
        // 1b. application_keypad = true, no modifiers -> application keypad sequence \x1bOx
        assert_eq!(
            keyboard_input_bytes(
                &Key::Character("8".into()),
                Some("8"),
                mods,
                false,
                true,
                true
            ),
            Some(b"\x1bOx".to_vec())
        );
        // 1c. application_keypad = true, Ctrl modifier -> application keypad is bypassed, sends ctrl fallback
        assert_eq!(
            keyboard_input_bytes(
                &Key::Character("8".into()),
                Some("8"),
                ctrl,
                false,
                true,
                true
            ),
            Some(b"8".to_vec())
        );

        // 2. NumLock OFF (represented by Key::Named)
        // 2a. application_keypad = false, no modifiers -> standard ArrowUp \x1b[A
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::ArrowUp), None, mods, false, false, true),
            Some(b"\x1b[A".to_vec())
        );
        // 2b. application_keypad = true, no modifiers -> standard ArrowUp \x1b[A
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::ArrowUp), None, mods, false, true, true),
            Some(b"\x1b[A".to_vec())
        );
        // 2c. Ctrl modifier -> sends Ctrl+ArrowUp \x1b[1;5A
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::ArrowUp), None, ctrl, false, true, true),
            Some(b"\x1b[1;5A".to_vec())
        );
    }
}
