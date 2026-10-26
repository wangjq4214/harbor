use super::*;
use crate::read_model::{BufferKind, MAX_READ_UNITS};

fn retained(rows: usize) -> ReadScope {
    ReadScope::Retained {
        start_row: 0,
        row_count: rows,
    }
}

fn tiny() -> ReadBudget {
    ReadBudget {
        rows: 1,
        units: 2,
        payload_bytes: 64,
    }
}

fn finish(job: &mut SearchJob, terminal: &Terminal) {
    for _ in 0..100_000 {
        let status = job.step(terminal);
        assert_work(job);
        if status != SearchStatus::Running {
            return;
        }
    }
    panic!("bounded steps did not progress");
}

fn assert_work(job: &SearchJob) {
    let work = job.last_work();
    assert!(work.read_calls <= 1);
    assert!(work.rows_visited <= job.budget.rows);
    assert!(work.copied_units <= job.budget.units);
    assert!(work.copied_payload_bytes <= job.budget.payload_bytes);
    assert!(work.scanned_bytes <= work.copied_payload_bytes);
    assert!(work.range_refreshes <= MAX_QUERY_BYTES);
    assert!(job.results.ranges.len() <= MAX_SEARCH_RESULTS);
    assert!(job.results.ranges.capacity() <= MAX_SEARCH_RESULTS);
    if let Some(scan) = &job.scan {
        assert!(scan.pattern.len() <= MAX_QUERY_BYTES);
        assert_eq!(scan.prefix.len(), scan.pattern.len());
        assert!(scan.recent.len() <= scan.pattern.len());
        assert!(scan.recent.capacity() <= 256);
    }
}

fn search(
    gate: &mut LiteralSearch,
    terminal: &Terminal,
    query: &str,
    scope: ReadScope,
) -> AcceptedSearch {
    let mut job = gate.start(terminal, query, scope, tiny()).unwrap();
    finish(&mut job, terminal);
    gate.accept(terminal, job.into_results()).unwrap()
}

fn offsets(results: &AcceptedSearch) -> Vec<(usize, usize)> {
    results
        .ranges()
        .iter()
        .map(|r| (r.start().offset.0, r.end().offset.0))
        .collect()
}

#[test]
fn csi_3j_refreshes_search_without_erased_history_or_stale_targets() {
    let mut t = Terminal::new_headless(1, 12);
    t.put_str("OLD hit\r\nhit LIVE");
    let mut gate = LiteralSearch::default();
    let before = search(&mut gate, &t, "hit", retained(usize::MAX));
    assert_eq!(before.ranges().len(), 2);
    let old_target = gate.target(&t, &before, 0).unwrap();
    let live_target = gate.target(&t, &before, 1).unwrap();
    let version = t.content_version();

    t.put_str("\x1b[3J");

    assert_ne!(t.content_version(), version);
    assert!(!t.validate_action_target(&old_target));
    assert!(t.validate_action_target(&live_target));
    assert!(gate.target(&t, &before, 0).is_none());
    assert!(gate.target(&t, &before, 1).is_some());
    assert!(matches!(
        gate.accept(&t, before.into_results()),
        Err(Rejection::StaleText)
    ));
    let after = search(&mut gate, &t, "hit", retained(usize::MAX));
    assert_eq!(after.ranges().len(), 1);
    assert_eq!(after.results().observed_version(), t.content_version());
    assert!(
        search(&mut gate, &t, "OLD", retained(usize::MAX))
            .ranges()
            .is_empty()
    );
}

