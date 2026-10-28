use super::*;

#[test]
fn padded_table_rows_do_not_gain_empty_continuations_on_primary_resize() {
    let mut terminal = Terminal::new_headless(6, 8);
    terminal.put_str("a       \r\nb       \r\nc");
    assert_eq!(terminal.row_text(1), "b       ");
    let before = terminal.screen();
    let before_top = before.history_start() + before.scroll_count() as u64;
    assert_eq!(
        before.selected_text(crate::SelectionBounds {
            start_row: before_top,
            start_col: 0,
            end_row: before_top,
            end_col: 7,
        }),
        "a       "
    );
    terminal.resize(6, 4);
    assert_eq!(terminal.row_text(0), "a   ");
    assert_eq!(terminal.row_text(1), "b   ");
    assert_eq!(terminal.row_text(2), "c   ");
    let screen = terminal.screen();
    let top = screen.history_start() + screen.scroll_count() as u64;
    assert_eq!(
        screen.selected_text(crate::SelectionBounds {
            start_row: top,
            start_col: 0,
            end_row: top + 2,
            end_col: 3,
        }),
        "a\nb\nc"
    );
    terminal.resize(6, 8);
    assert_eq!(terminal.row_text(0), "a       ");
    assert_eq!(terminal.row_text(1), "b       ");
    assert_eq!(terminal.row_text(2), "c       ");
    let screen = terminal.screen();
    let top = screen.history_start() + screen.scroll_count() as u64;
    assert_eq!(
        screen.selected_text(crate::SelectionBounds {
            start_row: top,
            start_col: 0,
            end_row: top + 2,
            end_col: 7,
        }),
        "a\nb\nc",
        "discarded ordinary tails must not reappear on widening"
    );
}

#[test]
fn tail_space_cursor_keeps_original_insertion_after_resize() {
    let mut terminal = Terminal::new_headless(4, 6);
    terminal.put_str("x     ");
    terminal.resize(4, 3);
    assert_eq!(terminal.row_text(0), "x  ");
    terminal.put_str("Z");
    let screen = terminal.screen();
    let top = screen.history_start() + screen.scroll_count() as u64;
    let text = screen.selected_text(crate::SelectionBounds {
        start_row: top,
        start_col: 0,
        end_row: top + screen.rows() as u64 - 1,
        end_col: screen.cols() - 1,
    });
    assert!(
        text.starts_with("x     Z"),
        "cursor must append after retained spaces: {text:?}"
    );
}

#[test]
fn resize_trims_only_logical_tail_not_interior_spaces_or_each_wrapped_row() {
    let mut terminal = Terminal::new_headless(5, 10);
    terminal.put_str("a   b    \r\nc");
    terminal.resize(5, 3);
    assert_eq!(terminal.row_text(0), "a  ");
    assert_eq!(terminal.row_text(1), " b ");
    assert_eq!(terminal.row_text(2), "c  ");
    let screen = terminal.screen();
    let top = screen.history_start() + screen.scroll_count() as u64;
    assert_eq!(
        screen.selected_text(crate::SelectionBounds {
            start_row: top,
            start_col: 0,
            end_row: top + 2,
            end_col: 2,
        }),
        "a   b\nc"
    );
}

#[test]
fn saved_cursor_inside_tail_retains_insertion_but_trims_later_spaces() {
    let mut terminal = Terminal::new_headless(6, 8);
    terminal.put_str("a  \x1b7  \r\nb");
    terminal.resize(6, 2);
    assert_eq!(terminal.row_text(2), "b ");
    terminal.put_str("\x1b8Z");
    let screen = terminal.screen();
    let top = screen.history_start() + screen.scroll_count() as u64;
    assert_eq!(
        screen.selected_text(crate::SelectionBounds {
            start_row: top,
            start_col: 0,
            end_row: top + 1,
            end_col: 1,
        }),
        "a  Z"
    );
}

#[test]
fn protected_and_hyperlinked_tail_blanks_are_not_trimmed() {
    let mut terminal = Terminal::new_headless(7, 8);
    terminal.put_str("a \x1b[1\"q \x1b[0\"q  \r\nb");
    terminal.resize(7, 2);
    assert!(terminal.screen().cell(1, 0).protected);
    assert_eq!(terminal.row_text(2), "b ");

    let mut linked = Terminal::new_headless(5, 8);
    linked.put_str("a \x1b]8;;https://example.com\x1b\\ \x1b]8;;\x1b\\  \r\nb");
    linked.resize(5, 2);
    assert!(linked.screen().cell(1, 0).hyperlink.is_some());
    assert_eq!(linked.row_text(2), "b ");
}

