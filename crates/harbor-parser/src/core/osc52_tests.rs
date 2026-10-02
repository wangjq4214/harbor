use super::*;
use crate::Params;

#[derive(Default)]
struct Sink {
    osc: Vec<(Vec<u8>, usize, bool)>,
    text: String,
    string_bytes: usize,
}
impl VtHandler for Sink {
    fn print(&mut self, ch: char) {
        self.text.push(ch);
    }
    fn execute(&mut self, _: u8) {}
    fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: u8, _: Option<u8>) {}
    fn esc_dispatch(&mut self, _: &[u8], _: u8) {}
    fn osc_dispatch(&mut self, command: &[u8], payload: &[u8], bell: bool) {
        self.osc.push((command.to_vec(), payload.len(), bell));
    }
    fn dcs_hook(&mut self, _: &Params, _: &[u8], _: u8) {}
    fn dcs_put(&mut self, _: u8) {
        self.string_bytes += 1;
    }
    fn dcs_unhook(&mut self, _: bool) {}
    fn start_string(&mut self, _: u8) {}
}
fn feed(parser: &mut Parser, sink: &mut Sink, bytes: &[u8]) {
    for &byte in bytes {
        parser.advance(sink, byte);
    }
}

#[test]
fn exact_clipboard_prefixes_receive_only_the_fixed_data_budget() {
    const CAP: usize = 5_592_408;
    for prefix in [b"52;c;".as_slice(), b"52;;"] {
        for terminator in [b"\x07".as_slice(), b"\x1b\\"] {
            let mut parser = Parser::default();
            let mut sink = Sink::default();
            feed(&mut parser, &mut sink, b"\x1b]");
            feed(&mut parser, &mut sink, prefix);
            for _ in 0..CAP {
                parser.advance(&mut sink, b'A');
            }
            assert_eq!(parser.osc.len(), CAP + prefix.len());
            assert!(parser.osc.capacity() <= CAP + prefix.len());
            assert!(parser.retained_state_within_limits());
            feed(&mut parser, &mut sink, terminator);
            assert_eq!(
                sink.osc,
                vec![(
                    b"52".to_vec(),
                    CAP + prefix.len() - 3,
                    terminator == b"\x07"
                )]
            );
            // The reused buffer must still apply a fresh per-sequence bound.
            feed(&mut parser, &mut sink, b"\x1b]");
            feed(&mut parser, &mut sink, prefix);
            for _ in 0..CAP + 100 {
                parser.advance(&mut sink, b'A');
            }
            assert_eq!(parser.osc.len(), CAP + prefix.len());
            assert!(parser.osc_overflow);
            feed(&mut parser, &mut sink, terminator);
            feed(&mut parser, &mut sink, b"OK\x1b]0;next\x07");
            assert_eq!(sink.osc.len(), 2);
            assert_eq!(sink.text, "OK");
        }
    }
}

#[test]
fn unsupported_framing_and_other_string_families_keep_4096_limit() {
    for prefix in [
        b"52;p;".as_slice(),
        b"052;c;",
        b"52;cc;",
        b"52;c",
        b"52; ;",
        b"0;",
        b"7;",
        b"8;",
        b"999;",
    ] {
        let mut parser = Parser::default();
        let mut sink = Sink::default();
        feed(&mut parser, &mut sink, b"\x1b]");
        feed(&mut parser, &mut sink, prefix);
        for _ in 0..5000 {
            parser.advance(&mut sink, b'A');
        }
        assert_eq!(parser.osc.len(), 4096);
        assert!(parser.osc_overflow);
        feed(&mut parser, &mut sink, b"\x07OK");
        assert!(sink.osc.is_empty());
        assert_eq!(sink.text, "OK");
    }
    for prefix in [b"\x1bPq".as_slice(), b"\x1bX", b"\x1b^", b"\x1b_"] {
        let mut parser = Parser::default();
        let mut sink = Sink::default();
        feed(&mut parser, &mut sink, prefix);
        for _ in 0..5000 {
            parser.advance(&mut sink, b'A');
        }
        assert_eq!(sink.string_bytes, 4096);
        assert!(parser.retained_state_within_limits());
        feed(&mut parser, &mut sink, b"\x1b\\OK");
        assert_eq!(sink.text, "OK");
    }
}

#[test]
fn cancellation_and_debug_do_not_deliver_or_disclose_retained_clipboard() {
    for cancel in [0x18, 0x1a] {
        let mut parser = Parser::default();
        let mut sink = Sink::default();
        feed(&mut parser, &mut sink, b"\x1b]52;c;synthetic-secret");
        assert!(!format!("{parser:?}").contains("synthetic-secret"));
        parser.advance(&mut sink, cancel);
        assert!(parser.osc.is_empty());
        feed(&mut parser, &mut sink, b"OK\x1b]0;next\x07");
        assert_eq!(sink.osc, vec![(b"0".to_vec(), 4, true)]);
        assert_eq!(sink.text, "OK");
    }
}
