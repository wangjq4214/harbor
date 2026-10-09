use super::{TerminalParser, osc52};
use crate::{ClipboardDelivery, ClipboardWrite, Screen, TerminalOutputEvent};

fn encode(bytes: &[u8]) -> Vec<u8> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(a >> 2) as usize]);
        out.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize]);
        out.push(if chunk.len() >= 2 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize]
        } else {
            b'='
        });
        out.push(if chunk.len() == 3 {
            TABLE[(c & 63) as usize]
        } else {
            b'='
        });
    }
    out
}
fn frame(text: &[u8]) -> Vec<u8> {
    let mut stream = b"\x1b]52;c;".to_vec();
    stream.extend(encode(text));
    stream.push(7);
    stream
}
fn event(text: &str) -> TerminalOutputEvent {
    TerminalOutputEvent::ClipboardWrite(ClipboardWrite::new(text.into()).unwrap())
}
fn feed(parser: &mut TerminalParser, screen: &mut Screen, bytes: &[u8]) {
    assert_eq!(parser.put_bytes(screen, bytes).consumed, bytes.len());
}

proptest::proptest! {
    #[test]
    fn decoder_utf8_validation_matches_standard_library(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..128)) {
        let mut payload = b"c;".to_vec();
        payload.extend(encode(&bytes));
        let expected = std::str::from_utf8(&bytes).ok().filter(|_| !bytes.contains(&0));
        let actual = osc52::parse(&payload);
        proptest::prop_assert_eq!(actual.as_ref().map(ClipboardWrite::as_str), expected);
    }

    #[test]
    fn arbitrary_encoded_candidates_never_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..128)) {
        let mut payload = b"c;".to_vec();
        payload.extend(bytes);
        if let Some(write) = osc52::parse(&payload) {
            proptest::prop_assert!(!write.as_str().as_bytes().contains(&0));
            proptest::prop_assert!(write.as_str().len() <= ClipboardWrite::MAX_BYTES);
        }
    }
}

#[test]
fn strict_base64_accepts_padding_omission_clear_and_utf8_without_normalization() {
    for text in ["", "f", "fo", "foo", "\tline\r\n界🦀é", "\u{1f}\u{7f}"] {
        let encoded = encode(text.as_bytes());
        for data in [
            encoded.as_slice(),
            encoded
                .trim_ascii_end()
                .split(|&b| b == b'=')
                .next()
                .unwrap(),
        ] {
            for prefix in [b"c;".as_slice(), b";"] {
                let mut payload = prefix.to_vec();
                payload.extend_from_slice(data);
                assert_eq!(osc52::parse(&payload).unwrap().into_text(), text);
            }
        }
    }
}

#[test]
fn malformed_encoding_selection_binary_and_queries_are_denied_without_replies() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(2, 20);
    for payload in [
        b"c;?".as_slice(),
        b";?",
        b"p;Zg==",
        b"pc;Zg==",
        b"c0;Zg==",
        b"0;Zg==",
        b"C;Zg==",
        b"c",
        b"c;A",
        b"c;Zg=",
        b"c;Zg===",
        b"c;Zg====",
        b"c;=Zg=",
        b"c;Zm=8",
        b"c;Zm9v=",
        b"c;====",
        b"c;=",
        b"c;Zh==",
        b"c;Zh",
        b"c;Zm9=",
        b"c;Zm9",
        b"c;Z g==",
        b"c;Zg==\n",
        b"c;_w==",
        b"c;-w==",
        b"c;AA==",
        b"c;/w==",
        b"c;wIA=",
        b"c;7aCA",
        b"c;9JCAgA==",
        b"c;4oI=",
        b"c;gA==",
        b"c;Zg==;",
        b"c;\xff",
        b" ;Zg==",
    ] {
        assert!(osc52::parse(payload).is_none());
        feed(&mut parser, &mut screen, b"\x1b]52;");
        feed(&mut parser, &mut screen, payload);
        feed(&mut parser, &mut screen, b"\x07");
    }
    feed(&mut parser, &mut screen, b"OK");
    assert!(parser.drain_output_events().is_empty());
    assert!(screen.drain_replies().is_empty());
    assert!(screen.row_text(0).starts_with("OK"));
}

