//! Unit tests for Screen.

use super::*;
use crate::terminal::TerminalParser;

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
    screen.cursor.y = 2; // bottom row
    screen.newline(); // triggers scroll_up
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(dirty.len(), 3, "scroll_up should mark all rows");
}

#[test]
fn erase_line_marks_cursor_row() {
    let mut screen = Screen::new(4, 4);
    screen.cursor.y = 2;
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
    screen.set_cursor(2, 2);
    assert_eq!(
        screen.dirty_rows().len(),
        0,
        "cursor-only ops should not mark rows dirty"
    );
}

#[test]
fn erase_display_mode_0_marks_from_cursor_to_end() {
    let mut screen = Screen::new(5, 4);
    screen.cursor.y = 2;
    screen.clear_dirty();
    screen.erase_display(0);
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(
        dirty,
        vec![2, 3, 4],
        "erase_display(0) should mark rows cursor..end"
    );
}

#[test]
fn erase_display_mode_1_marks_from_start_to_cursor() {
    let mut screen = Screen::new(5, 4);
    screen.cursor.y = 2;
    screen.clear_dirty();
    screen.erase_display(1);
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(
        dirty,
        vec![0, 1, 2],
        "erase_display(1) should mark rows 0..=cursor"
    );
}

#[test]
fn erase_display_mode_2_marks_all() {
    let mut screen = Screen::new(5, 4);
    screen.clear_dirty();
    screen.erase_display(2);
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(dirty.len(), 5, "erase_display(2) should mark all rows");
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
    screen.cursor.y = 0;
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
    screen.cursor.x = 2;
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
    screen.cursor.y = 1;
    screen.clear_dirty();
    screen.newline();
    assert_eq!(
        screen.dirty_rows().len(),
        0,
        "newline without scroll should not mark dirty"
    );
}

#[test]
fn resize_rebuilds_dirty_all_true() {
    let mut screen = Screen::new(2, 4);
    screen.clear_dirty();
    screen.resize(4, 4);
    let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
    assert_eq!(
        dirty.len(),
        4,
        "resize should rebuild dirty_rows with all true"
    );
}

#[test]
fn resize_clamps_margins_and_updates_tab_stops() {
    let mut screen = Screen::new(2, 12);
    screen.margins.enabled = true;
    screen.margins.left = 8;
    screen.margins.right = 11;
    screen.clear_tab_stops(3);
    screen.tab_stops.0[4] = true;

    screen.resize(2, 6);
    assert_eq!((screen.margins.left, screen.margins.right), (5, 5));
    assert_eq!(
        screen.tab_stops.0,
        vec![false, false, false, false, true, false]
    );

    screen.resize(2, 18);
    assert!(
        screen.tab_stops.0[4],
        "existing tab stops must be preserved"
    );
    assert!(screen.tab_stops.0[8], "new default tab stop at column 8");
    assert!(screen.tab_stops.0[16], "new default tab stop at column 16");
}
#[test]
fn erase_chars_clears_from_cursor_to_right() {
    let mut screen = Screen::new(1, 14);
    // Write "hello world!" without wrapping.
    screen.write_char('h');
    screen.write_char('e');
    screen.write_char('l');
    screen.write_char('l');
    screen.write_char('o');
    screen.write_char(' ');
    screen.write_char('w');
    screen.write_char('o');
    screen.write_char('r');
    screen.write_char('l');
    screen.write_char('d');
    screen.write_char('!');
    assert_eq!(screen.row_text(0).trim_end(), "hello world!");

    // Move cursor back to column 5 (at ' ') and erase 7 chars.
    screen.cursor.x = 5;
    screen.clear_dirty();
    screen.erase_chars(7);
    assert_eq!(screen.row_text(0).trim_end(), "hello");
    assert_eq!(screen.dirty_rows().into_iter().collect::<Vec<_>>(), vec![0]);
}

#[test]
fn erase_chars_clamps_to_row_end() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "ab  ");

    screen.cursor.x = 2;
    screen.erase_chars(10); // more than remaining cols
    assert_eq!(screen.row_text(0), "ab  ");
}

#[test]
fn erase_chars_zero_acts_as_one() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "ab  ");

    screen.cursor.x = 1;
    screen.erase_chars(0);
    assert_eq!(screen.row_text(0), "a   ");
}

// ── ICH / DCH ──────────────────────────────────────────────

#[test]
fn insert_chars_shifts_right() {
    let mut screen = Screen::new(1, 8);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "abcdef  ");

    screen.cursor.x = 2;
    screen.clear_dirty();
    screen.insert_chars(2);
    assert_eq!(screen.row_text(0), "ab  cdef");
    assert!(screen.dirty_rows().into_iter().any(|r| r == 0));
}

#[test]
fn insert_chars_zero_acts_as_one() {
    let mut screen = Screen::new(1, 6);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "abcde ");

    screen.cursor.x = 1;
    screen.insert_chars(0);
    assert_eq!(screen.row_text(0), "a bcde");
}

#[test]
fn insert_chars_clamps_to_row_end() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "ab  ");

    screen.cursor.x = 2;
    screen.insert_chars(10);
    assert_eq!(screen.row_text(0), "ab  ");
}

#[test]
fn delete_chars_shifts_left() {
    let mut screen = Screen::new(1, 8);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "abcdef  ");

    screen.cursor.x = 2;
    screen.clear_dirty();
    screen.delete_chars(2);
    assert_eq!(screen.row_text(0), "abef    ");
    assert!(screen.dirty_rows().into_iter().any(|r| r == 0));
}

#[test]
fn delete_chars_zero_acts_as_one() {
    let mut screen = Screen::new(1, 5);
    for ch in "abcd".chars() {
        screen.write_char(ch);
    }
    screen.cursor.x = 1;
    screen.delete_chars(0);
    assert_eq!(screen.row_text(0), "acd  ");
}

#[test]
fn delete_chars_clamps_to_row_end() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "ab  ");

    screen.cursor.x = 2;
    screen.delete_chars(10);
    assert_eq!(screen.row_text(0), "ab  ");
}

// ── IL / DL ─────────────────────────────────────────────────

