//! §1.1 chunk-equivalence harness and string-family consume-only tests.

use super::*;
use crate::Terminal;
use crate::screen::{AltScreenAction, Screen};
use harbor_parser::{Params, Parser, VtHandler};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

#[derive(Default)]
struct CsiRecorder {
    dispatches: Vec<(Option<u8>, Vec<Option<usize>>, Vec<u8>, u8)>,
}

impl VtHandler for CsiRecorder {
    fn print(&mut self, _ch: char) {}

    fn execute(&mut self, _byte: u8) {}

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        action: u8,
        private_marker: Option<u8>,
    ) {
        self.dispatches.push((
            private_marker,
            params.iter_flat().collect::<Vec<_>>(),
            intermediates.to_vec(),
            action,
        ));
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _byte: u8) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _action: u8) {}

    fn dcs_put(&mut self, _byte: u8) {}

    fn dcs_unhook(&mut self, _terminated: bool) {}

    fn start_string(&mut self, _kind: u8) {}
}

fn feed_core(parser: &mut Parser, recorder: &mut CsiRecorder, bytes: &[u8]) {
    for &byte in bytes {
        parser.advance(recorder, byte);
    }
}

/// Snapshot of screen-visible parser outcomes for equivalence checks.
#[derive(Debug, PartialEq, Eq)]
struct ScreenSnap {
    cursor_x: usize,
    cursor_y: usize,
    rows: Vec<String>,
    replies: Vec<u8>,
    is_alt: bool,
    scroll_count: usize,
}

fn snap(screen: &mut Screen) -> ScreenSnap {
    let rows = (0..screen.rows()).map(|r| screen.row_text(r)).collect();
    let replies = screen.drain_replies();
    ScreenSnap {
        cursor_x: screen.cursor_x(),
        cursor_y: screen.cursor_y(),
        rows,
        replies,
        is_alt: screen.is_alt(),
        scroll_count: screen.scroll_count(),
    }
}

/// Feed bytes through `TerminalParser`, honoring mid-batch alt-screen splits
/// the same way `Terminal::put_bytes` does.
fn feed_all(parser: &mut TerminalParser, screen: &mut Screen, data: &[u8]) {
    let mut remaining = data;
    while !remaining.is_empty() {
        let result = parser.put_bytes(screen, remaining);
        remaining = &remaining[result.consumed..];
        if let Some(action) = result.alt_request {
            // Mirror TerminalIo: consume the pending request, then apply the action before
            // feeding the unconsumed suffix. This makes alternate-screen behavior part of the
            // one-shot/chunked comparison rather than dropping the boundary event.
            let _ = screen.take_alt_request();
            match action {
                AltScreenAction::Enter => screen.enter_alt(),
                AltScreenAction::Exit => screen.exit_alt(),
            }
        }
    }
}

/// Feed `data` in fixed-size chunks, each chunk processed with alt-split handling.
fn feed_chunks(parser: &mut TerminalParser, screen: &mut Screen, data: &[u8], chunk: usize) {
    assert!(chunk > 0);
    let mut i = 0;
    while i < data.len() {
        let end = (i + chunk).min(data.len());
        feed_all(parser, screen, &data[i..end]);
        i = end;
    }
}

/// Feed bytes through independently-sized `put_bytes` calls. Unlike the parser-core tests, this
/// exercises the actual slice-ingestion boundary and its consumed-prefix contract.
fn feed_scheduled(
    parser: &mut TerminalParser,
    screen: &mut Screen,
    data: &[u8],
    schedule: &[usize],
) {
    let mut offset = 0;
    let mut schedule_index = 0;
    while offset < data.len() {
        let requested = schedule.get(schedule_index).copied().unwrap_or(1).max(1);
        let end = (offset + requested).min(data.len());
        feed_all(parser, screen, &data[offset..end]);
        offset = end;
        schedule_index += 1;
    }
}

fn run_bulk(rows: usize, cols: usize, data: &[u8]) -> ScreenSnap {
    let mut screen = Screen::new(rows, cols);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, data);
    snap(&mut screen)
}

fn run_chunked(rows: usize, cols: usize, data: &[u8], chunk: usize) -> ScreenSnap {
    let mut screen = Screen::new(rows, cols);
    let mut parser = TerminalParser::default();
    feed_chunks(&mut parser, &mut screen, data, chunk);
    snap(&mut screen)
}

fn run_scheduled(rows: usize, cols: usize, data: &[u8], schedule: &[usize]) -> ScreenSnap {
    let mut screen = Screen::new(rows, cols);
    let mut parser = TerminalParser::default();
    feed_scheduled(&mut parser, &mut screen, data, schedule);
    snap(&mut screen)
}

