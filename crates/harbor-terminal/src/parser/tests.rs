use super::*;
use crate::screen::{AltScreenAction, CursorShape, DefaultColorSlot, Screen};
use harbor_config::{Palette, Rgba};
use harbor_parser::Params;

fn feed(parser: &mut TerminalParser, screen: &mut Screen, seq: &[u8]) {
    parser.put_bytes(screen, seq);
}

fn feed_with_alt_transitions(parser: &mut TerminalParser, screen: &mut Screen, seq: &[u8]) {
    let mut remaining = seq;
    while !remaining.is_empty() {
        let result = parser.put_bytes(screen, remaining);
        remaining = &remaining[result.consumed..];
        match result.alt_request {
            Some(AltScreenAction::Enter { clear }) => screen.enter_alt(clear),
            Some(AltScreenAction::Exit) => screen.exit_alt(),
            None => {}
        }
    }
}

fn replies_for(query: &[u8]) -> Vec<u8> {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, query);
    screen.drain_replies()
}

/// Move cursor to (row, col) 1-based via `CSI row;col H`.
fn move_to(parser: &mut TerminalParser, screen: &mut Screen, row: usize, col: usize) {
    feed(parser, screen, format!("\x1b[{row};{col}H").as_bytes());
}

#[test]
fn combining_utf8_split_across_reads_extends_base_and_dirties_it() {
    use crate::SelectionBounds;
    let mut screen = Screen::new(2, 3);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"abc");
    screen.clear_dirty();
    feed(&mut parser, &mut screen, &[0xcc]);
    assert!(screen.dirty_ranges().is_empty());
    feed(&mut parser, &mut screen, &[0x81]);
    assert_eq!(screen.cell(0, 2).raw_text(), "c\u{0301}");
    assert_eq!((screen.cursor_x(), screen.cursor_y()), (2, 0));
    assert!(
        screen
            .dirty_ranges()
            .iter()
            .any(|r| r.row == 0 && r.start_col <= 2 && r.end_col > 2)
    );
    assert_eq!(
        screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 2
        }),
        "abc\u{0301}"
    );
    feed(&mut parser, &mut screen, b"d");
    assert_eq!(screen.cell(1, 0).raw_text(), "d");
}

#[test]
fn combining_mark_at_clamped_right_margin_extends_written_base() {
    let mut screen = Screen::new(1, 4);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?7labcd");
    feed(&mut parser, &mut screen, "\u{0301}".as_bytes());
    assert_eq!(screen.cell(0, 3).raw_text(), "d\u{0301}");
    assert_eq!(screen.cell(0, 2).raw_text(), "c");
    assert_eq!(screen.cursor_x(), 3);
    // After an ordinary advance *to* the occupied margin, the preceding
    // freshly written cell is still the attachment target.
    feed(&mut parser, &mut screen, b"\rabc");
    feed(&mut parser, &mut screen, "\u{0301}".as_bytes());
    assert_eq!(screen.cell(0, 2).raw_text(), "c\u{0301}");
    assert_eq!(screen.cell(0, 3).raw_text(), "d\u{0301}");
}

#[test]
fn zero_width_format_and_supplementary_selector_are_not_isolated_marks() {
    let mut screen = Screen::new(1, 4);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, "\u{200b}\u{e0100}".as_bytes());
    assert_eq!(screen.cursor_x(), 0);
    assert!(!screen.cell(0, 0).isolated_mark);
    feed(&mut parser, &mut screen, "e\u{0301}\u{200b}".as_bytes());
    assert_eq!(screen.cell(0, 0).raw_text(), "e\u{0301}");
}
#[test]
fn insert_before_combined_wide_unit_preserves_atomic_text_and_style() {
    use crate::SelectionBounds;
    let mut screen = Screen::new(1, 7);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, "ab界\u{0301}z".as_bytes());
    feed(&mut parser, &mut screen, b"\r\x1b[3G\x1b[@");
    assert_eq!(screen.cell(0, 3).raw_text(), "界\u{0301}");
    assert_eq!(screen.cell(0, 3).grid_width(), 2);
    assert!(screen.cell(0, 4).wide_continuation);
    assert_eq!(
        screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 6,
        }),
        "ab 界\u{0301}z"
    );
}

#[test]
fn isolated_mark_is_cued_but_copies_only_source_and_erases_whole_unit() {
    use crate::SelectionBounds;
    let mut screen = Screen::new(1, 4);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, "\u{0301}".as_bytes());
    assert!(screen.cell(0, 0).isolated_mark);
    assert_eq!(screen.cell(0, 0).width, 1);
    assert_eq!(
        screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0
        }),
        "\u{0301}"
    );
    feed(&mut parser, &mut screen, b"\r\x1b[X");
    assert_eq!(screen.cell(0, 0).raw_text(), " ");
    assert!(!screen.cell(0, 0).isolated_mark);
}
#[test]
fn combining_unit_survives_primary_reflow_and_alternate_rectangular_resize() {
    use crate::SelectionBounds;
    let mut screen = Screen::new(2, 5);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, "ab界\u{0301}z".as_bytes());
    assert_eq!(screen.cell(0, 2).raw_text(), "界\u{0301}");
    for cols in [3, 6, 5] {
        let prepared = screen.prepare_resize(2, cols).unwrap();
        screen.commit_resize(prepared);
        let retained: Vec<_> = (0..screen.rows())
            .flat_map(|row| (0..screen.cols()).map(move |col| (row, col)))
            .filter(|&(row, col)| screen.cell(row, col).ch == '界')
            .map(|(row, col)| screen.cell(row, col).raw_text())
            .collect();
        assert_eq!(retained, ["界\u{0301}"]);
    }
    assert!(
        screen
            .selected_text(SelectionBounds {
                start_row: screen.history_start(),
                start_col: 0,
                end_row: screen.history_start() + 1,
                end_col: 4
            })
            .contains("界\u{0301}")
    );
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049h");
    feed(&mut parser, &mut screen, "e\u{0301}".as_bytes());
    let prepared = screen.prepare_resize(2, 3).unwrap();
    screen.commit_resize(prepared);
    assert_eq!(screen.cell(0, 0).raw_text(), "e\u{0301}");
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049l");
}

#[test]
fn overwrite_and_erase_do_not_leave_detached_marks() {
    use crate::SelectionBounds;
    let mut screen = Screen::new(2, 5);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, "e\u{0301}界\u{0301}".as_bytes());
    assert_eq!(screen.cell(0, 1).width, 2);
    assert_eq!(screen.cell(0, 1).raw_text(), "界\u{0301}");
    feed(&mut parser, &mut screen, b"\rZ");
    assert_eq!(screen.cell(0, 0).raw_text(), "Z");
    assert_eq!(
        screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 2
        }),
        "Z界\u{0301}"
    );
    feed(&mut parser, &mut screen, b"\r\x1b[2G\x1b[2X");
    assert_eq!(screen.cell(0, 1).raw_text(), " ");
    assert_eq!(
        screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 3
        }),
        "Z"
    );
}

#[test]
fn decaln_fills_active_screen_and_preserves_saved_primary_screen() {
    let mut screen = Screen::new(2, 4);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"primary");
    let primary = screen.row_text(0);

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049h\x1b#8");
    assert_eq!(screen.row_text(0), "EEEE");
    assert_eq!(screen.row_text(1), "EEEE");
    assert_eq!((screen.cursor_x(), screen.cursor_y()), (0, 0));

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049l");
    assert_eq!(screen.row_text(0), primary);
}

#[test]
fn unknown_escape_intermediate_pair_is_consumed_without_visible_effect() {
    for sequence in [b"\x1b#9Z".as_slice(), b"\x1b##9Z".as_slice()] {
        let mut screen = Screen::new(2, 4);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"A");
        let before_cursor = (screen.cursor_x(), screen.cursor_y());

        feed(&mut parser, &mut screen, sequence);

        assert_eq!(screen.row_text(0), "AZ  ");
        assert_eq!(screen.cursor_y(), before_cursor.1);
        assert_eq!(screen.scroll_count(), 0);
    }
}

#[test]
fn oversized_param_skips_dispatch() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 4, 4);
    assert_eq!(screen.cursor_y(), 3);
    feed(&mut parser, &mut screen, b"\x1b[999999A");
    assert_eq!(screen.cursor_y(), 3, "oversized param should skip dispatch");
}

#[test]
fn normal_param_still_dispatches() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[5B");
    assert_eq!(screen.cursor_y(), 5);
}

#[test]
fn max_valid_param_dispatches_and_clamps() {
    let mut screen = Screen::new(100, 100);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[65535B");
    assert_eq!(
        screen.cursor_y(),
        screen.rows() - 1,
        "valid param at MAX should dispatch and clamp"
    );
}

#[test]
fn saturated_oversized_param_still_rejected() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 4, 4);
    feed(&mut parser, &mut screen, b"\x1b[99999999999999999999A");
    assert_eq!(
        screen.cursor_y(),
        3,
        "saturated oversized param should skip"
    );
}

#[test]
fn intermediate_byte_cancels_sequence() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 4, 4);
    feed(&mut parser, &mut screen, b"\x1b[!A");
    assert_eq!(screen.cursor_y(), 3, "intermediate byte should cancel CSI");
}

