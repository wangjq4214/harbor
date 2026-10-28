use super::*;

#[test]
fn selected_text_joins_long_colored_hyperlinked_soft_wraps() {
    let mut screen = Screen::new(3, 3);
    screen.set_sgr_slice(&[Some(1), Some(31)]);
    screen.open_hyperlink("https://example.test/not-copied".to_owned(), None);
    for ch in "abcdefg".chars() {
        screen.write_char(ch);
    }

    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 2,
        end_col: 2,
    });

    assert_eq!(result, "abcdefg");
}

#[test]
fn test_autowrap_and_pending_wrap() {
    let mut screen = Screen::new(3, 5);
    screen.cursor.modes.autowrap = true;

    // Write 5 characters to fill the first line (cols = 5)
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "abcde");
    assert_eq!(screen.cursor.cursor.x, 4, "cursor stays at the last column");
    assert_eq!(screen.cursor.cursor.y, 0);
    assert!(
        screen.cursor.modes.pending_wrap,
        "should enter pending wrap state"
    );

    // Writing the 6th character should wrap to next line
    screen.write_char('f');
    assert_eq!(screen.row_text(1), "f    ");
    assert_eq!(screen.cursor.cursor.x, 1);
    assert_eq!(screen.cursor.cursor.y, 1);
    assert!(!screen.cursor.modes.pending_wrap);

    // Turn autowrap off: writing past last column should overwrite the last column
    screen.cursor.modes.autowrap = false;
    screen.cursor.cursor.x = 4;
    screen.cursor.cursor.y = 1;
    screen.write_char('x');
    assert_eq!(screen.row_text(1), "f   x");
    assert_eq!(screen.cursor.cursor.x, 4);
    screen.write_char('y');
    assert_eq!(
        screen.row_text(1),
        "f   y",
        "should overwrite last column when autowrap is off"
    );
    assert_eq!(screen.cursor.cursor.x, 4);
    assert!(!screen.cursor.modes.pending_wrap);
}

#[test]
fn irm_with_pending_wrap_wraps_before_inserting() {
    let mut screen = Screen::new(2, 6);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    assert!(screen.cursor.modes.pending_wrap);
    screen.cursor.modes.insert = true;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abcdef");
    assert_eq!(screen.cell(1, 0).ch, 'X');
    assert_eq!(screen.cursor.cursor.y, 1);
    assert_eq!(screen.cursor.cursor.x, 1);
}

#[test]
fn irm_with_autowrap_off_at_right_margin_overwrites() {
    let mut screen = Screen::new(1, 6);
    screen.cursor.modes.autowrap = false;
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.cursor.cursor.x = 5;
    screen.cursor.modes.insert = true;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abcdeX");
    assert_eq!(screen.cursor.cursor.x, 5);
    assert!(!screen.cursor.modes.pending_wrap);
}

#[test]
fn irm_wide_char_at_right_margin_wraps_to_next_line() {
    let mut screen = Screen::new(2, 6);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.cursor.modes.insert = true;
    assert_eq!(screen.cursor.cursor.x, 5);
    assert!(!screen.cursor.modes.pending_wrap);

    screen.write_char('中');

    assert_eq!(screen.row_text(0), "abcde ");
    assert_eq!(screen.cell(1, 0).ch, '中');
    assert!(screen.cell(1, 1).wide_continuation);
    assert_eq!(screen.cursor.cursor.y, 1);
    assert_eq!(screen.cursor.cursor.x, 2);
    assert!(screen.is_wrapped(1));
    assert_wide_cell_invariant(&screen);
}

#[test]
fn irm_wide_char_at_right_margin_discarded_with_autowrap_off() {
    let mut screen = Screen::new(1, 6);
    screen.cursor.modes.autowrap = false;
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.cursor.modes.insert = true;
    let before = screen_cells(&screen);

    screen.write_char('中');

    assert_eq!(screen_cells(&screen), before);
    assert_eq!(screen.cursor.cursor.x, 5);
    assert!(!screen.cursor.modes.pending_wrap);
    assert_wide_cell_invariant(&screen);
}

#[test]
fn test_margin_autowrap_and_wide_characters() {
    let mut screen = Screen::new(5, 8);
    screen.cursor.margins.enabled = true;
    screen.cursor.margins.left = 2;
    screen.cursor.margins.right = 5;
    screen.cursor.modes.autowrap = true;

    // Move cursor to (2, 0)
    screen.cursor.cursor.x = 2;
    screen.cursor.cursor.y = 0;

    // Write 'a' -> cursor_x = 3
    screen.write_char('a');
    assert_eq!(screen.row_text(0), "  a     ");

    // Write 'b', 'c' -> cursor_x = 5
    screen.write_char('b');
    screen.write_char('c');
    assert_eq!(screen.row_text(0), "  abc   ");

    // Write 'd' -> cursor_x = 5, pending_wrap = true
    screen.write_char('d');
    assert_eq!(screen.row_text(0), "  abcd  ");
    assert!(screen.cursor.modes.pending_wrap);

    // Write 'e' -> wraps to row 1, col 2
    screen.write_char('e');
    assert_eq!(screen.row_text(0), "  abcd  ");
    assert_eq!(screen.row_text(1), "  e     ");
}

