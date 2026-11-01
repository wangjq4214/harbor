use super::*;

#[test]
fn erase_display_mode_0_marks_from_cursor_to_end() {
    let mut screen = Screen::new(5, 4);
    screen.active.cursor.cursor.y = 2;
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
    screen.active.cursor.cursor.y = 2;
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
fn decaln_fills_visible_cells_homes_cursor_and_preserves_terminal_state() {
    let mut screen = Screen::new(3, 4);
    for row in 0..screen.rows() {
        for col in 0..screen.cols() {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
        screen.active.normal.set_wrapped(row, true);
    }
    screen.active.cursor.cursor.x = 3;
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.modes.pending_wrap = true;
    screen.active.cursor.modes.origin = true;
    screen.active.cursor.modes.autowrap = false;
    screen.active.cursor.modes.insert = true;
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 2;
    screen.active.cursor.cursor.visible = false;
    screen.designate_g1(b'0');
    screen.set_active_charset(1);
    screen.active.pen_state.pen.fg = Color::Named(1);
    screen.active.pen_state.pen.bg = Color::Named(4);
    screen.active.pen_state.pen.attrs.set(CellAttrs::BOLD);
    screen.save_cursor();
    let saved_cursor = screen.active.cursor.cursor.saved.as_ref().map(|saved| {
        (
            saved.cursor_x,
            saved.cursor_y,
            saved.origin_mode,
            saved.autowrap,
            saved.pending_wrap,
        )
    });
    let scroll_count = screen.scroll_count();
    let input_modes = screen.input_modes();
    let current_sgr = screen.current_sgr();
    screen.clear_dirty();

    screen.decaln();

    let expected = Cell {
        ch: 'E',
        ..Cell::default()
    };
    for row in 0..screen.rows() {
        for col in 0..screen.cols() {
            assert_eq!(screen.cell(row, col), &expected);
        }
        assert!(!screen.is_wrapped(row));
    }
    assert_eq!((screen.cursor_x(), screen.cursor_y()), (0, 0));
    assert!(!screen.pending_wrap());
    assert_eq!(screen.scroll_count(), scroll_count);
    assert_eq!(screen.input_modes(), input_modes);
    assert_eq!(screen.current_sgr(), current_sgr);
    let actual_saved_cursor = screen.active.cursor.cursor.saved.as_ref().map(|saved| {
        (
            saved.cursor_x,
            saved.cursor_y,
            saved.origin_mode,
            saved.autowrap,
            saved.pending_wrap,
        )
    });
    assert_eq!(actual_saved_cursor, saved_cursor);
    assert!(!screen.cursor_visible());
    assert_eq!(screen.dirty_rows().len(), screen.rows());

    // The character-set designation is unrelated state and remains active after DECALN.
    screen.set_cursor_position(1, 1);
    screen.write_char('q');
    assert_eq!(screen.cell(1, 1).ch, '─');
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
    screen.active.cursor.cursor.x = 5;
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

    screen.active.cursor.cursor.x = 2;
    screen.erase_chars(10); // more than remaining cols
    assert_eq!(screen.row_text(0), "ab  ");
}

#[test]
fn erase_chars_zero_acts_as_one() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "ab  ");

    screen.active.cursor.cursor.x = 1;
    screen.erase_chars(0);
    assert_eq!(screen.row_text(0), "a   ");
}

#[test]
fn insert_chars_shifts_right() {
    let mut screen = Screen::new(1, 8);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.row_text(0), "abcdef  ");

    screen.active.cursor.cursor.x = 2;
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

    screen.active.cursor.cursor.x = 1;
    screen.insert_chars(0);
    assert_eq!(screen.row_text(0), "a bcde");
}

#[test]
fn insert_chars_clamps_to_row_end() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "ab  ");

    screen.active.cursor.cursor.x = 2;
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

    screen.active.cursor.cursor.x = 2;
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
    screen.active.cursor.cursor.x = 1;
    screen.delete_chars(0);
    assert_eq!(screen.row_text(0), "acd  ");
}

#[test]
fn delete_chars_clamps_to_row_end() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "ab  ");

    screen.active.cursor.cursor.x = 2;
    screen.delete_chars(10);
    assert_eq!(screen.row_text(0), "ab  ");
}

#[test]
fn insert_lines_within_region() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");
    assert_eq!(screen.row_text(3), "dddd");

    screen.active.cursor.cursor.y = 1;
    screen.insert_lines(1);
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "bbbb");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn insert_lines_outside_region_noop() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    screen.active.cursor.cursor.y = 0; // above scroll_top
    screen.insert_lines(1);
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn delete_lines_within_region() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");

    screen.active.cursor.cursor.y = 1;
    screen.delete_lines(1);
    assert_eq!(screen.row_text(0), "aaaa");
    assert_eq!(screen.row_text(1), "cccc");
    assert_eq!(screen.row_text(2), "    ");
    assert_eq!(screen.row_text(3), "dddd");
}