#[test]
fn paged_soft_wraps_overlap_but_never_join_hard_or_blank_lines() {
    let mut t = Terminal::new_headless(4, 4);
    t.put_str("ababaX\r\naba\r\n\r\nba");
    let mut gate = LiteralSearch::default();
    let result = search(&mut gate, &t, "aba", retained(usize::MAX));
    assert_eq!(offsets(&result), [(0, 3), (2, 5), (0, 3)]);
    assert_eq!(result.results().status(), SearchStatus::Complete);
    let soft = search(&mut gate, &t, "baX", retained(usize::MAX));
    assert_eq!(offsets(&soft), [(3, 6)]);
    for query in ["Xaba", "ababa", "aba\nba"] {
        let result = search(&mut gate, &t, query, retained(usize::MAX));
        assert_eq!(result.ranges().len(), usize::from(query == "ababa"));
    }
    let result = search(&mut gate, &t, "aXab", retained(usize::MAX));
    assert!(result.ranges().is_empty());
    assert_eq!(result.results().status(), SearchStatus::Complete);
}

#[test]
fn unicode_occurrences_expand_to_complete_units_not_bytes_scalars_or_cells() {
    let mut t = Terminal::new_headless(4, 5);
    t.put_str("a界e\u{301}👩\u{200d}💻Z\r\n\u{301}x");
    let mut gate = LiteralSearch::default();
    for (query, expected) in [
        ("界e\u{301}👩\u{200d}💻", vec![(1, 4)]),
        ("e", vec![(2, 3)]),
        ("\u{301}", vec![(2, 3), (0, 1)]),
        ("💻", vec![(3, 4)]),
        ("◌", vec![]),
        ("a 界", vec![]),
    ] {
        let result = search(&mut gate, &t, query, retained(usize::MAX));
        assert_eq!(offsets(&result), expected, "query={query:?}");
        for i in 0..result.ranges().len() {
            let target = gate.target(&t, &result, i).unwrap();
            assert_eq!(target.action, TargetAction::Navigate);
            assert!(t.validate_action_target(&target));
        }
    }
    // Repeated hits inside one unit yield one whole-unit decoration, not duplicates.
    t.put_str("\r\na\u{301}\u{301}\u{301}");
    let result = search(&mut gate, &t, "\u{301}", retained(usize::MAX));
    assert_eq!(result.ranges().len(), 3);
}

#[test]
fn meaningful_blanks_are_searchable_but_capacity_and_wide_edge_padding_are_not() {
    let mut t = Terminal::new_headless(4, 4);
    t.put_str("abc界\r\nx  y\r\n\x1b[4m  \x1b[0m");
    let mut gate = LiteralSearch::default();
    let result = search(&mut gate, &t, "c界", retained(usize::MAX));
    assert_eq!(offsets(&result), [(2, 4)]);
    assert!(
        search(&mut gate, &t, "c 界", retained(usize::MAX))
            .ranges()
            .is_empty()
    );
    let spaces = search(&mut gate, &t, "  ", retained(usize::MAX));
    assert_eq!(spaces.ranges().len(), 2);
    let mut blank = Terminal::new_headless(5, 80);
    blank.put_str("x");
    assert!(
        search(&mut gate, &blank, " ", retained(usize::MAX))
            .ranges()
            .is_empty()
    );
}

#[test]
fn long_lines_cross_many_batches_and_keep_retained_suffix_absolute_offsets() {
    let mut t = Terminal::new_headless_with_history(
        2,
        3,
        crate::TerminalAppearance::default(),
        harbor_config::HistoryLimits {
            max_rows: 1000,
            ..Default::default()
        },
    );
    t.put_str(&"abc".repeat(1002));
    t.put_str("mnopqr");
    let mut gate = LiteralSearch::default();
    let result = search(&mut gate, &t, "cabcabca", retained(8));
    assert_eq!(result.ranges()[0].start().offset.0, 8);
    assert_eq!(result.ranges()[0].end().offset.0, 16);
    assert!(result.ranges().iter().all(|r| r.start().offset.0 >= 6));
    assert!(
        search(&mut gate, &t, "qrabc", retained(8))
            .ranges()
            .is_empty()
    );
    // A query as large as the proof limit can span 64 two-column physical rows.
    let mut long = Terminal::new_headless(2, 2);
    long.put_str(&format!("{}b", "a".repeat(127)));
    let max_query = format!("{}b", "a".repeat(127));
    let result = search(&mut gate, &long, &max_query, retained(usize::MAX));
    assert_eq!(offsets(&result), [(0, 128)]);
}

