use super::*;

#[test]
fn queued_capability_queries_reach_the_reply_writer_with_exact_bytes() {
    let queries = b"\x1b[c\x1b[>c\x1b[5n\x1b[6n\x1b[?6n\x1b[?1$p\x1bP$qm\x1b\\\x1bP+q544E;436F;636F6C6F7273;5538;7538\x1b\\";
    let (mut terminal, written, wake_rx) = terminal_with_io(std::io::Cursor::new(queries));
    wait_for_pty_wake(&wake_rx);
    terminal.drain_pty();
    assert_eq!(
        written.lock().unwrap().as_slice(),
        b"\x1b[?62;6;17;22;28c\x1b[>1;1;0c\x1b[0n\x1b[1;1R\x1b[?1;1R\x1b[?1;2$y\x1bP1$r0m\x1b\\\x1bP1+r544E=787465726D2D323536636F6C6F72;436F=323536;636F6C6F7273=323536;5538=31;7538\x1b\\"
    );
}

#[test]
fn queued_smkx_rmkx_select_terminfo_cursor_home_and_end_encoding_before_input() {
    for (sequence, expected) in [
        (
            b"\x1b[?1h\x1b=".as_slice(),
            b"\x1bOA\x1bOB\x1bOC\x1bOD\x1bOH\x1bOF".as_slice(),
        ),
        (
            b"\x1b[?1h\x1b=\x1b[?1l\x1b>",
            b"\x1b[A\x1b[B\x1b[C\x1b[D\x1b[H\x1b[F",
        ),
    ] {
        let (mut terminal, written, wake_rx) = terminal_with_io(std::io::Cursor::new(sequence));
        wait_for_pty_wake(&wake_rx);
        // Do not explicitly drain: pending mode changes must precede input encoding.
        for key in [
            TerminalKey::ArrowUp,
            TerminalKey::ArrowDown,
            TerminalKey::ArrowRight,
            TerminalKey::ArrowLeft,
            TerminalKey::Home,
            TerminalKey::End,
        ] {
            terminal
                .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
                    key,
                    modifiers: TerminalModifiers::default(),
                }))
                .unwrap();
        }
        assert_eq!(written.lock().unwrap().as_slice(), expected);
    }
}

#[test]
fn resize_rejects_active_reader_without_interrupt_capability() {
    let output = b"L00:abcdefgh\r\nL01:ijklmnop\r\nL02:qrstuvwx".to_vec();
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::from([output]),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should enqueue output before resize");

    let error = terminal
        .try_resize_if_changed(TerminalSize { rows: 2, cols: 4 })
        .expect_err("an active reader without interrupt control cannot resize safely");

    assert!(error.to_string().contains("no interrupt capability"));
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 8));
    assert!(terminal.drain_pty());
    assert!(terminal.snapshot().cells.iter().any(|cell| cell.ch == 'L'));
}

#[test]
fn configuration_group_failure_resumes_every_paused_reader_without_losing_bytes() {
    for failure in 0..3 {
        let mut terminals = Vec::new();
        let mut readers = Vec::new();
        for _ in 0..2 {
            let (blocked_tx, blocked_rx) = std::sync::mpsc::channel();
            let (interrupt_tx, interrupt_rx) = std::sync::mpsc::channel();
            let reader = BarrierTestReader {
                state: 0,
                initial: b"BEFORE".to_vec(),
                blocked: blocked_tx,
                interrupted: interrupt_rx,
                after_resume: b"AFTER".to_vec(),
            };
            let mut terminal = Terminal::new_headless(4, 20);
            terminal.io = crate::io::TerminalIo::new_with_test_barrier(
                reader,
                std::io::sink(),
                move |_| {
                    let _ = interrupt_tx.send(());
                    Ok(())
                },
                std::time::Duration::from_secs(1),
                || true,
            );
            blocked_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap();
            readers.push(blocked_rx);
            terminals.push(terminal);
        }
        let leases = terminals
            .iter_mut()
            .map(|terminal| terminal.begin_configuration_resize().unwrap())
            .collect::<Vec<_>>();
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            for terminal in &mut terminals {
                terminal.drain_pty();
            }
            if leases.iter().all(|lease| lease.is_ready().unwrap()) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let transactions = terminals
            .iter_mut()
            .map(|terminal| {
                terminal
                    .prepare_configuration_resize(TerminalSize { rows: 3, cols: 12 })
                    .unwrap()
            })
            .collect::<Vec<_>>();
        // Preparation rejection, compensated later-session failure, and double failure.
        let mut transactions = transactions;
        if failure != 0 {
            let mut calls = 0;
            let error = crate::configuration::resize_group_with(&mut transactions, |_, _, _| {
                calls += 1;
                if calls == 2 || (failure == 2 && calls == 3) {
                    anyhow::bail!("injected resize failure");
                }
                Ok(())
            })
            .unwrap_err();
            assert_eq!(error.failures.len(), failure);
        }
        drop(transactions);
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            for terminal in &mut terminals {
                terminal.drain_pty();
            }
            if terminals
                .iter()
                .all(|terminal| terminal.row_text(0).starts_with("BEFOREAFTER"))
            {
                break;
            }
            assert!(Instant::now() < deadline, "reader did not resume");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        for (index, terminal) in terminals.iter().enumerate() {
            assert_eq!(
                terminal.screen().cols(),
                if failure == 2 && index == 0 { 12 } else { 20 }
            );
            assert_eq!(terminal.row_text(0).trim_end(), "BEFOREAFTER");
        }
    }
}