#[test]
fn insert_lines_within_region() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");
    assert_eq!(screen.row_text(3), "dddd");

    screen.cursor.y = 1;
    screen.insert_lines(1);
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "bbbb");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn insert_lines_outside_region_noop() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    screen.cursor.y = 0; // above scroll_top
    screen.insert_lines(1);
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn delete_lines_within_region() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");

    screen.cursor.y = 1;
    screen.delete_lines(1);
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "cccc");
    assert_eq!(screen.row_text(2), "    ");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn delete_lines_outside_region_noop() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    screen.cursor.y = 3; // below scroll_bottom
    screen.delete_lines(1);
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");
}

// ── SU / SD ─────────────────────────────────────────────────

#[test]
fn scroll_up_region_scrolls() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 0;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");
    assert_eq!(screen.row_text(3), "dddd");

    screen.scroll_up_region(2);
    assert_eq!(screen.row_text(0), "cccc");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "    ");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn scroll_down_region_scrolls() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 0;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }

    screen.scroll_down_region(2);
    assert_eq!(screen.row_text(0), "    ");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "aaaa");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn scroll_up_region_clamps_n() {
    let mut screen = Screen::new(3, 4);
    for row in 0..3 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    screen.scroll_up_region(100);
    assert_eq!(screen.row_text(0), "    ");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "    ");
}

// ── DECSTBM ─────────────────────────────────────────────────

#[test]
fn set_scroll_region_default() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 2;
    screen.scroll_region.bottom = 3;
    screen.set_scroll_region(0, 0);
    assert_eq!(screen.scroll_region.top, 0);
    assert_eq!(screen.scroll_region.bottom, 3);
    // Cursor homes on success.
    assert_eq!(screen.cursor.x, 0);
    assert_eq!(screen.cursor.y, 0);
}

#[test]
fn set_scroll_region_custom() {
    let mut screen = Screen::new(4, 4);
    screen.cursor.x = 3;
    screen.cursor.y = 3;
    screen.set_scroll_region(2, 3);
    assert_eq!(screen.scroll_region.top, 1);
    assert_eq!(screen.scroll_region.bottom, 2);
    assert_eq!(screen.cursor.x, 0);
    assert_eq!(screen.cursor.y, 0);
}

#[test]
fn set_scroll_region_invalid_ignored() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 0;
    screen.scroll_region.bottom = 3;
    screen.set_scroll_region(3, 2); // top >= bottom after clamping
    assert_eq!(screen.scroll_region.top, 0);
    assert_eq!(screen.scroll_region.bottom, 3);
}

// ── Cursor save/restore ──────────────────────────────────────

#[test]
fn save_restore_cursor_roundtrips() {
    let mut screen = Screen::new(4, 4);
    screen.save_cursor();
    screen.cursor.x = 2;
    screen.cursor.y = 3;
    screen.pen.fg = Color::Named(1);
    screen.pen.bg = Color::Named(2);
    screen.pen.attrs.set(CellAttrs::BOLD);
    screen.restore_cursor();
    assert_eq!(screen.cursor.x, 0);
    assert_eq!(screen.cursor.y, 0);
    assert_eq!(screen.pen.fg, Color::Default);
    assert_eq!(screen.pen.bg, Color::Default);
    assert_eq!(screen.pen.attrs, CellAttrs::default());
}

#[test]
fn restore_cursor_none_is_noop() {
    let mut screen = Screen::new(4, 4);
    screen.cursor.x = 2;
    screen.cursor.y = 3;
    screen.restore_cursor();
    assert_eq!(screen.cursor.x, 2);
    assert_eq!(screen.cursor.y, 3);
}

// ── Region-aware newline / reverse_index ────────────────────

#[test]
fn newline_scrolls_region() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    // Cursor at scroll_bottom, call newline.
    screen.cursor.y = 2;
    screen.cursor.x = 0;
    screen.clear_dirty();
    screen.newline();
    // Region [1,2] scrolled up: row 1 gets old row 2 ("cccc"), row 2 blanked.
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "cccc");
    assert_eq!(screen.row_text(2), "    ");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn reverse_index_scrolls_region() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    // Cursor at scroll_top, call reverse_index.
    screen.cursor.y = 1;
    screen.clear_dirty();
    screen.reverse_index();
    // Region [1,2] scrolled down: row 1 blank, row 2 gets old row 1 ("bbbb").
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "bbbb");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn reverse_index_above_region_no_panic() {
    let mut screen = Screen::new(5, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 3;
    // Cursor at (0, 0) — above the scroll region.
    screen.reverse_index(); // must NOT panic
    // No rows or cursor should have changed (no-op above region).
    assert_eq!(screen.cursor.x, 0);
    assert_eq!(screen.cursor.y, 0);
    assert_eq!(screen.row_text(0), "    ");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "    ");
    assert_eq!(screen.row_text(3), "    ");
    assert_eq!(screen.row_text(4), "    ");
}

#[test]
fn newline_below_region_no_panic() {
    let mut screen = Screen::new(4, 4);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 2;
    screen.cursor.y = 3; // below region at last row
    screen.cursor.x = 0;
    screen.newline(); // must NOT panic
    // Cursor should not have wrapped past rows-1.
    assert!(screen.cursor.y < screen.rows());
    assert_eq!(screen.cursor.y, 3);
    assert_eq!(screen.cursor.x, 0);
}

#[test]
fn resize_preserves_saved_cursor() {
    // Save at home, resize smaller, restore → clamped to new bounds.
    let mut screen = Screen::new(5, 10);
    screen.save_cursor();
    // Move cursor away and set SGR.
    screen.cursor.y = 4;
    screen.cursor.x = 7;
    screen.pen.fg = Color::Named(1);
    screen.pen.bg = Color::Named(2);
    screen.pen.attrs.set(CellAttrs::BOLD);
    screen.resize(3, 5); // smaller — saved cursor must be clamped
    screen.restore_cursor();
    assert_eq!(screen.cursor.x, 0, "saved x clamped to 0.min(4)");
    assert_eq!(screen.cursor.y, 0, "saved y clamped to 0.min(2)");
    assert_eq!(screen.pen.fg, Color::Default);
    assert_eq!(screen.pen.bg, Color::Default);
    assert_eq!(screen.pen.attrs, CellAttrs::default());

    // Save at non-home, resize larger, restore → original position preserved.
    let mut screen = Screen::new(2, 5);
    screen.cursor.y = 1;
    screen.cursor.x = 3;
    screen.save_cursor();
    screen.resize(10, 20); // larger — no clamping needed
    screen.restore_cursor();
    assert_eq!(screen.cursor.x, 3, "original x preserved");
    assert_eq!(screen.cursor.y, 1, "original y preserved");
}