#[test]
fn delete_lines_outside_region_noop() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    screen.active.cursor.cursor.y = 3; // below scroll_bottom
    screen.delete_lines(1);
    assert_eq!(screen.row_text(1), "bbbb");
    assert_eq!(screen.row_text(2), "cccc");
}

#[test]
fn scroll_up_region_scrolls() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 0;
    screen.active.cursor.scroll_region.bottom = 2;
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
    screen.active.cursor.scroll_region.top = 0;
    screen.active.cursor.scroll_region.bottom = 2;
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

#[test]
fn set_scroll_region_default() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 2;
    screen.active.cursor.scroll_region.bottom = 3;
    screen.set_scroll_region(0, 0);
    assert_eq!(screen.active.cursor.scroll_region.top, 0);
    assert_eq!(screen.active.cursor.scroll_region.bottom, 3);
    // Cursor homes on success.
    assert_eq!(screen.active.cursor.cursor.x, 0);
    assert_eq!(screen.active.cursor.cursor.y, 0);
}

#[test]
fn set_scroll_region_custom() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.cursor.x = 3;
    screen.active.cursor.cursor.y = 3;
    screen.set_scroll_region(2, 3);
    assert_eq!(screen.active.cursor.scroll_region.top, 1);
    assert_eq!(screen.active.cursor.scroll_region.bottom, 2);
    assert_eq!(screen.active.cursor.cursor.x, 0);
    assert_eq!(screen.active.cursor.cursor.y, 0);
}

#[test]
fn set_scroll_region_invalid_ignored() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 0;
    screen.active.cursor.scroll_region.bottom = 3;
    screen.set_scroll_region(3, 2); // top >= bottom after clamping
    assert_eq!(screen.active.cursor.scroll_region.top, 0);
    assert_eq!(screen.active.cursor.scroll_region.bottom, 3);
}

#[test]
fn save_restore_cursor_roundtrips() {
    let mut screen = Screen::new(4, 4);
    screen.save_cursor();
    screen.active.cursor.cursor.x = 2;
    screen.active.cursor.cursor.y = 3;
    screen.active.pen_state.pen.fg = Color::Named(1);
    screen.active.pen_state.pen.bg = Color::Named(2);
    screen.active.pen_state.pen.attrs.set(CellAttrs::BOLD);
    screen.restore_cursor();
    assert_eq!(screen.active.cursor.cursor.x, 0);
    assert_eq!(screen.active.cursor.cursor.y, 0);
    assert_eq!(screen.active.pen_state.pen.fg, Color::Default);
    assert_eq!(screen.active.pen_state.pen.bg, Color::Default);
    assert_eq!(screen.active.pen_state.pen.attrs, CellAttrs::default());
}

#[test]
fn restore_cursor_none_is_noop() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.cursor.x = 2;
    screen.active.cursor.cursor.y = 3;
    screen.restore_cursor();
    assert_eq!(screen.active.cursor.cursor.x, 2);
    assert_eq!(screen.active.cursor.cursor.y, 3);
}

#[test]
fn newline_scrolls_region() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    // Cursor at scroll_bottom, call newline.
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 0;
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
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    for row in 0..4 {
        for col in 0..4 {
            screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
        }
    }
    // Cursor at scroll_top, call reverse_index.
    screen.active.cursor.cursor.y = 1;
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
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 3;
    // Cursor at (0, 0) — above the scroll region.
    screen.reverse_index(); // must NOT panic
    // No rows or cursor should have changed (no-op above region).
    assert_eq!(screen.active.cursor.cursor.x, 0);
    assert_eq!(screen.active.cursor.cursor.y, 0);
    assert_eq!(screen.row_text(0), "    ");
    assert_eq!(screen.row_text(1), "    ");
    assert_eq!(screen.row_text(2), "    ");
    assert_eq!(screen.row_text(3), "    ");
    assert_eq!(screen.row_text(4), "    ");
}

#[test]
fn newline_below_region_no_panic() {
    let mut screen = Screen::new(4, 4);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;
    screen.active.cursor.cursor.y = 3; // below region at last row
    screen.active.cursor.cursor.x = 0;
    screen.newline(); // must NOT panic
    // Cursor should not have wrapped past rows-1.
    assert!(screen.active.cursor.cursor.y < screen.rows());
    assert_eq!(screen.active.cursor.cursor.y, 3);
    assert_eq!(screen.active.cursor.cursor.x, 0);
}