#[test]
fn late_completions_cannot_replace_new_queries_or_revive_cancelled_targets() {
    let mut t = Terminal::new_headless(3, 12);
    t.put_str("old new");
    let mut gate = LiteralSearch::default();
    let mut old = gate.start(&t, "old", retained(3), tiny()).unwrap();
    finish(&mut old, &t);
    let delayed = old.into_results();
    let current = search(&mut gate, &t, "new", retained(3));
    assert_eq!(
        gate.accept(&t, delayed).unwrap_err(),
        Rejection::ObsoleteQuery
    );
    assert!(gate.target(&t, &current, 0).is_some());
    gate.cancel();
    assert!(gate.target(&t, &current, 0).is_none());
    assert_eq!(
        gate.accept(&t, current.into_results()).unwrap_err(),
        Rejection::ObsoleteQuery
    );

    let mut delayed = gate.start(&t, "old", retained(3), tiny()).unwrap();
    delayed.step(&t);
    let _new = gate.start(&t, "new", retained(3), tiny()).unwrap();
    assert_eq!(delayed.step(&t), SearchStatus::Cancelled);
    assert_eq!(delayed.last_work(), SearchWork::default());
    assert!(delayed.scan.is_none());
    assert_eq!(delayed.results.ranges.capacity(), 0);
    assert!(gate.accept(&t, delayed.into_results()).is_err());
}

#[test]
fn dropped_gate_and_session_close_replacement_invalidate_jobs_without_an_eof_rule() {
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("same");
    let mut gate = LiteralSearch::default();
    let mut job = gate.start(&t, "same", retained(2), tiny()).unwrap();
    drop(gate); // closing the logical host owner cancels pending work
    assert_eq!(job.step(&t), SearchStatus::Cancelled);
    let mut gate = LiteralSearch::default();
    let current = search(&mut gate, &t, "same", retained(2));
    let mut replacement = Terminal::new_headless(2, 8);
    replacement.put_str("same");
    assert!(gate.target(&replacement, &current, 0).is_none());
    assert_eq!(
        gate.accept(&replacement, current.into_results())
            .unwrap_err(),
        Rejection::WrongContent
    );
    let mut pending = gate.start(&t, "same", retained(2), tiny()).unwrap();
    assert_eq!(pending.step(&replacement), SearchStatus::Stale);
    // Headless retained content stays eligible without a live child/PTY.
    let result = search(&mut gate, &t, "same", retained(2));
    assert!(gate.target(&t, &result, 0).is_some());
    gate.cancel(); // host explicitly closes the session
    assert!(gate.target(&t, &result, 0).is_none());
}

#[test]
fn publication_and_activation_revalidate_text_even_while_logical_anchors_survive() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("match\r\nother");
    let mut gate = LiteralSearch::default();
    let result = search(&mut gate, &t, "match", retained(3));
    let position = result.ranges()[0].start();
    t.put_str("\x1b[1;1Hpatch");
    let page = t.read_content(retained(1), ReadBudget::default()).unwrap();
    assert_eq!(page.fragments[0].range(0..5).unwrap().start(), position);
    assert!(gate.target(&t, &result, 0).is_none());
    assert_eq!(
        gate.accept(&t, result.into_results()).unwrap_err(),
        Rejection::StaleText
    );

    let mut job = gate.start(&t, "patch", retained(3), tiny()).unwrap();
    finish(&mut job, &t);
    t.put_str("\x1b[1;1Hmatch"); // completion delayed until after overwrite
    assert_eq!(
        gate.accept(&t, job.into_results()).unwrap_err(),
        Rejection::StaleText
    );
}