// ── selected_text ──────────────────────────────────────────────

#[test]
fn selected_text_single_row() {
    let mut screen = Screen::new(3, 11);
    // Fill row 1 with "hello world" (some chars repeated to fill)
    let text: Vec<char> = "hello world".chars().collect();
    for (col, ch) in text.iter().enumerate() {
        screen.cell_mut(1, col).ch = *ch;
    }
    let result = screen.selected_text(SelectionBounds {
        start_row: 1,
        start_col: 0,
        end_row: 1,
        end_col: 10,
    });
    assert_eq!(result, "hello world");
}

#[test]
fn selected_text_multi_row() {
    let mut screen = Screen::new(3, 4);
    let rows = ["ab", "cd", "ef"];
    for (r, line) in rows.iter().enumerate() {
        for (c, ch) in line.chars().enumerate() {
            screen.cell_mut(r, c).ch = ch;
        }
    }
    // Select rows 0-1, full row 0 and partial row 1 (only col 0)
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 0,
    });
    assert_eq!(result, "ab\nc");
}

#[test]
fn selected_text_skips_wide_continuation() {
    let mut screen = Screen::new(1, 4);
    // Simulate a double-width character at col 0: set ch at 0, continuation at 1.
    screen.cell_mut(0, 0).ch = 'A';
    screen.cell_mut(0, 1).wide_continuation = true;
    screen.cell_mut(0, 2).ch = 'B';
    screen.cell_mut(0, 3).ch = 'C';
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 3,
    });
    assert_eq!(result, "ABC");
}

#[test]
fn selected_text_trims_trailing_whitespace() {
    let mut screen = Screen::new(2, 5);
    screen.cell_mut(0, 0).ch = 'a';
    screen.cell_mut(0, 1).ch = ' ';
    screen.cell_mut(0, 2).ch = ' ';
    screen.cell_mut(1, 0).ch = 'b';
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 1,
    });
    assert_eq!(result, "a\nb");
}

#[test]
fn selected_text_empty_selection() {
    let screen = Screen::new(3, 10);
    let result = screen.selected_text(SelectionBounds {
        start_row: 1,
        start_col: 2,
        end_row: 1,
        end_col: 1,
    });
    assert_eq!(result, "");
}

// ── SGR-aware erase ──────────────────────────────────────────

#[test]
fn erase_line_preserves_current_bg() {
    let mut screen = Screen::new(1, 4);
    screen.pen.bg = Color::Named(4); // blue
    screen.write_char('a');
    screen.cursor.x = 0;
    screen.erase_line(0);
    for col in 0..4 {
        assert_eq!(
            screen.cell(0, col).bg,
            Color::Named(4),
            "erase_line(0) should fill erased cells with current_bg"
        );
    }
}

#[test]
fn erase_display_mode_0_preserves_current_bg() {
    let mut screen = Screen::new(2, 3);
    screen.pen.bg = Color::Named(2); // green
    screen.cursor.y = 0;
    screen.cursor.x = 1;
    screen.erase_display(0);
    // cursor row, from cursor_x onward
    for col in 1..3 {
        assert_eq!(screen.cell(0, col).bg, Color::Named(2));
    }
    // following rows entirely
    for col in 0..3 {
        assert_eq!(screen.cell(1, col).bg, Color::Named(2));
    }
}

#[test]
fn erase_display_mode_1_preserves_current_bg() {
    let mut screen = Screen::new(2, 3);
    screen.pen.bg = Color::Rgb(64, 128, 255);
    screen.cursor.y = 1;
    screen.cursor.x = 1;
    screen.erase_display(1);
    // rows before cursor entirely
    for col in 0..3 {
        assert_eq!(screen.cell(0, col).bg, Color::Rgb(64, 128, 255));
    }
    // cursor row from start to cursor_x inclusive
    for col in 0..2 {
        assert_eq!(screen.cell(1, col).bg, Color::Rgb(64, 128, 255));
    }
}

#[test]
fn erase_display_mode_2_preserves_current_bg() {
    let mut screen = Screen::new(2, 3);
    screen.pen.bg = Color::Bright(7);
    screen.erase_display(2);
    for row in 0..2 {
        for col in 0..3 {
            assert_eq!(
                screen.cell(row, col).bg,
                Color::Bright(7),
                "erase_display(2) should fill all cells with current_bg"
            );
        }
    }
}

#[test]
fn erase_chars_preserves_current_bg() {
    let mut screen = Screen::new(1, 4);
    screen.pen.bg = Color::Named(1); // red
    screen.cursor.x = 1;
    screen.erase_chars(2);
    assert_eq!(
        screen.cell(0, 0).bg,
        Color::Default,
        "cell before cursor should be unchanged"
    );
    assert_eq!(screen.cell(0, 1).bg, Color::Named(1));
    assert_eq!(screen.cell(0, 2).bg, Color::Named(1));
    assert_eq!(
        screen.cell(0, 3).bg,
        Color::Default,
        "cell past erase range should be unchanged"
    );
}

#[test]
fn erase_uses_current_fg_too() {
    let mut screen = Screen::new(1, 3);
    screen.pen.fg = Color::Named(3); // yellow
    screen.pen.bg = Color::Named(4); // blue
    screen.pen.attrs.set(CellAttrs::BOLD);
    screen.erase_line(2);
    for col in 0..3 {
        let cell = screen.cell(0, col);
        assert_eq!(cell.fg, Color::Named(3), "erase should preserve current_fg");
        assert_eq!(cell.bg, Color::Named(4), "erase should preserve current_bg");
        assert!(
            cell.attrs.contains(CellAttrs::BOLD),
            "erase should preserve current_attrs"
        );
    }
}

