use super::*;
use crate::{Terminal, TerminalAppearance, TerminalSize};
use harbor_config::HistoryLimits;

fn terminal(rows: usize, cols: usize, max_rows: usize, max_bytes: usize) -> Terminal {
    Terminal::new_headless_with_history(
        rows,
        cols,
        TerminalAppearance::default(),
        HistoryLimits {
            max_rows,
            max_bytes,
        },
    )
}

fn accounting(screen: &Screen) {
    let source = screen.saved_primary.as_deref().unwrap_or(screen);
    let buf = &source.normal;
    let recomputed: usize = buf
        .retained_rows()
        .take(buf.scroll_count())
        .map(|row| NormalBuf::row_source_bytes(row.cells, row.metadata.meaningful_extent))
        .sum();
    assert_eq!(screen.history_content_bytes(), recomputed);
    assert!(recomputed <= screen.history_limits.max_bytes);
    assert!(buf.scroll_count() <= screen.history_limits.max_rows);
}

#[test]
fn history_independent_row_byte_and_combined_eviction_keep_newest_whole_rows() {
    for (rows, bytes, retained) in [(2, 100, 2), (20, 3, 1), (2, 3, 1), (0, 100, 0), (20, 0, 0)] {
        let mut t = terminal(1, 8, rows, bytes);
        t.put_str("aaa\r\nbbb\r\nccc\r\nLIVE");
        accounting(&t.screen);
        assert_eq!(t.screen.scroll_count(), retained);
        if retained > 0 {
            assert_eq!(
                t.screen
                    .normal
                    .retained_rows()
                    .nth(retained - 1)
                    .unwrap()
                    .cells[0]
                    .ch,
                'c'
            );
        }
        assert_eq!(&t.screen.row_text(0)[..4], "LIVE");
    }
    // One too-large physical row is evicted, not sliced into smaller text units.
    let mut t = terminal(1, 8, 10, 2);
    t.put_str("e\u{301}\r\nx\r\nZ");
    assert_eq!(t.screen.history_content_bytes(), 1);
    assert_eq!(
        t.screen.normal.retained_rows().next().unwrap().cells[0].ch,
        'x'
    );
}

#[test]
fn history_source_bytes_include_sequences_and_spaces_not_wide_slots_or_capacity() {
    let mut t = terminal(1, 12, 20, 100);
    t.put_str("e\u{301}界👩\u{200d}💻 \r\n");
    assert_eq!(
        t.screen.history_content_bytes(),
        "e\u{301}界👩\u{200d}💻 ".len()
    );
    t.put_str("\r\n"); // An unwritten empty history row has no source text.
    assert_eq!(
        t.screen.history_content_bytes(),
        "e\u{301}界👩\u{200d}💻 ".len()
    );
    accounting(&t.screen);
    t.put_str("\x1b[?1049h");
    t.put_str(&"界\r\n".repeat(30));
    assert_eq!(t.screen.scroll_count(), 0);
    accounting(&t.screen);
    t.put_str("\x1b[?1049l");
    accounting(&t.screen);
    t.put_str("\x1bc");
    assert_eq!(t.screen.history_content_bytes(), 0);
    assert_eq!(t.screen.history_limits().max_rows, 20);
}

#[test]
fn history_retention_only_reload_preserves_live_geometry_cursor_and_explicit_tail() {
    let mut t = terminal(2, 8, 10, 100);
    t.put_str("old\r\nnew\r\nL  ");
    let before = t.screen.terminal_snapshot();
    let limits = HistoryLimits {
        max_rows: 1,
        max_bytes: 3,
    };
    let palette = t
        .read_update(std::time::Instant::now())
        .appearance
        .palette();
    let mut tx = t
        .prepare_configuration_with_history(TerminalSize { rows: 2, cols: 8 }, limits)
        .unwrap();
    tx.resize_pty().unwrap();
    tx.commit(palette, None, 1.0);
    let after = t.screen.terminal_snapshot();
    assert_eq!(after.cells, before.cells);
    assert_eq!(
        (after.cursor_x, after.cursor_y),
        (before.cursor_x, before.cursor_y)
    );
    assert_eq!(t.screen.history_limits(), limits);
    accounting(&t.screen);
}

