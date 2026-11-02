use harbor_terminal::{AltScreenAction, Screen, TerminalParser};

/// Feed the whole slice, applying buffer switches before parsing the remainder.
#[track_caller]
pub fn feed(parser: &mut TerminalParser, screen: &mut Screen, bytes: &[u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        let result = parser.put_bytes(screen, &bytes[offset..]);
        assert!(
            result.consumed > 0 && result.consumed <= bytes.len() - offset,
            "parser consumed {} of {} remaining bytes at offset {offset}",
            result.consumed,
            bytes.len() - offset,
        );
        offset += result.consumed;
        match result.alt_request {
            Some(AltScreenAction::Enter { clear }) => screen.enter_alt(clear),
            Some(AltScreenAction::Exit) => screen.exit_alt(),
            None => {}
        }
    }
}