#[test]
fn every_two_part_fragmentation_and_single_byte_bel_st_preserve_exact_text() {
    let text = "界🦀\n\tfixture";
    for prefix in [b"\x1b]52;c;".as_slice(), b"\x1b]52;;"] {
        for terminator in [b"\x07".as_slice(), b"\x1b\\"] {
            let mut stream = prefix.to_vec();
            stream.extend(encode(text.as_bytes()));
            stream.extend_from_slice(terminator);
            stream.extend_from_slice(b"OK");
            for split in 0..=stream.len() {
                let mut parser = TerminalParser::default();
                let mut screen = Screen::new(2, 20);
                feed(&mut parser, &mut screen, &stream[..split]);
                feed(&mut parser, &mut screen, &stream[split..]);
                assert_eq!(parser.drain_output_events(), vec![event(text)]);
                assert!(screen.drain_replies().is_empty());
                assert!(screen.row_text(0).starts_with("OK"));
            }
            let mut parser = TerminalParser::default();
            let mut screen = Screen::new(2, 20);
            for byte in &stream {
                feed(&mut parser, &mut screen, &[*byte]);
            }
            assert_eq!(parser.drain_output_events(), vec![event(text)]);
        }
    }
}

#[test]
fn incomplete_cancelled_and_reset_requests_do_not_become_writes() {
    for suffix in [
        b"\x18OK".as_slice(),
        b"\x1aOK",
        b"\x1b\x18\x1bcOK",
        b"\x1b\x1a\x1bcOK",
    ] {
        let mut parser = TerminalParser::default();
        let mut screen = Screen::new(2, 20);
        feed(&mut parser, &mut screen, b"\x1b]52;c;Zg==");
        assert!(parser.drain_output_events().is_empty());
        feed(&mut parser, &mut screen, suffix);
        assert!(
            !parser
                .drain_output_events()
                .iter()
                .any(|e| matches!(e, TerminalOutputEvent::ClipboardWrite(_)))
        );
        assert!(screen.row_text(0).starts_with("OK"));
        feed(&mut parser, &mut screen, &frame(b"after"));
        assert_eq!(parser.drain_output_events(), vec![event("after")]);
    }
}

#[test]
fn latest_first_and_discard_bound_pending_events_without_reordering_metadata() {
    for delivery in [
        ClipboardDelivery::Latest,
        ClipboardDelivery::First,
        ClipboardDelivery::Discard,
    ] {
        let mut parser = TerminalParser::default();
        let mut screen = Screen::new(2, 20);
        parser.set_clipboard_delivery(delivery);
        feed(&mut parser, &mut screen, b"\x1b]0;before\x07");
        feed(&mut parser, &mut screen, &frame(b"A"));
        feed(&mut parser, &mut screen, b"\x1b]0;between\x07");
        // Invalid writes never replace the admitted payload.
        feed(&mut parser, &mut screen, b"\x1b]52;c;Zh==\x07");
        for _ in 0..1000 {
            feed(&mut parser, &mut screen, &frame(b"B"));
            assert!(
                parser
                    .output_events
                    .iter()
                    .filter(|e| matches!(e, TerminalOutputEvent::ClipboardWrite(_)))
                    .count()
                    <= 1
            );
        }
        feed(&mut parser, &mut screen, b"\x1b]0;after\x07");
        let before = TerminalOutputEvent::TitleChanged("before".into());
        let between = TerminalOutputEvent::TitleChanged("between".into());
        let after = TerminalOutputEvent::TitleChanged("after".into());
        assert_eq!(
            parser.drain_output_events(),
            match delivery {
                ClipboardDelivery::Latest => vec![before, between, event("B"), after],
                ClipboardDelivery::First => vec![before, event("A"), between, after],
                ClipboardDelivery::Discard => vec![before, between, after],
            }
        );
        feed(&mut parser, &mut screen, &frame(b"C"));
        assert_eq!(
            parser.drain_output_events(),
            if delivery == ClipboardDelivery::Discard {
                vec![]
            } else {
                vec![event("C")]
            }
        );
    }
}

