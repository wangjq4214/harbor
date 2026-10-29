use super::*;

#[test]
fn conpty_trimmed_live_continuation_does_not_pull_history_into_viewport() {
    for cols in [2, 4, 6] {
        let mut screen = Screen::new(2, 4);
        for ch in "ABCD  ".chars() {
            screen.write_char(ch);
        }
        screen.carriage_return();
        screen.line_feed();
        assert_eq!(screen.scroll_count(), 1);
        assert_eq!((screen.cursor_y(), screen.cursor_x()), (1, 0));

        let prepared = screen
            .prepare_resize_with_viewport(2, cols, ReflowViewport::PreserveLiveTop)
            .unwrap();
        screen.commit_resize(prepared);
        assert_eq!(
            screen.scroll_count(),
            if cols == 2 { 2 } else { 1 },
            "history must remain outside ConPTY live viewport at width {cols}"
        );
        assert_eq!(
            screen
                .cell_at_generation(screen.history_start(), 0)
                .unwrap()
                .ch,
            'A'
        );
        assert_eq!(screen.row_text(0), " ".repeat(cols));
        assert_eq!((screen.cursor_y(), screen.cursor_x()), (1, 0));
        screen.write_char('!');
        assert_eq!(screen.row_text(1), format!("!{}", " ".repeat(cols - 1)));
    }
}

#[test]
fn selected_text_copies_wide_glyph_once() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('界');
    screen.write_char('B');
    screen.write_char('C');
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 3,
    });
    assert_eq!(result, "界BC");
}

#[test]
fn selected_text_omits_generated_wide_edge_padding() {
    let mut screen = Screen::new(2, 3);
    screen.cursor.cursor.x = 2;
    screen.write_char('界');

    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 2,
    });

    assert_eq!(result, "界");
}

#[test]
fn selected_text_fails_closed_for_malformed_wide_state() {
    let mut screen = Screen::new(1, 2);
    screen.normal.write_meaningful_cell(
        0,
        0,
        Cell {
            wide_continuation: true,
            ..Cell::default()
        },
    );

    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 1,
    });

    assert_eq!(result, "");
}

#[test]
fn selective_erase_preserves_protected_combined_wide_unit() {
    let mut screen = Screen::new(1, 5);
    screen.set_character_protection(crate::model::CharacterProtection::Protected);
    screen.write_char('界');
    screen.write_char('\u{0301}');
    screen.set_character_protection(crate::model::CharacterProtection::Unprotected);
    screen.write_char('x');
    screen.selective_erase_line(2);
    assert_eq!(screen.cell(0, 0).raw_text(), "界\u{0301}");
    assert!(screen.cell(0, 1).wide_continuation);
    assert_eq!(screen.cell(0, 2).raw_text(), " ");
}

#[test]
fn should_preserve_wide_pairs_when_selective_erase_hits_protected_cells() {
    // Arrange — protect both halves of a wide glyph, including when the selection starts on its base.
    let mut screen = Screen::new(1, 6);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    screen.cell_mut(0, 2).protected = true;
    screen.cell_mut(0, 3).protected = true;
    screen.clear_dirty();

    // Act — selective line erase touches the protected glyph through its base.
    screen.cursor.cursor.x = 2;
    screen.selective_erase_line(2);

    // Assert — protection applies to the complete glyph and the invariant remains intact.
    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);
    assert_wide_cell_invariant(&screen);
    assert!(screen.dirty_ranges().iter().any(|range| range.row == 0));
}

#[test]
fn should_move_complete_wide_pairs_when_lines_and_scroll_regions_change() {
    // Arrange — constrain vertical and horizontal movement to active margins.
    let mut screen = Screen::new(4, 8);
    screen.cursor.margins.enabled = true;
    screen.cursor.margins.left = 2;
    screen.cursor.margins.right = 5;
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;
    screen.cursor.cursor.x = 3;
    screen.cursor.cursor.y = 1;
    screen.write_char('中');
    screen.clear_dirty();

    // Act — insert/delete lines and scroll in both directions.
    screen.insert_lines(1);
    assert_wide_cell_invariant(&screen);
    assert_eq!(screen.cell(2, 3).ch, '中');
    assert!(screen.cell(2, 4).wide_continuation);

    screen.cursor.cursor.y = 2;
    screen.delete_lines(1);
    assert_wide_cell_invariant(&screen);

    screen.cursor.cursor.y = 1;
    screen.cursor.cursor.x = 3;
    screen.write_char('中');
    screen.scroll_up_region(1);
    screen.scroll_down_region(1);
    assert_wide_cell_invariant(&screen);
    assert!(screen.dirty_ranges().iter().any(|range| range.row >= 1));
}

