use super::*;

#[test]
fn test_character_set_designation_and_mapping() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);

    // Designate G1 as DEC Special Graphics: ESC ) 0
    // Designate G0 as ASCII: ESC ( B
    parser.put_bytes(&mut screen, b"\x1b)0\x1b(B");

    // By default G0 is active. Write 'q' -> should be 'q' (ASCII)
    parser.put_bytes(&mut screen, b"q");
    assert_eq!(screen.row_text(0), "q    ");

    // Invoke G1 (SO / 0x0E). Write 'q' -> should be mapped to '─'
    parser.put_bytes(&mut screen, b"\x0eq");
    assert_eq!(screen.row_text(0), "q─   ");

    // Invoke G0 (SI / 0x0F). Write 'q' -> should be 'q'
    parser.put_bytes(&mut screen, b"\x0fq");
    assert_eq!(screen.row_text(0), "q─q  ");
}

#[test]
fn test_g2_g3_designation_and_ss2_ss3_single_shift() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 10);

    // Designate G2 as DEC Special Graphics: ESC * 0
    // Designate G3 as DEC Special Graphics: ESC + 0
    parser.put_bytes(&mut screen, b"\x1b*0\x1b+0");

    // Default is G0 (ASCII). Write 'q' -> 'q'
    parser.put_bytes(&mut screen, b"q");
    assert_eq!(screen.row_text(0), "q         ");

    // SS2 (ESC N) single shifts to G2 for immediate next char 'q' -> '─'.
    // Subsequent 'q' reverts to G0 -> 'q'.
    parser.put_bytes(&mut screen, b"\x1bNqq");
    assert_eq!(screen.row_text(0), "q─q       ");

    // SS3 (ESC O) single shifts to G3 for immediate next char 'a' -> '▒'.
    // Subsequent 'a' reverts to G0 -> 'a'.
    parser.put_bytes(&mut screen, b"\x1bOaa");
    assert_eq!(screen.row_text(0), "q─q▒a     ");
}

#[test]
fn test_ss2_ss3_c1_8bit_controls() {
    let mut parser = TerminalParser::default();
    parser.set_c1_enabled(true);
    let mut screen = Screen::new(5, 10);

    parser.put_bytes(&mut screen, b"\x1b*0\x1b+0");

    // 0x8E is 8-bit SS2
    parser.put_bytes(&mut screen, b"\x8Eqq");
    assert_eq!(screen.row_text(0), "─q        ");

    // 0x8F is 8-bit SS3
    parser.put_bytes(&mut screen, b"\x8Faa");
    assert_eq!(screen.row_text(0), "─q▒a      ");
}

#[test]
fn test_ss2_ss3_affects_only_immediate_next_graphic_character() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 10);

    parser.put_bytes(&mut screen, b"\x1b*0");

    // Intervening control characters (CR, LF, BS) do not consume the single shift.
    parser.put_bytes(&mut screen, b"\x1bN\r\nq");
    assert_eq!(screen.row_text(0), "          ");
    assert_eq!(screen.row_text(1), "─         ");

    // Subsequent graphic character is back to G0 (ASCII).
    parser.put_bytes(&mut screen, b"q");
    assert_eq!(screen.row_text(1), "─q        ");
}

#[test]
fn test_ss2_survives_horizontal_tab_before_next_graphic_character() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 10);

    parser.put_bytes(&mut screen, b"\x1b*0\x1bN\tq");

    assert_eq!(screen.row_text(0), "        ─ ");
    assert_eq!(screen.active.pen_state.charsets.last_char, Some('─'));
}

#[test]
fn test_ss2_survives_rep_before_next_graphic_character() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 10);

    parser.put_bytes(&mut screen, b"q\x1b*0\x1bN\x1b[bq");

    assert_eq!(screen.row_text(0), "qq─       ");
    assert_eq!(screen.active.pen_state.charsets.last_char, Some('─'));
}

