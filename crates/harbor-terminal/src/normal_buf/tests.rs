use super::*;

fn copy_fixture(rows: usize, cols: usize) -> NormalBuf {
    let mut buf = NormalBuf::with_max_scrollback(rows, cols, 0);
    for index in 0..buf.cells.len() {
        let mut cell = match index % 5 {
            0 => Cell {
                ch: '界',
                suffix: format!("\u{301}{index}\u{200d}\u{1f680}"),
                width: 2,
                protected: true,
                ..Cell::default()
            },
            1 => Cell::wide_continuation_of(&buf.cells[index - 1]),
            2 => Cell {
                ch: '\u{301}',
                suffix: format!("\u{fe0f}{index}"),
                isolated_mark: true,
                ..Cell::default()
            },
            3 => {
                let mut cell = Cell::default();
                cell.apply_sgr(41);
                cell.apply_sgr(4);
                cell.hyperlink = Some(crate::model::HyperlinkId::from_nonzero(
                    std::num::NonZeroU32::new(index as u32 + 1).unwrap(),
                ));
                cell
            }
            _ => Cell::default(),
        };
        if index % 3 == 0 {
            cell.discard_text_extension('\u{200d}');
        }
        buf.cell_state[index] = if index % 5 == 4 {
            CellState::erase(&cell)
        } else {
            CellState::explicit(&cell)
        };
        buf.cells[index] = cell;
    }
    for row in 0..rows {
        buf.row_metadata[row].logical_start = row * 10;
        buf.row_metadata[row].logical_atom_start = row * 7;
        buf.row_metadata[row].soft_wrapped = row % 2 != 0;
        buf.row_metadata[row].head_truncated = row % 3 == 0;
        buf.recompute_ring_row_extent(row);
    }
    buf.clear_dirty();
    buf
}

#[test]
fn copy_linear_range_matches_snapshot_in_both_directions_and_preserves_state() {
    let base = copy_fixture(2, 5);
    let old_snapshot = base.snapshot_cells();
    for start in 0..=base.cells.len() {
        for end in start..=base.cells.len() {
            let count = end - start;
            for dst in 0..=base.cells.len() - count {
                let mut actual = base.clone();
                let mut expected_cells = base.cells.clone();
                let mut expected_state = base.cell_state.clone();
                expected_cells[dst..dst + count].clone_from_slice(&base.cells[start..end]);
                expected_state.copy_within(start..end, dst);

                actual.copy_linear_range(start, end, dst);

                assert_eq!(actual.cells, expected_cells);
                assert_eq!(actual.cell_state, expected_state);
                let snapshot = actual.snapshot_cells();
                let mut revision = base.content_revision;
                for row in 0..base.total_rows {
                    let touched =
                        count > 0 && row >= dst / base.cols && row <= (dst + count - 1) / base.cols;
                    let mut metadata = base.row_metadata[row];
                    if touched {
                        revision += 1;
                        metadata.meaningful_extent = expected_state
                            [row * base.cols..(row + 1) * base.cols]
                            .iter()
                            .rposition(|state| state.is_meaningful())
                            .map_or(0, |col| col + 1);
                    }
                    assert_eq!(actual.row_metadata[row], metadata);
                    assert_eq!(
                        actual.row_versions[row],
                        if touched {
                            revision
                        } else {
                            base.row_versions[row]
                        }
                    );
                }
                assert_eq!(actual.content_revision, revision);
                assert_eq!(actual.dirty_ranges(), base.dirty_ranges());
                for index in 0..base.cells.len() {
                    assert_eq!(snapshot[index], expected_cells[index]);
                    assert_eq!(old_snapshot[index], base.cells[index]);
                }
            }
        }
    }
}