fn assert_chunk_equiv(rows: usize, cols: usize, data: &[u8]) {
    let bulk = run_bulk(rows, cols, data);
    for chunk in [1usize, 2, 3, 7] {
        let chunked = run_chunked(rows, cols, data, chunk);
        assert_eq!(
            bulk, chunked,
            "chunk size {chunk} diverged from bulk for {data:?}"
        );
    }
}

proptest! {
    #![proptest_config(Config {
        cases: 128,
        rng_seed: RngSeed::Fixed(0x0072_0220_0000_0072),
        ..Config::default()
    })]

    #[test]
    fn arbitrary_put_bytes_chunking_matches_one_shot(
        data in prop::collection::vec(any::<u8>(), 0..=2048),
        schedule in prop::collection::vec(1usize..=16, 0..=256),
    ) {
        // The one-shot path invokes put_bytes once; the scheduled path invokes it through
        // independently-sized slices and handles PutResult::consumed/alt_request at each call.
        let one_shot = run_bulk(8, 40, &data);
        let chunked = run_scheduled(8, 40, &data, &schedule);
        prop_assert_eq!(one_shot, chunked);
    }
}

#[test]
fn chunk_equiv_plain_text_and_csi_cursor() {
    assert_chunk_equiv(5, 40, b"hi\x1b[2;3Hthere");
}

#[test]
fn chunk_equiv_csi_split_mid_params() {
    // Full stream is CSI 123 A; equivalence across arbitrary cuts.
    assert_chunk_equiv(20, 20, b"\x1b[123A");
}

#[test]
fn chunk_equiv_esc_save_cursor() {
    assert_chunk_equiv(10, 10, b"\x1b7");
}

#[test]
fn chunk_equiv_osc_st_terminated() {
    assert_chunk_equiv(5, 40, b"\x1b]0;title\x1b\\visible");
}

#[test]
fn chunk_equiv_osc_bel_terminated() {
    assert_chunk_equiv(5, 40, b"\x1b]0;title\x07visible");
}

#[test]
fn chunk_equiv_dcs_then_text() {
    assert_chunk_equiv(5, 40, b"\x1bP$q q\x1b\\hello");
}

#[test]
fn chunk_equiv_apc_pm_sos_then_text() {
    assert_chunk_equiv(5, 40, b"\x1b_apc-payload\x1b\\OK");
    assert_chunk_equiv(5, 40, b"\x1b^pm-payload\x1b\\OK");
    assert_chunk_equiv(5, 40, b"\x1bXsos-payload\x1b\\OK");
}

#[test]
fn chunk_equiv_utf8_multibyte() {
    // "你" = E4 BD A0; cut at every offset covered by chunk sizes 1/2/3/7.
    let mut data = Vec::new();
    data.extend_from_slice("hi".as_bytes());
    data.extend_from_slice("你".as_bytes());
    data.extend_from_slice("x".as_bytes());
    assert_chunk_equiv(5, 40, &data);
}

#[test]
fn chunk_equiv_mixed_stream() {
    let data = b"ab\x1b[2;2Hcd\x1b]0;t\x07ef\x1b[1Axy";
    assert_chunk_equiv(10, 40, data);
}

#[test]
fn chunk_equiv_alt_screen_switch_consumes_and_replays_suffix() {
    let data = b"before\x1b[?1049hafter\x1b[?1049lrest";
    assert_chunk_equiv(5, 40, data);
}

#[test]
fn put_result_reports_alt_boundary_before_suffix_and_consumes_request() {
    // Arrange
    let mut screen = Screen::new(3, 20);
    let mut parser = TerminalParser::default();
    let data = b"before\x1b[?1049hafter";
    let switch_len = b"before\x1b[?1049h".len();

    // Act
    let result = parser.put_bytes(&mut screen, data);

    // Assert
    assert_eq!(result.consumed, switch_len);
    assert_eq!(result.alt_request, Some(AltScreenAction::Enter));
    assert_eq!(screen.row_text(0).trim(), "before");
    assert_eq!(screen.alt_request(), None);

    // Arrange — apply the request before replaying the unconsumed suffix.
    screen.enter_alt();

    // Act
    let suffix_result = parser.put_bytes(&mut screen, &data[result.consumed..]);

    // Assert
    assert_eq!(suffix_result.consumed, data.len() - switch_len);
    assert_eq!(suffix_result.alt_request, None);
    assert_eq!(screen.row_text(0).trim(), "after");
}

