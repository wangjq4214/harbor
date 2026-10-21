//! Opt-in, thread-local unit-test instrumentation. Absent from production builds.
use crate::{Screen, TerminalParser};
use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;

thread_local! {
    static COUNTERS: Cell<Option<(u64, u128)>> = const { Cell::new(None) };
}

pub(super) struct BuildTimer(Instant);

pub(super) fn build_timer() -> Option<BuildTimer> {
    COUNTERS.with(|state| state.get().map(|_| BuildTimer(Instant::now())))
}

impl Drop for BuildTimer {
    fn drop(&mut self) {
        let elapsed = self.0.elapsed().as_nanos();
        COUNTERS.with(|state| {
            if let Some((count, ns)) = state.get() {
                state.set(Some((count + 1, ns + elapsed)));
            }
        });
    }
}

#[test]
#[ignore = "manual release diagnostic; per-accepted-byte anchor reconciliation"]
fn ingest_reconciliation_cost_by_chunking() {
    // Same parser and screen; only the caller's chunk cadence differs. The engine
    // path runs TerminalIo::feed_parser_bytes, which reconciles anchors per call.
    // ADR 0074: with no selection, saved cursor or review anchor, ingestion is
    // untracked and this reports zero projection calls by design.
    let input: String = (0..2000)
        .map(|i| format!("{i:06} e\u{301} ❤ 👩\u{200d}💻 界 😀 \r\n"))
        .collect();
    for (label, chunks) in [
        (
            "4096_byte",
            input.as_bytes().chunks(4096).collect::<Vec<_>>(),
        ),
        (
            "one_row",
            input.as_bytes().split_inclusive(|b| *b == b'\n').collect(),
        ),
    ] {
        for round in 0..2 {
            let mut terminal = crate::Terminal::new_headless(32, 107);
            COUNTERS.with(|state| state.set(Some((0, 0))));
            let started = Instant::now();
            for chunk in &chunks {
                terminal.process_output(black_box(chunk));
            }
            let total_ns = started.elapsed().as_nanos();
            let (calls, build_ns) = COUNTERS.with(|state| state.replace(None).unwrap());
            println!(
                "case=engine_no_vs16 chunking={label} chunks={} round={round} projection_calls={calls} projection_build_ns={build_ns} total_ns={total_ns}",
                chunks.len()
            );
        }
    }
}

#[test]
#[ignore = "manual release diagnostic; counts/times full retained-content projection"]
fn unicode_promotion_projection_cost() {
    for (name, payload) in [
        ("original", "e\u{301} ❤\u{fe0f} 👩\u{200d}💻 界 😀 "),
        ("no_vs16", "e\u{301} ❤ 👩\u{200d}💻 界 😀 "),
        ("wide_vs16", "e\u{301} 😀\u{fe0f} 👩\u{200d}💻 界 😀 "),
    ] {
        let input: String = (0..2000).map(|i| format!("{i:06} {payload}\r\n")).collect();
        for round in 0..3 {
            let mut screen = Screen::new(32, 107);
            let mut parser = TerminalParser::default();
            // Measures the tracked path (the stage-2 candidate): a registered
            // consumer keeps the per-promotion projection builds.
            screen.set_external_anchor_interest(true);
            COUNTERS.with(|state| state.set(Some((0, 0))));
            let started = Instant::now();
            for chunk in input.as_bytes().chunks(4096) {
                parser.put_bytes(&mut screen, black_box(chunk));
            }
            let total_ns = started.elapsed().as_nanos();
            let (calls, build_ns) = COUNTERS.with(|state| state.replace(None).unwrap());
            // row_text() exposes leading grid scalars, not retained suffix source.
            let retained: String = (0..107)
                .map(|col| screen.cell(30, col).raw_text())
                .collect();
            assert_eq!(
                retained.trim_end(),
                format!("001999 {}", payload.trim_end())
            );
            println!(
                "case={name} round={round} input_bytes={} grid=107x32 rows=2000 projection_calls={calls} projection_build_ns={build_ns} total_ns={total_ns}",
                input.len()
            );
        }
    }
}
/// Counts builds for the duration of one ingest and returns the build count.
fn count_builds(ingest: impl FnOnce()) -> u64 {
    COUNTERS.with(|state| state.set(Some((0, 0))));
    ingest();
    COUNTERS.with(|state| state.replace(None).map_or(0, |(calls, _)| calls))
}

#[test]
fn idle_ingest_does_not_rebuild_the_content_projection() {
    // Every row promotes `❤` to width 2 through `U+FE0F`, which is the measured
    // worst case for the promotion path.
    let input: String = (0..200)
        .map(|i| format!("{i:03} e\u{301} ❤\u{fe0f} 界 😀\r\n"))
        .collect();

    // Nothing consumes anchor mutations here: no selection, no saved cursor
    // anchor and no review anchor.
    let parser_calls = count_builds(|| {
        let mut screen = Screen::new(32, 107);
        let mut parser = TerminalParser::default();
        for chunk in input.as_bytes().chunks(4096) {
            parser.put_bytes(&mut screen, black_box(chunk));
        }
    });
    assert_eq!(
        parser_calls, 0,
        "untracked ingestion must not project the retained buffer"
    );

    let engine_calls = count_builds(|| {
        let mut terminal = crate::Terminal::new_headless(32, 107);
        for chunk in input.as_bytes().chunks(4096) {
            terminal.process_output(black_box(chunk));
        }
    });
    assert_eq!(
        engine_calls, 0,
        "untracked feed reconciliation must not project the retained buffer"
    );

    // Control: registering a consumer re-enables the same work, proving the
    // counter observes this path rather than a cold one.
    let tracked_calls = count_builds(|| {
        let mut screen = Screen::new(32, 107);
        let mut parser = TerminalParser::default();
        screen.set_external_anchor_interest(true);
        for chunk in input.as_bytes().chunks(4096) {
            parser.put_bytes(&mut screen, black_box(chunk));
        }
    });
    assert!(
        tracked_calls > 0,
        "a tracked ingest still projects so anchors stay resolvable"
    );
}