#[test]
fn reset_display_uses_default_not_current_bg() {
    let mut screen = Screen::new(2, 3);
    screen.pen.bg = Color::Named(4); // blue
    screen.reset_display();
    for row in 0..2 {
        for col in 0..3 {
            assert_eq!(
                screen.cell(row, col).bg,
                Color::Default,
                "reset_display (RIS) should use default bg, not current_bg"
            );
        }
    }
}

#[test]
fn test_autowrap_and_pending_wrap() {
    let mut screen = Screen::new(3, 5);
    screen.modes.autowrap = true;

    // Write 5 characters to fill the first line (cols = 5)
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "abcde");
    assert_eq!(screen.cursor.x, 4, "cursor stays at the last column");
    assert_eq!(screen.cursor.y, 0);
    assert!(screen.modes.pending_wrap, "should enter pending wrap state");

    // Writing the 6th character should wrap to next line
    screen.write_char('f');
    assert_eq!(screen.row_text(1), "f    ");
    assert_eq!(screen.cursor.x, 1);
    assert_eq!(screen.cursor.y, 1);
    assert!(!screen.modes.pending_wrap);

    // Turn autowrap off: writing past last column should overwrite the last column
    screen.modes.autowrap = false;
    screen.cursor.x = 4;
    screen.cursor.y = 1;
    screen.write_char('x');
    assert_eq!(screen.row_text(1), "f   x");
    assert_eq!(screen.cursor.x, 4);
    screen.write_char('y');
    assert_eq!(
        screen.row_text(1),
        "f   y",
        "should overwrite last column when autowrap is off"
    );
    assert_eq!(screen.cursor.x, 4);
    assert!(!screen.modes.pending_wrap);
}

#[test]
fn test_cursor_visibility() {
    let mut screen = Screen::new(5, 5);
    assert!(screen.cursor.visible);
    screen.set_private_mode(25, false);
    assert!(!screen.cursor.visible);
    screen.set_private_mode(25, true);
    assert!(screen.cursor.visible);
}

#[test]
fn test_origin_mode_positioning() {
    let mut screen = Screen::new(5, 5);
    screen.scroll_region.top = 1;
    screen.scroll_region.bottom = 3;
    screen.margins.left = 1;
    screen.margins.right = 3;

    // Origin mode off: set_cursor uses absolute screen coordinates
    screen.modes.origin = false;
    screen.set_cursor(1, 1);
    assert_eq!(screen.cursor.y, 0);
    assert_eq!(screen.cursor.x, 0);

    // Origin mode on: set_cursor is relative to scroll region and margins
    screen.modes.origin = true;
    screen.set_cursor(1, 1); // Top-left of region/margins
    assert_eq!(screen.cursor.y, 1);
    assert_eq!(screen.cursor.x, 1);

    screen.set_cursor(2, 2);
    assert_eq!(screen.cursor.y, 2);
    assert_eq!(screen.cursor.x, 2);

    // Should clamp to the scrolling region boundaries
    screen.set_cursor(100, 100);
    assert_eq!(screen.cursor.y, 3);
    assert_eq!(screen.cursor.x, 3);
}

#[test]
fn set_scroll_region_homes_cursor_within_origin_mode() {
    let mut screen = Screen::new(6, 8);
    screen.modes.origin = true;
    screen.margins.enabled = true;
    screen.margins.left = 2;
    screen.margins.right = 6;
    screen.cursor.x = 5;
    screen.cursor.y = 5;

    screen.set_scroll_region(2, 5);

    assert_eq!((screen.cursor.x, screen.cursor.y), (2, 1));
}

#[test]
fn test_horizontal_margins() {
    let mut screen = Screen::new(5, 5);
    screen.margins.enabled = true;
    screen.margins.left = 1;
    screen.margins.right = 3;

    // Cursor movements should clamp to margins
    screen.cursor.x = 2;
    screen.cursor_left(5);
    assert_eq!(
        screen.cursor.x, 1,
        "cursor_left should clamp to margin_left"
    );

    screen.cursor.x = 2;
    screen.cursor_right(5);
    assert_eq!(
        screen.cursor.x, 3,
        "cursor_right should clamp to margin_right"
    );

    // Carriage return should go to margin_left
    screen.cursor.x = 3;
    screen.carriage_return();
    assert_eq!(
        screen.cursor.x, 1,
        "carriage_return should reset to margin_left"
    );

    // Insert/delete character should only operate within margins
    // Write "12345"
    screen.margins.enabled = false;
    screen.cursor.x = 0;
    for ch in "12345".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "12345");

    screen.margins.enabled = true;
    screen.cursor.x = 1; // pointing at '2'
    screen.insert_chars(1); // shifts cells in col 1..3 right by 1. col 4 '5' is outside margins, stays.
    assert_eq!(
        screen.row_text(0),
        "1 235",
        "should shift only within margins"
    );

    screen.cursor.x = 1;
    screen.delete_chars(1); // deletes col 1 (' '), shifts '23' left.
    assert_eq!(screen.row_text(0), "123 5");
}

#[test]
fn test_tab_stops_hts_tbc() {
    let mut screen = Screen::new(1, 20);
    // Default stops are every 8 columns: 0, 8, 16
    screen.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(screen.cursor.x, 8);

    // Set custom tab stop at col 4
    screen.cursor.x = 4;
    screen.set_tab_stop();

    screen.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(screen.cursor.x, 4, "should jump to custom tab stop");

    // Clear tab stop at col 4
    screen.cursor.x = 4;
    screen.clear_tab_stops(0);

    screen.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(screen.cursor.x, 8, "should jump over cleared tab stop");

    // Clear all tab stops
    screen.clear_tab_stops(3);
    screen.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(screen.cursor.x, 19, "should jump to the right margin limit");
}