#[test]
fn should_normalize_wide_pairs_when_dec_rectangles_touch_or_overlap() {
    // Arrange/Act/Assert — each DEC rectangle operation touches a wide glyph through one half.
    let mut parser = TerminalParser::default();

    let mut screen = Screen::new(2, 8);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4$z");
    assert_eq!(screen.cell(0, 2).ch, ' ');
    assert_eq!(screen.cell(0, 3).ch, ' ');

    let mut screen = Screen::new(2, 8);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    screen.cell_mut(0, 2).protected = true;
    screen.cell_mut(0, 3).protected = true;
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4${");
    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);

    let mut screen = Screen::new(2, 8);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    parser.put_bytes(&mut screen, b"\x1b[88;1;4;1;4$x");
    assert_eq!(screen.row_text(0), "  XX    ");
    assert_wide_cell_invariant(&screen);

    let mut screen = Screen::new(2, 8);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;1$r");
    assert!(screen.cell(0, 2).attrs.contains(CellAttrs::BOLD));
    assert!(screen.cell(0, 3).attrs.contains(CellAttrs::BOLD));
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;1$t");
    assert!(!screen.cell(0, 2).attrs.contains(CellAttrs::BOLD));
    assert!(!screen.cell(0, 3).attrs.contains(CellAttrs::BOLD));

    // A partial source copies blanks rather than creating a destination fragment.
    let mut screen = Screen::new(2, 8);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;;2;1$v");
    assert_eq!(screen.cell(1, 0).ch, ' ');
    assert!(!screen.cell(1, 1).wide_continuation);
    assert_wide_cell_invariant(&screen);

    // An overlapping complete source is snapshot-safe and keeps the pair intact.
    parser.put_bytes(&mut screen, b"\x1b[1;3;1;4;;1;4$v");
    assert_wide_cell_invariant(&screen);

    let mut screen = Screen::new(1, 8);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;;1;1$v");
    assert_eq!(screen.cell(0, 0).ch, ' ');
    assert!(!screen.cell(0, 1).wide_continuation);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn delete_chars_from_continuation_preserves_shift_source() {
    let mut screen = Screen::new(1, 8);
    screen.cursor.cursor.x = 0;
    screen.write_char('a');
    screen.cursor.cursor.x = 1;
    screen.write_char('中');
    screen.cursor.cursor.x = 3;
    screen.write_char('b');
    screen.cursor.cursor.x = 2;
    screen.clear_dirty();

    screen.delete_chars(1);

    assert_eq!(screen.cell(0, 0).ch, 'a');
    assert_eq!(screen.cell(0, 1).ch, 'b');
    assert!(!screen.cell(0, 1).wide_continuation);
    assert!(!screen.cell(0, 2).wide_continuation);
    assert_wide_cell_invariant(&screen);
    assert!(screen.dirty_ranges().iter().any(|range| range.row == 0));
}

#[test]
fn insert_chars_from_continuation_preserves_complete_glyph() {
    let mut screen = Screen::new(1, 8);
    screen.cursor.cursor.x = 0;
    screen.write_char('a');
    screen.cursor.cursor.x = 1;
    screen.write_char('中');
    screen.cursor.cursor.x = 3;
    screen.write_char('b');
    screen.cursor.cursor.x = 2;

    screen.insert_chars(1);

    assert_eq!(screen.cell(0, 0).ch, 'a');
    assert_eq!(screen.cell(0, 1).ch, ' ');
    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);
    assert_eq!(screen.cell(0, 4).ch, 'b');
    assert_wide_cell_invariant(&screen);
}

#[test]
fn insert_mode_write_on_continuation_preserves_shifted_glyph() {
    let mut screen = Screen::new(1, 8);
    screen.cursor.cursor.x = 0;
    screen.write_char('a');
    screen.cursor.cursor.x = 1;
    screen.write_char('中');
    screen.cursor.cursor.x = 3;
    screen.write_char('b');
    screen.cursor.modes.insert = true;
    screen.cursor.cursor.x = 2;

    screen.write_char('x');

    assert_eq!(screen.cell(0, 1).ch, 'x');
    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);
    assert_eq!(screen.cell(0, 4).ch, 'b');
    assert_eq!(screen.cell(0, 5).ch, ' ');
    assert_wide_cell_invariant(&screen);
}

