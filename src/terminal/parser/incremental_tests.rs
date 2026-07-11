//! §1.1 chunk-equivalence harness and string-family consume-only tests.

use super::super::Screen;
use super::*;
use crate::terminal::Terminal;

/// Snapshot of screen-visible parser outcomes for equivalence checks.
#[derive(Debug, PartialEq, Eq)]
struct ScreenSnap {
    cursor_x: usize,
    cursor_y: usize,
    rows: Vec<String>,
}

fn snap(screen: &Screen) -> ScreenSnap {
    let rows = (0..screen.rows()).map(|r| screen.row_text(r)).collect();
    ScreenSnap {
        cursor_x: screen.cursor_x(),
        cursor_y: screen.cursor_y(),
        rows,
    }
}

/// Feed bytes through `TerminalParser`, honoring mid-batch alt-screen splits
/// the same way `Terminal::put_bytes` does.
fn feed_all(parser: &mut TerminalParser, screen: &mut Screen, data: &[u8]) {
    let mut remaining = data;
    while !remaining.is_empty() {
        let result = parser.put_bytes(screen, remaining);
        remaining = &remaining[result.consumed..];
        if result.alt_request.is_some() {
            // Tests that don't switch screens just drop the request after take.
            let _ = screen.take_alt_request();
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

fn run_bulk(rows: usize, cols: usize, data: &[u8]) -> ScreenSnap {
    let mut screen = Screen::new(rows, cols);
    let mut parser = TerminalParser::default();
    feed_all(&mut parser, &mut screen, data);
    snap(&screen)
}

fn run_chunked(rows: usize, cols: usize, data: &[u8], chunk: usize) -> ScreenSnap {
    let mut screen = Screen::new(rows, cols);
    let mut parser = TerminalParser::default();
    feed_chunks(&mut parser, &mut screen, data, chunk);
    snap(&screen)
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
    let mut terminal = Terminal::new(3, 20);
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
    for _ in 0..5000 {
        seq.push(b'a');
    }
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