#[test]
fn private_markers_and_colons_parsed() {
    // Colons should not cancel: CSI : A dispatches and moves cursor up (y goes from 3 to 2)
    {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 4);
        feed(&mut parser, &mut screen, b"\x1b[:A");
        assert_eq!(
            screen.cursor_y(),
            2,
            "colon sub-parameter separator should not cancel"
        );
    }

    // Private markers < and > should set private flag and get ignored for CUU (y stays at 3)
    for &byte in b"<>" {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 4);
        let seq = [b'\x1b', b'[', byte, b'A'];
        feed(&mut parser, &mut screen, &seq);
        assert_eq!(
            screen.cursor_y(),
            3,
            "private marker 0x{:02x} should route to private ignore path",
            byte
        );
    }
}

#[test]
fn csi_overflow_limits_cancel_sequence() {
    // Sub-parameter count overflow (> MAX_SUBPARAMS = 8) -> malformed -> cancel (y stays 3)
    {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 4);
        feed(&mut parser, &mut screen, b"\x1b[1:2:3:4:5:6:7:8:9A");
        assert_eq!(
            screen.cursor_y(),
            3,
            "sub-parameter count overflow should cancel"
        );
    }

    // Intermediate count overflow (> MAX_INTERMEDIATES = 2) -> malformed -> cancel (y stays 3)
    {
        let mut screen = Screen::new(10, 10);
        let mut parser = TerminalParser::default();
        move_to(&mut parser, &mut screen, 4, 4);
        feed(&mut parser, &mut screen, b"\x1b[   A"); // 3 spaces -> 3 intermediates
        assert_eq!(
            screen.cursor_y(),
            3,
            "intermediate count overflow should cancel"
        );
    }
}
#[test]
fn many_empty_params_does_not_panic() {
    // 17 semicolons → 17 push_current calls → 16 fit, 17th triggers warn.
    let mut screen = Screen::new(5, 5);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 3, 3);
    feed(&mut parser, &mut screen, b"\x1b[;;;;;;;;;;;;;;;;;H");
    // Sequence still dispatches (empty params → defaults → cursor home).
    assert_eq!(screen.cursor_y(), 0, "overflow should not panic");
    assert_eq!(screen.cursor_x(), 0);
}

#[test]
fn empty_params_use_defaults() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 5, 5);
    feed(&mut parser, &mut screen, b"\x1b[;;;;H");
    assert_eq!(screen.cursor_y(), 0, "empty params should use defaults");
    assert_eq!(screen.cursor_x(), 0);
}

#[test]
fn decscusr_ps5_is_blinking_bar() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[5 q");
    assert_eq!(screen.cursor_shape(), CursorShape::Bar);
    assert!(screen.cursor_blink());
}

#[test]
fn decscusr_ps2_is_steady_block() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[2 q");
    assert_eq!(screen.cursor_shape(), CursorShape::Block);
    assert!(!screen.cursor_blink());
}

#[test]
fn decscusr_ps0_is_blinking_block() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[0 q");
    assert_eq!(screen.cursor_shape(), CursorShape::Block);
    assert!(screen.cursor_blink());
}

#[test]
fn decscusr_ps1_is_blinking_block() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[1 q");
    assert_eq!(screen.cursor_shape(), CursorShape::Block);
    assert!(screen.cursor_blink());
}

#[test]
fn initial_cursor_shape_is_block() {
    let screen = Screen::new(10, 10);
    assert_eq!(screen.cursor_shape(), CursorShape::Block);
    assert!(screen.cursor_blink());
}

// ── CHA (CSI n G) ─────────────────────────────────────────────

#[test]
fn cha_sets_column_keeps_row() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 3, 5);
    feed(&mut parser, &mut screen, b"\x1b[12G");
    assert_eq!(screen.cursor_y(), 2, "CHA should keep row unchanged");
    assert_eq!(screen.cursor_x(), 11, "CHA should set column (0-based)");
}

#[test]
fn cha_default_param_is_one() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 2, 8);
    feed(&mut parser, &mut screen, b"\x1b[G");
    assert_eq!(screen.cursor_x(), 0, "default CHA param = 1 → col 0");
}

#[test]
fn cha_clamps_to_cols() {
    let mut screen = Screen::new(5, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[999G");
    assert_eq!(screen.cursor_x(), 9, "CHA clamps to cols-1");
}

// ── VPA (CSI n d) ─────────────────────────────────────────────

#[test]
fn vpa_sets_row_keeps_col() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 5, 7);
    feed(&mut parser, &mut screen, b"\x1b[3d");
    assert_eq!(screen.cursor_y(), 2, "VPA should set row (0-based)");
    assert_eq!(screen.cursor_x(), 6, "VPA should keep col unchanged");
}

#[test]
fn vpa_default_param_is_one() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 5, 5);
    feed(&mut parser, &mut screen, b"\x1b[d");
    assert_eq!(screen.cursor_y(), 0, "default VPA param = 1 → row 0");
}

#[test]
fn vpa_clamps_to_rows() {
    let mut screen = Screen::new(5, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[999d");
    assert_eq!(screen.cursor_y(), 4, "VPA clamps to rows-1");
}

// ── HPA (CSI n `) ────────────────────────────────────────────

#[test]
fn hpa_sets_column_keeps_row() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 3, 5);
    feed(&mut parser, &mut screen, b"\x1b[12`");
    assert_eq!(screen.cursor_y(), 2, "HPA should keep row unchanged");
    assert_eq!(screen.cursor_x(), 11, "HPA should set column (0-based)");
}

#[test]
fn hpa_default_param_is_one() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 2, 8);
    feed(&mut parser, &mut screen, b"\x1b[`");
    assert_eq!(screen.cursor_x(), 0, "default HPA param = 1 → col 0");
}

#[test]
fn hpa_clamps_to_cols() {
    let mut screen = Screen::new(5, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[999`");
    assert_eq!(screen.cursor_x(), 9, "HPA clamps to cols-1");
}

// ── HPR (CSI n a) ────────────────────────────────────────────

#[test]
fn hpr_moves_right() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 3, 5);
    feed(&mut parser, &mut screen, b"\x1b[4a");
    assert_eq!(screen.cursor_y(), 2, "HPR should keep row unchanged");
    assert_eq!(screen.cursor_x(), 8, "HPR 4 from col 5 → col 9 (0-based 8)");
}

#[test]
fn hpr_default_param_is_one() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 2, 8);
    feed(&mut parser, &mut screen, b"\x1b[a");
    assert_eq!(
        screen.cursor_x(),
        8,
        "default HPR param = 1 → col 9 (0-based 8)"
    );
}

#[test]
fn hpr_clamps_to_cols() {
    let mut screen = Screen::new(5, 10);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 2, 3);
    feed(&mut parser, &mut screen, b"\x1b[999a");
    assert_eq!(screen.cursor_x(), 9, "HPR clamps to cols-1");
}

// ── VPR (CSI n e) ────────────────────────────────────────────

#[test]
fn vpr_moves_down() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 3, 5);
    feed(&mut parser, &mut screen, b"\x1b[3e");
    assert_eq!(screen.cursor_y(), 5, "VPR 3 from row 3 → row 6 (0-based 5)");
    assert_eq!(screen.cursor_x(), 4, "VPR should keep col unchanged");
}

#[test]
fn vpr_default_param_is_one() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 5, 5);
    feed(&mut parser, &mut screen, b"\x1b[e");
    assert_eq!(
        screen.cursor_y(),
        5,
        "default VPR param = 1 → row 6 (0-based 5)"
    );
}

#[test]
fn vpr_clamps_to_rows() {
    let mut screen = Screen::new(5, 10);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 2, 3);
    feed(&mut parser, &mut screen, b"\x1b[999e");
    assert_eq!(screen.cursor_y(), 4, "VPR clamps to rows-1");
}

// ── HPA / HPR / VPR: origin mode, margins, regions, pending wrap ──

#[test]
fn hpa_relative_to_margins_in_origin_mode() {
    let mut screen = Screen::new(5, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?69h"); // DECLRMM
    feed(&mut parser, &mut screen, b"\x1b[3;7s"); // margins cols 3..7 (1-based)
    feed(&mut parser, &mut screen, b"\x1b[?6h"); // DECOM
    feed(&mut parser, &mut screen, b"\x1b[3`");
    assert_eq!(
        screen.cursor_x(),
        4,
        "HPA col is relative to margin left (0-based 2 + 2)"
    );
    feed(&mut parser, &mut screen, b"\x1b[999`");
    assert_eq!(screen.cursor_x(), 6, "HPA clamps to margin right");
}

#[test]
fn hpr_clamps_to_right_margin() {
    let mut screen = Screen::new(5, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?69h");
    feed(&mut parser, &mut screen, b"\x1b[3;7s");
    move_to(&mut parser, &mut screen, 1, 4); // x = 3, inside margins
    feed(&mut parser, &mut screen, b"\x1b[999a");
    assert_eq!(
        screen.cursor_x(),
        6,
        "HPR clamps to right margin (0-based 6)"
    );
}

#[test]
fn vpr_clamps_to_scroll_region_bottom() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[2;6r"); // scroll region rows 2..6
    move_to(&mut parser, &mut screen, 2, 1); // y = 1, inside region
    feed(&mut parser, &mut screen, b"\x1b[999e");
    assert_eq!(
        screen.cursor_y(),
        5,
        "VPR clamps to scroll region bottom (0-based 5)"
    );
}

#[test]
fn hpa_clears_pending_wrap() {
    let mut screen = Screen::new(3, 5);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"abcde"); // pending wrap at last col
    feed(&mut parser, &mut screen, b"\x1b[2`");
    feed(&mut parser, &mut screen, b"Z");
    assert_eq!(
        screen.cursor_y(),
        0,
        "HPA must clear pending wrap so Z stays on row 0"
    );
    assert_eq!(screen.row_text(0), "aZcde");
}

#[test]
fn hpr_clears_pending_wrap() {
    let mut screen = Screen::new(3, 5);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"abcde");
    feed(&mut parser, &mut screen, b"\x1b[1a");
    feed(&mut parser, &mut screen, b"Z");
    assert_eq!(
        screen.cursor_y(),
        0,
        "HPR must clear pending wrap so Z stays on row 0"
    );
    assert_eq!(screen.row_text(0), "abcdZ");
}

#[test]
fn vpr_clears_pending_wrap() {
    let mut screen = Screen::new(3, 5);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"abcde");
    feed(&mut parser, &mut screen, b"\x1b[1e");
    feed(&mut parser, &mut screen, b"Z");
    assert_eq!(
        screen.cursor_y(),
        1,
        "VPR moves down and clears pending wrap"
    );
    assert_eq!(
        screen.cursor_x(),
        4,
        "Z prints at col 4, not wrapped to col 0"
    );
    assert_eq!(screen.row_text(1), "    Z");
}

