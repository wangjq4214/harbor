use super::*;

#[test]
fn put_bytes_does_not_snap_viewport() {
    let mut terminal = Terminal::new_headless(5, 10);
    // Write enough lines to create scrollback.
    for _ in 0..6 {
        terminal.process_output(b"line\n");
    }
    // Scroll up, confirming we're scrolled back.
    terminal.scroll_viewport_up(2);
    assert!(
        terminal.screen().view_offset() > 0,
        "expected scrollback before put_bytes"
    );
    let offset_before = terminal.screen().view_offset();

    // `put_bytes` must NOT snap the viewport.
    terminal.put_bytes(b"data");
    assert_eq!(
        terminal.screen().view_offset(),
        offset_before,
        "put_bytes must not snap viewport to bottom"
    );
}

#[test]
fn process_output_snaps_viewport() {
    let mut terminal = Terminal::new_headless(5, 10);
    // Write enough lines to create scrollback.
    for _ in 0..6 {
        terminal.process_output(b"line\n");
    }
    // Scroll up, then call process_output — must snap to bottom.
    terminal.scroll_viewport_up(3);
    assert!(
        terminal.screen().view_offset() > 0,
        "expected scrollback before process_output"
    );
    terminal.process_output(b"more data\n");
    assert_eq!(
        terminal.screen().view_offset(),
        0,
        "process_output must snap viewport to bottom"
    );
}

#[test]
fn viewport_navigation_pages_and_reaches_top() {
    let mut terminal = Terminal::new_headless(4, 10);
    for _ in 0..10 {
        terminal.process_output(b"line\n");
    }

    let page_rows = terminal.screen().rows();
    let scroll_count = terminal.screen().scroll_count();
    assert!(
        scroll_count > page_rows,
        "test setup requires more than one page of scrollback"
    );

    terminal.scroll_viewport_up(page_rows);
    assert_eq!(terminal.screen().view_offset(), page_rows);

    terminal.scroll_viewport_down(page_rows);
    assert_eq!(terminal.screen().view_offset(), 0);

    terminal.scroll_viewport_to_top();
    assert_eq!(terminal.screen().view_offset(), scroll_count);

    terminal.scroll_viewport_down(page_rows);
    assert_eq!(
        terminal.screen().view_offset(),
        scroll_count - page_rows,
        "PageDown must move one viewport height toward live content"
    );

    terminal.scroll_viewport_to_bottom();
    assert_eq!(terminal.screen().view_offset(), 0);
}

#[test]
fn ctrl_shift_c_is_encoded_after_copy_migration() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);

    let outcome = terminal
        .handle_event_with_outcome(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Character('C'),
            modifiers: TerminalModifiers {
                ctrl: true,
                shift: true,
                ..TerminalModifiers::default()
            },
        }))
        .expect("terminal input should be encoded");

    assert_eq!(outcome.clipboard_text, None);
    assert_eq!(written.lock().unwrap().as_slice(), b"\x03");
}

#[test]
fn copy_selection_returns_text_and_clears_completed_selection() {
    let mut terminal = Terminal::new_headless(1, 10);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("selection");
    for phase in [
        TerminalPointerPhase::Down,
        TerminalPointerPhase::Move,
        TerminalPointerPhase::Up,
    ] {
        terminal
            .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                if matches!(phase, TerminalPointerPhase::Down) {
                    (1.0, 1.0)
                } else {
                    (21.0, 1.0)
                },
                phase,
                TerminalPointerButton::Left,
                80,
            )))
            .unwrap();
    }
    assert_eq!(terminal.selection_text(), "sel");

    let outcome = terminal.command_copy_selection();

    assert_eq!(outcome.clipboard_text.as_deref(), Some("sel"));
    assert!(outcome.release_pointer.is_none());
    assert!(outcome.redraw);
    assert!(!terminal.pointer.has_selection_state());
}

#[test]
fn copy_selection_returns_text_clears_highlight_and_releases_pointer() {
    let mut terminal = Terminal::new_headless(1, 10);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("selection");
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            81,
        )))
        .unwrap();
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (21.0, 1.0),
            TerminalPointerPhase::Move,
            TerminalPointerButton::Left,
            81,
        )))
        .unwrap();
    assert_eq!(terminal.selection_text(), "sel");

    let outcome = terminal.command_copy_selection();

    assert_eq!(outcome.clipboard_text.as_deref(), Some("sel"));
    assert_eq!(outcome.release_pointer, Some(81));
    assert!(outcome.redraw);
    assert!(!terminal.pointer.has_selection_state());
    assert_eq!(terminal.selection_text(), "");
}

