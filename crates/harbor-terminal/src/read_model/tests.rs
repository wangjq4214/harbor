use super::*;
use crate::TerminalOutputEvent;
use std::time::Instant;

fn retained(start_row: usize, row_count: usize) -> ReadScope {
    ReadScope::Retained {
        start_row,
        row_count,
    }
}

fn visible(row_count: usize) -> ReadScope {
    ReadScope::Visible {
        start_row: 0,
        row_count,
    }
}

fn text(page: &ReadPage) -> String {
    page.fragments
        .iter()
        .flat_map(|f| f.units())
        .map(|u| u.text.as_str())
        .collect()
}

fn first_range(terminal: &Terminal) -> ContentRange {
    terminal
        .read_content(visible(1), ReadBudget::default())
        .unwrap()
        .fragments[0]
        .range(0..1)
        .unwrap()
}

#[test]
fn visible_and_history_subsets_are_direct_and_report_coverage() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("old\r\nmid\r\nnew\r\nlive");
    let history = t
        .read_content(retained(0, 1), ReadBudget::default())
        .unwrap();
    assert_eq!(text(&history), "old");
    assert_eq!(history.rows_visited, 1);
    assert_eq!(history.selected_rows, 1);
    assert_eq!(history.copied_units, 3);
    assert_eq!(history.copied_payload_bytes, 3);
    assert!(history.continuation.is_none());
    assert_eq!(history.fragments[0].end(), FragmentEnd::HardBreak);
    let live = t.read_content(visible(3), ReadBudget::default()).unwrap();
    assert_eq!(text(&live), "midnewlive");
    let version = t.content_version();
    t.screen.scroll_up(1);
    let review = t.read_content(visible(1), ReadBudget::default()).unwrap();
    assert_eq!(text(&review), "old");
    assert_eq!(version, t.content_version(), "viewport is not content");
    let clipped = t
        .read_content(retained(3, usize::MAX), ReadBudget::default())
        .unwrap();
    assert_eq!(clipped.selected_rows, 1);
    assert!(clipped.scope_clipped);
    assert!(clipped.continuation.is_none());
    assert!(matches!(
        t.read_content(retained(5, 1), ReadBudget::default()),
        Err(ReadError::InvalidScope)
    ));
    let empty = t
        .read_content(retained(4, 0), ReadBudget::default())
        .unwrap();
    assert_eq!(empty.rows_visited, 0);
    assert!(empty.fragments.is_empty());
}

#[test]
fn subset_does_not_decode_or_copy_unrequested_history() {
    let mut t = Terminal::new_headless(2, 8);
    t.put_str(&"old\r\n".repeat(1000));
    // Deliberately malformed/unbounded suffix outside the selected subset. A
    // full-history decoder would fail/clone it before returning the live subset.
    t.screen.cell_mut(0, 0).wide_continuation = true;
    t.screen.cell_mut(0, 0).suffix = "\u{301}".repeat(200_000);
    t.put_str("\x1b[2;1Hok");
    let page = t
        .read_content(
            ReadScope::Visible {
                start_row: 1,
                row_count: 1,
            },
            ReadBudget {
                rows: 1,
                units: 2,
                payload_bytes: 2,
            },
        )
        .unwrap();
    assert_eq!(text(&page), "ok");
    assert_eq!(page.copied_payload_bytes, 2);
    assert_eq!(page.rows_visited, 1);
    assert!(matches!(
        t.read_content(visible(1), ReadBudget::default()),
        Err(ReadError::InvalidContent)
    ));
}