#[test]
fn erase_line_preserves_current_bg() {
    let mut screen = Screen::new(1, 4);
    screen.active.pen_state.pen.bg = Color::Named(4); // blue
    screen.write_char('a');
    screen.active.cursor.cursor.x = 0;
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
    screen.active.pen_state.pen.bg = Color::Named(2); // green
    screen.active.cursor.cursor.y = 0;
    screen.active.cursor.cursor.x = 1;
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
    screen.active.pen_state.pen.bg = Color::Rgb(64, 128, 255);
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 1;
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
    screen.active.pen_state.pen.bg = Color::Bright(7);
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
    screen.active.pen_state.pen.bg = Color::Named(1); // red
    screen.active.cursor.cursor.x = 1;
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
    screen.active.pen_state.pen.fg = Color::Named(3); // yellow
    screen.active.pen_state.pen.bg = Color::Named(4); // blue
    screen.active.pen_state.pen.attrs.set(CellAttrs::BOLD);
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
    screen.active.pen_state.pen.bg = Color::Named(4); // blue
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
fn should_clear_pending_wrap_before_next_print() {
    assert_next_print_stays_on_current_row(Screen::line_feed);
    assert_next_print_stays_on_current_row(Screen::index);
    assert_next_print_stays_on_current_row(Screen::reverse_index);
    assert_next_print_stays_on_current_row(|screen| screen.resize(4, 6));
    assert_next_print_stays_on_current_row(|screen| screen.erase_display(2));
    assert_next_print_stays_on_current_row(|screen| screen.erase_line(2));
    assert_next_print_stays_on_current_row(|screen| screen.erase_chars(1));
    assert_next_print_stays_on_current_row(|screen| screen.selective_erase_display(2));
    assert_next_print_stays_on_current_row(|screen| screen.selective_erase_line(2));
    assert_next_print_stays_on_current_row(|screen| screen.insert_chars(1));
    assert_next_print_stays_on_current_row(|screen| screen.delete_chars(1));
    assert_next_print_stays_on_current_row(|screen| screen.insert_lines(1));
    assert_next_print_stays_on_current_row(|screen| screen.delete_lines(1));
    assert_next_print_stays_on_current_row(|screen| screen.scroll_up_region(1));
    assert_next_print_stays_on_current_row(|screen| screen.scroll_down_region(1));
    assert_next_print_stays_on_current_row(Screen::horizontal_tab);
    assert_next_print_stays_on_current_row(Screen::reset_display);
    assert_next_print_stays_on_current_row(Screen::soft_reset);
}

#[test]
fn should_clear_pending_wrap_before_accepted_edit_noop_returns() {
    let mut screen = screen_with_pending_wrap();
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 3;
    screen.active.cursor.cursor.x = 0;
    screen.erase_line(0);
    assert!(!screen.active.cursor.modes.pending_wrap);

    screen.active.cursor.modes.pending_wrap = true;
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.cursor.y = 0;
    screen.insert_lines(1);
    assert!(!screen.active.cursor.modes.pending_wrap);
}

#[test]
fn should_preserve_pending_wrap_for_non_moving_c0_controls() {
    let mut screen = Screen::new(3, 5);
    let mut parser = TerminalParser::default();
    parser.put_bytes(&mut screen, b"abcde");
    assert!(screen.active.cursor.modes.pending_wrap);

    for control in *b"\x05\x07\x0e\x0f" {
        parser.put_bytes(&mut screen, &[control]);
        assert!(
            screen.active.cursor.modes.pending_wrap,
            "control {control:#04x} must not clear pending wrap"
        );
    }
}

#[test]
fn should_clear_pending_wrap_on_both_reset_paths() {
    let mut screen = screen_with_pending_wrap();
    screen.reset_display();
    assert!(!screen.active.cursor.modes.pending_wrap);

    let mut screen = screen_with_pending_wrap();
    screen.soft_reset();
    assert!(!screen.active.cursor.modes.pending_wrap);
}

#[test]
fn should_clear_markers_when_full_height_edits_blank_rows() {
    fn assert_blank_rows(operation: impl FnOnce(&mut Screen)) {
        let mut screen = Screen::new(3, 4);
        for row in 0..screen.rows() {
            screen.active.normal.set_wrapped(row, true);
        }
        operation(&mut screen);
        for row in 0..screen.rows() {
            assert!(!screen.is_wrapped(row), "row {row} must be unwrapped");
        }
    }

    assert_blank_rows(|screen| screen.insert_lines(99));
    assert_blank_rows(|screen| screen.delete_lines(99));
    assert_blank_rows(|screen| screen.scroll_up_region(99));
    assert_blank_rows(|screen| screen.scroll_down_region(99));
}

#[test]
fn test_cursor_visibility() {
    let mut screen = Screen::new(5, 5);
    assert!(screen.active.cursor.cursor.visible);
    screen.set_private_mode(25, false);
    assert!(!screen.active.cursor.cursor.visible);
    screen.set_private_mode(25, true);
    assert!(screen.active.cursor.cursor.visible);
}

#[test]
fn test_origin_mode_positioning() {
    let mut screen = Screen::new(5, 5);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 3;
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 3;

    // Origin mode off: set_cursor_position uses absolute screen coordinates
    screen.active.cursor.modes.origin = false;
    screen.set_cursor_position(1, 1);
    assert_eq!(screen.active.cursor.cursor.y, 0);
    assert_eq!(screen.active.cursor.cursor.x, 0);

    // Origin mode on: set_cursor_position is relative to scroll region and margins
    screen.active.cursor.modes.origin = true;
    screen.set_cursor_position(1, 1); // Top-left of region/margins
    assert_eq!(screen.active.cursor.cursor.y, 1);
    assert_eq!(screen.active.cursor.cursor.x, 1);

    screen.set_cursor_position(2, 2);
    assert_eq!(screen.active.cursor.cursor.y, 2);
    assert_eq!(screen.active.cursor.cursor.x, 2);

    // Should clamp to the scrolling region boundaries
    screen.set_cursor_position(100, 100);
    assert_eq!(screen.active.cursor.cursor.y, 3);
    assert_eq!(screen.active.cursor.cursor.x, 3);
}

#[test]
fn should_expose_decrqss_getters_for_sgr_regions_style_and_protection() {
    // Arrange
    let mut screen = Screen::new(10, 20);
    screen.set_sgr_slice(&[Some(1), Some(31), Some(44)]);
    screen.set_scroll_region(2, 8);
    screen.set_private_mode(69, true);
    screen.set_left_right_margins(3, 15);
    screen.set_cursor_style(crate::model::CursorStyleArg::SteadyUnderline);
    screen.set_character_protection(crate::model::CharacterProtection::Protected);

    // Act / Assert — public Screen observation surface used by DECRQSS.
    let (fg, bg, attrs, _) = screen.current_sgr();
    assert_eq!(fg, Color::Named(1));
    assert_eq!(bg, Color::Named(4));
    assert!(attrs.contains(CellAttrs::BOLD));
    assert_eq!(screen.scroll_region(), (2, 8));
    assert_eq!(screen.left_right_margins(), (3, 15));
    assert_eq!(
        screen.cursor_style(),
        crate::model::CursorStyleArg::SteadyUnderline
    );
    assert_eq!(
        screen.character_protection(),
        crate::model::CharacterProtection::Protected
    );

    // Act — disabling DECLRMM must preserve saved margins for later queries.
    screen.set_private_mode(69, false);
    // Assert
    assert_eq!(screen.left_right_margins(), (3, 15));
}

#[test]
fn set_scroll_region_homes_cursor_within_origin_mode() {
    let mut screen = Screen::new(6, 8);
    screen.active.cursor.modes.origin = true;
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 6;
    screen.active.cursor.cursor.x = 5;
    screen.active.cursor.cursor.y = 5;

    screen.set_scroll_region(2, 5);

    assert_eq!(
        (screen.active.cursor.cursor.x, screen.active.cursor.cursor.y),
        (2, 1)
    );
}

#[test]
fn test_tab_stops_hts_tbc() {
    let mut screen = Screen::new(1, 20);
    // Default stops are every 8 columns: 0, 8, 16
    screen.active.cursor.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(screen.active.cursor.cursor.x, 8);

    // Set custom tab stop at col 4
    screen.active.cursor.cursor.x = 4;
    screen.set_tab_stop();

    screen.active.cursor.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(
        screen.active.cursor.cursor.x, 4,
        "should jump to custom tab stop"
    );

    // Clear tab stop at col 4
    screen.active.cursor.cursor.x = 4;
    screen.clear_tab_stops(0);

    screen.active.cursor.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(
        screen.active.cursor.cursor.x, 8,
        "should jump over cleared tab stop"
    );

    // Clear all tab stops
    screen.clear_tab_stops(3);
    screen.active.cursor.cursor.x = 0;
    screen.horizontal_tab();
    assert_eq!(
        screen.active.cursor.cursor.x, 19,
        "should jump to the right margin limit"
    );
}

#[test]
fn test_forward_and_backward_tabulation_dispatch() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(1, 20);

    parser.put_bytes(&mut screen, b"\x1b[I");
    assert_eq!(screen.cursor_x(), 8, "omitted Ps defaults to one stop");

    parser.put_bytes(&mut screen, b"\x1b[2Z");
    assert_eq!(screen.cursor_x(), 0, "CBT traverses two stops backward");

    parser.put_bytes(&mut screen, b"\x1b[0I");
    assert_eq!(screen.cursor_x(), 8, "zero Ps is normalized to one stop");

    parser.put_bytes(&mut screen, b"\x1b[2I");
    assert_eq!(
        screen.cursor_x(),
        19,
        "CHT clamps at the physical right boundary"
    );

    parser.put_bytes(&mut screen, b"\x1b[Z");
    assert_eq!(screen.cursor_x(), 16, "CBT defaults to one stop");
}