#[test]
fn styled_tail_and_real_blank_line_still_take_rows_on_resize() {
    let mut terminal = Terminal::new_headless(6, 6);
    terminal.put_str("a \x1b[41m \x1b[0m   \r\n\r\nb");
    terminal.resize(6, 2);
    assert_eq!(terminal.row_text(0), "a ");
    assert_ne!(
        terminal.screen().cell(1, 0).bg,
        harbor_config::Color::Default
    );
    assert_eq!(
        terminal.row_text(2),
        "  ",
        "explicit empty line must survive"
    );
    assert_eq!(terminal.row_text(3), "b ");
}

#[test]
fn selection_of_discarded_tail_does_not_alias_other_rows_after_resize() {
    let mut terminal = Terminal::new_headless(4, 8);
    terminal.put_str("a       \r\nb");
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    for (phase, position) in [
        (TerminalPointerPhase::Down, (5.0, 10.0)),
        (TerminalPointerPhase::Move, (75.0, 10.0)),
        (TerminalPointerPhase::Up, (75.0, 10.0)),
    ] {
        terminal
            .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                position,
                phase,
                TerminalPointerButton::Left,
                8,
            )))
            .unwrap();
    }
    assert_eq!(terminal.selection_text(), "a       ");
    terminal.resize(4, 4);
    assert_eq!(terminal.row_text(1), "b   ");
    assert!(
        matches!(terminal.selection_text().as_str(), "" | "a"),
        "discarded tail selection must not claim lost spaces or select unrelated rows: {:?}",
        terminal.selection_text()
    );
}

#[test]
fn one_row_autowrap_keeps_scrolled_source_metadata() {
    let mut terminal = Terminal::new_headless(1, 4);

    terminal.put_str("abcd界ef");

    let screen = terminal.screen();
    let retained_rows = screen.scroll_count() + screen.rows();
    let text = screen.selected_text(crate::SelectionBounds {
        start_row: screen.history_start(),
        start_col: 0,
        end_row: screen.history_start() + retained_rows as u64 - 1,
        end_col: screen.cols() - 1,
    });
    assert_eq!(text, "abcd界ef");
}

#[test]
fn resize_preserves_scrollback_viewport() {
    let mut terminal = Terminal::new_headless(2, 4);
    for line in ["A", "B", "C", "D"] {
        terminal.process_output(format!("{line}\r\n").as_bytes());
    }
    terminal.scroll_viewport_up(1);
    let history_start = terminal.screen().history_start();

    terminal.resize(3, 6);

    assert_eq!(terminal.screen().view_offset(), 0);
    assert!(terminal.screen().scroll_count() > 0);
    assert!(terminal.screen().history_start() >= history_start);
}

#[test]
fn resize_zero_dimensions_uses_safe_terminal_size() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal.resize(0, 0);

    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 2));
}

#[test]
fn transactional_resize_preserves_state_on_pty_failure_and_retries() {
    let mut terminal = Terminal::new_headless(2, 4);
    let before = terminal.snapshot();
    let mut attempted_size = None;

    let failed = Terminal::try_resize_transaction(
        &mut terminal.screen,
        &mut terminal.pointer,
        &mut terminal.io,
        TerminalSize { rows: 0, cols: 1 },
        |_io, size| {
            attempted_size = Some(size);
            anyhow::bail!("injected PTY failure")
        },
    );

    assert!(failed.is_err());
    assert_eq!(attempted_size, Some(TerminalSize { rows: 1, cols: 2 }));
    assert_eq!(terminal.snapshot(), before);

    let changed = Terminal::try_resize_transaction(
        &mut terminal.screen,
        &mut terminal.pointer,
        &mut terminal.io,
        TerminalSize { rows: 0, cols: 1 },
        |_io, _size| Ok(()),
    )
    .unwrap();

    assert!(changed);
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 2));
}

#[test]
fn pty_failure_preserves_active_selection_state() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_str("abc");
    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            65,
        ))
    };
    terminal
        .handle_event(event(TerminalPointerPhase::Down, 11.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Move, 21.0))
        .unwrap();
    terminal
        .handle_event(event(TerminalPointerPhase::Up, 21.0))
        .unwrap();
    let selection_before = terminal.selection_text();

    let result = Terminal::try_resize_transaction(
        &mut terminal.screen,
        &mut terminal.pointer,
        &mut terminal.io,
        TerminalSize { rows: 1, cols: 4 },
        |_io, _size| anyhow::bail!("injected PTY failure"),
    );

    assert!(result.is_err());
    assert_eq!(terminal.selection_text(), selection_before);
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 8));
}