#[test]
fn test_unknown_charset_designation_does_not_corrupt_state() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 10);

    // Designate G2 as DEC Special Graphics: ESC * 0
    parser.put_bytes(&mut screen, b"\x1b*0");
    assert_eq!(screen.active.pen_state.charsets.g2, b'0');

    // Attempt to designate G2 as unknown set '?' (ESC * ?)
    parser.put_bytes(&mut screen, b"\x1b*?");
    // Unknown set must not corrupt state; prior designation remains '0'.
    assert_eq!(screen.active.pen_state.charsets.g2, b'0');

    // UK ('A') is not implemented, so it must not replace the prior set.
    parser.put_bytes(&mut screen, b"\x1b*A");
    assert_eq!(screen.active.pen_state.charsets.g2, b'0');

    // Invoke G2 via SS2: should still map 'q' to '─'
    parser.put_bytes(&mut screen, b"\x1bNqq");
    assert_eq!(screen.row_text(0), "─q        ");

    // Designate G3 as DEC Special Graphics: ESC + 0
    parser.put_bytes(&mut screen, b"\x1b+0");
    assert_eq!(screen.active.pen_state.charsets.g3, b'0');

    // Attempt to designate G3 as unknown set '~' (ESC + ~)
    parser.put_bytes(&mut screen, b"\x1b+~");
    assert_eq!(screen.active.pen_state.charsets.g3, b'0');

    // Invoke G3 via SS3: should still map 'a' to '▒'
    parser.put_bytes(&mut screen, b"\x1bOaa");
    assert_eq!(screen.row_text(0), "─q▒a      ");

    // G0 and G1 use the same supported-set filter.
    parser.put_bytes(&mut screen, b"\x1b(0\x1b)0\x1b(?\x1b)?");
    assert_eq!(screen.active.pen_state.charsets.g0, b'0');
    assert_eq!(screen.active.pen_state.charsets.g1, b'0');
}

#[test]
fn test_g2_g3_and_single_shift_reset_behavior() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 10);

    parser.put_bytes(&mut screen, b"\x1b*0\x1b+0");
    assert_eq!(screen.active.pen_state.charsets.g2, b'0');
    assert_eq!(screen.active.pen_state.charsets.g3, b'0');

    // Trigger single shift SS2, then soft reset (DECSTR: ESC [ ! p)
    parser.put_bytes(&mut screen, b"\x1bN\x1b[!p");

    // DECSTR leaves G0-G3 designations intact, but clears pending single shift.
    assert_eq!(screen.active.pen_state.charsets.g2, b'0');
    assert_eq!(screen.active.pen_state.charsets.g3, b'0');
    assert_eq!(screen.active.pen_state.charsets.single_shift, None);

    // Next character is ASCII 'q' because single shift was cleared.
    parser.put_bytes(&mut screen, b"q");
    assert_eq!(screen.row_text(0), "q         ");

    // Hard reset (RIS: ESC c) resets all designations to US-ASCII ('B').
    parser.put_bytes(&mut screen, b"\x1bc");
    assert_eq!(screen.active.pen_state.charsets.g0, b'B');
    assert_eq!(screen.active.pen_state.charsets.g1, b'B');
    assert_eq!(screen.active.pen_state.charsets.g2, b'B');
    assert_eq!(screen.active.pen_state.charsets.g3, b'B');
    assert_eq!(screen.active.pen_state.charsets.active, 0);
    assert_eq!(screen.active.pen_state.charsets.single_shift, None);

    // SS2 now maps to G2 which is US-ASCII 'B', so 'q' stays 'q'.
    parser.put_bytes(&mut screen, b"\x1bNq");
    assert_eq!(screen.row_text(0), "q         ");
}

#[test]
fn test_charset_state_preserved_when_saving_restoring_cursor() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 10);

    // Designate G0..G3 as DEC Special Graphics, and invoke G1 (SO)
    parser.put_bytes(&mut screen, b"\x1b(0\x1b)0\x1b*0\x1b+0\x0e");
    assert_eq!(screen.active.pen_state.charsets.g0, b'0');
    assert_eq!(screen.active.pen_state.charsets.g1, b'0');
    assert_eq!(screen.active.pen_state.charsets.g2, b'0');
    assert_eq!(screen.active.pen_state.charsets.g3, b'0');
    assert_eq!(screen.active.pen_state.charsets.active, 1);

    // Save cursor (DECSC: ESC 7)
    parser.put_bytes(&mut screen, b"\x1b7");

    // Re-designate G0..G3 as ASCII and invoke G0 (SI)
    parser.put_bytes(&mut screen, b"\x1b(B\x1b)B\x1b*B\x1b+B\x0f");
    assert_eq!(screen.active.pen_state.charsets.g0, b'B');
    assert_eq!(screen.active.pen_state.charsets.g1, b'B');
    assert_eq!(screen.active.pen_state.charsets.g2, b'B');
    assert_eq!(screen.active.pen_state.charsets.g3, b'B');
    assert_eq!(screen.active.pen_state.charsets.active, 0);

    // Restore cursor (DECRC: ESC 8)
    parser.put_bytes(&mut screen, b"\x1b8");
    assert_eq!(screen.active.pen_state.charsets.g0, b'0');
    assert_eq!(screen.active.pen_state.charsets.g1, b'0');
    assert_eq!(screen.active.pen_state.charsets.g2, b'0');
    assert_eq!(screen.active.pen_state.charsets.g3, b'0');
    assert_eq!(screen.active.pen_state.charsets.active, 1);
}
