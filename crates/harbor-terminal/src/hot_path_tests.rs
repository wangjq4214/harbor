//! Allocation measurements use the real connected paths, with setup outside probes.
use super::*;
use crate::primary_reflow::allocation_probe::measure;
use std::hint::black_box;

const ITERATIONS: usize = 128;

#[test]
fn hot_path_allocation_measurements() {
    for (rows, cols) in [(24, 80), (80, 240)] {
        let mut terminal = Terminal::new_headless_with_history(
            rows,
            cols,
            TerminalAppearance::default(),
            harbor_config::HistoryLimits {
                max_rows: 0,
                max_bytes: 0,
            },
        );
        let now = Instant::now();
        let initial = terminal.read_update(now);
        assert!(terminal.acknowledge_update(&initial));
        drop(initial);
        let old = terminal.read_update(now);
        let (_, unchanged) = measure(|| {
            for _ in 0..ITERATIONS {
                black_box(terminal.read_update(now));
            }
        });
        let (_, local) = measure(|| {
            for i in 0..ITERATIONS {
                terminal.process_output(if i % 2 == 0 {
                    b"\x1b[1;1HA"
                } else {
                    b"\x1b[1;1HB"
                });
                let update = terminal.read_update(now);
                assert_eq!(
                    update.snapshot.cells[0].ch,
                    if i % 2 == 0 { 'A' } else { 'B' }
                );
                assert!(terminal.acknowledge_update(&update));
            }
        });
        assert_eq!(old.snapshot.cells[0].ch, ' ');
        eprintln!(
            "HOT update {rows}x{cols} n={ITERATIONS} unchanged={unchanged:?} local_retained={local:?}"
        );

        let normal = NormalBuf::with_history_limits(
            rows,
            cols,
            harbor_config::HistoryLimits {
                max_rows: 0,
                max_bytes: 0,
            },
        );
        black_box(normal.snapshot_cells());
        let (_, versions) = measure(|| {
            for _ in 0..ITERATIONS {
                black_box(normal.snapshot_cells());
            }
        });
        eprintln!("HOT versions {rows}x{cols} n={ITERATIONS} {versions:?}");
    }
}

#[test]
fn hot_path_damage_owned_allocation_measurement() {
    let mut damage = crate::damage::DamageTracker::new(24, 80);
    damage.clear();
    for row in 0..24 {
        damage.mark_range_dirty(row, 1, 3);
        damage.mark_range_dirty(row, 60, 70);
    }
    let (_, stats) = measure(|| {
        for _ in 0..ITERATIONS {
            black_box(damage.dirty_ranges());
        }
    });
    eprintln!("HOT damage owned 24x80 ranges=48 n={ITERATIONS} {stats:?}");

    let expected = damage.dirty_ranges();
    let mut scratch = Vec::new();
    damage.dirty_ranges_into(&mut scratch);
    let capacity = scratch.capacity();
    let (_, reused) = measure(|| {
        for _ in 0..ITERATIONS {
            damage.dirty_ranges_into(&mut scratch);
            assert_eq!(scratch, expected);
        }
    });
    assert_eq!(reused.calls, 0);
    assert_eq!(scratch.capacity(), capacity);
    assert_eq!(
        damage.dirty_ranges(),
        expected,
        "collection does not consume damage"
    );
    eprintln!("HOT damage reusable 24x80 ranges=48 n={ITERATIONS} {reused:?}");
}

