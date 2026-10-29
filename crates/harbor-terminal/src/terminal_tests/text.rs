use super::*;

#[test]
fn writes_plain_characters_and_tracks_cursor() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_str("ab");

    assert_eq!(terminal.row_text(0), "ab  ");
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 4));
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn crlf_moves_to_next_row_start() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_str("a\r\nb");

    assert_eq!(terminal.row_text(0), "a   ");
    assert_eq!(terminal.row_text(1), "b   ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (1, 1)
    );
}

#[test]
fn carriage_return_overwrites_from_row_start() {
    let mut terminal = Terminal::new_headless(1, 4);

    terminal.put_str("ab\rc");

    assert_eq!(terminal.row_text(0), "cb  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (1, 0)
    );
}

#[test]
fn backspace_is_non_destructive() {
    let mut terminal = Terminal::new_headless(1, 4);

    terminal.put_str("ab\u{8}");

    assert_eq!(terminal.row_text(0), "ab  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (1, 0)
    );
}

#[test]
fn backspace_erases_previous_cell() {
    let mut terminal = Terminal::new_headless(1, 4);

    terminal.put_str("ab\u{8}c");

    assert_eq!(terminal.row_text(0), "ac  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn scrolls_when_writing_past_last_row() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_str("one\r\ntwo\r\nthr");

    assert_eq!(terminal.row_text(0), "two ");
    assert_eq!(terminal.row_text(1), "thr ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (3, 1)
    );
}

#[test]
fn resize_reflows_primary_and_keeps_the_live_cursor_suffix() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal.put_str("abcdef");

    terminal.resize(1, 3);

    assert_eq!(terminal.row_text(0), "def");
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 3));
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn csi_erase_line_clears_selected_range() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_bytes(b"abcd\x1b[1;3H\x1b[K");

    assert_eq!(terminal.row_text(0), "ab  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn csi_erase_display_mode_two_clears_and_homes() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_bytes(b"abcd");
    terminal.put_bytes(b"\x1b[2Jx");

    assert_eq!(terminal.row_text(0), "x   ");
    assert_eq!(terminal.row_text(1), "    ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (1, 0)
    );
}

#[test]
fn keeps_incomplete_escape_sequence_across_chunks() {
    let mut terminal = Terminal::new_headless(1, 5);

    terminal.put_bytes(b"a\x1b[");

    assert_eq!(terminal.row_text(0), "a    ");

    terminal.put_bytes(b"2CZ");

    assert_eq!(terminal.row_text(0), "a  Z ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (4, 0)
    );
}

#[test]
fn keeps_incomplete_utf8_sequence_across_chunks() {
    let mut terminal = Terminal::new_headless(1, 4);
    let bytes = "中".as_bytes();

    terminal.put_bytes(&bytes[..1]);
    terminal.put_bytes(&bytes[1..]);

    assert_eq!(terminal.row_text(0), "中   ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn treats_cjk_characters_as_double_width_cells() {
    let mut terminal = Terminal::new_headless(1, 4);

    terminal.put_str("中a");

    assert_eq!(terminal.row_text(0), "中 a ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (3, 0)
    );
}

#[test]
fn overwrites_both_cells_of_double_width_character() {
    let mut terminal = Terminal::new_headless(1, 4);

    terminal.put_str("中b");
    terminal.put_bytes(b"\x1b[1;2HX");

    assert_eq!(terminal.row_text(0), " Xb ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn backspace_on_double_width_character_is_non_destructive() {
    let mut terminal = Terminal::new_headless(1, 4);

    terminal.put_str("中\u{8}");

    assert_eq!(terminal.row_text(0), "中   ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 0)
    );
}

#[test]
fn horizontal_tab_at_line_end_does_not_loop_forever() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_str("abc\tz");

    assert_eq!(terminal.row_text(0), "abcz");
    assert_eq!(terminal.row_text(1), "    ");
}

#[test]
fn ignores_private_cursor_visibility_sequence() {
    let mut terminal = Terminal::new_headless(1, 6);

    terminal.put_bytes(b"a\x1b[?25lb");

    assert_eq!(terminal.row_text(0), "ab    ");
}

#[test]
fn ignores_osc_title_sequence_terminated_by_bel() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"a\x1b]0;C:\\Windows\\system32\\cmd.exe\x07b");

    assert_eq!(terminal.row_text(0), "ab      ");
}

#[test]
fn keeps_incomplete_osc_sequence_across_chunks() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"a\x1b]0;title");
    terminal.put_bytes(b"\x1b\\b");

    assert_eq!(terminal.row_text(0), "ab      ");
}

#[test]
fn osc_titles_validate_and_drain_once_in_fifo_order() {
    let mut terminal = Terminal::new_headless(1, 8);

    terminal.put_bytes(b"\x1b]0;first\x07\x1b]2;second;part\x1b\\");

    assert_eq!(
        terminal.drain_output_events(),
        vec![
            TerminalOutputEvent::TitleChanged("first".to_owned()),
            TerminalOutputEvent::TitleChanged("second;part".to_owned()),
        ]
    );
    assert!(terminal.drain_output_events().is_empty());
}

#[test]
fn erase_chars_via_csi_x_clears_specified_count() {
    let mut terminal = Terminal::new_headless(1, 20);
    terminal.put_bytes(b"hello world!!!!!!");
    assert_eq!(terminal.row_text(0).trim_end(), "hello world!!!!!!");

    // Move cursor to col 11 (at '!') and ECH 6 chars.
    terminal.put_bytes(b"\r\x1b[11C\x1b[6X");
    assert_eq!(
        terminal.row_text(0).trim_end(),
        "hello world",
        "CSI 6 X should erase 6 exclamation marks"
    );
}

#[test]
fn alt_screen_enter_exit_preserves_normal_screen() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("normal");
    assert_eq!(terminal.row_text(0).trim(), "normal");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (6, 0)
    );

    // Enter alt screen
    terminal.put_str("\x1b[?1049h");
    // Alt screen starts blank with default cursor
    assert!(terminal.row_text(0).trim().is_empty());
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 0)
    );

    // Write to alt screen
    terminal.put_str("alt");
    assert_eq!(terminal.row_text(0).trim(), "alt");

    // Exit alt screen
    terminal.put_str("\x1b[?1049l");
    // Normal screen restored, alt content gone, cursor restored
    assert_eq!(terminal.row_text(0).trim(), "normal");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (6, 0)
    );
    assert!(!terminal.is_alt_screen());
}