#[test]
fn barrier_resize_preserves_single_narrow_review_and_copy_without_duplication() {
    let initial: Vec<u8> = (0..12)
        .flat_map(|line| format!("L{line:02}:abcdefg\r\n").into_bytes())
        .collect();
    let after_resume: Vec<u8> = (12..24)
        .flat_map(|line| format!("L{line:02}:abcdefg\r\n").into_bytes())
        .collect();
    let (blocked_tx, blocked_rx) = std::sync::mpsc::channel();
    let (interrupt_tx, interrupt_rx) = std::sync::mpsc::channel();
    let reader = BarrierTestReader {
        state: 0,
        initial: initial.clone(),
        blocked: blocked_tx,
        interrupted: interrupt_rx,
        after_resume: after_resume.clone(),
    };
    let bytes = Default::default();
    let writer = RecordingWriter {
        bytes,
        max_write: usize::MAX,
    };
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut terminal = Terminal::new_headless(4, 12);
    terminal.io = crate::io::TerminalIo::new_with_test_barrier(
        reader,
        writer,
        move |_| interrupt_tx.send(()).map_err(Into::into),
        std::time::Duration::from_secs(1),
        move || wake_tx.send(()).is_ok(),
    );
    wait_for_pty_wake(&wake_rx);
    blocked_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should block after publishing the old-geometry chunk");

    resize_to_completion(&mut terminal, TerminalSize { rows: 6, cols: 6 }).unwrap();
    wait_for_pty_bytes(&mut terminal, &wake_rx);
    scroll_to_review_seam(&mut terminal, "L12:");
    let copied = select_visible_viewport(&mut terminal, 91);

    let mut reference = Terminal::new_headless(4, 12);
    reference.put_str(std::str::from_utf8(&initial).unwrap());
    assert!(reference.resize_if_changed(TerminalSize { rows: 6, cols: 6 }));
    reference.process_output(&after_resume);
    scroll_to_review_seam(&mut reference, "L12:");
    let expected = select_visible_viewport(&mut reference, 92);

    assert_eq!(copied, expected);
    assert!(
        copied.contains("L12"),
        "copied viewport must cross from old-geometry rows into post-resume output: {copied:?}; rows={:?}",
        [terminal.row_text(2), terminal.row_text(3)]
    );
    let markers: Vec<_> = copied
        .split_whitespace()
        .filter(|word| word.starts_with('L'))
        .collect();
    assert!(
        !markers.is_empty(),
        "copied review region must contain line markers"
    );
    let unique: std::collections::HashSet<_> = markers.iter().copied().collect();
    assert_eq!(markers.len(), unique.len(), "copied lines were duplicated");
}

