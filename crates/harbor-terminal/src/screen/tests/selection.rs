use super::*;

#[test]
fn selected_text_single_row() {
    let mut screen = Screen::new(3, 11);
    for (col, ch) in "hello world".chars().enumerate() {
        set_semantic_cell(&mut screen, 1, col, ch);
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
    for (row, line) in ["ab", "cd", "ef"].iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            set_semantic_cell(&mut screen, row, col, ch);
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
fn selected_text_preserves_meaningful_trailing_whitespace() {
    let mut screen = Screen::new(2, 5);
    set_semantic_cell(&mut screen, 0, 0, 'a');
    set_semantic_cell(&mut screen, 0, 1, ' ');
    set_semantic_cell(&mut screen, 0, 2, ' ');
    set_semantic_cell(&mut screen, 1, 0, 'b');
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 1,
    });
    assert_eq!(result, "a  \nb");
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

#[test]
fn selected_text_pending_wrap_does_not_invent_a_row_boundary() {
    let mut screen = Screen::new(2, 3);
    for ch in "abc".chars() {
        screen.write_char(ch);
    }
    assert!(screen.pending_wrap());

    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 2,
    });

    assert_eq!(result, "abc");
}

#[test]
fn selected_text_retains_combining_and_presentation_scalars() {
    let mut screen = Screen::new(1, 3);
    screen.write_char('a');
    screen.write_char('\u{0301}');
    screen.write_char('\u{fe0f}');
    screen.write_char('\u{200d}');

    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 2,
    });

    assert_eq!(result, "a\u{0301}\u{fe0f}\u{200d}");
}

#[test]
fn selected_text_across_scrollback_generations() {
    let mut screen = Screen::new(5, 3);
    // Write identifiable content to display rows.
    set_semantic_cell(&mut screen, 0, 0, 'A');
    set_semantic_cell(&mut screen, 1, 0, 'B');
    set_semantic_cell(&mut screen, 2, 0, 'C');
    // Push into scrollback.
    screen
        .active
        .normal
        .scroll_up_full_screen(3, Cell::default());
    // Now gen 0 = 'A', gen 1 = 'B', gen 2 = 'C'.
    // Write fresh content to new visible rows.
    set_semantic_cell(&mut screen, 0, 0, 'X'); // gen 3
    set_semantic_cell(&mut screen, 1, 0, 'Y'); // gen 4

    // Select gens 0-4 — spans scrollback + visible.
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 4,
        end_col: 0,
    });
    assert_eq!(result, "A\nB\nC\nX\nY", "scrollback+visible selection");

    // Select just visible range (gens 3-4).
    let result = screen.selected_text(SelectionBounds {
        start_row: 3,
        start_col: 0,
        end_row: 4,
        end_col: 0,
    });
    assert_eq!(result, "X\nY", "visible-only selection via generation");

    // Select just scrollback (gens 0-1).
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 0,
    });
    assert_eq!(result, "A\nB", "scrollback-only selection via generation");
}

#[test]
fn selected_text_evicted_generation_skipped() {
    let mut screen = Screen::new(5, 3);
    let max = screen.active.normal.max_scrollback();

    // Write a marker at gen 0.
    set_semantic_cell(&mut screen, 0, 0, 'M');
    // Fill the ring until gen 0 is evicted (max+1 scrolls).
    for _ in 0..max + 1 {
        screen
            .active
            .normal
            .scroll_up_full_screen(1, Cell::default());
    }
    // gen 0 is now evicted (history_start = 1).

    // Write fresh content to new display rows.
    set_semantic_cell(&mut screen, 0, 0, 'N'); // current first visible generation

    // Selecting gen 0 should skip it (not panic).
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 0,
    });
    assert_eq!(
        result, "",
        "evicted gen produces empty selection, not panic"
    );

    // Select the current generation (display row 0 after max+1 scrolls).
    let hist_start = screen.active.normal.history_start();
    let scroll_count = screen.active.normal.scroll_count();
    let current_gen = hist_start + scroll_count as u64; // display row 0
    let result = screen.selected_text(SelectionBounds {
        start_row: current_gen,
        start_col: 0,
        end_row: current_gen,
        end_col: 0,
    });
    assert_eq!(result, "N", "current visible gen after eviction");
}

#[test]
fn selected_text_evicted_start_clamped_no_blank_lines() {
    let mut screen = Screen::new(5, 3);
    let max = screen.active.normal.max_scrollback();

    // Write a distinct marker at display row 1 (generation 1: hist_start=0, scroll_count=0, row=1).
    set_semantic_cell(&mut screen, 1, 0, 'Z');
    // One scroll pushes gen 0 into scrollback; 'Z' lands at gen 1 display row 0.
    screen
        .active
        .normal
        .scroll_up_full_screen(1, Cell::default());
    // Fill the ring to evict gen 0 (max more scrolls → total = max+1).
    for _ in 0..max {
        screen
            .active
            .normal
            .scroll_up_full_screen(1, Cell::default());
    }
    // history_start == 1 now → gen 0 evicted, gen 1 is the first retained gen.
    assert_eq!(
        screen.active.normal.history_start(),
        1,
        "gen 0 must be evicted"
    );

    // Select from evicted gen 0 through the first retained gen (hist_start = 1).
    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 0,
    });
    // Must NOT have leading blank lines from billions of evicted generations.
    assert_eq!(
        result, "Z",
        "clamped selection must not produce leading blank lines"
    );
}