#[test]
fn erase_structural_edits_and_ring_reuse_never_leave_actionable_old_text() {
    for edit in [
        "\x1b[1;1H\x1b[K",
        "\x1b[1;1H\x1b[@",
        "\x1b[1;1H\x1b[P",
        "\x1b[1;1H\x1b[L",
        "\x1b[1;1H\x1b[M",
        "\x1b[2J",
        "\x1b[1S",
        "\x1b[1T",
    ] {
        let mut t = Terminal::new_headless(3, 8);
        t.put_str("match\r\nother\r\nlast");
        let mut gate = LiteralSearch::default();
        let result = search(&mut gate, &t, "match", retained(3));
        t.put_str(edit);
        assert!(gate.target(&t, &result, 0).is_none(), "edit={edit:?}");
        assert_eq!(
            gate.accept(&t, result.into_results()).unwrap_err(),
            Rejection::StaleText
        );
    }
    let mut t = Terminal::new_headless_with_history(
        2,
        3,
        crate::TerminalAppearance::default(),
        harbor_config::HistoryLimits {
            max_rows: 1000,
            ..Default::default()
        },
    );
    t.put_str(&"abc".repeat(1002));
    let mut gate = LiteralSearch::default();
    let result = search(&mut gate, &t, "abc", retained(5));
    t.put_str("mnopqr");
    assert!(gate.target(&t, &result, 0).is_none()); // evicted
    assert!(gate.target(&t, &result, 2).is_none()); // newly truncated head
    assert!(gate.target(&t, &result, 3).is_some()); // untouched suffix
    t.put_str(&"new\r\n".repeat(1004));
    assert!(gate.target(&t, &result, 3).is_none()); // reused ring slot
}

#[test]
fn incompatible_portions_or_changed_suffix_stop_instead_of_silently_joining() {
    for source in ["abcdefgh", "abcd\r\nefgh"] {
        let mut t = Terminal::new_headless(4, 4);
        t.put_str(source);
        let mut gate = LiteralSearch::default();
        let mut job = gate
            .start(&t, "bcde", retained(2), ReadBudget { units: 4, ..tiny() })
            .unwrap();
        assert_eq!(job.step(&t), SearchStatus::Running);
        t.put_str("\x1b[1;1HX"); // retained suffix or completed-row checkpoint changes
        assert_eq!(job.step(&t), SearchStatus::Stale);
        assert_eq!(job.last_work().read_calls, 0);
        assert!(job.results.ranges.is_empty());
        assert_eq!(
            gate.accept(&t, job.into_results()).unwrap_err(),
            Rejection::StaleWork
        );
    }
}

#[test]
fn repeated_output_rebases_pages_and_suffix_proofs_with_useful_fixed_scope_progress() {
    let mut t = Terminal::new_headless(3, 5);
    for _ in 0..40 {
        t.put_str("abcdefghi\r\n");
    }
    let mut gate = LiteralSearch::default();
    let mut job = gate.start(&t, "defgh", retained(60), tiny()).unwrap();
    let mut steps = 0;
    let mut total_scanned = 0;
    let mut total_comparisons = 0;
    loop {
        let status = job.step(&t);
        assert_work(&job);
        steps += 1;
        total_scanned += job.last_work().scanned_bytes;
        total_comparisons += job.last_work().comparisons;
        if status != SearchStatus::Running {
            break;
        }
        t.put_str("out\r\n");
    }
    assert!(steps > 100);
    assert!(steps < 200, "fixed end, not endless growing scope/restarts");
    assert_eq!(total_scanned, 30 * 9);
    assert!(total_comparisons <= 2 * total_scanned);
    assert_eq!(
        job.status(),
        SearchStatus::Partial(PartialReason::ContentChanged)
    );
    let results = gate.accept(&t, job.into_results()).unwrap();
    assert_eq!(results.ranges().len(), 30);
    for i in 0..results.ranges().len() {
        assert_eq!(results.ranges()[i].version(), t.content_version());
        assert!(gate.target(&t, &results, i).is_some());
    }
}