#[test]
fn should_mark_entered_row_when_autowrap_from_last_column() {
    // Arrange
    let mut screen = Screen::new(3, 5);
    // Act — fill the last column, then one more char wraps.
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    assert!(!screen.is_wrapped(0));
    screen.write_char('f');
    // Assert
    assert_eq!(screen.row_text(1), "f    ");
    assert!(screen.is_wrapped(1), "entered row must be marked wrapped");
    assert!(!screen.is_wrapped(0), "source row must not be marked");
}

#[test]
fn should_not_mark_when_autowrap_off() {
    // Arrange
    let mut screen = Screen::new(3, 5);
    screen.cursor.modes.autowrap = false;
    // Act — fill the last column, then one more char overwrites it.
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.write_char('x');
    // Assert
    assert_eq!(screen.row_text(0), "abcdx");
    assert!(!screen.is_wrapped(0));
    assert!(!screen.is_wrapped(1));
}

#[test]
fn should_clear_wrapped_marker_when_index_moves_onto_row() {
    // Arrange — wrap to row 1 so it is marked.
    let mut screen = Screen::new(3, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.write_char('f');
    assert!(screen.is_wrapped(1));
    // Move back to row 0, then IND into the marked row.
    screen.set_cursor_position(1, 1);
    // Act
    screen.index();
    // Assert
    assert!(!screen.is_wrapped(1), "IND must clear the wrap marker");
}

#[test]
fn should_preserve_wrapped_marker_when_index_is_noop_below_scroll_region() {
    // Arrange — cursor pinned at the last row, below the scroll region, with
    // that row already marked as a wrapped continuation.
    let mut screen = Screen::new(3, 5);
    screen.cursor.scroll_region.top = 0;
    screen.cursor.scroll_region.bottom = 1;
    screen.cursor.cursor.y = 2;
    screen.normal.set_wrapped(2, true);
    assert!(screen.is_wrapped(2));
    let y_before = screen.cursor.cursor.y;
    // Act — IND below the region at the last row does not move or scroll.
    screen.index();
    // Assert — a no-op index must not clear an existing wrap flag.
    assert_eq!(
        screen.cursor.cursor.y, y_before,
        "no-op index must not move"
    );
    assert!(
        screen.is_wrapped(2),
        "no-op index must preserve the wrap flag"
    );
}

#[test]
fn should_clear_wrapped_marker_when_line_feed_moves_onto_row() {
    // Arrange
    let mut screen = Screen::new(3, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.write_char('f');
    assert!(screen.is_wrapped(1));
    screen.set_cursor_position(1, 1);
    // Act
    screen.line_feed();
    // Assert
    assert!(!screen.is_wrapped(1), "LF must clear the wrap marker");
}

#[test]
fn should_clear_all_wrapped_markers_when_reset_display() {
    // Arrange — mark two rows via wrapping.
    let mut screen = Screen::new(3, 4);
    for ch in "abcd".chars() {
        screen.write_char(ch);
    }
    screen.write_char('x'); // wrap -> row 1 marked
    for ch in "yzw".chars() {
        screen.write_char(ch);
    }
    screen.write_char('q'); // wrap -> row 2 marked
    assert!(screen.is_wrapped(1));
    assert!(screen.is_wrapped(2));
    // Act
    screen.reset_display();
    // Assert
    assert!(!screen.is_wrapped(0));
    assert!(!screen.is_wrapped(1));
    assert!(!screen.is_wrapped(2));
}

#[test]
fn should_preserve_wrapped_markers_when_soft_reset() {
    // Arrange
    let mut screen = Screen::new(3, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.write_char('f');
    assert!(screen.is_wrapped(1));
    // Act
    screen.soft_reset();
    // Assert — DECSTR resets modes but leaves screen content and markers.
    assert!(screen.is_wrapped(1), "DECSTR must preserve wrap markers");
    assert!(!screen.is_wrapped(0));
    assert_eq!(screen.row_text(1), "f    ");
}

#[test]
fn insert_lines_severs_only_the_moved_soft_wrap_boundary() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;
    screen.cursor.cursor.y = 1;

    screen.insert_lines(1);

    assert!(!screen.is_wrapped(1), "inserted row must be unwrapped");
    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 2, 3);
}

#[test]
fn delete_lines_severs_only_the_moved_soft_wrap_boundary() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;
    screen.cursor.cursor.y = 1;

    screen.delete_lines(1);

    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 1, 2);
    assert!(!screen.is_wrapped(3), "blanked row must be unwrapped");
}