#[test]
fn copy_selection_preserves_empty_copy_without_selection() {
    let mut terminal = Terminal::new_headless(1, 10);

    let outcome = terminal.command_copy_selection();

    assert_eq!(outcome.clipboard_text.as_deref(), Some(""));
    assert!(!outcome.redraw);
    assert!(outcome.release_pointer.is_none());
    assert!(!terminal.pointer.has_selection_state());
}

#[test]
fn bare_navigation_keys_encode_after_scroll_migration() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..12 {
        terminal.process_output(b"line\r\n");
    }
    let before = terminal.screen().view_offset();

    for key in [
        TerminalKey::PageUp,
        TerminalKey::PageDown,
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

    assert_eq!(terminal.screen().view_offset(), before);
    assert!(!written.lock().unwrap().is_empty());
}

#[test]
fn explicit_primary_scroll_commands_report_visible_changes() {
    let mut terminal = Terminal::new_headless(3, 8);
    for _ in 0..12 {
        terminal.process_output(b"line\r\n");
    }

    assert!(terminal.command_page_up().redraw);
    assert!(terminal.screen().view_offset() > 0);
    assert!(terminal.command_scroll_to_top().redraw);
    assert!(!terminal.command_scroll_to_top().redraw);
    assert!(terminal.command_page_down().redraw);
    assert!(terminal.command_scroll_to_bottom().redraw);
    assert!(!terminal.command_scroll_to_bottom().redraw);

    terminal.process_output(b"\x1b[?1049h");
    assert!(terminal.is_alt_screen());
    assert!(!terminal.command_page_up().redraw);
}

#[test]
fn explicit_scroll_command_clears_selection_even_at_viewport_boundary() {
    let mut terminal = Terminal::new_headless(3, 8);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("selection");
    terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            TerminalPointerPhase::Down,
            TerminalPointerButton::Left,
            41,
        )))
        .unwrap();
    terminal
        .handle_event_with_outcome(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (25.0, 1.0),
            TerminalPointerPhase::Move,
            TerminalPointerButton::Left,
            41,
        )))
        .unwrap();
    assert!(terminal.has_non_empty_selection());

    let outcome = terminal.command_page_down();

    assert!(outcome.redraw);
    assert_eq!(outcome.release_pointer, Some(41));
    assert!(!terminal.has_non_empty_selection());
}

#[test]
fn parser_insert_before_selection_reprojects_canonical_endpoints() {
    let mut terminal = Terminal::new_headless(1, 10);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abcdefgh");
    let now = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            71,
        ))
    };
    terminal
        .handle_event(now(TerminalPointerPhase::Down, 41.0))
        .unwrap();
    terminal
        .handle_event(now(TerminalPointerPhase::Move, 61.0))
        .unwrap();
    terminal
        .handle_event(now(TerminalPointerPhase::Up, 61.0))
        .unwrap();
    assert_eq!(terminal.selection_text(), "efg");

    terminal.put_bytes(b"\x1b[1;3H\x1b[4hX\x1b[4l");

    assert_eq!(terminal.selection_text(), "efg");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 5,
            end_row: 0,
            end_col: 7,
        })
    );

    terminal.put_bytes(b"\x1b[1;3H\x1b[P");
    assert_eq!(terminal.selection_text(), "efg");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 4,
            end_row: 0,
            end_col: 6,
        })
    );

    terminal.put_bytes(b"\x1b[1;2HZ");
    assert_eq!(terminal.selection_text(), "efg");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 4,
            end_row: 0,
            end_col: 6,
        })
    );
}

#[test]
fn replacing_one_selected_logical_line_clears_the_whole_selection() {
    let mut terminal = Terminal::new_headless(1, 10);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abcdefgh");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            72,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 11.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 31.0))
        .unwrap();
    assert!(terminal.has_non_empty_selection());

    terminal.put_bytes(b"\r\x1b[2K");

    assert!(!terminal.has_non_empty_selection());
    assert_eq!(terminal.selection_text(), "");
}

#[test]
fn trailing_blank_selection_preserves_columns_when_its_row_moves() {
    let mut terminal = Terminal::new_headless(2, 10);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abc");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            69,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 51.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 61.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 61.0))
        .unwrap();

    terminal.put_bytes(b"\x1b[1;1H\x1b[L");

    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 1,
            start_col: 5,
            end_row: 1,
            end_col: 6,
        })
    );
}