// ── CNL (CSI n E) / CPL (CSI n F) ────────────────────────────

#[test]
fn cnl_moves_down_and_cr() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 3, 8);
    feed(&mut parser, &mut screen, b"\x1b[2E");
    assert_eq!(screen.cursor_y(), 4, "CNL 2 from row 3 → row 5 (0-based 4)");
    assert_eq!(screen.cursor_x(), 0, "CNL resets column to 0");
}

#[test]
fn cpl_moves_up_and_cr() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 6, 5);
    feed(&mut parser, &mut screen, b"\x1b[3F");
    assert_eq!(screen.cursor_y(), 2, "CPL 3 from row 6 → row 3 (0-based 2)");
    assert_eq!(screen.cursor_x(), 0, "CPL resets column to 0");
}

// ── SCP / RCP (CSI s / CSI u) ─────────────────────────────────

#[test]
fn csi_save_restore_cursor() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    move_to(&mut parser, &mut screen, 4, 6);
    feed(&mut parser, &mut screen, b"\x1b[s");
    move_to(&mut parser, &mut screen, 8, 12);
    feed(&mut parser, &mut screen, b"\x1b[u");
    assert_eq!(screen.cursor_y(), 3, "RCP should restore row");
    assert_eq!(screen.cursor_x(), 5, "RCP should restore col");
}

// ── DSR / CPR Terminal Replies (CSI 5 n / CSI 6 n / CSI ? 6 n) ──

#[test]
fn dsr_status_report_replies_ok() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[5n");
    let replies = screen.drain_replies();
    assert_eq!(replies, b"\x1b[0n");
}

#[test]
fn cpr_standard_report_absolute_coordinates() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    // Place cursor at 0-based (y=3, x=5) -> 1-based (row=4, col=6)
    move_to(&mut parser, &mut screen, 4, 6);
    feed(&mut parser, &mut screen, b"\x1b[6n");
    let replies = screen.drain_replies();
    assert_eq!(replies, b"\x1b[4;6R");
}

#[test]
fn cpr_private_report_absolute_coordinates() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    // Place cursor at 0-based (y=2, x=7) -> 1-based (row=3, col=8)
    move_to(&mut parser, &mut screen, 3, 8);
    feed(&mut parser, &mut screen, b"\x1b[?6n");
    let replies = screen.drain_replies();
    assert_eq!(replies, b"\x1b[?3;8R");
}

#[test]
fn cpr_with_decom_and_margins_combos() {
    let mut screen = Screen::new(20, 30);
    let mut parser = TerminalParser::default();

    // 1. Enable vertical scroll margins (rows 5 to 15, 1-based)
    feed(&mut parser, &mut screen, b"\x1b[5;15r");

    // 2. Enable left/right margins (cols 8 to 22, 1-based)
    // First enable Declrmm mode 69 (margin_mode)
    feed(&mut parser, &mut screen, b"\x1b[?69h");
    feed(&mut parser, &mut screen, b"\x1b[8;22s");

    // 3. Enable Origin Mode (DECOM, private mode 6)
    feed(&mut parser, &mut screen, b"\x1b[?6h");

    // Origin mode is now active, so cursor homed to top-left of margins (y=4, x=7, 0-based)
    assert_eq!(screen.cursor_y(), 4);
    assert_eq!(screen.cursor_x(), 7);

    // CPR should report (1, 1) relative to margin origins
    feed(&mut parser, &mut screen, b"\x1b[6n");
    assert_eq!(screen.drain_replies(), b"\x1b[1;1R");

    // Move cursor relative to margins (row 3, col 4 relative to margins, which is y=6, x=10 absolute)
    feed(&mut parser, &mut screen, b"\x1b[3;4H");
    assert_eq!(screen.cursor_y(), 6);
    assert_eq!(screen.cursor_x(), 10);

    // CPR should report relative coords (3, 4)
    feed(&mut parser, &mut screen, b"\x1b[6n");
    assert_eq!(screen.drain_replies(), b"\x1b[3;4R");

    // 4. Disable Left/Right Margin mode (Declrmm mode 69)
    feed(&mut parser, &mut screen, b"\x1b[?69l");

    // Disabling mode 69 homes the cursor (in DECOM -> y=scroll_region.top, x=left_margin=0)
    assert_eq!(screen.cursor_y(), 4);
    assert_eq!(screen.cursor_x(), 0);

    // Move cursor to relative (3, 11) -> absolute (y=6, x=10) since left margin is now 0.
    feed(&mut parser, &mut screen, b"\x1b[3;11H");
    assert_eq!(screen.cursor_y(), 6);
    assert_eq!(screen.cursor_x(), 10);

    // Now left/right margin mode is inactive, but DECOM is still active.
    // So vertical is relative (y=6 - top=4 + 1 = 3).
    // Horizontal is absolute (x=10 + 1 = 11).
    feed(&mut parser, &mut screen, b"\x1b[6n");
    assert_eq!(screen.drain_replies(), b"\x1b[3;11R");
}

#[test]
fn should_reply_with_primary_device_attributes_when_query_is_omitted() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(&mut parser, &mut screen, b"\x1b[c");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[?62;6;17;22;28c");
}

#[test]
fn should_reply_with_primary_device_attributes_when_query_parameter_is_zero() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(&mut parser, &mut screen, b"\x1b[0c");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[?62;6;17;22;28c");
}

#[test]
fn should_reply_with_secondary_device_attributes_when_query_is_omitted() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(&mut parser, &mut screen, b"\x1b[>c");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[>1;1;0c");
}

#[test]
fn should_reply_with_secondary_device_attributes_when_query_parameter_is_zero() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(&mut parser, &mut screen, b"\x1b[>0c");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[>1;1;0c");
}

#[test]
fn should_produce_no_reply_when_device_attributes_parameter_is_nonzero() {
    // Arrange
    let queries = [b"\x1b[1c".as_slice(), b"\x1b[>1c".as_slice()];

    // Act
    let replies = queries
        .iter()
        .map(|query| replies_for(query))
        .collect::<Vec<_>>();

    // Assert
    assert_eq!(replies, vec![Vec::new(), Vec::new()]);
}

#[test]
fn should_produce_no_reply_when_device_attributes_parameters_are_multiple() {
    // Arrange
    let queries = [b"\x1b[0;0c".as_slice(), b"\x1b[>0;0c".as_slice()];

    // Act
    let replies = queries
        .iter()
        .map(|query| replies_for(query))
        .collect::<Vec<_>>();

    // Assert
    assert_eq!(replies, vec![Vec::new(), Vec::new()]);
}

#[test]
fn should_produce_no_reply_when_device_attributes_parameter_has_subparameters() {
    // Arrange
    let queries = [b"\x1b[0:0c".as_slice(), b"\x1b[>0:0c".as_slice()];

    // Act
    let replies = queries
        .iter()
        .map(|query| replies_for(query))
        .collect::<Vec<_>>();

    // Assert
    assert_eq!(replies, vec![Vec::new(), Vec::new()]);
}

#[test]
fn should_produce_no_reply_when_device_attributes_query_is_malformed() {
    // Arrange
    let queries = [b"\x1b[999999c".as_slice(), b"\x1b[>999999c".as_slice()];

    // Act
    let replies = queries
        .iter()
        .map(|query| replies_for(query))
        .collect::<Vec<_>>();

    // Assert
    assert_eq!(replies, vec![Vec::new(), Vec::new()]);
}

#[test]
fn should_not_dispatch_late_or_duplicate_private_markers() {
    // Arrange
    let queries = [b"\x1b[0>c".as_slice(), b"\x1b[>>0c".as_slice()];

    // Act
    let replies = queries
        .iter()
        .map(|query| replies_for(query))
        .collect::<Vec<_>>();

    // Assert
    assert_eq!(replies, vec![Vec::new(), Vec::new()]);
}

