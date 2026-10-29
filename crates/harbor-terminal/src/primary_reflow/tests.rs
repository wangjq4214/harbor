use std::num::NonZeroU32;

use crate::content_anchor::{Affinity, ContentProjection};
use crate::model::{CellAttrs, HyperlinkId};
use crate::selection_model::GenPos;

use super::*;

fn write(normal: &mut NormalBuf, row: usize, col: usize, cell: Cell) {
    normal.write_meaningful_cell(row, col, cell);
}

#[test]
fn normalizes_width_and_moves_complete_wide_atoms_past_edge_padding() {
    let mut normal = NormalBuf::new(1, 4);
    write(
        &mut normal,
        0,
        0,
        Cell {
            ch: 'a',
            ..Cell::default()
        },
    );
    let hyperlink = HyperlinkId::from_nonzero(NonZeroU32::new(7).unwrap());
    let wide = Cell {
        ch: '界',
        protected: true,
        hyperlink: Some(hyperlink),
        ..Cell::default()
    };
    write(&mut normal, 0, 1, wide.clone());
    write(&mut normal, 0, 2, Cell::wide_continuation_of(&wide));
    write(
        &mut normal,
        0,
        3,
        Cell {
            ch: 'b',
            ..Cell::default()
        },
    );

    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 1).unwrap();

    assert_eq!(prepared.cols(), 2);
    assert_eq!(prepared.rows().len(), 3);
    assert_eq!(prepared.rows()[0].metadata.meaningful_extent, 1);
    assert_eq!(prepared.rows()[0].cells[1], Cell::default());
    assert!(!prepared.rows()[0].cell_state[1].is_meaningful());
    assert_eq!(prepared.rows()[1].cells[0], wide);
    assert_eq!(
        prepared.rows()[1].cells[1],
        Cell::wide_continuation_of(&wide)
    );
    assert_eq!(prepared.rows()[1].metadata.logical_start, 1);
    assert_eq!(prepared.rows()[2].metadata.logical_start, 3);
    assert!(prepared.rows()[1].metadata.soft_wrapped);
    assert!(prepared.rows()[2].metadata.soft_wrapped);
}

#[test]
fn preserves_meaningful_trailing_blanks_and_hyperlink_payload() {
    let mut normal = NormalBuf::new(1, 4);
    let hyperlink = HyperlinkId::from_nonzero(NonZeroU32::new(9).unwrap());
    for (col, cell) in [
        Cell {
            ch: 'x',
            ..Cell::default()
        },
        Cell::default(),
        Cell {
            hyperlink: Some(hyperlink),
            ..Cell::default()
        },
    ]
    .into_iter()
    .enumerate()
    {
        write(&mut normal, 0, col, cell);
    }

    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 2).unwrap();

    assert_eq!(prepared.rows().len(), 2);
    assert_eq!(prepared.rows()[0].cells[1].ch, ' ');
    assert!(prepared.rows()[0].cell_state[1].is_meaningful());
    assert_eq!(prepared.rows()[1].cells[0].hyperlink, Some(hyperlink));
    assert!(prepared.rows()[1].cell_state[0].is_meaningful());
    assert_eq!(prepared.rows()[1].metadata.logical_start, 2);
}

#[test]
fn preserves_explicit_empty_lines_and_is_deterministic() {
    let mut normal = NormalBuf::new(3, 4);
    for (col, ch) in "abc".chars().enumerate() {
        write(
            &mut normal,
            0,
            col,
            Cell {
                ch,
                ..Cell::default()
            },
        );
    }
    write(
        &mut normal,
        2,
        0,
        Cell {
            ch: 'z',
            ..Cell::default()
        },
    );

    let first = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 2).unwrap();
    let second = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 2).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.rows().len(), 4);
    assert_eq!(first.rows()[2].metadata.meaningful_extent, 0);
    assert_ne!(
        first.rows()[1].metadata.logical_line_id,
        first.rows()[2].metadata.logical_line_id
    );
    assert_ne!(
        first.rows()[2].metadata.logical_line_id,
        first.rows()[3].metadata.logical_line_id
    );
}

