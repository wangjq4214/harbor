//! Test-only observations for Spec 0031/T0001, not shipping budget enforcement.
//! cargo test --release -p harbor-terminal calibration -- --ignored --nocapture --test-threads=1
use super::{CellState, NormalBuf, RowMetadata};
use crate::{Cell, Terminal};
use std::hint::black_box;
use std::mem::size_of;
use std::time::Instant;

/// Owned backing-array capacities only. Excludes inline owner, damage storage,
/// suffix heaps, allocator overhead, native/font/GPU resources and temporaries.
fn backing_bytes(buf: &NormalBuf) -> [usize; 4] {
    [
        buf.cells.capacity() * size_of::<Cell>(),
        buf.cell_state.capacity() * size_of::<CellState>(),
        buf.row_metadata.capacity() * size_of::<RowMetadata>(),
        buf.row_versions.capacity() * size_of::<u64>(),
    ]
}

/// Scalar-aligned *model* for a known single unit. Does not implement unit
/// recognition, stream admission, recovery, or the future production policy.
fn prefix_at_byte_limit(unit: &str, limit: usize) -> &str {
    let mut end = limit.min(unit.len());
    while !unit.is_char_boundary(end) {
        end -= 1;
    }
    &unit[..end]
}

#[test]
fn calibration_accounting_controls() {
    let buf = NormalBuf::with_max_scrollback(2, 3, 4);
    let bytes = backing_bytes(&buf);
    assert!(bytes[0] >= 6 * 3 * size_of::<Cell>());
    assert!(bytes[1] >= 6 * 3 * size_of::<CellState>());
    assert!(bytes[2] >= 6 * size_of::<RowMetadata>());
    assert!(bytes[3] >= 6 * size_of::<u64>());
    for unit in ["e\u{301}", "\u{2764}\u{fe0f}", "👩\u{200d}💻"] {
        for limit in 0..=unit.len() + 1 {
            let prefix = prefix_at_byte_limit(unit, limit);
            assert!(prefix.len() <= limit);
            assert!(unit.starts_with(prefix));
            assert_eq!(prefix, &unit[..prefix.len()]);
        }
        assert_eq!(prefix_at_byte_limit(unit, 64), unit);
        for chunk in [1, 7, 4096] {
            let mut terminal = Terminal::new_headless(2, 8);
            for bytes in unit.as_bytes().chunks(chunk) {
                terminal.process_output(bytes);
            }
            assert_eq!(terminal.screen().cell(0, 0).raw_text(), unit);
        }
    }
}

#[test]
#[ignore = "manual release-mode storage/candidate measurement"]
fn calibration_history_candidates() {
    println!(
        "candidate_only; blank primary ring; grid rows=24; repetitions=7; bytes are owned array capacities, not process peaks; sizes Cell={} CellState={} RowMetadata={} row_version={}",
        size_of::<Cell>(),
        size_of::<CellState>(),
        size_of::<RowMetadata>(),
        size_of::<u64>()
    );
    for cols in [80, 160] {
        for rows in [1000, 4000, 16000] {
            let mut construct_ns = Vec::new();
            let mut resize_ns = Vec::new();
            let mut components = [0; 4];
            let mut old_plus_new_backing = 0;
            for _ in 0..7 {
                let start = Instant::now();
                let mut buf = black_box(NormalBuf::with_max_scrollback(24, cols, rows));
                construct_ns.push(start.elapsed().as_nanos());
                components = backing_bytes(&buf);
                let start = Instant::now();
                buf.resize(30, cols + 1); // rectangular NormalBuf only, NOT primary reflow/PTY
                resize_ns.push(start.elapsed().as_nanos());
                old_plus_new_backing =
                    components.iter().sum::<usize>() + backing_bytes(&buf).iter().sum::<usize>();
                black_box(buf);
            }
            println!(
                "rows={rows} cols={cols} components_bytes={components:?} modeled_old_plus_new_backing_bytes={old_plus_new_backing} construct_ns={construct_ns:?} rectangular_resize_ns={resize_ns:?}"
            );
            // Uniform full rows: arithmetic retention model, NOT actual eviction.
            // ASCII is 1 source byte/column; e+acute is 3 source bytes/column.
            for source_bytes_per_col in [1, 3] {
                for cap in [1024 * 1024, 4 * 1024 * 1024, 16 * 1024 * 1024] {
                    let row_bytes = cols * source_bytes_per_col;
                    let retained_rows = rows.min(cap / row_bytes);
                    println!(
                        "content_model rows={rows} cols={cols} source_bytes_per_col={source_bytes_per_col} cap_bytes={cap} retained_rows={retained_rows} retained_source_bytes={}",
                        retained_rows * row_bytes
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release-mode bounded Unicode growth measurement"]
fn calibration_text_unit_candidates() {
    println!(
        "candidate_only; grid=80x2; parser+engine feed timing excludes construction/PTY/render; full current retention versus scalar-aligned prefix MODEL; sample order retained"
    );
    let mut cases = vec![
        ("combining_control", "e\u{301}".to_owned()),
        ("selector_control", "\u{2764}\u{fe0f}".to_owned()),
        ("zwj_control", "👩\u{200d}💻".to_owned()),
    ];
    for extensions in [32, 128, 512, 4096] {
        cases.push((
            "combining_growth",
            format!("a{}", "\u{301}".repeat(extensions)),
        ));
        cases.push((
            "selector_growth",
            format!("❤{}", "\u{fe0f}".repeat(extensions)),
        ));
        cases.push((
            "zwj_growth",
            format!("👩{}", "\u{200d}👩".repeat(extensions)),
        ));
    }
    for (name, unit) in cases {
        for alternate in [false, true] {
            for chunk in [1, 7, 4096] {
                let mut feed_ns = Vec::new();
                let mut suffix_capacity = 0;
                for _ in 0..7 {
                    let mut terminal = Terminal::new_headless(2, 80);
                    if alternate {
                        terminal.process_output(b"\x1b[?1049h");
                    }
                    let start = Instant::now();
                    for bytes in unit.as_bytes().chunks(chunk) {
                        terminal.process_output(black_box(bytes));
                    }
                    feed_ns.push(start.elapsed().as_nanos());
                    let cell = terminal.screen().cell(0, 0);
                    assert_eq!(
                        cell.raw_text(),
                        unit,
                        "{name} chunk={chunk} alt={alternate}"
                    );
                    suffix_capacity = cell.suffix.capacity();
                    // An independent following unit remains independent on this baseline.
                    let next_col = usize::from(cell.grid_width());
                    terminal.process_output(b"Z");
                    assert_eq!(terminal.screen().cell(0, next_col).raw_text(), "Z");
                }
                println!(
                    "unit={name} source_bytes={} alternate={alternate} chunk={chunk} suffix_capacity_bytes={suffix_capacity} feed_ns={feed_ns:?}",
                    unit.len()
                );
            }
        }
        for limit in [64, 256, 1024] {
            let prefix = prefix_at_byte_limit(&unit, limit);
            println!(
                "prefix_model unit={name} source_bytes={} limit_bytes={limit} admitted_bytes={} discarded_bytes={}",
                unit.len(),
                prefix.len(),
                unit.len() - prefix.len()
            );
        }
    }
}