#[test]
fn test_tabulation_is_non_destructive_and_clears_pending_wrap() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(1, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    assert!(screen.pending_wrap());
    let before = screen.row_text(0);

    parser.put_bytes(&mut screen, b"\x1b[I");

    assert_eq!(screen.row_text(0), before);
    assert!(!screen.pending_wrap());
    screen.write_char('Z');
    assert_eq!(screen.row_text(0), "abcdZ");
}

#[test]
fn should_move_back_one_tab_stop_when_cbt_parameter_is_zero() {
    // Arrange
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(1, 20);
    screen.active.cursor.cursor.x = 16;

    // Act
    parser.put_bytes(&mut screen, b"\x1b[0Z");

    // Assert
    assert_eq!(screen.cursor_x(), 8, "zero Ps must move back one tab stop");
}

#[test]
fn should_move_forward_one_tab_stop_when_direct_step_count_is_zero() {
    // Arrange
    let mut screen = Screen::new(1, 20);

    // Act
    screen.forward_tab(0);

    // Assert
    assert_eq!(screen.cursor_x(), 8);
}

#[test]
fn should_move_backward_one_tab_stop_when_direct_step_count_is_zero() {
    // Arrange
    let mut screen = Screen::new(1, 20);
    screen.active.cursor.cursor.x = 16;

    // Act
    screen.backward_tab(0);

    // Assert
    assert_eq!(screen.cursor_x(), 8);
}

