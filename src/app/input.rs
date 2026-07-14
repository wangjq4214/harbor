//! Keyboard → PTY byte mapping.

use std::borrow::Cow;
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Lightweight snapshot of Screen state relevant to keyboard mapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyboardConfig {
    pub(super) application_cursor: bool,
    pub(super) application_keypad: bool,
}

fn modifier_code(modifiers: ModifiersState) -> u8 {
    let mut code = 1;
    if modifiers.shift_key() {
        code += 1;
    }
    if modifiers.alt_key() {
        code += 2;
    }
    if modifiers.control_key() {
        code += 4;
    }
    if modifiers.super_key() {
        code += 8;
    }
    code
}

/// Application Keypad mode SS3 sequences. Called only when
/// `application_keypad && is_numpad && modifier_code == 1`.
fn keypad_sequence(logical_key: &Key) -> Option<Cow<'static, [u8]>> {
    match logical_key {
        Key::Character(ch) => match ch.as_str() {
            "0" => Some(Cow::Borrowed(b"\x1bOp")),
            "1" => Some(Cow::Borrowed(b"\x1bOq")),
            "2" => Some(Cow::Borrowed(b"\x1bOr")),
            "3" => Some(Cow::Borrowed(b"\x1bOs")),
            "4" => Some(Cow::Borrowed(b"\x1bOt")),
            "5" => Some(Cow::Borrowed(b"\x1bOu")),
            "6" => Some(Cow::Borrowed(b"\x1bOv")),
            "7" => Some(Cow::Borrowed(b"\x1bOw")),
            "8" => Some(Cow::Borrowed(b"\x1bOx")),
            "9" => Some(Cow::Borrowed(b"\x1bOy")),
            "." => Some(Cow::Borrowed(b"\x1bOn")),
            "-" => Some(Cow::Borrowed(b"\x1bOm")),
            "+" => Some(Cow::Borrowed(b"\x1bOk")),
            "/" => Some(Cow::Borrowed(b"\x1bOo")),
            "*" => Some(Cow::Borrowed(b"\x1bOj")),
            "," => Some(Cow::Borrowed(b"\x1bOl")),
            "=" => Some(Cow::Borrowed(b"\x1bOX")),
            _ => None,
        },
        Key::Named(NamedKey::Enter) => Some(Cow::Borrowed(b"\x1bOM")),
        _ => None,
    }
}

/// Catch-all for character keys not handled by named-key or Ctrl paths.
/// Returns the UTF-8 bytes of `text` (or the logical-key's string when no
/// text is provided and Alt is held), optionally prefixed with ESC.
fn text_fallback(
    logical_key: &Key,
    text: Option<&str>,
    modifiers: ModifiersState,
) -> Option<Cow<'static, [u8]>> {
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
            Some(Cow::Owned(result))
        } else {
            Some(Cow::Owned(bytes))
        }
    }
}

