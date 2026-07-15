use super::*;
use crate::screen::{CursorShape, Screen};

fn feed(parser: &mut TerminalParser, screen: &mut Screen, seq: &[u8]) {
    parser.put_bytes(screen, seq);
}

/// Move cursor to (row, col) 1-based via `CSI row;col H`.
fn move_to(parser: &mut TerminalParser, screen: &mut Screen, row: usize, col: usize) {
    feed(parser, screen, format!("\x1b[{row};{col}H").as_bytes());
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
fn decscusr_ps0_is_blinking_bar() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    feed(&mut parser, &mut screen, b"\x1b[0 q");
    assert_eq!(screen.cursor_shape(), CursorShape::Bar);
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
fn initial_cursor_shape_is_bar() {
    let screen = Screen::new(10, 10);
    assert_eq!(screen.cursor_shape(), CursorShape::Bar);
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