#[test]
fn should_clear_pending_wrap_when_backward_tabulation_is_dispatched() {
    // Arrange
    let mut parser = TerminalParser::default();
    let mut screen = screen_with_pending_wrap();

    // Act
    parser.put_bytes(&mut screen, b"\x1b[Z");

    // Assert
    assert!(!screen.pending_wrap());
    assert_eq!(screen.cursor_x(), 0);
    screen.write_char('Z');
    assert_eq!(screen.row_text(0), "Zbcde");
}

#[test]
fn should_preserve_cell_state_when_forward_tabulation_moves_cursor() {
    // Arrange
    let mut screen = Screen::new(1, 20);
    screen.write_char('A');
    screen.cell_mut(0, 0).fg = Color::Named(1);
    screen.cell_mut(0, 0).bg = Color::Named(4);
    screen.cell_mut(0, 0).attrs.set(CellAttrs::BOLD);
    screen.cell_mut(0, 0).protected = true;
    screen.clear_dirty();
    let before: Vec<Cell> = (0..screen.cols())
        .map(|col| screen.cell(0, col).clone())
        .collect();

    // Act
    screen.forward_tab(1);

    // Assert
    let after: Vec<Cell> = (0..screen.cols())
        .map(|col| screen.cell(0, col).clone())
        .collect();
    assert_eq!(screen.cursor_x(), 8);
    assert_eq!(after, before);
    assert!(screen.dirty_rows().is_empty());
}

#[test]
fn should_preserve_cell_state_when_backward_tabulation_moves_cursor() {
    // Arrange
    let mut screen = Screen::new(1, 20);
    screen.cell_mut(0, 16).ch = 'A';
    screen.active.cursor.cursor.x = 16;
    screen.cell_mut(0, 16).fg = Color::Named(1);
    screen.cell_mut(0, 16).bg = Color::Named(4);
    screen.cell_mut(0, 16).attrs.set(CellAttrs::BOLD);
    screen.cell_mut(0, 16).protected = true;
    screen.clear_dirty();
    let before: Vec<Cell> = (0..screen.cols())
        .map(|col| screen.cell(0, col).clone())
        .collect();

    // Act
    screen.backward_tab(1);

    // Assert
    let after: Vec<Cell> = (0..screen.cols())
        .map(|col| screen.cell(0, col).clone())
        .collect();
    assert_eq!(screen.cursor_x(), 8);
    assert_eq!(after, before);
    assert!(screen.dirty_rows().is_empty());
}