#[test]
fn history_byte_eviction_truncates_heads_invalidates_selection_and_review_without_aliasing() {
    let mut t = terminal(2, 3, 20, 100);
    t.put_str("abcdefghijklmnopqr");
    t.screen.scroll_up(20);
    let projection = t.screen.content_projection().unwrap();
    let lost = projection
        .to_anchor(GenPos::new(t.screen.history_start(), 0), Affinity::Before)
        .unwrap();
    let tail = projection
        .to_anchor(
            GenPos::new(t.screen.history_start() + 3, 0),
            Affinity::Before,
        )
        .unwrap();
    let limits = HistoryLimits {
        max_rows: 20,
        max_bytes: 3,
    };
    let p = t
        .screen
        .prepare_resize_with_history(2, 3, ReflowViewport::PullHistory, limits)
        .unwrap();
    assert!(p.project_active_selection(lost).is_none());
    assert!(p.project_active_selection(tail).is_some());
    t.screen.commit_resize(p);
    assert_eq!(t.screen.view_offset(), 1);
    assert!(
        t.screen
            .normal
            .retained_rows()
            .next()
            .unwrap()
            .metadata
            .head_truncated
    );
    assert_eq!(
        t.screen.review_anchor.unwrap(),
        t.screen
            .content_projection()
            .unwrap()
            .oldest_anchor()
            .unwrap()
    );
    for _ in 0..30 {
        t.put_str("uvw");
        for cols in [2, 5, 3] {
            t.resize(2, cols);
            accounting(&t.screen);
        }
        assert!(
            t.screen
                .content_projection()
                .unwrap()
                .resolve_selection(lost)
                .is_none()
        );
    }
}

#[test]
fn history_conpty_saved_primary_shrinks_now_and_deferred_resize_cannot_resurrect() {
    let mut t = terminal(2, 6, 20, 100);
    t.put_str("aaaaaa\r\nbbbbbb\r\ncccccc\r\ndddddd\r\nLIVE");
    t.put_str("\x1b[?1049hALT");
    let limits = HistoryLimits {
        max_rows: 1,
        max_bytes: 6,
    };
    let p = t
        .screen
        .prepare_resize_with_history(3, 4, ReflowViewport::PreserveLiveTop, limits)
        .unwrap();
    t.screen.commit_resize(p);
    assert_eq!(
        t.screen.saved_primary.as_ref().unwrap().cols(),
        6,
        "producer geometry stays deferred"
    );
    assert_eq!(t.screen.saved_primary.as_ref().unwrap().scroll_count(), 1);
    accounting(&t.screen);
    let lost = t
        .screen
        .saved_primary
        .as_ref()
        .unwrap()
        .normal
        .history_start();
    // A later growth must not resurrect evicted source from a stale deferred candidate.
    let p = t
        .screen
        .prepare_resize_with_history(
            3,
            8,
            ReflowViewport::PreserveLiveTop,
            HistoryLimits {
                max_rows: 20,
                max_bytes: 100,
            },
        )
        .unwrap();
    t.screen.commit_resize(p);
    assert_eq!(
        t.screen
            .saved_primary
            .as_ref()
            .unwrap()
            .normal
            .history_start(),
        lost
    );
    t.screen.exit_alt();
    accounting(&t.screen);
    let text: String = t
        .screen
        .normal
        .retained_rows()
        .flat_map(|r| r.cells.iter())
        .map(|c| c.ch)
        .collect();
    assert!(!text.contains('a') && !text.contains('b'));
    assert!(text.contains("LIVE"));
}

#[test]
fn history_resize_height_width_and_ring_churn_always_agree_with_source_bytes() {
    let mut t = terminal(3, 9, 4, 22);
    for n in 0..50 {
        t.put_str("e\u{301}界👩\u{200d}💻\r\n");
        let cols = [4, 12, 3, 9][n % 4];
        let rows = [1, 4, 2][n % 3];
        t.resize(rows, cols);
        accounting(&t.screen);
    }
}

#[test]
fn history_lost_endpoint_clears_whole_active_or_saved_selection_and_read_targets() {
    use crate::read_model::{ReadBudget, ReadScope};
    use crate::{
        RenderViewport, TerminalEvent, TerminalPointerButton, TerminalPointerEvent,
        TerminalPointerPhase,
    };
    for saved in [false, true] {
        let mut t = terminal(2, 8, 20, 100);
        t.pointer
            .set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
        t.put_str("OLD-END");
        for (phase, x) in [
            (TerminalPointerPhase::Down, 1.0),
            (TerminalPointerPhase::Move, 61.0),
            (TerminalPointerPhase::Up, 61.0),
        ] {
            t.handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                (x, 1.0),
                phase,
                TerminalPointerButton::Left,
                70,
            )))
            .unwrap();
        }
        assert_eq!(t.selection_text(), "OLD-END");
        t.put_str("\r\nnew\r\nlive");
        let page = t
            .read_content(
                ReadScope::Retained {
                    start_row: 0,
                    row_count: 1,
                },
                ReadBudget::default(),
            )
            .unwrap();
        let range = page.fragments[0].range(0..3).unwrap();
        assert!(t.validate_content_range(&range));
        if saved {
            t.put_str("\x1b[?1049hALT");
        }
        let mut tx = t
            .prepare_configuration_with_history(
                TerminalSize { rows: 2, cols: 8 },
                HistoryLimits {
                    max_rows: 0,
                    max_bytes: 0,
                },
            )
            .unwrap();
        tx.resize_pty().unwrap();
        tx.commit(Palette::default(), None, 1.0);
        if saved {
            t.put_str("\x1b[?1049l");
        }
        assert!(!t.has_non_empty_selection());
        assert_eq!(t.selection_text(), "");
        assert!(!t.validate_content_range(&range));
    }
}
