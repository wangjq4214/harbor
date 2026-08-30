//! Integration-style tests for the Terminal facade.

use crate::io::PTY_QUEUE_CAPACITY;
use crate::screen::CellAttrs;
use crate::screen::Color;
use crate::{
    FrameDemand, InputModes, PasteDisposition, Terminal, TerminalEvent, TerminalFocusEvent,
    TerminalKey, TerminalKeyboardEvent, TerminalModifiers, TerminalPointerButton,
    TerminalPointerEvent, TerminalPointerPhase, TerminalSize, safe_preview_line,
    should_confirm_multiline,
};
use std::borrow::Cow;
use std::time::Instant;

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
fn resize_preserves_visible_cells_and_clamps_cursor() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal.put_str("abcdef");

    terminal.resize(1, 3);

    assert_eq!(terminal.row_text(0), "abc");
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 3));
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn resize_preserves_scrollback_viewport() {
    let mut terminal = Terminal::new_headless(2, 4);
    for line in ["A", "B", "C", "D"] {
        terminal.process_output(format!("{line}\r\n").as_bytes());
    }
    terminal.scroll_viewport_up(1);
    let displayed = terminal.row_text(0);
    let history_start = terminal.screen().history_start();

    terminal.resize(3, 6);

    assert_eq!(terminal.screen().view_offset(), 1);
    assert!(terminal.screen().scroll_count() > 0);
    assert_eq!(terminal.row_text(0), format!("{displayed}  "));
    assert_eq!(terminal.screen().history_start(), history_start);
}

#[test]
fn resize_zero_dimensions_uses_safe_terminal_size() {
    let mut terminal = Terminal::new_headless(2, 4);
    terminal.resize(0, 0);

    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 1));
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

// ── SGR attribute tests ─────────────────────────────────────────

#[test]
fn sgr_bold_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1ma");
    assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::BOLD));
}

#[test]
fn sgr_dim_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[2ma");
    assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::DIM));
}

#[test]
fn sgr_italic_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[3ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::ITALIC)
    );
}

#[test]
fn sgr_underline_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[4ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::UNDERLINE)
    );
}

#[test]
fn sgr_blink_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[5ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::BLINK)
    );
}

#[test]
fn sgr_inverse_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[7ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::INVERSE)
    );
}

#[test]
fn sgr_strikethrough_sets_attr() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[9ma");
    assert!(
        terminal
            .screen()
            .cell(0, 0)
            .attrs
            .contains(CellAttrs::STRIKETHROUGH)
    );
}

#[test]
fn sgr_reset_clears_all() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;31;42ma");
    terminal.put_bytes(b"\x1b[0mb");
    let cell = terminal.screen().cell(0, 1);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
    assert!(cell.attrs.is_empty());
}

// ── SGR 8-color tests ───────────────────────────────────────────

#[test]
fn sgr_8color_fg_sets_named() {
    for code in 30u8..=37u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).fg,
            Color::Named(code - 30),
            "SGR {} should set fg Named({})",
            code,
            code - 30
        );
    }
}

#[test]
fn sgr_8color_bg_sets_named() {
    for code in 40u8..=47u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).bg,
            Color::Named(code - 40),
            "SGR {} should set bg Named({})",
            code,
            code - 40
        );
    }
}

#[test]
fn sgr_bright_fg_sets_bright() {
    for code in 90u8..=97u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).fg,
            Color::Bright(code - 90),
            "SGR {} should set fg Bright({})",
            code,
            code - 90
        );
    }
}

#[test]
fn sgr_bright_bg_sets_bright() {
    for code in 100u8..=107u8 {
        let mut terminal = Terminal::new_headless(1, 2);
        let seq = format!("\x1b[{}mX", code);
        terminal.put_bytes(seq.as_bytes());
        assert_eq!(
            terminal.screen().cell(0, 0).bg,
            Color::Bright(code - 100),
            "SGR {} should set bg Bright({})",
            code,
            code - 100
        );
    }
}

#[test]
fn sgr_256color_fg_sets_indexed() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[38;5;200mb");
    assert_eq!(terminal.screen().cell(0, 0).fg, Color::Indexed(200));
}

#[test]
fn sgr_256color_bg_sets_indexed() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[48;5;100mb");
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Indexed(100));
}

#[test]
fn sgr_truecolor_fg_sets_rgb() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[38;2;10;20;30mb");
    assert_eq!(terminal.screen().cell(0, 0).fg, Color::Rgb(10, 20, 30));
}

#[test]
fn sgr_truecolor_bg_sets_rgb() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[48;2;100;150;200mb");
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Rgb(100, 150, 200));
}

