use super::*;

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
    screen.active.cursor.margins.enabled = true;
    screen.active.cursor.margins.left = 8;
    screen.active.cursor.margins.right = 11;
    screen.clear_tab_stops(3);
    screen.active.pen_state.tab_stops.0[4] = true;

    let prepared = screen.prepare_resize(2, 6).unwrap();
    screen.commit_resize(prepared);
    assert_eq!(
        (
            screen.active.cursor.margins.left,
            screen.active.cursor.margins.right
        ),
        (5, 5)
    );
    assert_eq!(
        screen.active.pen_state.tab_stops.0,
        vec![false, false, false, false, true, false]
    );

    let prepared = screen.prepare_resize(2, 18).unwrap();
    screen.commit_resize(prepared);
    assert!(
        screen.active.pen_state.tab_stops.0[4],
        "existing tab stops must be preserved"
    );
    assert!(
        screen.active.pen_state.tab_stops.0[8],
        "new default tab stop at column 8"
    );
    assert!(
        screen.active.pen_state.tab_stops.0[16],
        "new default tab stop at column 16"
    );
}

#[test]
fn resize_preserves_saved_cursor() {
    // Save at home, resize smaller, restore → clamped to new bounds.
    let mut screen = Screen::new(5, 10);
    screen.save_cursor();
    // Move cursor away and set SGR.
    screen.active.cursor.cursor.y = 4;
    screen.active.cursor.cursor.x = 7;
    screen.active.pen_state.pen.fg = Color::Named(1);
    screen.active.pen_state.pen.bg = Color::Named(2);
    screen.active.pen_state.pen.attrs.set(CellAttrs::BOLD);
    screen.resize(3, 5); // smaller — saved cursor must be clamped
    screen.restore_cursor();
    assert_eq!(
        screen.active.cursor.cursor.x, 4,
        "invalidated saved cursor leaves the clamped live cursor in place"
    );
    assert_eq!(
        screen.active.cursor.cursor.y, 2,
        "invalidated saved cursor leaves the clamped live cursor in place"
    );
    assert_eq!(screen.active.pen_state.pen.fg, Color::Default);
    assert_eq!(screen.active.pen_state.pen.bg, Color::Default);
    assert_eq!(screen.active.pen_state.pen.attrs, CellAttrs::default());

    // Save at non-home, resize larger, restore → original position preserved.
    let mut screen = Screen::new(2, 5);
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 3;
    screen.save_cursor();
    screen.resize(10, 20); // larger — no clamping needed
    screen.restore_cursor();
    assert_eq!(screen.active.cursor.cursor.x, 3, "original x preserved");
    assert_eq!(screen.active.cursor.cursor.y, 1, "original y preserved");
}

#[test]
fn resize_preserves_live_and_saved_cursor_in_unwritten_suffix() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.active.cursor.cursor.x = 3;
    screen.save_cursor();

    screen.resize(1, 6);
    assert_eq!(screen.cursor_x(), 3);
    screen.active.cursor.cursor.x = 0;
    screen.restore_cursor();
    assert_eq!(screen.cursor_x(), 3);
    screen.write_char('b');
    assert_eq!(screen.row_text(0), "a  b  ");
}

#[test]
fn repeated_resize_preserves_prompt_cursor_and_next_print() {
    for prompt in [
        "PS D:\\Code\\harbor> ",
        "prompt> 中文输入 ",
        "12345678901234567890",
    ] {
        let mut screen = Screen::new(8, 40);
        for ch in prompt.chars() {
            screen.write_char(ch);
        }
        let expected_x = screen.cursor_x();
        for _ in 0..5 {
            for cols in [20, 9, 41, 13, 40] {
                screen.resize(8, cols);
                assert_eq!(screen.cols(), cols);
            }
            assert_eq!(
                (screen.cursor_y(), screen.cursor_x()),
                (0, expected_x),
                "{prompt:?}"
            );
            assert!(!screen.pending_wrap());
        }
        screen.write_char('!');
        assert_eq!(screen.cell_char(0, expected_x), '!');
    }
}