#[test]
fn should_preserve_device_attribute_replies_when_stream_is_chunked() {
    // Arrange
    let data = b"\x1b[c\x1b[>0c\x1bZ";
    let chunk_sizes = [1usize, 2, 3, 7];

    // Act
    let bulk = run_bulk(5, 40, data);
    let chunked = chunk_sizes
        .iter()
        .map(|chunk| run_chunked(5, 40, data, *chunk))
        .collect::<Vec<_>>();

    // Assert
    for (chunk, result) in chunk_sizes.iter().zip(chunked) {
        assert_eq!(result, bulk, "chunk size {chunk} diverged from bulk");
    }
}

#[test]
fn dcs_payload_never_prints_following_text_does() {
    let mut screen = Screen::new(5, 40);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, b"\x1bP0;1|payload\x1b\\OK");
    let row = screen.row_text(0);
    assert!(row.contains("OK"), "row={row:?}");
    assert!(!row.contains("payload"), "row={row:?}");
    assert!(!row.contains('|'), "row={row:?}");
}

#[test]
fn osc_never_paints_payload() {
    let mut screen = Screen::new(5, 40);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, b"\x1b]0;secret\x07visible");
    let row = screen.row_text(0);
    assert!(row.contains("visible"), "row={row:?}");
    assert!(!row.contains("secret"), "row={row:?}");
}

#[test]
fn split_st_across_calls_then_print() {
    let mut screen = Screen::new(5, 40);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, b"\x1b]0;a\x1b");
    feed_all(&mut parser, &mut screen, b"\\x");
    let row = screen.row_text(0);
    assert!(row.contains('x'), "row={row:?}");
    assert!(!row.contains('a'), "row={row:?}");
}

#[test]
fn apc_payload_never_prints() {
    let mut screen = Screen::new(5, 40);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, b"\x1b_hidden\x1b\\shown");
    let row = screen.row_text(0);
    assert!(row.contains("shown"), "row={row:?}");
    assert!(!row.contains("hidden"), "row={row:?}");
}

#[test]
fn pm_payload_never_prints() {
    let mut screen = Screen::new(5, 40);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, b"\x1b^hidden\x1b\\shown");
    let row = screen.row_text(0);
    assert!(row.contains("shown"), "row={row:?}");
    assert!(!row.contains("hidden"), "row={row:?}");
}

#[test]
fn sos_payload_never_prints() {
    let mut screen = Screen::new(5, 40);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, b"\x1bXhidden\x1b\\shown");
    let row = screen.row_text(0);
    assert!(row.contains("shown"), "row={row:?}");
    assert!(!row.contains("hidden"), "row={row:?}");
}

#[test]
fn lone_esc_at_end_does_not_print() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, b"hi\x1b");
    assert_eq!(screen.row_text(0).chars().take(2).collect::<String>(), "hi");
    // Resume with final ESC 7 (save cursor) then more text.
    feed_all(&mut parser, &mut screen, b"7more");
    let row = screen.row_text(0);
    assert!(
        row.contains("himore") || row.starts_with("hi"),
        "row={row:?}"
    );
    assert!(!row.contains('\u{1b}'));
}

#[test]
fn incomplete_csi_resumes_across_calls() {
    let mut screen = Screen::new(10, 10);
    let mut parser = TerminalParser::default();
    // Place cursor, then incomplete CSI, then finish as cursor-up 2.
    feed_all(&mut parser, &mut screen, b"\x1b[5;5H");
    assert_eq!(screen.cursor_y(), 4);
    feed_all(&mut parser, &mut screen, b"\x1b[2");
    feed_all(&mut parser, &mut screen, b"A");
    assert_eq!(screen.cursor_y(), 2);
}

#[test]
fn alt_screen_mid_batch_still_splits_via_terminal() {
    // Keep the existing Terminal-level contract green under the new parser.
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_str("before");
    // CSI ?1049h then text in one batch — Terminal::put_bytes must split.
    terminal.put_bytes(b"\x1b[?1049hAFTER");
    // After enter-alt, content lands on the alt buffer; primary still has "before".
    // We only assert no panic and that alt switch was applied (in_alt).
    assert!(
        terminal.screen().is_alt(),
        "alt screen should be active after mid-batch switch"
    );
}

#[test]
fn c1_8bit_recognition() {
    // Disabled by default: 0x9B is treated as non-ASCII text, printed as replacement char.
    {
        let mut screen = Screen::new(5, 20);
        let mut parser = TerminalParser::default();
        feed_all(&mut parser, &mut screen, b"\x9b3A");
        let text = screen.row_text(0);
        assert!(text.contains('3') && text.contains('A'));
    }

    // Enabled explicitly: 0x9B acts as CSI.
    {
        let mut screen = Screen::new(5, 20);
        let mut parser = TerminalParser::default();
        parser.inner.set_c1_enabled(true);
        feed_all(&mut parser, &mut screen, b"\x1b[3;3H");
        assert_eq!(screen.cursor_y(), 2);
        feed_all(&mut parser, &mut screen, b"\x9b1A"); // CSI 1 A -> cursor up to y=1
        assert_eq!(screen.cursor_y(), 1);
    }
}