#[test]
fn test_erase_background_filling() {
    let mut screen = Screen::new(3, 5);
    screen.pen.bg = Color::Named(4); // Blue
    screen.pen.fg = Color::Named(1); // Red
    screen.pen.attrs.set(CellAttrs::ITALIC);

    // Erase exposed cells in insert_chars
    screen.cursor.x = 0;
    screen.write_char('a');
    screen.cursor.x = 0;
    screen.insert_chars(1);
    let cell = screen.cell(0, 0);
    assert_eq!(cell.ch, ' ');
    assert_eq!(cell.bg, Color::Named(4));
    assert_eq!(cell.fg, Color::Named(1));
    assert!(cell.attrs.contains(CellAttrs::ITALIC));
}

#[test]
fn test_selective_erase_protection() {
    let mut screen = Screen::new(1, 5);

    // Write "abcde" with 'c' protected
    screen.write_char('a');
    screen.write_char('b');
    screen.pen.protected = true;
    screen.write_char('c');
    screen.pen.protected = false;
    screen.write_char('d');
    screen.write_char('e');
    assert_eq!(screen.row_text(0), "abcde");

    // Normal erase line clears everything
    screen.cursor.x = 0;
    let mut screen_copy = Screen::new(1, 5);
    for col in 0..5 {
        *screen_copy.cell_mut(0, col) = *screen.cell(0, col);
    }
    screen_copy.erase_line(2);
    assert_eq!(screen_copy.row_text(0), "     ");

    // Selective erase line clears only non-protected 'a', 'b', 'd', 'e'
    screen.selective_erase_line(2);
    assert_eq!(
        screen.row_text(0),
        "  c  ",
        "protected cell should be preserved"
    );
}

#[test]
fn test_soft_reset_decstr() {
    let mut screen = Screen::new(5, 5);
    screen.pen.bg = Color::Named(4);
    screen.pen.fg = Color::Named(1);
    screen.modes.origin = true;
    screen.modes.autowrap = false;
    screen.margins.enabled = true;
    screen.margins.left = 1;
    screen.margins.right = 3;
    screen.home_cursor(); // Set cursor to margin_left

    screen.write_char('a');
    assert_eq!(screen.row_text(0), " a   ");

    // Perform soft reset
    screen.soft_reset();

    // Modes and attributes should be reset
    assert_eq!(screen.pen.bg, Color::Default);
    assert_eq!(screen.pen.fg, Color::Default);
    assert!(!screen.modes.origin);
    assert!(screen.modes.autowrap);
    assert!(!screen.margins.enabled);
    // Screen contents should NOT be cleared
    assert_eq!(screen.row_text(0), " a   ");
}

#[test]
fn line_feed_mode_defaults_and_resets_disabled() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);
    screen.cursor.x = 3;
    screen.cursor.y = 1;

    parser.put_bytes(&mut screen, b"\n");
    assert_eq!(screen.cursor.x, 3);
    assert_eq!(screen.cursor.y, 2);

    parser.put_bytes(&mut screen, b"\x1b[20h\n");
    assert_eq!(screen.cursor.x, 0);
    assert_eq!(screen.cursor.y, 3);

    screen.cursor.x = 3;
    parser.put_bytes(&mut screen, b"\x1b[20l\n");
    assert_eq!(screen.cursor.x, 3);
    assert_eq!(screen.cursor.y, 4);

    parser.put_bytes(&mut screen, b"\x1b[20h\x1bc");
    assert!(!screen.modes.line_feed);

    parser.put_bytes(&mut screen, b"\x1b[20h\x1b[!p");
    assert!(!screen.modes.line_feed);
}

#[test]
fn index_is_independent_of_line_feed_mode() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(3, 6);
    screen.cursor.x = 4;
    screen.cursor.y = 1;

    parser.put_bytes(&mut screen, b"\x1b[20h\x1bD");

    assert!(screen.modes.line_feed);
    assert_eq!((screen.cursor.x, screen.cursor.y), (4, 2));
}

#[test]
fn scrolling_preserves_the_column_chosen_by_the_caller() {
    let mut screen = Screen::new(3, 6);
    screen.cursor.x = 4;
    screen.cursor.y = 2;
    screen.line_feed();
    assert_eq!(
        screen.cursor.x, 4,
        "LF must preserve its column while scrolling"
    );

    screen.margins.enabled = true;
    screen.margins.left = 1;
    screen.margins.right = 4;
    screen.cursor.x = 3;
    screen.cursor.y = 2;
    screen.newline();
    assert_eq!(
        screen.cursor.x, 1,
        "newline must retain the carriage-return margin while scrolling"
    );
}

#[test]
fn test_decsca_protected_attr() {
    let mut screen = Screen::new(5, 5);
    assert!(!screen.pen.protected);
    screen.set_character_protection(1);
    assert!(screen.pen.protected);
    screen.set_character_protection(0);
    assert!(!screen.pen.protected);
}

#[test]
fn test_decstr_csi_dispatch() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);
    screen.pen.bg = Color::Named(4);

    // Dispatch soft reset CSI ! p
    parser.put_bytes(&mut screen, b"\x1b[!p");
    assert_eq!(screen.pen.bg, Color::Default);
}

#[test]
fn test_decsca_csi_dispatch() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);
    assert!(!screen.pen.protected);

    // Dispatch DECSCA 1 (protected on): CSI 1 " q
    parser.put_bytes(&mut screen, b"\x1b[1\"q");
    assert!(screen.pen.protected);
}

#[test]
fn test_decslrm_csi_dispatch() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);

    // With margin_mode false, CSI s saves the cursor position
    screen.cursor.x = 3;
    screen.cursor.y = 3;
    parser.put_bytes(&mut screen, b"\x1b[s");

    screen.cursor.x = 0;
    screen.cursor.y = 0;
    parser.put_bytes(&mut screen, b"\x1b[u");
    assert_eq!(screen.cursor.x, 3);
    assert_eq!(screen.cursor.y, 3);

    // Turn margin mode on: CSI left;right s sets horizontal margins to [left-1, right-1]
    parser.put_bytes(&mut screen, b"\x1b[?69h"); // enable DECLRMM
    parser.put_bytes(&mut screen, b"\x1b[2;4s"); // set left=2, right=4 (margins: 1, 3)
    assert_eq!(screen.margins.left, 1);
    assert_eq!(screen.margins.right, 3);
}