#[test]
fn test_erase_background_filling() {
    let mut screen = Screen::new(3, 5);
    screen.active.pen_state.pen.bg = Color::Named(4); // Blue
    screen.active.pen_state.pen.fg = Color::Named(1); // Red
    screen.active.pen_state.pen.attrs.set(CellAttrs::ITALIC);

    // Erase exposed cells in insert_chars
    screen.active.cursor.cursor.x = 0;
    screen.write_char('a');
    screen.active.cursor.cursor.x = 0;
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
    screen.active.pen_state.pen.protected = true;
    screen.write_char('c');
    screen.active.pen_state.pen.protected = false;
    screen.write_char('d');
    screen.write_char('e');
    assert_eq!(screen.row_text(0), "abcde");

    // Normal erase line clears everything
    screen.active.cursor.cursor.x = 0;
    let mut screen_copy = Screen::new(1, 5);
    for col in 0..5 {
        *screen_copy.cell_mut(0, col) = screen.cell(0, col).clone();
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
    screen.active.pen_state.pen.bg = Color::Named(4);
    screen.active.pen_state.pen.fg = Color::Named(1);
    screen.active.cursor.modes.origin = true;
    screen.active.cursor.modes.autowrap = false;
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 3;
    screen.home_cursor(); // Set cursor to margin_left

    screen.write_char('a');
    assert_eq!(screen.row_text(0), " a   ");

    // Perform soft reset
    screen.soft_reset();

    // Modes and attributes should be reset
    assert_eq!(screen.active.pen_state.pen.bg, Color::Default);
    assert_eq!(screen.active.pen_state.pen.fg, Color::Default);
    assert!(!screen.active.cursor.modes.origin);
    assert!(screen.active.cursor.modes.autowrap);
    assert!(!screen.active.cursor.margins.enabled);
    // Screen contents should NOT be cleared
    assert_eq!(screen.row_text(0), " a   ");
}

#[test]
fn line_feed_mode_defaults_and_resets_disabled() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);
    screen.active.cursor.cursor.x = 3;
    screen.active.cursor.cursor.y = 1;

    parser.put_bytes(&mut screen, b"\n");
    assert_eq!(screen.active.cursor.cursor.x, 3);
    assert_eq!(screen.active.cursor.cursor.y, 2);

    parser.put_bytes(&mut screen, b"\x1b[20h\n");
    assert_eq!(screen.active.cursor.cursor.x, 0);
    assert_eq!(screen.active.cursor.cursor.y, 3);

    screen.active.cursor.cursor.x = 3;
    parser.put_bytes(&mut screen, b"\x1b[20l\n");
    assert_eq!(screen.active.cursor.cursor.x, 3);
    assert_eq!(screen.active.cursor.cursor.y, 4);

    parser.put_bytes(&mut screen, b"\x1b[20h\x1bc");
    assert!(!screen.active.cursor.modes.line_feed);

    parser.put_bytes(&mut screen, b"\x1b[20h\x1b[!p");
    assert!(!screen.active.cursor.modes.line_feed);
}

#[test]
fn index_is_independent_of_line_feed_mode() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(3, 6);
    screen.active.cursor.cursor.x = 4;
    screen.active.cursor.cursor.y = 1;

    parser.put_bytes(&mut screen, b"\x1b[20h\x1bD");

    assert!(screen.active.cursor.modes.line_feed);
    assert_eq!(
        (screen.active.cursor.cursor.x, screen.active.cursor.cursor.y),
        (4, 2)
    );
}

#[test]
fn scrolling_preserves_the_column_chosen_by_the_caller() {
    let mut screen = Screen::new(3, 6);
    screen.active.cursor.cursor.x = 4;
    screen.active.cursor.cursor.y = 2;
    screen.line_feed();
    assert_eq!(
        screen.active.cursor.cursor.x, 4,
        "LF must preserve its column while scrolling"
    );

    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 1;
    screen.active.cursor.margins.right = 4;
    screen.active.cursor.cursor.x = 3;
    screen.active.cursor.cursor.y = 2;
    screen.newline();
    assert_eq!(
        screen.active.cursor.cursor.x, 1,
        "newline must retain the carriage-return margin while scrolling"
    );
}

#[test]
fn test_decsca_protected_attr() {
    let mut screen = Screen::new(5, 5);
    assert!(!screen.active.pen_state.pen.protected);
    screen.set_character_protection(crate::model::CharacterProtection::Protected);
    assert!(screen.active.pen_state.pen.protected);
    screen.set_character_protection(crate::model::CharacterProtection::Unprotected);
    assert!(!screen.active.pen_state.pen.protected);
}

#[test]
fn test_decstr_csi_dispatch() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);
    screen.active.pen_state.pen.bg = Color::Named(4);

    // Dispatch soft reset CSI ! p
    parser.put_bytes(&mut screen, b"\x1b[!p");
    assert_eq!(screen.active.pen_state.pen.bg, Color::Default);
}

#[test]
fn test_decsca_csi_dispatch() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);
    assert!(!screen.active.pen_state.pen.protected);

    // Dispatch DECSCA 1 (protected on): CSI 1 " q
    parser.put_bytes(&mut screen, b"\x1b[1\"q");
    assert!(screen.active.pen_state.pen.protected);
}

