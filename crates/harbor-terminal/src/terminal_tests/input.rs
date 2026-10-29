use super::*;

#[test]
fn vt_mouse_release_consumes_pending_local_capture_after_alt_transition() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, _written, _wake_rx) = terminal_with_io(reader);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    let event = |phase| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            phase,
            TerminalPointerButton::Left,
            7,
        ))
    };
    assert_eq!(
        terminal
            .handle_event_with_outcome(event(TerminalPointerPhase::Down))
            .unwrap()
            .capture_pointer,
        Some(7)
    );
    terminal.process_output(b"\x1b[?1049h\x1b[?1000;1006h");
    assert_eq!(
        terminal
            .handle_event_with_outcome(event(TerminalPointerPhase::Up))
            .unwrap()
            .release_pointer,
        Some(7)
    );
    assert!(!terminal.pointer.has_active_pointer());
}

#[test]
fn legacy_tracking_reports_wheel_without_local_scroll_until_tracking_is_disabled() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    for line in 0..8 {
        terminal.process_output(format!("line {line}\r\n").as_bytes());
    }
    assert_eq!(terminal.screen().view_offset(), 0);

    terminal.process_output(b"\x1b[?1000h");
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            TerminalPointerButton::None,
            1,
        )))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 0);
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[M\x60\x21\x21");

    terminal.process_output(b"\x1b[?1000l");
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            TerminalPointerButton::None,
            1,
        )))
        .unwrap();
    assert!(terminal.screen().view_offset() > 0);
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[M\x60\x21\x21");
}

#[test]
fn terminal_mouse_filters_follow_effective_mode_fallback_and_cancel_state() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    let event = |phase, button, pointer_id| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            phase,
            button,
            pointer_id,
        ))
    };

    terminal.process_output(b"\x1b[?1000;1006h");
    terminal
        .handle_event(event(
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            1,
        ))
        .unwrap();
    terminal
        .handle_event(event(
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            1,
        ))
        .unwrap();
    terminal
        .handle_event(event(
            TerminalPointerPhase::Up,
            TerminalPointerButton::Left,
            1,
        ))
        .unwrap();
    terminal
        .handle_event(event(
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
            TerminalPointerButton::None,
            1,
        ))
        .unwrap();
    assert_eq!(
        written.lock().unwrap().as_slice(),
        b"\x1b[<0;1;1M\x1b[<0;1;1m\x1b[<65;1;1M"
    );
    written.lock().unwrap().clear();

    terminal.process_output(b"\x1b[?1002h");
    terminal
        .handle_event(event(
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            2,
        ))
        .unwrap();
    terminal
        .handle_event(event(
            TerminalPointerPhase::Down,
            TerminalPointerButton::Middle,
            2,
        ))
        .unwrap();
    terminal
        .handle_event(event(
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            2,
        ))
        .unwrap();
    terminal
        .handle_event(event(
            TerminalPointerPhase::Up,
            TerminalPointerButton::Middle,
            2,
        ))
        .unwrap();
    assert_eq!(
        written.lock().unwrap().as_slice(),
        b"\x1b[<1;1;1M\x1b[<33;1;1M\x1b[<1;1;1m"
    );
    written.lock().unwrap().clear();

    terminal.process_output(b"\x1b[?1003h");
    terminal
        .handle_event(event(
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            3,
        ))
        .unwrap();
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[<35;1;1M");
    written.lock().unwrap().clear();

    terminal.process_output(b"\x1b[?1003l");
    terminal
        .handle_event(event(
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            3,
        ))
        .unwrap();
    assert!(written.lock().unwrap().is_empty());
    terminal.process_output(b"\x1b[?1002l");
    terminal
        .handle_event(event(
            TerminalPointerPhase::Down,
            TerminalPointerButton::Right,
            4,
        ))
        .unwrap();
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[<2;1;1M");
    written.lock().unwrap().clear();

    terminal.process_output(b"\x1b[?1002h");
    let captured = terminal
        .handle_event_with_outcome(event(
            TerminalPointerPhase::Down,
            TerminalPointerButton::Right,
            9,
        ))
        .unwrap();
    assert_eq!(captured.capture_pointer, Some(9));
    let cancelled = terminal
        .handle_event_with_outcome(event(
            TerminalPointerPhase::Cancel,
            TerminalPointerButton::None,
            9,
        ))
        .unwrap();
    assert_eq!(cancelled.release_pointer, Some(9));
    terminal
        .handle_event(event(
            TerminalPointerPhase::Move,
            TerminalPointerButton::None,
            9,
        ))
        .unwrap();
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[<2;1;1M");
}

#[test]
fn preedit_clears_through_all_paths_without_mutating_terminal_cursor_state() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal.put_bytes(b"\x1b[2;3H\x1b[2 q\x1b[?25l");
    let cursor_state = |terminal: &Terminal| {
        let snap = terminal.snapshot();
        (
            snap.cursor_x,
            snap.cursor_y,
            snap.cursor_shape,
            snap.cursor_blink,
            snap.cursor_visible,
        )
    };
    let expected_cursor = cursor_state(&terminal);

    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("draft", None)))
        .unwrap();
    let cleared = terminal
        .handle_event_with_outcome(TerminalEvent::Preedit(Preedit::default()))
        .unwrap();
    assert!(cleared.redraw);
    assert!(terminal.preedit().is_none());
    assert_eq!(cursor_state(&terminal), expected_cursor);

    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("候補", None)))
        .unwrap();
    let committed = terminal
        .handle_event_with_outcome(TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(
            "候補".into(),
        )))
        .unwrap();
    assert!(committed.redraw);
    assert!(terminal.preedit().is_none());
    assert_eq!(cursor_state(&terminal), expected_cursor);
    assert_eq!(written.lock().unwrap().as_slice(), "候補".as_bytes());

    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("stale", None)))
        .unwrap();
    let lost = terminal
        .handle_event_with_outcome(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();
    assert!(lost.redraw);
    assert!(terminal.preedit().is_none());
    assert_eq!(cursor_state(&terminal), expected_cursor);

    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("tab switch", None)))
        .unwrap();
    assert!(terminal.clear_preedit());
    assert!(!terminal.clear_preedit());
    assert_eq!(cursor_state(&terminal), expected_cursor);
}

