//! Bounded in-flight XTGETTCAP request and the terminfo capability registry.
//!
//! Queries may list multiple hex-encoded names; unknown or malformed names are
//! skipped and the remaining names are still answered. This deliberately
//! returns more data than xterm, which stops at the first unrecognized name.
//!
//! Numeric capabilities use hex-encoded decimal values, not name-only booleans.

use super::dcs::{Completion, DcsCollector};
use super::hex;
use crate::screen::Screen;
use harbor_parser::Params;

/// Maximum Pt bytes retained for a single XTGETTCAP request.
const MAX_PT: usize = 256;
/// Maximum framed reply bytes allowed for one XTGETTCAP response.
const MAX_REPLY: usize = 256;

/// Value kind of a supported terminfo capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CapabilityValue {
    /// Boolean capability: advertised by name only.
    Bool,
    /// String capability: advertised as `name=value`.
    Str(&'static [u8]),
    /// Numeric capability: advertised as a hex-encoded decimal `name=value`.
    Number(u16),
}

/// Compile-time terminfo capability registry for XTGETTCAP replies.
///
/// `u8` here is the UTF-8 boolean per issue #80. Note the naming collision
/// with ncurses terminfo, where lowercase `u8` is user string #8 (a string
/// capability holding the DA-reply format); do not silently "correct" it.
fn lookup(name: &[u8]) -> Option<CapabilityValue> {
    match name {
        b"TN" => Some(CapabilityValue::Str(b"xterm-256color")),
        b"RGB" => Some(CapabilityValue::Str(b"8/8/8")),
        b"Co" | b"colors" => Some(CapabilityValue::Number(256)),
        // Prefer Unicode line drawing in UTF-8 locales (ncurses user_caps(5)),
        // not a general UTF-8 flag and not an alias of the legacy lowercase u8.
        b"U8" => Some(CapabilityValue::Number(1)),
        b"u8" | b"Su" => Some(CapabilityValue::Bool),
        _ => None,
    }
}

/// One active XTGETTCAP request collected across streaming DCS callbacks.
#[derive(Debug)]
pub(super) struct XtgettcapRequest {
    collector: DcsCollector,
}

impl Default for XtgettcapRequest {
    fn default() -> Self {
        Self {
            collector: DcsCollector::new(MAX_PT),
        }
    }
}

impl XtgettcapRequest {
    /// Begin collecting when the DCS introducer is exactly `DCS + q`.
    pub(super) fn hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
        self.collector.hook(params, intermediates, action, b'+');
    }

    /// Append a payload byte while under the local Pt bound.
    pub(super) fn put(&mut self, byte: u8) {
        self.collector.put(byte);
    }

    /// Complete the request: cancel silently, or queue one success/failure reply.
    pub(super) fn finish(&mut self, screen: &mut Screen, terminated: bool) {
        let reply = match self.collector.complete(terminated) {
            Completion::Silent => return,
            Completion::Overflow => failure_frame(),
            Completion::Payload(pt) => build_reply(&pt),
        };
        screen.push_reply(&reply);
    }

    /// Drop any pending request (e.g. when APC/PM/SOS starts).
    pub(super) fn cancel(&mut self) {
        self.collector.cancel();
    }
}

/// Decodes the semicolon-separated hex names, resolves each against the
/// registry, and frames the reply. An empty or unmatched query yields the
/// empty failure frame.
fn build_reply(pt: &[u8]) -> Vec<u8> {
    const PREFIX: &[u8] = b"\x1bP1+r";
    let mut frame = PREFIX.to_vec();
    for segment in pt.split(|&byte| byte == b';') {
        let Some(name) = hex_decode(segment) else {
            continue;
        };
        let Some(value) = lookup(&name) else {
            continue;
        };
        if frame.len() > PREFIX.len() {
            frame.push(b';');
        }
        hex::encode_upper(&mut frame, &name);
        match value {
            CapabilityValue::Bool => {}
            CapabilityValue::Str(value) => {
                frame.push(b'=');
                hex::encode_upper(&mut frame, value);
            }
            CapabilityValue::Number(value) => {
                frame.push(b'=');
                hex::encode_upper(&mut frame, value.to_string().as_bytes());
            }
        }
        // Later names cannot make a growing reply fit. Drop the whole reply,
        // not an accepted prefix, accounting for the two-byte ST terminator.
        if frame.len() + 2 > MAX_REPLY {
            return failure_frame();
        }
    }
    if frame.len() == PREFIX.len() {
        return failure_frame();
    }
    frame.extend_from_slice(b"\x1b\\");
    frame
}