#[test]
fn session_closure_releases_partial_wire_state_and_pending_clipboard_only() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(2, 20);
    feed(&mut parser, &mut screen, b"\x1b]0;metadata\x07");
    feed(&mut parser, &mut screen, &frame(b"pending"));
    let partial = frame(&vec![b'a'; 100_000]);
    feed(&mut parser, &mut screen, &partial[..partial.len() - 1]);
    parser.close_session();
    assert!(format!("{parser:?}").contains("osc_bytes: 0"));
    assert_eq!(
        parser.drain_output_events(),
        vec![TerminalOutputEvent::TitleChanged("metadata".into())]
    );
    feed(&mut parser, &mut screen, b"\x07OK");
    feed(&mut parser, &mut screen, &frame(b"stale"));
    assert!(parser.drain_output_events().is_empty());
    assert!(screen.row_text(0).starts_with("OK"));
}

#[test]
fn delivery_changes_release_discarded_text_but_preserve_first_slot() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(2, 20);
    feed(&mut parser, &mut screen, &frame(b"A"));
    parser.set_clipboard_delivery(ClipboardDelivery::First);
    feed(&mut parser, &mut screen, &frame(b"B"));
    assert_eq!(parser.output_events, vec![event("A")]);
    parser.set_clipboard_delivery(ClipboardDelivery::Discard);
    assert!(parser.output_events.is_empty());
    parser.set_clipboard_delivery(ClipboardDelivery::Latest);
    feed(&mut parser, &mut screen, &frame(b"C"));
    assert_eq!(parser.drain_output_events(), vec![event("C")]);
}

#[test]
fn exact_decoded_limit_and_independent_overflow_limits() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(2, 20);
    for size in [
        ClipboardWrite::MAX_BYTES,
        ClipboardWrite::MAX_BYTES + 1,
        ClipboardWrite::MAX_BYTES + 2,
    ] {
        let text = vec![b'a'; size];
        let stream = frame(&text);
        assert!(stream.len() - 8 <= 5_592_408);
        feed(&mut parser, &mut screen, &stream);
        let events = parser.drain_output_events();
        if size == ClipboardWrite::MAX_BYTES {
            let [TerminalOutputEvent::ClipboardWrite(write)] = events.as_slice() else {
                panic!("missing bounded event");
            };
            assert_eq!(write.as_str().as_bytes(), text);
        } else {
            assert!(events.is_empty());
        }
    }
    let mut oversized = b"\x1b]52;c;".to_vec();
    oversized.extend(std::iter::repeat_n(b'A', 5_592_409));
    oversized.extend_from_slice(b"\x1b\\OK\x1b]0;still-supported\x07");
    feed(&mut parser, &mut screen, &oversized);
    assert_eq!(
        parser.drain_output_events(),
        vec![TerminalOutputEvent::TitleChanged("still-supported".into())]
    );
    assert!(screen.row_text(0).starts_with("OK"));
    assert!(screen.drain_replies().is_empty());
}

#[test]
fn repeated_large_writes_keep_one_bounded_payload_per_parser() {
    let stream = frame(&vec![b'a'; ClipboardWrite::MAX_BYTES]);
    let mut first = TerminalParser::default();
    first.set_clipboard_delivery(ClipboardDelivery::First);
    let mut latest = TerminalParser::default();
    let mut screen = Screen::new(2, 20);
    for _ in 0..4 {
        for parser in [&mut first, &mut latest] {
            feed(parser, &mut screen, &stream);
            assert_eq!(parser.output_events.len(), 1);
            assert!(
                matches!(&parser.output_events[0], TerminalOutputEvent::ClipboardWrite(w) if w.as_str().len() == ClipboardWrite::MAX_BYTES)
            );
        }
    }
    feed(&mut latest, &mut screen, &frame(b"latest"));
    assert_eq!(latest.drain_output_events(), vec![event("latest")]);
    assert_eq!(first.drain_output_events().len(), 1);
}

#[test]
fn bounded_type_and_debug_never_disclose_fixture_contents() {
    let text = "synthetic-secret";
    let write = ClipboardWrite::new(text.into()).unwrap();
    assert_eq!(write.as_str(), text);
    assert!(!format!("{write:?}").contains(text));
    assert!(!format!("{:?}", event(text)).contains(text));
    assert!(ClipboardWrite::new("bad\0fixture".into()).is_none());
    assert!(ClipboardWrite::new("a".repeat(ClipboardWrite::MAX_BYTES + 1)).is_none());
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(2, 20);
    feed(&mut parser, &mut screen, &frame(text.as_bytes()));
    let debug = format!("{parser:?}");
    assert!(!debug.contains(text));
    assert!(!debug.contains("c3ludGhldGlj"));
}
