//! Production clipboard-paste decision/write seam, without a native clipboard or dialog.
use super::*;
use std::sync::atomic::AtomicUsize;

struct CaptureWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    calls: Arc<AtomicUsize>,
    fail: bool,
}
impl std::io::Write for CaptureWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "test writer",
            ));
        }
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn clipboard_paste_direct_bracketed_and_confirmation_use_existing_policy_once() {
    for (modes, text, expected, confirmation) in [
        (b"".as_slice(), "", b"".as_slice(), false),
        (
            b"".as_slice(),
            "plain\ttext",
            b"plain\ttext".as_slice(),
            false,
        ),
        (b"", "first\r\nsecond", b"".as_slice(), true),
        (
            b"\x1b[?2004h",
            "first\nsecond",
            b"\x1b[200~first\nsecond\x1b[201~".as_slice(),
            false,
        ),
    ] {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let writer = CaptureWriter {
            bytes: bytes.clone(),
            calls: calls.clone(),
            fail: false,
        };
        let mut terminal = Terminal::new_headless_with_io(4, 20, std::io::empty(), writer, || true);
        terminal.process_output(modes);
        let terminal = Arc::new(Mutex::new(terminal));
        let disposition = prepare_paste(Some(&terminal), text).unwrap();
        assert_eq!(
            matches!(disposition, PasteDisposition::Confirm { .. }),
            confirmation
        );
        assert_eq!(&*bytes.lock().unwrap(), expected);
        assert_eq!(
            calls.load(Ordering::Relaxed),
            usize::from(!confirmation && !text.is_empty())
        );
        if let PasteDisposition::Confirm { raw_text } = disposition {
            assert_eq!(raw_text, text);
            let outcome = DialogOutcome::Confirmed(raw_text);
            // Confirmation uses the production completion path and current modes.
            terminal.lock().unwrap().process_output(b"\x1b[?2004h");
            assert_eq!(
                complete_paste_confirmation(&outcome, Some(&terminal)),
                Ok(CommandCompletion::Applied)
            );
            assert_eq!(
                &*bytes.lock().unwrap(),
                b"\x1b[200~first\r\nsecond\x1b[201~"
            );
            assert_eq!(calls.load(Ordering::Relaxed), 1);
        }
    }
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn clipboard_paste_propagates_actual_write_failure_and_missing_terminal() {
    assert!(
        prepare_paste(None, "plain")
            .unwrap_err()
            .contains("no active terminal")
    );
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let writer = CaptureWriter {
        bytes: bytes.clone(),
        calls: calls.clone(),
        fail: true,
    };
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        std::io::empty(),
        writer,
        || true,
    )));
    assert_eq!(
        prepare_paste(Some(&terminal), "plain").unwrap_err(),
        "Failed: could not write clipboard paste"
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        complete_paste_confirmation(&DialogOutcome::Cancelled, Some(&terminal)),
        Err("Cancelled: paste was not confirmed")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        complete_paste_confirmation(&DialogOutcome::Confirmed("plain".into()), Some(&terminal)),
        Err("Failed: could not write confirmed paste")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}