#[test]
fn cursor_projection_derives_pending_wrap_from_new_width() {
    let mut normal = NormalBuf::new(1, 4);
    for (col, ch) in "abcd".chars().enumerate() {
        write(
            &mut normal,
            0,
            col,
            Cell {
                ch,
                ..Cell::default()
            },
        );
    }
    let projection = ContentProjection::build(&normal).unwrap();
    let pending = projection.cursor_anchor(GenPos::new(0, 3), true).unwrap();
    let selection = projection
        .to_anchor(GenPos::new(0, 2), Affinity::Before)
        .unwrap();

    let width_three = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 3).unwrap();
    let width_two = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 2).unwrap();

    assert_eq!(
        width_three.project_cursor(pending),
        Some(ProjectedInsertion {
            position: GenPos::new(2, 1),
            pending_wrap: false,
        })
    );
    assert_eq!(
        width_two.project_cursor(pending),
        Some(ProjectedInsertion {
            position: GenPos::new(2, 1),
            pending_wrap: true,
        })
    );
    assert_eq!(
        width_two.project_selection(selection),
        Some(GenPos::new(2, 0))
    );
}

fn logical_payload(prepared: &PreparedPrimaryResize) -> Vec<(LogicalLineId, Vec<Cell>)> {
    let mut lines: Vec<(LogicalLineId, Vec<Cell>)> = Vec::new();
    for row in prepared.rows() {
        if lines
            .last()
            .is_none_or(|(line_id, _)| *line_id != row.metadata.logical_line_id)
        {
            lines.push((row.metadata.logical_line_id, Vec::new()));
        }
        let cells = &mut lines.last_mut().unwrap().1;
        for cell in &row.cells[..row.metadata.meaningful_extent] {
            if !cell.wide_continuation {
                cells.push(cell.clone());
            }
        }
    }
    lines
}

#[test]
fn narrow_and_wide_preparations_preserve_logical_payload_without_padding() {
    let mut normal = NormalBuf::new(1, 8);
    let mut attrs = CellAttrs::default();
    attrs.set(CellAttrs::BOLD);
    let hyperlink = HyperlinkId::from_nonzero(NonZeroU32::new(11).unwrap());
    let cells = [
        Cell {
            ch: 'a',
            fg: harbor_config::Color::Indexed(4),
            attrs,
            ..Cell::default()
        },
        Cell {
            ch: ' ',
            bg: harbor_config::Color::Indexed(2),
            ..Cell::default()
        },
        Cell {
            ch: '界',
            hyperlink: Some(hyperlink),
            ..Cell::default()
        },
        Cell {
            ch: 'b',
            protected: true,
            ..Cell::default()
        },
    ];
    write(&mut normal, 0, 0, cells[0].clone());
    write(&mut normal, 0, 1, cells[1].clone());
    write(&mut normal, 0, 2, cells[2].clone());
    write(&mut normal, 0, 3, Cell::wide_continuation_of(&cells[2]));
    write(&mut normal, 0, 4, cells[3].clone());

    let narrow = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 3).unwrap();
    let wide = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 8).unwrap();

    assert_eq!(logical_payload(&narrow), logical_payload(&wide));
    assert_eq!(logical_payload(&wide)[0].1, cells);
    assert_eq!(narrow.rows()[0].metadata.meaningful_extent, 2);
    assert_eq!(narrow.rows()[0].cells[2], Cell::default());
}

#[test]
fn zero_width_normalizes_and_unallocatable_width_fails_cleanly() {
    let normal = NormalBuf::new(1, 2);
    assert_eq!(
        PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 0)
            .unwrap()
            .cols(),
        2
    );
    assert_eq!(
        PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), usize::MAX,),
        Err(PreparationError::AllocationFailed)
    );
}
#[test]
fn geometry_preparation_grows_with_distinct_blank_lines_and_full_damage() {
    let normal = NormalBuf::new_for_test(2, 3, 2);

    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 4, 3).unwrap();

    assert_eq!(prepared.target_rows(), 4);
    assert_eq!(prepared.rows().len(), 4);
    assert_eq!(prepared.normal().rows(), 4);
    assert_eq!(prepared.normal().scroll_count(), 0);
    assert_ne!(
        prepared.rows()[2].metadata.logical_line_id,
        prepared.rows()[3].metadata.logical_line_id
    );
    assert_eq!(prepared.normal().dirty_ranges().len(), 4);
    assert!(
        prepared
            .normal()
            .dirty_ranges()
            .iter()
            .all(|range| range.start_col == 0 && range.end_col == 3)
    );
}

