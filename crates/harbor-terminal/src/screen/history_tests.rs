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

#[test]
fn csi_3j_clears_wrapped_ring_history_without_changing_live_screen_or_limits() {
    let mut t = terminal(2, 8, 4, 100);
    for _ in 0..20 {
        t.put_str("e\u{301}界\r\n");
    }
    t.put_str("LIVE\r\nTAIL\x1b[45m\x1b7\x1b[?69h\x1b[3;6s\x1b[1;1H");
    assert!(t.screen.history_start() > 0);
    assert!(t.screen.cursor.cursor.x < t.screen.cursor.margins.left);
    let cells: Vec<_> = t
        .screen
        .normal
        .retained_rows()
        .skip(t.screen.scroll_count())
        .flat_map(|row| row.cells.iter().cloned())
        .collect();
    let cursor = (t.screen.cursor.cursor.x, t.screen.cursor.cursor.y);
    let saved = t.screen.cursor.cursor.saved.clone().unwrap();
    let limits = t.screen.history_limits();
    let old_start = t.screen.history_start();
    let count = t.screen.scroll_count();
    assert!(count > 0);
    let version = t.content_version();

    t.put_str("\x1b[3J");

    assert_eq!(t.screen.scroll_count(), 0);
    assert_eq!(t.screen.history_content_bytes(), 0);
    assert_eq!(t.screen.view_offset(), 0);
    assert_eq!(t.screen.history_start(), old_start + count as u64);
    assert_eq!(t.screen.history_limits(), limits);
    assert_eq!(
        t.screen
            .normal
            .retained_rows()
            .flat_map(|row| row.cells.iter().cloned())
            .collect::<Vec<_>>(),
        cells
    );
    assert_eq!((t.screen.cursor.cursor.x, t.screen.cursor.cursor.y), cursor);
    let after_saved = t.screen.cursor.cursor.saved.as_ref().unwrap();
    assert_eq!(
        (after_saved.cursor_x, after_saved.cursor_y),
        (saved.cursor_x, saved.cursor_y)
    );
    assert_eq!(t.content_version().generation, version.generation);
    assert!(t.content_version().revision > version.revision);
    assert!(t.screen.normal.cell_at_generation(old_start, 0).is_none());
    accounting(&t.screen);
    // Current pen attributes also survive; erase is not a terminal reset.
    t.put_str("\x1b[?69lX");
    assert_eq!(t.screen.cell(0, 0).bg, Color::Named(5));
}

#[test]
fn csi_3j_streams_is_idempotent_and_does_not_disable_future_history() {
    let sequence = b"\x1b[3J";
    for split in 0..=sequence.len() {
        let mut t = terminal(1, 8, 4, 100);
        t.put_str("OLD\r\nLIVE");
        t.put_bytes(&sequence[..split]);
        t.put_bytes(&sequence[split..]);
        assert_eq!(t.screen.scroll_count(), 0, "split={split}");
        assert_eq!(&t.screen.row_text(0)[..4], "LIVE");
        let version = t.content_version();
        t.put_bytes(sequence);
        assert_eq!(t.content_version(), version);
        t.put_str("\r\nNEXT");
        assert_eq!(t.screen.scroll_count(), 1);
        accounting(&t.screen);
        t.resize(3, 8);
        let text: String = t
            .screen
            .normal
            .retained_rows()
            .flat_map(|row| row.cells.iter().map(|cell| cell.ch))
            .collect();
        assert!(!text.contains("OLD"));
        assert!(text.contains("LIVE"));
        assert!(text.contains("NEXT"));
    }

    let mut empty = terminal(1, 4, 0, 0);
    empty.put_str("ABCD");
    assert!(empty.screen.cursor.modes.pending_wrap);
    empty.put_bytes(sequence);
    assert!(!empty.screen.cursor.modes.pending_wrap);
    assert_eq!(empty.screen.cursor.cursor.x, 3);
    assert_eq!(empty.screen.row_text(0), "ABCD");
}