#[test]
fn long_wrapped_line_pages_preserve_offsets_and_never_invent_newlines() {
    let mut t = Terminal::new_headless(3, 5);
    let source = "abcdefghijklmnopqrstuvwxyz0123456789";
    t.put_str(source);
    let budget = ReadBudget {
        rows: 2,
        units: 3,
        payload_bytes: 3,
    };
    let mut page = t.read_content(retained(0, 8), budget).unwrap();
    let line = page.fragments[0].line();
    let mut collected = String::new();
    let mut offset = 0;
    let mut joined: Option<ContentRange> = None;
    let mut pages = 0;
    loop {
        pages += 1;
        assert!(page.rows_visited <= 2);
        assert!(page.copied_units <= 3);
        assert!(page.copied_payload_bytes <= 3);
        for fragment in &page.fragments {
            assert_eq!(fragment.line(), line);
            assert_eq!(fragment.atom_start().0, offset);
            assert_eq!(fragment.start() == FragmentStart::LineStart, offset == 0);
            assert_ne!(fragment.end(), FragmentEnd::HardBreak);
            offset += fragment.units().len();
            let range = fragment.range(0..fragment.units().len()).unwrap();
            joined = Some(match joined {
                Some(old) => old.join(&range).unwrap(),
                None => range,
            });
        }
        collected.push_str(&text(&page));
        let Some(cursor) = page.continuation.take() else {
            break;
        };
        page = t.continue_content_read(&cursor, budget).unwrap();
    }
    assert_eq!(collected, source);
    assert!(pages > 8);
    let range = joined.unwrap();
    assert_eq!(range.start().offset.0, 0);
    assert_eq!(range.end().offset.0, source.len());
    assert!(t.validate_content_range(&range));
}

#[test]
fn explicit_blank_lines_are_fragments_not_capacity_text_or_page_breaks() {
    let mut t = Terminal::new_headless(4, 10);
    t.put_str("a\r\n\r\nb");
    let budget = ReadBudget {
        rows: 1,
        ..ReadBudget::default()
    };
    let first = t.read_content(retained(0, 3), budget).unwrap();
    assert_eq!(first.fragments[0].end(), FragmentEnd::HardBreak);
    let blank = t
        .continue_content_read(first.continuation.as_ref().unwrap(), budget)
        .unwrap();
    let f = &blank.fragments[0];
    assert!(f.units().is_empty());
    assert_eq!(f.start(), FragmentStart::LineStart);
    assert_eq!(f.end(), FragmentEnd::HardBreak);
    assert!(t.validate_content_range(&f.range(0..0).unwrap()));
    let last = t
        .continue_content_read(blank.continuation.as_ref().unwrap(), budget)
        .unwrap();
    assert_eq!(text(&last), "b");
    assert!(last.continuation.is_none());
    assert!(
        first.fragments[0]
            .range(0..1)
            .unwrap()
            .join(&last.fragments[0].range(0..1).unwrap())
            .is_none()
    );
}

#[test]
fn hard_limits_and_oversized_units_have_explicit_non_looping_errors() {
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("a\u{301}");
    for budget in [
        ReadBudget {
            rows: 0,
            ..ReadBudget::default()
        },
        ReadBudget {
            rows: MAX_READ_ROWS + 1,
            ..ReadBudget::default()
        },
        ReadBudget {
            units: MAX_READ_UNITS + 1,
            ..ReadBudget::default()
        },
        ReadBudget {
            payload_bytes: MAX_READ_PAYLOAD_BYTES + 1,
            ..ReadBudget::default()
        },
    ] {
        assert!(matches!(
            t.read_content(visible(1), budget),
            Err(ReadError::InvalidBudget)
        ));
    }
    let tiny = ReadBudget {
        rows: 1,
        units: 1,
        payload_bytes: 2,
    };
    assert!(matches!(
        t.read_content(visible(1), tiny),
        Err(ReadError::UnitTooLarge {
            required_payload_bytes: 3
        })
    ));
    let exact = t
        .read_content(
            visible(1),
            ReadBudget {
                payload_bytes: 3,
                ..tiny
            },
        )
        .unwrap();
    assert_eq!(text(&exact), "a\u{301}");
    assert_eq!(exact.copied_units, 1);
    assert!(exact.continuation.is_none());
    t.screen.cell_mut(0, 0).suffix = "\u{301}".repeat(MAX_READ_PAYLOAD_BYTES);
    assert!(matches!(
        t.read_content(
            visible(1),
            ReadBudget {
                payload_bytes: MAX_READ_PAYLOAD_BYTES,
                ..ReadBudget::default()
            }
        ),
        Err(ReadError::UnitTooLarge { .. })
    ));
}