#[test]
fn transactional_resize_does_not_call_pty_when_preparation_fails() {
    let mut terminal = Terminal::new_headless(2, 4);
    let before = terminal.snapshot();
    let mut pty_called = false;

    let result = Terminal::try_resize_transaction(
        &mut terminal.screen,
        &mut terminal.pointer,
        &mut terminal.io,
        TerminalSize {
            rows: usize::MAX,
            cols: 2,
        },
        |_io, _size| {
            pty_called = true;
            Ok(())
        },
    );

    assert!(result.is_err());
    assert!(!pty_called);
    assert_eq!(terminal.snapshot(), before);
}

#[test]
fn repeated_mixed_resize_preserves_selection_and_primary_across_alt_and_retry() {
    let mut terminal = Terminal::new_headless(4, 12);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    terminal.put_bytes(b"\x1b[31mABC\x1b[0m");
    terminal.put_str("界DEF  \r\n\r\nTAIL");

    let event = |phase, x| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (x, 1.0),
            phase,
            TerminalPointerButton::Left,
            70,
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
    assert_eq!(terminal.selection_text(), "ABC");

    for (index, size) in [
        TerminalSize { rows: 4, cols: 8 },
        TerminalSize { rows: 6, cols: 8 },
        TerminalSize { rows: 3, cols: 6 },
        TerminalSize { rows: 4, cols: 12 },
    ]
    .into_iter()
    .enumerate()
    {
        assert!(terminal.resize_if_changed(size));
        terminal.put_str(&format!("\r\n\x1b[3{}mstep{index}\x1b[0m界", index + 1));
        assert_eq!(terminal.selection_text(), "ABC");
    }

    let before_failure = terminal.snapshot();
    let selection_before_failure = terminal.selection_text();
    let failed = Terminal::try_resize_transaction(
        &mut terminal.screen,
        &mut terminal.pointer,
        &mut terminal.io,
        TerminalSize { rows: 5, cols: 10 },
        |_io, _size| anyhow::bail!("injected PTY failure"),
    );
    assert!(failed.is_err());
    assert_eq!(terminal.snapshot(), before_failure);
    assert_eq!(terminal.selection_text(), selection_before_failure);

    terminal.put_bytes(b"\x1b[?1049h");
    assert_eq!(terminal.screen().scroll_count(), 0);
    assert!(terminal.screen().saved_primary_scroll_count() > 0);
    terminal.put_str("ALT");
    assert!(terminal.resize_if_changed(TerminalSize { rows: 5, cols: 10 }));
    terminal.put_bytes(b"\x1b[?1049l");

    assert!(!terminal.is_alt_screen());
    assert_eq!(
        (terminal.screen().rows(), terminal.screen().cols()),
        (5, 10)
    );
    assert_eq!(terminal.selection_text(), "ABC");
    assert!(terminal.screen().scroll_count() > 0);
}

#[test]
fn sgr_sets_fg_color_on_written_cells() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"a\x1b[31mb\x1b[0mc");

    // 'a' is default, 'b' is red (31), 'c' is reset to default
    assert_eq!(terminal.row_text(0), "abc     ");
    assert_eq!(terminal.screen().cell(0, 0).fg, Color::Default);
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1)); // 31 = red = Named(1)
    assert_eq!(terminal.screen().cell(0, 2).fg, Color::Default);
}