/// How to format the escape sequence when modifiers are held (modifier_code > 1).
enum ModFormat {
    /// CSI 1;{modifier_code}{byte}  — e.g., \x1b[1;5A for Ctrl+ArrowUp
    CsiSuffix(u8),
    /// CSI {param};{modifier_code}~  — e.g., \x1b[15;5~ for Ctrl+F5
    Tilde(&'static str),
}

/// Descriptor for named keys that have a CSI/SS3 escape-sequence.
struct KeyDesc {
    /// Escape sequence when no modifiers are held and no application mode is active.
    normal: &'static [u8],
    /// Escape sequence when application mode (cursor keys or SS3) is active.
    /// Empty slice for keys that have no application-mode variant.
    app: &'static [u8],
    /// How to produce the sequence when `modifier_code > 1`.
    mod_format: ModFormat,
}

/// Maps a named key to its escape-sequence descriptor, or `None` for keys
/// that should fall through (e.g., media keys, unhandled keys).
fn named_key_desc(key: &NamedKey) -> Option<KeyDesc> {
    use ModFormat::*;
    match key {
        // ── Cursor keys (6) ──
        NamedKey::ArrowUp => Some(KeyDesc {
            normal: b"\x1b[A",
            app: b"\x1bOA",
            mod_format: CsiSuffix(b'A'),
        }),
        NamedKey::ArrowDown => Some(KeyDesc {
            normal: b"\x1b[B",
            app: b"\x1bOB",
            mod_format: CsiSuffix(b'B'),
        }),
        NamedKey::ArrowRight => Some(KeyDesc {
            normal: b"\x1b[C",
            app: b"\x1bOC",
            mod_format: CsiSuffix(b'C'),
        }),
        NamedKey::ArrowLeft => Some(KeyDesc {
            normal: b"\x1b[D",
            app: b"\x1bOD",
            mod_format: CsiSuffix(b'D'),
        }),
        NamedKey::Home => Some(KeyDesc {
            normal: b"\x1b[H",
            app: b"\x1bOH",
            mod_format: CsiSuffix(b'H'),
        }),
        NamedKey::End => Some(KeyDesc {
            normal: b"\x1b[F",
            app: b"\x1bOF",
            mod_format: CsiSuffix(b'F'),
        }),
        // ── F1-F4: SS3 when no mod (4) ──
        NamedKey::F1 => Some(KeyDesc {
            normal: b"\x1bOP",
            app: b"",
            mod_format: CsiSuffix(b'P'),
        }),
        NamedKey::F2 => Some(KeyDesc {
            normal: b"\x1bOQ",
            app: b"",
            mod_format: CsiSuffix(b'Q'),
        }),
        NamedKey::F3 => Some(KeyDesc {
            normal: b"\x1bOR",
            app: b"",
            mod_format: CsiSuffix(b'R'),
        }),
        NamedKey::F4 => Some(KeyDesc {
            normal: b"\x1bOS",
            app: b"",
            mod_format: CsiSuffix(b'S'),
        }),
        // ── Tilde-terminated keys (12) ──
        NamedKey::F5 => Some(KeyDesc {
            normal: b"\x1b[15~",
            app: b"",
            mod_format: Tilde("15"),
        }),
        NamedKey::F6 => Some(KeyDesc {
            normal: b"\x1b[17~",
            app: b"",
            mod_format: Tilde("17"),
        }),
        NamedKey::F7 => Some(KeyDesc {
            normal: b"\x1b[18~",
            app: b"",
            mod_format: Tilde("18"),
        }),
        NamedKey::F8 => Some(KeyDesc {
            normal: b"\x1b[19~",
            app: b"",
            mod_format: Tilde("19"),
        }),
        NamedKey::F9 => Some(KeyDesc {
            normal: b"\x1b[20~",
            app: b"",
            mod_format: Tilde("20"),
        }),
        NamedKey::F10 => Some(KeyDesc {
            normal: b"\x1b[21~",
            app: b"",
            mod_format: Tilde("21"),
        }),
        NamedKey::F11 => Some(KeyDesc {
            normal: b"\x1b[23~",
            app: b"",
            mod_format: Tilde("23"),
        }),
        NamedKey::F12 => Some(KeyDesc {
            normal: b"\x1b[24~",
            app: b"",
            mod_format: Tilde("24"),
        }),
        NamedKey::Insert => Some(KeyDesc {
            normal: b"\x1b[2~",
            app: b"",
            mod_format: Tilde("2"),
        }),
        NamedKey::Delete => Some(KeyDesc {
            normal: b"\x1b[3~",
            app: b"",
            mod_format: Tilde("3"),
        }),
        NamedKey::PageUp => Some(KeyDesc {
            normal: b"\x1b[5~",
            app: b"",
            mod_format: Tilde("5"),
        }),
        NamedKey::PageDown => Some(KeyDesc {
            normal: b"\x1b[6~",
            app: b"",
            mod_format: Tilde("6"),
        }),
        _ => None,
    }
}

/// Renders a `KeyDesc` into its escape-sequence bytes.
/// Zero-allocation for the no-modifier + application-mode paths (Cow::Borrowed).
fn render_csi_key(
    desc: &KeyDesc,
    modifier_code: u8,
    application_cursor: bool,
) -> Cow<'static, [u8]> {
    if modifier_code > 1 {
        match desc.mod_format {
            ModFormat::CsiSuffix(b) => {
                Cow::Owned(format!("\x1b[1;{}{}", modifier_code, b as char).into_bytes())
            }
            ModFormat::Tilde(param) => {
                Cow::Owned(format!("\x1b[{};{}~", param, modifier_code).into_bytes())
            }
        }
    } else if application_cursor && !desc.app.is_empty() {
        Cow::Borrowed(desc.app)
    } else {
        Cow::Borrowed(desc.normal)
    }
}

