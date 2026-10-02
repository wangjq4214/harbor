//! GPU-free tests of the public terminal-to-host clipboard request boundary.
use harbor_terminal::{ClipboardDelivery, ClipboardWrite, Terminal, TerminalOutputEvent};

#[test]
fn public_delivery_api_forwards_through_io_and_keeps_unrelated_fifo_events() {
    for mode in [
        ClipboardDelivery::Latest,
        ClipboardDelivery::First,
        ClipboardDelivery::Discard,
    ] {
        let mut terminal = Terminal::new_headless(2, 20);
        assert!(!terminal.is_session_closed());
        terminal.set_clipboard_delivery(mode);
        terminal
            .put_bytes(b"\x1b]0;before\x07\x1b]52;c;QQ==\x07\x1b]0;between\x07\x1b]52;;Qg\x1b\\");
        let title = |text: &str| TerminalOutputEvent::TitleChanged(text.into());
        let write = |text: &str| {
            TerminalOutputEvent::ClipboardWrite(ClipboardWrite::new(text.into()).unwrap())
        };
        assert_eq!(
            terminal.drain_output_events(),
            match mode {
                ClipboardDelivery::Latest => vec![title("before"), title("between"), write("B")],
                ClipboardDelivery::First => vec![title("before"), write("A"), title("between")],
                ClipboardDelivery::Discard => vec![title("before"), title("between")],
            }
        );
        assert!(terminal.drain_output_events().is_empty());
    }
}

#[test]
fn default_is_latest_and_discard_releases_pending_text_without_discarding_metadata() {
    let mut terminal = Terminal::new_headless(2, 20);
    terminal.put_bytes(b"\x1b]52;c;QQ==\x07\x1b]52;c;Qg==\x07");
    let events = terminal.drain_output_events();
    let [TerminalOutputEvent::ClipboardWrite(write)] = events.as_slice() else {
        panic!("expected one write");
    };
    assert_eq!(write.as_str(), "B");
    assert_eq!(write.clone().into_text(), "B");
    terminal.put_bytes(b"\x1b]52;c;QQ==\x07\x1b]0;metadata\x07");
    terminal.set_clipboard_delivery(ClipboardDelivery::Discard);
    assert_eq!(
        terminal.drain_output_events(),
        vec![TerminalOutputEvent::TitleChanged("metadata".into())]
    );
}
