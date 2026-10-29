//! Manual release-mode measurement of the complete detached preparation.
use super::*;
use std::hint::black_box;
use std::time::Instant;

fn fixture(kind: &str) -> NormalBuf {
    let mut normal = NormalBuf::new_for_test(24, 160, 4096);
    for _ in 0..4000 {
        let mut col = 0;
        while col < 144 {
            let cell = match kind {
                "ascii" => Cell {
                    ch: (b'a' + (col % 26) as u8) as char,
                    ..Cell::default()
                },
                "styled" => {
                    let mut cell = Cell {
                        ch: (b'a' + (col % 26) as u8) as char,
                        ..Cell::default()
                    };
                    cell.attrs.set(crate::model::CellAttrs::BOLD);
                    cell
                }
                _ => Cell {
                    ch: 'e',
                    suffix: "\u{301}".to_owned(),
                    ..Cell::default()
                },
            };
            normal.write_meaningful_cell(23, col, cell);
            col += 1;
        }
        normal.scroll_up_full_screen(1, Cell::default());
    }
    normal
}

#[test]
#[ignore = "manual release-mode primary preparation timing; excludes fixture construction and result destruction"]
fn preparation_performance() {
    for kind in ["ascii", "styled", "combining"] {
        let normal = fixture(kind);
        for cols in [80, 200] {
            for viewport in [ReflowViewport::PullHistory, ReflowViewport::PreserveLiveTop] {
                let prepare = |baseline| {
                    if baseline {
                        reference::prepare_geometry_with_limits(
                            black_box(&normal),
                            24,
                            cols,
                            None,
                            viewport,
                            &[],
                            normal.history_limits(),
                        )
                    } else {
                        PreparedPrimaryResize::prepare_geometry_with_limits(
                            black_box(&normal),
                            24,
                            cols,
                            None,
                            viewport,
                            &[],
                            normal.history_limits(),
                        )
                    }
                    .unwrap()
                };
                // Warm both implementations, then alternate order in the same
                // process; fixture creation/result destruction are not timed.
                black_box(prepare(true));
                black_box(prepare(false));
                let mut samples = [Vec::new(), Vec::new()];
                for iteration in 0..9 {
                    for method in [iteration % 2, 1 - iteration % 2] {
                        let start = Instant::now();
                        let prepared = prepare(method == 0);
                        samples[method].push(start.elapsed().as_micros());
                        black_box(prepared);
                    }
                }
                for times in &mut samples {
                    times.sort_unstable();
                }
                let (expected, before) = allocation_probe::measure(|| prepare(true));
                let (actual, after) = allocation_probe::measure(|| prepare(false));
                assert_eq!(actual, expected);
                println!(
                    "primary_prepare kind={kind} history=4000 source_cols=160 target_cols={cols} viewport={viewport:?} baseline_median_us={} optimized_median_us={} baseline_alloc={before:?} optimized_alloc={after:?}",
                    samples[0][4], samples[1][4]
                );
            }
        }
    }
}
