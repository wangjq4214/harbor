//! Update-boundary regression contracts; full cell walks here are test oracles only.
use crate::{Preedit, Terminal, TerminalAppearance, TerminalEvent, UpdateDamage};
use std::time::Instant;

fn terminal(rows: usize, cols: usize) -> Terminal {
    Terminal::new_headless_with_history(
        rows,
        cols,
        TerminalAppearance::from_palette(harbor_config::Palette::default()),
        harbor_config::HistoryLimits {
            max_rows: 4,
            max_bytes: 16384,
        },
    )
}

fn assert_current(terminal: &Terminal) {
    let update = terminal.read_update(Instant::now());
    let snap = &update.snapshot;
    let screen = terminal.screen();
    assert_eq!((snap.rows, snap.cols), (screen.rows(), screen.cols()));
    for row in 0..snap.rows {
        assert_eq!(snap.wrapped[row], screen.is_wrapped(row));
        for col in 0..snap.cols {
            assert_eq!(
                snap.cell(row, col),
                screen.cell(row, col),
                "cell {row},{col}"
            );
        }
    }
    assert_eq!(
        (snap.cursor_x, snap.cursor_y),
        (screen.cursor_x(), screen.cursor_y())
    );
    assert_eq!(snap.input_modes, screen.input_modes());
    assert_eq!(snap.is_alt, screen.is_alt());
    assert_eq!(snap.view_offset, screen.view_offset());
    assert_eq!(snap.history_start, screen.history_start());
    assert_eq!(snap.scroll_count, screen.scroll_count());
    assert_eq!(snap.dirty_ranges, screen.dirty_ranges());
}

#[test]
fn cell_and_noncell_a_b_a_do_not_revive_old_acknowledgements() {
    let mutations: &[fn(&mut Terminal)] = &[
        |t| t.put_str("B\rA\r"),
        |t| t.put_str("\x1b[2G\x1b[1G"),
        |t| t.put_str("\x1b[?25l\x1b[?25h"),
        |t| t.put_str("\x1b[?1h\x1b[?1l"),
        |t| t.put_str("\x1b[?2026h\x1b[?2026l"),
        |t| t.put_str("\x1b]11;#112233\x07\x1b]111\x07"),
        |t| t.put_str("\x1b[?47h\x1b[?47l"),
        |t| {
            t.set_backdrop_available(true);
            t.set_backdrop_available(false);
        },
        |t| {
            t.handle_event(TerminalEvent::Preedit(Preedit::new("ime", None)))
                .unwrap();
            t.clear_preedit();
        },
    ];
    for (case, mutate) in mutations.iter().enumerate() {
        let mut t = terminal(3, 12);
        t.put_str("A\r");
        assert!(t.acknowledge_update(&t.read_update(Instant::now())));
        let old = t.read_update(Instant::now());
        mutate(&mut t);
        assert!(!t.acknowledge_update(&old), "case {case}");
        assert_eq!(
            t.read_update(Instant::now()).snapshot.cells,
            old.snapshot.cells,
            "case {case}"
        );
        assert_current(&t);
        assert!(
            t.acknowledge_update(&t.read_update(Instant::now())),
            "case {case}"
        );
    }
}

#[test]
fn noncell_updates_reuse_all_cell_payloads() {
    let mut t = terminal(24, 80);
    t.put_str("source");
    let old = t.read_update(Instant::now());
    t.put_str("\x1b[2;2H\x1b[?25l\x1b]11;#123456\x07");
    t.set_backdrop_available(true);
    t.handle_event(TerminalEvent::Preedit(Preedit::new("ime", None)))
        .unwrap();
    let next = t.read_update(Instant::now());
    assert_eq!(
        old.snapshot.cells.shared_rows_with(&next.snapshot.cells),
        24
    );
    assert!(!t.acknowledge_update(&old));
    assert!(t.acknowledge_update(&next));
}

