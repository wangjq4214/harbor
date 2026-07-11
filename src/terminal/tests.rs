//! Integration-style tests for the Terminal facade.

use super::Terminal;
use super::screen::CellAttrs;
use super::screen::Color;

#[test]
fn writes_plain_characters_and_tracks_cursor() {
    let mut terminal = Terminal::new(2, 4);

    terminal.put_str("ab");

    assert_eq!(terminal.row_text(0), "ab  ");
    assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 4));
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn newline_moves_to_next_row_start() {
    let mut terminal = Terminal::new(2, 4);

    terminal.put_str("a\nb");

    assert_eq!(terminal.row_text(0), "a   ");
    assert_eq!(terminal.row_text(1), "b   ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (1, 1)
    );
}

#[test]
fn carriage_return_overwrites_from_row_start() {
    let mut terminal = Terminal::new(1, 4);

    terminal.put_str("ab\rc");

    assert_eq!(terminal.row_text(0), "cb  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (1, 0)
    );
}

#[test]
fn backspace_is_non_destructive() {
    let mut terminal = Terminal::new(1, 4);

    terminal.put_str("ab\u{8}");

    assert_eq!(terminal.row_text(0), "ab  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (1, 0)
    );
}

#[test]
fn backspace_erases_previous_cell() {
    let mut terminal = Terminal::new(1, 4);

    terminal.put_str("ab\u{8}c");

    assert_eq!(terminal.row_text(0), "ac  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn scrolls_when_writing_past_last_row() {
    let mut terminal = Terminal::new(2, 4);

    terminal.put_str("one\ntwo\nthr");

    assert_eq!(terminal.row_text(0), "two ");
    assert_eq!(terminal.row_text(1), "thr ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (3, 1)
    );
}

#[test]
fn resize_preserves_visible_cells_and_clamps_cursor() {
    let mut terminal = Terminal::new(2, 4);
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
fn sgr_sets_fg_color_on_written_cells() {
    let mut terminal = Terminal::new(1, 8);

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
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[1ma");
    assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::BOLD));
}

#[test]
fn sgr_dim_sets_attr() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[2ma");
    assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::DIM));
}

#[test]
fn sgr_italic_sets_attr() {
    let mut terminal = Terminal::new(1, 4);
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
    let mut terminal = Terminal::new(1, 4);
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
    let mut terminal = Terminal::new(1, 4);
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
    let mut terminal = Terminal::new(1, 4);
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
    let mut terminal = Terminal::new(1, 4);
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
    let mut terminal = Terminal::new(1, 4);
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
        let mut terminal = Terminal::new(1, 2);
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
        let mut terminal = Terminal::new(1, 2);
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
        let mut terminal = Terminal::new(1, 2);
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
        let mut terminal = Terminal::new(1, 2);
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
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[38;5;200mb");
    assert_eq!(terminal.screen().cell(0, 0).fg, Color::Indexed(200));
}

#[test]
fn sgr_256color_bg_sets_indexed() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[48;5;100mb");
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Indexed(100));
}

#[test]
fn sgr_truecolor_fg_sets_rgb() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[38;2;10;20;30mb");
    assert_eq!(terminal.screen().cell(0, 0).fg, Color::Rgb(10, 20, 30));
}

#[test]
fn sgr_truecolor_bg_sets_rgb() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[48;2;100;150;200mb");
    assert_eq!(terminal.screen().cell(0, 0).bg, Color::Rgb(100, 150, 200));
}

#[test]
fn sgr_multi_param_sets_all() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[1;31;44ma");
    let cell = terminal.screen().cell(0, 0);
    assert!(cell.attrs.contains(CellAttrs::BOLD));
    assert_eq!(cell.fg, Color::Named(1));
    assert_eq!(cell.bg, Color::Named(4));
}

#[test]
fn sgr_default_fg_bg_resets_colors() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[31;42m\x1b[39;49mb");
    let cell = terminal.screen().cell(0, 0);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
}

#[test]
fn sgr_compound_clear_removes_attrs() {
    let mut terminal = Terminal::new(1, 4);
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
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[1;2ma\x1b[22mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::BOLD));
    assert!(!cell.attrs.contains(CellAttrs::DIM));
}

#[test]
fn sgr_24_clears_underline() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[4ma\x1b[24mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::UNDERLINE));
}

