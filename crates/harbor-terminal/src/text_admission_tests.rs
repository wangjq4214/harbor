//! The ADR 0075 oversized-input exception, not a new Unicode/fallback policy.
use crate::{
    Cell, SelectionBounds, Terminal,
    literal_search::{LiteralSearch, SearchStatus},
    model::MAX_TEXT_UNIT_BYTES,
    read_model::{ContentRange, ReadBudget, ReadScope},
};

fn scope() -> ReadScope {
    ReadScope::Retained {
        start_row: 0,
        row_count: usize::MAX,
    }
}

fn read_text(t: &Terminal) -> String {
    let page = t.read_content(scope(), ReadBudget::default()).unwrap();
    assert!(page.continuation.is_none());
    page.fragments
        .iter()
        .flat_map(|f| f.units())
        .map(|u| u.text.as_str())
        .collect()
}

fn matches(t: &Terminal, query: &str) -> Vec<ContentRange> {
    let mut gate = LiteralSearch::default();
    let mut job = gate
        .start(t, query, scope(), ReadBudget::default())
        .unwrap();
    for _ in 0..100 {
        if job.step(t) != SearchStatus::Running {
            return gate
                .accept(t, job.into_results())
                .unwrap()
                .ranges()
                .to_vec();
        }
    }
    panic!("search did not finish");
}

fn copy_all(t: &Terminal) -> String {
    let screen = &t.screen;
    screen.selected_text(SelectionBounds {
        start_row: screen.history_start(),
        start_col: 0,
        end_row: screen.history_start() + (screen.scroll_count() + screen.rows() - 1) as u64,
        end_col: screen.cols() - 1,
    })
}

fn resize(t: &mut Terminal, rows: usize, cols: usize) {
    let prepared = t.screen.prepare_resize(rows, cols).unwrap();
    t.screen.commit_resize(prepared);
}

#[test]
fn text_admission_exact_limit_and_sticky_scalar_aligned_prefix() {
    assert_eq!(MAX_TEXT_UNIT_BYTES, 1024, "ADR 0075 contract");
    assert_eq!(std::mem::size_of::<Cell>(), 56, "calibrated cell backing");
    let exact = format!("a{}\u{fe0f}", "\u{301}".repeat(510));
    let short = format!("a{}", "\u{301}".repeat(511));
    for prefix in [exact, short] {
        let mut t = Terminal::new_headless(2, 8);
        t.put_str(&prefix);
        let capacity = t.screen.cell(0, 0).suffix.capacity();
        let cursor = (t.screen.cursor_x(), t.screen.cursor_y());
        // The 4-byte selector does not fit. A subsequent smaller scalar must
        // still be dropped, rather than filling the remaining prefix space.
        for _ in 0..100 {
            t.put_str("\u{e0100}\u{301}\u{fe0f}");
        }
        assert_eq!(t.screen.cell(0, 0).raw_text(), prefix);
        assert_eq!(t.screen.cell(0, 0).suffix.capacity(), capacity);
        assert!(capacity <= MAX_TEXT_UNIT_BYTES);
        assert_eq!((t.screen.cursor_x(), t.screen.cursor_y()), cursor);
        t.put_str("B\u{302}");
        assert_eq!(t.screen.cell(0, 1).raw_text(), "B\u{302}");
        assert_eq!(read_text(&t), format!("{prefix}B\u{302}"));
    }
}

#[test]
fn text_admission_fragmentation_and_discarded_zwj_tail_recover_in_both_buffers() {
    // One prefix retains its joiner, the other drops its joiner. Both drop
    // the joined laptop, then admit the next independent woman emoji.
    for prefix in [
        format!("♥{}\u{200d}", "\u{301}".repeat(509)),
        format!("♥{}", "\u{301}".repeat(510)),
    ] {
        let joiner = if prefix.ends_with('\u{200d}') {
            ""
        } else {
            "\u{200d}"
        };
        let body = format!("{prefix}{joiner}💻👩X\u{302}");
        for alternate in [false, true] {
            let source = format!("{}{body}", if alternate { "\x1b[?1049h" } else { "" });
            let mut expected_cells = None;
            for chunk in [usize::MAX, 1, 2, 3, 7, 1023, 1024] {
                let mut t = Terminal::new_headless(2, 8);
                for bytes in source.as_bytes().chunks(chunk) {
                    t.put_bytes(bytes);
                }
                assert_eq!(t.screen.is_alt(), alternate);
                assert_eq!(t.screen.cell(0, 0).raw_text(), prefix);
                assert_eq!(t.screen.cell(0, 0).grid_width(), 1);
                assert_eq!(t.screen.cell(0, 1).raw_text(), "👩");
                assert!(t.screen.cell(0, 2).wide_continuation);
                assert_eq!(t.screen.cell(0, 3).raw_text(), "X\u{302}");
                assert_eq!(t.screen.cursor_x(), 4);
                assert_eq!(read_text(&t), format!("{prefix}👩X\u{302}"));
                let cells = t.snapshot().cells;
                if let Some(expected) = &expected_cells {
                    assert_eq!(&cells, expected, "chunk={chunk}, alternate={alternate}");
                } else {
                    expected_cells = Some(cells);
                }
            }
        }
    }
}