#[test]
fn conpty_resize_keeps_partial_history_line_outside_live_viewport() {
    let mut screen = Screen::new(3, 12);
    for ch in "abcdefghijkl".chars() {
        screen.write_char(ch);
    }
    screen.carriage_return();
    screen.line_feed();
    screen.write_char('>');

    for cols in [5, 6] {
        let prepared = screen
            .prepare_resize_with_viewport(3, cols, ReflowViewport::PreserveLiveTop)
            .unwrap();
        screen.commit_resize(prepared);
    }

    // ConPTY retained only the suffix beginning at 'f' after narrowing.
    // Joining it back to the historical 'abcde' would shift the prompt up.
    assert_eq!(screen.row_text(0), "fghijk");
    assert_eq!(screen.row_text(1), "l     ");
    assert_eq!(screen.row_text(2), ">     ");
    assert_eq!((screen.cursor_y(), screen.cursor_x()), (2, 1));
    assert_eq!(screen.scroll_count(), 1);
    let text = screen.selected_text(crate::SelectionBounds {
        start_row: screen.history_start(),
        start_col: 0,
        end_row: screen.history_start() + (screen.scroll_count() + screen.rows()) as u64 - 1,
        end_col: screen.cols() - 1,
    });
    assert_eq!(text, "abcdefghijkl\n>");
    screen.write_char('x');
    assert_eq!(screen.row_text(2), ">x    ");
}

#[test]
fn conpty_alt_resize_uses_original_primary_and_only_final_dimensions() {
    for final_cols in [6, 12] {
        let mut screen = Screen::new(3, 12);
        for ch in "abcdefghijkl".chars() {
            screen.write_char(ch);
        }
        screen.carriage_return();
        screen.line_feed();
        screen.write_char('>');
        screen.enter_alt(false);
        for cols in [5, 2, 9, final_cols] {
            let prepared = screen
                .prepare_resize_with_viewport(3, cols, ReflowViewport::PreserveLiveTop)
                .unwrap();
            screen.commit_resize(prepared);
            assert_eq!(screen.saved_primary.as_ref().unwrap().cols(), 12);
        }
        screen.exit_alt();
        assert_eq!(screen.cols(), final_cols);
        assert_eq!(screen.scroll_count(), 0);
        let row = if final_cols == 6 { 2 } else { 1 };
        assert_eq!((screen.cursor_y(), screen.cursor_x()), (row, 1));
        assert!(screen.row_text(row).starts_with('>'));
        screen.write_char('x');
        assert!(screen.row_text(row).starts_with(">x"));
        assert!(screen.deferred_primary_resize.is_empty());
    }
}

#[test]
fn partial_erase_rebases_following_soft_wrap_for_copy_and_resize() {
    let mut screen = Screen::new(2, 4);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.y = 0;
    screen.active.cursor.cursor.x = 2;
    screen.erase_line(0);

    assert!(screen.content_projection().is_ok());
    assert_eq!(
        screen.selected_text(SelectionBounds {
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 0,
        }),
        "abe"
    );
    screen.resize(2, 3);
    assert_eq!(screen.rows(), 2);
    assert_eq!(screen.cols(), 3);
}

#[test]
fn resize_refreshes_clamped_saved_cursor_anchor_before_later_edits() {
    let mut screen = Screen::new(1, 6);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.finish_anchor_mutations(None).unwrap();
    screen.active.cursor.cursor.x = 5;
    screen.save_cursor();

    screen.resize(1, 4);
    screen.active.cursor.cursor.x = 0;
    screen.delete_chars(1);
    screen.finish_anchor_mutations(None).unwrap();
    screen.restore_cursor();

    assert_eq!(screen.cursor_x(), 1);
    assert_eq!(screen.cursor_y(), 0);
}