#[test]
fn should_leave_screen_unchanged_when_tertiary_device_attributes_is_requested() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"visible");
    let before = (screen.row_text(0), screen.cursor_x(), screen.cursor_y());

    // Act
    feed(&mut parser, &mut screen, b"\x1b[=c");

    // Assert
    assert_eq!(screen.drain_replies(), b"");
    assert_eq!(
        (screen.row_text(0), screen.cursor_x(), screen.cursor_y()),
        before
    );
}

#[test]
fn should_produce_no_reply_when_unsupported_device_attributes_marker_is_used() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(&mut parser, &mut screen, b"\x1b[<c");

    // Assert
    assert_eq!(screen.drain_replies(), b"");
}

#[test]
fn should_match_primary_device_attributes_reply_when_decid_is_requested() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(&mut parser, &mut screen, b"\x1b[c");
    let primary_reply = screen.drain_replies();
    feed(&mut parser, &mut screen, b"\x1bZ");
    let decid_reply = screen.drain_replies();

    // Assert
    assert_eq!(decid_reply, primary_reply);
}

#[test]
fn should_produce_no_reply_when_eight_bit_device_attributes_is_received_in_default_mode() {
    // Arrange
    let queries = [
        b"\x9bc".as_slice(),
        b"\x9b>c".as_slice(),
        b"\x9b=c".as_slice(),
    ];

    // Act
    let replies = queries
        .iter()
        .map(|query| replies_for(query))
        .collect::<Vec<_>>();

    // Assert — 8-bit primary, secondary, and tertiary forms are unrecognized here.
    assert_eq!(replies, vec![Vec::new(), Vec::new(), Vec::new()]);
}

#[test]
fn should_ignore_late_or_repeated_markers_when_sequence_is_split_across_chunks() {
    // Arrange
    let cases = [
        (b"\x1b[0".as_slice(), b">cOK".as_slice()),
        (b"\x1b[>".as_slice(), b">0cOK".as_slice()),
    ];

    // Act
    let outcomes = cases
        .iter()
        .map(|(first, second)| {
            let mut screen = Screen::new(10, 20);
            let mut parser = TerminalParser::default();
            feed(&mut parser, &mut screen, first);
            feed(&mut parser, &mut screen, second);
            (screen.drain_replies(), screen.row_text(0))
        })
        .collect::<Vec<_>>();

    // Assert — each malformed query is consumed without a reply and text recovers.
    assert_eq!(
        outcomes,
        vec![(Vec::new(), "OK                  ".to_owned()); 2]
    );
}

#[test]
fn should_treat_eight_bit_device_attributes_as_unrecognized_and_resume_text() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    let c1_csi = [0x9b];

    // Act — feed the unrecognized 8-bit introducer separately from its final byte.
    feed(&mut parser, &mut screen, &c1_csi);
    feed(&mut parser, &mut screen, b"cVISIBLE");

    // Assert — no DA reply is generated and subsequent printable input is preserved.
    assert_eq!(screen.drain_replies(), Vec::new());
    assert!(screen.row_text(0).contains("cVISIBLE"));
}

#[test]
fn osc_color_reply_is_atomic_at_capacity_boundary() {
    let reply = b"\x1b]10;rgb:ffff/ffff/ffff\x07";
    for (remaining, accepted) in [(reply.len(), true), (reply.len() - 1, false)] {
        let prefix = vec![b'x'; 1024 - remaining];
        let mut screen = Screen::new(2, 4);
        let mut parser = TerminalParser::default();
        screen.push_reply(&prefix);

        feed(&mut parser, &mut screen, b"\x1b]10;?\x07");

        let replies = screen.drain_replies();
        if accepted {
            assert_eq!(replies.len(), 1024);
            assert_eq!(&replies[prefix.len()..], reply);
        } else {
            assert_eq!(replies, prefix);
        }
    }
}

#[test]
fn should_accept_primary_reply_when_buffer_has_exact_remaining_capacity() {
    // Arrange
    let reply = b"\x1b[?62;6;17;22;28c";
    let prefix = vec![b'x'; 1024 - reply.len()];
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    screen.push_reply(&prefix);

    // Act
    feed(&mut parser, &mut screen, b"\x1b[c");

    // Assert
    let replies = screen.drain_replies();
    assert_eq!(replies.len(), 1024);
    assert_eq!(&replies[..prefix.len()], prefix.as_slice());
    assert_eq!(&replies[prefix.len()..], reply);
}

#[test]
fn should_drop_primary_reply_when_buffer_is_one_byte_short() {
    // Arrange
    let reply = b"\x1b[?62;6;17;22;28c";
    let prefix = vec![b'x'; 1024 - reply.len() + 1];
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    screen.push_reply(&prefix);

    // Act
    feed(&mut parser, &mut screen, b"\x1b[c");

    // Assert
    assert_eq!(screen.drain_replies(), prefix);
}

#[test]
fn replies_buffer_exceeding_cap_discards() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    // Send DSR queries repeatedly to fill up the buffer.
    // Each reply to CSI 5 n is "\x1b[0n" (4 bytes).
    // 1024 / 4 = 256 replies can fit. The 257th reply should be discarded.
    let mut sequence = Vec::new();
    for _ in 0..300 {
        sequence.extend_from_slice(b"\x1b[5n");
    }
    feed(&mut parser, &mut screen, &sequence);
    let replies = screen.drain_replies();
    assert_eq!(replies.len(), 1024);
    assert_eq!(screen.drain_replies().len(), 0);
}

// ── DECRQSS (DCS $ q Pt ST) ─────────────────────────────────────────────

#[derive(Default)]
struct DcsRecorder {
    params: Option<Params>,
    intermediates: Vec<u8>,
    action: Option<u8>,
    payload: Vec<u8>,
    terminated: Option<bool>,
    hook_count: usize,
    put_count: usize,
    unhook_count: usize,
}

impl harbor_parser::VtHandler for DcsRecorder {
    fn print(&mut self, _ch: char) {}
    fn execute(&mut self, _byte: u8) {}
    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _action: u8,
        _private_marker: Option<u8>,
    ) {
    }
    fn esc_dispatch(&mut self, _intermediates: &[u8], _byte: u8) {}
    fn osc_dispatch(&mut self, _command: &[u8], _payload: &[u8], _bell_terminated: bool) {}
    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
        self.hook_count += 1;
        self.params = Some(*params);
        self.intermediates = intermediates.to_vec();
        self.action = Some(action);
    }
    fn dcs_put(&mut self, byte: u8) {
        self.put_count += 1;
        self.payload.push(byte);
    }
    fn dcs_unhook(&mut self, terminated: bool) {
        self.unhook_count += 1;
        self.terminated = Some(terminated);
    }
    fn start_string(&mut self, _kind: u8) {}
}

fn assert_decrqss_round_trip(reply: &[u8], expected_ps: usize, expected_payload: &[u8]) {
    let mut parser = harbor_parser::Parser::default();
    let mut recorder = DcsRecorder::default();
    for &byte in reply {
        parser.advance(&mut recorder, byte);
    }
    assert_eq!(recorder.hook_count, 1);
    assert_eq!(recorder.unhook_count, 1);
    assert_eq!(recorder.terminated, Some(true));
    assert_eq!(recorder.action, Some(b'r'));
    assert_eq!(recorder.intermediates, b"$");
    let params = recorder.params.expect("DCS hook params");
    assert_eq!(params.len(), 1);
    assert_eq!(params.get(0), Some(expected_ps));
    assert_eq!(recorder.payload, expected_payload);
}

fn decrqss(pt: &[u8]) -> Vec<u8> {
    let mut req = b"\x1bP$q".to_vec();
    req.extend_from_slice(pt);
    req.extend_from_slice(b"\x1b\\");
    req
}

fn success_reply(status: &[u8]) -> Vec<u8> {
    let mut reply = b"\x1bP1$r".to_vec();
    reply.extend_from_slice(status);
    reply.extend_from_slice(b"\x1b\\");
    reply
}

const FAILURE_REPLY: &[u8] = b"\x1bP0$r\x1b\\";

#[test]
fn decrqss_default_sgr_returns_reset() {
    let reply = replies_for(&decrqss(b"m"));
    assert_eq!(reply, success_reply(b"0m"));
    assert_decrqss_round_trip(&reply, 1, b"0m");
}

#[test]
fn decrqss_sgr_reports_attrs_and_color_families() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(
        &mut parser,
        &mut screen,
        b"\x1b[1;2;3;4;5;7;9;38;2;1;2;3;48;5;9m",
    );
    feed(&mut parser, &mut screen, &decrqss(b"m"));
    let reply = screen.drain_replies();
    assert_eq!(reply, success_reply(b"0;1;2;3;4;5;7;9;38;2;1;2;3;48;5;9m"));
    assert_decrqss_round_trip(&reply, 1, b"0;1;2;3;4;5;7;9;38;2;1;2;3;48;5;9m");
}