#[test]
fn test_character_repetition_rep() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);

    // Write 'a', then repeat 3 times: CSI 3 b
    parser.put_bytes(&mut screen, b"a\x1b[3b");
    assert_eq!(screen.row_text(0), "aaaa ");

    // Empty/default parameter repeats once
    parser.put_bytes(&mut screen, b"\x1b[2;2H"); // Move cursor to (1, 1 0-based), resets pending wrap
    parser.put_bytes(&mut screen, b"b\x1b[b");
    assert_eq!(screen.row_text(1), " bb  ");
}

#[test]
fn test_rectangular_area_operations() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);

    // Fill screen with 'a'
    for _ in 0..25 {
        screen.write_char('a');
    }
    assert_eq!(screen.row_text(0), "aaaaa");

    // 1. Erase Rectangular Area (DECERA): top=2, left=2, bottom=4, right=4 (margins 1..3)
    parser.put_bytes(&mut screen, b"\x1b[2;2;4;4$z");
    assert_eq!(screen.row_text(0), "aaaaa");
    assert_eq!(screen.row_text(1), "a   a");
    assert_eq!(screen.row_text(2), "a   a");
    assert_eq!(screen.row_text(3), "a   a");
    assert_eq!(screen.row_text(4), "aaaaa");

    // 2. Fill Rectangular Area (DECFRA): fill with 'X' (ASCII 88) at top=2, left=2, bottom=3, right=3
    parser.put_bytes(&mut screen, b"\x1b[88;2;2;3;3$x");
    assert_eq!(screen.row_text(1), "aXX a");
    assert_eq!(screen.row_text(2), "aXX a");

    // 3. Copy Rectangular Area (DECCRA): copy st=2, sl=2, sb=3, sr=3 (the 'X' block) to dt=3, dl=4
    parser.put_bytes(&mut screen, b"\x1b[2;2;3;3;;3;4$v");
    assert_eq!(screen.row_text(2), "aXXXX"); // dest row 2 (0-based) col 3..4 becomes 'X'
    assert_eq!(screen.row_text(3), "a  XX"); // dest row 3 (0-based) col 3..4 becomes 'X'

    // 4. Change Attributes in Rectangular Area (DECCARA): SGR 1 (bold) at top=2, left=2, bottom=2, right=2
    parser.put_bytes(&mut screen, b"\x1b[2;2;2;2;1$r");
    assert!(screen.cell(1, 1).attrs.contains(CellAttrs::BOLD));
    assert!(!screen.cell(1, 2).attrs.contains(CellAttrs::BOLD));

    // 5. Reverse Attributes in Rectangular Area (DECRARA): SGR 1 (toggle bold) at top=2, left=2, bottom=2, right=2
    parser.put_bytes(&mut screen, b"\x1b[2;2;2;2;1$t");
    assert!(
        !screen.cell(1, 1).attrs.contains(CellAttrs::BOLD),
        "bold should be toggled off"
    );
}

fn screen_cells(screen: &Screen) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(screen.rows() * screen.cols());
    for row in 0..screen.rows() {
        for col in 0..screen.cols() {
            cells.push(*screen.cell(row, col));
        }
    }
    cells
}

#[test]
fn reversed_rectangular_ranges_are_ignored() {
    let invalid_sequences: [(&str, &[u8]); 12] = [
        ("DECERA vertical", b"\x1b[4;2;2;4$z".as_slice()),
        ("DECERA horizontal", b"\x1b[2;4;4;2$z".as_slice()),
        ("DECSERA vertical", b"\x1b[4;2;2;4${".as_slice()),
        ("DECSERA horizontal", b"\x1b[2;4;4;2${".as_slice()),
        ("DECFRA vertical", b"\x1b[88;4;2;2;4$x".as_slice()),
        ("DECFRA horizontal", b"\x1b[88;2;4;4;2$x".as_slice()),
        ("DECCRA vertical", b"\x1b[4;2;2;4;;1;1$v".as_slice()),
        ("DECCRA horizontal", b"\x1b[2;4;4;2;;1;1$v".as_slice()),
        ("DECCARA vertical", b"\x1b[4;2;2;4;1$r".as_slice()),
        ("DECCARA horizontal", b"\x1b[2;4;4;2;1$r".as_slice()),
        ("DECRARA vertical", b"\x1b[4;2;2;4;1$t".as_slice()),
        ("DECRARA horizontal", b"\x1b[2;4;4;2;1$t".as_slice()),
    ];

    for origin_mode in [false, true] {
        for (name, sequence) in invalid_sequences {
            let mut parser = TerminalParser::default();
            let mut screen = Screen::new(6, 6);
            for row in 0..screen.rows() {
                for col in 0..screen.cols() {
                    screen.cell_mut(row, col).ch =
                        char::from_u32(0x41 + (row * screen.cols() + col) as u32).unwrap();
                }
            }
            if origin_mode {
                screen.scroll_region.top = 1;
                screen.scroll_region.bottom = 4;
                screen.margins.left = 1;
                screen.margins.right = 4;
                screen.modes.origin = true;
            }
            screen.clear_dirty();
            let before = screen_cells(&screen);

            parser.put_bytes(&mut screen, sequence);

            let after = screen_cells(&screen);
            assert_eq!(after, before, "{name}, origin_mode={origin_mode}");
            assert!(
                screen.dirty_rows().is_empty(),
                "{name} dirtied rows, origin_mode={origin_mode}"
            );
        }
    }
}

#[test]
fn test_character_set_designation_and_mapping() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);

    // Designate G1 as DEC Special Graphics: ESC ) 0
    // Designate G0 as ASCII: ESC ( B
    parser.put_bytes(&mut screen, b"\x1b)0\x1b(B");

    // By default G0 is active. Write 'q' -> should be 'q' (ASCII)
    parser.put_bytes(&mut screen, b"q");
    assert_eq!(screen.row_text(0), "q    ");

    // Invoke G1 (SO / 0x0E). Write 'q' -> should be mapped to '─'
    parser.put_bytes(&mut screen, b"\x0eq");
    assert_eq!(screen.row_text(0), "q─   ");

    // Invoke G0 (SI / 0x0F). Write 'q' -> should be 'q'
    parser.put_bytes(&mut screen, b"\x0fq");
    assert_eq!(screen.row_text(0), "q─q  ");
}