#[test]
fn sgr_multi_param_sets_all() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;31;44ma");
    let cell = terminal.screen().cell(0, 0);
    assert!(cell.attrs.contains(CellAttrs::BOLD));
    assert_eq!(cell.fg, Color::Named(1));
    assert_eq!(cell.bg, Color::Named(4));
}

#[test]
fn sgr_default_fg_bg_resets_colors() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31;42m\x1b[39;49mb");
    let cell = terminal.screen().cell(0, 0);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
}

#[test]
fn sgr_compound_clear_removes_attrs() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;3ma");
    let cell = terminal.screen().cell(0, 0);
    assert!(cell.attrs.contains(CellAttrs::BOLD));
    assert!(cell.attrs.contains(CellAttrs::ITALIC));
    terminal.put_bytes(b"\x1b[23mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::ITALIC));
    assert!(cell.attrs.contains(CellAttrs::BOLD));
}

#[test]
fn sgr_22_clears_bold_and_dim() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;2ma\x1b[22mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::BOLD));
    assert!(!cell.attrs.contains(CellAttrs::DIM));
}

#[test]
fn sgr_24_clears_underline() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[4ma\x1b[24mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::UNDERLINE));
}

#[test]
fn sgr_25_clears_blink() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[5ma\x1b[25mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::BLINK));
}

#[test]
fn sgr_27_clears_inverse() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[7ma\x1b[27mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::INVERSE));
}

#[test]
fn sgr_29_clears_strikethrough() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[9ma\x1b[29mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::STRIKETHROUGH));
}

#[test]
fn sgr_bare_csi_m_is_reset() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[1;31;42ma\x1b[mb");
    let cell = terminal.screen().cell(0, 1);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
    assert!(cell.attrs.is_empty());
}

// ── SGR error handling / robustness ─────────────────────────────

#[test]
fn sgr_indexed_out_of_range_ignored() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;5;300mb");
    // 300 > 255 so fg should still be Named(1) from the 31 sequence
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}

#[test]
fn sgr_truecolor_missing_params_ignored() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;2;128;64mx");
    // Incomplete truecolor seq — fg stays red, 'x' still renders
    assert_eq!(terminal.row_text(0), "ax  ");
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}

#[test]
fn sgr_truecolor_component_out_of_range_ignored() {
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;2;300;0;0mb");
    // 300 > 255 — fg stays red
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}
#[test]
fn csi_cursor_position_overwrites_target_cell() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.put_bytes(b"abcd\x1b[1;2HZ");

    assert_eq!(terminal.row_text(0), "aZcd");
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
fn cargo_update_output_spans_multiple_rows() {
    // Replay the PTY output chunks logged during `cargo update`.
    let mut terminal = Terminal::new_headless(5, 80);

    // Chunk 0: "    Updating crates.io index\r\n"
    terminal.put_bytes(b"\x1b[92m\x1b[1m    Updating\x1b[m crates.io index\r\n");
    assert_eq!(
        terminal.row_text(0).trim_end(),
        "    Updating crates.io index"
    );
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 1)
    );

    // Chunks 1-6: progress-bar updates that rewrite the same row via \r.
    terminal.put_bytes(b"\x1b[96m\x1b[1m       Fetch\x1b[m ");
    terminal.put_bytes(b"\x1b]9;4;3;0\x1b\\");
    terminal.put_bytes(b"[=====>                           ] 0 complete; 1 pending\x1b[144X\r");
    terminal.put_bytes(b"\x1b[96m\x1b[1m       Fetch\x1b[m ");
    terminal.put_bytes(b"\x1b]9;4;3;0\x1b\\");
    terminal.put_bytes(b"[=====>                           ] 1 complete; 0 pending\x1b[144X\r");
    // Confirm row 0 is untouched by progress bars.
    assert_eq!(
        terminal.row_text(0).trim_end(),
        "    Updating crates.io index"
    );

    // Chunk 7: "     Locking 0 packages ...\r\n"
    terminal.put_bytes(
            b"\x1b[92m\x1b[1m     Locking\x1b[m 0 packages to latest Rust 1.95.0 compatible versions\x1b[151X\r\n",
        );
    let row1 = terminal.row_text(1);
    assert!(
        row1.contains("Locking 0 packages to latest Rust 1.95.0 compatible versions"),
        "expected locking line on row 1, got: {row1:?}"
    );
    // CSI 151 X should have erased the stale progress-bar tail ("0 pending").
    assert!(
        !row1.contains("pending"),
        "ECH should have erased stale 'pending' text from progress bar, got: {row1:?}"
    );
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 2)
    );

    // Chunk 8: "\r\nd:\workspaces\harbor>"
    // Chunk 8: "\r\nd:\workspaces\harbor>" — the leading \r\n advances
    // to the next row, so the prompt lands on row 3, not row 2.
    terminal.put_bytes(b"\r\nd:\\workspaces\\harbor>");
    let row3 = terminal.row_text(3);
    assert!(
        row3.contains("d:\\workspaces\\harbor>"),
        "expected prompt on row 3, got: {row3:?}"
    );
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