#[test]
fn unicode_units_styles_blanks_and_generated_padding_use_shared_semantics() {
    let mut t = Terminal::new_headless(6, 4);
    t.put_str("\u{301}a\u{301}界👩\u{200d}💻\r\n\x1b[4;53m  \x1b[0m");
    let page = t
        .read_content(retained(0, 4), ReadBudget::default())
        .unwrap();
    let units: Vec<_> = page.fragments.iter().flat_map(|f| f.units()).collect();
    assert_eq!(
        units.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(),
        ["\u{301}", "a\u{301}", "界", "👩\u{200d}💻", " ", " "]
    );
    assert_eq!(
        units.iter().map(|u| u.cell_width).collect::<Vec<_>>(),
        [1, 1, 2, 2, 1, 1]
    );
    assert!(units[4].meaningful_blank);
    assert!(
        units[4]
            .style
            .attrs
            .contains(CellAttrs::UNDERLINE | CellAttrs::OVERLINE)
    );
    assert!(!text(&page).contains('◌'));
    // A wide unit at the right edge produces a generated padding cell, not text.
    let mut edge = Terminal::new_headless(3, 3);
    edge.put_str("ab界");
    let page = edge
        .read_content(retained(0, 2), ReadBudget::default())
        .unwrap();
    assert_eq!(text(&page), "ab界");
    assert_eq!(page.fragments[0].end(), FragmentEnd::SoftWrap);
    assert_eq!(page.fragments[1].atom_start().0, 2);
}

#[test]
fn links_are_budgeted_owned_values_safe_after_cleanup_reset_and_engine_drop() {
    let link = {
        let mut t = Terminal::new_headless(2, 8);
        t.put_str("\x1b]8;id=tag;https://example.test\x1b\\X\x1b]8;;\x1b\\");
        let bytes = 1 + "https://example.test".len() + 3;
        let page = t
            .read_content(
                visible(1),
                ReadBudget {
                    payload_bytes: bytes,
                    ..ReadBudget::default()
                },
            )
            .unwrap();
        assert_eq!(page.copied_payload_bytes, bytes);
        assert!(
            matches!(t.read_content(visible(1), ReadBudget { payload_bytes: bytes - 1, ..ReadBudget::default() }),
            Err(ReadError::UnitTooLarge { required_payload_bytes }) if required_payload_bytes == bytes)
        );
        let link = page.fragments[0].units()[0].hyperlink.clone().unwrap();
        t.put_str("\r\x1b[2K");
        for i in 0..600 {
            t.put_str(&format!(
                "\x1b]8;;https://example.test/{i}\x1b\\\rX\x1b]8;;\x1b\\"
            ));
        }
        t.put_str("\x1bc");
        assert_eq!(link.id.as_deref(), Some("tag"));
        link
    };
    assert_eq!(link.uri, "https://example.test");
}

#[test]
fn independent_readers_do_not_acknowledge_damage_or_consume_parser_events() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("a\r\nb\x1b]133;A\x07");
    let now = Instant::now();
    let pending = t.read_update(now);
    let version = t.content_version();
    let budget = ReadBudget {
        rows: 1,
        ..ReadBudget::default()
    };
    let a = t.read_content(retained(0, 3), budget).unwrap();
    let b = t.read_content(retained(0, 3), budget).unwrap();
    let a2 = t
        .continue_content_read(a.continuation.as_ref().unwrap(), budget)
        .unwrap();
    let b2 = t
        .continue_content_read(b.continuation.as_ref().unwrap(), budget)
        .unwrap();
    assert_eq!(text(&a2), text(&b2));
    assert_eq!(
        t.read_update(now).snapshot,
        pending.snapshot,
        "failed/skipped draw remains replayable"
    );
    assert!(
        t.drain_output_events()
            .iter()
            .any(|e| matches!(e, TerminalOutputEvent::ShellIntegration(_)))
    );
    assert!(t.acknowledge_update(&pending));
    assert_eq!(t.content_version(), version);
    t.invalidate_update();
    assert_eq!(t.content_version(), version);
    assert_eq!(
        text(
            &t.continue_content_read(a.continuation.as_ref().unwrap(), budget)
                .unwrap()
        ),
        "b"
    );
}

