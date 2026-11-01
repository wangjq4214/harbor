use super::*;

#[test]
fn write_char_marks_its_row() {
    let mut screen = Screen::new(3, 4);
    // After new, all rows are dirty.
    screen.clear_dirty();
    screen.write_char('x');
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(dirty, vec![0], "write_char at row 0 should mark row 0");
}

#[test]
fn clear_dirty_resets_all() {
    let mut screen = Screen::new(2, 4);
    // All rows initially dirty.
    assert_eq!(screen.dirty_rows().len(), 2);
    screen.clear_dirty();
    assert_eq!(screen.dirty_rows().len(), 0);
}

#[test]
fn scroll_up_marks_all_rows() {
    let mut screen = Screen::new(3, 4);
    // Fill all rows, then scroll up via newline on bottom row.
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 2; // bottom row
    screen.newline(); // triggers scroll_up
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(dirty.len(), 3, "scroll_up should mark all rows");
}

#[test]
fn erase_line_marks_cursor_row() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.cursor.y = 2;
    screen.clear_dirty();
    screen.erase_line(2);
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(dirty, vec![2], "erase_line should mark only cursor row");
}

#[test]
fn cursor_movement_does_not_mark_dirty() {
    let mut screen = Screen::new(3, 4);
    screen.clear_dirty();
    screen.cursor_up(1);
    screen.cursor_down(1);
    screen.cursor_left(1);
    screen.cursor_right(1);
    screen.carriage_return();
    screen.set_cursor_position(2, 2);
    assert_eq!(
        screen.dirty_rows().len(),
        0,
        "cursor-only ops should not mark rows dirty"
    );
}

#[test]
fn reset_display_marks_all_rows() {
    let mut screen = Screen::new(3, 4);
    screen.clear_dirty();
    screen.reset_display();
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(dirty.len(), 3, "reset_display should mark all rows");
}

#[test]
fn reverse_index_scroll_marks_all_rows() {
    let mut screen = Screen::new(3, 4);
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 0;
    screen.reverse_index(); // triggers scroll
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(
        dirty.len(),
        3,
        "reverse_index with scroll should mark all rows"
    );
}

#[test]
fn backspace_does_not_mark_dirty() {
    let mut screen = Screen::new(1, 4);
    screen.active.cursor.cursor.x = 2;
    screen.clear_dirty();
    screen.backspace();
    assert_eq!(
        screen.dirty_rows().len(),
        0,
        "backspace should not mark rows dirty"
    );
}

#[test]
fn newline_no_scroll_does_not_mark_dirty() {
    let mut screen = Screen::new(3, 4);
    screen.active.cursor.cursor.y = 1;
    screen.clear_dirty();
    screen.newline();
    assert_eq!(
        screen.dirty_rows().len(),
        0,
        "newline without scroll should not mark dirty"
    );
}

#[test]
fn test_write_char_range_dirty() {
    use crate::DirtyRange;
    let mut screen = Screen::new(3, 10);
    screen.clear_dirty();

    // Write a character at row 0, col 0.
    screen.write_char('x');
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 2
        }]
    );

    screen.clear_dirty();
    // Move cursor to row 1, col 5.
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 5;
    screen.write_char('y');
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 1,
            start_col: 5,
            end_col: 7
        }]
    );

    // Case 1: Overwriting a wide_continuation cell (splits wide char)
    screen.clear_dirty();
    // Write a double-width char at row 1, col 4 (occupies 4 and 5)
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 4;
    screen.write_char('中'); // double-width character
    screen.clear_dirty();
    // Now write a normal char at col 5 (overwriting the continuation)
    screen.active.cursor.cursor.x = 5;
    screen.write_char('z');
    let ranges = screen.dirty_ranges();
    // Overwriting continuation cell splits the wide character at col 4, so start_col is 4
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 1,
            start_col: 4,
            end_col: 7
        }]
    );

    // Case 2: Overwriting the start of a wide character
    screen.clear_dirty();
    // Write a double-width char at row 1, col 4 (occupies 4 and 5)
    screen.active.cursor.cursor.x = 4;
    screen.write_char('中');
    screen.clear_dirty();
    // Now write a normal char at col 4 (overwriting the base cell)
    screen.active.cursor.cursor.x = 4;
    screen.write_char('w');
    let ranges = screen.dirty_ranges();
    // Overwriting base cell clears its continuation at col 5, so end_col covers col 5 (up to 6)
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 1,
            start_col: 4,
            end_col: 6
        }]
    );

    // Case 3: Clamping at end of row
    screen.clear_dirty();
    screen.active.cursor.cursor.x = 9; // last column
    screen.write_char('a');
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 1,
            start_col: 9,
            end_col: 10
        }]
    );
}

#[test]
fn test_mutations_range_dirty() {
    use crate::DirtyRange;
    let mut screen = Screen::new(3, 10);

    // 1. erase_line(0) (cursor to end)
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 0;
    screen.active.cursor.cursor.x = 4;
    screen.erase_line(0);
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 0,
            start_col: 3,
            end_col: 10
        }]
    );

    // 2. erase_chars(3) (ECH)
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 3;
    screen.erase_chars(3);
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 1,
            start_col: 2,
            end_col: 7
        }]
    );

    // 3. insert_chars(2) (ICH)
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 4;
    screen.insert_chars(2);
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 2,
            start_col: 3,
            end_col: 10
        }]
    );

    // 4. delete_chars(2) (DCH)
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 4;
    screen.delete_chars(2);
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![DirtyRange {
            row: 2,
            start_col: 3,
            end_col: 10
        }]
    );
}