#[test]
fn copy_ring_rows_matches_snapshot_for_all_small_ring_overlaps() {
    for total in 1..=8 {
        let base = copy_fixture(total, 3);
        let old_snapshot = base.snapshot_cells();
        for src in 0..total {
            for count in 0..total {
                for dst in 0..total {
                    let mut actual = base.clone();
                    let mut expected_cells = base.cells.clone();
                    let mut expected_state = base.cell_state.clone();
                    let mut expected_metadata = base.row_metadata.clone();
                    let mut expected_versions = base.row_versions.clone();
                    for offset in 0..count {
                        let from = (src + offset) % total;
                        let to = (dst + offset) % total;
                        expected_cells[to * 3..to * 3 + 3]
                            .clone_from_slice(&base.cells[from * 3..from * 3 + 3]);
                        expected_state[to * 3..to * 3 + 3]
                            .copy_from_slice(&base.cell_state[from * 3..from * 3 + 3]);
                        expected_metadata[to] = base.row_metadata[from];
                        expected_versions[to] = base.content_revision + offset as u64 + 1;
                    }

                    actual.copy_ring_rows(src, (src + count) % total, dst);

                    assert_eq!(actual.cells, expected_cells);
                    assert_eq!(actual.cell_state, expected_state);
                    assert_eq!(actual.row_metadata, expected_metadata);
                    assert_eq!(actual.row_versions, expected_versions);
                    assert_eq!(
                        actual.content_revision,
                        base.content_revision + count as u64
                    );
                    assert_eq!(actual.content_generation, base.content_generation);
                    assert_eq!(actual.dirty_ranges(), base.dirty_ranges());
                    assert_eq!(actual.history_content_bytes(), base.history_content_bytes());
                    assert_eq!(actual.history_start(), base.history_start());
                    assert_eq!(actual.visible_start(), base.visible_start());
                    let snapshot = actual.snapshot_cells();
                    for index in 0..base.cells.len() {
                        assert_eq!(snapshot[index], expected_cells[index]);
                        assert_eq!(old_snapshot[index], base.cells[index]);
                    }
                }
            }
        }
    }
}

#[test]
fn new_creates_blank_grid() {
    let buf = NormalBuf::new(3, 4);
    assert_eq!(buf.rows(), 3);
    assert_eq!(buf.cols(), 4);
    for row in 0..3 {
        for col in 0..4 {
            assert_eq!(buf.cell(row, col).ch, ' ');
        }
    }
}

#[test]
fn write_and_read_cells() {
    let mut buf = NormalBuf::new(2, 3);
    buf.cell_mut(0, 0).ch = 'a';
    buf.cell_mut(0, 1).ch = 'b';
    buf.cell_mut(0, 2).ch = 'c';
    assert_eq!(buf.cell(0, 0).ch, 'a');
    assert_eq!(buf.cell(0, 1).ch, 'b');
    assert_eq!(buf.cell(0, 2).ch, 'c');
}

#[test]
fn cell_mut_writes_through_ring() {
    let mut buf = NormalBuf::new(2, 3);
    buf.cell_mut(0, 0).ch = 'x';
    assert_eq!(buf.cell(0, 0).ch, 'x');
}

#[test]
fn cells_iter_yields_row_major() {
    let mut buf = NormalBuf::new(2, 3);
    for i in 0..6 {
        let ch = (b'a' + i as u8) as char;
        buf.cells[buf.visible_start * buf.cols + i].ch = ch;
    }
    let collected: Vec<(usize, usize, char)> = buf.cells().collect();
    assert_eq!(collected.len(), 6);
    assert_eq!(collected[0], (0, 0, 'a'));
    assert_eq!(collected[1], (0, 1, 'b'));
    assert_eq!(collected[2], (0, 2, 'c'));
    assert_eq!(collected[3], (1, 0, 'd'));
    assert_eq!(collected[4], (1, 1, 'e'));
    assert_eq!(collected[5], (1, 2, 'f'));
}

#[test]
fn scroll_up_full_screen_advances_ring() {
    let mut buf = NormalBuf::new(2, 3);
    let old_start = buf.visible_start;
    buf.scroll_up_full_screen(1, Cell::default());
    assert_eq!(
        buf.visible_start,
        (old_start + 1) % buf.total_rows,
        "ring head should advance by 1"
    );
    assert!(buf.scroll_count >= 1, "scrollback should increase");
    assert_eq!(buf.dirty_rows().len(), 2, "all rows should be dirty");
}

#[test]
fn viewport_scroll_marks_all_dirty() {
    let mut buf = NormalBuf::new(3, 4);
    buf.clear_dirty();
    buf.scroll_count = 5;
    buf.scroll_up(2);
    assert_eq!(buf.view_offset, 2);
    assert_eq!(buf.dirty_rows().len(), 3);
}

#[test]
fn dirty_rows_returns_all_when_scrolled_back() {
    let mut buf = NormalBuf::new(3, 4);
    buf.clear_dirty();
    buf.scroll_count = 5;
    buf.scroll_up(1);
    let dirty = buf.dirty_rows();
    assert_eq!(dirty.len(), 3, "all rows dirty when scrolled back");
}

