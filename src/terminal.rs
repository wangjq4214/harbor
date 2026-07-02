mod parser;
mod screen;

use parser::TerminalParser;
pub(crate) use screen::Screen;

/// Terminal dimensions in character cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSize {
    /// Number of visible terminal rows.
    pub(crate) rows: usize,
    /// Number of visible terminal columns.
    pub(crate) cols: usize,
}

/// Stateful terminal model: a byte-stream parser plus the visible screen it mutates.
#[derive(Debug)]
pub(crate) struct Terminal {
    /// Incremental ANSI/VT parser. It keeps partial escape sequences and split UTF-8 bytes
    /// across PTY read chunks.
    parser: TerminalParser,
    /// Current visible cell grid and cursor position consumed by the renderer.
    screen: Screen,
}

impl Terminal {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        Self {
            parser: TerminalParser::default(),
            screen: Screen::new(rows, cols),
        }
    }

    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        self.screen.resize(rows, cols);
    }

    #[cfg(test)]
    pub(crate) fn put_str(&mut self, text: &str) {
        self.put_bytes(text.as_bytes());
    }

    /// Feeds raw PTY bytes through the streaming parser.
    ///
    /// This is the production ingestion path. Do not decode bytes with
    /// `String::from_utf8_lossy` first: doing so would lose split UTF-8 state and could render
    /// replacement characters before the next PTY chunk arrives.
    pub(crate) fn put_bytes(&mut self, bytes: &[u8]) {
        self.parser.put_bytes(&mut self.screen, bytes);
    }

    #[cfg(test)]
    pub(crate) fn put_char(&mut self, ch: char) {
        let mut bytes = [0; 4];
        self.put_bytes(ch.encode_utf8(&mut bytes).as_bytes());
    }

    /// Returns the renderable screen snapshot owned by this terminal.
    pub(crate) fn screen(&self) -> &Screen {
        &self.screen
    }

    #[cfg(test)]
    pub(crate) fn row_text(&self, row: usize) -> String {
        self.screen.row_text(row)
    }

}

#[cfg(test)]
mod tests {
    use super::Terminal;

    #[test]
    fn writes_plain_characters_and_tracks_cursor() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("ab");

        assert_eq!(terminal.row_text(0), "ab  ");
        assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (2, 4));
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (2, 0)
        );
    }

    #[test]
    fn newline_moves_to_next_row_start() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("a\nb");

        assert_eq!(terminal.row_text(0), "a   ");
        assert_eq!(terminal.row_text(1), "b   ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (1, 1)
        );
    }

    #[test]
    fn carriage_return_overwrites_from_row_start() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("ab\rc");

        assert_eq!(terminal.row_text(0), "cb  ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (1, 0)
        );
    }

    #[test]
    fn backspace_erases_previous_cell() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("ab\u{8}c");

        assert_eq!(terminal.row_text(0), "ac  ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (2, 0)
        );
    }

    #[test]
    fn scrolls_when_writing_past_last_row() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("one\ntwo\nthr");

        assert_eq!(terminal.row_text(0), "two ");
        assert_eq!(terminal.row_text(1), "thr ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (3, 1)
        );
    }

    #[test]
    fn resize_preserves_visible_cells_and_clamps_cursor() {
        let mut terminal = Terminal::new(2, 4);
        terminal.put_str("abcdef");

        terminal.resize(1, 3);

        assert_eq!(terminal.row_text(0), "abc");
        assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 3));
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (2, 0)
        );
    }

    #[test]
    fn parses_sgr_without_rendering_escape_bytes() {
        let mut terminal = Terminal::new(1, 8);

        terminal.put_bytes(b"a\x1b[31mb\x1b[0mc");

        assert_eq!(terminal.row_text(0), "abc     ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (3, 0)
        );
    }

    #[test]
    fn csi_cursor_position_overwrites_target_cell() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_bytes(b"abcd\x1b[1;2HZ");

        assert_eq!(terminal.row_text(0), "aZcd");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (2, 0)
        );
    }

    #[test]
    fn csi_erase_line_clears_selected_range() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_bytes(b"abcd\x1b[1;3H\x1b[K");

        assert_eq!(terminal.row_text(0), "ab  ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (2, 0)
        );
    }

    #[test]
    fn csi_erase_display_mode_two_clears_and_homes() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_bytes(b"abcd");
        terminal.put_bytes(b"\x1b[2Jx");

        assert_eq!(terminal.row_text(0), "x   ");
        assert_eq!(terminal.row_text(1), "    ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (1, 0)
        );
    }

    #[test]
    fn keeps_incomplete_escape_sequence_across_chunks() {
        let mut terminal = Terminal::new(1, 5);

        terminal.put_bytes(b"a\x1b[");

        assert_eq!(terminal.row_text(0), "a    ");

        terminal.put_bytes(b"2CZ");

        assert_eq!(terminal.row_text(0), "a  Z ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (4, 0)
        );
    }

    #[test]
    fn keeps_incomplete_utf8_sequence_across_chunks() {
        let mut terminal = Terminal::new(1, 4);
        let bytes = "中".as_bytes();

        terminal.put_bytes(&bytes[..1]);
        terminal.put_bytes(&bytes[1..]);

        assert_eq!(terminal.row_text(0), "中   ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (2, 0)
        );
    }

    #[test]
    fn treats_cjk_characters_as_double_width_cells() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("中a");

        assert_eq!(terminal.row_text(0), "中 a ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (3, 0)
        );
    }

    #[test]
    fn overwrites_both_cells_of_double_width_character() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("中b");
        terminal.put_bytes(b"\x1b[1;2HX");

        assert_eq!(terminal.row_text(0), " Xb ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (2, 0)
        );
    }

    #[test]
    fn backspace_erases_double_width_character() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("中\u{8}");

        assert_eq!(terminal.row_text(0), "    ");
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (0, 0)
        );
    }

    #[test]
    fn horizontal_tab_at_line_end_does_not_loop_forever() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("abc\tz");

        assert_eq!(terminal.row_text(0), "abc ");
        assert_eq!(terminal.row_text(1), "z   ");
    }

    #[test]
    fn ignores_private_cursor_visibility_sequence() {
        let mut terminal = Terminal::new(1, 6);

        terminal.put_bytes(b"a\x1b[?25lb");

        assert_eq!(terminal.row_text(0), "ab    ");
    }

    #[test]
    fn ignores_osc_title_sequence_terminated_by_bel() {
        let mut terminal = Terminal::new(1, 8);

        terminal.put_bytes(b"a\x1b]0;C:\\Windows\\system32\\cmd.exe\x07b");

        assert_eq!(terminal.row_text(0), "ab      ");
    }

    #[test]
    fn keeps_incomplete_osc_sequence_across_chunks() {
        let mut terminal = Terminal::new(1, 8);

        terminal.put_bytes(b"a\x1b]0;title");
        terminal.put_bytes(b"\x1b\\b");

        assert_eq!(terminal.row_text(0), "ab      ");
    }
}