#[test]
fn should_clear_pending_wrap_for_line_controls_resize_and_edits() {
    assert_pending_wrap_cleared(Screen::line_feed);
    assert_pending_wrap_cleared(Screen::index);
    assert_pending_wrap_cleared(Screen::reverse_index);
    assert_pending_wrap_cleared(|screen| screen.resize(4, 6));
    assert_pending_wrap_cleared(|screen| screen.erase_display(2));
    assert_pending_wrap_cleared(|screen| screen.erase_line(2));
    assert_pending_wrap_cleared(|screen| screen.erase_chars(1));
    assert_pending_wrap_cleared(|screen| screen.selective_erase_display(2));
    assert_pending_wrap_cleared(|screen| screen.selective_erase_line(2));
    assert_pending_wrap_cleared(|screen| screen.insert_chars(1));
    assert_pending_wrap_cleared(|screen| screen.delete_chars(1));
    assert_pending_wrap_cleared(|screen| screen.insert_lines(1));
    assert_pending_wrap_cleared(|screen| screen.delete_lines(1));
    assert_pending_wrap_cleared(|screen| screen.scroll_up_region(1));
    assert_pending_wrap_cleared(|screen| screen.scroll_down_region(1));
}

#[test]
fn should_project_pending_wrap_on_resize_and_preserve_surviving_marker() {
    let mut screen = screen_with_pending_wrap();
    screen.active.normal.set_wrapped(1, true);

    screen.resize(2, 6);

    assert!(screen.active.cursor.modes.pending_wrap);
    assert!(screen.is_wrapped(1));
}

#[test]
fn prepared_alt_resize_is_rectangular_repairs_wide_edge_and_keeps_zero_history() {
    let mut screen = Screen::new(2, 4);
    screen.enter_alt(false);
    screen.set_cursor_position(1, 3);
    screen.write_char('界');
    for _ in 0..4 {
        screen.newline();
    }
    assert_eq!(screen.scroll_count(), 0);

    let prepared = screen.prepare_resize(1, 3).unwrap();
    screen.commit_resize(prepared);

    assert!(screen.is_alt());
    assert_eq!((screen.rows(), screen.cols()), (1, 3));
    assert_eq!(screen.scroll_count(), 0);
    assert_eq!(screen.cell(0, 2), &Cell::default());
    assert_eq!(
        screen.dirty_ranges(),
        vec![crate::DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 3,
        }]
    );
}

#[test]
fn prepared_resize_reflows_saved_primary_while_alt_is_active() {
    let mut screen = Screen::new(2, 4);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.enter_alt(false);
    screen.write_char('A');

    let prepared = screen.prepare_resize(1, 3).unwrap();
    screen.commit_resize(prepared);
    assert_eq!(screen.row_text(0), "A  ");

    screen.exit_alt();
    assert_eq!((screen.rows(), screen.cols()), (1, 3));
    assert_eq!(screen.row_text(0), "def");
    assert_eq!((screen.cursor_x(), screen.cursor_y()), (2, 0));
}

#[test]
fn prepared_resize_retains_only_reachable_hyperlinks() {
    let mut screen = Screen::new(2, 8);
    screen.open_hyperlink("https://retained.example".into(), None);
    screen.write_char('R');
    let retained_id = screen.cell(0, 0).hyperlink.unwrap();
    screen.close_hyperlink();
    screen.open_hyperlink("https://stale.example".into(), None);
    screen.close_hyperlink();
    assert_eq!(screen.active.hyperlinks.len(), 2);

    let prepared = screen.prepare_resize(2, 4).unwrap();
    screen.commit_resize(prepared);

    assert_eq!(screen.active.hyperlinks.len(), 1);
    assert_eq!(
        screen
            .active
            .hyperlinks
            .get(retained_id)
            .map(|link| link.uri.as_str()),
        Some("https://retained.example")
    );
}

#[test]
fn should_preserve_wrapped_flags_when_resize_shrinks_rows() {
    let mut screen = Screen::new(4, 6);
    screen.active.normal.set_wrapped(1, true);
    screen.active.normal.set_wrapped(3, true);
    // Act — shrink to 2 rows: rows 0..1 survive, rows 2..3 are clipped.
    screen.resize(2, 6);
    // Assert
    assert!(
        !screen.is_wrapped(0),
        "unmarked surviving row stays unwrapped"
    );
    assert!(screen.is_wrapped(1), "surviving row keeps its flag");
}