#[test]
fn trailing_blank_selection_keeps_its_physical_projection_after_unrelated_ingest() {
    let mut terminal = Terminal::new_headless(1, 10);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abc");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            73,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 51.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 61.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 61.0))
        .unwrap();
    let expected = terminal.pointer.bounds();

    terminal.put_bytes(b"\x1b[1;1H");

    assert_eq!(terminal.pointer.bounds(), expected);
    assert_eq!(terminal.selection_text(), "");
}

#[test]
fn repeated_content_insert_uses_semantic_transform_not_payload_diffing() {
    let mut terminal = Terminal::new_headless(1, 5);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("aaaaB");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            74,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 11.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 31.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 31.0))
        .unwrap();

    terminal.put_bytes(b"\x1b[1;1H\x1b[4ha\x1b[4l");

    assert_eq!(terminal.selection_text(), "aaa");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 2,
            end_row: 0,
            end_col: 4,
        })
    );
}

#[test]
fn equal_boundary_insertion_is_drag_direction_independent() {
    fn selected(reverse: bool, pointer_id: u64) -> Terminal {
        let mut terminal = Terminal::new_headless(1, 10);
        terminal
            .pointer
            .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
        terminal.put_str("abcdefgh");
        let (start, end) = if reverse { (61.0, 41.0) } else { (41.0, 61.0) };
        let event = |phase, x| {
            TerminalEvent::Pointer(TerminalPointerEvent::new(
                (x, 1.0),
                phase,
                TerminalPointerButton::Left,
                pointer_id,
            ))
        };
        terminal
            .handle_event(event(TerminalPointerPhase::Down, start))
            .unwrap();
        terminal
            .handle_event(event(TerminalPointerPhase::Move, end))
            .unwrap();
        terminal
            .handle_event(event(TerminalPointerPhase::Up, end))
            .unwrap();
        terminal.put_bytes(b"\x1b[1;5H\x1b[4hX\x1b[4l");
        terminal
    }

    let forward = selected(false, 75);
    let reverse = selected(true, 76);
    assert_eq!(forward.pointer.bounds(), reverse.pointer.bounds());
    assert_eq!(forward.selection_text(), reverse.selection_text());
    assert_eq!(forward.selection_text(), "Xefg");
}

#[test]
fn explicit_scroll_command_observes_queued_alt_screen_transition() {
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?1049h".to_vec()]),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wait_for_pty_wake(&wake_rx);
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("alternate-screen transition should finish reading");

    let outcome = terminal.command_page_up();

    assert!(terminal.is_alt_screen());
    assert!(!outcome.redraw);
}

#[test]
fn modified_or_alt_screen_navigation_encodes_to_pty() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..8 {
        terminal.process_output(b"line\r\n");
    }

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::PageUp,
            modifiers: TerminalModifiers {
                shift: true,
                ..TerminalModifiers::default()
            },
        }))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 0);
    assert!(!written.lock().unwrap().is_empty());
    written.lock().unwrap().clear();

    terminal.process_output(b"\x1b[?1049h");
    assert!(terminal.is_alt_screen());
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Home,
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    assert!(!written.lock().unwrap().is_empty());
}

#[test]
fn rectangular_alt_resize_preserves_retained_soft_wrapped_selection_identity() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_bytes(b"\x1b[?47h");
    terminal.put_str("abcdef");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 21.0),
            phase,
            TerminalPointerButton::Left,
            67,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 1.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 11.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 11.0))
        .unwrap();
    assert_eq!(terminal.selection_text(), "ef");

    assert!(terminal.resize_if_changed(TerminalSize { rows: 2, cols: 3 }));

    assert_eq!(terminal.selection_text(), "ef");
}

#[test]
fn rectangular_alt_resize_clears_selection_when_one_endpoint_is_clipped() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_bytes(b"\x1b[?47h");
    terminal.put_str("abcdef");
    let event = |phase, position| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            position,
            phase,
            TerminalPointerButton::Left,
            68,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, (31.0, 1.0)))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, (1.0, 21.0)))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, (1.0, 21.0)))
        .unwrap();
    assert!(terminal.has_non_empty_selection());

    assert!(terminal.resize_if_changed(TerminalSize { rows: 2, cols: 3 }));

    assert_eq!(terminal.selection_text(), "");
}