#[test]
fn scroll_up_region_severs_only_the_moved_soft_wrap_boundary() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;

    screen.scroll_up_region(1);

    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 1, 2);
    assert!(
        !screen.is_wrapped(3),
        "blanked bottom row must be unwrapped"
    );
}

#[test]
fn scroll_down_region_severs_only_the_moved_soft_wrap_boundary() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;

    screen.scroll_down_region(1);

    assert!(!screen.is_wrapped(1), "blanked top row must be unwrapped");
    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 2, 3);
}

#[test]
fn index_scroll_severs_the_partial_region_soft_wrap_boundary() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;
    screen.cursor.cursor.y = 3;

    screen.index();

    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 1, 2);
    assert!(!screen.is_wrapped(3), "new index row must be a hard line");
}

#[test]
fn autowrap_scroll_rebases_the_continuation_after_severing_the_region_boundary() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;
    screen.cursor.cursor.y = 3;
    screen.cursor.cursor.x = 3;
    screen.cursor.modes.pending_wrap = true;

    screen.write_char('z');

    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 1, 2);
    let prior = screen.normal.live_row_metadata(2);
    let wrapped = screen.normal.live_row_metadata(3);
    assert!(wrapped.soft_wrapped);
    assert_eq!(wrapped.logical_line_id, prior.logical_line_id);
    assert_narrow_reflow_prepares(&screen);
}

#[test]
fn reverse_index_severs_the_partial_region_soft_wrap_boundary() {
    let mut screen = screen_with_soft_line(4);
    screen.cursor.scroll_region.top = 1;
    screen.cursor.scroll_region.bottom = 3;
    screen.cursor.cursor.y = 1;

    screen.reverse_index();

    assert!(
        !screen.is_wrapped(1),
        "new reverse-index row must be unwrapped"
    );
    assert_moved_suffix_starts_hard_and_keeps_internal_wraps(&screen, 2, 3);
}

#[test]
fn should_preserve_wrapped_flags_on_margin_rect_scroll_up() {
    let mut screen = Screen::new(4, 5);
    screen.set_private_mode(69, true); // DECSLRM
    screen.set_left_right_margins(2, 4); // 0-based left=1, right=3
    screen.cursor.scroll_region.top = 0;
    screen.cursor.scroll_region.bottom = 2;
    screen.normal.set_wrapped(2, true);
    // Act
    screen.scroll_up_region(1);
    // Assert
    assert!(
        !screen.is_wrapped(1),
        "partial-row movement must preserve the destination hard-break relationship"
    );
    assert!(
        screen.is_wrapped(2),
        "partial-row blanking must preserve the destination wrap relationship"
    );
}

#[test]
fn should_clear_wrapped_flag_when_erase_line_full() {
    let mut screen = Screen::new(3, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.write_char('f'); // row 1 marked wrapped
    assert!(screen.is_wrapped(1));
    screen.set_cursor_position(2, 1); // row 1, col 0
    // Act — EL mode 2 erases the entire line.
    screen.erase_line(2);
    // Assert
    assert!(!screen.is_wrapped(1), "full-line erase must clear the flag");
}

#[test]
fn should_keep_wrapped_flag_when_erase_line_partial() {
    let mut screen = Screen::new(3, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.write_char('f'); // row 1 marked wrapped
    assert!(screen.is_wrapped(1));
    screen.set_cursor_position(2, 2); // row 1, col 1
    // Act — EL mode 0 erases from cursor to end (partial row).
    screen.erase_line(0);
    // Assert
    assert!(screen.is_wrapped(1), "partial erase must keep the flag");
}

#[test]
fn should_preserve_wrapped_flag_when_margin_erase_covers_active_region() {
    let mut screen = Screen::new(3, 5);
    screen.cursor.cursor.y = 1;
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.normal.set_wrapped(1, true);
    screen.cursor.margins.enabled = true;
    screen.cursor.margins.left = 1;
    screen.cursor.margins.right = 3;
    screen.cursor.cursor.x = 1;
    screen.cursor.modes.pending_wrap = true;

    screen.erase_line(2);

    assert_eq!(screen.row_text(1), "a   e");
    assert!(
        screen.is_wrapped(1),
        "a complete margin region is still a partial-row erase"
    );
    assert!(!screen.cursor.modes.pending_wrap);
}