#[test]
fn first_non_empty_preedit_returns_scrollback_to_live_bottom_without_deleting_history() {
    let mut terminal = Terminal::new_headless(3, 8);
    for line in 0..12 {
        terminal.put_str(&format!("{line}\r\n"));
    }
    terminal.scroll_viewport_up(2);
    let before = terminal.snapshot();
    assert!(before.view_offset > 0);

    terminal
        .handle_event(TerminalEvent::Preedit(Preedit::new("x", None)))
        .unwrap();
    let after = terminal.snapshot();

    assert_eq!(after.view_offset, 0);
    assert_eq!(after.scroll_count, before.scroll_count);
    assert_eq!(after.history_start, before.history_start);
}

#[test]
fn focus_reporting_writes_only_enabled_real_transitions() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);

    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();
    terminal.process_output(b"\x1b[?1004h");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();
    assert!(written.lock().unwrap().is_empty());

    for event in [
        TerminalFocusEvent::Lost,
        TerminalFocusEvent::Lost,
        TerminalFocusEvent::Gained,
    ] {
        terminal.handle_event(TerminalEvent::Focus(event)).unwrap();
    }
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[O\x1b[I");

    terminal.process_output(b"\x1b[?1004l");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();
    terminal.process_output(b"\x1b[?1004h");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[O\x1b[I");
}

#[test]
fn ris_disables_focus_reporting_and_clears_the_observed_edge() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?1004h");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[O");

    terminal.process_output(b"\x1bc\x1b[?1004h");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        .unwrap();
    assert_eq!(
        written.lock().unwrap().as_slice(),
        b"\x1b[O\x1b[O",
        "RIS must clear the previously observed Lost edge"
    );

    terminal.process_output(b"\x1bc");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();
    terminal.process_output(b"\x1b[?1004h");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();
    assert_eq!(
        written.lock().unwrap().as_slice(),
        b"\x1b[O\x1b[O",
        "the disabled observation after RIS must become the new baseline"
    );
}

#[test]
fn focus_reporting_state_survives_parser_driven_alt_screen_families() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?1004h");
    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();

    for enter in [b"\x1b[?1049h".as_slice(), b"\x1b[?1047h", b"\x1b[?47h"] {
        terminal.process_output(enter);
        terminal
            .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
            .unwrap();
        terminal
            .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Lost))
            .unwrap();
        terminal.process_output(match enter {
            b"\x1b[?1049h" => b"\x1b[?1049l",
            b"\x1b[?1047h" => b"\x1b[?1047l",
            _ => b"\x1b[?47l",
        });
        terminal
            .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Lost))
            .unwrap();
        terminal
            .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
            .unwrap();
    }

    assert_eq!(
        written.lock().unwrap().as_slice(),
        b"\x1b[I\x1b[O\x1b[I\x1b[O\x1b[I\x1b[O\x1b[I"
    );
}

#[test]
fn focus_reporting_does_not_snap_scrollback_to_live_viewport() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?1004h");
    for line in 0..8 {
        terminal.process_output(format!("line {line}\r\n").as_bytes());
    }
    terminal.scroll_viewport_up(3);
    let offset = terminal.screen().view_offset();

    terminal
        .handle_event(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        .unwrap();

    assert_eq!(terminal.screen().view_offset(), offset);
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[I");
}

#[test]
fn direct_widget_input_observes_current_application_modes() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    terminal.process_output(b"\x1b[?1h\x1b=");

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::ArrowUp,
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::NumpadCharacter('1'),
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();

    assert_eq!(written.lock().unwrap().as_slice(), b"\x1bOA\x1bOq");
}

#[test]
fn should_write_modified_cursor_sequence_when_widget_event_has_shift() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    let event = TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::ArrowUp,
        modifiers: TerminalModifiers {
            shift: true,
            ..TerminalModifiers::default()
        },
    });

    // Act
    terminal.handle_event(event).unwrap();

    // Assert
    assert_eq!(written.lock().unwrap().as_slice(), b"\x1b[1;2A");
}
