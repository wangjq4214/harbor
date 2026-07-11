//! Unit tests for Screen.

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
        screen.cursor_y = 2; // bottom row
        screen.newline(); // triggers scroll_up
        let dirty: Vec<usize> = screen.dirty_rows().into_iter().collect();
        assert_eq!(dirty.len(), 3, "scroll_up should mark all rows");
    }

    #[test]
    fn erase_line_marks_cursor_row() {
        let mut screen = Screen::new(4, 4);
        screen.cursor_y = 2;
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
        screen.cursor_y = 2;
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
        screen.cursor_y = 2;
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
        screen.cursor_y = 0;
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
        screen.cursor_x = 2;
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
        screen.cursor_y = 1;
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
        screen.cursor_x = 5;
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

        screen.cursor_x = 2;
        screen.erase_chars(10); // more than remaining cols
        assert_eq!(screen.row_text(0), "ab  ");
    }

    #[test]
    fn erase_chars_zero_acts_as_one() {
        let mut screen = Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        assert_eq!(screen.row_text(0), "ab  ");

        screen.cursor_x = 1;
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

        screen.cursor_x = 2;
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

        screen.cursor_x = 1;
        screen.insert_chars(0);
        assert_eq!(screen.row_text(0), "a bcde");
    }

    #[test]
    fn insert_chars_clamps_to_row_end() {
        let mut screen = Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        assert_eq!(screen.row_text(0), "ab  ");

        screen.cursor_x = 2;
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

        screen.cursor_x = 2;
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
        screen.cursor_x = 1;
        screen.delete_chars(0);
        assert_eq!(screen.row_text(0), "acd  ");
    }

    #[test]
    fn delete_chars_clamps_to_row_end() {
        let mut screen = Screen::new(1, 4);
        screen.write_char('a');
        screen.write_char('b');
        assert_eq!(screen.row_text(0), "ab  ");

        screen.cursor_x = 2;
        screen.delete_chars(10);
        assert_eq!(screen.row_text(0), "ab  ");
    }

    // ── IL / DL ─────────────────────────────────────────────────

    #[test]
    fn insert_lines_within_region() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");
        assert_eq!(screen.row_text(3), "dddd");

        screen.cursor_y = 1;
        screen.insert_lines(1);
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "bbbb");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn insert_lines_outside_region_noop() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        screen.cursor_y = 0; // above scroll_top
        screen.insert_lines(1);
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn delete_lines_within_region() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");

        screen.cursor_y = 1;
        screen.delete_lines(1);
        assert_eq!(screen.row_text(0), "aaaa");
        assert_eq!(screen.row_text(1), "cccc");
        assert_eq!(screen.row_text(2), "    ");
        assert_eq!(screen.row_text(3), "dddd");
    }

    #[test]
    fn delete_lines_outside_region_noop() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        screen.cursor_y = 3; // below scroll_bottom
        screen.delete_lines(1);
        assert_eq!(screen.row_text(1), "bbbb");
        assert_eq!(screen.row_text(2), "cccc");
    }

    // ── SU / SD ─────────────────────────────────────────────────

    #[test]
    fn scroll_up_region_scrolls() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 0;
        screen.scroll_bottom = 2;
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
        screen.scroll_top = 0;
        screen.scroll_bottom = 2;
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
        screen.scroll_top = 2;
        screen.scroll_bottom = 3;
        screen.set_scroll_region(0, 0);
        assert_eq!(screen.scroll_top, 0);
        assert_eq!(screen.scroll_bottom, 3);
        // Cursor homes on success.
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
    }

    #[test]
    fn set_scroll_region_custom() {
        let mut screen = Screen::new(4, 4);
        screen.cursor_x = 3;
        screen.cursor_y = 3;
        screen.set_scroll_region(2, 3);
        assert_eq!(screen.scroll_top, 1);
        assert_eq!(screen.scroll_bottom, 2);
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
    }

    #[test]
    fn set_scroll_region_invalid_ignored() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 0;
        screen.scroll_bottom = 3;
        screen.set_scroll_region(3, 2); // top >= bottom after clamping
        assert_eq!(screen.scroll_top, 0);
        assert_eq!(screen.scroll_bottom, 3);
    }

    // ── Cursor save/restore ──────────────────────────────────────

    #[test]
    fn save_restore_cursor_roundtrips() {
        let mut screen = Screen::new(4, 4);
        screen.save_cursor();
        screen.cursor_x = 2;
        screen.cursor_y = 3;
        screen.current_fg = Color::Named(1);
        screen.current_bg = Color::Named(2);
        screen.current_attrs.set(CellAttrs::BOLD);
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
        assert_eq!(screen.current_fg, Color::Default);
        assert_eq!(screen.current_bg, Color::Default);
        assert_eq!(screen.current_attrs, CellAttrs::default());
    }

    #[test]
    fn restore_cursor_none_is_noop() {
        let mut screen = Screen::new(4, 4);
        screen.cursor_x = 2;
        screen.cursor_y = 3;
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 2);
        assert_eq!(screen.cursor_y, 3);
    }

    // ── Region-aware newline / reverse_index ────────────────────

    #[test]
    fn newline_scrolls_region() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        // Cursor at scroll_bottom, call newline.
        screen.cursor_y = 2;
        screen.cursor_x = 0;
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
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        for row in 0..4 {
            for col in 0..4 {
                screen.cell_mut(row, col).ch = (b'a' + row as u8) as char;
            }
        }
        // Cursor at scroll_top, call reverse_index.
        screen.cursor_y = 1;
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
        screen.scroll_top = 1;
        screen.scroll_bottom = 3;
        // Cursor at (0, 0) — above the scroll region.
        screen.reverse_index(); // must NOT panic
        // No rows or cursor should have changed (no-op above region).
        assert_eq!(screen.cursor_x, 0);
        assert_eq!(screen.cursor_y, 0);
        assert_eq!(screen.row_text(0), "    ");
        assert_eq!(screen.row_text(1), "    ");
        assert_eq!(screen.row_text(2), "    ");
        assert_eq!(screen.row_text(3), "    ");
        assert_eq!(screen.row_text(4), "    ");
    }

    #[test]
    fn newline_below_region_no_panic() {
        let mut screen = Screen::new(4, 4);
        screen.scroll_top = 1;
        screen.scroll_bottom = 2;
        screen.cursor_y = 3; // below region at last row
        screen.cursor_x = 0;
        screen.newline(); // must NOT panic
        // Cursor should not have wrapped past rows-1.
        assert!(screen.cursor_y < screen.rows());
        assert_eq!(screen.cursor_y, 3);
        assert_eq!(screen.cursor_x, 0);
    }

    #[test]
    fn resize_preserves_saved_cursor() {
        // Save at home, resize smaller, restore → clamped to new bounds.
        let mut screen = Screen::new(5, 10);
        screen.save_cursor();
        // Move cursor away and set SGR.
        screen.cursor_y = 4;
        screen.cursor_x = 7;
        screen.current_fg = Color::Named(1);
        screen.current_bg = Color::Named(2);
        screen.current_attrs.set(CellAttrs::BOLD);
        screen.resize(3, 5); // smaller — saved cursor must be clamped
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 0, "saved x clamped to 0.min(4)");
        assert_eq!(screen.cursor_y, 0, "saved y clamped to 0.min(2)");
        assert_eq!(screen.current_fg, Color::Default);
        assert_eq!(screen.current_bg, Color::Default);
        assert_eq!(screen.current_attrs, CellAttrs::default());

        // Save at non-home, resize larger, restore → original position preserved.
        let mut screen = Screen::new(2, 5);
        screen.cursor_y = 1;
        screen.cursor_x = 3;
        screen.save_cursor();
        screen.resize(10, 20); // larger — no clamping needed
        screen.restore_cursor();
        assert_eq!(screen.cursor_x, 3, "original x preserved");
        assert_eq!(screen.cursor_y, 1, "original y preserved");
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
        screen.current_bg = Color::Named(4); // blue
        screen.write_char('a');
        screen.cursor_x = 0;
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
        screen.current_bg = Color::Named(2); // green
        screen.cursor_y = 0;
        screen.cursor_x = 1;
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
        screen.current_bg = Color::Rgb(64, 128, 255);
        screen.cursor_y = 1;
        screen.cursor_x = 1;
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
        screen.current_bg = Color::Bright(7);
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
        screen.current_bg = Color::Named(1); // red
        screen.cursor_x = 1;
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
        screen.current_fg = Color::Named(3); // yellow
        screen.current_bg = Color::Named(4); // blue
        screen.current_attrs.set(CellAttrs::BOLD);
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
        screen.current_bg = Color::Named(4); // blue
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