#[test]
fn irm_off_overwrites_without_shift() {
    let mut screen = Screen::new(1, 6);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.cursor.cursor.x = 2;
    screen.cursor.modes.pending_wrap = false;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abXdef");
    assert_eq!(screen.cursor.cursor.x, 3);
}

#[test]
fn irm_wide_insert_shifts_and_preserves_pairing() {
    let mut screen = Screen::new(1, 8);
    for ch in "abcd".chars() {
        screen.write_char(ch);
    }
    screen.cursor.cursor.x = 2;
    screen.cursor.modes.insert = true;

    screen.write_char('中');

    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);
    assert_eq!(screen.cell(0, 4).ch, 'c');
    assert_eq!(screen.cell(0, 5).ch, 'd');
    assert_eq!(screen.cursor.cursor.x, 4);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn irm_insert_at_wide_base_shifts_whole_glyph() {
    let mut screen = Screen::new(1, 8);
    screen.write_char('a');
    screen.write_char('b');
    screen.write_char('中');
    screen.write_char('c');
    screen.cursor.modes.insert = true;
    screen.cursor.cursor.x = 2;

    screen.write_char('X');

    assert_eq!(screen.cell(0, 2).ch, 'X');
    assert_eq!(screen.cell(0, 3).ch, '中');
    assert!(screen.cell(0, 4).wide_continuation);
    assert_eq!(screen.cell(0, 5).ch, 'c');
    assert_eq!(screen.cursor.cursor.x, 3);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn wide_cell_edits_normalize_touched_halves_and_damage() {
    let mut screen = Screen::new(2, 6);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    screen.clear_dirty();
    screen.cursor.cursor.x = 3;
    screen.erase_line(1);

    assert_eq!(screen.cell(0, 2).ch, ' ');
    assert_eq!(screen.cell(0, 3).ch, ' ');
    assert_wide_cell_invariant(&screen);
    assert!(
        screen
            .dirty_ranges()
            .iter()
            .any(|range| range.row == 0 && range.start_col <= 2 && range.end_col > 3)
    );

    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    screen.clear_dirty();
    screen.cell_mut(0, 2).protected = true;
    screen.cell_mut(0, 3).protected = true;
    screen.cursor.cursor.x = 3;
    screen.selective_erase_line(1);
    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn wide_cell_insert_delete_and_scroll_normalize_rows() {
    let mut screen = Screen::new(3, 7);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    screen.cursor.cursor.x = 3;
    screen.insert_chars(1);
    assert_wide_cell_invariant(&screen);

    screen.cursor.cursor.x = 2;
    screen.delete_chars(1);
    assert_wide_cell_invariant(&screen);

    screen.cursor.cursor.y = 0;
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    screen.scroll_up_region(1);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn wide_cell_rectangle_operations_preserve_pairs() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(2, 6);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');

    // Attribute only selections extend from the continuation to the complete glyph.
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;1$r");
    assert!(screen.cell(0, 2).attrs.contains(CellAttrs::BOLD));
    assert!(screen.cell(0, 3).attrs.contains(CellAttrs::BOLD));
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;1$t");
    assert!(!screen.cell(0, 2).attrs.contains(CellAttrs::BOLD));
    assert!(!screen.cell(0, 3).attrs.contains(CellAttrs::BOLD));

    // Selective erase preserves a protected pair touched through its continuation.
    screen.cell_mut(0, 2).protected = true;
    screen.cell_mut(0, 3).protected = true;
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4${");
    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);
    screen.cell_mut(0, 2).protected = false;
    screen.cell_mut(0, 3).protected = false;

    // Erase and fill clean both halves when their rectangle touches one half.
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4$z");
    assert_wide_cell_invariant(&screen);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    parser.put_bytes(&mut screen, b"\x1b[88;1;4;1;4$x");
    assert_wide_cell_invariant(&screen);

    // Copying a partial source does not create a partial destination glyph.
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;;2;1$v");
    assert_wide_cell_invariant(&screen);
}

#[test]
fn deccra_rejects_partial_wide_source_and_handles_overlapping_snapshot() {
    // Arrange — create a wide source glyph and clear setup damage.
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(2, 8);
    screen.cursor.cursor.x = 2;
    screen.write_char('中');
    screen.clear_dirty();

    // Act — copy only the continuation half to the second row.
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;;2;1$v");

    // Assert — a partial source becomes a blank cell, not a fragment, and the
    // destination is still reported as damaged.
    assert_eq!(screen.cell(1, 0).ch, ' ');
    assert!(!screen.cell(1, 0).wide_continuation);
    assert!(
        screen
            .dirty_ranges()
            .iter()
            .any(|range| { range.row == 1 && range.start_col == 0 && range.end_col > 0 })
    );

    // Act — copy a complete source over an overlapping destination.
    parser.put_bytes(&mut screen, b"\x1b[1;3;1;4;;1;3$v");

    // Assert — the snapshot prevents source corruption and preserves the pair.
    assert_eq!(screen.cell(0, 2).ch, '中');
    assert!(screen.cell(0, 3).wide_continuation);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn should_mark_entered_row_when_wide_char_cannot_fit() {
    // Arrange — cursor parked at the last column, no pending wrap.
    let mut screen = Screen::new(2, 4);
    screen.set_cursor_position(1, 4); // row 0, col 3
    // Act
    screen.write_char('中');
    // Assert
    assert!(
        screen.is_wrapped(1),
        "wide char that cannot fit wraps and marks the entered row"
    );
    assert!(!screen.is_wrapped(0));
    assert!(screen.row_text(1).starts_with('中'));
}

#[test]
fn wide_glyph_wrap_rebases_the_continuation_after_partial_region_scroll() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;
    screen.cursor.cursor.y = 3;
    screen.cursor.cursor.x = 3;

    screen.write_char('界');

    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 1, 2);
    let prior = screen.normal.live_row_metadata(2);
    let wrapped = screen.normal.live_row_metadata(3);
    assert!(wrapped.soft_wrapped);
    assert_eq!(wrapped.logical_line_id, prior.logical_line_id);
    assert_narrow_reflow_prepares(&screen);
}

fn shift_chars_for_boundary_test(screen: &mut Screen, insert: bool) -> bool {
    let Screen {
        pen_state,
        normal,
        cursor,
        ..
    } = screen;
    if insert {
        CellOps::insert_chars(pen_state, normal, cursor, 1)
    } else {
        CellOps::delete_chars(pen_state, normal, cursor, 1)
    }
}

#[test]
fn boundary_shift_characterizes_malformed_wide_fragments() {
    for insert in [true, false] {
        // Orphan halves at both boundaries do not constitute crossings and are
        // normalized within the margins when the shift is accepted.
        let mut screen = Screen::new(1, 8);
        for (col, ch) in "abcdefgh".chars().enumerate() {
            screen.cell_mut(0, col).ch = ch;
        }
        screen.cursor.margins.enabled = true;
        screen.cursor.margins.left = 2;
        screen.cursor.margins.right = 5;
        screen.cursor.cursor.x = 3;
        screen.cell_mut(0, 2).wide_continuation = true;
        screen.cell_mut(0, 2).width = 0;
        screen.cell_mut(0, 5).ch = '中';
        screen.cell_mut(0, 5).width = 2;
        let before = screen_cells(&screen);
        screen.cursor.modes.pending_wrap = true;
        screen.clear_dirty();

        assert!(shift_chars_for_boundary_test(&mut screen, insert));
        assert_eq!(screen.cell(0, 2), &Cell::default());
        if !insert {
            assert_eq!(screen.cell(0, 4), &Cell::default());
        }
        for col in [0, 1, 6, 7] {
            assert_eq!(
                screen.cell(0, col),
                &before[col],
                "insert={insert}, col={col}"
            );
        }
        assert!(!screen.cursor.modes.pending_wrap);
        let damage: Vec<_> = screen
            .dirty_ranges()
            .iter()
            .map(|range| (range.row, range.start_col, range.end_col))
            .collect();
        assert_eq!(damage, vec![(0, 2, 6)]);

        // A malformed outside base that is also a continuation is still
        // recognized by the inside continuation and rejects a left crossing.
        let mut screen = Screen::new(1, 8);
        screen.cursor.margins.enabled = true;
        screen.cursor.margins.left = 2;
        screen.cursor.margins.right = 5;
        screen.cursor.cursor.x = 3;
        screen.cell_mut(0, 1).width = 2;
        screen.cell_mut(0, 1).wide_continuation = true;
        screen.cell_mut(0, 2).width = 0;
        screen.cell_mut(0, 2).wide_continuation = true;
        let before = screen_cells(&screen);
        assert!(!shift_chars_for_boundary_test(&mut screen, insert));
        assert_eq!(screen_cells(&screen), before);

        // Conversely, a right-boundary cell marked as both a base and a
        // continuation is interpreted as a continuation first. An outside
        // continuation can see it as a base, but the original in-margin scan
        // does not inspect that outside cell and therefore accepts this shift.
        let mut screen = Screen::new(1, 8);
        screen.cursor.margins.enabled = true;
        screen.cursor.margins.left = 2;
        screen.cursor.margins.right = 5;
        screen.cursor.cursor.x = 3;
        screen.cell_mut(0, 5).width = 2;
        screen.cell_mut(0, 5).wide_continuation = true;
        screen.cell_mut(0, 6).width = 0;
        screen.cell_mut(0, 6).wide_continuation = true;
        assert_eq!(CellOps::wide_range(&screen.normal, 0, 5), None);
        assert_eq!(CellOps::wide_range(&screen.normal, 0, 6), Some((5, 6)));
        let outside = screen.cell(0, 6).clone();
        assert!(shift_chars_for_boundary_test(&mut screen, insert));
        assert_eq!(screen.cell(0, 6), &outside);
    }
}

#[test]
fn boundary_shift_matches_original_scan_for_valid_and_malformed_cells() {
    // Include explicit and legacy wide bases, ordinary and malformed
    // continuations, and a cell that is simultaneously a base and a continuation.
    let cells = [
        Cell {
            ch: 'x',
            ..Cell::default()
        },
        Cell {
            ch: '中',
            ..Cell::default()
        },
        Cell {
            ch: 'x',
            width: 2,
            ..Cell::default()
        },
        Cell {
            width: 0,
            wide_continuation: true,
            ..Cell::default()
        },
        Cell {
            width: 1,
            wide_continuation: true,
            ..Cell::default()
        },
        Cell {
            width: 2,
            wide_continuation: true,
            ..Cell::default()
        },
    ];
    let mut screen = Screen::new(1, 5);
    screen.cursor.margins.enabled = true;
    for pattern in 0..cells.len().pow(5) {
        let mut indices = [0; 5];
        let mut remaining = pattern;
        for index in &mut indices {
            *index = remaining % cells.len();
            remaining /= cells.len();
        }
        for left in 0..5 {
            for right in left..5 {
                for insert in [true, false] {
                    for (col, &index) in indices.iter().enumerate() {
                        *screen.cell_mut(0, col) = cells[index].clone();
                    }
                    screen.cursor.margins.left = left;
                    screen.cursor.margins.right = right;
                    screen.cursor.cursor.x = (left + right) / 2;
                    let crossing = (left..=right).any(|col| {
                        CellOps::wide_range(&screen.normal, 0, col).is_some_and(
                            |(base, continuation)| {
                                (base < left || continuation > right)
                                    && base < right + 1
                                    && continuation >= left
                            },
                        )
                    });

                    let accepted = shift_chars_for_boundary_test(&mut screen, insert);
                    assert_eq!(
                        accepted, !crossing,
                        "pattern={indices:?}, margins={left}..={right}, insert={insert}"
                    );
                }
            }
        }
    }
}

#[test]
fn boundary_shift_accepts_fragments_on_a_single_column_screen() {
    for enabled in [true, false] {
        for insert in [true, false] {
            for (width, continuation) in [(1, false), (2, false), (0, true), (2, true)] {
                let mut screen = Screen::new(1, 1);
                screen.cursor.margins.enabled = enabled;
                screen.cell_mut(0, 0).width = width;
                screen.cell_mut(0, 0).wide_continuation = continuation;
                screen.cursor.modes.pending_wrap = true;
                screen.cursor.last_clamped_write = true;
                screen.clear_dirty();

                assert!(shift_chars_for_boundary_test(&mut screen, insert));
                assert_eq!(screen.cell(0, 0), &Cell::default());
                assert!(!screen.cursor.modes.pending_wrap);
                assert!(!screen.cursor.last_clamped_write);
                let damage: Vec<_> = screen
                    .dirty_ranges()
                    .iter()
                    .map(|range| (range.row, range.start_col, range.end_col))
                    .collect();
                assert_eq!(damage, vec![(0, 0, 1)]);
            }
        }
    }
}