#[test]
fn complete_negative_coverage_becomes_partial_after_output_and_running_is_never_complete() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str("old\r\nother");
    let mut gate = LiteralSearch::default();
    let mut job = gate.start(&t, "new", retained(3), tiny()).unwrap();
    assert_eq!(job.step(&t), SearchStatus::Running);
    let partial = gate.accept(&t, job.into_results()).unwrap();
    assert_eq!(partial.results().status(), SearchStatus::Running);
    assert!(partial.ranges().is_empty());
    let mut job = gate.start(&t, "new", retained(3), tiny()).unwrap();
    finish(&mut job, &t);
    assert_eq!(job.status(), SearchStatus::Complete);
    t.put_str("\r\nnew");
    let result = gate.accept(&t, job.into_results()).unwrap();
    assert!(result.ranges().is_empty());
    assert_eq!(
        result.results().status(),
        SearchStatus::Partial(PartialReason::ContentChanged)
    );
    let fresh = search(&mut gate, &t, "new", retained(usize::MAX));
    assert_eq!(fresh.ranges().len(), 1);
}

#[test]
fn resize_height_reflow_alt_saved_primary_and_reset_obey_boundary_generations() {
    let mut t = Terminal::new_headless(4, 8);
    t.put_str("match");
    let mut gate = LiteralSearch::default();
    for (rows, cols) in [(4, 4), (6, 4), (3, 10)] {
        let old = search(&mut gate, &t, "match", retained(usize::MAX));
        t.resize(rows, cols);
        assert!(gate.target(&t, &old, 0).is_none());
        assert_eq!(
            gate.accept(&t, old.into_results()).unwrap_err(),
            Rejection::WrongContent
        );
        let fresh = search(&mut gate, &t, "match", retained(usize::MAX));
        assert_eq!(fresh.ranges().len(), 1);
        assert!(gate.target(&t, &fresh, 0).is_some());
        let mut pending = gate
            .start(&t, "match", retained(usize::MAX), tiny())
            .unwrap();
        pending.step(&t);
        t.resize(rows + 1, cols); // independently exercise delayed work
        assert_eq!(pending.step(&t), SearchStatus::Stale);
    }
    let primary = search(&mut gate, &t, "match", retained(usize::MAX));
    t.put_str("\x1b[?47hmatch");
    assert!(gate.target(&t, &primary, 0).is_none());
    t.put_str("\x1b[?47l");
    assert!(gate.target(&t, &primary, 0).is_some());
    t.put_str("\x1b[?47h");
    let alt = search(&mut gate, &t, "match", retained(usize::MAX));
    assert_eq!(
        alt.results().provenance().content.buffer,
        BufferKind::Alternate
    );
    t.resize(4, 7); // saved primary also changes generation
    assert!(gate.target(&t, &alt, 0).is_none());
    t.put_str("\x1b[?47l");
    let restored = search(&mut gate, &t, "match", retained(usize::MAX));
    assert_ne!(
        restored.results().provenance().content.generation,
        primary.results().provenance().content.generation
    );
    t.put_str("\x1bcmatch");
    assert!(gate.target(&t, &restored, 0).is_none());
    let fresh = search(&mut gate, &t, "match", retained(usize::MAX));
    assert_eq!(fresh.ranges().len(), 1);
    t.put_str("\x1b[?1049hmatch");
    let replaced = search(&mut gate, &t, "match", retained(usize::MAX));
    t.put_str("\x1b[?1049l\x1b[?1049hmatch");
    assert!(gate.target(&t, &replaced, 0).is_none());
}