#[test]
fn capacity_evicts_prefix_when_oversized_line_reaches_live_viewport() {
    let mut normal = NormalBuf::new_for_test(3, 4, 2);
    let mut ch = b'a';
    for row in 0..3 {
        for col in 0..4 {
            write(
                &mut normal,
                row,
                col,
                Cell {
                    ch: ch as char,
                    ..Cell::default()
                },
            );
            ch += 1;
        }
        if row > 0 {
            let source = normal.live_row_metadata(row - 1);
            let source_atoms = normal.live_row_logical_atom_count(row - 1);
            normal.continue_logical_line(row, source, source_atoms);
        }
    }
    let source_before = normal.clone();

    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 2, 2).unwrap();

    assert_eq!(prepared.dropped_rows(), 2);
    assert_eq!(prepared.rows().len(), 4);
    assert_eq!(prepared.normal().scroll_count(), 2);
    assert_eq!(prepared.normal().history_start(), 3);
    assert!(prepared.normal().cell_at_generation(0, 0).is_none());
    assert!(prepared.rows()[0].metadata.head_truncated);
    assert_eq!(prepared.rows()[0].metadata.logical_start, 4);
    assert_eq!(prepared.rows()[0].metadata.logical_atom_start, 4);
    assert_eq!(normal, source_before, "preparation must not mutate source");

    let prefix = prepared
        .source_anchor(GenPos::new(0, 0), Affinity::Before)
        .unwrap();
    let suffix = prepared
        .source_anchor(GenPos::new(2, 0), Affinity::Before)
        .unwrap();
    assert_eq!(prepared.project_selection(prefix), None);
    assert_eq!(prepared.project_selection(suffix), Some(GenPos::new(5, 0)));

    let live_cursor = prepared
        .source_cursor_anchor(GenPos::new(2, 3), false)
        .unwrap();
    let anchored = prepared
        .clone()
        .attach_screen_anchors(
            live_cursor,
            PreparedProjection::Projected(prefix),
            PreparedProjection::Projected(prefix),
        )
        .unwrap();
    assert_eq!(anchored.saved_cursor, PreparedProjection::Invalid);
    assert_eq!(
        anchored.review,
        PreparedProjection::Projected(GenPos::new(3, 0))
    );
    assert_eq!(anchored.normal().view_offset(), 2);

    let repeated = PreparedPrimaryResize::prepare_geometry(prepared.normal(), 2, 4).unwrap();
    assert!(repeated.rows()[0].metadata.head_truncated);
    assert_eq!(repeated.rows()[0].metadata.logical_start, 4);
    assert_eq!(repeated.rows()[0].metadata.logical_atom_start, 4);
    assert_eq!(repeated.normal().history_start(), 7);
}

#[test]
fn capacity_retains_head_truncated_historical_suffix() {
    let mut normal = NormalBuf::new_for_test(4, 4, 2);
    for (row, text) in ["abcd", "efgh", "x", "y"].into_iter().enumerate() {
        for (col, ch) in text.chars().enumerate() {
            write(
                &mut normal,
                row,
                col,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
    }
    let source = normal.live_row_metadata(0);
    let source_atoms = normal.live_row_logical_atom_count(0);
    normal.continue_logical_line(1, source, source_atoms);

    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 2, 2).unwrap();

    assert_eq!(prepared.dropped_rows(), 2);
    assert_eq!(prepared.rows().len(), 4);
    assert_eq!(prepared.normal().scroll_count(), 2);
    assert!(prepared.rows()[0].metadata.head_truncated);
    assert_eq!(prepared.rows()[0].metadata.logical_start, 4);
    assert_eq!(prepared.rows()[0].cells[0].ch, 'e');
    assert_eq!(prepared.rows()[1].cells[0].ch, 'g');
    assert_eq!(prepared.rows()[2].cells[0].ch, 'x');
    assert_eq!(prepared.rows()[3].cells[0].ch, 'y');
    let prefix = prepared
        .source_anchor(GenPos::new(0, 0), Affinity::Before)
        .unwrap();
    let suffix = prepared
        .source_anchor(GenPos::new(1, 0), Affinity::Before)
        .unwrap();
    assert_eq!(prepared.project_selection(prefix), None);
    assert!(prepared.project_selection(suffix).is_some());
}

#[test]
fn preparation_preserves_semantic_order_after_source_ring_wrap() {
    let mut normal = NormalBuf::new_for_test(2, 4, 2);
    for index in 0..5 {
        write(
            &mut normal,
            0,
            0,
            Cell {
                ch: (b'a' + index) as char,
                ..Cell::default()
            },
        );
        normal.scroll_up_full_screen(1, Cell::default());
    }
    let source_order: Vec<_> = normal.retained_rows().map(|row| row.cells[0].ch).collect();

    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 2, 2).unwrap();
    let prepared_order: Vec<_> = prepared.rows().iter().map(|row| row.cells[0].ch).collect();

    assert_eq!(prepared_order, source_order);
    assert_eq!(normal.history_start(), 3);
    assert_eq!(prepared.normal().history_start(), 7);
    assert!(prepared.normal().cell_at_generation(6, 0).is_none());
}