#[test]
fn saved_cursor_anchor_adjusts_for_insert_before_saved_position() {
    let mut screen = Screen::new(1, 10);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.x = 4;
    screen.save_cursor();
    assert!(
        screen
            .active
            .cursor
            .cursor
            .saved
            .as_ref()
            .unwrap()
            .anchor
            .is_some()
    );

    let before = screen.content_projection().unwrap();
    screen.active.cursor.cursor.x = 1;
    screen.active.cursor.modes.insert = true;
    screen.write_char('X');
    screen
        .finish_anchor_mutations(Some(&before))
        .expect("valid projection");
    screen.restore_cursor();

    assert_eq!(screen.cursor_x(), 5);
    assert_eq!(screen.cursor_y(), 0);
}

#[test]
fn pending_wrap_saved_cursor_uses_after_final_atom_anchor() {
    let mut screen = Screen::new(1, 3);
    for ch in "abc".chars() {
        screen.write_char(ch);
    }
    assert!(screen.pending_wrap());

    screen.save_cursor();
    let projection = screen.content_projection().expect("valid projection");
    let anchor = screen
        .active
        .cursor
        .cursor
        .saved
        .as_ref()
        .and_then(|saved| saved.anchor)
        .expect("saved cursor anchor");
    let projected = projection
        .resolve_cursor(anchor)
        .expect("cursor projection");

    assert_eq!(projected.pos, crate::GenPos::new(0, 2));
    assert!(projected.pending_wrap);
}

#[test]
fn resize_pending_wrap_follows_current_physical_row_not_logical_line_end() {
    for (text, overwrite) in [("abcdefgh", "cd"), ("ab中efgh", "中")] {
        let mut screen = Screen::new(4, 4);
        for ch in text.chars() {
            screen.write_char(ch);
        }
        screen.set_cursor_position(1, 3);
        for ch in overwrite.chars() {
            screen.write_char(ch);
        }
        assert!(screen.pending_wrap());
        screen.resize(4, 8);
        assert_eq!((screen.cursor_y(), screen.cursor_x()), (0, 4));
        assert!(!screen.pending_wrap());
        screen.write_char('!');
        assert_eq!(screen.cell_char(0, 4), '!');
        assert_eq!(screen.cursor_y(), 0);
    }
}

#[test]
fn resize_saved_pending_wrap_follows_current_physical_row() {
    let mut screen = Screen::new(4, 4);
    for ch in "abcdefgh".chars() {
        screen.write_char(ch);
    }
    screen.set_cursor_position(1, 4);
    screen.write_char('X');
    screen.save_cursor();
    screen.set_cursor_position(2, 2);
    screen.resize(4, 8);
    screen.restore_cursor();
    assert_eq!((screen.cursor_y(), screen.cursor_x()), (0, 4));
    assert!(!screen.pending_wrap());
    screen.write_char('!');
    assert_eq!(screen.cell_char(0, 4), '!');
    assert_eq!(screen.cursor_y(), 0);
}

#[test]
fn review_anchor_tracks_retained_top_and_falls_back_after_eviction() {
    let mut screen = Screen::new(2, 2);
    screen
        .active
        .normal
        .scroll_up_full_screen(2, Cell::default());
    screen.scroll_up(1);
    let retained_anchor = screen.active.review_anchor.expect("review anchor");
    let before = screen.content_projection().unwrap();

    screen
        .active
        .normal
        .scroll_up_full_screen(1, Cell::default());
    screen
        .finish_anchor_mutations(Some(&before))
        .expect("valid retained projection");
    assert_eq!(screen.active.review_anchor, Some(retained_anchor));
    assert_eq!(screen.view_offset(), 2);

    let before_eviction = screen.content_projection().unwrap();
    for _ in 0..=screen.active.normal.max_scrollback() {
        screen
            .active
            .normal
            .scroll_up_full_screen(1, Cell::default());
    }
    let (_, projection) = screen
        .finish_anchor_mutations(Some(&before_eviction))
        .expect("valid projection after eviction");
    let fallback = screen
        .active
        .review_anchor
        .expect("oldest retained fallback");
    assert_ne!(fallback.line_id, retained_anchor.line_id);
    assert!(projection.resolve_selection(fallback).is_some());
    assert_eq!(screen.view_offset(), screen.scroll_count());
}

