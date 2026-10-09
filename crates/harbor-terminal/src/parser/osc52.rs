//! Strict, allocation-bounded OSC 52 write subset. No clipboard I/O or read replies.
use crate::ClipboardWrite;

const MAX_ENCODED_BYTES: usize = 5_592_408;

pub(super) fn parse(payload: &[u8]) -> Option<ClipboardWrite> {
    let data = payload
        .strip_prefix(b"c;")
        .or_else(|| payload.strip_prefix(b";"))?;
    if data.len() > MAX_ENCODED_BYTES {
        return None;
    }
    let symbols = data
        .iter()
        .rposition(|&byte| byte != b'=')
        .map_or(0, |i| i + 1);
    let padding = data.len() - symbols;
    let remainder = symbols % 4;
    if remainder == 1
        || padding > 2
        || (padding != 0 && (data.len() % 4 != 0 || padding != 4 - remainder))
    {
        return None;
    }
    // The encoded cap admits a few decoded bytes beyond 4 MiB. Check independently.
    let decoded_len = symbols / 4 * 3 + remainder.saturating_sub(1);
    if decoded_len > ClipboardWrite::MAX_BYTES {
        return None;
    }
    let data = &data[..symbols];
    if (remainder == 2 && sextet(*data.last()?)? & 15 != 0)
        || (remainder == 3 && sextet(*data.last()?)? & 3 != 0)
    {
        return None;
    }

    // Validate the base64 alphabet, UTF-8 and NUL without allocating decoded bytes.
    // Bounds on each first continuation exclude overlongs, surrogates and > U+10FFFF.
    let (mut remaining, mut low, mut high) = (0, 0x80, 0xbf);
    visit_decoded(data, |byte| {
        if remaining != 0 {
            if !(low..=high).contains(&byte) {
                return None;
            }
            remaining -= 1;
            low = 0x80;
            high = 0xbf;
        } else {
            match byte {
                0 => return None,
                1..=0x7f => {}
                0xc2..=0xdf => remaining = 1,
                0xe0 => {
                    remaining = 2;
                    low = 0xa0;
                }
                0xe1..=0xec | 0xee..=0xef => remaining = 2,
                0xed => {
                    remaining = 2;
                    high = 0x9f;
                }
                0xf0 => {
                    remaining = 3;
                    low = 0x90;
                }
                0xf1..=0xf3 => remaining = 3,
                0xf4 => {
                    remaining = 3;
                    high = 0x8f;
                }
                _ => return None,
            }
        }
        Some(())
    })?;
    if remaining != 0 {
        return None;
    }

    let mut decoded = Vec::with_capacity(decoded_len);
    visit_decoded(data, |byte| {
        decoded.push(byte);
        Some(())
    })?;
    ClipboardWrite::new(String::from_utf8(decoded).ok()?)
}

fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn visit_decoded(data: &[u8], mut visit: impl FnMut(u8) -> Option<()>) -> Option<()> {
    for chunk in data.chunks(4) {
        let a = sextet(chunk[0])?;
        let b = sextet(chunk[1])?;
        visit((a << 2) | (b >> 4))?;
        if let Some(&c) = chunk.get(2) {
            let c = sextet(c)?;
            visit((b << 4) | (c >> 2))?;
            if let Some(&d) = chunk.get(3) {
                visit((c << 6) | sextet(d)?)?;
            }
        }
    }
    Some(())
}