#[test]
fn barrier_timeout_preserves_geometry_and_late_ack_cannot_block_retry() {
    let (blocked_tx, blocked_rx) = std::sync::mpsc::channel();
    let (interrupt_tx, interrupt_rx) = std::sync::mpsc::channel();
    let reader = BarrierTestReader {
        state: 0,
        initial: b"old".to_vec(),
        blocked: blocked_tx,
        interrupted: interrupt_rx,
        after_resume: b"after".to_vec(),
    };
    let bytes = Default::default();
    let writer = RecordingWriter {
        bytes,
        max_write: usize::MAX,
    };
    let allow_interrupt = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let callback_flag = std::sync::Arc::clone(&allow_interrupt);
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut terminal = Terminal::new_headless(2, 8);
    terminal.io = crate::io::TerminalIo::new_with_test_barrier(
        reader,
        writer,
        move |_| {
            if callback_flag.load(std::sync::atomic::Ordering::Acquire) {
                interrupt_tx.send(()).map_err(Into::into)
            } else {
                Ok(())
            }
        },
        std::time::Duration::from_millis(25),
        move || wake_tx.send(()).is_ok(),
    );
    wait_for_pty_wake(&wake_rx);
    blocked_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should enter its blocking read");

    assert!(
        !terminal
            .try_resize_if_changed(TerminalSize { rows: 2, cols: 4 })
            .unwrap()
    );
    let deadline = Instant::now() + std::time::Duration::from_secs(1);
    let error = loop {
        terminal.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer));
        if let Err(error) = terminal.try_resize_if_changed(TerminalSize { rows: 2, cols: 4 }) {
            break error;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    assert!(error.to_string().contains("timed out"));
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 8));

    allow_interrupt.store(true, std::sync::atomic::Ordering::Release);
    resize_to_completion(&mut terminal, TerminalSize { rows: 2, cols: 4 }).unwrap();
    wait_for_pty_bytes(&mut terminal, &wake_rx);
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 4));
    assert!(terminal.snapshot().cells.iter().any(|cell| cell.ch == 'a'));
}

#[test]
fn barrier_interrupt_failure_preserves_geometry_and_resumes_late_reader() {
    let (blocked_tx, blocked_rx) = std::sync::mpsc::channel();
    let (interrupt_tx, interrupt_rx) = std::sync::mpsc::channel();
    let reader = BarrierTestReader {
        state: 0,
        initial: b"old".to_vec(),
        blocked: blocked_tx,
        interrupted: interrupt_rx,
        after_resume: b"resumed".to_vec(),
    };
    let writer = RecordingWriter {
        bytes: Default::default(),
        max_write: usize::MAX,
    };
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let mut terminal = Terminal::new_headless(2, 8);
    terminal.io = crate::io::TerminalIo::new_with_test_barrier(
        reader,
        writer,
        move |_| anyhow::bail!("injected interrupt failure"),
        std::time::Duration::from_secs(1),
        move || wake_tx.send(()).is_ok(),
    );
    wait_for_pty_wake(&wake_rx);
    blocked_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should enter its blocking read");

    let error = terminal
        .try_resize_if_changed(TerminalSize { rows: 2, cols: 4 })
        .expect_err("interrupt failure must reject resize");
    assert!(error.to_string().contains("injected interrupt failure"));
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 8));

    interrupt_tx.send(()).unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(1);
    while !terminal.snapshot().cells.iter().any(|cell| cell.ch == 'r') {
        assert!(Instant::now() < deadline);
        let _ = wake_rx.recv_timeout(std::time::Duration::from_millis(5));
        terminal.drain_pty();
    }
    assert!(terminal.snapshot().cells.iter().any(|cell| cell.ch == 'r'));
}

#[test]
fn sgr_mouse_routes_cell_coordinates_button_state_and_vt_capture_to_pty() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 2.0));
    terminal.process_output(b"\x1b[?1003;1006h");

    let down = terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (27.0, 23.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            7,
        )))
        .unwrap();
    assert_eq!(down.capture_pointer, Some(7));

    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (999.0, 999.0),
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            7,
        )))
        .unwrap();
    let up = terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (-5.0, -5.0),
            TerminalPointerPhase::Up,
            TerminalPointerButton::Left,
            7,
        )))
        .unwrap();

    assert_eq!(up.release_pointer, Some(7));
    assert_eq!(
        written.lock().unwrap().as_slice(),
        b"\x1b[<0;3;2M\x1b[<32;8;2M\x1b[<0;1;1m"
    );

    terminal.process_output(b"\x1b[?1006l");
    let before = written.lock().unwrap().len();
    let legacy = terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (2.0, 2.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            8,
        )))
        .unwrap();
    assert_eq!(legacy.capture_pointer, Some(8));
    assert_eq!(&written.lock().unwrap()[before..], b"\x1b[M\x20\x21\x21");
    let before = written.lock().unwrap().len();

    terminal.process_output(b"\x1bc");
    assert_eq!(terminal.screen().input_modes(), InputModes::default());
    let released = terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (2.0, 2.0),
            TerminalPointerPhase::Cancel,
            TerminalPointerButton::None,
            8,
        )))
        .unwrap();
    assert_eq!(released.release_pointer, Some(8));

    let local = terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (2.0, 2.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            10,
        )))
        .unwrap();
    assert_eq!(local.capture_pointer, Some(10));
    assert_eq!(written.lock().unwrap().len(), before);
}