#[test]
fn clipped_resize_tails_are_not_resurrected_by_search() {
    use crate::primary_reflow::ReflowViewport;
    for (policy, spaces) in [
        (ReflowViewport::PullHistory, 8),
        (ReflowViewport::PreserveLiveTop, 1),
    ] {
        let mut t = Terminal::new_headless(4, 12);
        t.put_str("text\x1b[4;53m        \x1b[0m\r\nend");
        let prepared = t.screen.prepare_resize_with_viewport(4, 5, policy).unwrap();
        t.screen.commit_resize(prepared);
        let mut gate = LiteralSearch::default();
        let query = format!("text{}", " ".repeat(spaces));
        assert_eq!(
            search(&mut gate, &t, &query, retained(usize::MAX))
                .ranges()
                .len(),
            1
        );
        let lost = format!("text{}", " ".repeat(spaces + 1));
        assert!(
            search(&mut gate, &t, &lost, retained(usize::MAX))
                .ranges()
                .is_empty()
        );
        let prepared = t
            .screen
            .prepare_resize_with_viewport(4, 12, policy)
            .unwrap();
        t.screen.commit_resize(prepared);
        assert!(
            search(&mut gate, &t, &lost, retained(usize::MAX))
                .ranges()
                .is_empty()
        );
    }
    let mut t = Terminal::new_headless(3, 12);
    t.put_str("text      \r\nend");
    t.resize(3, 5);
    t.resize(3, 12);
    let mut gate = LiteralSearch::default();
    assert!(
        search(&mut gate, &t, "text ", retained(usize::MAX))
            .ranges()
            .is_empty()
    );
}

#[test]
fn query_limits_read_errors_empty_scope_and_result_caps_have_explicit_coverage() {
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("a\u{301}");
    let mut gate = LiteralSearch::default();
    let old = search(&mut gate, &t, "a", retained(2));
    for (query, budget, error) in [
        ("".to_owned(), tiny(), QueryError::Empty),
        ("界".repeat(43), tiny(), QueryError::TooLong),
        (
            "x".to_owned(),
            ReadBudget {
                units: MAX_READ_UNITS + 1,
                ..tiny()
            },
            QueryError::InvalidBudget,
        ),
        (
            "x".to_owned(),
            ReadBudget { rows: 0, ..tiny() },
            QueryError::InvalidBudget,
        ),
    ] {
        assert!(matches!(gate.start(&t, &query, retained(2), budget), Err(e) if e == error));
        assert!(gate.target(&t, &old, 0).is_some());
    }
    let mut job = gate
        .start(
            &t,
            "a",
            retained(2),
            ReadBudget {
                payload_bytes: 2,
                ..tiny()
            },
        )
        .unwrap();
    assert_eq!(
        job.step(&t),
        SearchStatus::Partial(PartialReason::Read(ReadError::UnitTooLarge {
            required_payload_bytes: 3
        }))
    );
    let result = gate.accept(&t, job.into_results()).unwrap();
    assert!(result.ranges().is_empty());
    let mut job = gate
        .start(
            &t,
            "x",
            ReadScope::Retained {
                start_row: 99,
                row_count: 1,
            },
            tiny(),
        )
        .unwrap();
    assert_eq!(
        job.step(&t),
        SearchStatus::Partial(PartialReason::Read(ReadError::InvalidScope))
    );
    let result = search(&mut gate, &t, "x", retained(0));
    assert_eq!(result.results().status(), SearchStatus::Complete);
    assert_eq!(result.results().selected_rows(), Some(0));
    let result = search(&mut gate, &t, "x", retained(usize::MAX));
    assert!(result.results().scope_clipped());
    t.put_str(&"a".repeat(1200));
    let mut job = gate
        .start(&t, "a", retained(usize::MAX), ReadBudget::default())
        .unwrap();
    finish(&mut job, &t);
    assert_eq!(
        job.status(),
        SearchStatus::Partial(PartialReason::ResultLimit)
    );
    assert!(job.scan.is_none());
    assert_eq!(job.results.ranges.len(), MAX_SEARCH_RESULTS);
    let result = gate.accept(&t, job.into_results()).unwrap();
    assert_eq!(result.ranges().len(), MAX_SEARCH_RESULTS);
}

