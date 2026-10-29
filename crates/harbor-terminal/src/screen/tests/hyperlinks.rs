use super::*;

#[test]
fn selected_text_preserves_styled_and_hyperlinked_trailing_blanks() {
    let mut screen = Screen::new(1, 4);
    screen.write_char('a');
    screen.set_sgr_slice(&[Some(44)]);
    screen.write_char(' ');
    screen.open_hyperlink("https://example.test/not-copied".to_owned(), None);
    screen.write_char(' ');

    let result = screen.selected_text(SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 3,
    });

    assert_eq!(result, "a  ");
}

#[test]
fn hyperlink_registry_deduplicates_and_periodically_collects_orphans() {
    let mut screen = Screen::new(1, 2);
    screen.open_hyperlink("https://same.test".to_owned(), Some("id".to_owned()));
    let first = screen.pen_state.active_hyperlink.unwrap();
    screen.open_hyperlink("https://same.test".to_owned(), Some("id".to_owned()));
    assert_eq!(screen.pen_state.active_hyperlink, Some(first));

    screen.close_hyperlink();
    for index in 0..600 {
        screen.open_hyperlink(format!("https://example.test/{index}"), None);
        screen.close_hyperlink();
    }
    assert!(screen.hyperlinks.len() <= 256);
}

#[test]
fn saved_hyperlink_restores_and_erase_cells_are_unlinked() {
    let mut screen = Screen::new(1, 4);
    screen.open_hyperlink("https://one.test".to_owned(), None);
    screen.save_cursor();
    screen.open_hyperlink("https://two.test".to_owned(), None);
    screen.write_char('a');
    let second = screen.cell(0, 0).hyperlink;
    screen.restore_cursor();
    screen.write_char('b');
    assert_ne!(screen.cell(0, 1).hyperlink, second);

    screen.set_cursor_position(1, 1);
    screen.erase_line(0);
    for col in 0..screen.cols() {
        assert!(screen.cell(0, col).hyperlink.is_none());
    }
}