#[test]
fn pty_reader_output_is_drained_fifo_coalesces_wakes_and_refreshes_snapshot() {
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::from([b"first\r\n".to_vec(), b"second".to_vec()]),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);

    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should request a redraw for queued output");
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should enqueue both chunks before EOF");
    assert!(
        wake_rx
            .recv_timeout(std::time::Duration::from_millis(20))
            .is_err(),
        "queued chunks must share one pending wake"
    );

    // drain_and_snapshot drains queued output before exposing current parser/screen state.
    let snapshot = terminal.drain_and_snapshot();
    assert_eq!(terminal.row_text(0), "first   ");
    assert_eq!(terminal.row_text(1), "second  ");
    assert_eq!(snapshot.cursor_y, 1);
    assert!(!terminal.drain_pty());
}

#[test]
fn focus_event_drains_queued_mode_and_reset_changes_before_reporting() {
    let cases: &[(&[u8], &[u8])] = &[
        (b"\x1b[?1004h", b"\x1b[I"),
        (b"\x1b[?1004h\x1b[?1004l", b""),
        (b"\x1b[?1004h\x1bc", b""),
    ];

    for &(queued, expected) in cases {
        let (completed_tx, completed_rx) = std::sync::mpsc::channel();
        let reader = CompletedScriptedReader {
            chunks: std::collections::VecDeque::from([queued.to_vec()]),
            completed: completed_tx,
        };
        let (mut terminal, written, wake_rx) = terminal_with_io(reader);
        wait_for_pty_wake(&wake_rx);
        completed_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("reader should queue mode bytes before the focus event");

        terminal
            .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
            .unwrap();

        assert_eq!(
            written.lock().unwrap().as_slice(),
            expected,
            "unexpected focus report after queued bytes {queued:?}"
        );
    }
}

#[test]
fn direct_widget_input_writes_all_encoded_bytes() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Character('c'),
            modifiers: TerminalModifiers {
                ctrl: true,
                ..TerminalModifiers::default()
            },
        }))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(
            "語".into(),
        )))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Character('x'),
            modifiers: TerminalModifiers {
                alt: true,
                ..TerminalModifiers::default()
            },
        }))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
            key: TerminalKey::Enter,
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();

    assert_eq!(
        written.lock().unwrap().as_slice(),
        [b"\x03".as_slice(), "語".as_bytes(), b"\x1bx".as_slice()].concat()
    );
}

#[test]
fn preedit_replaces_transient_state_without_screen_or_pty_mutation() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal.put_str("prompt> ");
    let before = terminal.snapshot();

    let first = terminal
        .handle_event_with_outcome(TerminalEvent::Preedit(Preedit::new("ni", Some((2, 2)))))
        .unwrap();
    let replacement = terminal
        .handle_event_with_outcome(TerminalEvent::Preedit(Preedit::new("你", Some((3, 3)))))
        .unwrap();

    assert!(first.redraw);
    assert!(replacement.redraw);
    assert_eq!(terminal.snapshot(), before);
    assert_eq!(terminal.preedit(), Some(&Preedit::new("你", Some((3, 3)))));
    assert!(written.lock().unwrap().is_empty());
}

#[test]
fn should_wake_once_after_eof_so_close_can_be_observed() {
    // Arrange
    let (reader, reads, started_rx, _exited_rx) = eof_reader();
    let (_terminal, _written, wake_rx) = terminal_with_io(reader);

    // Act
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should attempt its initial read");
    wait_for_pty_wake(&wake_rx);

    // Assert
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(wake_rx.try_recv().is_err());
}

#[test]
fn should_stop_reader_when_terminal_receiver_is_disconnected() {
    // Arrange
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (permit_tx, permit_rx) = std::sync::mpsc::channel();
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = BlockingThenChunkReader {
        reads: std::sync::Arc::clone(&reads),
        entered: entered_tx,
        permit: permit_rx,
        completed: completed_tx,
    };
    let (terminal, _written, _wake_rx) = terminal_with_io(reader);
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should block waiting for input");

    // Act
    drop(terminal);
    permit_tx.send(()).expect("reader should still be waiting");

    // Assert
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should exit after its send is rejected");
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn should_write_application_keypad_enter_from_widget_event() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b=");
    let event = TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::NumpadEnter,
        modifiers: TerminalModifiers::default(),
    });

    // Act
    terminal.handle_event(event).unwrap();

    // Assert
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1bOM");
}

