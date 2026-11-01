use super::*;

#[test]
fn should_use_absolute_column_when_origin_is_on_but_margins_are_disabled() {
    // Arrange — saved margin bounds remain, but DECLRMM is off.
    let mut screen = Screen::new(5, 5);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 3;
    screen.active.cursor.margins.enabled = false;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 3;
    screen.active.cursor.modes.origin = true;

    // Act
    screen.set_cursor_position(1, 1);
    // Assert — row is region-relative; column ignores saved left margin.
    assert_eq!(screen.active.cursor.cursor.y, 1);
    assert_eq!(screen.active.cursor.cursor.x, 0);

    // Act
    screen.home_cursor();
    // Assert
    assert_eq!(screen.active.cursor.cursor.y, 1);
    assert_eq!(screen.active.cursor.cursor.x, 0);

    // Act
    screen.set_cursor_position(2, 3);
    // Assert
    assert_eq!(screen.active.cursor.cursor.y, 2);
    assert_eq!(screen.active.cursor.cursor.x, 2);
}

#[test]
fn test_horizontal_margins() {
    let mut screen = Screen::new(5, 5);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 3;

    // Cursor movements should clamp to margins
    screen.active.cursor.cursor.x = 2;
    screen.cursor_left(5);
    assert_eq!(
        screen.active.cursor.cursor.x, 1,
        "cursor_left should clamp to margin_left"
    );

    screen.active.cursor.cursor.x = 2;
    screen.cursor_right(5);
    assert_eq!(
        screen.active.cursor.cursor.x, 3,
        "cursor_right should clamp to margin_right"
    );

    // Carriage return should go to margin_left
    screen.active.cursor.cursor.x = 3;
    screen.carriage_return();
    assert_eq!(
        screen.active.cursor.cursor.x, 1,
        "carriage_return should reset to margin_left"
    );

    // Insert/delete character should only operate within margins
    // Write "12345"
    screen.active.cursor.margins.enabled = false;
    screen.active.cursor.cursor.x = 0;
    for ch in "12345".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "12345");

    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.cursor.x = 1; // pointing at '2'
    screen.insert_chars(1); // shifts cells in col 1..3 right by 1. col 4 '5' is outside margins, stays.
    assert_eq!(
        screen.row_text(0),
        "1 235",
        "should shift only within margins"
    );

    screen.active.cursor.cursor.x = 1;
    screen.delete_chars(1); // deletes col 1 (' '), shifts '23' left.
    assert_eq!(screen.row_text(0), "123 5");
}

#[test]
fn should_clear_pending_wrap_when_tab_reaches_right_margin() {
    let mut screen = Screen::new(1, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    assert!(screen.active.cursor.modes.pending_wrap);

    screen.horizontal_tab();
    screen.write_char('Z');

    assert_eq!(screen.active.cursor.cursor.x, 4);
    assert_eq!(screen.active.cursor.cursor.y, 0);
    assert_eq!(screen.row_text(0), "abcdZ");
    assert!(
        screen.active.cursor.modes.pending_wrap,
        "the sentinel itself reaches the right margin and starts the next pending wrap"
    );
}

#[test]
fn test_tabulation_uses_custom_stops_and_active_margins() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(1, 14);
    screen.set_private_mode(69, true);
    screen.set_left_right_margins(3, 12); // active columns 2..11
    screen.clear_tab_stops(3);

    // Stops outside the active margins must not be reachable.
    screen.active.cursor.cursor.x = 1;
    screen.set_tab_stop();
    screen.active.cursor.cursor.x = 4;
    screen.set_tab_stop();
    screen.active.cursor.cursor.x = 10;
    screen.set_tab_stop();
    screen.active.cursor.cursor.x = 12;
    screen.set_tab_stop();

    screen.active.cursor.cursor.x = 2;
    parser.put_bytes(&mut screen, b"\x1b[I");
    assert_eq!(screen.cursor_x(), 4);

    parser.put_bytes(&mut screen, b"\x1b[2I");
    assert_eq!(
        screen.cursor_x(),
        11,
        "CHT clamps at the active right margin"
    );

    parser.put_bytes(&mut screen, b"\x1b[Z");
    assert_eq!(screen.cursor_x(), 10);

    parser.put_bytes(&mut screen, b"\x1b[2Z");
    assert_eq!(screen.cursor_x(), 2, "CBT clamps at the active left margin");
}