#[test]
fn stale_continuations_reject_overwrite_even_when_anchors_resolve() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("abcd\r\nother");
    let budget = ReadBudget {
        units: 2,
        ..ReadBudget::default()
    };
    let old = t.read_content(retained(0, 3), budget).unwrap();
    let range = old.fragments[0].range(0..2).unwrap();
    let pos = range.start();
    let anchor = crate::content_anchor::ContentAnchor::from_logical_parts(
        pos.line,
        pos.offset,
        pos.affinity,
        None,
        false,
    );
    t.put_str("\x1b[1;1Hzz");
    assert!(
        t.screen
            .content_projection()
            .unwrap()
            .resolve_selection(anchor)
            .is_some()
    );
    assert!(!t.validate_content_range(&range));
    assert!(matches!(
        t.continue_content_read(old.continuation.as_ref().unwrap(), budget),
        Err(ReadError::StaleContinuation)
    ));
    assert!(matches!(
        t.rebase_content_read(old.continuation.as_ref().unwrap()),
        Err(ReadError::StaleContinuation)
    ));
}

#[test]
fn unchanged_retained_ranges_and_explicit_rebase_progress_under_output() {
    let mut t = Terminal::new_headless(3, 16);
    t.put_str(&"history\r\n".repeat(80));
    let budget = ReadBudget {
        rows: 2,
        units: 32,
        payload_bytes: 64,
    };
    let mut page = t.read_content(retained(0, 60), budget).unwrap();
    let first = page.fragments[0].range(0..7).unwrap();
    let initial_version = page.version;
    let mut consumed = 0;
    loop {
        consumed += page.fragments.len();
        let Some(cursor) = page.continuation.take() else {
            break;
        };
        t.put_str("output\r\n");
        assert!(t.validate_content_range(&first));
        assert!(matches!(
            t.continue_content_read(&cursor, budget),
            Err(ReadError::StaleContinuation)
        ));
        let rebased = t.rebase_content_read(&cursor).unwrap();
        page = t.continue_content_read(&rebased, budget).unwrap();
        assert_ne!(page.version, initial_version);
    }
    assert_eq!(
        consumed, 60,
        "fixed-scope progress, not a full-history restart"
    );
    let refreshed = t.refresh_content_range(&first).unwrap();
    assert_eq!(refreshed.version(), t.content_version());
}

#[test]
fn mutations_invalidate_row_proofs_but_not_unrelated_surviving_rows() {
    for operation in [
        "\x1b[1;1HX",
        "\x1b[1;1H\x1b[K",
        "\x1b[1;1H\x1b[@",
        "\x1b[1;1H\x1b[P",
        "\x1b[1;1H\x1b[L",
        "\x1b[1;1H\x1b[M",
        "\x1b[2J",
        "\x1b[1S",
        "\x1b[1T",
        "\x1b[1;1;1;1$r",
    ] {
        let mut t = Terminal::new_headless(3, 8);
        t.put_str("abc\r\ndef\r\nghi");
        let range = first_range(&t);
        let version = t.content_version();
        t.put_str(operation);
        assert_ne!(version, t.content_version(), "operation={operation:?}");
        assert!(!t.validate_content_range(&range), "operation={operation:?}");
    }
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("abc\r\ndef\r\nghi");
    let first = first_range(&t);
    t.put_str("\x1b[3;1HX");
    assert!(t.validate_content_range(&first));
}

#[test]
fn partial_eviction_preserves_absolute_offsets_and_ring_reuse_never_aliases() {
    let mut t = Terminal::new_headless(2, 3);
    // Fill the real bounded ring (1000 history + 2 live rows) with one line.
    t.put_str(&"abc".repeat(1002));
    let old = t
        .read_content(retained(0, 4), ReadBudget::default())
        .unwrap();
    let lost = old.fragments[0].range(0..3).unwrap();
    let survivor = old.fragments[2].range(0..3).unwrap();
    t.put_str("mnopqr");
    let page = t
        .read_content(retained(0, 4), ReadBudget::default())
        .unwrap();
    assert_eq!(text(&page), "abc".repeat(4));
    assert_eq!(page.fragments[0].atom_start().0, 6);
    assert_eq!(
        page.fragments[0].start(),
        FragmentStart::Continuation {
            prefix_evicted: true
        }
    );
    assert!(!t.validate_content_range(&lost));
    // The newly truncated first row is conservatively invalidated; later rows survive.
    assert!(!t.validate_content_range(&survivor));
    let retained_tail = old.fragments[3].range(0..3).unwrap();
    assert!(t.validate_content_range(&retained_tail));
    t.put_str("\r\nnew\r\nnew\r\nnew\r\nnew");
    assert!(!t.validate_content_range(&retained_tail));
}