#[test]
fn should_ignore_unsuitable_widget_events() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    let events = [
        TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
            key: TerminalKey::Character('x'),
            modifiers: TerminalModifiers::default(),
        }),
        TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(String::new())),
        TerminalEvent::Focus(TerminalFocusEvent::Lost),
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            1,
        )),
    ];

    // Act
    for event in events {
        terminal.handle_event(event).unwrap();
    }

    // Assert
    assert!(written.lock().unwrap().is_empty());
}

#[test]
fn should_propagate_writer_errors_for_encodable_widget_events() {
    // Arrange
    let mut terminal = Terminal::new_headless_with_io(
        2,
        8,
        ScriptedReader {
            chunks: std::collections::VecDeque::new(),
        },
        FailingWriter,
        || true,
    );
    let event = TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::Character('x'),
        modifiers: TerminalModifiers::default(),
    });

    // Act
    let result = terminal.handle_event(event);

    // Assert
    assert!(result.is_err());
}

#[test]
fn focus_loss_keeps_pointer_cleanup_and_restores_output_scrollback_snap() {
    let mut terminal = Terminal::new_headless(2, 8);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("selection");
    let pressed = terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            7,
        )))
        .unwrap();
    assert_eq!(pressed.capture_pointer, Some(7));
    let dragged = terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (25.0, 1.0),
            TerminalPointerPhase::Move,
            TerminalPointerButton::Left,
            7,
        )))
        .unwrap();
    assert!(dragged.redraw);

    let lost = terminal
        .handle_event_with_outcome(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();

    assert_eq!(lost.release_pointer, Some(7));
    assert!(lost.redraw);
    for line in 0..8 {
        terminal.put_str(&format!("line {line}\r\n"));
    }
    terminal.scroll_viewport_up(2);
    assert!(terminal.screen().view_offset() > 0);
    terminal.process_output(b"new output\r\n");
    assert_eq!(
        terminal.screen().view_offset(),
        0,
        "focus loss must clear local pointer scroll-snap suppression"
    );
}

#[test]
fn focus_reporting_propagates_writer_errors() {
    let mut terminal = Terminal::new_headless_with_io(
        2,
        8,
        ScriptedReader {
            chunks: std::collections::VecDeque::new(),
        },
        FailingWriter,
        || true,
    );
    terminal.process_output(b"\x1b[?1004h");

    let result = terminal.handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained));

    assert!(result.is_err());
}

#[test]
fn pty_queue_is_bounded_and_wakes_for_remaining_batches() {
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = BurstReader {
        remaining: PTY_QUEUE_CAPACITY + 1,
        reads: std::sync::Arc::clone(&reads),
        completed: std::sync::Arc::clone(&completed),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);

    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("first queued chunk must wake the UI");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while reads.load(std::sync::atomic::Ordering::SeqCst) < PTY_QUEUE_CAPACITY + 1 {
        assert!(
            std::time::Instant::now() < deadline,
            "reader did not reach the send that must wait for bounded queue capacity"
        );
        std::thread::yield_now();
    }
    assert!(
        wake_rx
            .recv_timeout(std::time::Duration::from_millis(20))
            .is_err(),
        "a full burst must not post a wake per chunk"
    );

    assert!(terminal.drain_pty());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !completed.load(std::sync::atomic::Ordering::SeqCst) {
        assert!(
            std::time::Instant::now() < deadline,
            "reader did not resume after output was drained"
        );
        std::thread::yield_now();
    }
    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("budget exhaustion must request a continuation");
    // A batch is bounded, so the remaining FIFO may take several drains.
    for _ in 0..PTY_QUEUE_CAPACITY {
        if !terminal.drain_pty() {
            return;
        }
    }
    panic!("finite burst did not drain");
}

#[test]
fn should_wake_once_without_enqueuing_when_reader_exits_after_read_error() {
    // Arrange
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let reader = ErrorReader {
        started: started_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);

    // Act
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should make its initial read");
    wait_for_pty_wake(&wake_rx);

    // Assert — close observation wakes once; the failed read queued no bytes
    assert!(wake_rx.try_recv().is_err());
    assert!(!terminal.drain_pty());
}

#[test]
fn terminal_input_returns_scrollback_to_live_viewport() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for line in 0..8 {
        terminal.process_output(format!("line {line}\r\n").as_bytes());
    }
    terminal.scroll_viewport_up(3);
    assert!(terminal.screen().view_offset() > 0);

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Character('x'),
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();

    assert_eq!(terminal.screen().view_offset(), 0);
    assert_eq!(written.lock().unwrap().as_slice(), b"x");
}
