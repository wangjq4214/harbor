#[path = "tests/architecture.rs"]
mod architecture;
mod paste_command_tests;

use super::*;

#[test]
fn confirmed_paste_writes_unchanged_raw_text_with_current_modes() {
    for text in ["first\r\nsecond\t\x1b[A", "hello\nworld\r\n"] {
        let mut writes = Vec::new();

        let wrote = write_confirmation_outcome(Some(text), InputModes::default(), |bytes| {
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
    let mut writes = Vec::new();
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };

    let wrote = write_confirmation_outcome(Some("first\nsecond"), modes, |bytes| {
        writes.push(bytes.to_vec());
        Ok::<_, ()>(())
    })
    .expect("capture writer succeeds");

    assert!(wrote);
    assert_eq!(writes, vec![b"\x1b[200~first\nsecond\x1b[201~".to_vec()]);
}

#[test]
fn cancelled_confirmation_never_calls_the_writer() {
    let wrote = write_confirmation_outcome(None, InputModes::default(), |_| -> Result<(), ()> {
        panic!("cancelled outcomes must not write to the PTY")
    })
    .expect("no-write outcome cannot fail");
    assert!(!wrote);
}

#[test]
fn should_return_the_writer_error_after_one_confirmed_paste_attempt() {
    let write_attempts = std::cell::Cell::new(0);

    let result = write_confirmation_outcome(Some("paste"), InputModes::default(), |_| {
        write_attempts.set(write_attempts.get() + 1);
        Err::<(), _>("PTY disconnected")
    });

    assert_eq!(result, Err("PTY disconnected"));
    assert_eq!(write_attempts.get(), 1);
}