#[test]
fn live_bottom_resize_round_trip_preserves_every_scrollback_view() {
    fn retained_text(terminal: &Terminal) -> String {
        let screen = terminal.screen();
        let retained_rows = screen.scroll_count() + screen.rows();
        screen.selected_text(crate::SelectionBounds {
            start_row: screen.history_start(),
            start_col: 0,
            end_row: screen.history_start() + retained_rows as u64 - 1,
            end_col: screen.cols() - 1,
        })
    }

    fn reviewed_text(terminal: &Terminal) -> String {
        let screen = terminal.screen();
        let top = screen.history_start() + (screen.scroll_count() - screen.view_offset()) as u64;
        screen.selected_text(crate::SelectionBounds {
            start_row: top,
            start_col: 0,
            end_row: top + screen.rows() as u64 - 1,
            end_col: screen.cols() - 1,
        })
    }

    fn scrollback_views(terminal: &mut Terminal) -> Vec<(Vec<String>, Vec<bool>)> {
        terminal.scroll_viewport_to_bottom();
        let mut views = Vec::new();
        loop {
            let snapshot = terminal.snapshot();
            let rows = (0..snapshot.rows)
                .map(|row| {
                    (0..snapshot.cols)
                        .map(|col| snapshot.cell_char(row, col))
                        .collect()
                })
                .collect();
            let queried_rows: Vec<String> = (0..snapshot.rows)
                .map(|row| terminal.row_text(row))
                .collect();
            assert_eq!(
                queried_rows, rows,
                "row_text must follow the reviewed viewport"
            );
            views.push((rows, snapshot.wrapped));
            if snapshot.view_offset == snapshot.scroll_count {
                break;
            }
            terminal.scroll_viewport_up(1);
        }
        terminal.scroll_viewport_to_bottom();
        views
    }

    let output: String = (0..24)
        .map(|line| format!("L{line:02}:abcdef界ghij\r\n"))
        .collect();
    let mut terminal = Terminal::new_headless(4, 12);
    terminal.put_str(&output);
    assert_eq!(terminal.screen().view_offset(), 0);
    let expected_text = retained_text(&terminal);
    let expected_views = scrollback_views(&mut terminal);

    let mut narrow_reference = Terminal::new_headless(2, 6);
    narrow_reference.put_str(&output);

    assert!(terminal.resize_if_changed(TerminalSize { rows: 2, cols: 6 }));
    assert_eq!(terminal.screen().view_offset(), 0);
    assert_eq!(retained_text(&terminal), retained_text(&narrow_reference));
    terminal.scroll_viewport_up(3);
    narrow_reference.scroll_viewport_up(3);
    assert_eq!(reviewed_text(&terminal), reviewed_text(&narrow_reference));
    assert_eq!(
        scrollback_views(&mut terminal),
        scrollback_views(&mut narrow_reference)
    );

    assert!(terminal.resize_if_changed(TerminalSize { rows: 4, cols: 12 }));
    assert_eq!(terminal.screen().view_offset(), 0);

    assert_eq!(retained_text(&terminal), expected_text);
    assert_eq!(scrollback_views(&mut terminal), expected_views);
}

#[test]
fn narrow_resize_near_capacity_does_not_duplicate_logical_lines() {
    use std::collections::HashSet;

    let mut terminal = Terminal::new_headless(6, 24);
    for line in 0..300 {
        terminal.put_str(&format!("L{line:03}:abcdefghijklmnop\r\n"));
    }

    for cols in [18, 12, 6] {
        assert!(terminal.resize_if_changed(TerminalSize { rows: 4, cols }));
    }
    let text = {
        let screen = terminal.screen();
        let retained_rows = screen.scroll_count() + screen.rows();
        screen.selected_text(crate::SelectionBounds {
            start_row: screen.history_start(),
            start_col: 0,
            end_row: screen.history_start() + retained_rows as u64 - 1,
            end_col: screen.cols() - 1,
        })
    };
    let markers: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix('L'))
        .filter_map(|line| line.get(..3))
        .map(str::to_owned)
        .collect();
    let unique: HashSet<_> = markers.iter().collect();

    assert!(!markers.is_empty());
    assert_eq!(
        unique.len(),
        markers.len(),
        "logical lines were duplicated: {markers:?}"
    );
    assert!(
        markers.windows(2).all(|pair| pair[0] < pair[1]),
        "logical lines must remain ordered: {markers:?}"
    );
}

#[test]
fn mixed_resize_keeps_review_content_and_cursor_meaning_for_short_lines() {
    fn reviewed_first_line(terminal: &Terminal) -> String {
        let snapshot = terminal.snapshot();
        snapshot
            .cells
            .iter()
            .take(snapshot.cols)
            .map(|cell| cell.ch)
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    let mut terminal = Terminal::new_headless(3, 8);
    terminal.put_str("L0\r\nL1\r\nL2\r\nL3\r\nL4");
    assert!(terminal.screen().scroll_count() >= 2);
    terminal.scroll_viewport_up(1);
    let reviewed_line = reviewed_first_line(&terminal);
    assert!(terminal.screen().view_offset() > 0);

    assert!(terminal.resize_if_changed(TerminalSize { rows: 2, cols: 8 }));
    assert_eq!(reviewed_first_line(&terminal), reviewed_line);
    assert!(terminal.screen().view_offset() > 0);

    assert!(terminal.resize_if_changed(TerminalSize { rows: 2, cols: 6 }));
    assert_eq!(reviewed_first_line(&terminal), reviewed_line);
    assert!(terminal.screen().view_offset() > 0);
    terminal.put_str("Z");
    let screen = terminal.screen();
    let retained_rows = screen.scroll_count() + screen.rows();
    let text = screen.selected_text(crate::SelectionBounds {
        start_row: screen.history_start(),
        start_col: 0,
        end_row: screen.history_start() + retained_rows as u64 - 1,
        end_col: screen.cols() - 1,
    });
    assert!(text.contains("L4Z"));
}
