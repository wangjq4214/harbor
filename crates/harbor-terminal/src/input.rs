//! Widget keyboard event encoding for the terminal's direct PTY input path.

use harbor_widget::input::event::{Key, KeyboardEvent, Modifiers, UiEvent};

use crate::InputModes;

/// Encodes supported widget keyboard events against the terminal's current modes.
pub(super) struct TerminalInputEncoder;

impl TerminalInputEncoder {
    pub(super) fn encode(event: &UiEvent, modes: InputModes) -> Option<Vec<u8>> {
        match event {
            UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }) => {
                Self::encode_key(*key, *modifiers, modes)
            }
            UiEvent::Keyboard(KeyboardEvent::Ime(text)) if !text.is_empty() => {
                Some(text.as_bytes().to_vec())
            }
            _ => None,
        }
    }

    fn encode_key(key: Key, modifiers: Modifiers, modes: InputModes) -> Option<Vec<u8>> {
        let modifier_code = modifier_code(modifiers);
        let (key, is_numpad) = match key {
            Key::NumpadCharacter(character) => (Key::Character(character), true),
            Key::NumpadEnter => (Key::Enter, true),
            key => (key, false),
        };

        if modes.application_keypad && is_numpad && modifier_code == 1 {
            if let Some(sequence) = keypad_sequence(key) {
                return Some(sequence.to_vec());
            }
        }

        if modifiers.ctrl
            && let Key::Character(character) = key
            && let Some(control) = ctrl_key_to_byte(character)
        {
            return Some(if modifiers.alt {
                vec![0x1b, control]
            } else {
                vec![control]
            });
        }

        match key {
            Key::Tab => Some(if modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }),
            Key::Enter => Some(b"\r".to_vec()),
            Key::Space => Some(b" ".to_vec()),
            Key::Escape => Some(b"\x1b".to_vec()),
            Key::Backspace => Some(b"\x7f".to_vec()),
            Key::Insert => csi_tilde("2", modifier_code),
            Key::Delete => csi_tilde("3", modifier_code),
            Key::F1 => cursor_key(b'P', false, b"\x1bOP", b"\x1bOP", modifier_code),
            Key::F2 => cursor_key(b'Q', false, b"\x1bOQ", b"\x1bOQ", modifier_code),
            Key::F3 => cursor_key(b'R', false, b"\x1bOR", b"\x1bOR", modifier_code),
            Key::F4 => cursor_key(b'S', false, b"\x1bOS", b"\x1bOS", modifier_code),
            Key::F5 => csi_tilde("15", modifier_code),
            Key::F6 => csi_tilde("17", modifier_code),
            Key::F7 => csi_tilde("18", modifier_code),
            Key::F8 => csi_tilde("19", modifier_code),
            Key::F9 => csi_tilde("20", modifier_code),
            Key::F10 => csi_tilde("21", modifier_code),
            Key::F11 => csi_tilde("23", modifier_code),
            Key::F12 => csi_tilde("24", modifier_code),
            Key::PageUp => csi_tilde("5", modifier_code),
            Key::PageDown => csi_tilde("6", modifier_code),
            Key::ArrowUp => cursor_key(
                b'A',
                modes.application_cursor,
                b"\x1b[A",
                b"\x1bOA",
                modifier_code,
            ),
            Key::ArrowDown => cursor_key(
                b'B',
                modes.application_cursor,
                b"\x1b[B",
                b"\x1bOB",
                modifier_code,
            ),
            Key::ArrowRight => cursor_key(
                b'C',
                modes.application_cursor,
                b"\x1b[C",
                b"\x1bOC",
                modifier_code,
            ),
            Key::ArrowLeft => cursor_key(
                b'D',
                modes.application_cursor,
                b"\x1b[D",
                b"\x1bOD",
                modifier_code,
            ),
            Key::Home => cursor_key(
                b'H',
                modes.application_cursor,
                b"\x1b[H",
                b"\x1bOH",
                modifier_code,
            ),
            Key::End => cursor_key(
                b'F',
                modes.application_cursor,
                b"\x1b[F",
                b"\x1bOF",
                modifier_code,
            ),
            Key::Character('\0') => None,
            Key::Character(character) => {
                let mut bytes =
                    Vec::with_capacity(character.len_utf8() + usize::from(modifiers.alt));
                if modifiers.alt {
                    bytes.push(0x1b);
                }
                let mut text = [0; 4];
                bytes.extend_from_slice(character.encode_utf8(&mut text).as_bytes());
                Some(bytes)
            }
            Key::NumpadCharacter(_) | Key::NumpadEnter => {
                unreachable!("numpad keys normalized above")
            }
        }
    }
}