#[test]
fn test_erase_display_range_dirty() {
    use crate::DirtyRange;
    let mut screen = Screen::new(5, 10);

    // 1. erase_display(0) - cursor to end of screen
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 4;
    screen.erase_display(0);
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![
            DirtyRange {
                row: 2,
                start_col: 4,
                end_col: 10
            },
            DirtyRange {
                row: 3,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 4,
                start_col: 0,
                end_col: 10
            },
        ]
    );

    // 2. erase_display(1) - start of screen to cursor
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 4;
    screen.erase_display(1);
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![
            DirtyRange {
                row: 0,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 2,
                start_col: 0,
                end_col: 5
            },
        ]
    );

    // 3. erase_display(2) - entire screen
    screen.clear_dirty();
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 4;
    screen.erase_display(2);
    let ranges = screen.dirty_ranges();
    assert_eq!(
        ranges,
        vec![
            DirtyRange {
                row: 0,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 2,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 3,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 4,
                start_col: 0,
                end_col: 10
            },
        ]
    );
}

#[test]
fn test_scroll_region_operations_damage() {
    use crate::DirtyRange;
    let mut screen = Screen::new(5, 10);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 3;

    // 1. scroll_region_up_one
    screen.clear_dirty();
    screen.scroll_region_up_one();
    assert_eq!(
        screen.dirty_ranges(),
        vec![
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 2,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 3,
                start_col: 0,
                end_col: 10
            },
        ]
    );

    // 2. reverse_index (triggers scroll down in region [1, 3] when cursor at top)
    screen.active.cursor.cursor.y = 1;
    screen.clear_dirty();
    screen.reverse_index();
    assert_eq!(
        screen.dirty_ranges(),
        vec![
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 2,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 3,
                start_col: 0,
                end_col: 10
            },
        ]
    );

    // 3. insert_lines(2) with n > 1
    screen.active.cursor.cursor.y = 1;
    screen.clear_dirty();
    screen.insert_lines(2);
    assert_eq!(
        screen.dirty_ranges(),
        vec![
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 2,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 3,
                start_col: 0,
                end_col: 10
            },
        ]
    );

    // 4. delete_lines(2) with n > 1
    screen.active.cursor.cursor.y = 1;
    screen.clear_dirty();
    screen.delete_lines(2);
    assert_eq!(
        screen.dirty_ranges(),
        vec![
            DirtyRange {
                row: 1,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 2,
                start_col: 0,
                end_col: 10
            },
            DirtyRange {
                row: 3,
                start_col: 0,
                end_col: 10
            },
        ]
    );

    // 5. cursor outside scroll region -> no-op for insert_lines / delete_lines
    screen.active.cursor.cursor.y = 0; // outside [1, 3]
    screen.clear_dirty();
    screen.insert_lines(2);
    assert!(screen.dirty_ranges().is_empty());

    screen.active.cursor.cursor.y = 0; // outside [1, 3]
    screen.clear_dirty();
    screen.delete_lines(2);
    assert!(screen.dirty_ranges().is_empty());
}

#[test]
fn test_damage_accumulation_multi_frame() {
    use crate::DirtyRange;
    let mut screen = Screen::new(3, 10);
    screen.clear_dirty();

    // Step 1: Mark a single cell range at row 0, cols [2, 3)
    screen.mark_range_dirty(0, 2, 3);
    assert_eq!(
        screen.dirty_ranges(),
        vec![DirtyRange {
            row: 0,
            start_col: 2,
            end_col: 3
        }]
    );

    // Step 2: Without clearing dirty flags, mark a range at row 0, cols [5, 6)
    screen.mark_range_dirty(0, 5, 6);
    assert_eq!(
        screen.dirty_ranges(),
        vec![
            DirtyRange {
                row: 0,
                start_col: 2,
                end_col: 3
            },
            DirtyRange {
                row: 0,
                start_col: 5,
                end_col: 6
            },
        ]
    );

    // Step 3: Mark a range at row 0, cols [3, 4), which is adjacent to [2, 3), so they merge into [2, 4)
    screen.mark_range_dirty(0, 3, 4);
    assert_eq!(
        screen.dirty_ranges(),
        vec![
            DirtyRange {
                row: 0,
                start_col: 2,
                end_col: 4
            },
            DirtyRange {
                row: 0,
                start_col: 5,
                end_col: 6
            },
        ]
    );

    // Step 4: Mark a range at row 0, cols [4, 5), bridging the gap -> they all merge into [2, 6)
    screen.mark_range_dirty(0, 4, 5);
    assert_eq!(
        screen.dirty_ranges(),
        vec![DirtyRange {
            row: 0,
            start_col: 2,
            end_col: 6
        }]
    );

    // Step 5: Clear dirty flags, verify empty
    screen.clear_dirty();
    assert!(screen.dirty_ranges().is_empty());
}
