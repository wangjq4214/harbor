use super::*;

#[test]
fn alt_screen_enter_twice_is_idempotent() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("\x1b[?1049h");
    terminal.put_str("first");
    terminal.put_str("\x1b[?1049h"); // second enter — no-op
    assert_eq!(terminal.row_text(0).trim(), "first");
}

#[test]
fn alt_screen_exit_when_not_in_alt_is_noop() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?1049l"); // exit without enter — no panic
    assert_eq!(terminal.row_text(0).trim(), "normal");
}

#[test]
fn alt_screen_switch_mid_batch_splits_correctly() {
    // Simulates PTY sending CSI ?1049h followed by content in one read.
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("before");
    terminal.put_bytes(b"\x1b[?1049hafter");
    // "before" stayed on normal screen, "after" landed on alt screen.
    assert_eq!(terminal.row_text(0).trim(), "after");

    terminal.put_str("\x1b[?1049l");
    assert_eq!(terminal.row_text(0).trim(), "before");
}

#[test]
fn alt_screen_resize_preserves_both_screens() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?1049h");
    terminal.put_str("alt");
    // Resize: both screens resize without panic.
    terminal.resize(5, 30);
    assert_eq!(terminal.screen().rows(), 5);
    assert_eq!(terminal.screen().cols(), 30);
    terminal.put_str("\x1b[?1049l");
    assert_eq!(terminal.screen().rows(), 5);
}

#[test]
fn alt_screen_exit_restores_scrollback_viewport() {
    let mut terminal = Terminal::new_headless(5, 10);
    // Write enough lines to create scrollback.
    for _ in 0..6 {
        terminal.process_output(b"line\n");
    }
    terminal.scroll_viewport_up(2);
    let offset_before = terminal.screen().view_offset();
    assert!(offset_before > 0, "expected scrollback before alt screen");

    // Enter alt screen: viewport should snap to the live bottom of the alt buffer.
    terminal.put_str("\x1b[?1049h");
    assert_eq!(
        terminal.screen().view_offset(),
        0,
        "alt screen should start with zero view offset"
    );

    // Write to alt screen and exit.
    terminal.put_str("alt");
    terminal.put_str("\x1b[?1049l");

    // Normal screen's scrollback viewport must be restored exactly.
    assert_eq!(
        terminal.screen().view_offset(),
        offset_before,
        "exit alt screen should restore previous view offset"
    );
    assert!(!terminal.is_alt_screen());
}

#[test]
fn alt_screen_47_preserves_contents_across_reentry() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?47h");
    terminal.put_str("alt1");
    terminal.put_str("\x1b[?47l");
    assert_eq!(terminal.row_text(0).trim(), "normal");
    assert!(!terminal.is_alt_screen());

    // Re-enter without clear: the alternate buffer must keep its contents.
    terminal.put_str("\x1b[?47h");
    assert_eq!(
        terminal.row_text(0).trim(),
        "alt1",
        "?47 must preserve alternate-buffer contents across re-entry"
    );

    // Exit after re-entry: the primary screen must be restored (regression:
    // a parked-buffer restore must not clobber the saved primary screen).
    terminal.put_str("\x1b[?47l");
    assert_eq!(terminal.row_text(0).trim(), "normal");
    assert!(!terminal.is_alt_screen());
}

#[test]
fn alt_screen_47_cursor_is_per_buffer() {
    let mut terminal = Terminal::new_headless(5, 20);
    terminal.put_str("abc"); // cursor at 0-based (3, 0)
    terminal.put_str("\x1b[?47h");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 0),
        "alternate buffer has its own cursor"
    );
    terminal.put_str("\x1b[?47l");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (3, 0),
        "primary cursor restored on exit"
    );
}

#[test]
fn alt_screen_1047_clears_on_entry() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?1047h");
    assert!(terminal.row_text(0).trim().is_empty());
    terminal.put_str("alt1");
    terminal.put_str("\x1b[?1047l");
    assert_eq!(terminal.row_text(0).trim(), "normal");

    // Re-enter clears the alternate buffer.
    terminal.put_str("\x1b[?1047h");
    assert!(
        terminal.row_text(0).trim().is_empty(),
        "?1047 must clear on entry"
    );
}