#[test]
fn text_admission_long_joined_growth_is_bounded_before_history() {
    let source = format!("👩{}", "\u{200d}💻".repeat(4096));
    let mut prefix = String::new();
    for ch in source.chars() {
        if prefix.len() + ch.len_utf8() > MAX_TEXT_UNIT_BYTES {
            break;
        }
        prefix.push(ch);
    }
    assert_eq!(prefix.len(), 1022);
    assert!(prefix.ends_with('\u{200d}'));
    for alternate in [false, true] {
        for chunk in [1, 7, 4096] {
            let mut t = Terminal::new_headless(2, 8);
            if alternate {
                t.put_str("\x1b[?1049h");
            }
            for bytes in source.as_bytes().chunks(chunk) {
                t.put_bytes(bytes);
            }
            assert_eq!(t.screen.scroll_count(), 0);
            assert_eq!(t.screen.cell(0, 0).raw_text(), prefix);
            assert!(t.screen.cell(0, 0).suffix.capacity() <= MAX_TEXT_UNIT_BYTES);
            assert_eq!(t.screen.cursor_x(), 2);
            t.put_str("💡");
            assert_eq!(t.screen.cell(0, 2).raw_text(), "💡");
            assert_eq!(read_text(&t), format!("{prefix}💡"));
        }
    }
}

#[test]
fn text_admission_dropped_selector_has_no_promotion_anchor_damage_or_rep_effect() {
    let prefix = format!("♥{}", "\u{301}".repeat(510)); // 1023 bytes
    for insert in [false, true] {
        let mut t = Terminal::new_headless(2, 12);
        t.screen.set_external_anchor_interest(true);
        t.put_str("\x1b[2GNEIGHBOR\r");
        t.put_str(&prefix);
        if insert {
            t.put_str("\x1b[4h");
        }
        let ranges = matches(&t, "NEIGHBOR");
        assert_eq!(ranges.len(), 1);
        let (_, anchors) = t.screen.finish_anchor_mutations(None).unwrap();
        let version = t.content_version();
        t.screen.clear_dirty();
        t.put_str("\u{fe0f}\u{200d}💻");
        assert_eq!(t.content_version(), version);
        assert!(t.screen.dirty_ranges().is_empty());
        assert_eq!(t.screen.cell(0, 0).grid_width(), 1);
        assert_eq!(t.screen.cell(0, 0).raw_text(), prefix);
        assert_eq!(t.screen.cursor_x(), 1);
        assert!(t.validate_content_range(&ranges[0]));
        let (changes, _) = t.screen.finish_anchor_mutations(Some(&anchors)).unwrap();
        assert!(changes.iter().next().is_none());
        // REP must repeat the last admitted mark, not resurrect the discarded
        // laptop as a new unit; repeated marks stay suppressed at this base.
        t.put_str("\x1b[3b");
        assert_eq!(t.screen.cursor_x(), 1);
        assert_eq!(t.screen.cell(0, 0).raw_text(), prefix);
        assert_eq!(read_text(&t), format!("{prefix}NEIGHBOR"));
    }
}

#[test]
fn text_admission_pending_wrap_and_no_wrap_margin_do_not_move_on_discard() {
    let prefix = format!("♥{}", "\u{301}".repeat(510));
    for autowrap in [false, true] {
        let mut t = Terminal::new_headless(2, 4);
        if !autowrap {
            t.put_str("\x1b[?7l");
        }
        t.put_str("abc");
        t.put_str(&prefix);
        let pending = t.screen.pending_wrap();
        t.screen.clear_dirty();
        t.put_str("\u{fe0f}\u{200d}💻");
        assert_eq!(t.screen.cell(0, 3).raw_text(), prefix);
        assert_eq!(t.screen.cell(0, 3).grid_width(), 1);
        assert_eq!(t.screen.pending_wrap(), pending);
        assert_eq!((t.screen.cursor_x(), t.screen.cursor_y()), (3, 0));
        assert!(t.screen.dirty_ranges().is_empty());
        t.put_str("Z");
        assert_eq!(
            t.screen
                .cell(usize::from(autowrap), if autowrap { 0 } else { 3 })
                .ch,
            'Z'
        );
    }
    // Within-limit no-wrap final-column selector width is deliberately NOT
    // repaired by this exception (Spec 0015's independent known discrepancy).
    let mut t = Terminal::new_headless(1, 4);
    t.put_str("\x1b[?7labc♥️");
    assert_eq!(t.screen.cell(0, 3).raw_text(), "♥️");
    assert_eq!(t.screen.cell(0, 3).grid_width(), 1);
}

