//! Production clipboard-paste decision/write seam, without a native clipboard or dialog.
use super::*;
use std::sync::atomic::AtomicUsize;

use harbor_app::tab_manager::{PaneId, SessionId, TabId};

fn binding(n: u64) -> PaneBinding {
    PaneBinding {
        workspace: TabId(10),
        pane: PaneId(n + 20),
        session: SessionId(n + 30),
    }
}

/// Keep the synthetic reader live until the test releases it; EOF is retirement now.
struct HeldReader(std::sync::mpsc::Receiver<()>);
impl std::io::Read for HeldReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.0.recv();
        Ok(0)
    }
}

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
        let (_reader_lifetime, reader) = std::sync::mpsc::channel();
        let mut terminal =
            Terminal::new_headless_with_io(4, 20, HeldReader(reader), writer, || true);
        terminal.process_output(modes);
        let terminal = Arc::new(Mutex::new(terminal));
        let target = PasteTarget::new(binding(1), &terminal);
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
                complete_paste_confirmation(&outcome, Some(&target), Some((binding(1), &terminal))),
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
    let (_reader_lifetime, reader) = std::sync::mpsc::channel();
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        HeldReader(reader),
        writer,
        || true,
    )));
    assert_eq!(
        prepare_paste(Some(&terminal), "plain").unwrap_err(),
        "Failed: could not write clipboard paste"
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        complete_paste_confirmation(
            &DialogOutcome::Cancelled,
            Some(&PasteTarget::new(binding(1), &terminal)),
            Some((binding(1), &terminal))
        ),
        Err("Cancelled: paste was not confirmed")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        complete_paste_confirmation(
            &DialogOutcome::Confirmed("plain".into()),
            Some(&PasteTarget::new(binding(1), &terminal)),
            Some((binding(1), &terminal))
        ),
        Err("Failed: could not write confirmed paste")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn confirmation_never_redirects_to_a_survivor_or_replacement_and_is_one_shot() {
    let gate = Arc::new(AtomicBool::new(true));
    let mut paste = PasteController::new(gate.clone());
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let (_reader_lifetime, reader) = std::sync::mpsc::channel();
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        HeldReader(reader),
        CaptureWriter {
            bytes: bytes.clone(),
            calls: calls.clone(),
            fail: false,
        },
        || true,
    )));
    let target = PasteTarget::new(binding(1), &terminal);
    let outcome = DialogOutcome::Confirmed("first\nsecond".into());
    for current in [None, Some((binding(2), &terminal))] {
        assert!(complete_paste_confirmation(&outcome, Some(&target), current).is_err());
    }
    // Even a malformed same-binding replacement cannot substitute another engine.
    let replacement = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
    assert!(
        complete_paste_confirmation(&outcome, Some(&target), Some((binding(1), &replacement)))
            .is_err()
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    paste.target = Some(target);
    paste.palette_invocation = true;
    assert_eq!(paste.source(), Some(binding(1)));
    // Cancellation revokes target + shared gate before any queued dialog event.
    paste.cancel();
    assert_eq!(paste.source(), None);
    assert!(!gate.load(Ordering::Acquire));
    assert!(!paste.palette_invocation);
    assert!(
        complete_paste_confirmation(
            &outcome,
            paste.target.as_ref(),
            Some((binding(1), &terminal))
        )
        .is_err()
    );

    let target = PasteTarget::new(binding(1), &terminal);
    terminal.lock().unwrap().retire_session();
    assert!(
        complete_paste_confirmation(&outcome, Some(&target), Some((binding(1), &terminal)))
            .is_err()
    );
    assert!(prepare_paste(Some(&terminal), "plain").is_err());
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[cfg(windows)]
mod pty {
    use super::*;
    use std::time::Duration;

    #[test]
    #[ignore = "pty: Windows ConPTY with a dedicated local cmd child"]
    fn pending_paste_rejects_actual_shell_exit_before_and_after_model_retirement() {
        let command = harbor_pty::ShellCommand {
            program: Some("cmd.exe".into()),
            args: vec!["/D".into(), "/Q".into()],
            ..Default::default()
        };
        let endpoints = harbor_pty::PtyEndpoints::spawn_shell(
            harbor_pty::TerminalSize { rows: 8, cols: 40 },
            &command,
        )
        .unwrap();
        let receipt = endpoints.shutdown_receipt();
        let (exited, exit) = std::sync::mpsc::channel();
        endpoints
            .observe_exit(move || {
                let _ = exited.send(());
            })
            .unwrap();
        let terminal = Terminal::new_headless(8, 40)
            .start_session_from_endpoints(endpoints, || true)
            .unwrap();
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(terminal));
        let target = PasteTarget::new(binding(1), &terminal);
        let outcome = DialogOutcome::Confirmed("must not be delivered\n".into());
        terminal.lock().unwrap().write_pty(b"exit\r").unwrap();
        exit.recv_timeout(Duration::from_secs(15)).unwrap();
        assert!(terminal.lock().unwrap().shell_process_exited());
        assert_eq!(
            complete_paste_confirmation(&outcome, Some(&target), Some((binding(1), &terminal))),
            Err("Rejected: paste target retired")
        );
        terminal.lock().unwrap().retire_session();
        assert_eq!(
            complete_paste_confirmation(&outcome, Some(&target), Some((binding(1), &terminal))),
            Err("Rejected: paste target retired")
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while !receipt.is_complete() {
            assert!(
                Instant::now() < deadline,
                "source native teardown incomplete"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let gate = Arc::new(AtomicBool::new(true));
        let mut paste = PasteController::new(gate.clone());
        paste.target = Some(target);
        paste.palette_invocation = true;
        assert_eq!(
            paste.retire_target(),
            Some(Err("Rejected: paste target retired"))
        );
        assert_eq!(
            paste.retire_target(),
            None,
            "retirement completion is reported once"
        );
        assert!(!gate.load(Ordering::Acquire));
        assert_eq!(paste.source(), None);
    }
}