#[test]
fn resize_preserves_scrollback_generations() {
    let mut buf = NormalBuf::new(2, 3);
    for ch in ['A', 'B', 'C'] {
        buf.write_meaningful_cell(
            0,
            0,
            Cell {
                ch,
                ..Cell::default()
            },
        );
        buf.scroll_up_full_screen(1, Cell::default());
    }
    buf.write_meaningful_cell(
        0,
        0,
        Cell {
            ch: 'D',
            ..Cell::default()
        },
    );
    buf.scroll_up(2);
    let displayed = buf.row_text(0);
    let history_start = buf.history_start();

    buf.resize(3, 5);

    assert_eq!(buf.rows(), 3);
    assert_eq!(buf.cols(), 5);
    assert_eq!(buf.scroll_count, 3);
    assert_eq!(buf.view_offset, 2);
    assert_eq!(buf.history_start, history_start);
    assert_eq!(buf.row_text(0), format!("{displayed}  "));
    assert_eq!(
        buf.cell_at_generation(history_start, 0)
            .expect("oldest retained generation")
            .ch,
        'A'
    );
    assert_eq!(
        buf.cell_at_generation(history_start + 2, 0)
            .expect("latest retained history generation")
            .ch,
        'C'
    );
    assert_eq!(buf.dirty_rows().len(), 3);
}

#[test]
fn cells_iter_exact_size() {
    let buf = NormalBuf::new(2, 3);
    let iter = buf.cells();
    assert_eq!(iter.len(), 6);
    assert_eq!(iter.size_hint(), (6, Some(6)));
}

// ── generation stability ───────────────────────────────────────

#[test]
fn history_starts_at_zero() {
    let buf = NormalBuf::new(5, 10);
    assert_eq!(buf.history_start(), 0);
}

#[test]
fn scroll_up_full_screen_eviction_increments_history_start() {
    let mut buf = NormalBuf::new(5, 3);
    let max = buf.max_scrollback();
    for _ in 0..max + 10 {
        buf.scroll_up_full_screen(1, Cell::default());
    }
    assert_eq!(buf.history_start(), 10);
    assert_eq!(buf.scroll_count(), max);
}

#[test]
fn resize_after_ring_wrap_preserves_generation_coordinates() {
    let mut buf = NormalBuf::new(2, 2);
    let max = buf.max_scrollback();
    for index in 0..max + 7 {
        buf.write_meaningful_cell(
            0,
            0,
            Cell {
                ch: (b'a' + (index % 26) as u8) as char,
                ..Cell::default()
            },
        );
        buf.scroll_up_full_screen(1, Cell::default());
    }
    buf.scroll_up(1);
    let displayed = buf.row_text(0);
    let history_start = buf.history_start();

    buf.resize(2, 4);

    assert_eq!(buf.history_start(), history_start);
    assert_eq!(buf.scroll_count(), max);
    assert_eq!(buf.view_offset(), 1);
    assert_eq!(buf.row_text(0), format!("{displayed}  "));
    assert_eq!(
        buf.cell_at_generation(history_start, 0)
            .expect("oldest retained generation")
            .ch,
        (b'a' + 7) as char
    );
    assert!(buf.cell_at_generation(history_start - 1, 0).is_none());
}

#[test]
fn cell_at_generation_returns_none_for_evicted() {
    let mut buf = NormalBuf::new(5, 3);
    let max = buf.max_scrollback();
    for _ in 0..max + 1 {
        buf.scroll_up_full_screen(1, Cell::default());
    }
    assert!(
        buf.cell_at_generation(0, 0).is_none(),
        "evicted gen should be None"
    );
    assert!(
        buf.cell_at_generation(buf.history_start(), 0).is_some(),
        "oldest valid gen should be Some"
    );
}

#[test]
fn cell_at_generation_returns_correct_content() {
    let mut buf = NormalBuf::new(5, 3);
    buf.write_meaningful_cell(
        0,
        0,
        Cell {
            ch: 'X',
            ..Cell::default()
        },
    );
    let cell = buf
        .cell_at_generation(0, 0)
        .expect("valid gen should return cell");
    assert_eq!(cell.ch, 'X');
}

#[test]
fn cell_at_generation_out_of_range_returns_none() {
    let buf = NormalBuf::new(5, 3);
    let max_gen = buf.scroll_count() + buf.rows();
    assert!(
        buf.cell_at_generation(max_gen as u64, 0).is_none(),
        "gen past end should be None"
    );
}

// ── per-row soft-wrap flags ───────────────────────────────────