#[test]
fn test_decslrm_csi_dispatch() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(5, 5);

    // With margin_mode false, CSI s saves the cursor position
    screen.active.cursor.cursor.x = 3;
    screen.active.cursor.cursor.y = 3;
    parser.put_bytes(&mut screen, b"\x1b[s");

    screen.active.cursor.cursor.x = 0;
    screen.active.cursor.cursor.y = 0;
    parser.put_bytes(&mut screen, b"\x1b[u");
    assert_eq!(screen.active.cursor.cursor.x, 3);
    assert_eq!(screen.active.cursor.cursor.y, 3);

    // Turn margin mode on: CSI left;right s sets horizontal margins to [left-1, right-1]
    parser.put_bytes(&mut screen, b"\x1b[?69h"); // enable DECLRMM
    parser.put_bytes(&mut screen, b"\x1b[2;4s"); // set left=2, right=4 (margins: 1, 3)
    assert_eq!(screen.active.cursor.margins.left, 1);
    assert_eq!(screen.active.cursor.margins.right, 3);
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

#[test]
fn attribute_rectangles_preserve_explicit_blanks_and_recompute_style_only_blanks() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(1, 4);
    screen.write_char(' ');

    parser.put_bytes(&mut screen, b"\x1b[1;1;1;2;41$r");
    assert!(screen.active.normal.cell_is_meaningful(0, 0));
    assert!(screen.active.normal.cell_is_meaningful(0, 1));
    parser.put_bytes(&mut screen, b"\x1b[1;1;1;2;49$r");
    assert!(
        screen.active.normal.cell_is_meaningful(0, 0),
        "printed default blank must retain explicit provenance"
    );
    assert!(
        !screen.active.normal.cell_is_meaningful(0, 1),
        "unused blank must lose style-only meaning with its background"
    );

    parser.put_bytes(&mut screen, b"\x1b[1;3;1;3;7$r");
    assert!(screen.active.normal.cell_is_meaningful(0, 2));
    parser.put_bytes(&mut screen, b"\x1b[1;3;1;3;27$r");
    assert!(!screen.active.normal.cell_is_meaningful(0, 2));

    parser.put_bytes(&mut screen, b"\x1b[1;1;1;1;;1;4$v");
    parser.put_bytes(&mut screen, b"\x1b[1;4;1;4;49$r");
    assert!(
        screen.active.normal.cell_is_meaningful(0, 3),
        "DECCRA must copy explicit provenance with a printed blank"
    );
}