#[test]
fn decrqss_reports_regions_margins_cursor_and_protection() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    feed(&mut parser, &mut screen, b"\x1b[2;8r");
    feed(&mut parser, &mut screen, &decrqss(b"r"));
    let reply = screen.drain_replies();
    assert_eq!(reply, success_reply(b"2;8r"));
    assert_decrqss_round_trip(&reply, 1, b"2;8r");

    feed(&mut parser, &mut screen, b"\x1b[?69h\x1b[3;15s");
    feed(&mut parser, &mut screen, &decrqss(b"s"));
    let reply = screen.drain_replies();
    assert_eq!(reply, success_reply(b"3;15s"));

    // Saved margins remain queryable after mode 69 is disabled.
    feed(&mut parser, &mut screen, b"\x1b[?69l");
    feed(&mut parser, &mut screen, &decrqss(b"s"));
    assert_eq!(screen.drain_replies(), success_reply(b"3;15s"));

    for (seq, status) in [
        (b"\x1b[1 q".as_slice(), b"1 q".as_slice()),
        (b"\x1b[2 q", b"2 q"),
        (b"\x1b[3 q", b"3 q"),
        (b"\x1b[4 q", b"4 q"),
        (b"\x1b[5 q", b"5 q"),
        (b"\x1b[6 q", b"6 q"),
    ] {
        feed(&mut parser, &mut screen, seq);
        feed(&mut parser, &mut screen, &decrqss(b" q"));
        assert_eq!(screen.drain_replies(), success_reply(status));
    }

    feed(&mut parser, &mut screen, b"\x1b[1\"q");
    feed(&mut parser, &mut screen, &decrqss(b"\"q"));
    assert_eq!(screen.drain_replies(), success_reply(b"1\"q"));
    feed(&mut parser, &mut screen, b"\x1b[0\"q");
    feed(&mut parser, &mut screen, &decrqss(b"\"q"));
    assert_eq!(screen.drain_replies(), success_reply(b"0\"q"));
}

#[test]
fn decrqss_unsupported_or_empty_pt_returns_failure_without_state_change() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[1;4;31m\x1b[2;8r\x1b[5 q");
    let before_shape = screen.cursor_shape();
    let before_blink = screen.cursor_blink();
    let before_x = screen.cursor_x();
    let before_y = screen.cursor_y();

    for pt in [b"".as_slice(), b"x".as_slice(), b"mm".as_slice()] {
        feed(&mut parser, &mut screen, &decrqss(pt));
        let reply = screen.drain_replies();
        assert_eq!(reply, FAILURE_REPLY);
        assert_decrqss_round_trip(&reply, 0, b"");
        assert_eq!(screen.cursor_shape(), before_shape);
        assert_eq!(screen.cursor_blink(), before_blink);
        assert_eq!(screen.cursor_x(), before_x);
        assert_eq!(screen.cursor_y(), before_y);
    }
}

#[test]
fn decrqss_nonmatching_dcs_is_consume_only() {
    let reply = replies_for(b"\x1bP1$q m\x1b\\OK");
    assert_eq!(reply, Vec::<u8>::new());
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1bPqpayload\x1b\\Z");
    assert_eq!(screen.drain_replies(), Vec::<u8>::new());
    assert!(screen.row_text(0).contains('Z'));
}

#[test]
fn decrqss_cancellation_produces_no_reply() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1bP$qm\x18OK");
    assert_eq!(screen.drain_replies(), Vec::<u8>::new());
    assert!(screen.row_text(0).contains("OK"));

    feed(&mut parser, &mut screen, b"\x1bP$qm\x1aOK");
    assert_eq!(screen.drain_replies(), Vec::<u8>::new());
}

#[test]
fn decrqss_pt_overflow_and_followup_query_do_not_succeed_from_prefix() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    let mut oversized = b"\x1bP$q".to_vec();
    oversized.extend(std::iter::repeat_n(b'm', 17));
    oversized.extend_from_slice(b"\x1b\\");
    feed(&mut parser, &mut screen, &oversized);
    assert_eq!(screen.drain_replies(), FAILURE_REPLY);

    // A later DECRQSS after failure still succeeds from live state, not the truncated Pt.
    feed(&mut parser, &mut screen, &decrqss(b"m"));
    assert_eq!(screen.drain_replies(), success_reply(b"0m"));
}

#[test]
fn decrqss_chunking_matches_bulk_including_split_st() {
    let request = decrqss(b"m");
    let expected = success_reply(b"0m");

    for chunk in [1usize, 2, 3, 7] {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        for part in request.chunks(chunk) {
            feed(&mut parser, &mut screen, part);
        }
        assert_eq!(screen.drain_replies(), expected);
    }

    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    let split_at = request.len() - 1; // before final '\\'
    feed(&mut parser, &mut screen, &request[..split_at]);
    feed(&mut parser, &mut screen, &request[split_at..]);
    assert_eq!(screen.drain_replies(), expected);
}

#[test]
fn decrqss_apc_isolation_and_reply_capacity() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    // Cancel an in-flight DECRQSS, then run APC — no delayed reply may appear.
    feed(&mut parser, &mut screen, b"\x1bP$qm\x18\x1b_payload\x1b\\");
    assert_eq!(screen.drain_replies(), Vec::<u8>::new());

    let reply = success_reply(b"0m");
    let prefix = vec![b'x'; 1024 - reply.len()];
    screen.push_reply(&prefix);
    feed(&mut parser, &mut screen, &decrqss(b"m"));
    let replies = screen.drain_replies();
    assert_eq!(&replies[prefix.len()..], reply.as_slice());

    let prefix = vec![b'x'; 1024 - reply.len() + 1];
    screen.push_reply(&prefix);
    feed(&mut parser, &mut screen, &decrqss(b"m"));
    assert_eq!(screen.drain_replies(), prefix);
}

#[test]
fn decrqss_pm_sos_isolation_and_request_replacement() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    for intro in [
        b"\x1b^payload\x1b\\".as_slice(),
        b"\x1bXpayload\x1b\\".as_slice(),
    ] {
        feed(&mut parser, &mut screen, b"\x1bP$qm\x18");
        feed(&mut parser, &mut screen, intro);
        assert_eq!(screen.drain_replies(), Vec::<u8>::new());
    }

    // Cancelled mid-flight request is fully replaced by a later successful DECRQSS.
    feed(&mut parser, &mut screen, b"\x1bP$qx\x18");
    feed(&mut parser, &mut screen, &decrqss(b"m"));
    assert_eq!(screen.drain_replies(), success_reply(b"0m"));
}

#[test]
fn decrqss_parser_string_cap_overflow_still_fails() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    let mut oversized = b"\x1bP$q".to_vec();
    oversized.extend(std::iter::repeat_n(b'a', 5000));
    oversized.extend_from_slice(b"\x1b\\OK");
    feed(&mut parser, &mut screen, &oversized);
    assert_eq!(screen.drain_replies(), FAILURE_REPLY);
    assert!(screen.row_text(0).contains("OK"));
}

// ── XTGETTCAP (DCS + q Pt ST) ──────────────────────────────────────────

fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn xtgettcap(names: &[&str]) -> Vec<u8> {
    let mut request = b"\x1bP+q".to_vec();
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            request.push(b';');
        }
        request.extend_from_slice(hex_upper(name.as_bytes()).as_bytes());
    }
    request.extend_from_slice(b"\x1b\\");
    request
}

const XTGETTCAP_FAILURE_REPLY: &[u8] = b"\x1bP0+r\x1b\\";

fn assert_xtgettcap_round_trip(reply: &[u8], expected_ps: usize, expected_payload: &[u8]) {
    let mut parser = harbor_parser::Parser::default();
    let mut recorder = DcsRecorder::default();
    for &byte in reply {
        parser.advance(&mut recorder, byte);
    }
    assert_eq!(recorder.hook_count, 1);
    assert_eq!(recorder.unhook_count, 1);
    assert_eq!(recorder.terminated, Some(true));
    assert_eq!(recorder.action, Some(b'r'));
    assert_eq!(recorder.intermediates, b"+");
    let params = recorder.params.expect("DCS hook params");
    assert_eq!(params.len(), 1);
    assert_eq!(params.get(0), Some(expected_ps));
    assert_eq!(recorder.payload, expected_payload);
}

