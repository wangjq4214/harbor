//! Bounded in-flight DECRQSS request and canonical status-string serialization.

use crate::screen::Screen;
use harbor_parser::Params;
use harbor_types::{CellAttrs, CharacterProtection, Color, CursorStyleArg};

/// Maximum Pt bytes retained for a single DECRQSS request.
const MAX_PT: usize = 16;
/// Maximum framed reply bytes allowed for one DECRQSS response.
const MAX_REPLY: usize = 128;

/// One active DECRQSS request collected across streaming DCS callbacks.
#[derive(Debug, Default)]
pub(super) struct DecrqssRequest {
    active: bool,
    overflow: bool,
    pt: Vec<u8>,
}

impl DecrqssRequest {
    /// Begin collecting when the DCS introducer is exactly `DCS $ q`.
    pub(super) fn hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
        self.clear();
        self.active =
            params.len() == 1 && params.get(0).is_none() && intermediates == b"$" && action == b'q';
    }

    /// Append a payload byte while under the local Pt bound.
    pub(super) fn put(&mut self, byte: u8) {
        if !self.active {
            return;
        }
        if self.pt.len() < MAX_PT {
            self.pt.push(byte);
        } else {
            self.overflow = true;
        }
    }

    /// Complete the request: cancel silently, or queue one success/failure reply.
    pub(super) fn finish(&mut self, screen: &mut Screen, terminated: bool) {
        if !self.active || !terminated {
            self.clear();
            return;
        }

        let status = if self.overflow {
            None
        } else {
            serialize_status(screen, &self.pt)
        };
        let reply = status
            .map(|status| success_frame(&status))
            .filter(|frame| frame.len() <= MAX_REPLY)
            .unwrap_or_else(failure_frame);
        screen.push_reply(&reply);
        self.clear();
    }

    /// Drop any pending request (e.g. when APC/PM/SOS starts).
    pub(super) fn cancel(&mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        self.active = false;
        self.overflow = false;
        self.pt.clear();
    }
}

fn success_frame(status: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(5 + status.len() + 2);
    frame.extend_from_slice(b"\x1bP1$r");
    frame.extend_from_slice(status);
    frame.extend_from_slice(b"\x1b\\");
    frame
}

fn failure_frame() -> Vec<u8> {
    b"\x1bP0$r\x1b\\".to_vec()
}

fn serialize_status(screen: &Screen, pt: &[u8]) -> Option<Vec<u8>> {
    match pt {
        b"m" => Some(serialize_sgr(screen)),
        b"r" => Some(serialize_decstbm(screen)),
        b"s" => Some(serialize_decslrm(screen)),
        b" q" => Some(serialize_decscusr(screen)),
        b"\"q" => Some(serialize_decsca(screen)),
        _ => None,
    }
}

fn serialize_sgr(screen: &Screen) -> Vec<u8> {
    let (fg, bg, attrs) = screen.current_sgr();
    let mut parts = vec![String::from("0")];

    for &(bit, code) in &[
        (CellAttrs::BOLD, "1"),
        (CellAttrs::DIM, "2"),
        (CellAttrs::ITALIC, "3"),
        (CellAttrs::UNDERLINE, "4"),
        (CellAttrs::BLINK, "5"),
        (CellAttrs::INVERSE, "7"),
        (CellAttrs::STRIKETHROUGH, "9"),
    ] {
        if attrs.contains(bit) {
            parts.push(code.to_string());
        }
    }

    push_sgr_color(&mut parts, true, fg);
    push_sgr_color(&mut parts, false, bg);

    let mut out = parts.join(";");
    out.push('m');
    out.into_bytes()
}

fn push_sgr_color(parts: &mut Vec<String>, is_fg: bool, color: Color) {
    match color {
        Color::Default => {}
        Color::Named(n) => {
            let base = if is_fg { 30 } else { 40 };
            parts.push((base + usize::from(n)).to_string());
        }
        Color::Bright(n) => {
            let base = if is_fg { 90 } else { 100 };
            parts.push((base + usize::from(n)).to_string());
        }
        Color::Indexed(n) => {
            let prefix = if is_fg { "38;5;" } else { "48;5;" };
            parts.push(format!("{prefix}{n}"));
        }
        Color::Rgb(r, g, b) => {
            let prefix = if is_fg { "38;2;" } else { "48;2;" };
            parts.push(format!("{prefix}{r};{g};{b}"));
        }
    }
}

fn serialize_decstbm(screen: &Screen) -> Vec<u8> {
    let (top, bottom) = screen.scroll_region();
    format!("{top};{bottom}r").into_bytes()
}

