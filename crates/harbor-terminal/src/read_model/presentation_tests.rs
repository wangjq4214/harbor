use super::*;
use crate::{
    Terminal,
    literal_search::{LiteralSearch, SearchStatus},
};
use std::time::Instant;

fn matches(t: &Terminal, query: &str) -> Vec<ContentRange> {
    let mut gate = LiteralSearch::default();
    let mut job = gate
        .start(
            t,
            query,
            ReadScope::Retained {
                start_row: 0,
                row_count: usize::MAX,
            },
            ReadBudget {
                rows: 1,
                units: 1,
                ..ReadBudget::default()
            },
        )
        .unwrap();
    for _ in 0..5000 {
        if job.step(t) != SearchStatus::Running {
            return gate
                .accept(t, job.into_results())
                .unwrap()
                .ranges()
                .to_vec();
        }
    }
    panic!("bounded search failed to finish");
}

#[test]
fn prepared_ranges_project_paged_soft_wraps_not_hard_breaks_and_navigate_history() {
    let mut t = Terminal::new_headless(3, 4);
    t.put_str("abcdefghijkl\r\ncdef\r\nghij\r\nlast\r\nlive");
    let ranges = matches(&t, "cdefghij");
    assert_eq!(ranges.len(), 1);
    assert!(t.project_content_ranges(&ranges).is_empty());
    assert!(t.navigate_content_target(&ActionTarget {
        range: ranges[0].clone(),
        action: TargetAction::Navigate
    }));
    assert_eq!(
        t.project_content_ranges(&ranges),
        [
            CellSpan {
                row: 0,
                start_col: 2,
                end_col: 4
            },
            CellSpan {
                row: 1,
                start_col: 0,
                end_col: 4
            },
            CellSpan {
                row: 2,
                start_col: 0,
                end_col: 2
            },
        ]
    );
    t.resize(3, 6);
    assert!(
        t.project_content_ranges(&ranges).is_empty(),
        "no old physical coordinates after reflow"
    );
    let fresh = matches(&t, "cdefghij");
    assert_eq!(fresh.len(), 1);
    t.navigate_content_target(&ActionTarget {
        range: fresh[0].clone(),
        action: TargetAction::Navigate,
    });
    assert_eq!(
        t.project_content_ranges(&fresh),
        [
            CellSpan {
                row: 0,
                start_col: 2,
                end_col: 6
            },
            CellSpan {
                row: 1,
                start_col: 0,
                end_col: 4
            },
        ]
    );
}

#[test]
fn unicode_units_padding_and_synthetic_cues_share_current_geometry() {
    let mut t = Terminal::new_headless(5, 4);
    t.put_str("abc界e\u{301}👩‍💻\r\n\u{301}");
    let wide = matches(&t, "界");
    assert_eq!(
        t.project_content_ranges(&wide),
        [CellSpan {
            row: 1,
            start_col: 0,
            end_col: 2
        }]
    );
    let combining = matches(&t, "\u{301}");
    assert_eq!(
        t.project_content_ranges(&combining),
        [
            CellSpan {
                row: 1,
                start_col: 2,
                end_col: 3
            },
            CellSpan {
                row: 3,
                start_col: 0,
                end_col: 1
            },
        ]
    );
    let emoji = matches(&t, "💻");
    assert_eq!(
        t.project_content_ranges(&emoji),
        [CellSpan {
            row: 2,
            start_col: 0,
            end_col: 2
        }]
    );
    assert!(matches(&t, "◌").is_empty());
    assert!(
        matches(&t, "c 界").is_empty(),
        "generated edge capacity isn't source"
    );
    let all = [wide, combining, emoji].concat();
    assert_eq!(
        t.project_content_ranges(&all),
        [
            CellSpan {
                row: 1,
                start_col: 0,
                end_col: 3
            },
            CellSpan {
                row: 2,
                start_col: 0,
                end_col: 2
            },
            CellSpan {
                row: 3,
                start_col: 0,
                end_col: 1
            },
        ]
    );
}

#[test]
fn projection_rejects_changed_rows_and_does_not_consume_damage_events_or_transients() {
    let mut t = Terminal::new_headless(3, 12);
    t.put_str("\x1b[4mneedle\x1b[0m\x1b]7;file:///test\x1b\\\x1b]133;A\x1b\\");
    let ranges = matches(&t, "needle");
    let before = t.read_update(Instant::now());
    for _ in 0..3 {
        assert_eq!(t.project_content_ranges(&ranges).len(), 1);
    }
    let after = t.read_update(Instant::now());
    assert_eq!(
        format!("{:?}", before.snapshot.cells),
        format!("{:?}", after.snapshot.cells)
    );
    assert_eq!(
        format!("{:?}", before.damage),
        format!("{:?}", after.damage)
    );
    assert!(
        t.drain_output_events()
            .iter()
            .any(|e| matches!(e, crate::TerminalOutputEvent::ShellIntegration(_)))
    );
    t.put_str("\rX");
    assert!(t.project_content_ranges(&ranges).is_empty());
    assert!(!t.navigate_content_target(&ActionTarget {
        range: ranges[0].clone(),
        action: TargetAction::Navigate
    }));
}