#[test]
fn subset_read_and_matching_leave_source_damage_events_and_parser_state_untouched() {
    let mut t = Terminal::new_headless(3, 8);
    t.put_str(&"history\r\n".repeat(500));
    t.put_str("\x1b[3;1H\x1b]133;A\x1b\\match\x1b[31"); // pending parser CSI
    // Malformed unrelated row proves the consumer does not decode all history.
    t.screen.cell_mut(0, 0).wide_continuation = true;
    let before = t.snapshot();
    let version = t.content_version();
    let now = std::time::Instant::now();
    let update = t.read_update(now);
    let mut gate = LiteralSearch::default();
    let result = search(
        &mut gate,
        &t,
        "match",
        ReadScope::Visible {
            start_row: 2,
            row_count: 1,
        },
    );
    assert_eq!(result.ranges().len(), 1);
    assert!(gate.target(&t, &result, 0).is_some());
    assert_eq!(before, t.snapshot());
    assert_eq!(version, t.content_version());
    let after = t.read_update(now);
    assert_eq!(update.snapshot, after.snapshot);
    assert_eq!(update.damage, after.damage);
    assert!(
        t.drain_output_events()
            .iter()
            .any(|e| matches!(e, crate::TerminalOutputEvent::ShellIntegration(_)))
    );
    t.put_str("mZ"); // parser still finishes the pending SGR
    assert_eq!(t.screen.cell(2, 5).ch, 'Z');
    assert_eq!(t.screen.cell(2, 5).fg, crate::Color::Named(1));
}

#[test]
fn cancellation_after_read_and_during_scan_has_finite_work_and_releases_resources() {
    // Cancellation tokens are thread-safe, but deterministic checks do not depend
    // on racing a timer. Exercise the same token observed by every scan check.
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("aaaa");
    let mut gate = LiteralSearch::default();
    let mut job = gate.start(&t, "a", retained(2), tiny()).unwrap();
    assert_eq!(job.step(&t), SearchStatus::Running);
    assert_eq!(job.results.ranges.len(), 2);
    gate.cancel();
    let result = job.into_results();
    assert_eq!(result.status(), SearchStatus::Cancelled);
    assert!(result.ranges().is_empty());
    assert!(gate.accept(&t, result).is_err());
    // Cross-thread ownership needs no Terminal/Screen access in the worker token.
    let mut job = gate.start(&t, "a", retained(2), tiny()).unwrap();
    let token = job.cancelled.clone();
    std::thread::spawn(move || token.store(true, Ordering::Release))
        .join()
        .unwrap();
    assert_eq!(job.step(&t), SearchStatus::Cancelled);
    assert_eq!(job.last_work().read_calls, 0);
    assert!(job.scan.is_none());
    // Inject cancellation at the post-read observation and inside one very
    // large complete text unit. The public step uses the identical atomic check.
    let mut large = Terminal::new_headless(2, 8);
    large.put_str("a");
    // Defensive consumer fixture deliberately bypasses bounded PTY admission.
    large.screen.cell_mut(0, 0).suffix = "\u{301}".repeat(2000);
    for (cancel_on_check, expected_scan) in [(2, 0), (5, CANCEL_SCAN_BYTES)] {
        let mut job = gate
            .start(&large, "z", retained(2), ReadBudget::default())
            .unwrap();
        let token = job.cancelled.clone();
        let mut checks = 0;
        let status = job.step_inner(&large, || {
            checks += 1;
            if checks == cancel_on_check {
                token.store(true, Ordering::Release);
            }
            token.load(Ordering::Acquire)
        });
        assert_eq!(status, SearchStatus::Cancelled);
        assert_eq!(job.last_work().read_calls, 1);
        assert_eq!(job.last_work().copied_payload_bytes, 4001);
        assert_eq!(job.last_work().scanned_bytes, expected_scan);
        assert!(job.scan.is_none());
        assert_eq!(job.results.ranges.capacity(), 0);
    }
}

