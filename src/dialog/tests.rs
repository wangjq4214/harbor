#[path = "tests/architecture.rs"]
mod architecture;

use super::*;

#[test]
fn confirmed_paste_writes_unchanged_raw_text_with_current_modes() {
    for text in ["first\r\nsecond\t\x1b[A", "hello\nworld\r\n"] {
        let outcome = DialogOutcome::Confirmed(text.to_owned());
        let mut writes = Vec::new();

        let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |bytes| {
            writes.push(bytes.to_vec());
            Ok::<_, ()>(())
        })
        .expect("capture writer succeeds");

        assert!(wrote, "{text:?}");
        assert_eq!(writes, vec![text.as_bytes().to_vec()], "{text:?}");
    }
}

#[test]
fn confirmed_paste_uses_bracketed_mode_current_at_confirmation_time() {
    let outcome = DialogOutcome::Confirmed("first\nsecond".to_owned());
    let mut writes = Vec::new();
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };

    let wrote = write_confirmation_outcome(&outcome, modes, |bytes| {
        writes.push(bytes.to_vec());
        Ok::<_, ()>(())
    })
    .expect("capture writer succeeds");

    assert!(wrote);
    assert_eq!(writes, vec![b"\x1b[200~first\nsecond\x1b[201~".to_vec()]);
}

#[test]
fn cancelled_or_closed_confirmation_never_calls_the_writer() {
    for outcome in [DialogOutcome::Cancelled, DialogOutcome::None] {
        let wrote =
            write_confirmation_outcome(&outcome, InputModes::default(), |_| -> Result<(), ()> {
                panic!("cancelled and closed outcomes must not write to the PTY")
            })
            .expect("no-write outcome cannot fail");
        assert!(!wrote);
    }
}

#[test]
fn should_not_write_to_pty_when_confirmation_frame_is_fatal() {
    let outcome = DialogOutcome::Fatal(FrameError::out_of_memory());
    let write_attempts = std::cell::Cell::new(0);

    let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
        write_attempts.set(write_attempts.get() + 1);
        Ok::<_, ()>(())
    })
    .expect("fatal outcome cannot invoke the writer");

    assert!(!wrote);
    assert_eq!(write_attempts.get(), 0);
}

#[test]
fn should_return_the_writer_error_after_one_confirmed_paste_attempt() {
    let outcome = DialogOutcome::Confirmed("paste".to_owned());
    let write_attempts = std::cell::Cell::new(0);

    let result = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
        write_attempts.set(write_attempts.get() + 1);
        Err::<(), _>("PTY disconnected")
    });

    assert_eq!(result, Err("PTY disconnected"));
    assert_eq!(write_attempts.get(), 1);
}