#[test]
fn should_normalize_wide_pairs_when_edits_touch_active_margins() {
    // Arrange — place sentinels outside the active horizontal margins and a wide glyph inside.
    let mut screen = Screen::new(1, 8);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;
    screen.cell_mut(0, 2).ch = 'L';
    screen.cell_mut(0, 5).ch = 'R';
    screen.active.cursor.cursor.x = 3;
    screen.write_char('中');
    screen.clear_dirty();

    // Act — touch the continuation with EL, then each half with ECH, ICH, and DCH.
    screen.active.cursor.cursor.x = 4;
    screen.erase_line(0);
    assert_eq!(screen.cell(0, 2).ch, 'L');
    assert_eq!(screen.cell(0, 3).ch, ' ');
    assert_eq!(screen.cell(0, 4).ch, ' ');
    assert_eq!(screen.cell(0, 5).ch, ' ');
    assert!(
        screen
            .dirty_ranges()
            .iter()
            .any(|range| { range.row == 0 && range.start_col <= 3 && range.end_col >= 5 })
    );

    screen.active.cursor.cursor.x = 3;
    screen.write_char('中');
    screen.clear_dirty();
    screen.active.cursor.cursor.x = 4;
    screen.erase_chars(1);
    assert_eq!(screen.cell(0, 3).ch, ' ');
    assert_eq!(screen.cell(0, 4).ch, ' ');
    assert_wide_cell_invariant(&screen);

    screen.active.cursor.cursor.x = 3;
    screen.write_char('中');
    screen.clear_dirty();
    screen.active.cursor.cursor.x = 4;
    screen.insert_chars(1);
    assert_wide_cell_invariant(&screen);
    screen.active.cursor.cursor.x = 3;
    screen.delete_chars(1);
    assert_wide_cell_invariant(&screen);
    assert_eq!(screen.cell(0, 2).ch, 'L');
}

#[test]
fn margin_erase_and_shift_preserve_glyphs_crossing_both_boundaries() {
    let mut screen = Screen::new(1, 8);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;

    // Complete glyph crossing the left margin: columns 1/2. Construct the
    // cells directly so the test exercises the pre-existing boundary state,
    // rather than the writer's margin wrapping policy.
    screen.cell_mut(0, 1).ch = '中';
    screen.cell_mut(0, 2).ch = ' ';
    screen.cell_mut(0, 2).wide_continuation = true;
    // Complete glyph crossing the right margin: columns 5/6.
    screen.cell_mut(0, 5).ch = '中';
    screen.cell_mut(0, 6).ch = ' ';
    screen.cell_mut(0, 6).wide_continuation = true;
    let before = screen_cells(&screen);
    screen.clear_dirty();

    screen.active.cursor.cursor.x = 2;
    screen.erase_chars(1);
    screen.active.cursor.cursor.x = 2;
    screen.insert_chars(1);
    screen.active.cursor.cursor.x = 2;
    screen.delete_chars(1);
    screen.selective_erase_line(2);

    assert_eq!(screen_cells(&screen), before);
    assert_wide_cell_invariant(&screen);
    assert!(screen.dirty_ranges().iter().any(|range| range.row == 0));
}

