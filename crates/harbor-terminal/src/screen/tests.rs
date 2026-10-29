//! Unit tests for Screen.
mod alternate;
mod charsets;
mod damage;
mod editing;
mod hyperlinks;
mod margins;
mod modes;
mod resize;
mod selection;
mod wide_cells;
mod wrapping;

use super::*;
use crate::parser::TerminalParser;

fn set_semantic_cell(screen: &mut Screen, row: usize, col: usize, ch: char) {
    screen.normal.write_meaningful_cell(
        row,
        col,
        Cell {
            ch,
            ..Cell::default()
        },
    );
}

fn screen_with_pending_wrap() -> Screen {
    let mut screen = Screen::new(3, 5);
    for ch in "abcde".chars() {
        screen.write_char(ch);
    }
    assert!(screen.cursor.modes.pending_wrap);
    screen
}

fn assert_pending_wrap_cleared(operation: impl FnOnce(&mut Screen)) {
    let mut screen = screen_with_pending_wrap();
    operation(&mut screen);
    assert!(
        !screen.cursor.modes.pending_wrap,
        "operation must clear pending wrap"
    );
}

fn assert_next_print_stays_on_current_row(operation: impl FnOnce(&mut Screen)) {
    let mut screen = screen_with_pending_wrap();
    operation(&mut screen);
    let target_row = screen.cursor.cursor.y;

    screen.write_char('Z');

    assert_eq!(
        screen.cursor.cursor.y, target_row,
        "cleared pending wrap must not move the sentinel to another row"
    );
    assert!(screen.row_text(target_row).contains('Z'));
}

fn assert_wide_cell_invariant(screen: &Screen) {
    for row in 0..screen.rows() {
        for col in 0..screen.cols() {
            let cell = screen.cell(row, col);
            if cell.wide_continuation {
                assert!(col > 0, "continuation at start of row {row}");
                assert_eq!(
                    unicode_width::UnicodeWidthChar::width(screen.cell(row, col - 1).ch),
                    Some(2),
                    "continuation at ({row}, {col}) lacks its base"
                );
            }
            if unicode_width::UnicodeWidthChar::width(cell.ch) == Some(2) {
                assert!(
                    col + 1 < screen.cols(),
                    "wide base at row end ({row}, {col})"
                );
                assert!(
                    screen.cell(row, col + 1).wide_continuation,
                    "wide base at ({row}, {col}) lacks its continuation"
                );
            }
        }
    }
}

fn screen_cells(screen: &Screen) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(screen.rows() * screen.cols());
    for row in 0..screen.rows() {
        for col in 0..screen.cols() {
            cells.push(screen.cell(row, col).clone());
        }
    }
    cells
}

fn margin_scroll_fixture() -> Screen {
    let mut screen = Screen::new(4, 6);
    screen.cursor.margins.enabled = true;
    screen.cursor.margins.left = 1;
    screen.cursor.margins.right = 4;
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

fn screen_with_soft_line(rows: usize) -> Screen {
    let mut screen = Screen::new(rows, 4);
    for row in 0..rows {
        let ch = char::from(b'a' + row as u8);
        screen.normal.write_meaningful_cell(
            row,
            0,
            Cell {
                ch,
                ..Cell::default()
            },
        );
        if row > 0 {
            let source = screen.normal.live_row_metadata(row - 1);
            let source_atoms = screen.normal.live_row_logical_atom_count(row - 1);
            screen
                .normal
                .continue_logical_line(row, source, source_atoms);
        }
    }
    screen
}

fn assert_narrow_reflow_prepares(screen: &Screen) {
    screen
        .prepare_resize(screen.rows(), 2)
        .expect("row movement must leave logical metadata valid for narrow reflow");
}

fn assert_moved_suffix_starts_hard_and_keeps_internal_wraps(
    screen: &Screen,
    first_moved_row: usize,
    continued_row: usize,
) {
    let first = screen.normal.live_row_metadata(first_moved_row);
    let continued = screen.normal.live_row_metadata(continued_row);
    assert!(
        !first.soft_wrapped,
        "moved suffix must start at a hard boundary"
    );
    assert!(
        continued.soft_wrapped,
        "wraps inside the moved block must survive"
    );
    assert_eq!(continued.logical_line_id, first.logical_line_id);
    assert_eq!(
        continued.logical_start,
        first.logical_start + first.meaningful_extent
    );
    assert_narrow_reflow_prepares(screen);
}