#[test]
fn saved_cursor_anchor_reprojects_after_identity_preserving_row_movement() {
    let mut screen = Screen::new(3, 4);
    screen.active.cursor.cursor.y = 1;
    screen.write_char('x');
    screen.active.cursor.cursor.x = 0;
    screen.save_cursor();
    let before = screen.content_projection().unwrap();

    screen.active.cursor.cursor.y = 0;
    screen.insert_lines(1);
    screen
        .finish_anchor_mutations(Some(&before))
        .expect("valid projection after row movement");
    screen.restore_cursor();

    assert_eq!(screen.cursor_y(), 2);
    assert_eq!(screen.cursor_x(), 0);
}

#[test]
fn partial_soft_line_eviction_preserves_absolute_surviving_anchor_without_aliasing() {
    let mut screen = Screen::new(2, 1);
    let capacity = screen.active.normal.max_scrollback() + screen.rows();
    for _ in 0..capacity {
        screen.write_char('x');
    }
    let before = screen.content_projection().unwrap();
    let stale_offset = before
        .to_anchor(
            crate::GenPos::new(screen.history_start() + 1, 0),
            crate::content_anchor::Affinity::Before,
        )
        .expect("second retained atom anchor");
    assert_eq!(stale_offset.offset.0, 1);

    screen.write_char('x');
    let (mutations, projection) = screen
        .finish_anchor_mutations(Some(&before))
        .expect("valid projection after partial eviction");
    let rebased = mutations
        .apply(stale_offset)
        .expect("surviving atom remains anchored");

    assert_eq!(rebased.offset.0, 1);
    assert_eq!(
        projection.resolve_selection(rebased),
        Some(crate::GenPos::new(screen.history_start(), 0))
    );
}

#[test]
fn prepares_live_and_saved_cursor_without_mutating_screen_geometry() {
    let mut screen = Screen::new(2, 4);
    for ch in "abcd".chars() {
        screen.write_char(ch);
    }
    screen.save_cursor();
    let before = screen.terminal_snapshot();

    let prepared = screen
        .prepare_primary_resize(screen.rows(), 2)
        .expect("detached primary width preparation");

    assert_eq!(
        prepared.live_cursor,
        crate::primary_reflow::PreparedProjection::Projected(
            crate::primary_reflow::ProjectedInsertion {
                position: crate::GenPos::new(3, 1),
                pending_wrap: true,
            }
        )
    );
    assert_eq!(prepared.saved_cursor, prepared.live_cursor);
    assert_eq!(screen.terminal_snapshot(), before);
}

#[test]
fn preparation_applies_pending_saved_cursor_mutations_without_committing_them() {
    let mut screen = Screen::new(2, 10);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.x = 4;
    screen.save_cursor();
    screen.active.cursor.cursor.x = 1;
    screen.active.cursor.modes.insert = true;
    screen.write_char('X');

    let prepared = screen
        .prepare_primary_resize(screen.rows(), 8)
        .expect("preparation reconciles pending saved anchor");

    assert_eq!(
        prepared.saved_cursor,
        crate::primary_reflow::PreparedProjection::Projected(
            crate::primary_reflow::ProjectedInsertion {
                position: crate::GenPos::new(2, 5),
                pending_wrap: false,
            }
        )
    );
    assert_eq!(
        screen.active.cursor.cursor.saved.as_ref().unwrap().cursor_x,
        4,
        "detached preparation must not commit the adjusted source coordinate"
    );
}

#[test]
fn prepares_complete_history_sequence_and_review_anchor() {
    let mut screen = Screen::new(2, 4);
    for ch in "abcdefghi".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.scroll_count(), 1);
    screen.active.normal.set_view_offset(1);
    let before_offset = screen.view_offset();

    let prepared = screen
        .prepare_primary_resize(screen.rows(), 2)
        .expect("history width preparation");

    assert_eq!(prepared.rows().len(), 5);
    assert_eq!(
        prepared.review,
        crate::primary_reflow::PreparedProjection::Projected(crate::GenPos::new(3, 0))
    );
    assert_eq!(screen.view_offset(), before_offset);
}