/// Maps a logical key + optional text + modifier state to the byte sequence
/// to write to the PTY.  Named control/navigation keys are dispatched by
/// `logical_key` first.  When Ctrl is held and the key is a single ASCII
/// letter (a–z / A–Z), the corresponding control character (0x01–0x1A) is
/// emitted regardless of what winit places in `text`.
pub(super) fn keyboard_input_bytes(
    logical_key: &Key,
    text: Option<&str>,
    modifiers: ModifiersState,
    config: KeyboardConfig,
    is_numpad: bool,
) -> Option<Cow<'static, [u8]>> {
    let KeyboardConfig {
        application_cursor,
        application_keypad,
    } = config;

    let modifier_code = modifier_code(modifiers);

    if application_keypad
        && is_numpad
        && modifier_code == 1
        && let Some(seq) = keypad_sequence(logical_key)
    {
        return Some(seq);
    }

    // Ctrl+letter → control character (0x01–0x1A).
    // Prepend ESC (0x1b) if Alt is also pressed.
    if modifiers.control_key()
        && let Key::Character(ch) = logical_key
        && let Some(ctrl_byte) = ctrl_letter_to_byte(ch)
    {
        if modifiers.alt_key() {
            return Some(Cow::Owned(vec![0x1b, ctrl_byte]));
        } else {
            return Some(Cow::Owned(vec![ctrl_byte]));
        }
    }

    // If it's some other character with Ctrl held, fall through —
    // winit may have placed a control character in `text` already.
    match logical_key {
        Key::Named(NamedKey::Enter) => Some(Cow::Borrowed(b"\r")),
        Key::Named(NamedKey::Backspace) => Some(Cow::Borrowed(b"\x7f")),
        Key::Named(NamedKey::Tab) => {
            if modifiers.shift_key() {
                Some(Cow::Borrowed(b"\x1b[Z"))
            } else {
                Some(Cow::Borrowed(b"\t"))
            }
        }
        Key::Named(NamedKey::Escape) => Some(Cow::Borrowed(b"\x1b")),
        Key::Named(NamedKey::Space) => Some(Cow::Borrowed(b" ")),
        Key::Named(name) => {
            named_key_desc(name).map(|d| render_csi_key(&d, modifier_code, application_cursor))
        }
        _ => text_fallback(logical_key, text, modifiers),
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

    fn test_bytes(key: &Key, text: Option<&str>, m: ModifiersState) -> Option<Cow<'static, [u8]>> {
        keyboard_input_bytes(key, text, m, KeyboardConfig::default(), false)
    }

    #[test]
    fn backspace_with_unexpected_text_still_sends_del() {
        assert_eq!(
            test_bytes(&k(NamedKey::Backspace), Some("\x17"), mods()).as_deref(),
            Some(b"\x7f".as_slice())
        );
    }

    #[test]
    fn backspace_with_no_text_sends_del() {
        assert_eq!(
            test_bytes(&k(NamedKey::Backspace), None, mods()).as_deref(),
            Some(b"\x7f".as_slice())
        );
    }

    #[test]
    fn backspace_with_empty_text_sends_del() {
        assert_eq!(
            test_bytes(&k(NamedKey::Backspace), Some(""), mods()).as_deref(),
            Some(b"\x7f".as_slice())
        );
    }

    #[test]
    fn enter_sends_cr() {
        assert_eq!(
            test_bytes(&k(NamedKey::Enter), None, mods()).as_deref(),
            Some(b"\r".as_slice())
        );
    }

    #[test]
    fn escape_sends_esc() {
        assert_eq!(
            test_bytes(&k(NamedKey::Escape), None, mods()).as_deref(),
            Some(b"\x1b".as_slice())
        );
    }

    #[test]
    fn arrow_up() {
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, mods()).as_deref(),
            Some(b"\x1b[A".as_slice())
        );
    }

    #[test]
    fn printable_character() {
        assert_eq!(
            test_bytes(&Key::Character("a".into()), Some("a"), mods()).as_deref(),
            Some(b"a".as_slice())
        );
    }

    #[test]
    fn unrecognized_named_key_no_text_ignored() {
        assert_eq!(
            test_bytes(&k(NamedKey::AudioVolumeMute), None, mods()).as_deref(),
            None
        );
    }

    #[test]
    fn empty_text_ignored() {
        assert_eq!(
            test_bytes(&Key::Character("".into()), Some(""), mods()).as_deref(),
            None
        );
    }

    // ── Ctrl+letter → control character ───────────────────────────────

    #[test]
    fn ctrl_c_sends_etx_with_text() {
        // Ctrl held + 'c' → 0x03, even if winit puts plain "c" in text.
        assert_eq!(
            test_bytes(&Key::Character("c".into()), Some("c"), ctrl()).as_deref(),
            Some(b"\x03".as_slice())
        );
    }

    #[test]
    fn ctrl_c_sends_etx_without_text() {
        assert_eq!(
            test_bytes(&Key::Character("c".into()), None, ctrl()).as_deref(),
            Some(b"\x03".as_slice())
        );
    }

    #[test]
    fn ctrl_d_sends_eot() {
        assert_eq!(
            test_bytes(&Key::Character("d".into()), Some("d"), ctrl()).as_deref(),
            Some(b"\x04".as_slice())
        );
    }

    #[test]
    fn ctrl_shift_c_still_sends_etx() {
        // Ctrl+Shift+C = Ctrl+C → 0x03.
        assert_eq!(
            test_bytes(&Key::Character("C".into()), Some("C"), ctrl()).as_deref(),
            Some(b"\x03".as_slice())
        );
    }

    #[test]
    fn c_without_ctrl_is_plain_text() {
        // 'c' without Ctrl held → normal text.
        assert_eq!(
            test_bytes(&Key::Character("c".into()), Some("c"), mods()).as_deref(),
            Some(b"c".as_slice())
        );
    }

    #[test]
    fn c_without_text_or_ctrl_is_none() {
        assert_eq!(
            test_bytes(&Key::Character("c".into()), None, mods()).as_deref(),
            None
        );
    }

    #[test]
    fn ctrl_non_letter_falls_through_to_text() {
        // Ctrl+1 is not a letter — should use winit's text if provided.
        assert_eq!(
            test_bytes(&Key::Character("1".into()), Some("1"), ctrl()).as_deref(),
            Some(b"1".as_slice())
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
            keyboard_input_bytes(&key_up, None, mods, KeyboardConfig::default(), false).as_deref(),
            Some(b"\x1b[A".as_slice())
        );
        assert_eq!(
            keyboard_input_bytes(&key_1, Some("1"), mods, KeyboardConfig::default(), true)
                .as_deref(),
            Some(b"1".as_slice())
        );
        assert_eq!(
            keyboard_input_bytes(&key_enter, None, mods, KeyboardConfig::default(), true)
                .as_deref(),
            Some(b"\r".as_slice())
        );

        // Application Cursor Keys on
        assert_eq!(
            keyboard_input_bytes(
                &key_up,
                None,
                mods,
                KeyboardConfig {
                    application_cursor: true,
                    application_keypad: false
                },
                false
            )
            .as_deref(),
            Some(b"\x1bOA".as_slice())
        );

        // Application Keypad on, but is_numpad false
        assert_eq!(
            keyboard_input_bytes(
                &key_1,
                Some("1"),
                mods,
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                false
            )
            .as_deref(),
            Some(b"1".as_slice())
        );

        // Application Keypad on and is_numpad true
        assert_eq!(
            keyboard_input_bytes(
                &key_1,
                Some("1"),
                mods,
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                true
            )
            .as_deref(),
            Some(b"\x1bOq".as_slice())
        );
        assert_eq!(
            keyboard_input_bytes(
                &key_enter,
                None,
                mods,
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                true
            )
            .as_deref(),
            Some(b"\x1bOM".as_slice())
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
                    KeyboardConfig {
                        application_cursor: terminal.screen().application_cursor(),
                        application_keypad: terminal.screen().application_keypad(),
                    },
                    false,
                )
                .as_deref(),
                Some(b"\x1bOA".as_slice()),
                "{name} setup did not enable application cursor mode"
            );
            assert_eq!(
                keyboard_input_bytes(
                    &key_1,
                    Some("1"),
                    mods(),
                    KeyboardConfig {
                        application_cursor: terminal.screen().application_cursor(),
                        application_keypad: terminal.screen().application_keypad(),
                    },
                    true,
                )
                .as_deref(),
                Some(b"\x1bOq".as_slice()),
                "{name} setup did not enable application keypad mode"
            );

            terminal.put_bytes(reset);

            assert_eq!(
                keyboard_input_bytes(
                    &key_up,
                    None,
                    mods(),
                    KeyboardConfig {
                        application_cursor: terminal.screen().application_cursor(),
                        application_keypad: terminal.screen().application_keypad(),
                    },
                    false,
                )
                .as_deref(),
                Some(b"\x1b[A".as_slice()),
                "{name} did not reset application cursor mode"
            );
            assert_eq!(
                keyboard_input_bytes(
                    &key_1,
                    Some("1"),
                    mods(),
                    KeyboardConfig {
                        application_cursor: terminal.screen().application_cursor(),
                        application_keypad: terminal.screen().application_keypad(),
                    },
                    true,
                )
                .as_deref(),
                Some(b"1".as_slice()),
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
            test_bytes(&k(NamedKey::ArrowUp), None, mods()).as_deref(),
            Some(b"\x1b[A".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, shift).as_deref(),
            Some(b"\x1b[1;2A".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, alt).as_deref(),
            Some(b"\x1b[1;3A".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, ctrl).as_deref(),
            Some(b"\x1b[1;5A".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::ArrowUp), None, ctrl_shift).as_deref(),
            Some(b"\x1b[1;6A".as_slice())
        );

        // 2. Home / End:
        assert_eq!(
            test_bytes(&k(NamedKey::Home), None, mods()).as_deref(),
            Some(b"\x1b[H".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::Home), None, ctrl).as_deref(),
            Some(b"\x1b[1;5H".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::End), None, ctrl).as_deref(),
            Some(b"\x1b[1;5F".as_slice())
        );

        // 3. Insert / PageUp:
        assert_eq!(
            test_bytes(&k(NamedKey::Insert), None, mods()).as_deref(),
            Some(b"\x1b[2~".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::Insert), None, ctrl).as_deref(),
            Some(b"\x1b[2;5~".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::PageUp), None, ctrl).as_deref(),
            Some(b"\x1b[5;5~".as_slice())
        );

        // 4. F1 (SS3 -> CSI when parameterized):
        assert_eq!(
            test_bytes(&k(NamedKey::F1), None, mods()).as_deref(),
            Some(b"\x1bOP".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::F1), None, ctrl).as_deref(),
            Some(b"\x1b[1;5P".as_slice())
        );

        // 5. F5:
        assert_eq!(
            test_bytes(&k(NamedKey::F5), None, mods()).as_deref(),
            Some(b"\x1b[15~".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::F5), None, ctrl).as_deref(),
            Some(b"\x1b[15;5~".as_slice())
        );

        // 6. Shift+Tab -> \x1b[Z
        assert_eq!(
            test_bytes(&k(NamedKey::Tab), None, shift).as_deref(),
            Some(b"\x1b[Z".as_slice())
        );
        assert_eq!(
            test_bytes(&k(NamedKey::Tab), None, mods()).as_deref(),
            Some(b"\t".as_slice())
        );

        // 7. Alt + printable characters
        assert_eq!(
            test_bytes(&Key::Character("a".into()), Some("a"), alt).as_deref(),
            Some(b"\x1ba".as_slice())
        );
        assert_eq!(
            test_bytes(&Key::Character("a".into()), None, alt).as_deref(),
            Some(b"\x1ba".as_slice())
        );
        assert_eq!(
            test_bytes(&Key::Character("语".into()), Some("语"), alt).as_deref(),
            Some([vec![0x1b], "语".as_bytes().to_vec()].concat()).as_deref()
        );

        // 8. Alt + Ctrl + letter (Alt + Ctrl + C -> \x1b\x03)
        assert_eq!(
            test_bytes(&Key::Character("c".into()), None, alt_ctrl).as_deref(),
            Some(vec![0x1b, 0x03]).as_deref()
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
                KeyboardConfig::default(),
                true
            )
            .as_deref(),
            Some(b"8".as_slice())
        );
        // 1b. application_keypad = true, no modifiers -> application keypad sequence \x1bOx
        assert_eq!(
            keyboard_input_bytes(
                &Key::Character("8".into()),
                Some("8"),
                mods,
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                true
            )
            .as_deref(),
            Some(b"\x1bOx".as_slice())
        );
        // 1c. application_keypad = true, Ctrl modifier -> application keypad is bypassed, sends ctrl fallback
        assert_eq!(
            keyboard_input_bytes(
                &Key::Character("8".into()),
                Some("8"),
                ctrl,
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                true
            )
            .as_deref(),
            Some(b"8".as_slice())
        );

        // 2. NumLock OFF (represented by Key::Named)
        // 2a. application_keypad = false, no modifiers -> standard ArrowUp \x1b[A
        assert_eq!(
            keyboard_input_bytes(
                &k(NamedKey::ArrowUp),
                None,
                mods,
                KeyboardConfig::default(),
                true
            )
            .as_deref(),
            Some(b"\x1b[A".as_slice())
        );
        // 2b. application_keypad = true, no modifiers -> standard ArrowUp \x1b[A
        assert_eq!(
            keyboard_input_bytes(
                &k(NamedKey::ArrowUp),
                None,
                mods,
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                true
            )
            .as_deref(),
            Some(b"\x1b[A".as_slice())
        );
        // 2c. Ctrl modifier -> sends Ctrl+ArrowUp \x1b[1;5A
        assert_eq!(
            keyboard_input_bytes(
                &k(NamedKey::ArrowUp),
                None,
                ctrl,
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                true
            )
            .as_deref(),
            Some(b"\x1b[1;5A".as_slice())
        );
    }

    #[test]
    fn test_keyboard_bytes_ownership() {
        use std::borrow::Cow;

        // ── Borrowed: plain arrow key ──
        assert!(matches!(
            keyboard_input_bytes(
                &k(NamedKey::ArrowUp),
                None,
                mods(),
                KeyboardConfig::default(),
                false
            ),
            Some(Cow::Borrowed(b"\x1b[A"))
        ));
        // ── Borrowed: application cursor mode ArrowUp ──
        assert!(matches!(
            keyboard_input_bytes(
                &k(NamedKey::ArrowUp),
                None,
                mods(),
                KeyboardConfig {
                    application_cursor: true,
                    application_keypad: false
                },
                false
            ),
            Some(Cow::Borrowed(b"\x1bOA"))
        ));
        // ── Borrowed: application keypad mode Enter ──
        assert!(matches!(
            keyboard_input_bytes(
                &k(NamedKey::Enter),
                None,
                mods(),
                KeyboardConfig {
                    application_cursor: false,
                    application_keypad: true
                },
                true
            ),
            Some(Cow::Borrowed(b"\x1bOM"))
        ));

        // ── Owned: modifier formatting path (Ctrl+ArrowUp → \x1b[1;5A) ──
        assert!(matches!(
            keyboard_input_bytes(
                &k(NamedKey::ArrowUp),
                None,
                ModifiersState::CONTROL,
                KeyboardConfig::default(),
                false
            ),
            Some(Cow::Owned(_))
        ));
        // ── Owned: text fallback path (printable character) ──
        assert!(matches!(
            keyboard_input_bytes(
                &Key::Character("a".into()),
                Some("a"),
                ModifiersState::default(),
                KeyboardConfig::default(),
                false
            ),
            Some(Cow::Owned(_))
        ));
        // ── Owned: Ctrl+letter path (Ctrl+C → \x03) ──
        assert!(matches!(
            keyboard_input_bytes(
                &Key::Character("c".into()),
                None,
                ModifiersState::CONTROL,
                KeyboardConfig::default(),
                false
            ),
            Some(Cow::Owned(_))
        ));
    }
}
