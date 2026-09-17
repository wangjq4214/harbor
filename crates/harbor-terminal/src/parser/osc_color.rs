use crate::screen::DefaultColorSlot;
use harbor_config::Rgba;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Action {
    Set(DefaultColorSlot, [u8; 3]),
    Query(DefaultColorSlot),
    Reset(DefaultColorSlot),
}

pub(super) fn parse(command: &[u8], payload: &[u8]) -> Option<Action> {
    let slot = match command {
        b"10" | b"110" => DefaultColorSlot::Foreground,
        b"11" | b"111" => DefaultColorSlot::Background,
        b"12" | b"112" => DefaultColorSlot::Cursor,
        _ => return None,
    };

    if command.len() == 3 {
        return payload.is_empty().then_some(Action::Reset(slot));
    }
    if payload == b"?" {
        return Some(Action::Query(slot));
    }

    parse_color(payload).map(|rgb| Action::Set(slot, rgb))
}

fn parse_color(payload: &[u8]) -> Option<[u8; 3]> {
    if let Some(hex) = payload.strip_prefix(b"#") {
        if hex.len() != 6 {
            return None;
        }
        return Some([
            parse_byte(&hex[0..2])?,
            parse_byte(&hex[2..4])?,
            parse_byte(&hex[4..6])?,
        ]);
    }

    let components = payload.strip_prefix(b"rgb:")?;
    let mut parts = components.split(|byte| *byte == b'/');
    let rgb = [
        parse_component(parts.next()?)?,
        parse_component(parts.next()?)?,
        parse_component(parts.next()?)?,
    ];
    parts.next().is_none().then_some(rgb)
}

fn parse_byte(hex: &[u8]) -> Option<u8> {
    let high = hex_value(*hex.first()?)?;
    let low = hex_value(*hex.get(1)?)?;
    Some((high << 4) | low)
}

fn parse_component(hex: &[u8]) -> Option<u8> {
    if !(1..=4).contains(&hex.len()) {
        return None;
    }
    let mut value = 0_u32;
    for &digit in hex {
        value = (value << 4) | u32::from(hex_value(digit)?);
    }
    let max = (1_u32 << (4 * hex.len())) - 1;
    Some(((value * 255 + max / 2) / max) as u8)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) struct Reply {
    bytes: [u8; 26],
    len: usize,
}

impl Reply {
    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

pub(super) fn format_query(slot: DefaultColorSlot, color: Rgba, bell_terminated: bool) -> Reply {
    let command: &[u8] = match slot {
        DefaultColorSlot::Foreground => b"10",
        DefaultColorSlot::Background => b"11",
        DefaultColorSlot::Cursor => b"12",
    };
    let [red, green, blue, _] = color.components();
    let rgb = [quantize(red), quantize(green), quantize(blue)];
    let mut reply = Reply {
        bytes: [0; 26],
        len: 0,
    };
    reply.extend(b"\x1b]");
    reply.extend(command);
    reply.extend(b";rgb:");
    for (index, component) in rgb.into_iter().enumerate() {
        if index != 0 {
            reply.push(b'/');
        }
        let high = component >> 4;
        let low = component & 0x0f;
        reply.push(hex_digit(high));
        reply.push(hex_digit(low));
        reply.push(hex_digit(high));
        reply.push(hex_digit(low));
    }
    if bell_terminated {
        reply.push(0x07);
    } else {
        reply.extend(b"\x1b\\");
    }
    reply
}

impl Reply {
    fn push(&mut self, byte: u8) {
        self.bytes[self.len] = byte;
        self.len += 1;
    }

    fn extend(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push(byte);
        }
    }
}

fn quantize(component: f32) -> u8 {
    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}

const fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'a' + value - 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slots_queries_resets_and_supported_set_forms() {
        assert_eq!(
            parse(b"10", b"#12aBef"),
            Some(Action::Set(
                DefaultColorSlot::Foreground,
                [0x12, 0xab, 0xef]
            ))
        );
        assert_eq!(
            parse(b"11", b"rgb:f/80/8000"),
            Some(Action::Set(DefaultColorSlot::Background, [255, 128, 128]))
        );
        assert_eq!(
            parse(b"12", b"?"),
            Some(Action::Query(DefaultColorSlot::Cursor))
        );
        assert_eq!(
            parse(b"111", b""),
            Some(Action::Reset(DefaultColorSlot::Background))
        );
    }

    #[test]
    fn scales_each_component_width_with_rounding() {
        for (component, expected) in [
            (b"0".as_slice(), 0),
            (b"f", 255),
            (b"8", 136),
            (b"80", 128),
            (b"800", 128),
            (b"8000", 128),
            (b"ffff", 255),
        ] {
            assert_eq!(parse_component(component), Some(expected));
        }
    }

    #[test]
    fn rejects_unsupported_or_malformed_payloads() {
        for payload in [
            b"".as_slice(),
            b"red",
            b"rgbi:1/0/0",
            b"#12345678",
            b"#12345g",
            b"rgb:/0/0",
            b"rgb:0/0",
            b"rgb:0/0/0/0",
            b"rgb:00000/0/0",
            b"rgb:0/0/g",
            b"#123456;#abcdef",
        ] {
            assert_eq!(parse(b"10", payload), None, "payload {payload:?}");
        }
        assert_eq!(parse(b"110", b"?"), None);
        assert_eq!(parse(b"110", b"#123456"), None);
        assert_eq!(parse(b"13", b"?"), None);
    }

    #[test]
    fn formats_fixed_lowercase_16_bit_queries_with_matching_terminator() {
        let color = Rgba::from_rgba8(0x12, 0xab, 0xff, 0x40);
        assert_eq!(
            format_query(DefaultColorSlot::Foreground, color, true).as_bytes(),
            b"\x1b]10;rgb:1212/abab/ffff\x07"
        );
        assert_eq!(
            format_query(DefaultColorSlot::Cursor, color, false).as_bytes(),
            b"\x1b]12;rgb:1212/abab/ffff\x1b\\"
        );
    }
}
