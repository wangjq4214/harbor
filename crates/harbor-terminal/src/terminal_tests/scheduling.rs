use super::*;

#[test]
fn drain_and_snapshot_observes_queued_bracketed_paste_mode() {
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?2004h".to_vec()]),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("mode update should wake the UI");
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("mode update should finish reading");

    assert!(terminal.drain_and_snapshot().input_modes.bracketed_paste);
}

#[test]
fn should_return_empty_frame_demand_when_headless() {
    // Arrange
    let mut terminal = Terminal::new_headless(3, 3);
    let now = Instant::now();

    // Act
    let demand = terminal.frame_demand(now);

    // Assert
    assert!(!demand.redraw_now);
    assert!(demand.deadline.is_some());
    assert!(demand.ordinary_present_eligible);
}

#[test]
fn should_keep_empty_demand_after_cursor_move_when_headless() {
    // Arrange — CUP moves the cursor and resets engine-owned blink timing.
    let mut terminal = Terminal::new_headless(5, 10);
    let before = (terminal.screen().cursor_x(), terminal.screen().cursor_y());
    assert_eq!(before, (0, 0));

    // Act
    terminal.put_bytes(b"\x1b[3;4H");
    let after = (terminal.screen().cursor_x(), terminal.screen().cursor_y());
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert_eq!(after, (3, 2));
    assert_ne!(before, after);
    assert!(demand.redraw_now);
    assert!(demand.deadline.is_some());
}

#[test]
fn should_keep_empty_demand_after_non_moving_print_when_headless() {
    // Arrange — SGR alone does not move the cursor (would not reset blink)
    let mut terminal = Terminal::new_headless(3, 8);
    let before = (terminal.screen().cursor_x(), terminal.screen().cursor_y());

    // Act
    terminal.put_bytes(b"\x1b[31m");
    let after = (terminal.screen().cursor_x(), terminal.screen().cursor_y());
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert_eq!(before, after);
    assert!(!demand.redraw_now);
    assert!(demand.deadline.is_some());
}

#[test]
fn should_keep_empty_demand_after_input_write_when_headless() {
    // Arrange — KeyDown writes PTY input (would reset blink when a Cursor exists)
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);

    // Act
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Character('a'),
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert_eq!(written.lock().unwrap().as_slice(), b"a");
    assert!(demand.redraw_now);
    assert!(demand.deadline.is_some());
}

#[test]
fn should_ingest_queued_output_and_defer_present_when_2026_is_enabled() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?2026hhello".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("hello"));
    assert!(!demand.ordinary_present_eligible);
    assert!(!demand.redraw_now);
}

#[test]
fn should_set_redraw_now_when_matching_2026_disable_is_ingested() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?2026l".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026hhello");
    wait_for_pty_wake(&wake_rx);

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("hello"));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}

#[test]
fn should_complete_batch_in_one_ingest_when_enable_and_disable_share_a_chunk() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?2026hhello\x1b[?2026l".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("hello"));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}

#[test]
fn should_not_set_spurious_redraw_now_when_demand_is_polled_with_empty_queue() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"hello".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);
    let _ = terminal.frame_demand(Instant::now());
    let prepared = terminal.read_update(Instant::now());
    assert!(terminal.acknowledge_update(&prepared));

    // Act
    let idle = terminal.frame_demand(Instant::now());

    // Assert
    assert!(!idle.redraw_now);
    assert!(idle.ordinary_present_eligible);
}

#[test]
fn should_set_redraw_now_when_printable_follows_extra_disables() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"x".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026l\x1b[?2026l");
    wait_for_pty_wake(&wake_rx);

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains('x'));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}

#[test]
fn should_stay_ineligible_when_nested_disable_is_ingested() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"cd\x1b[?2026l".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026h\x1b[?2026hab");
    wait_for_pty_wake(&wake_rx);

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("abcd"));
    assert!(!demand.ordinary_present_eligible);
    assert!(!demand.redraw_now);
}

#[test]
fn should_set_redraw_now_when_final_nested_disable_is_ingested() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?2026l".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026h\x1b[?2026hab\x1b[?2026lcd");
    wait_for_pty_wake(&wake_rx);

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("abcd"));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}

#[test]
fn should_set_redraw_now_when_input_drain_consumes_matching_2026_disable() {
    // Arrange — input handling drains queued output before the next demand poll
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?2026l".to_vec()]),
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026hhello");
    wait_for_pty_wake(&wake_rx);

    // Act
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Character('a'),
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("hello"));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}

#[test]
fn should_set_redraw_now_when_prior_ingest_released_eligibility_and_this_drain_is_empty() {
    // Arrange — matching disable is applied before demand is polled; this drain has nothing queued
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.process_output(b"\x1b[?2026hhello");
    terminal.process_output(b"\x1b[?2026l");

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("hello"));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}

#[test]
fn should_not_set_redraw_now_when_demand_is_repolled_after_release_notify() {
    // Arrange
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.process_output(b"\x1b[?2026hhello");
    terminal.process_output(b"\x1b[?2026l");
    let _ = terminal.frame_demand(Instant::now());

    let prepared = terminal.read_update(Instant::now());
    assert!(terminal.acknowledge_update(&prepared));
    // Act
    let idle = terminal.frame_demand(Instant::now());

    // Assert
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.redraw_now);
}