#[test]
fn writes_at_margin_crossing_wide_pairs_leave_outside_cells_unchanged() {
    let mut screen = Screen::new(1, 8);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;

    // Pair crossing the left margin: the continuation is at the left bound.
    screen.cell_mut(0, 1).ch = '中';
    screen.cell_mut(0, 2).ch = ' ';
    screen.cell_mut(0, 2).wide_continuation = true;
    // Pair crossing the right margin: the base is at the right bound.
    screen.cell_mut(0, 5).ch = '中';
    screen.cell_mut(0, 6).ch = ' ';
    screen.cell_mut(0, 6).wide_continuation = true;
    let before_left_outside = screen.cell(0, 1).to_owned();
    let before_right_outside = screen.cell(0, 6).to_owned();

    screen.active.cursor.cursor.x = 2;
    screen.write_char('x');
    screen.active.cursor.cursor.x = 5;
    screen.write_char('y');

    assert_eq!(screen.cell(0, 1), &before_left_outside);
    assert_eq!(screen.cell(0, 6), &before_right_outside);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn insert_mode_does_not_fall_back_to_overwrite_when_margin_shift_is_unsafe() {
    let mut screen = Screen::new(1, 8);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;
    screen.cell_mut(0, 5).ch = '中';
    screen.cell_mut(0, 6).ch = ' ';
    screen.cell_mut(0, 6).wide_continuation = true;
    screen.cell_mut(0, 2).ch = 'a';
    let before = screen_cells(&screen);
    screen.active.cursor.modes.insert = true;
    screen.active.cursor.cursor.x = 2;

    screen.write_char('x');

    assert_eq!(screen_cells(&screen), before);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn irm_no_margins_insert_shifts_cells_right() {
    let mut screen = Screen::new(1, 6);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.x = 2;
    screen.active.cursor.modes.pending_wrap = false;
    screen.active.cursor.modes.insert = true;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abXcde");
    assert_eq!(screen.active.cursor.cursor.x, 3);
}

#[test]
fn irm_insert_within_margins_preserves_exteriors() {
    let mut screen = Screen::new(1, 8);
    for ch in "abcdefgh".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;
    screen.active.cursor.cursor.x = 2;
    screen.active.cursor.modes.pending_wrap = false;
    screen.active.cursor.modes.insert = true;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abXcdegh");
    assert_eq!(screen.active.cursor.cursor.x, 3);
}

#[test]
fn irm_insert_at_right_margin_discards_trailing_cell() {
    let mut screen = Screen::new(1, 6);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 4;
    screen.active.cursor.cursor.x = 4;
    screen.active.cursor.modes.pending_wrap = false;
    screen.active.cursor.modes.insert = true;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abcdXf");
    assert_eq!(screen.active.cursor.cursor.x, 4);
    assert!(screen.active.cursor.modes.pending_wrap);
}

#[test]
fn write_outside_horizontal_margins_does_not_mutate_cells() {
    let mut screen = Screen::new(1, 6);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 3;
    screen.cell_mut(0, 4).ch = 'q';
    screen.cell_mut(0, 5).ch = 'r';

    screen.active.cursor.cursor.x = 4;
    screen.write_char('x');

    assert_eq!(screen.cell(0, 4).ch, 'q');
    assert_eq!(screen.cell(0, 5).ch, 'r');
    assert_wide_cell_invariant(&screen);
}

#[test]
fn test_margin_wide_character_wrapping() {
    let mut screen = Screen::new(5, 8);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;
    screen.active.cursor.modes.autowrap = true;

    // Case A: Wide character fits when written at margin_right - 1 (col 4)
    screen.active.cursor.cursor.x = 4;
    screen.active.cursor.cursor.y = 0;
    screen.write_char('中'); // Width 2. Fits at col 4 and 5.
    assert_eq!(screen.active.normal.cell(0, 4).ch, '中');
    assert!(screen.active.normal.cell(0, 5).wide_continuation);
    assert_eq!(screen.active.cursor.cursor.x, 5);
    assert!(screen.active.cursor.modes.pending_wrap);

    // Next character 'x' should wrap to row 1, col 2
    screen.write_char('x');
    assert_eq!(screen.active.normal.cell(1, 2).ch, 'x');
    assert_eq!(screen.active.cursor.cursor.x, 3);

    // Case B: Wide character does not fit when written at margin_right (col 5)
    let mut screen2 = Screen::new(5, 8);
    screen2.active.cursor.margins.enabled = true;
    screen2.active.cursor.margins.left = 2;
    screen2.active.cursor.margins.right = 5;
    screen2.active.cursor.modes.autowrap = true;
    screen2.active.cursor.cursor.x = 5;
    screen2.active.cursor.cursor.y = 0;
    screen2.write_char('中'); // Width 2. Col 5 + 1 = 6 > 5. Wraps to row 1, col 2.
    // Check that row 0, col 5 is unchanged (empty)
    assert_eq!(screen2.active.normal.cell(0, 5).ch, ' ');
    assert!(!screen2.active.normal.cell(0, 5).wide_continuation);
    // Check that row 1, col 2 has '中'
    assert_eq!(screen2.active.normal.cell(1, 2).ch, '中');
    assert!(screen2.active.normal.cell(1, 3).wide_continuation);

    // Case C: with DECAWM disabled, a wide character that cannot fit is discarded.
    let mut screen3 = Screen::new(2, 8);
    screen3.active.cursor.margins.enabled = true;
    screen3.active.cursor.margins.left = 2;
    screen3.active.cursor.margins.right = 5;
    screen3.active.cursor.modes.autowrap = false;
    screen3.active.cursor.cursor.x = 5;
    screen3.write_char('中');
    assert_eq!(
        (
            screen3.active.cursor.cursor.x,
            screen3.active.cursor.cursor.y
        ),
        (5, 0)
    );
    assert_eq!(screen3.row_text(0), "        ");
}

#[test]
fn test_margin_erase_operations() {
    let mut screen = Screen::new(5, 8);
    // Fill screen with 'x' directly
    for r in 0..5 {
        for c in 0..8 {
            let cell = screen.active.normal.cell_mut(r, c);
            cell.ch = 'x';
            cell.protected = false;
        }
    }
    assert_eq!(screen.row_text(0), "xxxxxxxx");

    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;

    // 1. Test erase_chars (ECH) inside margins
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 3;
    screen.erase_chars(2); // Erase cols 3 and 4 on row 2
    // Row 2 should be: "xxx  xxx" (since cols 3 and 4 are erased, others stay 'x')
    assert_eq!(screen.row_text(2), "xxx  xxx");

    // 2. Test erase_line (EL) mode 1: start of line (left margin) to cursor
    screen.active.cursor.cursor.y = 3;
    screen.active.cursor.cursor.x = 3;
    screen.erase_line(1); // Erase cols 2..=3
    assert_eq!(screen.row_text(3), "xx  xxxx");

    // 3. Test erase_display (ED) mode 0: cursor to end of screen (within margins)
    screen.active.cursor.cursor.y = 3;
    screen.active.cursor.cursor.x = 3;
    screen.erase_display(0);
    // Row 3: cursor_x = 3. Erase cols 3..=5. Col 2 was already erased. So row 3 text: "xx    xx"
    assert_eq!(screen.row_text(3), "xx    xx");
    // Row 4: all cols 2..=5 erased. So row 4 text: "xx    xx"
    assert_eq!(screen.row_text(4), "xx    xx");
    // Row 1: untouched (above cursor) -> "xxxxxxxx"
    assert_eq!(screen.row_text(1), "xxxxxxxx");
}

#[test]
fn test_margin_selective_erase_operations() {
    let mut screen = Screen::new(5, 8);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;

    // Fill screen with 'x' and set protection directly
    for r in 0..5 {
        for c in 0..8 {
            let cell = screen.active.normal.cell_mut(r, c);
            cell.ch = 'x';

            cell.protected = r == 2 && (c == 4 || c == 6);
        }
    }

    // Verify fill and protection
    assert_eq!(screen.row_text(2), "xxxxxxxx");
    assert!(screen.active.normal.cell(2, 4).protected);
    assert!(screen.active.normal.cell(2, 6).protected);

    // Test selective_erase_line(2) (erase entire line within margins, except protected)
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 3;
    screen.selective_erase_line(2);
    // Columns 2, 3, 5 are unprotected within margins, so they are erased.
    // Col 4 is protected, so it remains 'x'.
    // Col 6 is outside margins, so it remains 'x' regardless.
    // Col 0, 1, 7 are outside margins, so they remain 'x'.
    // Result: "xx  x xx"
    assert_eq!(screen.row_text(2), "xx  x xx");
}

#[test]
fn insert_mode_shifts_cells_within_horizontal_margins() {
    let mut screen = Screen::new(1, 6);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 4;
    screen.active.cursor.cursor.x = 2;
    screen.active.cursor.modes.pending_wrap = false;
    screen.active.cursor.modes.insert = true;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abXcdf");
    assert_eq!(screen.active.cursor.cursor.x, 3);
}

#[test]
fn vertical_operations_scroll_only_within_horizontal_margins() {
    let mut screen = margin_scroll_fixture();
    screen.active.cursor.cursor.y = 3;
    screen.line_feed();
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.active.normal.cell(0, 1).ch, 'B');
    assert_eq!(screen.active.normal.cell(3, 1).ch, ' ');

    let mut screen = margin_scroll_fixture();
    screen.active.cursor.cursor.y = 0;
    screen.reverse_index();
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.active.normal.cell(0, 1).ch, ' ');
    assert_eq!(screen.active.normal.cell(1, 1).ch, 'A');

    let mut screen = margin_scroll_fixture();
    screen.active.cursor.cursor.y = 1;
    screen.insert_lines(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.active.normal.cell(1, 1).ch, ' ');
    assert_eq!(screen.active.normal.cell(2, 1).ch, 'B');

    let mut screen = margin_scroll_fixture();
    screen.active.cursor.cursor.y = 1;
    screen.delete_lines(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.active.normal.cell(1, 1).ch, 'C');
    assert_eq!(screen.active.normal.cell(3, 1).ch, ' ');

    let mut screen = margin_scroll_fixture();
    screen.scroll_up_region(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.active.normal.cell(0, 1).ch, 'B');
    assert_eq!(screen.active.normal.cell(3, 1).ch, ' ');

    let mut screen = margin_scroll_fixture();
    screen.scroll_down_region(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.active.normal.cell(0, 1).ch, ' ');
    assert_eq!(screen.active.normal.cell(1, 1).ch, 'A');
}

