use std::sync::Arc;
use parking_lot::Mutex;

pub(crate) type LockedTerminal = Arc<Mutex<Terminal>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnsiColor {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParserState {
    Normal,
    Escape,
    Csi,
    Osc,
}

/// One terminal grid cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    pub(crate) ch: char,
    pub(crate) fg_color: AnsiColor,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg_color: AnsiColor::Default,
        }
    }
}

/// A small in-memory terminal grid with cursor state.
#[derive(Debug)]
pub(crate) struct Terminal {
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) cursor_x: usize,
    pub(crate) cursor_y: usize,
    pub(crate) cursor_visible: bool,
    pub(crate) active_fg: AnsiColor,
    pub(crate) parser_state: ParserState,
    csi_params: Vec<u32>,
    current_param: Option<u32>,
    cells: Vec<Cell>,
}

impl Terminal {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        assert!(rows > 0, "terminal rows must be non-zero");
        assert!(cols > 0, "terminal cols must be non-zero");

        Self {
            rows,
            cols,
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            active_fg: AnsiColor::Default,
            parser_state: ParserState::Normal,
            csi_params: Vec::new(),
            current_param: None,
            cells: vec![Cell::default(); rows * cols],
        }
    }

    pub(crate) fn resize(&mut self, new_rows: usize, new_cols: usize) {
        assert!(new_rows > 0, "terminal rows must be non-zero");
        assert!(new_cols > 0, "terminal cols must be non-zero");

        let mut new_cells = vec![Cell::default(); new_rows * new_cols];

        let copy_rows = self.rows.min(new_rows);
        let copy_cols = self.cols.min(new_cols);
        for r in 0..copy_rows {
            let old_start = r * self.cols;
            let new_start = r * new_cols;
            new_cells[new_start..new_start + copy_cols]
                .copy_from_slice(&self.cells[old_start..old_start + copy_cols]);
        }

        self.rows = new_rows;
        self.cols = new_cols;
        self.cells = new_cells;

        self.cursor_x = self.cursor_x.min(self.cols - 1);
        self.cursor_y = self.cursor_y.min(self.rows - 1);
    }

    pub(crate) fn put_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.put_char(ch);
        }
    }

    pub(crate) fn put_char(&mut self, ch: char) {
        match self.parser_state {
            ParserState::Normal => match ch {
                '\x1b' => self.parser_state = ParserState::Escape,
                '\n' => self.newline(),
                '\r' => self.cursor_x = 0,
                '\u{8}' => self.backspace(),
                '\t' => {
                    let next_tab = ((self.cursor_x / 8) + 1) * 8;
                    self.cursor_x = next_tab.min(self.cols - 1);
                }
                ch if ch.is_ascii_control() => {
                    // Ignore unhandled control characters (like \x7f)
                }
                ch => self.write_char(ch),
            },
            ParserState::Escape => {
                if ch == '[' {
                    self.parser_state = ParserState::Csi;
                    self.csi_params.clear();
                    self.current_param = None;
                } else if ch == ']' {
                    self.parser_state = ParserState::Osc;
                } else {
                    self.parser_state = ParserState::Normal;
                }
            }
            ParserState::Csi => {
                if ch.is_ascii_digit() {
                    let val = ch.to_digit(10).unwrap();
                    let current = self.current_param.unwrap_or(0);
                    self.current_param = Some(current.saturating_mul(10).saturating_add(val));
                } else if ch == ';' {
                    if self.csi_params.len() < 32 {
                        self.csi_params.push(self.current_param.unwrap_or(0));
                    }
                    self.current_param = None;
                } else if (0x3C..=0x3F).contains(&(ch as u8)) {
                    // Allow parameter prefixes/modifiers like '?', '>', '<', '=' and stay in Csi state.
                } else if (0x40..=0x7E).contains(&(ch as u8)) {
                    if let Some(param) = self.current_param {
                        if self.csi_params.len() < 32 {
                            self.csi_params.push(param);
                        }
                    }
                    self.execute_csi(ch);
                    self.parser_state = ParserState::Normal;
                    self.csi_params.clear();
                    self.current_param = None;
                } else {
                    self.parser_state = ParserState::Normal;
                    self.csi_params.clear();
                    self.current_param = None;
                }
            }
            ParserState::Osc => {
                if ch == '\x07' {
                    self.parser_state = ParserState::Normal;
                } else if ch == '\x1b' {
                    self.parser_state = ParserState::Escape;
                }
            }
        }
    }

    fn execute_csi(&mut self, cmd: char) {
        match cmd {
            'm' => {
                if self.csi_params.is_empty() {
                    self.active_fg = AnsiColor::Default;
                } else {
                    for &param in &self.csi_params {
                        match param {
                            0 => self.active_fg = AnsiColor::Default,
                            30 => self.active_fg = AnsiColor::Black,
                            31 => self.active_fg = AnsiColor::Red,
                            32 => self.active_fg = AnsiColor::Green,
                            33 => self.active_fg = AnsiColor::Yellow,
                            34 => self.active_fg = AnsiColor::Blue,
                            35 => self.active_fg = AnsiColor::Magenta,
                            36 => self.active_fg = AnsiColor::Cyan,
                            37 => self.active_fg = AnsiColor::White,
                            39 => self.active_fg = AnsiColor::Default,
                            90 => self.active_fg = AnsiColor::BrightBlack,
                            91 => self.active_fg = AnsiColor::BrightRed,
                            92 => self.active_fg = AnsiColor::BrightGreen,
                            93 => self.active_fg = AnsiColor::BrightYellow,
                            94 => self.active_fg = AnsiColor::BrightBlue,
                            95 => self.active_fg = AnsiColor::BrightMagenta,
                            96 => self.active_fg = AnsiColor::BrightCyan,
                            97 => self.active_fg = AnsiColor::BrightWhite,
                            _ => {}
                        }
                    }
                }
            }
            'H' | 'f' => {
                let row = self.csi_params.first().copied().unwrap_or(1);
                let col = self.csi_params.get(1).copied().unwrap_or(1);
                let r = (row.saturating_sub(1) as usize).min(self.rows - 1);
                let c = (col.saturating_sub(1) as usize).min(self.cols - 1);
                self.cursor_y = r;
                self.cursor_x = c;
            }
            'A' => {
                let step = self.csi_params.first().copied().unwrap_or(1) as usize;
                self.cursor_y = self.cursor_y.saturating_sub(step).min(self.rows - 1);
            }
            'B' => {
                let step = self.csi_params.first().copied().unwrap_or(1) as usize;
                self.cursor_y = (self.cursor_y + step).min(self.rows - 1);
            }
            'C' => {
                let step = self.csi_params.first().copied().unwrap_or(1) as usize;
                self.cursor_x = (self.cursor_x + step).min(self.cols - 1);
            }
            'D' => {
                let step = self.csi_params.first().copied().unwrap_or(1) as usize;
                self.cursor_x = self.cursor_x.saturating_sub(step).min(self.cols - 1);
            }
            'J' => {
                let mode = self.csi_params.first().copied().unwrap_or(0);
                match mode {
                    0 => {
                        let start = self.cursor_y * self.cols + self.cursor_x;
                        if start < self.cells.len() {
                            self.cells[start..].fill(Cell::default());
                        }
                    }
                    1 => {
                        let end = (self.cursor_y * self.cols + self.cursor_x).min(self.cells.len() - 1);
                        self.cells[..=end].fill(Cell::default());
                    }
                    2 | 3 => {
                        self.cells.fill(Cell::default());
                        self.cursor_x = 0;
                        self.cursor_y = 0;
                    }
                    _ => {}
                }
            }
            'K' => {
                let mode = self.csi_params.first().copied().unwrap_or(0);
                let row_start = self.cursor_y * self.cols;
                match mode {
                    0 => {
                        let start = row_start + self.cursor_x;
                        let end = row_start + self.cols;
                        self.cells[start..end].fill(Cell::default());
                    }
                    1 => {
                        let start = row_start;
                        let end = row_start + self.cursor_x + 1;
                        self.cells[start..end].fill(Cell::default());
                    }
                    2 => {
                        let start = row_start;
                        let end = row_start + self.cols;
                        self.cells[start..end].fill(Cell::default());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub(crate) fn row_text(&self, row: usize) -> String {
        assert!(row < self.rows, "terminal row out of bounds");
        let start = row * self.cols;
        self.cells[start..start + self.cols]
            .iter()
            .map(|cell| cell.ch)
            .collect()
    }

    pub(crate) fn rows_text(&self) -> impl Iterator<Item = String> + '_ {
        (0..self.rows).map(|row| self.row_text(row))
    }

    pub(crate) fn row_cells(&self, row: usize) -> &[Cell] {
        assert!(row < self.rows, "terminal row out of bounds");
        let start = row * self.cols;
        &self.cells[start..start + self.cols]
    }

    fn write_char(&mut self, ch: char) {
        if self.cursor_x >= self.cols {
            self.newline();
        }

        let index = self.cursor_y * self.cols + self.cursor_x;
        self.cells[index] = Cell {
            ch,
            fg_color: self.active_fg,
        };
        self.cursor_x += 1;
    }

    fn newline(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y + 1 == self.rows {
            self.scroll_up();
        } else {
            self.cursor_y += 1;
        }
    }

    fn backspace(&mut self) {
        if self.cursor_x == 0 {
            return;
        }
        self.cursor_x -= 1;
        let index = self.cursor_y * self.cols + self.cursor_x;
        self.cells[index] = Cell::default();
    }

    fn scroll_up(&mut self) {
        self.cells.copy_within(self.cols.., 0);
        let first_blank = (self.rows - 1) * self.cols;
        self.cells[first_blank..].fill(Cell::default());
        self.cursor_y = self.rows - 1;
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
        assert_eq!((terminal.rows, terminal.cols), (2, 4));
        assert_eq!((terminal.cursor_x, terminal.cursor_y), (2, 0));
    }

    #[test]
    fn newline_moves_to_next_row_start() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("a\nb");

        assert_eq!(terminal.row_text(0), "a   ");
        assert_eq!(terminal.row_text(1), "b   ");
        assert_eq!((terminal.cursor_x, terminal.cursor_y), (1, 1));
    }

    #[test]
    fn carriage_return_overwrites_from_row_start() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("ab\rc");

        assert_eq!(terminal.row_text(0), "cb  ");
        assert_eq!((terminal.cursor_x, terminal.cursor_y), (1, 0));
    }

    #[test]
    fn backspace_erases_previous_cell() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_str("ab\u{8}c");

        assert_eq!(terminal.row_text(0), "ac  ");
        assert_eq!((terminal.cursor_x, terminal.cursor_y), (2, 0));
    }

    #[test]
    fn scrolls_when_writing_past_last_row() {
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("one\ntwo\nthr");

        assert_eq!(terminal.row_text(0), "two ");
        assert_eq!(terminal.row_text(1), "thr ");
        assert_eq!((terminal.cursor_x, terminal.cursor_y), (3, 1));
    }

    #[test]
    fn resizes_grid_and_retains_content() {
        let mut terminal = Terminal::new(2, 4);
        terminal.put_str("one\ntwo");
        assert_eq!(terminal.row_text(0), "one ");
        assert_eq!(terminal.row_text(1), "two ");

        terminal.resize(3, 5);
        assert_eq!(terminal.row_text(0), "one  ");
        assert_eq!(terminal.row_text(1), "two  ");
        assert_eq!(terminal.row_text(2), "     ");

        terminal.resize(1, 3);
        assert_eq!(terminal.row_text(0), "one");
    }

    #[test]
    fn parses_ansi_colors_and_csi_commands() {
        use super::AnsiColor;
        let mut terminal = Terminal::new(3, 10);

        // Test SGR colors
        terminal.put_str("a\x1b[31mb\x1b[92mc\x1b[0md\x7f");
        assert_eq!(terminal.row_text(0), "abcd      ");
        assert_eq!(terminal.row_cells(0)[0].fg_color, AnsiColor::Default);
        assert_eq!(terminal.row_cells(0)[1].fg_color, AnsiColor::Red);
        assert_eq!(terminal.row_cells(0)[2].fg_color, AnsiColor::BrightGreen);
        assert_eq!(terminal.row_cells(0)[3].fg_color, AnsiColor::Default);

        // Test CUP (Cursor Position)
        terminal.put_str("\x1b[2;5fX");
        // 2nd row (index 1), 5th col (index 4) should be 'X'
        assert_eq!(terminal.row_text(1), "    X     ");

        // Test EL (Erase in Line)
        terminal.put_str("\x1b[2;5H\x1b[K"); // Cursor at 2;5, clear to end of line
        assert_eq!(terminal.row_text(1), "          ");

        // Test ED (Erase in Display)
        terminal.put_str("\x1b[2J"); // Clear entire screen and home
        assert_eq!(terminal.row_text(0), "          ");
        assert_eq!(terminal.row_text(1), "          ");
        assert_eq!((terminal.cursor_x, terminal.cursor_y), (0, 0));
    }

    #[test]
    fn ignores_osc_escape_sequences() {
        let mut terminal = Terminal::new(2, 10);

        // OSC sequence terminated by \x07 (Bell)
        terminal.put_str("abc\x1b]2;title\x07def");
        assert_eq!(terminal.row_text(0), "abcdef    ");

        // OSC sequence terminated by \x1b\ (String Terminator)
        terminal.put_str("\r123\x1b]133;A;cl=m\x1b\\456");
        assert_eq!(terminal.row_text(0), "123456    ");
    }

    #[test]
    fn ignores_dec_private_mode_csi_sequences() {
        let mut terminal = Terminal::new(2, 10);

        // CSI sequence with private mode prefix '?' (e.g. bracketed paste)
        terminal.put_str("abc\x1b[?2004hdef\x1b[?1lghi");
        assert_eq!(terminal.row_text(0), "abcdefghi ");
    }
}
