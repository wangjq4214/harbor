//! Whole-request validation for the bounded OSC 4/104 indexed-color subset.
use super::osc_color::parse_color;

#[derive(Debug, PartialEq)]
pub(super) enum Entry {
    Set(u8, [u8; 3]),
    Query(u8),
    Reset(u8),
}

#[derive(Debug, PartialEq)]
pub(super) enum Action {
    Entries(Vec<Entry>),
    ResetAll,
}

pub(super) fn parse(command: &[u8], payload: &[u8]) -> Option<Action> {
    // Defense in depth for handler-owned allocations. The ordinary OSC framing
    // bound already includes the command; no other OSC (notably 52) is widened.
    if payload.len() > 4096 {
        return None;
    }
    if command == b"104" && payload.is_empty() {
        return Some(Action::ResetAll);
    }
    let mut parts = payload.split(|byte| *byte == b';');
    let mut entries = Vec::new();
    // Each entry consumes at least one payload byte (4 needs an index and color).
    // Thus even a malformed tail cannot cause unbounded temporary storage.
    while let Some(part) = parts.next() {
        let index = parse_index(part)?;
        entries.push(match command {
            b"4" => match parts.next()? {
                b"?" => Entry::Query(index),
                color => Entry::Set(index, parse_color(color)?),
            },
            b"104" => Entry::Reset(index),
            _ => return None,
        });
    }
    Some(Action::Entries(entries))
}

fn parse_index(bytes: &[u8]) -> Option<u8> {
    if bytes.is_empty() {
        return None;
    }
    bytes.iter().try_fold(0_u8, |value, &byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        value.checked_mul(10)?.checked_add(byte - b'0')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_actions_are_bounded_and_validate_before_delivery() {
        assert_eq!(
            parse(b"4", b"42;#123456;42;?;255;rgb:f/80/8000"),
            Some(Action::Entries(vec![
                Entry::Set(42, [0x12, 0x34, 0x56]),
                Entry::Query(42),
                Entry::Set(255, [255, 128, 128]),
            ]))
        );
        assert_eq!(parse(b"104", b""), Some(Action::ResetAll));
        assert_eq!(
            parse(b"104", b"1;42;42"),
            Some(Action::Entries(vec![
                Entry::Reset(1),
                Entry::Reset(42),
                Entry::Reset(42),
            ]))
        );
        for command in [b"4".as_slice(), b"104"] {
            assert_eq!(parse(command, &vec![b'0'; 4097]), None);
        }
        let mut maximal = b"0;".repeat(2047);
        maximal.push(b'0');
        let Some(Action::Entries(entries)) = parse(b"104", &maximal) else {
            panic!("bounded maximal reset list");
        };
        assert_eq!(entries.len(), 2048);
        assert!(parse(b"4", b"42;#123456;42;?;256;#abcdef").is_none());
        assert!(parse(b"104", b"1;256").is_none());
    }
}