#[test]
fn xtgettcap_reports_supported_string_capabilities() {
    let reply = replies_for(&xtgettcap(&["TN"]));
    assert_eq!(reply, b"\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\");
    assert_xtgettcap_round_trip(&reply, 1, b"544E=787465726D2D323536636F6C6F72");

    let reply = replies_for(&xtgettcap(&["RGB"]));
    assert_eq!(reply, b"\x1bP1+r524742=382F382F38\x1b\\");
    assert_xtgettcap_round_trip(&reply, 1, b"524742=382F382F38");
}

#[test]
fn xtgettcap_reports_boolean_capability_by_name_only() {
    let reply = replies_for(&xtgettcap(&["u8"]));
    assert_eq!(reply, b"\x1bP1+r7538\x1b\\");
    assert_xtgettcap_round_trip(&reply, 1, b"7538");
}

#[test]
fn xtgettcap_answers_multi_capability_queries_in_order() {
    let reply = replies_for(&xtgettcap(&["TN", "RGB", "u8"]));
    assert_eq!(
        reply,
        b"\x1bP1+r544E=787465726D2D323536636F6C6F72;524742=382F382F38;7538\x1b\\"
    );
    assert_xtgettcap_round_trip(
        &reply,
        1,
        b"544E=787465726D2D323536636F6C6F72;524742=382F382F38;7538",
    );
}

#[test]
fn xtgettcap_empty_or_unknown_query_returns_failure() {
    for names in [&[][..], &["xx"][..], &["colours"][..]] {
        let reply = replies_for(&xtgettcap(names));
        assert_eq!(reply, XTGETTCAP_FAILURE_REPLY);
        assert_xtgettcap_round_trip(&reply, 0, b"");
    }
}

#[test]
fn xtgettcap_mixed_query_skips_unknown_names() {
    let reply = replies_for(&xtgettcap(&["xx", "RGB", "yy"]));
    assert_eq!(reply, b"\x1bP1+r524742=382F382F38\x1b\\");
    assert_xtgettcap_round_trip(&reply, 1, b"524742=382F382F38");
}

#[test]
fn xtgettcap_malformed_hex_is_skipped_without_panic() {
    // Odd-length segment, non-hex digit, and empty segment from a trailing ';'.
    let mut request = b"\x1bP+q".to_vec();
    request.extend_from_slice(b"524;Z4;544E;");
    request.extend_from_slice(b"\x1b\\");
    let reply = replies_for(&request);
    assert_eq!(reply, b"\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\");
}

#[test]
fn xtgettcap_cancellation_produces_no_reply() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1bP+q544E\x18OK");
    assert_eq!(screen.drain_replies(), Vec::<u8>::new());
    assert!(screen.row_text(0).contains("OK"));

    feed(&mut parser, &mut screen, b"\x1bP+q544E\x1aOK");
    assert_eq!(screen.drain_replies(), Vec::<u8>::new());
}

#[test]
fn xtgettcap_pt_overflow_returns_failure() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    let mut oversized = b"\x1bP+q".to_vec();
    oversized.extend(std::iter::repeat_n(b'5', 257));
    oversized.extend_from_slice(b"\x1b\\");
    feed(&mut parser, &mut screen, &oversized);
    assert_eq!(screen.drain_replies(), XTGETTCAP_FAILURE_REPLY);
}

#[test]
fn xtgettcap_chunking_matches_bulk_including_split_st() {
    let request = xtgettcap(&["TN", "RGB", "u8"]);
    let expected = b"\x1bP1+r544E=787465726D2D323536636F6C6F72;524742=382F382F38;7538\x1b\\";

    for chunk in [1usize, 2, 3, 7] {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        for part in request.chunks(chunk) {
            feed(&mut parser, &mut screen, part);
        }
        assert_eq!(screen.drain_replies(), expected);
    }

    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    let split_at = request.len() - 1; // before final '\\'
    feed(&mut parser, &mut screen, &request[..split_at]);
    feed(&mut parser, &mut screen, &request[split_at..]);
    assert_eq!(screen.drain_replies(), expected);
}

#[test]
fn xtgettcap_apc_isolation_and_reply_capacity() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    // Cancel an in-flight XTGETTCAP, then run APC — no delayed reply may appear.
    feed(
        &mut parser,
        &mut screen,
        b"\x1bP+q544E\x18\x1b_payload\x1b\\",
    );
    assert_eq!(screen.drain_replies(), Vec::<u8>::new());

    let reply = b"\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\";
    let prefix = vec![b'x'; 1024 - reply.len()];
    screen.push_reply(&prefix);
    feed(&mut parser, &mut screen, &xtgettcap(&["TN"]));
    let replies = screen.drain_replies();
    assert_eq!(&replies[prefix.len()..], reply.as_slice());

    let prefix = vec![b'x'; 1024 - reply.len() + 1];
    screen.push_reply(&prefix);
    feed(&mut parser, &mut screen, &xtgettcap(&["TN"]));
    assert_eq!(screen.drain_replies(), prefix);
}

#[test]
fn xtgettcap_accepts_lowercase_hex_query_names() {
    // Arrange — TN and RGB requested with lowercase hex digits.
    let request = b"\x1bP+q544e;524742\x1b\\";

    // Act
    let reply = replies_for(request);

    // Assert — decoding is case-insensitive; the reply is always uppercase hex.
    assert_eq!(
        reply,
        b"\x1bP1+r544E=787465726D2D323536636F6C6F72;524742=382F382F38\x1b\\"
    );
}

#[test]
fn xtgettcap_sequential_queries_are_independent() {
    // Arrange — two complete requests back to back in a single stream.
    let mut stream = xtgettcap(&["TN"]);
    stream.extend_from_slice(&xtgettcap(&["RGB"]));

    // Act
    let reply = replies_for(&stream);

    // Assert — two exact frames; the first request's payload must not leak.
    let tn = b"\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\";
    let rgb = b"\x1bP1+r524742=382F382F38\x1b\\";
    let mut expected = tn.to_vec();
    expected.extend_from_slice(rgb);
    assert_eq!(reply, expected);
}

#[test]
fn xtgettcap_overflow_failure_does_not_poison_next_request() {
    // Arrange — a parser that just answered an overflowing request.
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    let mut oversized = b"\x1bP+q".to_vec();
    oversized.extend(std::iter::repeat_n(b'5', 257));
    oversized.extend_from_slice(b"\x1b\\");
    feed(&mut parser, &mut screen, &oversized);
    assert_eq!(screen.drain_replies(), XTGETTCAP_FAILURE_REPLY);

    // Act — a follow-up request on the same parser.
    feed(&mut parser, &mut screen, &xtgettcap(&["TN"]));

    // Assert — the overflow flag was reset; the reply is a clean success frame.
    assert_eq!(
        screen.drain_replies(),
        b"\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\"
    );
}

#[test]
fn xtgettcap_reply_cap_falls_back_when_many_names_are_found() {
    // Arrange — 7 × TN frames to 244 bytes (under MAX_REPLY), 8 × TN to 278 (over).
    let fits = xtgettcap(&["TN"; 7]);
    let exceeds = xtgettcap(&["TN"; 8]);

    // Act
    let reply = replies_for(&fits);
    let rejected = replies_for(&exceeds);

    // Assert — the success frame is kept up to the cap; past it, failure.
    let entry = b"544E=787465726D2D323536636F6C6F72";
    let mut expected = entry.to_vec();
    for _ in 1..7 {
        expected.push(b';');
        expected.extend_from_slice(entry);
    }
    assert_xtgettcap_round_trip(&reply, 1, &expected);
    assert_eq!(rejected, XTGETTCAP_FAILURE_REPLY);
    assert_xtgettcap_round_trip(&rejected, 0, b"");
}

#[test]
fn xtgettcap_embedded_escape_does_not_start_a_new_dcs() {
    // Arrange — ESC P appears inside the DCS payload (not a valid ST terminator).
    let stream = b"\x1bP+q544E\x1bP$qm\x1b\\";

    // Act
    let reply = replies_for(stream);

    // Assert — the embedded escape bytes become payload; garbled hex yields one
    // failure frame and no DECRQSS reply.
    assert_eq!(reply, XTGETTCAP_FAILURE_REPLY);
}

// ── DECRQM / DECRPM (CSI Ps $ p / CSI Ps ; Ps $ y) ─────────────────────

#[test]
fn should_report_standard_mode_states() {
    for &(param, default) in &[(4, 2), (20, 2)] {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[{param}$p").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1b[{param};{default}$y").into_bytes()
        );

        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[{param}h").as_bytes(),
        );
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[{param}$p").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1b[{param};1$y").into_bytes()
        );

        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[{param}l").as_bytes(),
        );
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[{param}$p").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1b[{param};2$y").into_bytes()
        );
    }
}

#[test]
fn should_report_private_mode_states_with_private_marker() {
    for &(param, default) in &[(1, 2), (6, 2), (7, 1), (25, 1), (66, 2), (69, 2), (2004, 2)] {
        let mut screen = Screen::new(10, 20);
        let mut parser = TerminalParser::default();
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[?{param}$p").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1b[?{param};{default}$y").into_bytes()
        );

        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[?{param}h").as_bytes(),
        );
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[?{param}$p").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1b[?{param};1$y").into_bytes()
        );
    }

    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049h");
    feed(&mut parser, &mut screen, b"\x1b[?1049$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1049;1$y");
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049l");
    feed(&mut parser, &mut screen, b"\x1b[?1049$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1049;2$y");
}
#[test]
fn should_track_mouse_modes_independently_across_multi_parameter_changes_and_ris() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    feed(
        &mut parser,
        &mut screen,
        b"\x1b[?1002;1000;1003;1006h\x1b[?1000$p\x1b[?1002$p\x1b[?1003$p\x1b[?1006$p",
    );
    assert_eq!(
        screen.drain_replies(),
        b"\x1b[?1000;1$y\x1b[?1002;1$y\x1b[?1003;1$y\x1b[?1006;1$y"
    );
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::AnyMotion
    );

    feed(
        &mut parser,
        &mut screen,
        b"\x1b[?1003;1006l\x1b[?1000$p\x1b[?1002$p\x1b[?1003$p\x1b[?1006$p",
    );
    assert_eq!(
        screen.drain_replies(),
        b"\x1b[?1000;1$y\x1b[?1002;1$y\x1b[?1003;2$y\x1b[?1006;2$y"
    );
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::ButtonMotion
    );

    feed(
        &mut parser,
        &mut screen,
        b"\x1bc\x1b[?1000$p\x1b[?1002$p\x1b[?1003$p\x1b[?1006$p",
    );
    assert_eq!(
        screen.drain_replies(),
        b"\x1b[?1000;2$y\x1b[?1002;2$y\x1b[?1003;2$y\x1b[?1006;2$y"
    );
    assert_eq!(screen.input_modes(), crate::InputModes::default());
}

