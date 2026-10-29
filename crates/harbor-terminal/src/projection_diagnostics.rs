//! Opt-in, thread-local unit-test instrumentation. Absent from production builds.
#[path = "projection_diagnostics/perf.rs"]
mod perf;

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

/// Counts projection builds during an operation.
fn count_builds(operation: impl FnOnce()) -> u64 {
    COUNTERS.with(|state| state.set(Some((0, 0))));
    operation();
    COUNTERS.with(|state| state.replace(None).map_or(0, |(calls, _)| calls))
}

#[test]
fn history_retrim_reuses_the_prepared_content_projection() {
    let mut screen = Screen::new(3, 8);
    screen.write_char('界');
    let limits = harbor_config::HistoryLimits {
        max_rows: 16,
        max_bytes: 1024,
    };
    let builds = count_builds(|| {
        let prepared = screen
            .prepare_resize_with_history(
                3,
                8,
                crate::primary_reflow::ReflowViewport::PullHistory,
                limits,
            )
            .unwrap();
        screen.commit_resize(prepared);
    });
    assert_eq!(builds, 1);
    assert_eq!(screen.history_limits(), limits);
    assert_eq!(screen.cell(0, 0).ch, '界');
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
