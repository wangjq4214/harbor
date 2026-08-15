//! Terminal keyboard event encoding for the direct PTY input path.

use crate::InputModes;
use crate::types::{TerminalEvent, TerminalKey, TerminalKeyboardEvent, TerminalModifiers};

/// Encodes supported terminal keyboard events against the terminal's current modes.
pub(super) struct TerminalInputEncoder;

impl TerminalInputEncoder {
    pub(super) fn encode(event: &TerminalEvent, modes: InputModes) -> Option<Vec<u8>> {
        match event {
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown { key, modifiers }) => {
                Self::encode_key(*key, *modifiers, modes)
            }
            TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(text)) if !text.is_empty() => {
                Some(text.as_bytes().to_vec())
            }
            _ => None,
        }
    }

    fn encode_key(
        key: TerminalKey,
        modifiers: TerminalModifiers,
        modes: InputModes,
    ) -> Option<Vec<u8>> {
        let modifier_code = modifier_code(modifiers);
        let (key, is_numpad) = match key {
            TerminalKey::NumpadCharacter(character) => (TerminalKey::Character(character), true),
            TerminalKey::NumpadEnter => (TerminalKey::Enter, true),
            key => (key, false),
        };

        if modes.application_keypad
            && is_numpad
            && modifier_code == 1
            && let Some(sequence) = keypad_sequence(key)
        {
            return Some(sequence.to_vec());
        }

        if modifiers.ctrl
            && let TerminalKey::Character(character) = key
            && let Some(control) = ctrl_key_to_byte(character)
        {
            return Some(if modifiers.alt {
                vec![0x1b, control]
            } else {
                vec![control]
            });
        }

        match key {
            TerminalKey::Tab => Some(if modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }),
            TerminalKey::Enter => Some(b"\r".to_vec()),
            TerminalKey::Space => Some(b" ".to_vec()),
            TerminalKey::Escape => Some(b"\x1b".to_vec()),
            TerminalKey::Backspace => Some(b"\x7f".to_vec()),
            TerminalKey::Insert => csi_tilde("2", modifier_code),
            TerminalKey::Delete => csi_tilde("3", modifier_code),
            TerminalKey::F1 => cursor_key(b'P', false, b"\x1bOP", b"\x1bOP", modifier_code),
            TerminalKey::F2 => cursor_key(b'Q', false, b"\x1bOQ", b"\x1bOQ", modifier_code),
            TerminalKey::F3 => cursor_key(b'R', false, b"\x1bOR", b"\x1bOR", modifier_code),
            TerminalKey::F4 => cursor_key(b'S', false, b"\x1bOS", b"\x1bOS", modifier_code),
            TerminalKey::F5 => csi_tilde("15", modifier_code),
            TerminalKey::F6 => csi_tilde("17", modifier_code),
            TerminalKey::F7 => csi_tilde("18", modifier_code),
            TerminalKey::F8 => csi_tilde("19", modifier_code),
            TerminalKey::F9 => csi_tilde("20", modifier_code),
            TerminalKey::F10 => csi_tilde("21", modifier_code),
            TerminalKey::F11 => csi_tilde("23", modifier_code),
            TerminalKey::F12 => csi_tilde("24", modifier_code),
            TerminalKey::PageUp => csi_tilde("5", modifier_code),
            TerminalKey::PageDown => csi_tilde("6", modifier_code),
            TerminalKey::ArrowUp => cursor_key(
                b'A',
                modes.application_cursor,
                b"\x1b[A",
                b"\x1bOA",
                modifier_code,
            ),
            TerminalKey::ArrowDown => cursor_key(
                b'B',
                modes.application_cursor,
                b"\x1b[B",
                b"\x1bOB",
                modifier_code,
            ),
            TerminalKey::ArrowRight => cursor_key(
                b'C',
                modes.application_cursor,
                b"\x1b[C",
                b"\x1bOC",
                modifier_code,
            ),
            TerminalKey::ArrowLeft => cursor_key(
                b'D',
                modes.application_cursor,
                b"\x1b[D",
                b"\x1bOD",
                modifier_code,
            ),
            TerminalKey::Home => cursor_key(
                b'H',
                modes.application_cursor,
                b"\x1b[H",
                b"\x1bOH",
                modifier_code,
            ),
            TerminalKey::End => cursor_key(
                b'F',
                modes.application_cursor,
                b"\x1b[F",
                b"\x1bOF",
                modifier_code,
            ),
            TerminalKey::Character('\0') => None,
            TerminalKey::Character(character) => {
                let mut bytes =
                    Vec::with_capacity(character.len_utf8() + usize::from(modifiers.alt));
                if modifiers.alt {
                    bytes.push(0x1b);
                }
                let mut text = [0; 4];
                bytes.extend_from_slice(character.encode_utf8(&mut text).as_bytes());
                Some(bytes)
            }
            TerminalKey::NumpadCharacter(_) | TerminalKey::NumpadEnter => {
                unreachable!("numpad keys normalized above")
            }
        }
    }
}