fn modifier_code(modifiers: Modifiers) -> u8 {
    let mut code = 1;
    if modifiers.shift {
        code += 1;
    }
    if modifiers.alt {
        code += 2;
    }
    if modifiers.ctrl {
        code += 4;
    }
    if modifiers.meta {
        code += 8;
    }
    code
}

fn keypad_sequence(key: Key) -> Option<&'static [u8]> {
    match key {
        Key::Character('0') => Some(b"\x1bOp"),
        Key::Character('1') => Some(b"\x1bOq"),
        Key::Character('2') => Some(b"\x1bOr"),
        Key::Character('3') => Some(b"\x1bOs"),
        Key::Character('4') => Some(b"\x1bOt"),
        Key::Character('5') => Some(b"\x1bOu"),
        Key::Character('6') => Some(b"\x1bOv"),
        Key::Character('7') => Some(b"\x1bOw"),
        Key::Character('8') => Some(b"\x1bOx"),
        Key::Character('9') => Some(b"\x1bOy"),
        Key::Character('.') => Some(b"\x1bOn"),
        Key::Character('-') => Some(b"\x1bOm"),
        Key::Character('+') => Some(b"\x1bOk"),
        Key::Character('/') => Some(b"\x1bOo"),
        Key::Character('*') => Some(b"\x1bOj"),
        Key::Character(',') => Some(b"\x1bOl"),
        Key::Character('=') => Some(b"\x1bOX"),
        Key::Enter => Some(b"\x1bOM"),
        _ => None,
    }
}

fn cursor_key(
    suffix: u8,
    application_cursor: bool,
    normal: &'static [u8],
    application: &'static [u8],
    modifier_code: u8,
) -> Option<Vec<u8>> {
    if modifier_code > 1 {
        Some(format!("\x1b[1;{}{}", modifier_code, suffix as char).into_bytes())
    } else if application_cursor {
        Some(application.to_vec())
    } else {
        Some(normal.to_vec())
    }
}

fn csi_tilde(parameter: &str, modifier_code: u8) -> Option<Vec<u8>> {
    if modifier_code > 1 {
        Some(format!("\x1b[{parameter};{modifier_code}~").into_bytes())
    } else {
        Some(format!("\x1b[{parameter}~").into_bytes())
    }
}

fn ctrl_key_to_byte(character: char) -> Option<u8> {
    match character {
        'a'..='z' => Some(character as u8 - b'a' + 1),
        'A'..='Z' => Some(character as u8 - b'A' + 1),
        '\0'..='\x1f' => Some(character as u8),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_down(key: Key, modifiers: Modifiers) -> UiEvent {
        UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers })
    }

    #[test]
    fn direct_encoding_preserves_insert_and_function_key_sequences() {
        let expected: &[(Key, &[u8])] = &[
            (Key::Insert, b"\x1b[2~"),
            (Key::F1, b"\x1bOP"),
            (Key::F2, b"\x1bOQ"),
            (Key::F3, b"\x1bOR"),
            (Key::F4, b"\x1bOS"),
            (Key::F5, b"\x1b[15~"),
            (Key::F6, b"\x1b[17~"),
            (Key::F7, b"\x1b[18~"),
            (Key::F8, b"\x1b[19~"),
            (Key::F9, b"\x1b[20~"),
            (Key::F10, b"\x1b[21~"),
            (Key::F11, b"\x1b[23~"),
            (Key::F12, b"\x1b[24~"),
        ];

        for &(key, sequence) in expected {
            assert_eq!(
                TerminalInputEncoder::encode(
                    &key_down(key, Modifiers::default()),
                    InputModes {
                        application_cursor: true,
                        ..InputModes::default()
                    },
                ),
                Some(sequence.to_vec()),
                "unexpected sequence for {key:?}",
            );
        }
    }

    #[test]
    fn direct_encoding_preserves_modified_insert_and_function_key_sequences() {
        let expected: &[(Key, &[u8])] = &[
            (Key::Insert, b"\x1b[2;5~"),
            (Key::F1, b"\x1b[1;5P"),
            (Key::F2, b"\x1b[1;5Q"),
            (Key::F3, b"\x1b[1;5R"),
            (Key::F4, b"\x1b[1;5S"),
            (Key::F5, b"\x1b[15;5~"),
            (Key::F6, b"\x1b[17;5~"),
            (Key::F7, b"\x1b[18;5~"),
            (Key::F8, b"\x1b[19;5~"),
            (Key::F9, b"\x1b[20;5~"),
            (Key::F10, b"\x1b[21;5~"),
            (Key::F11, b"\x1b[23;5~"),
            (Key::F12, b"\x1b[24;5~"),
        ];
        let modifiers = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };

        for &(key, sequence) in expected {
            assert_eq!(
                TerminalInputEncoder::encode(&key_down(key, modifiers), InputModes::default()),
                Some(sequence.to_vec()),
                "unexpected modified sequence for {key:?}",
            );
        }
    }
}