#[test]
fn should_roundtrip_wrapped_flag_through_set_and_get() {
    // Arrange
    let mut buf = NormalBuf::new(3, 4);
    // Act
    buf.set_wrapped(1, true);
    // Assert
    assert!(!buf.is_wrapped(0), "unset rows default to unwrapped");
    assert!(buf.is_wrapped(1), "set_wrapped must persist the flag");
    assert!(!buf.is_wrapped(2), "unset rows default to unwrapped");
    buf.set_wrapped(1, false);
    assert!(!buf.is_wrapped(1), "set_wrapped(false) must clear the flag");
}

#[test]
fn should_clear_wrapped_flag_when_fill_row_with() {
    // Arrange
    let mut buf = NormalBuf::new(2, 3);
    buf.set_wrapped(0, true);
    buf.set_wrapped(1, true);
    // Act
    buf.fill_row_with(0, Cell::default());
    // Assert
    assert!(!buf.is_wrapped(0), "filled row must clear its wrap flag");
    assert!(buf.is_wrapped(1), "untouched row must keep its wrap flag");
}

#[test]
fn should_fill_every_visible_row_with_supplied_cell_and_clear_wrap_flags() {
    // Arrange
    let mut buf = NormalBuf::new(3, 2);
    for row in 0..buf.rows() {
        buf.set_wrapped(row, true);
    }
    let cell = Cell {
        ch: 'E',
        ..Cell::default()
    };

    // Act
    buf.fill_all_with(cell.clone());

    // Assert
    for row in 0..buf.rows() {
        for col in 0..buf.cols() {
            assert_eq!(buf.cell(row, col), &cell);
        }
        assert!(!buf.is_wrapped(row));
    }
}

#[test]
fn should_keep_default_blank_behavior_for_fill_all() {
    // Arrange
    let mut buf = NormalBuf::new(1, 2);
    buf.cell_mut(0, 0).ch = 'X';

    // Act
    buf.fill_all_with(Cell::default());

    // Assert
    assert_eq!(buf.row_text(0), "  ");
}

#[test]
fn should_preserve_wrapped_flags_on_surviving_rows_and_blank_new_rows_when_resize() {
    // Arrange
    let mut buf = NormalBuf::new(2, 3);
    buf.set_wrapped(0, true);
    buf.set_wrapped(1, true);
    // Act
    buf.resize(4, 5);
    // Assert
    assert!(buf.is_wrapped(0), "surviving row 0 keeps its flag");
    assert!(buf.is_wrapped(1), "surviving row 1 keeps its flag");
    assert!(!buf.is_wrapped(2), "newly exposed row 2 is unwrapped");
    assert!(!buf.is_wrapped(3), "newly exposed row 3 is unwrapped");
}

#[test]
fn should_clear_wrapped_flags_on_blanked_rows_when_scroll_up_full_screen() {
    // Arrange
    let mut buf = NormalBuf::new(2, 3);
    buf.set_wrapped(0, true);
    buf.set_wrapped(1, true);
    // Act
    buf.scroll_up_full_screen(1, Cell::default());
    // Assert: ring advanced — old row 1 is now display row 0 (flag kept),
    // the newly blanked bottom row is unwrapped.
    assert!(buf.is_wrapped(0), "scrolled-up row keeps its flag");
    assert!(!buf.is_wrapped(1), "blanked bottom row is unwrapped");
}

#[test]
fn copy_ring_rows_snapshots_source_and_wraps_destination_after_rotation() {
    let mut buf = NormalBuf::new(2, 3);
    let total = buf.total_rows();
    buf.scroll_up_full_screen(1, Cell::default());
    assert_eq!(buf.visible_start(), total - 1, "ring must be rotated");

    let src_start = total - 2;
    let src_end = 1;
    for (offset, ch) in ['a', 'b', 'c'].into_iter().enumerate() {
        let ring_row = (src_start + offset) % total;
        let index = ring_row * buf.cols + offset;
        let cell = Cell {
            ch,
            ..Cell::default()
        };
        buf.cell_state[index] = CellState::explicit(&cell);
        buf.cells[index] = cell;
        buf.row_metadata[ring_row].logical_start = 10 + offset;
        buf.row_metadata[ring_row].soft_wrapped = offset != 0;
        buf.recompute_ring_row_extent(ring_row);
    }
    let expected: Vec<_> = (0..3)
        .map(|offset| {
            let ring_row = (src_start + offset) % total;
            let start = ring_row * buf.cols;
            (
                buf.cells[start..start + buf.cols].to_vec(),
                buf.cell_state[start..start + buf.cols].to_vec(),
                buf.row_metadata[ring_row],
            )
        })
        .collect();

    let dst = total - 1;
    buf.copy_ring_rows(src_start, src_end, dst);

    for (offset, (cells, states, metadata)) in expected.into_iter().enumerate() {
        let ring_row = (dst + offset) % total;
        let start = ring_row * buf.cols;
        assert_eq!(&buf.cells[start..start + buf.cols], cells.as_slice());
        assert_eq!(&buf.cell_state[start..start + buf.cols], states.as_slice());
        assert_eq!(buf.row_metadata[ring_row], metadata);
    }
}