fn modifier_code(modifiers: TerminalModifiers) -> u8 {
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

fn keypad_sequence(key: TerminalKey) -> Option<&'static [u8]> {
    match key {
        TerminalKey::Character('0') => Some(b"\x1bOp"),
        TerminalKey::Character('1') => Some(b"\x1bOq"),
        TerminalKey::Character('2') => Some(b"\x1bOr"),
        TerminalKey::Character('3') => Some(b"\x1bOs"),
        TerminalKey::Character('4') => Some(b"\x1bOt"),
        TerminalKey::Character('5') => Some(b"\x1bOu"),
        TerminalKey::Character('6') => Some(b"\x1bOv"),
        TerminalKey::Character('7') => Some(b"\x1bOw"),
        TerminalKey::Character('8') => Some(b"\x1bOx"),
        TerminalKey::Character('9') => Some(b"\x1bOy"),
        TerminalKey::Character('.') => Some(b"\x1bOn"),
        TerminalKey::Character('-') => Some(b"\x1bOm"),
        TerminalKey::Character('+') => Some(b"\x1bOk"),
        TerminalKey::Character('/') => Some(b"\x1bOo"),
        TerminalKey::Character('*') => Some(b"\x1bOj"),
        TerminalKey::Character(',') => Some(b"\x1bOl"),
        TerminalKey::Character('=') => Some(b"\x1bOX"),
        TerminalKey::Enter => Some(b"\x1bOM"),
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

    fn key_down(key: TerminalKey, modifiers: TerminalModifiers) -> TerminalEvent {
        TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown { key, modifiers })
    }

    #[test]
    fn direct_encoding_preserves_insert_and_function_key_sequences() {
        let expected: &[(TerminalKey, &[u8])] = &[
            (TerminalKey::Insert, b"\x1b[2~"),
            (TerminalKey::F1, b"\x1bOP"),
            (TerminalKey::F2, b"\x1bOQ"),
            (TerminalKey::F3, b"\x1bOR"),
            (TerminalKey::F4, b"\x1bOS"),
            (TerminalKey::F5, b"\x1b[15~"),
            (TerminalKey::F6, b"\x1b[17~"),
            (TerminalKey::F7, b"\x1b[18~"),
            (TerminalKey::F8, b"\x1b[19~"),
            (TerminalKey::F9, b"\x1b[20~"),
            (TerminalKey::F10, b"\x1b[21~"),
            (TerminalKey::F11, b"\x1b[23~"),
            (TerminalKey::F12, b"\x1b[24~"),
        ];

        for &(key, sequence) in expected {
            assert_eq!(
                TerminalInputEncoder::encode(
                    &key_down(key, TerminalModifiers::default()),
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
        let expected: &[(TerminalKey, &[u8])] = &[
            (TerminalKey::Insert, b"\x1b[2;5~"),
            (TerminalKey::F1, b"\x1b[1;5P"),
            (TerminalKey::F2, b"\x1b[1;5Q"),
            (TerminalKey::F3, b"\x1b[1;5R"),
            (TerminalKey::F4, b"\x1b[1;5S"),
            (TerminalKey::F5, b"\x1b[15;5~"),
            (TerminalKey::F6, b"\x1b[17;5~"),
            (TerminalKey::F7, b"\x1b[18;5~"),
            (TerminalKey::F8, b"\x1b[19;5~"),
            (TerminalKey::F9, b"\x1b[20;5~"),
            (TerminalKey::F10, b"\x1b[21;5~"),
            (TerminalKey::F11, b"\x1b[23;5~"),
            (TerminalKey::F12, b"\x1b[24;5~"),
        ];
        let modifiers = TerminalModifiers {
            ctrl: true,
            ..TerminalModifiers::default()
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