#[test]
fn geometry_reset_replacement_and_buffer_transitions_have_distinct_lifetimes() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("same");
    let primary = first_range(&t);
    t.put_str("\x1b[?47h");
    t.put_str("same");
    let alternate = first_range(&t);
    assert_eq!(alternate.version().buffer, BufferKind::Alternate);
    assert!(!t.validate_content_range(&primary));
    t.put_str("\x1b[?47l");
    assert!(t.validate_content_range(&primary));
    assert!(!t.validate_content_range(&alternate));
    t.put_str("\x1b[?47h");
    assert!(t.validate_content_range(&alternate));
    t.resize(4, 4); // active rectangular alt and saved primary reflow
    assert!(!t.validate_content_range(&alternate));
    t.put_str("\x1b[?47l");
    assert!(!t.validate_content_range(&primary));
    let resized = first_range(&t);
    t.resize(5, 4); // height only
    assert!(!t.validate_content_range(&resized));
    let before_reset = first_range(&t);
    let session = t.content_version().session;
    t.put_str("\x1bcsame");
    assert_eq!(session, t.content_version().session);
    assert!(!t.validate_content_range(&before_reset));
    let current = first_range(&t);
    let mut replacement = Terminal::new_headless(5, 4);
    replacement.put_str("same");
    assert_ne!(replacement.content_version().session, session);
    assert!(!replacement.validate_content_range(&current));
    t.put_str("\x1b[?1049hsame\x1b[?1049l\x1b[?1049hsame");
    assert!(!t.validate_content_range(&alternate));
}

#[test]
fn targets_are_versioned_data_without_source_or_renderer_side_effects() {
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("target");
    let before = t.snapshot();
    let target = ActionTarget {
        range: first_range(&t),
        action: TargetAction::Navigate,
    };
    assert!(t.validate_action_target(&target));
    assert_eq!(before, t.snapshot());
    t.put_str("\rX");
    assert!(!t.validate_action_target(&target));
}

#[test]
fn bounded_reads_agree_with_shared_decoder_before_and_after_reflow() {
    let mut t = Terminal::new_headless(5, 7);
    t.put_str("a  b\r\n\u{301}e\u{301}界👩\u{200d}💻\r\n\r\n\x1b[4;53m  \x1b[0m");
    for cols in [7, 3, 11, 4] {
        t.resize(5, cols);
        let expected = crate::logical_content::decode(t.screen.read_buffer()).unwrap();
        let expected_text: String = expected
            .iter()
            .filter_map(|atom| match atom {
                crate::logical_content::LogicalAtom::Glyph { glyph, .. } => {
                    Some(glyph.cell.raw_text())
                }
                _ => None,
            })
            .collect();
        let budget = ReadBudget {
            rows: 1,
            units: 2,
            payload_bytes: 64,
        };
        let mut page = t.read_content(retained(0, usize::MAX), budget).unwrap();
        let mut actual_text = String::new();
        let mut actual_breaks = 0;
        loop {
            actual_text.push_str(&text(&page));
            actual_breaks += page
                .fragments
                .iter()
                .filter(|f| f.end() == FragmentEnd::HardBreak)
                .count();
            let Some(cursor) = page.continuation.take() else {
                break;
            };
            page = t.continue_content_read(&cursor, budget).unwrap();
        }
        let expected_breaks = expected
            .iter()
            .filter(|a| matches!(a, crate::logical_content::LogicalAtom::HardBreak { .. }))
            .count();
        assert_eq!(actual_text, expected_text);
        assert_eq!(actual_breaks, expected_breaks);
    }
}