#[test]
fn test_margin_autowrap_and_wide_characters() {
    let mut screen = Screen::new(5, 8);
    screen.margins.enabled = true;
    screen.margins.left = 2;
    screen.margins.right = 5;
    screen.modes.autowrap = true;

    // Move cursor to (2, 0)
    screen.cursor.x = 2;
    screen.cursor.y = 0;

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
    assert!(screen.modes.pending_wrap);

    // Write 'e' -> wraps to row 1, col 2
    screen.write_char('e');
    assert_eq!(screen.row_text(0), "  abcd  ");
    assert_eq!(screen.row_text(1), "  e     ");
}

#[test]
fn test_margin_wide_character_wrapping() {
    let mut screen = Screen::new(5, 8);
    screen.margins.enabled = true;
    screen.margins.left = 2;
    screen.margins.right = 5;
    screen.modes.autowrap = true;

    // Case A: Wide character fits when written at margin_right - 1 (col 4)
    screen.cursor.x = 4;
    screen.cursor.y = 0;
    screen.write_char('中'); // Width 2. Fits at col 4 and 5.
    assert_eq!(screen.normal.cell(0, 4).ch, '中');
    assert!(screen.normal.cell(0, 5).wide_continuation);
    assert_eq!(screen.cursor.x, 5);
    assert!(screen.modes.pending_wrap);

    // Next character 'x' should wrap to row 1, col 2
    screen.write_char('x');
    assert_eq!(screen.normal.cell(1, 2).ch, 'x');
    assert_eq!(screen.cursor.x, 3);

    // Case B: Wide character does not fit when written at margin_right (col 5)
    let mut screen2 = Screen::new(5, 8);
    screen2.margins.enabled = true;
    screen2.margins.left = 2;
    screen2.margins.right = 5;
    screen2.modes.autowrap = true;
    screen2.cursor.x = 5;
    screen2.cursor.y = 0;
    screen2.write_char('中'); // Width 2. Col 5 + 1 = 6 > 5. Wraps to row 1, col 2.
    // Check that row 0, col 5 is unchanged (empty)
    assert_eq!(screen2.normal.cell(0, 5).ch, ' ');
    assert!(!screen2.normal.cell(0, 5).wide_continuation);
    // Check that row 1, col 2 has '中'
    assert_eq!(screen2.normal.cell(1, 2).ch, '中');
    assert!(screen2.normal.cell(1, 3).wide_continuation);

    // Case C: with DECAWM disabled, a wide character that cannot fit is discarded.
    let mut screen3 = Screen::new(2, 8);
    screen3.margins.enabled = true;
    screen3.margins.left = 2;
    screen3.margins.right = 5;
    screen3.modes.autowrap = false;
    screen3.cursor.x = 5;
    screen3.write_char('中');
    assert_eq!((screen3.cursor.x, screen3.cursor.y), (5, 0));
    assert_eq!(screen3.row_text(0), "        ");
}

#[test]
fn test_margin_erase_operations() {
    let mut screen = Screen::new(5, 8);
    // Fill screen with 'x' directly
    for r in 0..5 {
        for c in 0..8 {
            let cell = screen.normal.cell_mut(r, c);
            cell.ch = 'x';
            cell.protected = false;
        }
    }
    assert_eq!(screen.row_text(0), "xxxxxxxx");

    screen.margins.enabled = true;
    screen.margins.left = 2;
    screen.margins.right = 5;

    // 1. Test erase_chars (ECH) inside margins
    screen.cursor.y = 2;
    screen.cursor.x = 3;
    screen.erase_chars(2); // Erase cols 3 and 4 on row 2
    // Row 2 should be: "xxx  xxx" (since cols 3 and 4 are erased, others stay 'x')
    assert_eq!(screen.row_text(2), "xxx  xxx");

    // 2. Test erase_line (EL) mode 1: start of line (left margin) to cursor
    screen.cursor.y = 3;
    screen.cursor.x = 3;
    screen.erase_line(1); // Erase cols 2..=3
    assert_eq!(screen.row_text(3), "xx  xxxx");

    // 3. Test erase_display (ED) mode 0: cursor to end of screen (within margins)
    screen.cursor.y = 3;
    screen.cursor.x = 3;
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
    screen.margins.enabled = true;
    screen.margins.left = 2;
    screen.margins.right = 5;

    // Fill screen with 'x' and set protection directly
    for r in 0..5 {
        for c in 0..8 {
            let cell = screen.normal.cell_mut(r, c);
            cell.ch = 'x';

            cell.protected = r == 2 && (c == 4 || c == 6);
        }
    }

    // Verify fill and protection
    assert_eq!(screen.row_text(2), "xxxxxxxx");
    assert!(screen.normal.cell(2, 4).protected);
    assert!(screen.normal.cell(2, 6).protected);

    // Test selective_erase_line(2) (erase entire line within margins, except protected)
    screen.cursor.y = 2;
    screen.cursor.x = 3;
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
    screen.margins.enabled = true;
    screen.margins.left = 1;
    screen.margins.right = 4;
    screen.cursor.x = 2;
    screen.modes.pending_wrap = false;
    screen.modes.insert = true;

    screen.write_char('X');

    assert_eq!(screen.row_text(0), "abXcdf");
    assert_eq!(screen.cursor.x, 3);
}

fn margin_scroll_fixture() -> Screen {
    let mut screen = Screen::new(4, 6);
    screen.margins.enabled = true;
    screen.margins.left = 1;
    screen.margins.right = 4;
    for row in 0..4 {
        for col in 0..6 {
            screen.normal.cell_mut(row, col).ch = char::from(b'A' + row as u8);
        }
    }
    screen
}

fn assert_margin_exterior_unchanged(screen: &Screen) {
    for row in 0..4 {
        let expected = char::from(b'A' + row as u8);
        assert_eq!(screen.normal.cell(row, 0).ch, expected);
        assert_eq!(screen.normal.cell(row, 5).ch, expected);
    }
}

