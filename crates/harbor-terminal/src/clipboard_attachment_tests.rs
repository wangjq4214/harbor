use super::*;
use std::sync::mpsc;

struct LiveReader(mpsc::Receiver<()>);
impl Read for LiveReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.0.recv();
        Ok(0)
    }
}

#[test]
fn osc52_delivery_configured_before_session_attachment_is_preserved() {
    for delivery in [
        ClipboardDelivery::First,
        ClipboardDelivery::Discard,
        ClipboardDelivery::Latest,
    ] {
        let mut terminal = Terminal::new_headless(2, 20);
        terminal.set_clipboard_delivery(delivery);
        let (close_tx, close_rx) = mpsc::channel();
        let io = TerminalIo::new(LiveReader(close_rx), std::io::sink(), None, || true);
        // Exercise the same attachment path as start_session_from_endpoints without an OS PTY.
        let mut terminal = terminal.attach_io(io);
        assert!(!terminal.is_session_closed());
        terminal.put_bytes(b"\x1b]52;c;QQ==\x07\x1b]52;c;Qg==\x07\x1b]0;metadata\x07");
        let metadata = TerminalOutputEvent::TitleChanged("metadata".into());
        let write = |text: &str| {
            TerminalOutputEvent::ClipboardWrite(ClipboardWrite::new(text.into()).unwrap())
        };
        assert_eq!(
            terminal.drain_output_events(),
            match delivery {
                ClipboardDelivery::First => vec![write("A"), metadata],
                ClipboardDelivery::Discard => vec![metadata],
                ClipboardDelivery::Latest => vec![write("B"), metadata],
            }
        );
        close_tx.send(()).unwrap();
    }
}