#[test]
fn retained_metadata_initializes_with_distinct_independent_rows() {
    let buf = NormalBuf::new(3, 4);
    let metadata: Vec<_> = (0..buf.rows()).map(|row| buf.row_metadata(row)).collect();

    assert_ne!(metadata[0].logical_line_id, metadata[1].logical_line_id);
    assert_ne!(metadata[1].logical_line_id, metadata[2].logical_line_id);
    for row in 0..buf.rows() {
        let metadata = buf.row_metadata(row);
        assert_eq!(metadata.logical_start, 0);
        assert_eq!(metadata.meaningful_extent, 0);
        assert!(!metadata.soft_wrapped);
        assert!(!metadata.head_truncated);
        for col in 0..buf.cols() {
            assert!(!buf.cell_is_meaningful(row, col));
        }
    }
}

#[test]
fn attribute_mutation_preserves_explicit_provenance_and_recomputes_style_visibility() {
    let mut buf = NormalBuf::new(1, 4);

    buf.write_meaningful_cell(0, 0, Cell::default());
    buf.mutate_cell_semantics(0, 0, |cell| cell.apply_sgr(41));
    assert!(buf.cell_is_meaningful(0, 0));
    buf.mutate_cell_semantics(0, 0, |cell| cell.apply_sgr(49));
    assert!(
        buf.cell_is_meaningful(0, 0),
        "a printed default blank remains explicit after background removal"
    );

    buf.mutate_cell_semantics(0, 1, |cell| cell.apply_sgr(41));
    assert!(buf.cell_is_meaningful(0, 1));
    buf.mutate_cell_semantics(0, 1, |cell| cell.apply_sgr(49));
    assert!(
        !buf.cell_is_meaningful(0, 1),
        "an unused blank is meaningful only while its background is visible"
    );

    buf.mutate_cell_semantics(0, 2, |cell| cell.apply_sgr(7));
    assert!(buf.cell_is_meaningful(0, 2));
    buf.mutate_cell_semantics(0, 2, |cell| cell.apply_sgr(27));
    assert!(
        !buf.cell_is_meaningful(0, 2),
        "an unused blank is meaningful only while inverse is active"
    );

    let hyperlink = crate::model::HyperlinkId::from_nonzero(
        std::num::NonZeroU32::new(1).expect("non-zero hyperlink ID"),
    );
    buf.write_meaningful_cell(
        0,
        3,
        Cell {
            hyperlink: Some(hyperlink),
            ..Cell::default()
        },
    );
    buf.mutate_cell_semantics(0, 3, |cell| cell.apply_sgr(49));
    assert!(
        buf.cell_is_meaningful(0, 3),
        "a hyperlinked printed blank retains explicit provenance"
    );
    assert_eq!(buf.row_metadata(0).meaningful_extent, 4);
}

#[test]
fn fresh_row_identity_repairs_a_three_row_soft_wrapped_chain() {
    let mut buf = NormalBuf::new(3, 4);
    for (row, col) in [(0, 3), (1, 1), (2, 2)] {
        buf.write_meaningful_cell(
            row,
            col,
            Cell {
                ch: 'x',
                ..Cell::default()
            },
        );
    }
    let first = buf.live_row_metadata(0);
    let first_atoms = buf.live_row_logical_atom_count(0);
    buf.continue_logical_line(1, first, first_atoms);
    let second = buf.live_row_metadata(1);
    let second_atoms = buf.live_row_logical_atom_count(1);
    buf.continue_logical_line(2, second, second_atoms);

    buf.begin_hard_line(0);

    let head = buf.live_row_metadata(0);
    let middle = buf.live_row_metadata(1);
    let tail = buf.live_row_metadata(2);
    assert!(!head.soft_wrapped);
    assert_eq!(middle.logical_line_id, head.logical_line_id);
    assert_eq!(middle.logical_start, head.meaningful_extent);
    assert_eq!(tail.logical_line_id, head.logical_line_id);
    assert_eq!(
        tail.logical_start,
        middle.logical_start + middle.meaningful_extent
    );

    buf.fill_row_with(0, Cell::default());

    let blank = buf.live_row_metadata(0);
    let middle = buf.live_row_metadata(1);
    let tail = buf.live_row_metadata(2);
    assert!(!blank.soft_wrapped, "fresh blank row is a hard boundary");
    assert!(middle.soft_wrapped);
    assert_eq!(middle.logical_line_id, blank.logical_line_id);
    assert_eq!(middle.logical_start, blank.meaningful_extent);
    assert_eq!(tail.logical_line_id, blank.logical_line_id);
    assert_eq!(
        tail.logical_start,
        middle.logical_start + middle.meaningful_extent
    );
    assert!(tail.soft_wrapped);
}