#[test]
fn margin_shifts_reject_each_wide_crossing_and_keep_damage_and_wrap_effects() {
    // The glyph at either boundary must reject the entire shift, even when the
    // cursor is elsewhere. One-column margins exercise both checks on one cell.
    for (left, right, cursor, base) in [(2, 5, 3, 1), (2, 5, 3, 5), (3, 3, 3, 2), (3, 3, 3, 3)] {
        for insert in [true, false] {
            let mut screen = Screen::new(2, 8);
            for ch in "abcdefgh".chars() {
                screen.write_char(ch);
            }
            screen.active.cursor.modes.pending_wrap = false;
            screen.active.cursor.cursor.x = base;
            screen.write_char('中');
            screen.active.cursor.margins.enabled = true;
            screen.active.cursor.margins.left = left;
            screen.active.cursor.margins.right = right;
            screen.active.cursor.cursor.x = cursor;
            screen.active.cursor.modes.pending_wrap = true;
            screen.active.cursor.last_clamped_write = true;
            screen.set_external_anchor_interest(true);
            screen.discard_anchor_mutations();
            assert!(screen.capture_edit_anchor(1, None).is_some());
            let before = screen_cells(&screen);
            screen.clear_dirty();

            if insert {
                screen.insert_chars(1);
            } else {
                screen.delete_chars(1);
            }

            let context = format!("insert={insert}, margins={left}..={right}, base={base}");
            assert_eq!(screen_cells(&screen), before, "{context}");
            assert_eq!(screen.active.cursor.cursor.x, cursor, "{context}");
            assert!(!screen.active.cursor.modes.pending_wrap, "{context}");
            assert!(!screen.active.cursor.last_clamped_write, "{context}");
            assert_eq!(
                screen.active.anchor_mutations.iter().count(),
                0,
                "{context}"
            );
            let damage: Vec<_> = screen
                .dirty_ranges()
                .iter()
                .map(|range| (range.row, range.start_col, range.end_col))
                .collect();
            assert_eq!(damage, vec![(0, cursor - 1, right + 1)], "{context}");
            assert_wide_cell_invariant(&screen);
        }
    }
}

