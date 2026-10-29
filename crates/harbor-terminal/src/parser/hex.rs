//! Hexadecimal conversion shared by OSC and DCS payload parsers.

/// Decodes one hexadecimal digit, accepting either case.
pub(super) fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Decodes two hexadecimal digits into a byte, accepting either case.
pub(super) fn pair(high: u8, low: u8) -> Option<u8> {
    Some((digit(high)? << 4) | digit(low)?)
}

/// Appends the uppercase hex form of `bytes`, two digits per byte.
pub(super) fn encode_upper(out: &mut Vec<u8>, bytes: &[u8]) {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)]);
        out.push(DIGITS[usize::from(byte & 0x0f)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_both_cases_and_rejects_non_digits() {
        assert_eq!(digit(b'0'), Some(0));
        assert_eq!(digit(b'9'), Some(9));
        assert_eq!(digit(b'a'), Some(10));
        assert_eq!(digit(b'F'), Some(15));
        assert_eq!(digit(b'g'), None);
        assert_eq!(digit(b'/'), None);
    }

    #[test]
    fn decodes_pairs_and_rejects_invalid_nibbles() {
        assert_eq!(pair(b'0', b'0'), Some(0));
        assert_eq!(pair(b'a', b'F'), Some(0xaf));
        assert_eq!(pair(b'F', b'f'), Some(0xff));
        assert_eq!(pair(b'g', b'0'), None);
        assert_eq!(pair(b'0', b'/'), None);
    }

    #[test]
    fn encodes_uppercase_pairs_and_preserves_existing_output() {
        let mut out = b"prefix".to_vec();
        encode_upper(&mut out, b"TN");
        assert_eq!(out, b"prefix544E");
        let mut empty = Vec::new();
        encode_upper(&mut empty, b"");
        assert!(empty.is_empty());
    }
}
