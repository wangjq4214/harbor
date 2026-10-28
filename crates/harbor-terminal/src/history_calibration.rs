//! Test-only observations for Spec 0031/T0001, not shipping budget enforcement.
//! cargo test --release -p harbor-terminal calibration -- --ignored --nocapture --test-threads=1
#[path = "history_calibration/perf.rs"]
mod perf;

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