// ── ?47 / ?1047 / ?1048 alternate-screen family (issue #92) ────────

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

// ── ICH / DCH integration ───────────────────────────────────

#[test]
fn ich_via_csi_at_shifts_cells_right() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal.put_str("abcdef");
    terminal.put_bytes(b"\x1b[1;3H"); // CUP: col 3 (0-based col 2)
    terminal.put_bytes(b"\x1b[2@"); // ICH 2
    assert_eq!(terminal.row_text(0), "ab  cdef");
}

#[test]
fn dch_via_csi_p_shifts_cells_left() {
    let mut terminal = Terminal::new_headless(1, 8);
    terminal.put_str("abcdef");
    terminal.put_bytes(b"\x1b[1;3H"); // col 3
    terminal.put_bytes(b"\x1b[2P"); // DCH 2
    assert_eq!(terminal.row_text(0), "abef    ");
}

// ── IL / DL integration ─────────────────────────────────────

#[test]
fn il_via_csi_l_inserts_lines() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4 (1-based)
    terminal.put_bytes(b"\x1b[2;1H"); // cursor to row 2
    terminal.put_bytes(b"\x1b[1L"); // IL 1
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "    ");
    assert_eq!(terminal.row_text(2), "bbbb");
    assert_eq!(terminal.row_text(3), "cccc");
    assert_eq!(terminal.row_text(4), "    ");
}

#[test]
fn dl_via_csi_m_deletes_lines() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4
    terminal.put_bytes(b"\x1b[2;1H"); // cursor to row 2
    terminal.put_bytes(b"\x1b[1M"); // DL 1
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "cccc");
    assert_eq!(terminal.row_text(2), "dddd");
    assert_eq!(terminal.row_text(3), "    ");
    assert_eq!(terminal.row_text(4), "    ");
}

// ── SU / SD integration ─────────────────────────────────────

#[test]
fn su_via_csi_s_scrolls_up() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4
    terminal.put_bytes(b"\x1b[2S"); // SU 2
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "dddd"); // shifted up by 2
    assert_eq!(terminal.row_text(2), "    ");
    assert_eq!(terminal.row_text(3), "    ");
    assert_eq!(terminal.row_text(4), "    ");
}

#[test]
fn sd_via_csi_t_scrolls_down() {
    let mut terminal = Terminal::new_headless(5, 4);
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    terminal.put_bytes(b"\x1b[4;1Hdddd");
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4
    terminal.put_bytes(b"\x1b[2T"); // SD 2
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "    ");
    assert_eq!(terminal.row_text(2), "    ");
    assert_eq!(terminal.row_text(3), "bbbb"); // shifted down by 2
    assert_eq!(terminal.row_text(4), "    ");
}

// ── DECSTBM region ──────────────────────────────────────────

#[test]
fn decstbm_region_respected_by_scroll() {
    let mut terminal = Terminal::new_headless(4, 4);
    // Write content with default (full) scroll region first.
    terminal.put_bytes(b"\x1b[1;1Haaaa");
    terminal.put_bytes(b"\x1b[2;1Hbbbb");
    terminal.put_bytes(b"\x1b[3;1Hcccc");
    // Now set scroll region to [1,2] via CSI r.
    terminal.put_bytes(b"\x1b[2;3r"); // region rows 2-3 (1-based) = [1,2]
    // Newline at scroll_bottom (row 2, 0-based) → only region scrolls.
    terminal.put_bytes(b"\x1b[3;1H"); // cursor to row 3 = scroll_bottom
    terminal.put_str("\n");
    // Region [1,2] scrolled up: row 1 gets old row 2 "cccc", row 2 blanked.
    assert_eq!(terminal.row_text(0), "aaaa");
    assert_eq!(terminal.row_text(1), "cccc");
    assert_eq!(terminal.row_text(2), "    ");
    assert_eq!(terminal.row_text(3), "    ");
}