#[test]
fn amortized_scan_work_stays_linear_on_repeated_prefixes_and_large_units() {
    let mut t = Terminal::new_headless(3, 80);
    t.put_str(&"a".repeat(4000));
    let mut gate = LiteralSearch::default();
    let mut job = gate
        .start(&t, "aaaaaaaab", retained(usize::MAX), tiny())
        .unwrap();
    let (mut bytes, mut comparisons) = (0, 0);
    loop {
        let status = job.step(&t);
        assert_work(&job);
        bytes += job.last_work().scanned_bytes;
        comparisons += job.last_work().comparisons;
        if status != SearchStatus::Running {
            break;
        }
    }
    assert_eq!(bytes, 4000);
    assert!(comparisons <= 2 * bytes);
    assert!(job.results.ranges.is_empty());
    // A complete unit may exceed the query bound, but the read/scan bound still
    // applies, and matches map to one unit without retaining its source string.
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("a");
    // Keep oversized stored-content defense coverage, independent of admission.
    t.screen.cell_mut(0, 0).suffix = "\u{301}".repeat(2000);
    let result = search(&mut gate, &t, "a", retained(2));
    // tiny() cannot copy that indivisible unit: explicit partial, not no-match.
    assert!(matches!(
        result.results().status(),
        SearchStatus::Partial(PartialReason::Read(ReadError::UnitTooLarge { .. }))
    ));
    let mut job = gate
        .start(&t, "\u{301}", retained(2), ReadBudget::default())
        .unwrap();
    finish(&mut job, &t);
    assert_eq!(job.last_work().scanned_bytes, 4001);
    let result = gate.accept(&t, job.into_results()).unwrap();
    assert_eq!(offsets(&result), [(0, 1)]);
}

#[test]
fn newly_read_revision_is_explicit_and_failed_reads_remain_partial() {
    let mut t = Terminal::new_headless(4, 4);
    t.put_str("abcdefgh");
    let mut gate = LiteralSearch::default();
    let mut job = gate
        .start(&t, "bcde", retained(2), ReadBudget { units: 4, ..tiny() })
        .unwrap();
    assert_eq!(job.step(&t), SearchStatus::Running);
    // Only the not-yet-read row changes. Rebase is allowed by T0001, but the
    // captured prefix must not be matched against the old suffix's text.
    t.put_str("\x1b[2;1HX");
    assert_eq!(
        job.step(&t),
        SearchStatus::Partial(PartialReason::ContentChanged)
    );
    let result = gate.accept(&t, job.into_results()).unwrap();
    assert!(result.ranges().is_empty());
    t.screen.cell_mut(0, 0).wide_continuation = true;
    let mut job = gate.start(&t, "x", retained(2), tiny()).unwrap();
    assert_eq!(
        job.step(&t),
        SearchStatus::Partial(PartialReason::Read(ReadError::InvalidContent))
    );
    assert!(job.scan.is_none());
    // Read counters describe successful pages; failed calls still obey the
    // submitted budget and cannot claim complete negative coverage.
    assert_eq!(job.last_work().read_calls, 1);
    assert_eq!(job.last_work().scanned_bytes, 0);
}

proptest::proptest! {
    #[test]
    fn paged_ascii_matches_agree_with_overlapping_literal_oracle(
        lines in proptest::collection::vec("[abc ]{0,35}", 1..8),
        query in "[abc ]{1,8}", cols in 2usize..10,
        units in 1usize..15,
    ) {
        let mut t = Terminal::new_headless(4, cols);
        t.put_str(&lines.join("\r\n"));
        let expected: Vec<(usize,usize)> = lines.iter().flat_map(|line|
            (0..line.len()).filter(|&start| line[start..].starts_with(&query))
                .map(|start| (start,start+query.len())).collect::<Vec<_>>()
        ).collect();
        let mut gate = LiteralSearch::default();
        let mut job = gate.start(&t, &query, retained(usize::MAX), ReadBudget { units, ..tiny() }).unwrap();
        finish(&mut job, &t);
        let result = gate.accept(&t, job.into_results()).unwrap();
        proptest::prop_assert_eq!(offsets(&result), expected);
    }
}