#[test]
fn cached_snapshot_reuses_unique_metadata_and_keeps_old_updates_immutable() {
    let mut terminal = Terminal::new_headless_with_history(
        3,
        8,
        TerminalAppearance::default(),
        harbor_config::HistoryLimits {
            max_rows: 4,
            max_bytes: 8192,
        },
    );
    let now = Instant::now();
    let initial = terminal.read_update(now);
    assert!(terminal.acknowledge_update(&initial));
    drop(initial);
    let old = terminal.read_update(now);
    let old_value = (*old.snapshot).clone();
    terminal.process_output(b"abcdefghijk");
    let current = terminal.read_update(now);
    assert_eq!(*current.snapshot, terminal.snapshot());
    assert!(current.snapshot.wrapped.iter().any(|w| *w));
    assert_eq!(*old.snapshot, old_value);
    assert!(!terminal.acknowledge_update(&old));
    let repeated = terminal.read_update(now);
    assert!(Arc::ptr_eq(&current.snapshot, &repeated.snapshot));
    let identity = Arc::as_ptr(&current.snapshot);
    let wrapped = current.snapshot.wrapped.as_ptr();
    let capacity = current.snapshot.dirty_ranges.capacity();
    drop(repeated);
    drop(current);
    terminal.process_output(b"\x1b[1;1HZ");
    let next = terminal.read_update(now);
    assert_eq!(
        Arc::as_ptr(&next.snapshot),
        identity,
        "unique storage is refilled"
    );
    assert_eq!(next.snapshot.wrapped.as_ptr(), wrapped);
    assert!(next.snapshot.dirty_ranges.capacity() >= capacity);
    assert_eq!(*next.snapshot, terminal.snapshot());
    assert_eq!(*old.snapshot, old_value);
    assert!(terminal.acknowledge_update(&next));
    let clean = terminal.read_update(now);
    assert!(
        clean.snapshot.dirty_ranges.is_empty(),
        "ACK invalidates cached damage"
    );
    assert_eq!(*old.snapshot, old_value);
    let weak = Arc::downgrade(&old.snapshot);
    drop(old);
    assert!(
        weak.upgrade().is_none(),
        "the cache does not retain historical projections"
    );
}

#[test]
fn reusable_damage_matches_cell_oracle_through_churn_and_clear() {
    let (rows, cols) = (5, 17);
    let mut tracker = crate::damage::DamageTracker::new(rows, cols);
    let mut scratch = Vec::new();
    for turn in 0..128 {
        tracker.clear();
        let mut oracle = [[false; 17]; 5];
        for mark in 0..20 {
            let row = (turn + mark) % rows;
            let start = (turn * 3 + mark * 7) % cols;
            let end = (start + mark % 4).min(cols);
            tracker.mark_range_dirty(row, start, end);
            oracle[row][start..end].fill(true);
        }
        tracker.dirty_ranges_into(&mut scratch);
        let mut observed = [[false; 17]; 5];
        let mut previous = (0, 0);
        for r in &scratch {
            assert!((r.row, r.start_col) >= previous);
            assert!(r.start_col < r.end_col);
            observed[r.row][r.start_col..r.end_col].fill(true);
            previous = (r.row, r.end_col);
        }
        assert_eq!(observed, oracle);
    }
    tracker.clear();
    tracker.dirty_ranges_into(&mut scratch);
    assert!(scratch.is_empty());
}

#[test]
fn hot_path_initial_projection_and_damage_growth_measurements() {
    let terminal = Terminal::new_headless_with_history(
        24,
        80,
        TerminalAppearance::default(),
        harbor_config::HistoryLimits {
            max_rows: 0,
            max_bytes: 0,
        },
    );
    let (initial, cold) = measure(|| terminal.read_update(Instant::now()));
    assert_eq!(initial.snapshot.cells.len(), 24 * 80);
    assert!(
        cold.calls > 0,
        "new independently owned projection may allocate"
    );
    eprintln!("HOT snapshot initial 24x80 {cold:?}");

    let mut tracker = crate::damage::DamageTracker::new(24, 80);
    tracker.clear();
    tracker.mark_range_dirty(0, 1, 3);
    let mut ranges = Vec::new();
    let (_, cold) = measure(|| tracker.dirty_ranges_into(&mut ranges));
    for row in 0..24 {
        tracker.mark_range_dirty(row, 1, 3);
        tracker.mark_range_dirty(row, 60, 70);
    }
    let (_, growth) = measure(|| tracker.dirty_ranges_into(&mut ranges));
    assert_eq!(ranges.len(), 48);
    assert!(growth.calls > 0, "new capacity may allocate");
    let (_, warm) = measure(|| tracker.dirty_ranges_into(&mut ranges));
    assert_eq!(warm.calls, 0);
    eprintln!("HOT damage cold={cold:?} growth_to48={growth:?} warm={warm:?}");
}