#[test]
fn sever_soft_wrap_changes_only_a_continuation_identity() {
    let mut buf = NormalBuf::new(3, 4);
    let hard_id = buf.live_row_metadata(0).logical_line_id;

    buf.sever_soft_wrap(0);
    assert_eq!(
        buf.live_row_metadata(0).logical_line_id,
        hard_id,
        "an existing hard line must keep its anchor identity"
    );

    let source = buf.live_row_metadata(0);
    let source_atoms = buf.live_row_logical_atom_count(0);
    buf.continue_logical_line(1, source, source_atoms);
    let old_continuation_id = buf.live_row_metadata(1).logical_line_id;
    buf.sever_soft_wrap(1);

    let severed = buf.live_row_metadata(1);
    assert!(!severed.soft_wrapped);
    assert_ne!(severed.logical_line_id, old_continuation_id);
}

#[test]
fn resize_clears_a_wide_glyph_when_its_continuation_is_clipped() {
    let mut buf = NormalBuf::new(1, 3);
    let base = Cell {
        ch: '界',
        ..Cell::default()
    };
    let continuation = Cell {
        wide_continuation: true,
        ..Cell::default()
    };
    buf.write_meaningful_cell(0, 1, base);
    buf.write_meaningful_cell(0, 2, continuation);

    buf.resize(1, 2);

    assert_eq!(buf.cell(0, 1), &Cell::default());
    assert!(!buf.cell_is_meaningful(0, 1));
    assert_eq!(buf.live_row_metadata(0).meaningful_extent, 0);
}

#[test]
fn reset_allocates_fresh_ids_and_resize_preserves_surviving_metadata() {
    let mut buf = NormalBuf::new(2, 4);
    buf.write_meaningful_cell(
        0,
        3,
        Cell {
            ch: 'x',
            ..Cell::default()
        },
    );
    let before_resize = buf.row_metadata(0);

    buf.resize(3, 2);
    let after_resize = buf.row_metadata(0);
    assert_eq!(after_resize.logical_line_id, before_resize.logical_line_id);
    assert_eq!(
        after_resize.meaningful_extent, 0,
        "clipped meaning repairs extent"
    );
    assert!(!after_resize.head_truncated);
    let ids_before_reset: Vec<_> = (0..buf.rows())
        .map(|row| buf.row_metadata(row).logical_line_id)
        .collect();

    buf.reset_all_retained();
    for row in 0..buf.rows() {
        let metadata = buf.row_metadata(row);
        assert!(!ids_before_reset.contains(&metadata.logical_line_id));
        assert_eq!(metadata.meaningful_extent, 0);
        assert!(!metadata.head_truncated);
    }
}

#[test]
fn retained_rows_are_bounded_and_generation_ordered_after_rotation() {
    let mut buf = NormalBuf::new(3, 2);
    buf.write_meaningful_cell(
        1,
        0,
        Cell {
            ch: 'Z',
            ..Cell::default()
        },
    );
    for _ in 0..=buf.max_scrollback() {
        buf.scroll_up_full_screen(1, Cell::default());
    }

    let rows: Vec<_> = buf.retained_rows().collect();
    assert_eq!(rows.len(), buf.max_scrollback() + buf.rows());
    assert_eq!(rows[0].generation, buf.history_start());
    assert_eq!(rows[0].generation, 1);
    assert_eq!(rows[0].cells[0].ch, 'Z');
    assert!(rows[0].cell_state[0].is_meaningful());
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.generation, buf.history_start() + index as u64);
        assert_eq!(row.cells.len(), buf.cols());
        assert_eq!(row.cell_state.len(), buf.cols());
    }
}