#[test]
fn vertical_operations_scroll_only_within_horizontal_margins() {
    let mut screen = margin_scroll_fixture();
    screen.cursor.y = 3;
    screen.line_feed();
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.normal.cell(0, 1).ch, 'B');
    assert_eq!(screen.normal.cell(3, 1).ch, ' ');

    let mut screen = margin_scroll_fixture();
    screen.cursor.y = 0;
    screen.reverse_index();
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.normal.cell(0, 1).ch, ' ');
    assert_eq!(screen.normal.cell(1, 1).ch, 'A');

    let mut screen = margin_scroll_fixture();
    screen.cursor.y = 1;
    screen.insert_lines(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.normal.cell(1, 1).ch, ' ');
    assert_eq!(screen.normal.cell(2, 1).ch, 'B');

    let mut screen = margin_scroll_fixture();
    screen.cursor.y = 1;
    screen.delete_lines(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.normal.cell(1, 1).ch, 'C');
    assert_eq!(screen.normal.cell(3, 1).ch, ' ');

    let mut screen = margin_scroll_fixture();
    screen.scroll_up_region(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.normal.cell(0, 1).ch, 'B');
    assert_eq!(screen.normal.cell(3, 1).ch, ' ');

    let mut screen = margin_scroll_fixture();
    screen.scroll_down_region(1);
    assert_margin_exterior_unchanged(&screen);
    assert_eq!(screen.normal.cell(0, 1).ch, ' ');
    assert_eq!(screen.normal.cell(1, 1).ch, 'A');
}

#[test]
fn alt_screen_restores_all_state_groups() {
    let mut screen = Screen::new(4, 12);

    // Configure normal screen state across all groups.
    screen.set_scroll_region(2, 4);
    screen.set_private_mode(69, true); // DECLRMM on
    screen.set_left_right_margins(3, 10);
    screen.set_private_mode(1, true); // application cursor
    screen.set_private_mode(66, true); // application keypad
    screen.set_private_mode(6, true); // origin mode
    screen.set_private_mode(7, false); // autowrap off
    screen.set_private_mode(25, false); // cursor invisible
    screen.set_cursor_style(2); // block, no blink
    screen.set_standard_mode(4, true); // insert mode
    screen.set_standard_mode(20, true); // line feed mode
    screen.set_character_protection(1); // protected
    screen.set_sgr_slice(&[Some(1), Some(31), Some(42)]);
    screen.write_char('X'); // set last_char (advances cursor to 1)
    screen.cursor.x = 5;
    screen.cursor.y = 2;
    screen.set_tab_stop();
    screen.save_cursor();
    screen.designate_g0(b'0');
    screen.designate_g1(b'A');
    screen.set_active_charset(1);
    // Enter alt — all groups should be saved.
    screen.enter_alt();

    // Mutate every group in alt.
    screen.cursor.x = 0;
    screen.cursor.y = 0;
    screen.set_private_mode(69, false);
    screen.set_private_mode(1, false);
    screen.set_private_mode(66, false);
    screen.set_private_mode(6, false);
    screen.set_private_mode(7, true); // autowrap on
    screen.set_private_mode(25, true); // cursor visible
    screen.set_cursor_style(0); // bar, blink
    screen.set_standard_mode(4, false); // replace mode
    screen.set_standard_mode(20, false); // normal line feed
    screen.set_character_protection(0); // not protected
    screen.set_sgr_slice(&[Some(0)]);
    screen.set_scroll_region(0, 0);
    screen.clear_tab_stops(3);
    screen.designate_g0(b'B');
    screen.designate_g1(b'B');
    screen.set_active_charset(0);

    // Exit alt.
    screen.exit_alt();

    // Assert all state groups restored.
    // CursorState
    assert_eq!(screen.cursor.x, 5, "cursor.x restored");
    assert_eq!(screen.cursor.y, 2, "cursor.y restored");
    assert_eq!(
        screen.cursor.shape,
        CursorShape::Block,
        "cursor shape restored"
    );
    assert!(!screen.cursor.blink, "cursor blink restored");
    assert!(!screen.cursor.visible, "cursor visible restored");
    // Pen
    assert!(
        screen.pen.attrs.contains(CellAttrs::BOLD),
        "SGR bold restored"
    );
    assert_eq!(screen.pen.fg, Color::Named(1), "SGR fg restored");
    assert_eq!(screen.pen.bg, Color::Named(2), "SGR bg restored");
    assert!(screen.pen.protected, "pen protected restored");
    // ScrollRegion
    assert_eq!(screen.scroll_region.top, 1, "scroll region top restored");
    assert_eq!(
        screen.scroll_region.bottom, 3,
        "scroll region bottom restored"
    );
    // Margins
    assert!(screen.margins.enabled, "margin mode restored");
    assert_eq!(screen.margins.left, 2, "margin left restored");
    assert_eq!(screen.margins.right, 9, "margin right restored");
    // TerminalModes
    assert!(!screen.modes.autowrap, "autowrap restored");
    assert!(!screen.modes.pending_wrap, "pending wrap restored");
    assert!(screen.modes.origin, "origin mode restored");
    assert!(screen.modes.insert, "insert mode restored");
    assert!(screen.modes.line_feed, "line feed mode restored");
    assert!(
        screen.modes.application_cursor,
        "application cursor restored"
    );
    assert!(
        screen.modes.application_keypad,
        "application keypad restored"
    );
    // TabStops
    assert!(screen.tab_stops.0[5], "tab stop restored");
    // CharacterSets
    assert_eq!(screen.charsets.last_char, Some('X'), "last_char restored");
    assert_eq!(screen.charsets.g0, b'0', "g0 charset restored");
    assert_eq!(screen.charsets.g1, b'A', "g1 charset restored");
    assert_eq!(screen.charsets.active, 1, "active charset restored");
    // Move cursor away, then restore_cursor proves saved cursor survived alt
    screen.cursor.x = 0;
    screen.cursor.y = 0;
    screen.restore_cursor();
    assert_eq!(screen.cursor.x, 5, "saved cursor x restored");
    assert_eq!(screen.cursor.y, 2, "saved cursor y restored");
}