#[test]
fn ris_drops_parked_alternate_selection_before_fresh_reentry() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_bytes(b"\x1b[?47h");
    terminal.put_str("old");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            66,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 1.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 21.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 21.0))
        .unwrap();
    assert_eq!(terminal.selection_text(), "old");
    terminal.put_bytes(b"\x1b[?47l");
    terminal.put_bytes(b"\x1bc");
    terminal.put_bytes(b"\x1b[?47h");

    assert_eq!(terminal.selection_text(), "");
}

#[test]
fn primary_selection_is_parked_reflowed_and_restored_across_alt_resize() {
    let mut terminal = Terminal::new_headless(2, 8);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("primary");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            69,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 1.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 21.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 21.0))
        .unwrap();
    assert_eq!(terminal.selection_text(), "pri");

    terminal.put_bytes(b"\x1b[?47h");
    assert!(terminal.is_alt_screen());
    assert_eq!(terminal.selection_text(), "");
    terminal.put_str("ALT");
    assert!(terminal.resize_if_changed(TerminalSize { rows: 2, cols: 4 }));
    terminal.put_bytes(b"\x1b[?47l");

    assert!(!terminal.is_alt_screen());
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 4));
    assert_eq!(terminal.selection_text(), "pri");
}

#[test]
fn conpty_deferred_primary_resize_preserves_selection_until_alt_exit() {
    for final_cols in [6, 12] {
        let mut terminal = Terminal::new_headless(3, 12);
        terminal
            .pointer
            .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
        terminal.put_str("abcdefghijkl\r\n>");
        for (phase, x) in [
            (TerminalPointerPhase::Down, 1.0),
            (TerminalPointerPhase::Move, 111.0),
            (TerminalPointerPhase::Up, 111.0),
        ] {
            terminal
                .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                    (x, 1.0),
                    phase,
                    TerminalPointerButton::Left,
                    69,
                )))
                .unwrap();
        }
        assert_eq!(terminal.selection_text(), "abcdefghijkl");
        terminal.put_bytes(b"\x1b[?47h");
        for cols in [5, 2, 9, final_cols] {
            let prepared = terminal
                .screen
                .prepare_resize_with_viewport(
                    3,
                    cols,
                    crate::primary_reflow::ReflowViewport::PreserveLiveTop,
                )
                .unwrap();
            let pointer = terminal.pointer.prepare_resize(&prepared);
            terminal.screen.commit_resize(prepared);
            terminal.pointer.commit_resize(pointer);
        }
        terminal.put_bytes(b"\x1b[?47l");
        assert_eq!(terminal.screen.cols(), final_cols);
        assert_eq!(terminal.selection_text(), "abcdefghijkl");
    }
}

#[test]
fn default_blank_insert_reprojects_selection_to_shifted_source_cells() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abcde");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            70,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 21.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 31.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 31.0))
        .unwrap();

    terminal.put_bytes(b"\x1b[1;2H\x1b[@");

    assert_eq!(terminal.selection_text(), "cd");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 3,
            end_row: 0,
            end_col: 4,
        })
    );
}

#[test]
fn margin_insert_drops_margin_tail_without_aliasing_fixed_exterior_selection() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("ABCDEFGH");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            71,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 61.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 71.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 71.0))
        .unwrap();

    terminal.put_bytes(b"\x1b[?69h\x1b[3;6s\x1b[1;3H\x1b[@");

    assert_eq!(terminal.selection_text(), "GH");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 6,
            end_row: 0,
            end_col: 7,
        })
    );
}

#[test]
fn output_between_pointer_down_and_up_keeps_click_zero_width() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abcdef");
    let down = TerminalEvent::Pointer(TerminalPointerEvent::new(
        (21.0, 1.0),
        TerminalPointerPhase::Down,
        TerminalPointerButton::Left,
        72,
    ));
    terminal.handle_event(down).unwrap();

    terminal.put_bytes(b"\x1b[1;3H\x1b[4hX\x1b[4l");
    let up = TerminalEvent::Pointer(TerminalPointerEvent::new(
        (21.0, 1.0),
        TerminalPointerPhase::Up,
        TerminalPointerButton::Left,
        72,
    ));
    terminal.handle_event(up).unwrap();

    assert!(!terminal.has_non_empty_selection());
    assert_eq!(terminal.selection_text(), "");
}

#[test]
fn wheel_line_and_pixel_convert_to_viewport_lines() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..12 {
        terminal.process_output(b"line\r\n");
    }

    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 3);

    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelPixel { dx: 0.0, dy: 40.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 5);
    assert!(written.lock().unwrap().is_empty());
}