#[test]
fn height_shrink_rejects_a_live_cursor_outside_the_final_suffix() {
    let normal = NormalBuf::new_for_test(3, 2, 2);
    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 1, 2).unwrap();
    let cursor = prepared
        .source_cursor_anchor(GenPos::new(0, 0), false)
        .unwrap();

    assert_eq!(
        prepared.attach_screen_anchors(
            cursor,
            PreparedProjection::Absent,
            PreparedProjection::Absent,
        ),
        Err(PreparationError::UnresolvedLiveCursor)
    );
    assert_eq!(normal.rows(), 3);
}

#[test]
fn height_shrink_invalidates_saved_cursor_that_moves_into_history() {
    let normal = NormalBuf::new_for_test(3, 2, 2);
    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, 2, 2).unwrap();
    let live_cursor = prepared
        .source_cursor_anchor(GenPos::new(2, 0), false)
        .unwrap();
    let saved_cursor = prepared
        .source_cursor_anchor(GenPos::new(0, 0), false)
        .unwrap();

    let prepared = prepared
        .attach_screen_anchors(
            live_cursor,
            PreparedProjection::Projected(saved_cursor),
            PreparedProjection::Absent,
        )
        .unwrap();

    assert_eq!(prepared.saved_cursor, PreparedProjection::Invalid);
    assert_eq!(
        prepared.live_cursor,
        PreparedProjection::Projected(ProjectedInsertion {
            position: GenPos::new(5, 0),
            pending_wrap: false,
        })
    );
}

#[test]
fn generation_range_overflow_fails_before_materialization() {
    let mut normal = NormalBuf::new_for_test(1, 2, 0);
    normal.set_history_start_for_test(u64::MAX - 1);

    assert_eq!(
        PreparedPrimaryResize::prepare_geometry(&normal, 2, 2),
        Err(PreparationError::ArithmeticOverflow)
    );
    assert_eq!(normal.history_start(), u64::MAX - 1);
}

#[test]
fn accepts_head_truncated_input_without_mutating_source() {
    let mut normal = NormalBuf::new(1, 2);
    normal.set_head_truncated(0, true);
    let before = normal.row_metadata(0);

    let prepared = PreparedPrimaryResize::prepare_geometry(&normal, normal.rows(), 4).unwrap();
    assert!(prepared.rows()[0].metadata.head_truncated);
    assert_eq!(normal.row_metadata(0), before);
}