#[test]
fn prepares_height_growth_with_review_clamped_to_new_live_top() {
    let mut screen = Screen::new(2, 2);
    screen
        .active
        .normal
        .scroll_up_full_screen(2, Cell::default());
    screen.active.normal.set_view_offset(1);
    let before = screen.terminal_snapshot();

    let prepared = screen
        .prepare_primary_resize(4, 2)
        .expect("detached height growth preparation");

    assert_eq!(prepared.target_rows(), 4);
    assert_eq!(prepared.normal().scroll_count(), 0);
    assert_eq!(prepared.normal().view_offset(), 0);
    assert_eq!(
        prepared.review,
        crate::primary_reflow::PreparedProjection::Projected(crate::GenPos::new(4, 0))
    );
    assert_eq!(screen.terminal_snapshot(), before);
}

#[test]
fn narrowing_with_trailing_unwritten_rows_does_not_push_to_scrollback() {
    let mut screen = Screen::new(24, 80);
    for line in 0..7 {
        screen.active.cursor.cursor.y = line;
        screen.active.cursor.cursor.x = 0;
        for ch in "123456789012345678901234567890".chars() {
            screen.write_char(ch);
        }
    }
    screen.active.cursor.cursor.y = 7;
    screen.active.cursor.cursor.x = 0;
    assert_eq!(screen.scroll_count(), 0);

    let prepared = screen
        .prepare_primary_resize(24, 20)
        .expect("narrowing resize");

    assert_eq!(prepared.normal().scroll_count(), 0);
    assert_eq!(prepared.normal().rows(), 24);
    assert_eq!(prepared.dropped_rows(), 0);
    assert_eq!(prepared.normal().cell(0, 0).ch, '1');
    assert_eq!(prepared.normal().cell(0, 1).ch, '2');

    screen.resize(24, 20);
    assert_eq!(screen.scroll_count(), 0);
    assert_eq!(screen.active.normal.cell(0, 0).ch, '1');

    screen.resize(24, 80);
    assert_eq!(screen.scroll_count(), 0);
    assert_eq!(screen.active.normal.cell(0, 0).ch, '1');
}

#[test]
fn narrowing_past_viewport_into_scrollback_and_widening_restores_clean_history() {
    let mut screen = Screen::new(5, 60);
    // Write 4 lines of text:
    // Line 0: "Microsoft Windows [Version 10.0.22621.4317]" (43 chars)
    // Line 1: "(c) Microsoft Corporation. All rights reserved." (47 chars)
    // Line 2: "" (empty)
    // Line 3: "C:\Users\test>" (14 chars)
    screen.active.cursor.cursor.y = 0;
    screen.active.cursor.cursor.x = 0;
    for ch in "Microsoft Windows [Version 10.0.22621.4317]".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 0;
    for ch in "(c) Microsoft Corporation. All rights reserved.".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 0;

    screen.active.cursor.cursor.y = 3;
    screen.active.cursor.cursor.x = 0;
    for ch in "C:\\Users\\test>".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.scroll_count(), 0);

    for _ in 0..5 {
        // Narrowing pushes wrapped pieces of the live viewport into physical
        // scrollback, but those pieces must not become permanent history.
        screen.resize(5, 10);
        assert!(
            screen.scroll_count() > 0,
            "scrollback should be non-zero after severe narrowing"
        );
        screen.resize(5, 60);
        assert_eq!(
            screen.scroll_count(),
            0,
            "temporary reflow rows should return to the viewport after widening"
        );
    }
    assert_eq!(screen.active.cursor.cursor.y, 3);
    assert!(
        screen
            .row_text(0)
            .starts_with("Microsoft Windows [Version 10.0.22621.4317]")
    );
    assert!(
        screen
            .row_text(1)
            .starts_with("(c) Microsoft Corporation. All rights reserved.")
    );
    assert_eq!(screen.row_text(2).trim(), "");
    assert!(screen.row_text(3).starts_with("C:\\Users\\test>"));
}

