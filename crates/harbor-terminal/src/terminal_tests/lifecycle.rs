use super::*;

#[test]
fn should_initialize_headless_terminal_with_given_dimensions() {
    // Arrange & Act
    let terminal = Terminal::new_headless(30, 100);

    // Assert
    assert_eq!(terminal.screen().rows(), 30);
    assert_eq!(terminal.screen().cols(), 100);
}

#[test]
fn should_return_true_and_update_size_when_resize_if_changed_has_new_dimensions() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    let new_size = TerminalSize {
        rows: 30,
        cols: 100,
    };

    // Act
    let changed = terminal.resize_if_changed(new_size);

    // Assert
    assert!(changed);
    assert_eq!(terminal.screen().rows(), 30);
    assert_eq!(terminal.screen().cols(), 100);
}

#[test]
fn should_return_false_and_preserve_size_when_resize_if_changed_has_same_dimensions() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    let same_size = TerminalSize { rows: 24, cols: 80 };

    // Act
    let changed = terminal.resize_if_changed(same_size);

    // Assert
    assert!(!changed);
    assert_eq!(terminal.screen().rows(), 24);
    assert_eq!(terminal.screen().cols(), 80);
}

#[test]
fn should_derive_grid_size_from_render_target_allocation() {
    // Arrange: allocation is smaller than the full surface.
    use crate::{RenderTarget, RenderViewport, TextMetrics};

    let metrics = TextMetrics {
        cell_width: 10.0,
        line_height: 20.0,
        ascent: 16.0,
        underline_position: 16.0,
        underline_thickness: 2.0,
        strikethrough_position: 10.0,
        strikethrough_thickness: 2.0,
    };
    let target = RenderTarget::new((20.0, 10.0), (400, 240), (800, 600));
    let viewport = RenderViewport::from_target(target, &metrics);
    let grid = viewport.compute_grid_size();
    let mut terminal = Terminal::new_headless(24, 80);

    // Act
    let changed = terminal.resize_if_changed(grid);

    // Assert
    assert!(changed);
    assert_eq!(terminal.screen().rows(), grid.rows);
    assert_eq!(terminal.screen().cols(), grid.cols);
    assert_ne!((grid.rows, grid.cols), (24, 80));
}

#[test]
fn should_not_resize_grid_when_render_target_keeps_same_rows_and_cols() {
    // Arrange: geometry change that preserves qualitative grid size.
    use crate::{RenderTarget, RenderViewport, TextMetrics};

    let metrics = TextMetrics {
        cell_width: 10.0,
        line_height: 20.0,
        ascent: 16.0,
        underline_position: 16.0,
        underline_thickness: 2.0,
        strikethrough_position: 10.0,
        strikethrough_thickness: 2.0,
    };
    let first = RenderTarget::new((0.0, 0.0), (800, 480), (800, 600));
    let second = RenderTarget::new((10.0, 10.0), (800, 480), (820, 620));
    let first_grid = RenderViewport::from_target(first, &metrics).compute_grid_size();
    let second_grid = RenderViewport::from_target(second, &metrics).compute_grid_size();
    assert_eq!(first_grid, second_grid);

    let mut terminal = Terminal::new_headless(first_grid.rows, first_grid.cols);

    // Act
    let changed = terminal.resize_if_changed(second_grid);

    // Assert: PTY/screen resize is skipped when rows/cols are unchanged.
    assert!(!changed);
    assert_eq!(terminal.screen().rows(), first_grid.rows);
    assert_eq!(terminal.screen().cols(), first_grid.cols);
}

#[test]
fn should_reset_scroll_snap_suppression_when_resized() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    terminal.set_suppress_scroll_snap(true);

    // Act
    terminal.resize(30, 100);

    // Assert
    // Verify scroll snap is no longer suppressed by checking behavior after scrolling
    for i in 0..35 {
        terminal.put_str(&format!("line {i}\r\n"));
    }
    terminal.scroll_viewport_up(5);
    let offset_before = terminal.screen().view_offset();
    assert!(
        offset_before > 0,
        "expected scrollback after output: rows={}, scroll_count={}, cursor_y={}, last_row={:?}",
        terminal.screen().rows(),
        terminal.screen().scroll_count(),
        terminal.screen().cursor_y(),
        terminal.row_text(29)
    );

    // process_output should snap to bottom now since suppress_scroll_snap was reset to false
    terminal.process_output(b"new output\r\n");
    assert_eq!(terminal.screen().view_offset(), 0);
}

#[test]
fn should_reset_scroll_snap_suppression_when_resize_if_changed_modifies_dimensions() {
    // Arrange
    let mut terminal = Terminal::new_headless(24, 80);
    terminal.set_suppress_scroll_snap(true);

    // Act
    let changed = terminal.resize_if_changed(TerminalSize {
        rows: 30,
        cols: 100,
    });

    // Assert
    assert!(changed);
    // Verify scroll snap is reset by scrolling up and then processing output
    for i in 0..35 {
        terminal.put_str(&format!("line {i}\r\n"));
    }
    terminal.scroll_viewport_up(5);
    terminal.process_output(b"more output\r\n");
    assert_eq!(terminal.screen().view_offset(), 0);
}