#[test]
fn text_admission_copy_read_search_render_and_history_reflow_share_prefix() {
    let prefix = format!("界{}", "\u{301}".repeat(510));
    let mut t = Terminal::new_headless(3, 8);
    t.put_str(&format!("{prefix}\u{e0100}\u{200d}💻Z"));
    assert_eq!(read_text(&t), format!("{prefix}Z"));
    assert_eq!(copy_all(&t), format!("{prefix}Z\n\n"));
    assert_eq!(t.snapshot().cells[0].raw_text(), prefix);
    assert!(t.snapshot().cells[1].wide_continuation);
    assert_eq!(matches(&t, "界").len(), 1);
    for dropped in ["\u{e0100}", "💻", "◌"] {
        assert!(matches(&t, dropped).is_empty());
    }
    // Move the unit to history, then reflow it more than once. Consumers keep
    // the exact source and a single logical atom, not discarded/orphan units.
    t.put_str("\r\nnext\r\nlast\r\nlive");
    for cols in [4, 10, 8] {
        resize(&mut t, 3, cols);
        assert!(copy_all(&t).starts_with(&format!("{prefix}Z\nnext\nlast\nlive")));
        assert_eq!(read_text(&t), format!("{prefix}Znextlastlive"));
        let ranges = matches(&t, "界");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end().offset.0 - ranges[0].start().offset.0, 1);
        assert!(matches(&t, "💻").is_empty());
    }
}

#[test]
fn text_admission_overflow_state_follows_edits_resize_and_saved_primary() {
    let prefix = format!("♥{}\u{200d}", "\u{301}".repeat(509));
    let mut t = Terminal::new_headless(2, 8);
    t.put_str(&format!("{prefix}💻")); // Input tail is not the retained ZWJ.
    t.put_str("\r\x1b[@"); // Move whole unit one column to the right.
    assert_eq!(t.screen.cell(0, 1).raw_text(), prefix);
    t.put_str("\x1b[3G\u{301}"); // A moved overflowed unit still rejects marks.
    assert_eq!(t.screen.cell(0, 1).raw_text(), prefix);
    resize(&mut t, 2, 10);
    t.put_str("\x1b[?1049h");
    t.put_str(&format!("{prefix}💻"));
    resize(&mut t, 2, 9); // Rectangular alt resize; primary saved behind it.
    t.put_str("👩"); // Must be independent, even though prefix ends in ZWJ.
    assert_eq!(t.screen.cell(0, 1).raw_text(), "👩");
    t.put_str("\x1b[?1049l👩");
    assert_eq!(t.screen.cell(0, 1).raw_text(), prefix);
    assert_eq!(t.screen.cell(0, 2).raw_text(), "👩");
    // Erase and overwrite reset admission state rather than poisoning reuse.
    t.put_str("\r\x1b[2G\x1b[Xe\u{301}");
    assert_eq!(t.screen.cell(0, 1).raw_text(), "e\u{301}");
    t.put_str("\r\x1b[2G♥️");
    assert_eq!(t.screen.cell(0, 1).raw_text(), "♥️");
    assert_eq!(t.screen.cell(0, 1).grid_width(), 2);
    assert!(t.screen.cell(0, 2).wide_continuation);
}

#[test]
fn text_admission_ordinary_and_isolated_units_keep_source_width_and_damage() {
    let controls = [
        (format!("a{}", "\u{301}".repeat(32)), 1, false), // 65 B
        (format!("♥{}\u{fe0f}", "\u{200d}♥".repeat(37)), 2, false), // 228 B
        ("♥️".into(), 2, false),
        ("👩\u{200d}💻".into(), 2, false),
        ("界\u{301}".into(), 2, false),
        ("\u{301}".repeat(512), 1, true), // exact 1024 B, isolated cue is display only
        ("\u{e0100}".into(), 1, false),
    ];
    for (source, width, isolated) in controls {
        let mut t = Terminal::new_headless(2, 8);
        t.screen.clear_dirty();
        for byte in source.as_bytes() {
            t.put_bytes(&[*byte]);
        }
        let cell = t.screen.cell(0, 0);
        assert_eq!(cell.raw_text(), source);
        assert_eq!(cell.grid_width(), width);
        assert_eq!(cell.isolated_mark, isolated);
        assert_eq!(t.screen.cursor_x(), usize::from(width));
        assert!(!t.screen.dirty_ranges().is_empty());
        assert_eq!(copy_all(&t), format!("{source}\n"));
        assert_eq!(read_text(&t), source);
        assert_eq!(t.snapshot().cells[0].raw_text(), source);
        assert!(matches(&t, "◌").is_empty());
    }
}
