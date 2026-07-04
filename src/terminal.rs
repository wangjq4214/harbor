mod parser;
mod screen;
mod text;

use parser::TerminalParser;
pub(crate) use screen::AltScreenAction;
pub(crate) use screen::Color;
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
    /// Primary screen buffer (shell prompt, command output).
    normal: Screen,
    /// Alternate screen buffer (vim, less, htop). `Some` means alt screen is active.
    alt: Option<Screen>,
}

impl Terminal {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        Self {
            parser: TerminalParser::default(),
            normal: Screen::new(rows, cols),
            alt: None,
        }
    }

    fn active_screen_mut(&mut self) -> &mut Screen {
        self.alt.as_mut().unwrap_or(&mut self.normal)
    }

    fn handle_alt_request(&mut self, action: AltScreenAction) {
        match action {
            AltScreenAction::Enter => {
                if self.alt.is_some() {
                    return; // already in alt screen — idempotent
                }
                self.alt = Some(Screen::new(self.normal.rows(), self.normal.cols()));
            }
            AltScreenAction::Exit => {
                if let Some(_alt) = self.alt.take() {
                    // Mark normal screen dirty so renderer rebuilds on next frame.
                    self.normal.mark_all_dirty();
                }
            }
        }
    }

    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        self.normal.resize(rows, cols);
        if let Some(alt) = self.alt.as_mut() {
            alt.resize(rows, cols);
        }
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
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let result = {
                let parser = &mut self.parser;
                let screen = match self.alt.as_mut() {
                    Some(s) => s,
                    None => &mut self.normal,
                };
                parser.put_bytes(screen, remaining)
            };
            remaining = &remaining[result.consumed..];
            if let Some(action) = result.alt_request {
                // Consume the flag from the screen for hygiene.
                self.active_screen_mut().take_alt_request();
                self.handle_alt_request(action);
            }
        }
    }

    /// Returns the renderable screen snapshot owned by this terminal.
    pub(crate) fn screen(&self) -> &Screen {
        self.alt.as_ref().unwrap_or(&self.normal)
    }

    /// Mutable screen access for tests.
    #[cfg(test)]
    pub(crate) fn screen_mut(&mut self) -> &mut Screen {
        &mut self.normal
    }

    /// Resets the screen's dirty-row tracking (called after layers consume the dirt).
    pub(crate) fn clear_screen_dirty(&mut self) {
        self.active_screen_mut().clear_dirty();
    }

    #[cfg(test)]
    pub(crate) fn row_text(&self, row: usize) -> String {
        self.screen().row_text(row)
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
    use super::screen::CellAttrs;
    use super::screen::Color;
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
        assert!(
            terminal
                .screen()
                .cell(0, 0)
                .attrs
                .contains(CellAttrs::ITALIC)
        );
    }

    #[test]
    fn sgr_underline_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[4ma");
        assert!(
            terminal
                .screen()
                .cell(0, 0)
                .attrs
                .contains(CellAttrs::UNDERLINE)
        );
    }

    #[test]
    fn sgr_blink_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[5ma");
        assert!(
            terminal
                .screen()
                .cell(0, 0)
                .attrs
                .contains(CellAttrs::BLINK)
        );
    }

    #[test]
    fn sgr_inverse_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[7ma");
        assert!(
            terminal
                .screen()
                .cell(0, 0)
                .attrs
                .contains(CellAttrs::INVERSE)
        );
    }

    #[test]
    fn sgr_strikethrough_sets_attr() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_bytes(b"\x1b[9ma");
        assert!(
            terminal
                .screen()
                .cell(0, 0)
                .attrs
                .contains(CellAttrs::STRIKETHROUGH)
        );
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
            assert_eq!(
                terminal.screen().cell(0, 0).fg,
                Color::Named(code - 30),
                "SGR {} should set fg Named({})",
                code,
                code - 30
            );
        }
    }

    #[test]
    fn sgr_8color_bg_sets_named() {
        for code in 40u8..=47u8 {
            let mut terminal = Terminal::new(1, 2);
            let seq = format!("\x1b[{}mX", code);
            terminal.put_bytes(seq.as_bytes());
            assert_eq!(
                terminal.screen().cell(0, 0).bg,
                Color::Named(code - 40),
                "SGR {} should set bg Named({})",
                code,
                code - 40
            );
        }
    }

    #[test]
    fn sgr_bright_fg_sets_bright() {
        for code in 90u8..=97u8 {
            let mut terminal = Terminal::new(1, 2);
            let seq = format!("\x1b[{}mX", code);
            terminal.put_bytes(seq.as_bytes());
            assert_eq!(
                terminal.screen().cell(0, 0).fg,
                Color::Bright(code - 90),
                "SGR {} should set fg Bright({})",
                code,
                code - 90
            );
        }
    }

    #[test]
    fn sgr_bright_bg_sets_bright() {
        for code in 100u8..=107u8 {
            let mut terminal = Terminal::new(1, 2);
            let seq = format!("\x1b[{}mX", code);
            terminal.put_bytes(seq.as_bytes());
            assert_eq!(
                terminal.screen().cell(0, 0).bg,
                Color::Bright(code - 100),
                "SGR {} should set bg Bright({})",
                code,
                code - 100
            );
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

    #[test]
    fn cargo_update_output_spans_multiple_rows() {
        // Replay the PTY output chunks logged during `cargo update`.
        let mut terminal = Terminal::new(5, 80);

        // Chunk 0: "    Updating crates.io index\r\n"
        terminal.put_bytes(b"\x1b[92m\x1b[1m    Updating\x1b[m crates.io index\r\n");
        assert_eq!(
            terminal.row_text(0).trim_end(),
            "    Updating crates.io index"
        );
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (0, 1)
        );

        // Chunks 1-6: progress-bar updates that rewrite the same row via \r.
        terminal.put_bytes(b"\x1b[96m\x1b[1m       Fetch\x1b[m ");
        terminal.put_bytes(b"\x1b]9;4;3;0\x1b\\");
        terminal.put_bytes(b"[=====>                           ] 0 complete; 1 pending\x1b[144X\r");
        terminal.put_bytes(b"\x1b[96m\x1b[1m       Fetch\x1b[m ");
        terminal.put_bytes(b"\x1b]9;4;3;0\x1b\\");
        terminal.put_bytes(b"[=====>                           ] 1 complete; 0 pending\x1b[144X\r");
        // Confirm row 0 is untouched by progress bars.
        assert_eq!(
            terminal.row_text(0).trim_end(),
            "    Updating crates.io index"
        );

        // Chunk 7: "     Locking 0 packages ...\r\n"
        terminal.put_bytes(
            b"\x1b[92m\x1b[1m     Locking\x1b[m 0 packages to latest Rust 1.95.0 compatible versions\x1b[151X\r\n",
        );
        let row1 = terminal.row_text(1);
        assert!(
            row1.contains("Locking 0 packages to latest Rust 1.95.0 compatible versions"),
            "expected locking line on row 1, got: {row1:?}"
        );
        // CSI 151 X should have erased the stale progress-bar tail ("0 pending").
        assert!(
            !row1.contains("pending"),
            "ECH should have erased stale 'pending' text from progress bar, got: {row1:?}"
        );
        assert_eq!(
            (terminal.screen().cursor_x(), terminal.screen().cursor_y()),
            (0, 2)
        );

        // Chunk 8: "\r\nd:\workspaces\harbor>"
        // Chunk 8: "\r\nd:\workspaces\harbor>" — the leading \r\n advances
        // to the next row, so the prompt lands on row 3, not row 2.
        terminal.put_bytes(b"\r\nd:\\workspaces\\harbor>");
        let row3 = terminal.row_text(3);
        assert!(
            row3.contains("d:\\workspaces\\harbor>"),
            "expected prompt on row 3, got: {row3:?}"
        );
    }

    #[test]
    fn erase_chars_via_csi_x_clears_specified_count() {
        let mut terminal = Terminal::new(1, 20);
        terminal.put_bytes(b"hello world!!!!!!");
        assert_eq!(terminal.row_text(0).trim_end(), "hello world!!!!!!");

        // Move cursor to col 11 (at '!') and ECH 6 chars.
        terminal.put_bytes(b"\r\x1b[11C\x1b[6X");
        assert_eq!(
            terminal.row_text(0).trim_end(),
            "hello world",
            "CSI 6 X should erase 6 exclamation marks"
        );
    }

    #[test]
    fn alt_screen_enter_exit_preserves_normal_screen() {
        let mut terminal = Terminal::new(3, 20);
        terminal.put_str("normal");
        assert_eq!(terminal.row_text(0).trim(), "normal");

        // Enter alt screen
        terminal.put_str("\x1b[?1049h");
        // Alt screen starts blank
        assert!(terminal.row_text(0).trim().is_empty());

        // Write to alt screen
        terminal.put_str("alt");
        assert_eq!(terminal.row_text(0).trim(), "alt");

        // Exit alt screen
        terminal.put_str("\x1b[?1049l");
        // Normal screen restored
        assert_eq!(terminal.row_text(0).trim(), "normal");
    }

    #[test]
    fn alt_screen_enter_twice_is_idempotent() {
        let mut terminal = Terminal::new(3, 20);
        terminal.put_str("\x1b[?1049h");
        terminal.put_str("first");
        terminal.put_str("\x1b[?1049h"); // second enter — no-op
        assert_eq!(terminal.row_text(0).trim(), "first");
    }

    #[test]
    fn alt_screen_exit_when_not_in_alt_is_noop() {
        let mut terminal = Terminal::new(3, 20);
        terminal.put_str("normal");
        terminal.put_str("\x1b[?1049l"); // exit without enter — no panic
        assert_eq!(terminal.row_text(0).trim(), "normal");
    }

    #[test]
    fn alt_screen_switch_mid_batch_splits_correctly() {
        // Simulates PTY sending CSI ?1049h followed by content in one read.
        let mut terminal = Terminal::new(3, 20);
        terminal.put_str("before");
        terminal.put_bytes(b"\x1b[?1049hafter");
        // "before" stayed on normal screen, "after" landed on alt screen.
        assert_eq!(terminal.row_text(0).trim(), "after");

        terminal.put_str("\x1b[?1049l");
        assert_eq!(terminal.row_text(0).trim(), "before");
    }

    #[test]
    fn alt_screen_resize_preserves_both_screens() {
        let mut terminal = Terminal::new(3, 20);
        terminal.put_str("normal");
        terminal.put_str("\x1b[?1049h");
        terminal.put_str("alt");
        // Resize: both screens resize without panic.
        terminal.resize(5, 30);
        assert_eq!(terminal.screen().rows(), 5);
        assert_eq!(terminal.screen().cols(), 30);
        terminal.put_str("\x1b[?1049l");
        assert_eq!(terminal.screen().rows(), 5);
    }
}
