use super::*;
use crate::primary_reflow::allocation_probe::measure;

fn fixture(with_suffix: bool) -> NormalBuf {
    let mut normal = NormalBuf::with_history_limits(
        2,
        4,
        HistoryLimits {
            max_rows: 4,
            max_bytes: 65536,
        },
    );
    for _ in 0..3 {
        for row in 0..2 {
            for col in 0..4 {
                normal.write_meaningful_cell(
                    row,
                    col,
                    Cell {
                        ch: 'a',
                        suffix: if with_suffix {
                            "\u{301}".repeat(32)
                        } else {
                            String::new()
                        },
                        ..Cell::default()
                    },
                );
            }
        }
        normal.scroll_up_full_screen(1, Cell::default());
    }
    normal
}

#[test]
fn retention_and_rectangular_allocate_suffixes_only_once_in_final_storage() {
    let plain = fixture(false);
    let source = fixture(true);
    let before = source.clone();
    let limits = HistoryLimits {
        max_rows: 1,
        max_bytes: 65536,
    };
    let (_, plain_cost) = measure(|| plain.prepare_history_limits(limits).unwrap());
    let (retained, cost) = measure(|| source.prepare_history_limits(limits).unwrap());
    let count = retained
        .retained_rows()
        .flat_map(|r| r.cells)
        .filter(|c| !c.suffix.is_empty())
        .count();
    assert!(count > 0);
    assert_eq!(cost.calls - plain_cost.calls, count);
    assert_eq!(source, before);
    for (old, new) in source.retained_rows().skip(2).zip(retained.retained_rows()) {
        assert_eq!(old.cells, new.cells);
        for (a, b) in old.cells.iter().zip(new.cells) {
            if !a.suffix.is_empty() {
                assert_ne!(a.suffix.as_ptr(), b.suffix.as_ptr());
            }
        }
    }

    let (_, plain_cost) = measure(|| plain.prepare_rectangular_resize(3, 3).unwrap());
    let (rect, cost) = measure(|| source.prepare_rectangular_resize(3, 3).unwrap());
    let count = rect
        .retained_rows()
        .flat_map(|r| r.cells)
        .filter(|c| !c.suffix.is_empty())
        .count();
    assert!(count > 0);
    assert_eq!(cost.calls - plain_cost.calls, count);
    assert_eq!(source, before);
    drop(source);
    assert_eq!(rect.cell(0, 0).suffix, "\u{301}".repeat(32));
}

#[test]
fn storage_failure_leaves_source_untouched() {
    let source = fixture(true);
    let before = source.clone();
    assert_eq!(
        source
            .prepare_rectangular_resize(usize::MAX, 2)
            .unwrap_err(),
        StorageError::ArithmeticOverflow
    );
    assert_eq!(source, before);
}