#[test]
fn sgr_25_clears_blink() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[5ma\x1b[25mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::BLINK));
}

#[test]
fn sgr_27_clears_inverse() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[7ma\x1b[27mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::INVERSE));
}

#[test]
fn sgr_29_clears_strikethrough() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[9ma\x1b[29mb");
    let cell = terminal.screen().cell(0, 1);
    assert!(!cell.attrs.contains(CellAttrs::STRIKETHROUGH));
}

#[test]
fn sgr_bare_csi_m_is_reset() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[1;31;42ma\x1b[mb");
    let cell = terminal.screen().cell(0, 1);
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
    assert!(cell.attrs.is_empty());
}

// ── SGR error handling / robustness ─────────────────────────────

#[test]
fn sgr_indexed_out_of_range_ignored() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;5;300mb");
    // 300 > 255 so fg should still be Named(1) from the 31 sequence
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}

#[test]
fn sgr_truecolor_missing_params_ignored() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;2;128;64mx");
    // Incomplete truecolor seq — fg stays red, 'x' still renders
    assert_eq!(terminal.row_text(0), "ax  ");
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}

#[test]
fn sgr_truecolor_component_out_of_range_ignored() {
    let mut terminal = Terminal::new(1, 4);
    terminal.put_bytes(b"\x1b[31ma");
    terminal.put_bytes(b"\x1b[38;2;300;0;0mb");
    // 300 > 255 — fg stays red
    assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
}
#[test]
fn csi_cursor_position_overwrites_target_cell() {
    let mut terminal = Terminal::new(2, 4);

    terminal.put_bytes(b"abcd\x1b[1;2HZ");

    assert_eq!(terminal.row_text(0), "aZcd");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn csi_erase_line_clears_selected_range() {
    let mut terminal = Terminal::new(2, 4);

    terminal.put_bytes(b"abcd\x1b[1;3H\x1b[K");

    assert_eq!(terminal.row_text(0), "ab  ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (2, 0)
    );
}

#[test]
fn csi_erase_display_mode_two_clears_and_homes() {
    let mut terminal = Terminal::new(2, 4);

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
    let mut terminal = Terminal::new(1, 5);

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
    let mut terminal = Terminal::new(1, 4);
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
    let mut terminal = Terminal::new(1, 4);

    terminal.put_str("中a");

    assert_eq!(terminal.row_text(0), "中 a ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (3, 0)
    );
}

#[test]
fn overwrites_both_cells_of_double_width_character() {
    let mut terminal = Terminal::new(1, 4);

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
    let mut terminal = Terminal::new(1, 4);

    terminal.put_str("中\u{8}");

    assert_eq!(terminal.row_text(0), "中   ");
    assert_eq!(
        (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
        (0, 0)
    );
}

#[test]
fn horizontal_tab_at_line_end_does_not_loop_forever() {
    let mut terminal = Terminal::new(2, 4);

    terminal.put_str("abc\tz");

    assert_eq!(terminal.row_text(0), "abc ");
    assert_eq!(terminal.row_text(1), "z   ");
}

#[test]
fn ignores_private_cursor_visibility_sequence() {
    let mut terminal = Terminal::new(1, 6);

    terminal.put_bytes(b"a\x1b[?25lb");

    assert_eq!(terminal.row_text(0), "ab    ");
}

#[test]
fn ignores_osc_title_sequence_terminated_by_bel() {
    let mut terminal = Terminal::new(1, 8);

    terminal.put_bytes(b"a\x1b]0;C:\\Windows\\system32\\cmd.exe\x07b");

    assert_eq!(terminal.row_text(0), "ab      ");
}

#[test]
fn keeps_incomplete_osc_sequence_across_chunks() {
    let mut terminal = Terminal::new(1, 8);

    terminal.put_bytes(b"a\x1b]0;title");
    terminal.put_bytes(b"\x1b\\b");

    assert_eq!(terminal.row_text(0), "ab      ");
}

#[test]
fn cargo_update_output_spans_multiple_rows() {
    // Replay the PTY output chunks logged during `cargo update`.
    let mut terminal = Terminal::new(5, 80);

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
    let mut terminal = Terminal::new(1, 20);
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
    let mut terminal = Terminal::new(3, 20);
    terminal.put_str("normal");
    assert_eq!(terminal.row_text(0).trim(), "normal");

    // Enter alt screen
    terminal.put_str("\x1b[?1049h");
    // Alt screen starts blank
    assert!(terminal.row_text(0).trim().is_empty());

    // Write to alt screen
    terminal.put_str("alt");
    assert_eq!(terminal.row_text(0).trim(), "alt");

    // Exit alt screen
    terminal.put_str("\x1b[?1049l");
    // Normal screen restored
    assert_eq!(terminal.row_text(0).trim(), "normal");
}

#[test]
fn alt_screen_enter_twice_is_idempotent() {
    let mut terminal = Terminal::new(3, 20);
    terminal.put_str("\x1b[?1049h");
    terminal.put_str("first");
    terminal.put_str("\x1b[?1049h"); // second enter — no-op
    assert_eq!(terminal.row_text(0).trim(), "first");
}

#[test]
fn alt_screen_exit_when_not_in_alt_is_noop() {
    let mut terminal = Terminal::new(3, 20);
    terminal.put_str("normal");
    terminal.put_str("\x1b[?1049l"); // exit without enter — no panic
    assert_eq!(terminal.row_text(0).trim(), "normal");
}

#[test]
fn alt_screen_switch_mid_batch_splits_correctly() {
    // Simulates PTY sending CSI ?1049h followed by content in one read.
    let mut terminal = Terminal::new(3, 20);
    terminal.put_str("before");
    terminal.put_bytes(b"\x1b[?1049hafter");
    // "before" stayed on normal screen, "after" landed on alt screen.
    assert_eq!(terminal.row_text(0).trim(), "after");

    terminal.put_str("\x1b[?1049l");
    assert_eq!(terminal.row_text(0).trim(), "before");
}

#[test]
fn alt_screen_resize_preserves_both_screens() {
    let mut terminal = Terminal::new(3, 20);
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

// ── ICH / DCH integration ───────────────────────────────────

#[test]
fn ich_via_csi_at_shifts_cells_right() {
    let mut terminal = Terminal::new(1, 8);
    terminal.put_str("abcdef");
    terminal.put_bytes(b"\x1b[1;3H"); // CUP: col 3 (0-based col 2)
    terminal.put_bytes(b"\x1b[2@"); // ICH 2
    assert_eq!(terminal.row_text(0), "ab  cdef");
}

#[test]
fn dch_via_csi_p_shifts_cells_left() {
    let mut terminal = Terminal::new(1, 8);
    terminal.put_str("abcdef");
    terminal.put_bytes(b"\x1b[1;3H"); // col 3
    terminal.put_bytes(b"\x1b[2P"); // DCH 2
    assert_eq!(terminal.row_text(0), "abef    ");
}

// ── IL / DL integration ─────────────────────────────────────

#[test]
fn il_via_csi_l_inserts_lines() {
    let mut terminal = Terminal::new(5, 4);
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
    let mut terminal = Terminal::new(5, 4);
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
    let mut terminal = Terminal::new(5, 4);
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
    let mut terminal = Terminal::new(5, 4);
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
    let mut terminal = Terminal::new(4, 4);
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
    let mut terminal = Terminal::new(5, 10);
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
    let mut terminal = Terminal::new(4, 10);
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
    let mut terminal = Terminal::new(5, 10);
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
    let mut terminal = Terminal::new(5, 10);
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

// ── SGR background + erase integration ────────────────────────

#[test]
fn sgr_bg_preserved_after_erase_line() {
    // Vim's pattern: set bg → write text → CSI K (erase to end of line)
    let mut terminal = Terminal::new(1, 6);
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
    let mut terminal = Terminal::new(2, 4);
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
    let mut terminal = Terminal::new(1, 4);
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
