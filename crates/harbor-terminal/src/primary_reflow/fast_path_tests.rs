use super::*;
use crate::content_anchor::ContentProjection;
use crate::model::{CellAttrs, HyperlinkId};
use proptest::prelude::*;
use std::num::NonZeroU32;

fn mixed_source(data: &[u8], cols: usize, scrolls: usize) -> NormalBuf {
    let mut normal = NormalBuf::new_for_test(8, cols, 4);
    for row in 0..8 {
        let mut col = 0;
        while col < cols {
            let value = data
                .get((row * cols + col) % data.len().max(1))
                .copied()
                .unwrap_or(0);
            let mut cell = Cell {
                ch: (b'a' + value % 26) as char,
                ..Cell::default()
            };
            match value % 12 {
                0 => cell = Cell::default(),
                1 => {
                    cell.ch = ' ';
                    cell.attrs.set(CellAttrs::OVERLINE);
                }
                2 => cell.attrs.set(CellAttrs::BOLD),
                3 => {
                    cell.ch = 'e';
                    cell.suffix = "\u{301}".to_owned();
                }
                4 if col + 1 < cols => {
                    cell.ch = '界';
                    cell.width = 2;
                }
                5 if col + 1 < cols => {
                    cell.ch = '👩';
                    cell.suffix = "\u{200d}💻".to_owned();
                    cell.width = 2;
                }
                6 => cell.hyperlink = Some(HyperlinkId::from_nonzero(NonZeroU32::new(1).unwrap())),
                7 => cell.protected = true,
                8 => {
                    cell.ch = '\u{301}';
                    cell.isolated_mark = true;
                }
                _ => {}
            }
            let width = usize::from(cell.grid_width());
            if value % 12 == 1 {
                normal.erase_cell(row, col, cell.clone());
            } else {
                normal.write_meaningful_cell(row, col, cell.clone());
            }
            if width == 2 {
                normal.write_meaningful_cell(row, col + 1, Cell::wide_continuation_of(&cell));
            }
            col += width;
        }
        if row > 0 && data.get(row % data.len().max(1)).is_none_or(|v| v % 2 == 0) {
            normal.continue_logical_line(
                row,
                normal.live_row_metadata(row - 1),
                normal.live_row_logical_atom_count(row - 1),
            );
        }
    }
    for _ in 0..scrolls {
        normal.scroll_up_full_screen(1, Cell::default());
    }
    normal
}