#[test]
fn margin_shifts_allow_interior_wide_pairs_and_preserve_exterior_cells() {
    for insert in [true, false] {
        let mut screen = Screen::new(2, 9);
        for ch in "abcdefghi".chars() {
            screen.write_char(ch);
        }
        screen.active.cursor.modes.pending_wrap = false;
        screen.active.cursor.cursor.x = 3;
        screen.write_char('中');
        screen.active.cursor.margins.enabled = true;
        screen.active.cursor.margins.left = 2;
        screen.active.cursor.margins.right = 6;
        screen.active.cursor.cursor.x = 2;
        screen.active.cursor.modes.pending_wrap = true;
        screen.active.cursor.last_clamped_write = true;
        let before = screen_cells(&screen);
        let mut expected = before.clone();
        if insert {
            expected[2] = Cell::default();
            expected[3..7].clone_from_slice(&before[2..6]);
        } else {
            expected[2..6].clone_from_slice(&before[3..7]);
            expected[6] = Cell::default();
        }
        screen.clear_dirty();

        if insert {
            screen.insert_chars(1);
        } else {
            screen.delete_chars(1);
        }

        assert_eq!(screen_cells(&screen), expected, "insert={insert}");
        assert!(!screen.active.cursor.modes.pending_wrap, "insert={insert}");
        assert!(!screen.active.cursor.last_clamped_write, "insert={insert}");
        let damage: Vec<_> = screen
            .dirty_ranges()
            .iter()
            .map(|range| (range.row, range.start_col, range.end_col))
            .collect();
        assert_eq!(damage, vec![(0, 1, 7)], "insert={insert}");
        assert_wide_cell_invariant(&screen);
    }
}