#[test]
fn wheel_on_alt_screen_or_zero_delta_is_consumed_without_pty_write() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..8 {
        terminal.process_output(b"line\r\n");
    }

    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelPixel { dx: 0.0, dy: 10.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 0);
    assert!(written.lock().unwrap().is_empty());

    terminal.process_output(b"\x1b[?1049h");
    let offset_before = terminal.screen().view_offset();
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 2.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), offset_before);
    assert!(written.lock().unwrap().is_empty());
}

#[test]
fn should_leave_view_offset_unchanged_when_wheel_hits_scroll_bound() {
    // Arrange — live bottom (offset 0); further scroll-down must not move
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..12 {
        terminal.process_output(b"line\r\n");
    }
    assert_eq!(terminal.screen().view_offset(), 0);

    // Act
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();

    // Assert — clamped: Host would skip redraw wake
    assert_eq!(terminal.screen().view_offset(), 0);
    assert!(written.lock().unwrap().is_empty());

    // Arrange — scroll to top, then wheel further up
    let scroll_count = terminal.screen().scroll_count();
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 40.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();
    let at_top = terminal.screen().view_offset();
    assert_eq!(at_top, scroll_count);

    // Act
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();

    // Assert
    assert_eq!(terminal.screen().view_offset(), at_top);
    assert!(written.lock().unwrap().is_empty());
}

#[test]
fn should_scroll_viewport_down_when_wheel_dy_is_negative() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..12 {
        terminal.process_output(b"line\r\n");
    }
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: 2.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 6);

    // Act — line delta -1 → 3 rows down; pixel -20 → 1 row down
    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 3);

    terminal
        .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
            (0.0, 0.0),
            TerminalPointerPhase::WheelPixel { dx: 0.0, dy: -20.0 },
            TerminalPointerButton::Left,
            0,
        )))
        .unwrap();

    // Assert
    assert_eq!(terminal.screen().view_offset(), 2);
    assert!(written.lock().unwrap().is_empty());
}

#[test]
fn should_encode_navigation_to_pty_when_ctrl_or_alt_modifier_set() {
    // Arrange
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..8 {
        terminal.process_output(b"line\r\n");
    }

    // Act / Assert — ctrl PageUp encodes, does not scroll
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::PageUp,
            modifiers: TerminalModifiers {
                ctrl: true,
                ..TerminalModifiers::default()
            },
        }))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 0);
    assert!(!written.lock().unwrap().is_empty());
    written.lock().unwrap().clear();

    // Act / Assert — alt PageDown encodes, does not scroll
    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::PageDown,
            modifiers: TerminalModifiers {
                alt: true,
                ..TerminalModifiers::default()
            },
        }))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 0);
    assert!(!written.lock().unwrap().is_empty());
}

#[test]
fn queued_output_updates_modes_before_input_encoding() {
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::from([b"\x1b[?1h\x1b=".to_vec()]),
        completed: completed_tx,
    };
    let (mut terminal, written, wake_rx) = terminal_with_io(reader);
    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("mode update should wake the UI");
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("mode update should finish reading");

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
fn irm_wide_wrap_does_not_mutate_anchors_on_the_source_row() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abcd");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            67,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 21.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 31.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 31.0))
        .unwrap();

    terminal.put_bytes(b"\x1b[1;4H\x1b[4h");
    terminal.put_str("界");
    terminal.put_bytes(b"\x1b[4l");

    assert_eq!(terminal.selection_text(), "cd");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 2,
            end_row: 0,
            end_col: 3,
        })
    );
}

#[test]
fn width_changing_overwrite_preserves_selection_of_later_content() {
    let mut terminal = Terminal::new_headless(1, 5);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("界xy");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            68,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 21.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 31.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 31.0))
        .unwrap();

    terminal.put_bytes(b"\x1b[1;1Hab");

    assert_eq!(terminal.selection_text(), "xy");
    assert_eq!(
        terminal.pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 2,
            end_row: 0,
            end_col: 3,
        })
    );
}

#[test]
fn should_keep_snapshot_non_draining_while_drain_and_snapshot_is_fresh() {
    // Arrange
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let reader = CompletedScriptedReader {
        chunks: std::collections::VecDeque::from([b"x".to_vec()]),
        completed: completed_tx,
    };
    let (mut terminal, _written, wake_rx) = terminal_with_io(reader);
    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("queued output should wake the UI");
    completed_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should finish after queuing output");

    // Act
    let cached = terminal.snapshot();
    let fresh = terminal.drain_and_snapshot();

    // Assert
    assert_eq!(cached.cells[0].ch, ' ');
    assert_eq!(fresh.cells[0].ch, 'x');
}