#[test]
fn narrow_output_then_widen_restores_cursor_above_trailing_blank_rows() {
    let mut screen = Screen::new(24, 80);
    let mut parser = TerminalParser::default();
    let line = "harbor-profile-burst 0123456789 abcdefghijklmnopqrstuvwxyz";
    let output = format!("{line}\r\n").repeat(20);
    let result = parser.put_bytes(&mut screen, output.as_bytes());
    assert_eq!(result.consumed, output.len());
    assert!(result.alt_request.is_none());

    for _ in 0..19 {
        screen.resize(24, 2);
        screen.resize(24, 80);
    }

    screen.resize(24, 2);
    let prompt = b"\r\nharbor> ";
    let result = parser.put_bytes(&mut screen, prompt);
    assert_eq!(result.consumed, prompt.len());
    assert!(result.alt_request.is_none());
    screen.resize(24, 80);

    assert_eq!((screen.cursor_x(), screen.cursor_y()), (8, 21));
    assert_eq!(screen.scroll_count(), 0);
    assert!(screen.row_text(21).starts_with("harbor> "));
    assert_eq!(screen.row_text(22).trim(), "");
    assert_eq!(screen.row_text(23).trim(), "");

    let preserved = crate::logical_content::decode_lines(&screen.active.normal)
        .expect("reflowed profile output should remain decodable")
        .into_iter()
        .filter(|logical_line| {
            logical_line
                .glyphs
                .iter()
                .map(|glyph| glyph.glyph.cell.ch)
                .collect::<String>()
                == line
        })
        .count();
    assert_eq!(preserved, 20);
}

#[test]
fn narrowing_past_viewport_into_scrollback_and_widening_restores_chinese_windows_banner() {
    let mut screen = Screen::new(24, 80);
    // Write the exact Windows console banner with Chinese characters:
    // Line 0: "Microsoft Windows [版本 10.0.22621.4317]"
    // Line 1: "(c) Microsoft Corporation。保留所有权利。"
    // Line 2: "" (empty)
    // Line 3: "C:\Users\Administrator>"
    screen.active.cursor.cursor.y = 0;
    screen.active.cursor.cursor.x = 0;
    for ch in "Microsoft Windows [版本 10.0.22621.4317]".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.y = 1;
    screen.active.cursor.cursor.x = 0;
    for ch in "(c) Microsoft Corporation。保留所有权利。".chars() {
        screen.write_char(ch);
    }
    screen.active.cursor.cursor.y = 2;
    screen.active.cursor.cursor.x = 0;

    screen.active.cursor.cursor.y = 3;
    screen.active.cursor.cursor.x = 0;
    for ch in "C:\\Users\\Administrator>".chars() {
        screen.write_char(ch);
    }
    assert_eq!(screen.scroll_count(), 0);

    // Compress to 30 columns
    screen.resize(24, 30);
    // Compress to 15 columns
    screen.resize(24, 15);
    // Compress to 8 columns and height 5 so lines overflow into scrollback
    screen.resize(5, 8);
    assert!(
        screen.scroll_count() > 0,
        "scrollback should be non-zero after severe narrowing and shortening"
    );

    // Widen back to 24 rows, 80 columns
    screen.resize(24, 80);
    assert_eq!(
        screen.scroll_count(),
        0,
        "scrollback should return to 0 after returning to 24x80"
    );
    assert_eq!(screen.active.cursor.cursor.y, 3);
    assert_eq!(screen.active.cursor.cursor.x, 23);
    assert!(
        screen
            .row_text(0)
            .starts_with("Microsoft Windows [版 本  10.0.22621.4317]")
    );
    assert!(
        screen
            .row_text(1)
            .starts_with("(c) Microsoft Corporation。 保 留 所 有 权 利 。")
    );
    assert_eq!(screen.row_text(2).trim(), "");
    assert!(screen.row_text(3).starts_with("C:\\Users\\Administrator>"));
}