#[test]
fn should_keep_decrqm_set_when_nested_batch_would_live_commit() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.process_output(b"\x1b[?2026h\x1b[?2026hhello");

    assert!(!terminal.screen().ordinary_present_eligible());
    assert_eq!(
        terminal.screen().mode_status(true, 2026),
        crate::screen::ModeStatus::Set
    );

    terminal.process_output(b"more");

    assert!(!terminal.screen().ordinary_present_eligible());
    assert_eq!(
        terminal.screen().mode_status(true, 2026),
        crate::screen::ModeStatus::Set
    );
    assert!(terminal.row_text(0).contains("hellomore"));
}

#[test]
fn should_set_redraw_now_when_ris_clears_nested_2026() {
    // Arrange
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.process_output(b"\x1b[?2026h\x1b[?2026hhello");

    // Act
    terminal.process_output(b"\x1bc");
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
    assert_eq!(
        terminal.screen().mode_status(true, 2026),
        crate::screen::ModeStatus::Reset
    );
}

#[test]
fn should_stay_ineligible_when_decstr_leaves_nested_2026() {
    // Arrange
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.process_output(b"\x1b[?2026hhello");

    // Act
    terminal.process_output(b"\x1b[!p");
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(!demand.ordinary_present_eligible);
    assert!(!demand.redraw_now);
    assert_eq!(
        terminal.screen().mode_status(true, 2026),
        crate::screen::ModeStatus::Set
    );
    assert!(terminal.row_text(0).contains("hello"));
}

#[test]
fn should_set_redraw_now_when_pty_eof_clears_nested_2026() {
    // Arrange
    let (reader, _reads, started_rx, exited_rx) = eof_reader();
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026hhello");
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should attempt its initial read");
    wait_for_pty_wake(&wake_rx);
    wait_for_reader_exit(&exited_rx);

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("hello"));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}

#[test]
fn should_not_set_redraw_now_when_demand_is_repolled_after_pty_eof_release() {
    // Arrange
    let (reader, _reads, started_rx, exited_rx) = eof_reader();
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026hhello");
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should attempt its initial read");
    wait_for_pty_wake(&wake_rx);
    wait_for_reader_exit(&exited_rx);
    let _ = terminal.frame_demand(Instant::now());

    let prepared = terminal.read_update(Instant::now());
    assert!(terminal.acknowledge_update(&prepared));
    // Act
    let idle = terminal.frame_demand(Instant::now());

    // Assert
    assert!(idle.ordinary_present_eligible);
    assert!(!idle.redraw_now);
}

#[test]
fn should_stop_parked_reader_when_terminal_is_dropped() {
    // Arrange — keep-alive readers park after their last chunk; Drop must unpark
    let (parked_tx, parked_rx) = std::sync::mpsc::channel();
    let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
    struct ParkedKeepAliveReader {
        parked: std::sync::mpsc::Sender<()>,
        dropped: std::sync::mpsc::Sender<()>,
    }
    impl std::io::Read for ParkedKeepAliveReader {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            let _ = self.parked.send(());
            std::thread::park();
            Ok(0)
        }
    }
    impl Drop for ParkedKeepAliveReader {
        fn drop(&mut self) {
            let _ = self.dropped.send(());
        }
    }
    let (terminal, _written, _wake_rx) = terminal_with_io(ParkedKeepAliveReader {
        parked: parked_tx,
        dropped: dropped_tx,
    });
    parked_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should park while waiting for more output");

    // Act
    drop(terminal);

    // Assert
    dropped_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("Drop should unpark the keep-alive reader so it can exit");
}

#[test]
fn should_return_false_when_drain_observes_disconnect_without_chunks() {
    // Arrange
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::new(),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should exit after EOF");

    // Act
    let drained = terminal.drain_pty();

    // Assert
    assert!(!drained);
}

#[test]
fn should_not_set_redraw_now_when_pty_eof_arrives_while_eligible() {
    // Arrange
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::new(),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should exit after EOF");

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(demand.ordinary_present_eligible);
    assert!(!demand.redraw_now);
}

#[test]
fn should_keep_2026_set_when_reenabled_after_disconnect_was_observed() {
    // Arrange — close clears once; a later drain of the dead session must not reclear
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::new(),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?2026hhello");
    wait_for_pty_wake(&wake_rx);
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should exit after EOF");
    let released = terminal.frame_demand(Instant::now());
    assert!(released.ordinary_present_eligible);
    terminal.process_output(b"\x1b[?2026hz");

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(!demand.ordinary_present_eligible);
    assert!(terminal.row_text(0).contains('z'));
}

#[test]
fn should_restore_eligibility_when_last_chunk_enables_2026_then_eof() {
    // Arrange — reader exits after one chunk so drain observes leftover bytes + disconnect
    let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
    struct ChunkThenDisconnectReader {
        chunk: Option<Vec<u8>>,
        dropped: std::sync::mpsc::Sender<()>,
    }
    impl std::io::Read for ChunkThenDisconnectReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let Some(chunk) = self.chunk.take() else {
                return Ok(0);
            };
            buffer[..chunk.len()].copy_from_slice(&chunk);
            Ok(chunk.len())
        }
    }
    impl Drop for ChunkThenDisconnectReader {
        fn drop(&mut self) {
            let _ = self.dropped.send(());
        }
    }
    let reader = ChunkThenDisconnectReader {
        chunk: Some(b"\x1b[?2026hhello".to_vec()),
        dropped: dropped_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);
    dropped_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should exit after enqueueing the last chunk");

    // Act
    let demand = terminal.frame_demand(Instant::now());

    // Assert
    assert!(terminal.row_text(0).contains("hello"));
    assert!(demand.ordinary_present_eligible);
    assert!(demand.redraw_now);
}