fn compare_preparation(
    normal: &NormalBuf,
    rows: usize,
    cols: usize,
    retain: bool,
    limits: harbor_config::HistoryLimits,
) {
    let before = normal.clone();
    let revision = normal.content_revision();
    let generation = normal.history_start() + normal.scroll_count() as u64;
    let position = GenPos::new(generation, normal.cols() - 1);
    let projection = ContentProjection::build(normal).unwrap();
    let mut cursors = Vec::new();
    if retain {
        cursors.push(CursorRetention::Position(position));
        if let Some(anchor) = projection.cursor_anchor(position, true) {
            cursors.push(CursorRetention::Anchor(anchor));
        }
    }
    for viewport in [ReflowViewport::PullHistory, ReflowViewport::PreserveLiveTop] {
        let expected = reference::prepare_geometry_with_limits(
            normal, rows, cols, None, viewport, &cursors, limits,
        );
        let actual = PreparedPrimaryResize::prepare_geometry_with_limits(
            normal, rows, cols, None, viewport, &cursors, limits,
        );
        assert_eq!(
            actual, expected,
            "rows={rows} cols={cols} viewport={viewport:?} retain={retain}"
        );
        if let (Ok(actual), Ok(expected)) = (actual, expected) {
            // Compare consumers too, including before/after affinities and wrap.
            for source in normal.retained_rows() {
                for col in 0..normal.cols() {
                    let pos = GenPos::new(source.generation, col);
                    for affinity in [Affinity::Before, Affinity::After] {
                        let anchor = expected.source_anchor(pos, affinity);
                        assert_eq!(actual.source_anchor(pos, affinity), anchor);
                        if let Some(anchor) = anchor {
                            assert_eq!(
                                actual.project_selection(anchor),
                                expected.project_selection(anchor)
                            );
                            assert_eq!(
                                actual.project_cursor(anchor),
                                expected.project_cursor(anchor)
                            );
                        }
                    }
                    for pending in [false, true] {
                        let anchor = expected.source_cursor_anchor(pos, pending);
                        assert_eq!(actual.source_cursor_anchor(pos, pending), anchor);
                        if let Some(anchor) = anchor {
                            assert_eq!(
                                actual.project_cursor(anchor),
                                expected.project_cursor(anchor)
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(normal, &before);
    assert_eq!(normal.content_revision(), revision);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn batched_preparation_matches_scalar_reference(
        data in prop::collection::vec(any::<u8>(), 0..256),
        source_cols in 2usize..25,
        target_cols in 0usize..33,
        target_rows in 1usize..12,
        scrolls in 0usize..13,
        retain in any::<bool>(),
        max_rows in 0usize..9,
        max_bytes in 0usize..300,
    ) {
        let normal = mixed_source(&data, source_cols, scrolls);
        let count = normal.scroll_count() + normal.rows();
        prop_assert_eq!(logical_content::decode_lines_active(&normal, count), reference::decode_lines_active(&normal, count));
        compare_preparation(&normal, target_rows, target_cols, retain, harbor_config::HistoryLimits { max_rows, max_bytes });
    }
}

#[test]
fn long_runs_and_complex_boundaries_match_scalar_reference() {
    // Long simple runs, short runs, alternating styles, complex payloads, and
    // entirely blank lines, each with/without cursor retention.
    for data in [
        &[9][..],
        &[2][..],
        &[0][..],
        &[9, 10, 11, 3, 9, 9, 9, 4, 9, 9, 6][..],
    ] {
        for scrolls in [0, 3, 7, 12] {
            let normal = mixed_source(data, 80, scrolls);
            for cols in [2, 3, 7, 40, 79, 80, 160] {
                for retain in [false, true] {
                    compare_preparation(&normal, 8, cols, retain, normal.history_limits());
                }
            }
        }
    }
}

#[test]
fn malformed_input_retains_decode_errors_and_does_not_mutate_source() {
    for corrupt in 0..3 {
        let mut normal = mixed_source(&[9], 8, 0);
        match corrupt {
            0 => normal.cell_mut(7, 3).ch = '界',
            1 => normal.cell_mut(7, 3).wide_continuation = true,
            _ => normal.set_wrapped(0, true),
        }
        for count in [0, 1, 7, 8] {
            assert_eq!(
                logical_content::decode_lines_active(&normal, count),
                reference::decode_lines_active(&normal, count)
            );
        }
        let before = normal.clone();
        for viewport in [ReflowViewport::PullHistory, ReflowViewport::PreserveLiveTop] {
            assert_eq!(
                PreparedPrimaryResize::prepare_geometry_with_limits(
                    &normal,
                    8,
                    4,
                    None,
                    viewport,
                    &[],
                    normal.history_limits()
                ),
                reference::prepare_geometry_with_limits(
                    &normal,
                    8,
                    4,
                    None,
                    viewport,
                    &[],
                    normal.history_limits()
                )
            );
        }
        assert_eq!(normal, before);
    }
}

#[test]
fn malformed_glyphs_cannot_hide_inside_simple_runs() {
    let normal = mixed_source(&[9], 16, 0);
    for corrupt in 0..4 {
        let mut line = logical_content::decode_lines_active(&normal, 8)
            .unwrap()
            .remove(0);
        let glyph = &mut line.glyphs[5];
        match corrupt {
            0 => glyph.atom_offset.0 += 1,
            1 => glyph.glyph.width = 0,
            2 => glyph.glyph.cell.width = 2,
            _ => {
                glyph.glyph.cell.ch = ' ';
                glyph.glyph.meaningful_blank = false;
            }
        }
        let mut expected_rows = Vec::new();
        let expected = reference::pack_line(&line, 7, None, &[], &mut expected_rows);
        let mut actual_rows = Vec::new();
        assert_eq!(
            pack_line(&mut line, 7, None, &[], &mut actual_rows),
            expected
        );
        assert!(expected.is_err());
    }
}