fn failure_frame() -> Vec<u8> {
    b"\x1bP0+r\x1b\\".to_vec()
}

/// Decodes uppercase or lowercase hex pairs; rejects odd length and
/// non-hex digits.
fn hex_decode(input: &[u8]) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    for pair in input.as_chunks::<2>().0 {
        let hi = hex::digit(pair[0])?;
        let lo = hex::digit(pair[1])?;
        out.push(hi << 4 | lo);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooked() -> XtgettcapRequest {
        let params = Params::from(&[None][..]);
        let mut request = XtgettcapRequest::default();
        request.hook(&params, b"+", b'q');
        request
    }

    fn queued_reply(payload: &[u8]) -> Vec<u8> {
        let mut request = hooked();
        let mut screen = Screen::new(10, 20);
        for &byte in payload {
            request.put(byte);
        }
        request.finish(&mut screen, true);
        screen.drain_replies()
    }

    #[test]
    fn registry_resolves_only_evidence_backed_capabilities() {
        assert_eq!(lookup(b"TN"), Some(CapabilityValue::Str(b"xterm-256color")));
        assert_eq!(lookup(b"RGB"), Some(CapabilityValue::Str(b"8/8/8")));
        assert_eq!(lookup(b"Co"), Some(CapabilityValue::Number(256)));
        assert_eq!(lookup(b"colors"), Some(CapabilityValue::Number(256)));
        assert_eq!(lookup(b"U8"), Some(CapabilityValue::Number(1)));
        assert_eq!(lookup(b"co"), None);
        assert_eq!(lookup(b"COLORS"), None);
        assert_eq!(lookup(b"u8"), Some(CapabilityValue::Bool));
        assert_eq!(lookup(b"Su"), Some(CapabilityValue::Bool));
        assert_eq!(lookup(b"su"), None);
        assert_eq!(lookup(b"xx"), None);
        assert_eq!(lookup(b""), None);
    }

    #[test]
    fn encode_upper_uses_uppercase_digits_and_preserves_prefix() {
        for (bytes, expected) in [
            (b"TN".as_slice(), b"544E".as_slice()),
            (b"RGB", b"524742"),
            (b"8/8/8", b"382F382F38"),
            (b"xterm-256color", b"787465726D2D323536636F6C6F72"),
        ] {
            let mut out = b"prefix".to_vec();
            hex::encode_upper(&mut out, bytes);
            assert_eq!(&out[..6], b"prefix");
            assert_eq!(&out[6..], expected);
        }
    }

    #[test]
    fn hex_decode_round_trips_and_rejects_invalid() {
        assert_eq!(hex_decode(b"544E"), Some(b"TN".to_vec()));
        assert_eq!(hex_decode(b"544"), None, "odd length is rejected");
        assert_eq!(hex_decode(b"Z4"), None, "non-hex digit is rejected");
        assert_eq!(hex_decode(b"4Z"), None, "non-hex digit is rejected");
        assert_eq!(hex_decode(b""), Some(Vec::new()));
    }

    #[test]
    fn hook_activates_only_for_exact_dcs_plus_q() {
        let empty = Params::from(&[None][..]);
        let with_param = Params::from(&[Some(1)][..]);

        let mut request = XtgettcapRequest::default();
        request.hook(&empty, b"+", b'q');
        assert!(request.collector.active);

        request.hook(&empty, b"$", b'q');
        assert!(
            !request.collector.active,
            "DECRQSS introducer must not activate"
        );
        request.hook(&empty, b"+", b'p');
        assert!(
            !request.collector.active,
            "wrong final byte must not activate"
        );
        request.hook(&with_param, b"+", b'q');
        assert!(
            !request.collector.active,
            "parameterized DCS must not activate"
        );
    }

    #[test]
    fn put_beyond_max_pt_sets_overflow_and_stops_retaining() {
        let mut request = hooked();
        for _ in 0..=MAX_PT {
            request.put(b'x');
        }
        assert!(request.collector.overflow);
        assert_eq!(request.collector.payload.len(), MAX_PT);
    }

    #[test]
    fn finish_queues_success_frame_for_string_capabilities() {
        assert_eq!(queued_reply(b"524742"), b"\x1bP1+r524742=382F382F38\x1b\\");
        assert_eq!(
            queued_reply(b"544E"),
            b"\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\"
        );
    }

    #[test]
    fn finish_queues_boolean_capability_by_name_only() {
        assert_eq!(queued_reply(b"7538"), b"\x1bP1+r7538\x1b\\");
        assert_eq!(queued_reply(b"5375"), b"\x1bP1+r5375\x1b\\");
    }

    #[test]
    fn finish_keeps_query_order_and_skips_unknown_names() {
        assert_eq!(
            queued_reply(b"524742;7878;544E"),
            b"\x1bP1+r524742=382F382F38;544E=787465726D2D323536636F6C6F72\x1b\\"
        );
    }

    #[test]
    fn finish_returns_empty_failure_for_empty_or_unmatched_queries() {
        assert_eq!(queued_reply(b""), b"\x1bP0+r\x1b\\");
        assert_eq!(queued_reply(b"7878"), b"\x1bP0+r\x1b\\");
    }

    #[test]
    fn finish_is_silent_when_unterminated_or_inactive() {
        let mut request = hooked();
        let mut screen = Screen::new(10, 20);
        request.put(b'5');
        request.finish(&mut screen, false);
        assert_eq!(screen.drain_replies(), Vec::<u8>::new());

        request.finish(&mut screen, true);
        assert_eq!(
            screen.drain_replies(),
            Vec::<u8>::new(),
            "cleared request stays silent"
        );
    }

    #[test]
    fn finish_returns_failure_when_payload_overflowed() {
        let mut request = hooked();
        let mut screen = Screen::new(10, 20);
        for _ in 0..=MAX_PT {
            request.put(b'5');
        }
        request.finish(&mut screen, true);
        assert_eq!(screen.drain_replies(), b"\x1bP0+r\x1b\\");
    }

    #[test]
    fn reply_falls_back_when_supported_entries_exceed_reply_cap() {
        let payload = [b"544E".as_slice(); 8].join(&b';');
        assert!(payload.len() <= MAX_PT);
        assert_eq!(queued_reply(&payload), failure_frame());
    }

    #[test]
    fn should_restart_collection_from_scratch_when_hook_fires_again() {
        // Arrange — a first request already retained a payload.
        let params = Params::from(&[None][..]);
        let mut request = hooked();
        request.put(b'5');
        request.put(b'4');
        request.put(b'4');
        request.put(b'E');

        // Act — a fresh `DCS + q` introducer hooks again.
        request.hook(&params, b"+", b'q');
        request.put(b'5');
        request.put(b'2');
        request.put(b'4');
        request.put(b'7');
        request.put(b'4');
        request.put(b'2');

        // Assert — the old payload must not leak into the new reply.
        let mut screen = Screen::new(10, 20);
        request.finish(&mut screen, true);
        assert_eq!(screen.drain_replies(), b"\x1bP1+r524742=382F382F38\x1b\\");
    }

    #[test]
    fn should_ignore_payload_bytes_when_no_request_is_active() {
        // Arrange — a default collector that never saw a DCS hook.
        let mut request = XtgettcapRequest::default();

        // Act — stray payload bytes, then a terminated finish.
        request.put(b'5');
        request.put(b'2');
        let mut screen = Screen::new(10, 20);
        request.finish(&mut screen, true);

        // Assert — nothing was retained and no reply is queued.
        assert_eq!(screen.drain_replies(), Vec::<u8>::new());
        assert!(!request.collector.active);
    }

    #[test]
    fn should_retain_exactly_max_pt_bytes_before_overflow() {
        // Arrange — one active request.
        let mut request = hooked();

        // Act — put exactly MAX_PT bytes, then one more.
        for _ in 0..MAX_PT {
            request.put(b'x');
        }
        let at_bound_overflow = request.collector.overflow;
        let retained_at_bound = request.collector.payload.len();
        request.put(b'x');

        // Assert — the bound is inclusive up to MAX_PT, then retention caps.
        assert!(!at_bound_overflow, "exactly MAX_PT bytes must not overflow");
        assert_eq!(retained_at_bound, MAX_PT);
        assert!(request.collector.overflow);
        assert_eq!(
            request.collector.payload.len(),
            MAX_PT,
            "retained bytes stay capped"
        );
    }

    #[test]
    fn should_queue_no_reply_when_cancelled_before_finish() {
        // Arrange — an active request with retained payload.
        let mut request = hooked();
        request.put(b'5');

        // Act — cancel (as when APC/PM/SOS starts), then stray bytes.
        request.cancel();
        request.put(b'2');

        // Assert — a later terminated finish stays silent.
        let mut screen = Screen::new(10, 20);
        request.finish(&mut screen, true);
        assert_eq!(screen.drain_replies(), Vec::<u8>::new());
    }

    #[test]
    fn should_accept_frame_at_exact_reply_cap() {
        // 50 four-byte boolean entries plus separators and framing = 256 bytes.
        let at_cap = [b"7538".as_slice(); 50].join(&b';');
        let mut one_past = at_cap.clone();
        one_past.push(b';'); // malformed/empty trailing names add no reply bytes
        assert!(one_past.len() <= MAX_PT);
        let accepted = queued_reply(&one_past);
        assert_eq!(accepted.len(), MAX_REPLY);
        assert!(accepted.starts_with(b"\x1bP1+r"));
        assert!(accepted.ends_with(b"\x1b\\"));

        // 33 booleans, one RGB and two TN entries frame to exactly 257 bytes.
        let mut over_cap = vec![b"7538".as_slice(); 33];
        over_cap.extend([b"524742".as_slice(), b"544E", b"544E"]);
        let payload = over_cap.join(&b';');
        assert!(payload.len() <= MAX_PT);
        assert_eq!(queued_reply(&payload), failure_frame());
    }

    proptest::proptest! {
        #[test]
        fn replies_match_ordered_entries_with_duplicates_and_malformed_names(
            choices in proptest::collection::vec(0usize..13, 0..=16)
        ) {
            let names: [&[u8]; 13] = [
                b"544E", b"544e", b"524742", b"7538", b"5375",
                b"436F", b"636F6C6F7273", b"5538", b"5538",
                b"544", b"Z4", b"", b"7878",
            ];
            let entries: [&[u8]; 9] = [
                b"544E=787465726D2D323536636F6C6F72",
                b"544E=787465726D2D323536636F6C6F72",
                b"524742=382F382F38", b"7538", b"5375",
                b"436F=323536", b"636F6C6F7273=323536", b"5538=31", b"5538=31",
            ];
            let payload = choices.iter().map(|&i| names[i]).collect::<Vec<_>>().join(&b';');
            let found = choices.iter().filter_map(|&i| entries.get(i).copied()).collect::<Vec<_>>();
            let mut expected = b"\x1bP1+r".to_vec();
            expected.extend(found.join(&b';'));
            expected.extend_from_slice(b"\x1b\\");
            if found.is_empty() || expected.len() > MAX_REPLY {
                expected = b"\x1bP0+r\x1b\\".to_vec();
            }
            proptest::prop_assert!(payload.len() <= MAX_PT);
            proptest::prop_assert_eq!(queued_reply(&payload), expected);
        }
    }
    #[test]
    fn hex_decode_accepts_lowercase_digits() {
        assert_eq!(hex_decode(b"544e"), Some(b"TN".to_vec()));
        assert_eq!(hex_decode(b"382f382f38"), Some(b"8/8/8".to_vec()));
    }
}