#[test]
fn alt_screen_1047_clear_entry_discards_parked_content_after_resize() {
    let mut terminal = Terminal::new_headless(2, 8);
    terminal.put_bytes(b"\x1b[?47h");
    terminal.put_str("old-alt");
    terminal.put_bytes(b"\x1b[?47l");
    assert!(terminal.resize_if_changed(TerminalSize { rows: 3, cols: 4 }));

    terminal.put_bytes(b"\x1b[?1047h");

    assert!(terminal.is_alt_screen());
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (3, 4));
    assert!(terminal.row_text(0).trim().is_empty());
}

#[test]
fn alt_screen_1048_saved_cursor_projects_through_resize() {
    let mut terminal = Terminal::new_headless(2, 8);
    terminal.put_str("abc");
    terminal.put_bytes(b"\x1b[?1048h");
    assert!(terminal.resize_if_changed(TerminalSize { rows: 2, cols: 4 }));
    terminal.put_bytes(b"\x1b[1;1H");

    terminal.put_bytes(b"\x1b[?1048l");

    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (3, 0)
    );
}

#[test]
fn alt_screen_1048_saves_and_restores_cursor() {
    let mut terminal = Terminal::new_headless(5, 20);
    terminal.put_str("abc"); // cursor at 0-based (3, 0)
    terminal.put_str("\x1b[?1048h"); // save cursor (DECSC)
    terminal.put_bytes(b"\x1b[5;10H"); // 0-based (9, 4)
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (9, 4)
    );
    terminal.put_str("\x1b[?1048l"); // restore cursor (DECRC)
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (3, 0),
        "?1048 l restores the saved cursor position"
    );
    assert!(!terminal.is_alt_screen(), "?1048 must not switch buffers");
    assert_eq!(terminal.row_text(0).trim(), "abc");
}

#[test]
fn alt_screen_1048_restore_without_save_is_noop() {
    let mut terminal = Terminal::new_headless(5, 20);
    terminal.put_bytes(b"\x1b[5;5H");
    terminal.put_str("\x1b[?1048l"); // no prior save
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (4, 4),
        "?1048 l without a prior save is a no-op"
    );
}

#[test]
fn ris_while_in_alt_exits_and_drops_alt_buffer() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?1049h");
    terminal.put_str("alt");
    terminal.put_str("\x1b[?47l"); // persist alternate contents
    terminal.put_str("\x1b[?47h");
    assert_eq!(terminal.row_text(0).trim(), "alt");

    terminal.process_output(b"\x1bc"); // RIS
    assert!(!terminal.is_alt_screen());
    assert!(terminal.row_text(0).trim().is_empty());

    // A later ?47 entry must not resurrect the pre-RIS alternate buffer.
    terminal.put_str("\x1b[?47h");
    assert!(
        terminal.row_text(0).trim().is_empty(),
        "RIS must drop the alt buffer"
    );
}

#[test]
fn decstr_while_in_alt_stays_in_alt() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?1049h");
    terminal.put_str("alt");
    terminal.put_bytes(b"\x1b[!p"); // DECSTR soft reset
    assert!(terminal.is_alt_screen(), "DECSTR must not switch screens");
    assert_eq!(
        terminal.row_text(0).trim(),
        "alt",
        "DECSTR must not clear cells"
    );
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 0),
        "DECSTR homes and resets the cursor"
    );
    terminal.put_str("\x1b[?1049l");
    assert_eq!(terminal.row_text(0).trim(), "normal");
}

#[test]
fn alt_screen_47_inside_1049_is_noop() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("\x1b[?1049h");
    terminal.put_str("first");
    terminal.put_str("\x1b[?47h"); // already in alt: no-op, must not clear
    assert_eq!(terminal.row_text(0).trim(), "first");
}

#[test]
fn alt_screen_1047_exit_then_47_reentry_shares_buffer() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?1047h");
    terminal.put_str("alt1");
    terminal.put_str("\x1b[?1047l");
    assert_eq!(terminal.row_text(0).trim(), "normal");

    // ?47 re-entry must restore the buffer parked by the ?1047 exit — the two
    // modes share one alternate buffer.
    terminal.put_str("\x1b[?47h");
    assert_eq!(terminal.row_text(0).trim(), "alt1");
}