#[test]
fn should_report_alt_screen_family_mode_states() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    for param in [47usize, 1047, 1049] {
        feed_with_alt_transitions(
            &mut parser,
            &mut screen,
            format!("\x1b[?{param}h").as_bytes(),
        );
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[?{param}$p").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1b[?{param};1$y").into_bytes(),
            "{param} should report Set while the alternate screen is active"
        );
        feed_with_alt_transitions(
            &mut parser,
            &mut screen,
            format!("\x1b[?{param}l").as_bytes(),
        );
        feed(
            &mut parser,
            &mut screen,
            format!("\x1b[?{param}$p").as_bytes(),
        );
        assert_eq!(
            screen.drain_replies(),
            format!("\x1b[?{param};2$y").into_bytes(),
            "{param} should report Reset after exit"
        );
    }
}

#[test]
fn should_report_1048_cursor_save_state() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    feed(&mut parser, &mut screen, b"\x1b[?1048$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1048;2$y");

    feed(&mut parser, &mut screen, b"\x1b[?1048h");
    feed(&mut parser, &mut screen, b"\x1b[?1048$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1048;1$y");

    // DECRC keeps the snapshot slot (matching DECSC/DECRC semantics).
    feed(&mut parser, &mut screen, b"\x1b[?1048l");
    feed(&mut parser, &mut screen, b"\x1b[?1048$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1048;1$y");

    // RIS clears the snapshot slot.
    feed(&mut parser, &mut screen, b"\x1bc");
    feed(&mut parser, &mut screen, b"\x1b[?1048$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1048;2$y");
}

#[test]
fn should_persist_47_contents_across_exit_and_reentry() {
    let mut screen = Screen::new(3, 20);
    let mut parser = TerminalParser::default();

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?47halt1\x1b[?47l");
    assert_eq!(screen.row_text(0).trim(), "", "primary screen is untouched");

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?47h");
    assert_eq!(
        screen.row_text(0).trim(),
        "alt1",
        "?47 re-entry restores the parked alternate contents"
    );
}

#[test]
fn should_clear_1047_contents_on_every_entry() {
    let mut screen = Screen::new(3, 20);
    let mut parser = TerminalParser::default();

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1047halt1\x1b[?1047l");
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1047h");
    assert!(
        screen.row_text(0).trim().is_empty(),
        "?1047 clears the alternate buffer on each entry"
    );
}

#[test]
fn should_report_1048_save_slot_per_buffer() {
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1048h");
    feed(&mut parser, &mut screen, b"\x1b[?1048$p");
    assert_eq!(
        screen.drain_replies(),
        b"\x1b[?1048;1$y",
        "primary has a saved slot"
    );

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049h");
    feed(&mut parser, &mut screen, b"\x1b[?1048$p");
    assert_eq!(
        screen.drain_replies(),
        b"\x1b[?1048;2$y",
        "the fresh alternate cursor has no saved slot"
    );

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049l");
    feed(&mut parser, &mut screen, b"\x1b[?1048$p");
    assert_eq!(
        screen.drain_replies(),
        b"\x1b[?1048;1$y",
        "the primary save slot survives an alt round-trip"
    );
}

#[test]
fn should_report_unknown_modes_and_ignore_malformed_queries() {
    assert_eq!(replies_for(b"\x1b[999$p"), b"\x1b[999;0$y");
    assert_eq!(replies_for(b"\x1b[?999$p"), b"\x1b[?999;0$y");
    for query in [
        b"\x1b[$p".as_slice(),
        b"\x1b[4;20$p",
        b"\x1b[4:1$p",
        b"\x1b[>4$p",
    ] {
        assert_eq!(replies_for(query), Vec::new(), "{query:?}");
    }
}

#[test]
fn should_bound_and_preserve_mode_reports_across_alt_transitions() {
    let reply = b"\x1b[?2004;2$y";
    let prefix = vec![b'x'; 1024 - reply.len()];
    let mut screen = Screen::new(10, 20);
    let mut parser = TerminalParser::default();
    screen.push_reply(&prefix);
    feed(&mut parser, &mut screen, b"\x1b[?2004$p");
    assert_eq!(screen.drain_replies().len(), 1024);

    let too_full = vec![b'x'; 1024 - reply.len() + 1];
    screen.push_reply(&too_full);
    feed(&mut parser, &mut screen, b"\x1b[?2004$p");
    assert_eq!(screen.drain_replies(), too_full);

    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?2004$p\x1b[?1049h");
    assert_eq!(screen.drain_replies(), b"\x1b[?2004;2$y");
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?2004$p\x1b[?1049l");
    assert_eq!(screen.drain_replies(), b"\x1b[?2004;2$y");
}

#[test]
fn should_write_cells_when_printable_arrives_between_2026_enable_and_disable() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(&mut parser, &mut screen, b"\x1b[?2026hbatch\x1b[?2026l");

    // Assert
    assert!(
        screen.row_text(0).contains("batch"),
        "printable output during a synchronized batch still mutates cells"
    );
    feed(&mut parser, &mut screen, b"\x1b[?2026$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;2$y");
}

#[test]
fn should_report_reset_set_set_when_decrqm_queries_2026_at_depths_0_1_2() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();

    // Act / Assert — depth 0 is Reset, never Unknown
    feed(&mut parser, &mut screen, b"\x1b[?2026$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;2$y");
    assert_ne!(
        screen.mode_status(true, 2026),
        crate::screen::ModeStatus::Unknown
    );

    // Act / Assert — depth 1 is Set
    feed(&mut parser, &mut screen, b"\x1b[?2026h\x1b[?2026$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;1$y");
    assert_ne!(
        screen.mode_status(true, 2026),
        crate::screen::ModeStatus::Unknown
    );

    // Act / Assert — depth 2 is Set, never Unknown
    feed(&mut parser, &mut screen, b"\x1b[?2026h\x1b[?2026$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;1$y");
    assert_ne!(
        screen.mode_status(true, 2026),
        crate::screen::ModeStatus::Unknown
    );
}

#[test]
fn should_keep_decrqm_set_when_one_disable_leaves_2026_nested() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?2026h\x1b[?2026h");

    // Act
    feed(&mut parser, &mut screen, b"\x1b[?2026l\x1b[?2026$p");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;1$y");
    assert!(!screen.ordinary_present_eligible());
}

#[test]
fn should_report_decrqm_reset_when_nested_2026_enables_are_fully_unwound() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(
        &mut parser,
        &mut screen,
        b"\x1b[?2026h\x1b[?2026hinner\x1b[?2026l\x1b[?2026l",
    );

    // Act
    feed(&mut parser, &mut screen, b"\x1b[?2026$p");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;2$y");
    assert!(screen.row_text(0).contains("inner"));
    assert!(screen.ordinary_present_eligible());
}

#[test]
fn should_keep_decrqm_reset_when_extra_2026_disable_arrives_at_zero() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();

    // Act
    feed(
        &mut parser,
        &mut screen,
        b"\x1b[?2026l\x1b[?2026l\x1b[?2026$p",
    );

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;2$y");
    assert!(screen.ordinary_present_eligible());
}

#[test]
fn should_write_printable_output_when_it_follows_extra_2026_disables() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?2026l\x1b[?2026l");

    // Act
    feed(&mut parser, &mut screen, b"later");

    // Assert
    assert!(screen.row_text(0).contains("later"));
    assert!(screen.ordinary_present_eligible());
}

#[test]
fn should_handle_focus_reporting_modes_queries_and_resets() {
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();

    feed(&mut parser, &mut screen, b"\x1b[?1004$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1004;2$y");

    feed(&mut parser, &mut screen, b"\x1b[?1004;2004h\x1b[?1004$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1004;1$y");

    feed(&mut parser, &mut screen, b"\x1b[!p\x1b[?1004$p");
    assert_eq!(
        screen.drain_replies(),
        b"\x1b[?1004;1$y",
        "DECSTR must not reset focus reporting"
    );

    feed(&mut parser, &mut screen, b"\x1b[?1004l\x1b[?1004$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1004;2$y");

    feed(&mut parser, &mut screen, b"\x1b[?1004h\x1bc\x1b[?1004$p");
    assert_eq!(screen.drain_replies(), b"\x1b[?1004;2$y");
}

#[test]
fn should_keep_2026_set_when_alt_screen_toggles_while_nested() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?2026h");

    // Act
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049h");
    feed(&mut parser, &mut screen, b"\x1b[?2026$p");
    let alt_reply = screen.drain_replies();
    feed_with_alt_transitions(&mut parser, &mut screen, b"\x1b[?1049l");
    feed(&mut parser, &mut screen, b"\x1b[?2026$p");
    let primary_reply = screen.drain_replies();

    // Assert
    assert_eq!(alt_reply, b"\x1b[?2026;1$y");
    assert_eq!(primary_reply, b"\x1b[?2026;1$y");
    assert!(!screen.ordinary_present_eligible());
    assert_ne!(
        screen.mode_status(true, 2026),
        crate::screen::ModeStatus::Unknown
    );
}