#[test]
fn margin_shifts_allow_one_column_without_a_crossing() {
    for insert in [true, false] {
        let mut screen = Screen::new(1, 7);
        for ch in "abcdefg".chars() {
            screen.write_char(ch);
        }
        screen.active.cursor.margins.enabled = true;
        screen.active.cursor.margins.left = 3;
        screen.active.cursor.margins.right = 3;
        screen.active.cursor.cursor.x = 3;
        screen.active.cursor.modes.pending_wrap = true;
        screen.active.cursor.last_clamped_write = true;
        let mut expected = screen_cells(&screen);
        expected[3] = Cell::default();
        screen.clear_dirty();

        if insert {
            screen.insert_chars(0);
        } else {
            screen.delete_chars(0);
        }

        assert_eq!(screen_cells(&screen), expected, "insert={insert}");
        assert!(!screen.active.cursor.modes.pending_wrap, "insert={insert}");
        assert!(!screen.active.cursor.last_clamped_write, "insert={insert}");
        let damage: Vec<_> = screen
            .dirty_ranges()
            .iter()
            .map(|range| (range.row, range.start_col, range.end_col))
            .collect();
        assert_eq!(damage, vec![(0, 2, 4)], "insert={insert}");
    }
}

#[test]
fn margin_shifts_reject_an_outside_cursor_with_original_damage_and_wrap_effects() {
    for (cursor, damage) in [(1, vec![(0, 0, 6)]), (6, vec![(0, 5, 6)]), (7, vec![])] {
        for insert in [true, false] {
            let mut screen = Screen::new(1, 8);
            for ch in "abcdefgh".chars() {
                screen.write_char(ch);
            }
            screen.active.cursor.margins.enabled = true;
            screen.active.cursor.margins.left = 2;
            screen.active.cursor.margins.right = 5;
            screen.active.cursor.cursor.x = cursor;
            screen.active.cursor.modes.pending_wrap = true;
            screen.active.cursor.last_clamped_write = true;
            let before = screen_cells(&screen);
            screen.clear_dirty();

            if insert {
                screen.insert_chars(1);
            } else {
                screen.delete_chars(1);
            }

            assert_eq!(
                screen_cells(&screen),
                before,
                "insert={insert}, cursor={cursor}"
            );
            assert_eq!(screen.active.cursor.cursor.x, cursor);
            assert!(!screen.active.cursor.modes.pending_wrap);
            assert!(!screen.active.cursor.last_clamped_write);
            let actual_damage: Vec<_> = screen
                .dirty_ranges()
                .iter()
                .map(|range| (range.row, range.start_col, range.end_col))
                .collect();
            assert_eq!(actual_damage, damage, "insert={insert}, cursor={cursor}");
        }
    }
}
