use super::*;
use crate::screen::{CursorShape, Screen};
use harbor_parser::Params;

fn feed(parser: &mut TerminalParser, screen: &mut Screen, seq: &[u8]) {
    parser.put_bytes(screen, seq);
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
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
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
    oversized.extend(std::iter::repeat(b'm').take(17));
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
    oversized.extend(std::iter::repeat(b'a').take(5000));
    oversized.extend_from_slice(b"\x1b\\OK");
    feed(&mut parser, &mut screen, &oversized);
    assert_eq!(screen.drain_replies(), FAILURE_REPLY);
    assert!(screen.row_text(0).contains("OK"));
}