fn serialize_decslrm(screen: &Screen) -> Vec<u8> {
    let (left, right) = screen.left_right_margins();
    format!("{left};{right}s").into_bytes()
}

fn serialize_decscusr(screen: &Screen) -> Vec<u8> {
    let ps = match screen.cursor_style() {
        CursorStyleArg::BlinkingBlock => 1,
        CursorStyleArg::SteadyBlock => 2,
        CursorStyleArg::BlinkingUnderline => 3,
        CursorStyleArg::SteadyUnderline => 4,
        CursorStyleArg::BlinkingBar => 5,
        CursorStyleArg::SteadyBar => 6,
    };
    format!("{ps} q").into_bytes()
}

fn serialize_decsca(screen: &Screen) -> Vec<u8> {
    let ps = match screen.character_protection() {
        CharacterProtection::Unprotected => 0,
        CharacterProtection::Protected => 1,
    };
    format!("{ps}\"q").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::Screen;

    #[test]
    fn serialize_default_sgr_is_reset_only() {
        let screen = Screen::new(10, 20);
        assert_eq!(serialize_sgr(&screen), b"0m");
    }

    #[test]
    fn serialize_sgr_orders_attrs_then_colors() {
        let mut screen = Screen::new(10, 20);
        screen.set_sgr_slice(&[
            Some(1),
            Some(4),
            Some(7),
            Some(31),
            Some(48),
            Some(5),
            Some(9),
        ]);
        assert_eq!(serialize_sgr(&screen), b"0;1;4;7;31;48;5;9m");
    }

    #[test]
    fn serialize_default_regions_and_styles() {
        let screen = Screen::new(10, 20);
        assert_eq!(serialize_decstbm(&screen), b"1;10r");
        assert_eq!(serialize_decslrm(&screen), b"1;20s");
        assert_eq!(serialize_decscusr(&screen), b"1 q");
        assert_eq!(serialize_decsca(&screen), b"0\"q");
    }

    #[test]
    fn unsupported_pt_is_none() {
        let screen = Screen::new(10, 20);
        assert!(serialize_status(&screen, b"").is_none());
        assert!(serialize_status(&screen, b"x").is_none());
    }

    #[test]
    fn should_serialize_bright_fg_and_bg_when_sgr_uses_bright_colors() {
        // Arrange
        let mut screen = Screen::new(10, 20);
        screen.set_sgr_slice(&[Some(91), Some(104)]);

        // Act
        let status = serialize_sgr(&screen);

        // Assert
        assert_eq!(status, b"0;91;104m");
    }

    #[test]
    fn should_queue_no_reply_when_dcs_hook_is_not_decrqss() {
        // Arrange — wrong final keeps the collector inactive.
        let mut req = DecrqssRequest::default();
        let mut screen = Screen::new(10, 20);
        let params = Params::from(&[None][..]);

        // Act
        req.hook(&params, b"$", b'p');
        req.put(b'm');
        req.finish(&mut screen, true);

        // Assert
        assert_eq!(screen.drain_replies(), Vec::<u8>::new());
    }

    #[test]
    fn should_queue_no_reply_when_finish_sees_unterminated_request() {
        // Arrange
        let mut req = DecrqssRequest::default();
        let mut screen = Screen::new(10, 20);
        let params = Params::from(&[None][..]);
        req.hook(&params, b"$", b'q');
        req.put(b'm');

        // Act — cancelled (CAN/SUB) completion must not emit a DECRQSS frame.
        req.finish(&mut screen, false);

        // Assert
        assert_eq!(screen.drain_replies(), Vec::<u8>::new());
    }

    #[test]
    fn should_queue_failure_reply_when_pt_exceeds_local_bound() {
        // Arrange
        let mut req = DecrqssRequest::default();
        let mut screen = Screen::new(10, 20);
        let params = Params::from(&[None][..]);
        req.hook(&params, b"$", b'q');
        for _ in 0..=MAX_PT {
            req.put(b'm');
        }

        // Act
        req.finish(&mut screen, true);

        // Assert
        assert_eq!(screen.drain_replies(), failure_frame());
    }

    #[test]
    fn should_prefer_failure_when_success_frame_exceeds_reply_cap() {
        // Arrange — status alone long enough that framed success exceeds MAX_REPLY.
        let status = vec![b'x'; MAX_REPLY];

        // Act
        let framed = success_frame(&status);
        let chosen = Some(status)
            .map(|status| success_frame(&status))
            .filter(|frame| frame.len() <= MAX_REPLY)
            .unwrap_or_else(failure_frame);

        // Assert
        assert!(framed.len() > MAX_REPLY);
        assert_eq!(chosen, failure_frame());
    }
}