#[test]
fn csi_3j_marks_truncated_soft_line_and_invalidates_only_lost_read_evidence() {
    use crate::read_model::{ReadBudget, ReadError, ReadScope};

    let mut t = terminal(2, 4, 20, 100);
    t.put_str("abcdefghij\x1b7");
    assert_eq!(t.screen.scroll_count(), 1);
    let page = t
        .read_content(
            ReadScope::Retained {
                start_row: 0,
                row_count: 3,
            },
            ReadBudget {
                rows: 1,
                ..ReadBudget::default()
            },
        )
        .unwrap();
    let lost = page.fragments[0].range(0..4).unwrap();
    let continuation = page.continuation.as_ref().unwrap();
    let live = t
        .read_content(
            ReadScope::Visible {
                start_row: 1,
                row_count: 1,
            },
            ReadBudget::default(),
        )
        .unwrap()
        .fragments[0]
        .range(0..2)
        .unwrap();
    let first_live = t.screen.normal.live_row_metadata(0);
    let saved = t.screen.cursor.cursor.saved.clone().unwrap();

    t.put_str("\x1b[3J");

    let after = t.screen.normal.live_row_metadata(0);
    assert!(after.head_truncated);
    assert_eq!(after.logical_line_id, first_live.logical_line_id);
    assert_eq!(after.logical_start, first_live.logical_start);
    assert_eq!(after.logical_atom_start, first_live.logical_atom_start);
    assert!(!t.validate_content_range(&lost));
    assert!(t.validate_content_range(&live));
    assert!(matches!(
        t.continue_content_read(continuation, ReadBudget::default()),
        Err(ReadError::StaleContinuation)
    ));
    let after_saved = t.screen.cursor.cursor.saved.as_ref().unwrap();
    assert_eq!(
        (after_saved.cursor_x, after_saved.cursor_y),
        (saved.cursor_x, saved.cursor_y)
    );
    let after_anchor = after_saved.anchor.unwrap();
    let before_anchor = saved.anchor.unwrap();
    assert_eq!(after_anchor.line_id, before_anchor.line_id);
    assert_eq!(after_anchor.offset, before_anchor.offset);
    assert_eq!(after_anchor.affinity, before_anchor.affinity);
    assert_eq!(t.screen.row_text(0), "efgh");
    assert_eq!(&t.screen.row_text(1)[..2], "ij");
}

#[test]
fn csi_3j_returns_review_to_live_and_keeps_it_live_after_same_feed_output() {
    let mut t = terminal(2, 8, 20, 100);
    t.put_str("OLD\r\nMID\r\nLIVE");
    t.screen.scroll_up(1);
    assert!(t.screen.review_anchor.is_some());
    t.screen.clear_dirty();

    t.put_str("\x1b[3J\r\nNEW");

    assert_eq!(t.screen.view_offset(), 0);
    assert!(t.screen.review_anchor.is_none());
    assert_eq!(t.screen.scroll_count(), 1);
    assert_eq!(&t.screen.row_text(0)[..4], "LIVE");
    assert_eq!(&t.screen.row_text(1)[..3], "NEW");
    assert_eq!(t.screen.dirty_rows().len(), 2);
}

#[test]
fn csi_3j_preserves_saved_primary_and_private_3j_does_not_erase_history() {
    for mode in [47, 1047, 1049] {
        let mut t = terminal(2, 8, 20, 100);
        t.put_str("OLD\r\nMID\r\nLIVE");
        t.screen.scroll_up(1);
        let normal = (*t.screen.normal).clone();
        t.put_str("\x1b[?3J");
        assert_eq!(*t.screen.normal, normal);
        t.put_str(&format!("\x1b[?{mode}hALT\x1b[3J"));
        assert!(t.screen.is_alt());
        assert_eq!(&t.screen.row_text(0)[..3], "ALT");
        assert_eq!(*t.screen.saved_primary.as_ref().unwrap().normal, normal);
        t.put_str(&format!("\x1b[?{mode}l"));
        assert!(!t.screen.is_alt());
        assert_eq!(*t.screen.normal, normal);
        accounting(&t.screen);
    }
}

#[test]
fn csi_3j_reconciles_history_and_live_selection_endpoints() {
    use crate::{
        RenderViewport, TerminalEvent, TerminalPointerButton, TerminalPointerEvent,
        TerminalPointerPhase,
    };

    // Independent hard lines and one soft line spanning history/live must agree.
    for (cols, text, output) in [
        (8, "OLD-END\r\nLIVE-END", "\r\nNEXT"),
        (4, "abcdefgh", "ij"),
    ] {
        for selected_row in [0, 1, 2] {
            let mut t = terminal(2, cols, 20, 100);
            t.pointer
                .set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
            t.put_str(text);
            let (start_y, end_y) = match selected_row {
                0 => (1.0, 1.0),
                1 => (21.0, 21.0),
                _ => (1.0, 21.0), // Only the first endpoint will be evicted.
            };
            let end_x = (cols - 2) as f32 * 10.0 + 1.0;
            for (phase, x, y) in [
                (TerminalPointerPhase::Down, 1.0, start_y),
                (TerminalPointerPhase::Move, end_x, end_y),
                (TerminalPointerPhase::Up, end_x, end_y),
            ] {
                t.handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                    (x, y),
                    phase,
                    TerminalPointerButton::Left,
                    70,
                )))
                .unwrap();
            }
            assert!(t.has_non_empty_selection());
            t.put_str(output);
            let selected = t.selection_text();
            t.put_str("\x1b[3J");
            if selected_row == 1 {
                assert!(t.has_non_empty_selection());
                assert_eq!(t.selection_text(), selected);
            } else {
                assert!(!t.has_non_empty_selection());
                assert_eq!(t.selection_text(), "");
            }
        }
    }
}
