use super::TerminalParser;
use crate::screen::Screen;
use crate::{ShellIntegrationMarker, TerminalOutputEvent, WorkingDirectoryMetadata};

fn observed(stream: &[u8], chunk_size: usize) -> (Vec<TerminalOutputEvent>, Vec<u8>, String) {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(3, 80);
    for chunk in stream.chunks(chunk_size) {
        assert_eq!(parser.put_bytes(&mut screen, chunk).consumed, chunk.len());
    }
    (
        parser.drain_output_events(),
        screen.drain_replies(),
        screen.row_text(0),
    )
}

#[test]
fn all_builtins_keep_event_and_reply_order_for_bel_st_and_fragmented_bytes() {
    let stream = b"\x1b]0;first\x07\x1b]1;second\x1b\\\x1b]2;third\x07\
        \x1b]7;file:///tmp\x1b\\\x1b]8;id=tag;https://example.test\x07X\
        \x1b]8;;\x1b\\\x1b]10;#123456\x07\x1b]11;#abcdef\x1b\\\x1b]12;#445566\x07\
        \x1b]10;?\x07\x1b]11;?\x1b\\\x1b]12;?\x07\
        \x1b]110;\x07\x1b]111;\x1b\\\x1b]112;\x07\x1b]133;A\x1b\\\
        \x1b]999;ignored\x07Z";
    let expected_events = vec![
        TerminalOutputEvent::TitleChanged("first".into()),
        TerminalOutputEvent::TitleChanged("second".into()),
        TerminalOutputEvent::TitleChanged("third".into()),
        TerminalOutputEvent::WorkingDirectoryChanged(WorkingDirectoryMetadata {
            host: None,
            path: "/tmp".into(),
        }),
        TerminalOutputEvent::ShellIntegration(ShellIntegrationMarker::PromptStart),
    ];
    let bulk = observed(stream, stream.len());
    assert_eq!(bulk.0, expected_events);
    assert_eq!(bulk.1, b"\x1b]10;rgb:1212/3434/5656\x07\x1b]11;rgb:abab/cdcd/efef\x1b\\\x1b]12;rgb:4444/5555/6666\x07");
    assert!(bulk.2.starts_with("XZ"));
    for chunk in [1, 2, 3, 7, 31] {
        assert_eq!(observed(stream, chunk), bulk, "fragment size {chunk}");
    }
}

#[test]
fn empty_payloads_and_ris_keep_reset_order_across_terminators() {
    let stream = b"\x1b]0;\x07\x1b]7;\x1b\\\x1b]133;\x07\x1bcX";
    let (events, replies, text) = observed(stream, 1);
    assert_eq!(
        events,
        vec![
            TerminalOutputEvent::TitleReset,
            TerminalOutputEvent::WorkingDirectoryReset,
            TerminalOutputEvent::ShellIntegrationReset,
            TerminalOutputEvent::TitleReset,
            TerminalOutputEvent::WorkingDirectoryReset,
            TerminalOutputEvent::ShellIntegrationReset,
        ]
    );
    assert!(replies.is_empty());
    assert!(text.starts_with('X'));
}

#[test]
fn invalid_and_oversized_payloads_are_ignored_without_swallowing_later_text() {
    let mut stream = Vec::new();
    for command in [b"0".as_slice(), b"1", b"2"] {
        stream.extend_from_slice(b"\x1b]");
        stream.extend_from_slice(command);
        stream.extend_from_slice(b";bad\x00title\x07");
    }
    for (command, payload) in [
        (b"7".as_slice(), b"file:///bad%gg".as_slice()),
        (b"8", b"id;https://example.test"),
        (b"10", b"red"),
        (b"11", b"red"),
        (b"12", b"red"),
        (b"110", b"?"),
        (b"111", b"?"),
        (b"112", b"?"),
        (b"133", b"D;NaN"),
    ] {
        stream.extend_from_slice(b"\x1b]");
        stream.extend_from_slice(command);
        stream.extend_from_slice(b";");
        stream.extend_from_slice(payload);
        stream.extend_from_slice(b"\x1b\\");
    }
    for (command, count) in [
        (b"0".as_slice(), 257),
        (b"7", 2050),
        (b"8", 2050),
        (b"133", 1025),
    ] {
        stream.extend_from_slice(b"\x1b]");
        stream.extend_from_slice(command);
        stream.extend_from_slice(b";");
        match command {
            b"7" => stream.extend_from_slice(b"file:///"),
            b"8" => stream.extend_from_slice(b";"),
            _ => {}
        }
        stream.extend(std::iter::repeat_n(b'a', count));
        stream.push(0x07);
    }
    stream.extend_from_slice(b"\x1b]999;ignored\x07\x1b]0;");
    stream.extend(std::iter::repeat_n(b'a', 4097));
    stream.extend_from_slice(b"\x07OK");
    let (events, replies, text) = observed(&stream, 1);
    assert!(events.is_empty(), "events: {events:?}");
    assert!(replies.is_empty());
    assert!(text.starts_with("OK"), "text: {text:?}");
}