#[test]
fn conpty_already_wrapped_blank_prefix_is_geometry_even_without_style() {
    for styled in [false, true] {
        let mut normal = NormalBuf::new(4, 12);
        let blank = if styled {
            overlined_blank()
        } else {
            Cell::default()
        };
        for row in 0..2 {
            for col in 0..12 {
                write(&mut normal, row, col, blank.clone());
            }
        }
        for (col, ch) in "AB".chars().enumerate() {
            write(
                &mut normal,
                0,
                col,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
        normal.continue_logical_line(1, normal.live_row_metadata(0), 12);
        write(
            &mut normal,
            2,
            0,
            Cell {
                ch: '>',
                ..Cell::default()
            },
        );
        let prepared = prepare_conpty(&normal, 5, &[]);
        // Only the FINAL old source row is trimmable. The first 12-column
        // wrapForced footprint still occupies three rows at five columns.
        assert_eq!(prepared.rows()[3].cells[0].ch, '>');
        assert!(prepared.rows()[1].metadata.soft_wrapped);
        assert!(prepared.rows()[2].metadata.soft_wrapped);
        assert_eq!(
            prepared.rows()[2].metadata.meaningful_extent,
            if styled { 5 } else { 2 }
        );
        // Once widened to one row, the next shrink may discard its tail.
        let widened = prepare_conpty(prepared.normal(), 12, &[]);
        assert_eq!(widened.rows()[1].cells[0].ch, '>');
        let narrowed = prepare_conpty(widened.normal(), 5, &[]);
        assert_eq!(narrowed.rows()[1].cells[0].ch, '>');
    }
}

#[test]
fn conpty_clips_tail_at_exact_text_edge_without_splitting_wide_units() {
    for wide in [false, true] {
        let mut normal = NormalBuf::new(4, 12);
        let cols = if wide { 3 } else { 5 };
        if wide {
            let cell = Cell {
                ch: '界',
                width: 2,
                ..Cell::default()
            };
            write(
                &mut normal,
                0,
                0,
                Cell {
                    ch: 'a',
                    ..Cell::default()
                },
            );
            write(&mut normal, 0, 1, cell.clone());
            write(&mut normal, 0, 2, Cell::wide_continuation_of(&cell));
        } else {
            for (col, ch) in "ABCDE".chars().enumerate() {
                write(
                    &mut normal,
                    0,
                    col,
                    Cell {
                        ch,
                        ..Cell::default()
                    },
                );
            }
        }
        for col in cols..12 {
            write(&mut normal, 0, col, overlined_blank());
        }
        write(
            &mut normal,
            1,
            0,
            Cell {
                ch: '>',
                ..Cell::default()
            },
        );
        let prepared = prepare_conpty(&normal, cols, &[]);
        assert_eq!(prepared.rows()[0].metadata.meaningful_extent, cols);
        assert_eq!(prepared.rows()[1].cells[0].ch, '>');
        if wide {
            assert_eq!(prepared.rows()[0].cells[1].ch, '界');
            assert!(prepared.rows()[0].cells[2].wide_continuation);
        }
    }
}

fn overlined_blank() -> Cell {
    let mut cell = Cell::default();
    cell.attrs.set(CellAttrs::OVERLINE);
    cell
}

fn prepare_conpty(
    normal: &NormalBuf,
    cols: usize,
    cursors: &[CursorRetention],
) -> PreparedPrimaryResize {
    PreparedPrimaryResize::prepare_geometry_with_viewport(
        normal,
        normal.rows(),
        cols,
        None,
        ReflowViewport::PreserveLiveTop,
        cursors,
    )
    .unwrap()
}

#[test]
fn conpty_live_styled_tail_fits_last_row_and_drops_overflow_anchors() {
    for erased in [false, true] {
        let mut normal = NormalBuf::new(4, 12);
        for (col, ch) in "AB".chars().enumerate() {
            write(
                &mut normal,
                0,
                col,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
        for col in 2..12 {
            if erased {
                normal.erase_cell(0, col, overlined_blank());
            } else {
                write(&mut normal, 0, col, overlined_blank());
            }
        }
        for (col, ch) in "END".chars().enumerate() {
            write(
                &mut normal,
                1,
                col,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
        write(
            &mut normal,
            2,
            0,
            Cell {
                ch: '>',
                ..Cell::default()
            },
        );
        let projection = ContentProjection::build(&normal).unwrap();
        let surviving = projection
            .to_anchor(GenPos::new(0, 3), Affinity::Before)
            .unwrap();
        let discarded = projection
            .to_anchor(GenPos::new(0, 10), Affinity::Before)
            .unwrap();
        let prepared = prepare_conpty(&normal, 5, &[]);
        assert_eq!(prepared.rows()[0].metadata.meaningful_extent, 5);
        assert!(
            prepared.rows()[0].cells[2..]
                .iter()
                .all(|cell| cell == &overlined_blank())
        );
        assert_eq!(prepared.rows()[1].cells[0].ch, 'E');
        assert_eq!(prepared.rows()[2].cells[0].ch, '>');
        assert!(prepared.project_selection(surviving).is_some());
        assert!(prepared.project_selection(discarded).is_none());
        let widened = prepare_conpty(prepared.normal(), 12, &[]);
        assert_eq!(widened.rows()[0].metadata.meaningful_extent, 5);
        assert!(
            widened.rows()[0].cells[5..]
                .iter()
                .all(|cell| cell == &Cell::default())
        );
        // Standalone/non-ConPTY policy still reflows the entire decorated tail.
        let generic = PreparedPrimaryResize::prepare_geometry(&normal, 4, 5).unwrap();
        assert_eq!(generic.rows()[3].cells[0].ch, 'E');
    }
}

#[test]
fn conpty_erase_only_row_remains_one_decorated_hard_line() {
    let mut normal = NormalBuf::new(3, 12);
    normal.fill_row_with(0, overlined_blank());
    write(
        &mut normal,
        1,
        0,
        Cell {
            ch: '>',
            ..Cell::default()
        },
    );
    let prepared = prepare_conpty(&normal, 5, &[]);
    assert_eq!(prepared.rows()[0].metadata.meaningful_extent, 5);
    assert!(
        prepared.rows()[0]
            .cells
            .iter()
            .all(|cell| cell == &overlined_blank())
    );
    assert_eq!(prepared.rows()[1].cells[0].ch, '>');
    assert!(!prepared.rows()[1].metadata.soft_wrapped);
}

#[test]
fn conpty_tail_clipping_keeps_interior_spaces_protection_and_links() {
    for significant in [
        Cell {
            protected: true,
            ..overlined_blank()
        },
        Cell {
            hyperlink: Some(HyperlinkId::from_nonzero(NonZeroU32::new(7).unwrap())),
            ..overlined_blank()
        },
        Cell {
            ch: 'Z',
            ..Cell::default()
        },
    ] {
        let mut normal = NormalBuf::new(4, 12);
        for col in 0..12 {
            write(&mut normal, 0, col, overlined_blank());
        }
        write(&mut normal, 0, 8, significant.clone());
        write(
            &mut normal,
            1,
            0,
            Cell {
                ch: '>',
                ..Cell::default()
            },
        );
        let prepared = prepare_conpty(&normal, 5, &[]);
        assert_eq!(prepared.rows()[1].cells[3], significant);
        assert_eq!(prepared.rows()[1].cells[4], overlined_blank());
        assert_eq!(prepared.rows()[2].cells[0].ch, '>');
    }
}

#[test]
fn conpty_styled_history_is_not_clipped_and_live_top_stays_separate() {
    let mut normal = NormalBuf::new(3, 12);
    normal.fill_row_with(0, overlined_blank());
    normal.fill_row_with(1, overlined_blank());
    write(
        &mut normal,
        2,
        0,
        Cell {
            ch: '>',
            ..Cell::default()
        },
    );
    normal.scroll_up_full_screen(1, Cell::default());
    let prepared = prepare_conpty(&normal, 5, &[]);
    assert_eq!(prepared.normal().scroll_count(), 3);
    assert_eq!(prepared.rows()[0].metadata.meaningful_extent, 5);
    assert_eq!(prepared.rows()[1].metadata.meaningful_extent, 5);
    assert_eq!(prepared.rows()[2].metadata.meaningful_extent, 2);
    assert_eq!(prepared.rows()[3].metadata.meaningful_extent, 5);
    assert_eq!(prepared.rows()[4].cells[0].ch, '>');
}

#[test]
fn conpty_tail_crossing_history_keeps_history_atoms_and_live_boundary() {
    let mut normal = NormalBuf::new(3, 12);
    for (col, ch) in "abcdefghijkl".chars().enumerate() {
        write(
            &mut normal,
            0,
            col,
            Cell {
                ch,
                ..Cell::default()
            },
        );
    }
    normal.fill_row_with(1, overlined_blank());
    normal.continue_logical_line(1, normal.live_row_metadata(0), 12);
    write(
        &mut normal,
        2,
        0,
        Cell {
            ch: '>',
            ..Cell::default()
        },
    );
    normal.scroll_up_full_screen(1, Cell::default());
    let prepared = prepare_conpty(&normal, 5, &[]);
    assert_eq!(prepared.normal().scroll_count(), 3);
    assert_eq!(prepared.rows()[2].cells[0].ch, 'k');
    assert_eq!(prepared.rows()[2].cells[1].ch, 'l');
    assert_eq!(prepared.rows()[3].metadata.meaningful_extent, 5);
    assert!(prepared.rows()[3].metadata.soft_wrapped);
    assert_eq!(prepared.rows()[4].cells[0].ch, '>');
}

#[test]
fn conpty_live_and_saved_cursor_floors_survive_styled_tail_clipping() {
    let mut normal = NormalBuf::new(4, 12);
    normal.fill_row_with(0, overlined_blank());
    let projection = ContentProjection::build(&normal).unwrap();
    let saved = projection.cursor_anchor(GenPos::new(0, 8), false).unwrap();
    let prepared = prepare_conpty(
        &normal,
        5,
        &[
            CursorRetention::Position(GenPos::new(0, 2)),
            CursorRetention::Anchor(saved),
        ],
    );
    assert_eq!(prepared.rows()[1].metadata.meaningful_extent, 5);
    assert_eq!(prepared.rows()[2].metadata.meaningful_extent, 0);
    let projected = prepared.project_cursor(saved).unwrap();
    assert_eq!(projected.position.col, 3);
    assert!(!projected.pending_wrap);
}