#[test]
fn should_report_decrqm_reset_when_ris_clears_nested_2026() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?2026h\x1b[?2026hhello");

    // Act
    feed(&mut parser, &mut screen, b"\x1bc\x1b[?2026$p");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;2$y");
    assert!(screen.ordinary_present_eligible());
}

#[test]
fn should_keep_decrqm_set_when_decstr_leaves_nested_2026() {
    // Arrange
    let mut screen = Screen::new(2, 20);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[?2026hhello");

    // Act
    feed(&mut parser, &mut screen, b"\x1b[!p\x1b[?2026$p");

    // Assert
    assert_eq!(screen.drain_replies(), b"\x1b[?2026;1$y");
    assert!(!screen.ordinary_present_eligible());
    assert!(screen.row_text(0).contains("hello"));
}

#[test]
fn osc_default_colors_set_query_reset_and_round_trip_all_slots() {
    let startup = Palette {
        foreground: Rgba::from_rgba8(1, 2, 3, 64),
        background: Rgba::from_rgba8(4, 5, 6, 96),
        cursor: Rgba::from_rgba8(7, 8, 9, 128),
        ..Palette::default()
    };
    let mut screen = Screen::with_palette(2, 8, startup);
    let mut parser = TerminalParser::default();

    screen.clear_dirty();
    feed(
        &mut parser,
        &mut screen,
        b"\x1b]10;#123456\x07\x1b]11;rgb:f/80/0000\x1b\\\x1b]12;#abcdef\x07",
    );
    let active = screen.active_palette();
    assert_eq!(active.foreground, Rgba::from_rgba8(0x12, 0x34, 0x56, 64));
    assert_eq!(active.background, Rgba::from_rgba8(255, 128, 0, 96));
    assert_eq!(active.cursor, Rgba::from_rgba8(0xab, 0xcd, 0xef, 128));
    let dirty = screen.dirty_ranges();
    assert_eq!(dirty.len(), 2);
    assert!(
        dirty
            .iter()
            .all(|range| range.start_col == 0 && range.end_col == 8)
    );
    screen.clear_dirty();
    feed(&mut parser, &mut screen, b"\x1b]12;#abcdef\x07");
    assert!(screen.dirty_ranges().is_empty());

    feed(
        &mut parser,
        &mut screen,
        b"\x1b]10;?\x07\x1b]11;?\x1b\\\x1b]12;?\x07",
    );
    assert_eq!(
        screen.drain_replies(),
        b"\x1b]10;rgb:1212/3434/5656\x07\x1b]11;rgb:ffff/8080/0000\x1b\\\x1b]12;rgb:abab/cdcd/efef\x07"
    );

    feed(&mut parser, &mut screen, b"\x1b]10;?\x07");
    let query_reply = screen.drain_replies();
    feed(&mut parser, &mut screen, b"\x1b]110;\x1b\\");
    assert_eq!(screen.active_palette().foreground, startup.foreground);
    feed(&mut parser, &mut screen, &query_reply);
    assert_eq!(
        screen.active_palette().foreground,
        Rgba::from_rgba8(0x12, 0x34, 0x56, 64)
    );

    feed(
        &mut parser,
        &mut screen,
        b"\x1b]110;\x07\x1b]111;\x1b\\\x1b]112;\x07",
    );
    assert_eq!(screen.active_palette(), startup);
}

#[test]
fn osc_default_colors_reject_invalid_input_and_recover_after_cancel_or_overflow() {
    let mut screen = Screen::new(2, 8);
    let startup = screen.active_palette();
    let mut parser = TerminalParser::default();
    for payload in [
        b"".as_slice(),
        b"red",
        b"rgbi:1/0/0",
        b"#12345678",
        b"rgb:0/0/0;rgb:f/f/f",
        b"\xff",
    ] {
        let mut sequence = b"\x1b]10;".to_vec();
        sequence.extend_from_slice(payload);
        sequence.push(0x07);
        feed(&mut parser, &mut screen, &sequence);
    }
    feed(&mut parser, &mut screen, b"\x1b]110;payload\x07");
    assert_eq!(screen.active_palette(), startup);
    assert!(screen.drain_replies().is_empty());

    feed(&mut parser, &mut screen, b"\x1b]10;#112233\x18");
    let mut overflow = b"\x1b]11;".to_vec();
    overflow.extend(std::iter::repeat_n(b'a', 4097));
    overflow.push(0x07);
    feed(&mut parser, &mut screen, &overflow);
    assert_eq!(screen.active_palette(), startup);

    for &byte in b"\x1b]12;#445566\x1b\\" {
        feed(&mut parser, &mut screen, &[byte]);
    }
    assert_eq!(
        screen.default_color(DefaultColorSlot::Cursor),
        Rgba::from_rgba8(0x44, 0x55, 0x66, 204)
    );
}

#[test]
fn osc_default_colors_survive_alt_screen_families_ris_decstr_and_sgr_resets() {
    for mode in [47, 1047, 1049] {
        let mut screen = Screen::new(2, 8);
        let mut parser = TerminalParser::default();
        feed(&mut parser, &mut screen, b"\x1b]10;#112233\x07");
        let sequence = format!(
            "\x1b[?{mode}h\x1b]11;#445566\x07\x1b[0m\x1b[!p\x1b[?{mode}l\x1b[?{mode}h\x1b[?{mode}l"
        );
        feed_with_alt_transitions(&mut parser, &mut screen, sequence.as_bytes());
        feed(&mut parser, &mut screen, b"\x1bc");

        let active = screen.active_palette();
        assert_eq!(active.foreground, Rgba::from_rgb8(0x11, 0x22, 0x33));
        assert_eq!(
            active.background,
            Rgba::new(
                0x44 as f32 / 255.0,
                0x55 as f32 / 255.0,
                0x66 as f32 / 255.0,
                0.25,
            )
        );
    }
}

#[test]
fn osc8_applies_cell_state_and_obeys_close_reset_and_invalid_preservation() {
    let mut screen = Screen::new(2, 8);
    let mut parser = TerminalParser::default();

    feed(
        &mut parser,
        &mut screen,
        b"\x1b]8;id=one;https://example.test\x1b\\a\x1b[0mb",
    );
    let link = screen.cell(0, 0).hyperlink.expect("opened hyperlink");
    assert_eq!(screen.cell(0, 1).hyperlink, Some(link));

    feed(
        &mut parser,
        &mut screen,
        b"\x1b]8;id=bad:id=dup;https://bad\x07c",
    );
    assert_eq!(screen.cell(0, 2).hyperlink, Some(link));

    feed(&mut parser, &mut screen, b"\x1b]8;;\x1b\\d");
    assert_eq!(screen.cell(0, 3).hyperlink, None);

    feed(
        &mut parser,
        &mut screen,
        b"\x1b]8;;https://again\x1b\\\x1b[!pe",
    );
    assert!(
        screen.cell(0, 0).hyperlink.is_some(),
        "DECSTR keeps active hyperlink"
    );
    feed(&mut parser, &mut screen, b"\x1bcf");
    assert_eq!(
        screen.cell(0, 0).hyperlink,
        None,
        "RIS clears cells and registry state"
    );
}

#[test]
fn osc8_fragmented_st_completion_preserves_wide_cell_identity() {
    let mut screen = Screen::new(2, 4);
    let mut parser = TerminalParser::default();
    for chunk in [
        b"\x1b]8;;https://example".as_slice(),
        b".test\x1b".as_slice(),
        b"\\\xe7\x95\x8c".as_slice(),
    ] {
        feed(&mut parser, &mut screen, chunk);
    }
    let link = screen.cell(0, 0).hyperlink.expect("base half linked");
    assert_eq!(screen.cell(0, 1).hyperlink, Some(link));
    assert!(screen.cell(0, 1).wide_continuation);
}

#[test]
fn osc8_cancel_overflow_and_incomplete_input_preserve_state_and_recover() {
    let mut screen = Screen::new(2, 12);
    let mut parser = TerminalParser::default();
    feed(
        &mut parser,
        &mut screen,
        b"\x1b]8;;https://original.test\x1b\\a",
    );
    let original = screen.cell(0, 0).hyperlink.unwrap();

    feed(
        &mut parser,
        &mut screen,
        b"\x1b]8;;https://cancelled.test\x18b",
    );
    assert_eq!(screen.cell(0, 1).hyperlink, Some(original));

    let mut overflow = b"\x1b]8;;".to_vec();
    overflow.extend(std::iter::repeat_n(b'x', 5000));
    overflow.extend_from_slice(b"\x07c");
    feed(&mut parser, &mut screen, &overflow);
    assert_eq!(screen.cell(0, 2).hyperlink, Some(original));

    let cursor_before = screen.cursor_x();
    feed(
        &mut parser,
        &mut screen,
        b"\x1b]8;;https://fragmented.test\x1b",
    );
    assert_eq!(screen.cursor_x(), cursor_before);
    feed(&mut parser, &mut screen, b"\\d");
    assert_ne!(screen.cell(0, 3).hyperlink, Some(original));
    assert!(screen.cell(0, 3).hyperlink.is_some());
}
