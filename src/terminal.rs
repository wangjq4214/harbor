mod parser;
mod screen;
mod text;

use parser::TerminalParser;
pub(crate) use screen::Screen;
pub(crate) use text::TextLayer;

use winit::window::Window;

use crate::renderer::Renderer;
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

    /// Returns the renderable screen snapshot owned by this terminal.
    pub(crate) fn screen(&self) -> &Screen {
        &self.screen
    }

    /// Mutable screen access for tests.
    #[cfg(test)]
    pub(crate) fn screen_mut(&mut self) -> &mut Screen {
        &mut self.screen
    }

    /// Resets the screen's dirty-row tracking (called after layers consume the dirt).
    pub(crate) fn clear_screen_dirty(&mut self) {
        self.screen.clear_dirty();
    }

    #[cfg(test)]
    pub(crate) fn row_text(&self, row: usize) -> String {
        self.screen.row_text(row)
    }

    /// Feeds raw PTY bytes into the terminal parser and refreshes the
    /// renderer's text/cursor GPU resources for the new screen content.
    pub(crate) fn process_output(
        &mut self,
        renderer: &mut Renderer,
        window: &Window,
        output: &[u8],
    ) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }

        // Feed bytes into the terminal parser (updates screen cells and cursor).
        self.put_bytes(output);
        // Upload new text atlas and cursor vertices for the changed screen.
        renderer.prepare_layers(self.screen());
        // Clear dirty tracking after layers consume it.
        self.clear_screen_dirty();
        // Request a redraw to display the updated screen.
        window.request_redraw();
    }

    /// Resizes the terminal grid when the window surface changes.
    ///
    /// Compares `new_size` against the current terminal dimensions.  If
    /// different, resizes the terminal, re-uploads layers, and clears dirty
    /// tracking.
    pub(crate) fn resize_with_renderer(&mut self, renderer: &mut Renderer, new_size: TerminalSize) {
        let current = TerminalSize {
            rows: self.screen().rows(),
            cols: self.screen().cols(),
        };

        if new_size != current {
            self.resize(new_size.rows, new_size.cols);
            renderer.prepare_layers(self.screen());
            self.clear_screen_dirty();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Terminal;
    use super::screen::Color;
    use super::screen::CellAttrs;
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
    fn backspace_is_non_destructive() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("ab\u{8}");

        assert_eq!(terminal.row_text(0), "ab  ");
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
    fn sgr_sets_fg_color_on_written_cells() {
        let mut terminal = Terminal::new(1, 8);

        terminal.put_bytes(b"a\x1b[31mb\x1b[0mc");

        // 'a' is default, 'b' is red (31), 'c' is reset to default
        assert_eq!(terminal.row_text(0), "abc     ");
        assert_eq!(terminal.screen().cell(0, 0).fg, Color::Default);
        assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1)); // 31 = red = Named(1)
        assert_eq!(terminal.screen().cell(0, 2).fg, Color::Default);
    }

    // ── SGR attribute tests ─────────────────────────────────────────

    #[test]
    fn sgr_bold_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[1ma");
        assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::BOLD));
    }

    #[test]
    fn sgr_dim_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[2ma");
        assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::DIM));
    }

    #[test]
    fn sgr_italic_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[3ma");
        assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::ITALIC));
    }

    #[test]
    fn sgr_underline_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[4ma");
        assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::UNDERLINE));
    }

    #[test]
    fn sgr_blink_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[5ma");
        assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::BLINK));
    }

    #[test]
    fn sgr_inverse_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[7ma");
        assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::INVERSE));
    }

    #[test]
    fn sgr_strikethrough_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[9ma");
        assert!(terminal.screen().cell(0, 0).attrs.contains(CellAttrs::STRIKETHROUGH));
    }

    #[test]
    fn sgr_reset_clears_all() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[1;31;42ma");
        terminal.put_bytes(b"\x1b[0mb");
        let cell = terminal.screen().cell(0, 1);
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Default);
        assert!(cell.attrs.is_empty());
    }

    // ── SGR 8-color tests ───────────────────────────────────────────

    #[test]
    fn sgr_8color_fg_sets_named() {
        for code in 30u8..=37u8 {
            let mut terminal = Terminal::new(1, 2);
            let seq = format!("\x1b[{}mX", code);
            terminal.put_bytes(seq.as_bytes());
            assert_eq!(terminal.screen().cell(0, 0).fg, Color::Named((code - 30) as u8),
                "SGR {} should set fg Named({})", code, code - 30);
        }
    }

    #[test]
    fn sgr_8color_bg_sets_named() {
        for code in 40u8..=47u8 {
            let mut terminal = Terminal::new(1, 2);
            let seq = format!("\x1b[{}mX", code);
            terminal.put_bytes(seq.as_bytes());
            assert_eq!(terminal.screen().cell(0, 0).bg, Color::Named((code - 40) as u8),
                "SGR {} should set bg Named({})", code, code - 40);
        }
    }

    #[test]
    fn sgr_bright_fg_sets_bright() {
        for code in 90u8..=97u8 {
            let mut terminal = Terminal::new(1, 2);
            let seq = format!("\x1b[{}mX", code);
            terminal.put_bytes(seq.as_bytes());
            assert_eq!(terminal.screen().cell(0, 0).fg, Color::Bright((code - 90) as u8),
                "SGR {} should set fg Bright({})", code, code - 90);
        }
    }

    #[test]
    fn sgr_bright_bg_sets_bright() {
        for code in 100u8..=107u8 {
            let mut terminal = Terminal::new(1, 2);
            let seq = format!("\x1b[{}mX", code);
            terminal.put_bytes(seq.as_bytes());
            assert_eq!(terminal.screen().cell(0, 0).bg, Color::Bright((code - 100) as u8),
                "SGR {} should set bg Bright({})", code, code - 100);
        }
    }

    #[test]
    fn sgr_256color_fg_sets_indexed() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[38;5;200mb");
        assert_eq!(terminal.screen().cell(0, 0).fg, Color::Indexed(200));
    }

    #[test]
    fn sgr_256color_bg_sets_indexed() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[48;5;100mb");
        assert_eq!(terminal.screen().cell(0, 0).bg, Color::Indexed(100));
    }

    #[test]
    fn sgr_truecolor_fg_sets_rgb() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[38;2;10;20;30mb");
        assert_eq!(terminal.screen().cell(0, 0).fg, Color::Rgb(10, 20, 30));
    }

    #[test]
    fn sgr_truecolor_bg_sets_rgb() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[48;2;100;150;200mb");
        assert_eq!(terminal.screen().cell(0, 0).bg, Color::Rgb(100, 150, 200));
    }

    #[test]
    fn sgr_multi_param_sets_all() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[1;31;44ma");
        let cell = terminal.screen().cell(0, 0);
        assert!(cell.attrs.contains(CellAttrs::BOLD));
        assert_eq!(cell.fg, Color::Named(1));
        assert_eq!(cell.bg, Color::Named(4));
    }

    #[test]
    fn sgr_default_fg_bg_resets_colors() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[31;42m\x1b[39;49mb");
        let cell = terminal.screen().cell(0, 0);
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Default);
    }

    #[test]
    fn sgr_compound_clear_removes_attrs() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[1;3ma");
        let cell = terminal.screen().cell(0, 0);
        assert!(cell.attrs.contains(CellAttrs::BOLD));
        assert!(cell.attrs.contains(CellAttrs::ITALIC));
        terminal.put_bytes(b"\x1b[23mb");
        let cell = terminal.screen().cell(0, 1);
        assert!(!cell.attrs.contains(CellAttrs::ITALIC));
        assert!(cell.attrs.contains(CellAttrs::BOLD));
    }

    #[test]
    fn sgr_22_clears_bold_and_dim() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[1;2ma\x1b[22mb");
        let cell = terminal.screen().cell(0, 1);
        assert!(!cell.attrs.contains(CellAttrs::BOLD));
        assert!(!cell.attrs.contains(CellAttrs::DIM));
    }

    #[test]
    fn sgr_24_clears_underline() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[4ma\x1b[24mb");
        let cell = terminal.screen().cell(0, 1);
        assert!(!cell.attrs.contains(CellAttrs::UNDERLINE));
    }

    #[test]
    fn sgr_25_clears_blink() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[5ma\x1b[25mb");
        let cell = terminal.screen().cell(0, 1);
        assert!(!cell.attrs.contains(CellAttrs::BLINK));
    }

    #[test]
    fn sgr_27_clears_inverse() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[7ma\x1b[27mb");
        let cell = terminal.screen().cell(0, 1);
        assert!(!cell.attrs.contains(CellAttrs::INVERSE));
    }

    #[test]
    fn sgr_29_clears_strikethrough() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[9ma\x1b[29mb");
        let cell = terminal.screen().cell(0, 1);
        assert!(!cell.attrs.contains(CellAttrs::STRIKETHROUGH));
    }

    #[test]
    fn sgr_bare_csi_m_is_reset() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[1;31;42ma\x1b[mb");
        let cell = terminal.screen().cell(0, 1);
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Default);
        assert!(cell.attrs.is_empty());
    }

    // ── SGR error handling / robustness ─────────────────────────────

    #[test]
    fn sgr_indexed_out_of_range_ignored() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[31ma");
        terminal.put_bytes(b"\x1b[38;5;300mb");
        // 300 > 255 so fg should still be Named(1) from the 31 sequence
        assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
    }

    #[test]
    fn sgr_truecolor_missing_params_ignored() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[31ma");
        terminal.put_bytes(b"\x1b[38;2;128;64mx");
        // Incomplete truecolor seq — fg stays red, 'x' still renders
        assert_eq!(terminal.row_text(0), "ax  ");
        assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
    }

    #[test]
    fn sgr_truecolor_component_out_of_range_ignored() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[31ma");
        terminal.put_bytes(b"\x1b[38;2;300;0;0mb");
        // 300 > 255 — fg stays red
        assert_eq!(terminal.screen().cell(0, 1).fg, Color::Named(1));
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
    fn backspace_on_double_width_character_is_non_destructive() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("中\u{8}");

        assert_eq!(terminal.row_text(0), "中   ");
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