#[test]
fn projected_cells_match_authority_across_structural_changes_and_replay() {
    let mut t = terminal(4, 12);
    let streams = [
        "a界e\u{301}👩‍💻",
        "\r\x1b[@",
        "\x1b[P",
        "\x1b[X",
        "\r\nsecond\r\nthird\r\nfourth\r\nfifth",
        "\x1b[2;1H\x1b[L",
        "\x1b[M",
        "\x1b[S",
        "\x1b[T",
        "\x1b[?47hALT",
        "\x1b[?47l",
        "\x1b[?47h",
        "\x1b[?47l",
        "\x1b[2J",
        "\x1bc",
        "after reset",
    ];
    let initial = t.read_update(Instant::now());
    assert!(t.acknowledge_update(&initial));
    for stream in streams {
        let retained = t.read_update(Instant::now());
        let original_cells = retained.snapshot.cells.iter().cloned().collect::<Vec<_>>();
        t.put_str(stream);
        assert_current(&t);
        assert_eq!(
            retained.snapshot.cells.iter().cloned().collect::<Vec<_>>(),
            original_cells
        );
        assert!(!t.acknowledge_update(&retained));
        let pending = t.read_update(Instant::now());
        assert!(t.acknowledge_update(&pending));
    }
    t.put_str("\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6");
    t.scroll_viewport_to_top();
    assert_current(&t);
    t.scroll_viewport_to_bottom();
    assert_current(&t);
    let old = t.read_update(Instant::now());
    t.resize(5, 8);
    assert!(!t.acknowledge_update(&old));
    assert_current(&t);
    let skipped = t.read_update(Instant::now());
    t.put_str("more");
    t.invalidate_update();
    assert!(!t.acknowledge_update(&skipped));
    assert_eq!(
        t.read_update(Instant::now()).damage,
        UpdateDamage::FullUpload
    );
    assert!(t.acknowledge_update(&t.read_update(Instant::now())));
    assert_eq!(
        t.read_update(Instant::now()).damage,
        UpdateDamage::Ranges(vec![])
    );
}

#[test]
fn overflow_bookkeeping_refreshes_projection_without_semantic_invalidation_or_damage() {
    let mut t = terminal(3, 8);
    t.put_str(&format!("♥{}", "\u{301}".repeat(510)));
    let old = t.read_update(Instant::now());
    let version = t.content_version();
    assert!(t.acknowledge_update(&old));
    t.put_str("\u{fe0f}\u{200d}💻");
    assert_eq!(t.content_version(), version);
    assert!(t.screen().dirty_ranges().is_empty());
    assert_current(&t);
    let next = t.read_update(Instant::now());
    assert_eq!(old.snapshot.cells.shared_rows_with(&next.snapshot.cells), 2);
    assert_ne!(
        old.snapshot.cells[0].text_overflow,
        next.snapshot.cells[0].text_overflow
    );
}

#[test]
fn update_proof_rejects_another_engine_and_tampered_damage() {
    let mut first = terminal(2, 8);
    let mut second = terminal(2, 8);
    let update = first.read_update(Instant::now());
    assert!(!second.acknowledge_update(&update));
    let mut altered = update.clone();
    altered.damage = UpdateDamage::Ranges(vec![]);
    assert!(!first.acknowledge_update(&altered));
    assert!(first.acknowledge_update(&update));
    assert!(!first.acknowledge_update(&update));
}

#[test]
fn selection_a_b_a_has_independent_mutation_provenance() {
    use crate::{
        RenderViewport, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    };
    let mut t = terminal(3, 12);
    t.put_str("selectable");
    t.pointer
        .set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
    let now = Instant::now();
    let old = t.read_update(now);
    let screen_revision = t.screen.mutation_revisions();
    for (phase, x) in [
        (TerminalPointerPhase::Down, 1.0),
        (TerminalPointerPhase::Move, 41.0),
        (TerminalPointerPhase::Up, 41.0),
    ] {
        t.pointer.handle_pointer(
            &mut t.screen,
            TerminalPointerEvent::new((x, 1.0), phase, TerminalPointerButton::Left, 1),
            now,
        );
    }
    assert!(t.pointer.bounds().is_some());
    t.pointer.clear_selection_outcome();
    assert_eq!(t.pointer.bounds(), old.selection);
    assert_eq!(t.screen.mutation_revisions(), screen_revision);
    assert!(!t.acknowledge_update(&old));
    assert!(t.acknowledge_update(&t.read_update(now)));
}
