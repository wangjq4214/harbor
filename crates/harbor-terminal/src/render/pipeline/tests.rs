#[path = "tests/gpu.rs"]
mod gpu;

use super::*;
use crate::Terminal;
use crate::render::gpu::{ColoredVertex, TexturedVertex, UploadMode, UploadPolicy};
use std::time::Instant;

#[test]
fn engine_damage_drives_full_and_incremental_uploads_for_all_grid_layers() {
    let mut terminal = Terminal::new_headless(10, 20);
    let now = Instant::now();
    let initial = terminal.read_update(now);
    let full = update_dirty_ranges(&initial);
    assert_eq!(full.len(), 10);
    assert!(
        full.iter()
            .enumerate()
            .all(|(row, range)| range.row == row && range.start_col == 0 && range.end_col == 20)
    );
    assert!(terminal.acknowledge_update(&initial));

    terminal.put_str("x");
    let incremental = terminal.read_update(now);
    let ranges = update_dirty_ranges(&incremental);
    assert!(matches!(incremental.damage, UpdateDamage::Ranges(_)));
    assert!(!ranges.is_empty());
    let policy = UploadPolicy::default();
    for bytes_per_cell in [
        6 * std::mem::size_of::<ColoredVertex>(),
        6 * std::mem::size_of::<TexturedVertex>(),
    ] {
        assert_eq!(
            policy.decide(10, 20, bytes_per_cell, &full, false).mode,
            UploadMode::Full
        );
        assert_eq!(
            policy.decide(10, 20, bytes_per_cell, &ranges, false).mode,
            UploadMode::Incremental
        );
    }
    // Reads and skipped draws never consume the pending ranges.
    assert_eq!(update_dirty_ranges(&terminal.read_update(now)), ranges);
    terminal.invalidate_update();
    assert!(!terminal.acknowledge_update(&incremental));
    assert_eq!(update_dirty_ranges(&terminal.read_update(now)), full);
}