#[test]
fn decstbm_vim_like_scenario() {
    // Simulate vim setting scroll region, writing lines, and scrolling within region.
    let mut terminal = Terminal::new_headless(5, 10);
    // Write lines with default (full) scroll region first.
    terminal.put_bytes(b"\x1b[1;1Htitle");
    terminal.put_bytes(b"\x1b[2;1Hline1");
    terminal.put_bytes(b"\x1b[3;1Hline2");
    terminal.put_bytes(b"\x1b[4;1Hline3");
    // Set scroll region to [1,3].
    terminal.put_bytes(b"\x1b[2;4r"); // CSI r: region rows 2-4 (1-based)
    // Trigger scroll within region: newline from row 3 (scroll_bottom).
    terminal.put_bytes(b"\x1b[4;1H\n"); // LF → scroll_up region
    // Region [1,3] scrolled up: row 1 gets old row 2 "line2",
    // row 2 gets old row 3 "line3", row 3 blanked.
    assert_eq!(terminal.row_text(0).trim_end(), "title");
    assert_eq!(terminal.row_text(1), "line2     ");
    assert_eq!(terminal.row_text(2), "line3     ");
    assert_eq!(terminal.row_text(3), "          ");
}

// ── Cursor save/restore ─────────────────────────────────────

#[test]
fn cursor_save_restore_via_esc_7_8() {
    let mut terminal = Terminal::new_headless(4, 10);
    terminal.put_bytes(b"\x1b[2;3H"); // cursor to row 2, col 3
    terminal.put_bytes(b"\x1b7"); // ESC 7 → save cursor (row 1, col 2)
    terminal.put_bytes(b"\x1b[4;8H"); // cursor to row 4, col 8
    terminal.put_str("XX"); // write at row 3, col 7
    terminal.put_bytes(b"\x1b8"); // ESC 8 → restore cursor to (row 1, col 2)
    terminal.put_str("YY"); // write starting at row 1, col 2
    // Row 1 should have spaces with YY at cols 2-3.
    let row1 = terminal.row_text(1);
    assert_eq!(&row1[..2], "  ");
    assert_eq!(&row1[2..4], "YY");
}

// ── viewport snap contract (step 5.4 from code review) ──────────
//
// `put_bytes` must NOT snap the viewport; `process_output` must.

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

// ── SGR background + erase integration ────────────────────────

#[test]
fn sgr_bg_preserved_after_erase_line() {
    // Vim's pattern: set bg → write text → CSI K (erase to end of line)
    let mut terminal = Terminal::new_headless(1, 6);
    terminal.put_bytes(b"\x1b[44mHi\x1b[K");
    // "Hi" should have blue bg; erased remainder should also have blue bg
    let cell = terminal.screen();
    assert_eq!(cell.cell(0, 0).ch, 'H');
    assert_eq!(cell.cell(0, 0).bg, Color::Named(4));
    assert_eq!(cell.cell(0, 1).ch, 'i');
    assert_eq!(cell.cell(0, 1).bg, Color::Named(4));
    // erased cells (cols 2-5) should have the same bg, not default
    for col in 2..6 {
        assert_eq!(cell.cell(0, col).bg, Color::Named(4));
    }
}

#[test]
fn sgr_bg_preserved_after_erase_display() {
    let mut terminal = Terminal::new_headless(2, 4);
    // Set bg green, write, erase entire display
    terminal.put_bytes(b"\x1b[42mab\x1b[2J");
    for row in 0..2 {
        for col in 0..4 {
            assert_eq!(
                terminal.screen().cell(row, col).bg,
                Color::Named(2),
                "erase_display(2) should preserve current_bg in all cells"
            );
        }
    }
}

#[test]
fn default_bg_after_sgr_reset_and_erase() {
    // After SGR reset (ESC [ m), erasing should produce default-bg cells
    let mut terminal = Terminal::new_headless(1, 4);
    terminal.put_bytes(b"\x1b[44mHi\x1b[0m\x1b[K");
    // "Hi" was written before reset, so still has blue bg
    assert_eq!(terminal.screen().cell(0, 0).ch, 'H');
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Named(4));
    assert_eq!(terminal.screen().cell(0, 1).ch, 'i');
    assert_eq!(terminal.screen().cell(0, 1).bg, Color::Named(4));
    // erased cells (cols 2-3) were erased after SGR reset → default bg
    for col in 2..4 {
        assert_eq!(terminal.screen().cell(0, col).bg, Color::Default);
    }
}