#[test]
fn deccra_destination_matches_origin_bounds() {
    let mut parser = TerminalParser::default();
    let mut screen = Screen::new(4, 8);
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 2;
    screen.active.cursor.margins.right = 5;
    for col in 0..8 {
        screen.cell_mut(0, col).ch = char::from(b'a' + col as u8);
        screen.cell_mut(1, col).ch = char::from(b'A' + col as u8);
        screen.cell_mut(2, col).ch = char::from(b'0' + col as u8);
        screen.cell_mut(3, col).ch = char::from(b'!' + col as u8);
    }
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 2;

    // With origin disabled, destination coordinates are absolute and use the
    // full screen. This valid destination is outside the scroll region and
    // also outside the horizontal margins, so it must still be copied.
    parser.put_bytes(&mut screen, b"\x1b[1;3;1;3;;1;6$v");
    assert_eq!(screen.cell(0, 5).ch, 'c');
    assert_eq!(screen.cell(0, 6).ch, 'g');

    // With origin enabled, destination coordinates are relative to the active
    // scroll region and margins, and a destination below that region clips.
    screen.active.cursor.modes.origin = true;
    let before = screen_cells(&screen);
    parser.put_bytes(&mut screen, b"\x1b[1;3;1;3;;3;3$v");
    assert_eq!(screen_cells(&screen), before);
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
                screen.active.cursor.scroll_region.top = 1;
                screen.active.cursor.scroll_region.bottom = 4;
                screen.active.cursor.margins.left = 1;
                screen.active.cursor.margins.right = 4;
                screen.active.cursor.modes.origin = true;
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
fn should_use_full_width_for_rectangular_ops_after_declrmm_disabled() {
    let mut parser = crate::parser::TerminalParser::default();
    let mut screen = Screen::new(5, 5);

    for _ in 0..25 {
        screen.write_char('a');
    }
    // Establish custom margins, then disable DECLRMM while keeping DECOM.
    parser.put_bytes(&mut screen, b"\x1b[?69h\x1b[2;4s\x1b[?6h\x1b[?69l");

    // Default DECERA under origin mode must erase the full width of the scroll region.
    parser.put_bytes(&mut screen, b"\x1b[1;1;5;5$z");
    assert_eq!(screen.row_text(0), "     ");
    assert_eq!(screen.row_text(1), "     ");
    assert_eq!(screen.row_text(2), "     ");
    assert_eq!(screen.row_text(3), "     ");
    assert_eq!(screen.row_text(4), "     ");

    // Saved margin bounds remain queryable for DECRQSS despite DECLRMM being off.
    assert_eq!(screen.left_right_margins(), (2, 4));
}

#[test]
fn private_mode_1048_saves_and_restores_cursor() {
    let mut screen = Screen::new(5, 20);
    screen.set_cursor_position(2, 3); // 1-based → 0-based (y=1, x=2)
    screen.set_private_mode(1048, true); // save cursor (DECSC)
    screen.set_cursor_position(5, 10); // 0-based (y=4, x=9)
    screen.set_private_mode(1048, false); // restore cursor (DECRC)
    assert_eq!((screen.cursor_x(), screen.cursor_y()), (2, 1));
}

#[test]
fn should_keep_prior_markers_and_mark_new_row_when_wrap_scrolls_at_bottom() {
    // Arrange
    let mut screen = Screen::new(2, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.write_char('f'); // wraps onto row 1 (bottom), marks it
    assert!(screen.is_wrapped(1));
    for ch in "ghij".chars() {
        screen.write_char(ch);
    }
    // Act — one more char wraps at the bottom, scrolling the screen.
    screen.write_char('k');
    // Assert — scrolled-up line keeps its marker, new bottom row is marked.
    assert_eq!(screen.row_text(0), "fghij");
    assert_eq!(screen.row_text(1), "k    ");
    assert!(screen.is_wrapped(0), "prior marker preserved across scroll");
    assert!(screen.is_wrapped(1), "newly entered row is marked");
}

#[test]
fn output_wrap_uses_live_metadata_while_viewport_is_scrolled_back() {
    let mut screen = Screen::new(2, 3);
    for ch in "abcdefghi".chars() {
        screen.write_char(ch);
    }
    assert!(screen.pending_wrap());
    screen.scroll_up(1);
    assert_eq!(screen.view_offset(), 1);

    let source = screen.active.normal.live_row_metadata(1);
    assert_ne!(
        screen.active.normal.row_metadata(1).logical_start,
        source.logical_start,
        "displayed metadata should differ from the writable live row in this fixture"
    );

    screen.write_char('j');

    let continuation = screen.active.normal.live_row_metadata(1);
    assert!(continuation.soft_wrapped);
    assert_eq!(continuation.logical_line_id, source.logical_line_id);
    assert_eq!(
        continuation.logical_start,
        source.logical_start + source.meaningful_extent
    );
}

#[test]
fn partial_region_move_severs_the_unchanged_tail_from_its_replaced_predecessor() {
    let mut screen = screen_with_soft_line(5);
    screen.active.cursor.scroll_region.top = 1;
    screen.active.cursor.scroll_region.bottom = 3;
    let old_tail_id = screen.active.normal.live_row_metadata(4).logical_line_id;

    screen.scroll_up_region(1);

    let tail = screen.active.normal.live_row_metadata(4);
    assert!(!tail.soft_wrapped);
    assert_ne!(tail.logical_line_id, old_tail_id);
    assert_narrow_reflow_prepares(&screen);
}

#[test]
fn alt_entry_reconciles_pending_primary_saved_cursor_mutations() {
    let mut screen = Screen::new(1, 10);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.finish_anchor_mutations(None).unwrap();
    screen.active.cursor.cursor.x = 4;
    screen.save_cursor();

    screen.active.cursor.cursor.x = 1;
    screen.active.cursor.modes.insert = true;
    screen.write_char('X');
    screen.enter_alt(true);
    screen.exit_alt();
    screen.restore_cursor();

    assert_eq!(screen.cursor_x(), 5);
    assert_eq!(screen.cursor_y(), 0);
}

#[test]
fn structural_erase_emits_atom_deletion_without_payload_heuristics() {
    let mut screen = Screen::new(1, 6);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    let before = screen.content_projection().unwrap();
    let endpoint = before
        .to_anchor(
            crate::GenPos::new(0, 4),
            crate::content_anchor::Affinity::After,
        )
        .expect("endpoint anchor");
    screen.active.cursor.cursor.x = 3;

    screen.erase_chars(2);
    let (mutations, projection) = screen
        .finish_anchor_mutations(Some(&before))
        .expect("valid projection after erase");
    let adjusted = mutations.apply(endpoint).expect("later atom survives");

    assert_eq!(adjusted.offset.0, 3);
    assert_eq!(
        projection.resolve_selection(adjusted),
        Some(crate::GenPos::new(0, 5))
    );
}

#[test]
fn saved_cursor_preserves_unwritten_column_when_blank_row_moves() {
    let mut screen = Screen::new(3, 6);
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 4;
    screen.save_cursor();
    let before = screen.content_projection().unwrap();

    screen.active.cursor.cursor.y = 0;
    screen.insert_lines(1);
    screen
        .finish_anchor_mutations(Some(&before))
        .expect("valid projection after blank row movement");
    screen.restore_cursor();

    assert_eq!(screen.cursor_y(), 2);
    assert_eq!(screen.cursor_x(), 4);
}