#[test]
fn read_boundary_does_not_restore_ordinary_or_conpty_clipped_tails() {
    let mut ordinary = Terminal::new_headless(3, 12);
    ordinary.put_str("text      \r\nend");
    assert!(
        text(
            &ordinary
                .read_content(retained(0, 3), ReadBudget::default())
                .unwrap()
        )
        .contains("text      ")
    );
    ordinary.resize(3, 5);
    ordinary.resize(3, 12);
    assert_eq!(
        text(
            &ordinary
                .read_content(retained(0, 8), ReadBudget::default())
                .unwrap()
        ),
        "textend"
    );

    for (policy, retained_spaces) in [
        (crate::primary_reflow::ReflowViewport::PullHistory, 8),
        (crate::primary_reflow::ReflowViewport::PreserveLiveTop, 1),
    ] {
        let mut t = Terminal::new_headless(4, 12);
        t.put_str("text\x1b[4;53m        \x1b[0m\r\nend");
        let old = first_range(&t);
        let prepared = t.screen.prepare_resize_with_viewport(4, 5, policy).unwrap();
        t.screen.commit_resize(prepared);
        assert!(!t.validate_content_range(&old));
        let expected = format!("text{}end", " ".repeat(retained_spaces));
        assert_eq!(
            text(
                &t.read_content(retained(0, 10), ReadBudget::default())
                    .unwrap()
            ),
            expected
        );
        let prepared = t
            .screen
            .prepare_resize_with_viewport(4, 12, policy)
            .unwrap();
        t.screen.commit_resize(prepared);
        assert_eq!(
            text(
                &t.read_content(retained(0, 10), ReadBudget::default())
                    .unwrap()
            ),
            expected
        );
    }
}

#[test]
fn unsafe_link_opens_never_become_read_metadata_and_exact_registry_limits_fit() {
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("\x1b]8;;https://bad\nlink\x1b\\X\x1b]8;;\x1b\\");
    let page = t.read_content(visible(1), ReadBudget::default()).unwrap();
    assert!(
        page.fragments
            .iter()
            .flat_map(|f| f.units())
            .all(|u| u.hyperlink.is_none())
    );
    let uri = format!("https://{}", "x".repeat(2048 - 8));
    let id = "i".repeat(250);
    t.put_str(&format!("\r\x1b]8;id={id};{uri}\x1b\\Y\x1b]8;;\x1b\\"));
    let page = t.read_content(visible(1), ReadBudget::default()).unwrap();
    assert_eq!(page.copied_payload_bytes, 2299);
    assert_eq!(
        page.fragments[0].units()[0].hyperlink.as_ref().unwrap().uri,
        uri
    );
}

#[test]
fn joining_rebased_pages_requires_explicit_range_refresh_and_bounds_proofs() {
    let mut t = Terminal::new_headless(3, 5);
    t.put_str(&"a".repeat(400));
    let budget = ReadBudget {
        rows: 1,
        ..ReadBudget::default()
    };
    let page = t.read_content(retained(0, 70), budget).unwrap();
    let first = page.fragments[0].range(0..5).unwrap();
    t.put_str("\r\noutput");
    let cursor = t
        .rebase_content_read(page.continuation.as_ref().unwrap())
        .unwrap();
    let next = t.continue_content_read(&cursor, budget).unwrap();
    let second = next.fragments[0].range(0..5).unwrap();
    assert!(first.join(&second).is_none());
    let refreshed = t.refresh_content_range(&first).unwrap();
    let both = refreshed.join(&second).unwrap();
    assert!(t.validate_content_range(&both));
    assert_eq!(both.end().offset.0, 10);
    assert!(page.fragments[0].range(5..6).is_none());
    // Real source text may exceed the maximum proof retention per neutral range.
    let mut t = Terminal::new_headless(2, 2);
    t.put_str(&"a".repeat((MAX_READ_ROWS + 1) * 2));
    let first = t
        .read_content(
            retained(0, MAX_READ_ROWS + 1),
            ReadBudget {
                rows: MAX_READ_ROWS,
                ..ReadBudget::default()
            },
        )
        .unwrap();
    let joined = first
        .fragments
        .iter()
        .map(|f| f.range(0..2).unwrap())
        .reduce(|a, b| a.join(&b).unwrap())
        .unwrap();
    let last = t
        .continue_content_read(first.continuation.as_ref().unwrap(), budget)
        .unwrap();
    assert!(
        joined
            .join(&last.fragments[0].range(0..2).unwrap())
            .is_none()
    );
}

#[test]
fn discarded_resize_preparation_does_not_invalidate_live_content() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("keep");
    let range = first_range(&t);
    let version = t.content_version();
    let prepared = t.screen.prepare_resize(4, 5).unwrap();
    drop(prepared); // e.g. failed PTY resize before commit
    assert_eq!(version, t.content_version());
    assert!(t.validate_content_range(&range));
    t.resize(3, 8); // normalized no-op resize also keeps validity
    assert_eq!(version, t.content_version());
    assert!(t.validate_content_range(&range));
}