#[test]
fn bracketed_paste_mode_tracks_decset_and_decrst() {
    let mut terminal = Terminal::new_headless(1, 1);

    terminal.put_bytes(b"\x1b[?2004h");
    assert!(terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004l");
    assert!(!terminal.screen().input_modes().bracketed_paste);
}

#[test]
fn bracketed_paste_mode_resets_and_is_scoped_to_active_screen() {
    let mut terminal = Terminal::new_headless(1, 1);

    terminal.put_bytes(b"\x1b[?2004h\x1bc");
    assert!(!terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004h\x1b[!p");
    assert!(!terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004h\x1b[?1049h");
    assert!(!terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004h\x1b[?1049l");
    assert!(terminal.screen().input_modes().bracketed_paste);
}

#[test]
fn paste_without_bracketed_mode_preserves_raw_bytes() {
    let modes = InputModes::default();
    let bytes = modes.paste(b"first\r\nsecond\x1b[A");

    assert!(matches!(bytes, Cow::Borrowed(_)));
    assert_eq!(bytes.as_ref(), b"first\r\nsecond\x1b[A");
}

#[test]
fn paste_with_bracketed_mode_frames_multiline_content() {
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };
    assert_eq!(
        modes.paste(b"first\r\nsecond\x1b[A").as_ref(),
        b"\x1b[200~first\r\nsecond\x1b[A\x1b[201~"
    );
}

#[test]
fn paste_with_bracketed_mode_frames_empty_content() {
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };
    assert_eq!(modes.paste(b"").as_ref(), b"\x1b[200~\x1b[201~");
}

#[test]
fn paste_with_bracketed_mode_retains_end_marker_for_large_content() {
    let text = vec![b'x'; 1024 * 1024];
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };

    let bytes = modes.paste(&text);
    assert_eq!(
        bytes.len(),
        text.len() + b"\x1b[200~".len() + b"\x1b[201~".len()
    );
    assert_eq!(&bytes[..b"\x1b[200~".len()], b"\x1b[200~");
    assert_eq!(
        &bytes[b"\x1b[200~".len()..b"\x1b[200~".len() + text.len()],
        text.as_slice()
    );
    assert_eq!(&bytes[text.len() + b"\x1b[200~".len()..], b"\x1b[201~");
}

// ── should_confirm_multiline ────────────────────────────────────────────

#[test]
fn multiline_empty_is_not_multiline() {
    assert!(!should_confirm_multiline(""));
}

#[test]
fn multiline_single_line_is_not_multiline() {
    assert!(!should_confirm_multiline("hello"));
}

#[test]
fn multiline_single_line_with_trailing_lf_is_not_multiline() {
    assert!(!should_confirm_multiline("hello\n"));
}

#[test]
fn multiline_single_line_with_trailing_crlf_is_not_multiline() {
    assert!(!should_confirm_multiline("hello\r\n"));
}

#[test]
fn multiline_single_line_with_multiple_trailing_newlines_is_not_multiline() {
    assert!(!should_confirm_multiline("hello\n\n\n"));
    assert!(!should_confirm_multiline("hello\r\n\r\n"));
    assert!(!should_confirm_multiline("hello\n\r\n"));
}

#[test]
fn multiline_two_lines_is_multiline() {
    assert!(should_confirm_multiline("hello\nworld"));
}

#[test]
fn multiline_two_lines_with_trailing_lf_is_multiline() {
    assert!(should_confirm_multiline("hello\nworld\n"));
}

#[test]
fn multiline_windows_crlf_is_multiline() {
    assert!(should_confirm_multiline("hello\r\nworld"));
}

#[test]
fn multiline_windows_crlf_with_trailing_crlf_is_multiline() {
    assert!(should_confirm_multiline("hello\r\nworld\r\n"));
}

#[test]
fn multiline_three_lines_is_multiline() {
    assert!(should_confirm_multiline("a\nb\nc"));
}

#[test]
fn multiline_only_newlines_is_not_multiline() {
    assert!(!should_confirm_multiline("\n"));
    assert!(!should_confirm_multiline("\n\n\n"));
    assert!(!should_confirm_multiline("\r\n\r\n"));
}

#[test]
fn multiline_mixed_line_endings_is_multiline() {
    assert!(should_confirm_multiline("a\r\nb\nc"));
    // After trimming trailing \r\n, the remaining "a" has no newline
    assert!(!should_confirm_multiline("a\r\n"));
    // After trimming trailing \n, "a\r" has the CR which is a line break
    assert!(should_confirm_multiline("a\rb\n"));
}

// ── PasteDisposition ─────────────────────────────────────────────────────

#[test]
fn disposition_bracketed_paste_on_sends_direct() {
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };
    assert_eq!(
        PasteDisposition::decide(modes, "hello\nworld"),
        PasteDisposition::SendDirect
    );
    // Single-line with BP on also SendDirect
    assert_eq!(
        PasteDisposition::decide(modes, "hello"),
        PasteDisposition::SendDirect
    );
}

#[test]
fn disposition_multiline_bp_off_is_confirm() {
    let modes = InputModes::default(); // bracketed_paste: false
    let disposition = PasteDisposition::decide(modes, "hello\nworld");
    assert_eq!(
        disposition,
        PasteDisposition::Confirm {
            raw_text: "hello\nworld".to_owned()
        }
    );
}

#[test]
fn disposition_single_line_bp_off_is_send_direct() {
    let modes = InputModes::default();
    assert_eq!(
        PasteDisposition::decide(modes, "hello"),
        PasteDisposition::SendDirect
    );
    // Single line + trailing newline
    assert_eq!(
        PasteDisposition::decide(modes, "hello\n"),
        PasteDisposition::SendDirect
    );
}

#[test]
fn disposition_confirm_preserves_raw_text() {
    let modes = InputModes::default();
    let text = "line1\r\nline2\twith\ttabs\nline3";
    let disposition = PasteDisposition::decide(modes, text);
    assert_eq!(
        disposition,
        PasteDisposition::Confirm {
            raw_text: text.to_owned()
        }
    );
}

// ── safe_preview_line ───────────────────────────────────────────────────

#[test]
fn preview_plain_text_is_unchanged() {
    assert_eq!(safe_preview_line("hello world"), "hello world");
}

#[test]
fn preview_empty_string_is_empty() {
    assert_eq!(safe_preview_line(""), "");
}

#[test]
fn preview_tab_becomes_visible_marker() {
    let result = safe_preview_line("a\tb");
    assert!(result.contains('\u{2192}')); // →
    assert!(!result.contains('\t'));
}

#[test]
fn preview_esc_becomes_visible_marker() {
    let result = safe_preview_line("a\x1bb");
    assert!(!result.contains('\x1b'));
    assert!(result.len() > 3); // marker should have been inserted
}

#[test]
fn preview_cr_and_lf_pass_through() {
    // CR and LF are line-break delimiters handled by the caller;
    // safe_preview_line receives pre-split lines and should not escape them.
    assert_eq!(safe_preview_line("a\rb"), "a\rb");
    assert_eq!(safe_preview_line("a\nb"), "a\nb");
}

#[test]
fn preview_null_becomes_visible_marker() {
    let result = safe_preview_line("a\x00b");
    assert!(!result.contains('\x00'));
    assert_ne!(result, "a\x00b");
}

#[test]
fn preview_del_becomes_visible_marker() {
    let result = safe_preview_line("a\x7fb");
    assert!(!result.contains('\x7f'));
    assert_ne!(result, "a\x7fb");
}

#[test]
fn preview_multiple_controls_in_one_line() {
    let result = safe_preview_line("\t\x1b\x00");
    assert!(!result.contains('\t'));
    assert!(!result.contains('\x1b'));
    assert!(!result.contains('\x00'));
    // Each control char should be replaced by a visible marker
    assert!(result.len() >= 3);
}

#[test]
fn preview_cjk_text_is_unchanged() {
    let cjk = "你好世界";
    assert_eq!(safe_preview_line(cjk), cjk);
}

#[test]
fn preview_printable_ascii_range_is_unchanged() {
    let printable: String = (b' '..=b'~').map(|b| b as char).collect();
    assert_eq!(safe_preview_line(&printable), printable);
}

// ── Terminal API & lifecycle tests ─────────────────────────────────────

#[test]
fn should_initialize_headless_terminal_with_given_dimensions() {
    // Arrange & Act
    let terminal = Terminal::new_headless(30, 100);

    // Assert
    assert_eq!(terminal.screen().rows(), 30);
    assert_eq!(terminal.screen().cols(), 100);
}

#[test]
fn should_return_none_for_render_component_getters_when_headless() {
    // Arrange
    let terminal = Terminal::new_headless(24, 80);

    // Act & Assert
    assert!(terminal.text_metrics().is_none());
    assert!(terminal.text_glyph('A').is_none());
    assert!(terminal.text_bind_group().is_none());
    assert!(terminal.text_bind_group_layout().is_none());
}

#[test]
fn should_return_true_and_update_size_when_resize_if_changed_has_new_dimensions() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    let new_size = TerminalSize {
        rows: 30,
        cols: 100,
    };

    // Act
    let changed = terminal.resize_if_changed(new_size);

    // Assert
    assert!(changed);
    assert_eq!(terminal.screen().rows(), 30);
    assert_eq!(terminal.screen().cols(), 100);
}

#[test]
fn should_return_false_and_preserve_size_when_resize_if_changed_has_same_dimensions() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    let same_size = TerminalSize { rows: 24, cols: 80 };

    // Act
    let changed = terminal.resize_if_changed(same_size);

    // Assert
    assert!(!changed);
    assert_eq!(terminal.screen().rows(), 24);
    assert_eq!(terminal.screen().cols(), 80);
}

#[test]
fn should_derive_grid_size_from_render_target_allocation() {
    // Arrange: allocation is smaller than the full surface.
    use crate::{RenderTarget, RenderViewport, TextMetrics};

    let metrics = TextMetrics {
        cell_width: 10.0,
        line_height: 20.0,
        ascent: 16.0,
        underline_position: 16.0,
        underline_thickness: 2.0,
        strikethrough_position: 10.0,
        strikethrough_thickness: 2.0,
    };
    let target = RenderTarget::new((20.0, 10.0), (400, 240), (800, 600));
    let viewport = RenderViewport::from_target(target, &metrics);
    let grid = viewport.compute_grid_size();
    let mut terminal = Terminal::new_headless(24, 80);

    // Act
    let changed = terminal.resize_if_changed(grid);

    // Assert
    assert!(changed);
    assert_eq!(terminal.screen().rows(), grid.rows);
    assert_eq!(terminal.screen().cols(), grid.cols);
    assert_ne!((grid.rows, grid.cols), (24, 80));
}

#[test]
fn should_not_resize_grid_when_render_target_keeps_same_rows_and_cols() {
    // Arrange: geometry change that preserves qualitative grid size.
    use crate::{RenderTarget, RenderViewport, TextMetrics};

    let metrics = TextMetrics {
        cell_width: 10.0,
        line_height: 20.0,
        ascent: 16.0,
        underline_position: 16.0,
        underline_thickness: 2.0,
        strikethrough_position: 10.0,
        strikethrough_thickness: 2.0,
    };
    let first = RenderTarget::new((0.0, 0.0), (800, 480), (800, 600));
    let second = RenderTarget::new((10.0, 10.0), (800, 480), (820, 620));
    let first_grid = RenderViewport::from_target(first, &metrics).compute_grid_size();
    let second_grid = RenderViewport::from_target(second, &metrics).compute_grid_size();
    assert_eq!(first_grid, second_grid);

    let mut terminal = Terminal::new_headless(first_grid.rows, first_grid.cols);

    // Act
    let changed = terminal.resize_if_changed(second_grid);

    // Assert: PTY/screen resize is skipped when rows/cols are unchanged.
    assert!(!changed);
    assert_eq!(terminal.screen().rows(), first_grid.rows);
    assert_eq!(terminal.screen().cols(), first_grid.cols);
}

#[test]
fn should_reset_scroll_snap_suppression_when_resized() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    terminal.set_suppress_scroll_snap(true);

    // Act
    terminal.resize(30, 100);

    // Assert
    // Verify scroll snap is no longer suppressed by checking behavior after scrolling
    for i in 0..35 {
        terminal.put_str(&format!("line {i}\r\n"));
    }
    terminal.scroll_viewport_up(5);
    let offset_before = terminal.screen().view_offset();
    assert!(offset_before > 0);

    // process_output should snap to bottom now since suppress_scroll_snap was reset to false
    terminal.process_output(b"new output\r\n");
    assert_eq!(terminal.screen().view_offset(), 0);
}