#[test]
fn c1_st_terminates_strings_after_escape() {
    for sequence in [
        b"\x1b]title\x1b\x9cvisible".as_slice(),
        b"\x1bPqpayload\x1b\x9cvisible".as_slice(),
        b"\x1bXpayload\x1b\x9cvisible".as_slice(),
    ] {
        let mut screen = Screen::new(5, 20);
        let mut parser = TerminalParser::default();
        parser.inner.set_c1_enabled(true);
        feed_all(&mut parser, &mut screen, sequence);
        assert!(screen.row_text(0).contains("visible"));
    }
}

#[test]
fn c0_executable_in_csi() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();
    // Place cursor at (2,2), then send CSI 1; \x0d (CR) 2 H.
    // CR executes immediately (cursor moves to col 0), then final H dispatches CUP with params [1, 2].
    feed_all(&mut parser, &mut screen, b"\x1b[3;3H\x1b[1;\x0d2H");
    assert_eq!(screen.cursor_y(), 0);
    assert_eq!(screen.cursor_x(), 1);
}

#[test]
fn string_overflow_safety() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();

    // Send an oversized OSC sequence (> 4096 bytes) followed by terminator then visible text.
    let mut seq = Vec::new();
    seq.extend_from_slice(b"\x1b]");
    seq.resize(seq.len() + 5000, b'a');
    seq.extend_from_slice(b"\x07visible");
    feed_all(&mut parser, &mut screen, &seq);

    let row = screen.row_text(0);
    assert!(row.contains("visible"), "row={row:?}");
    assert!(!row.contains('a'));
}

#[test]
fn string_cancellation() {
    let mut screen = Screen::new(5, 20);
    let mut parser = TerminalParser::default();

    // Send OSC, then CAN, then normal text.
    feed_all(&mut parser, &mut screen, b"\x1b]title\x18visible");
    let row = screen.row_text(0);
    assert!(row.contains("visible"), "row={row:?}");
    assert!(!row.contains("title"));

    // Send DCS, then SUB, then normal text.
    feed_all(&mut parser, &mut screen, b"\x1bPpayload\x1avisible2");
    let row = screen.row_text(0);
    assert!(row.contains("visible2"), "row={row:?}");
    assert!(!row.contains("payload"));
}

#[test]
fn should_preserve_csi_private_markers_when_sequences_are_dispatched() {
    // Arrange
    let mut parser = Parser::default();
    let mut recorder = CsiRecorder::default();

    // Act
    feed_core(
        &mut parser,
        &mut recorder,
        b"\x1b[>1m\x1b[?2m\x1b[<3m\x1b[=4m\x1b[5m",
    );

    // Assert
    assert_eq!(
        recorder.dispatches,
        vec![
            (Some(b'>'), vec![Some(1)], Vec::new(), b'm'),
            (Some(b'?'), vec![Some(2)], Vec::new(), b'm'),
            (Some(b'<'), vec![Some(3)], Vec::new(), b'm'),
            (Some(b'='), vec![Some(4)], Vec::new(), b'm'),
            (None, vec![Some(5)], Vec::new(), b'm'),
        ]
    );
}

#[test]
fn should_clear_csi_state_when_sequence_is_cancelled_before_dispatch() {
    // Arrange
    let mut parser = Parser::default();
    let mut recorder = CsiRecorder::default();

    // Act
    feed_core(&mut parser, &mut recorder, b"\x1b[?9\x18\x1b[6m");

    // Assert
    assert_eq!(
        recorder.dispatches,
        vec![(None, vec![Some(6)], Vec::new(), b'm')]
    );
}

#[test]
fn should_round_trip_private_mode_report_through_second_parser() {
    let mut screen = Screen::new(10, 20);
    let mut terminal_parser = TerminalParser::default();
    feed_all(&mut terminal_parser, &mut screen, b"\x1b[?2004$p");
    let reply = screen.drain_replies();

    assert_eq!(reply, b"\x1b[?2004;2$y");

    let mut reply_parser = Parser::default();
    let mut recorder = CsiRecorder::default();
    feed_core(&mut reply_parser, &mut recorder, &reply);

    assert_eq!(
        recorder.dispatches,
        vec![(Some(b'?'), vec![Some(2004), Some(2)], b"$".to_vec(), b'y')]
    );
}

#[test]
fn should_preserve_mode_query_replies_when_stream_is_chunked() {
    assert_chunk_equiv(5, 40, b"\x1b[4$p\x1b[?7$p");
}