#[test]
fn should_reset_scroll_snap_suppression_when_resize_if_changed_modifies_dimensions() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    terminal.set_suppress_scroll_snap(true);

    // Act
    let changed = terminal.resize_if_changed(TerminalSize {
        rows: 30,
        cols: 100,
    });

    // Assert
    assert!(changed);
    // Verify scroll snap is reset by scrolling up and then processing output
    for i in 0..35 {
        terminal.put_str(&format!("line {i}\r\n"));
    }
    terminal.scroll_viewport_up(5);
    terminal.process_output(b"more output\r\n");
    assert_eq!(terminal.screen().view_offset(), 0);
}

fn wait_for_pty_wake(wake_rx: &std::sync::mpsc::Receiver<()>) {
    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should wake for queued output");
}

struct ScriptedReader {
    chunks: std::collections::VecDeque<Vec<u8>>,
}

impl std::io::Read for ScriptedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let Some(chunk) = self.chunks.pop_front() else {
            std::thread::park();
            return Ok(0);
        };
        buffer[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

#[derive(Clone)]
struct RecordingWriter {
    bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    max_write: usize,
}

impl std::io::Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let length = buffer.len().min(self.max_write);
        self.bytes
            .lock()
            .expect("recording writer lock poisoned")
            .extend_from_slice(&buffer[..length]);
        Ok(length)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn terminal_with_io<R>(
    reader: R,
) -> (
    Terminal,
    std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    std::sync::mpsc::Receiver<()>,
)
where
    R: std::io::Read + Send + 'static,
{
    let bytes = Default::default();
    let writer = RecordingWriter {
        bytes: std::sync::Arc::clone(&bytes),
        max_write: 2,
    };
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let terminal =
        Terminal::new_headless_with_io(2, 8, reader, writer, move || wake_tx.send(()).is_ok());
    (terminal, bytes, wake_rx)
}

struct CompletedScriptedReader {
    chunks: std::collections::VecDeque<Vec<u8>>,
    completed: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for CompletedScriptedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let Some(chunk) = self.chunks.pop_front() else {
            return Ok(0);
        };
        buffer[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

impl Drop for CompletedScriptedReader {
    fn drop(&mut self) {
        let _ = self.completed.send(());
    }
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

struct EofReader {
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    started: std::sync::mpsc::Sender<()>,
    exited: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for EofReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.started.send(());
        Ok(0)
    }
}

impl Drop for EofReader {
    fn drop(&mut self) {
        let _ = self.exited.send(());
    }
}

fn eof_reader() -> (
    EofReader,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::Receiver<()>,
) {
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (exited_tx, exited_rx) = std::sync::mpsc::channel();
    let reader = EofReader {
        reads: std::sync::Arc::clone(&reads),
        started: started_tx,
        exited: exited_tx,
    };
    (reader, reads, started_rx, exited_rx)
}

fn wait_for_reader_exit(exited_rx: &std::sync::mpsc::Receiver<()>) {
    exited_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader must drop before drain can observe disconnect");
}

struct BlockingThenChunkReader {
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    entered: std::sync::mpsc::Sender<()>,
    permit: std::sync::mpsc::Receiver<()>,
    completed: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for BlockingThenChunkReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.entered.send(());
        self.permit.recv().expect("test must release reader");
        buffer[..4].copy_from_slice(b"late");
        Ok(4)
    }
}

impl Drop for BlockingThenChunkReader {
    fn drop(&mut self) {
        let _ = self.completed.send(());
    }
}

struct FailingWriter;

impl std::io::Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("write failed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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

struct BurstReader {
    remaining: usize,
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    completed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl std::io::Read for BurstReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            self.completed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(0);
        }
        self.remaining -= 1;
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        buffer[0] = b'x';
        Ok(1)
    }
}

#[test]
fn pty_queue_is_bounded_and_wakes_once_until_drained() {
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
    // The final chunk may have arrived during the first drain or after it
    // re-armed the wake flag; either way the UI thread can drain it now.
    let _ = terminal.drain_pty();
    assert!(!terminal.drain_pty());
}

struct ErrorReader {
    started: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for ErrorReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.started.send(());
        Err(std::io::Error::other("read failed"))
    }
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

#[test]
fn ctrl_shift_uppercase_c_copies_even_without_a_selection() {
    let mut terminal = Terminal::new_headless(2, 4);

    let outcome = terminal
        .handle_event_with_outcome(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Character('C'),
            modifiers: TerminalModifiers {
                ctrl: true,
                shift: true,
                ..TerminalModifiers::default()
            },
        }))
        .expect("copy shortcut should be handled");

    assert_eq!(outcome.clipboard_text, Some(String::new()));
}

#[test]
fn bare_navigation_keys_scroll_viewport_on_normal_screen() {
    let reader = ScriptedReader {
        chunks: std::collections::VecDeque::new(),
    };
    let (mut terminal, written, _wake_rx) = terminal_with_io(reader);
    for _ in 0..12 {
        terminal.process_output(b"line\r\n");
    }
    let rows = terminal.screen().rows();
    let scroll_count = terminal.screen().scroll_count();
    assert!(scroll_count > rows);

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::PageUp,
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), rows);

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::PageDown,
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 0);

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::Home,
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), scroll_count);

    terminal
        .handle_event(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
            key: TerminalKey::End,
            modifiers: TerminalModifiers::default(),
        }))
        .unwrap();
    assert_eq!(terminal.screen().view_offset(), 0);
    assert!(written.lock().unwrap().is_empty());
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
    assert_eq!(demand, FrameDemand::empty());
    assert!(!demand.redraw_now);
    assert!(demand.deadline.is_none());
    assert!(demand.ordinary_present_eligible);
}

#[test]
fn should_keep_empty_demand_after_cursor_move_when_headless() {
    // Arrange — CUP moves the cursor; without a renderer reset is a no-op
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
    assert_eq!(demand, FrameDemand::empty());
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
    assert_eq!(demand, FrameDemand::empty());
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
    assert_eq!(demand, FrameDemand::empty());
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

// ── Backdrop availability ────────────────────────────────────────────

#[test]
fn should_default_backdrop_availability_to_false_when_headless() {
    let terminal = Terminal::new_headless(2, 4);

    assert!(!terminal.backdrop_available);
}

#[test]
fn should_record_backdrop_availability_through_the_setter() {
    let mut terminal = Terminal::new_headless(2, 4);

    terminal.set_backdrop_available(true);
    assert!(terminal.backdrop_available);

    terminal.set_backdrop_available(false);
    assert!(!terminal.backdrop_available);
}
