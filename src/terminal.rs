/// Terminal dimensions in character cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSize {
    /// Number of visible terminal rows.
    pub(crate) rows: usize,
    /// Number of visible terminal columns.
    pub(crate) cols: usize,
}

/// One terminal grid cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    pub(crate) ch: char,
}

impl Default for Cell {
    fn default() -> Self {
        Self { ch: ' ' }
    }
}

/// A small in-memory terminal grid with cursor state.
#[derive(Debug)]
pub(crate) struct Terminal {
    /// Visible row count; kept public inside the crate because renderers size buffers from it.
    pub(crate) rows: usize,
    /// Visible column count; each row is stored contiguously in `cells`.
    pub(crate) cols: usize,
    pub(crate) cursor_x: usize,
    pub(crate) cursor_y: usize,
    /// Row-major backing store indexed as `row * cols + col`.
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
            cells: vec![Cell::default(); rows * cols],
        }
    }

    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        assert!(rows > 0, "terminal rows must be non-zero");
        assert!(cols > 0, "terminal cols must be non-zero");
        if self.rows == rows && self.cols == cols {
            return;
        }

        // Preserve the top-left visible rectangle and leave newly exposed cells blank.
        // This keeps resize behavior deterministic without trying to emulate scrollback.
        let mut cells = vec![Cell::default(); rows * cols];
        let copied_rows = self.rows.min(rows);
        let copied_cols = self.cols.min(cols);
        for row in 0..copied_rows {
            let old_start = row * self.cols;
            let new_start = row * cols;
            cells[new_start..new_start + copied_cols]
                .copy_from_slice(&self.cells[old_start..old_start + copied_cols]);
        }

        self.rows = rows;
        self.cols = cols;
        self.cursor_y = self.cursor_y.min(rows - 1);
        self.cursor_x = self.cursor_x.min(cols - 1);
        self.cells = cells;
    }

    pub(crate) fn put_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.put_char(ch);
        }
    }

    pub(crate) fn put_char(&mut self, ch: char) {
        match ch {
            '\n' => self.newline(),
            '\r' => self.cursor_x = 0,
            '\u{8}' => self.backspace(),
            ch => self.write_char(ch),
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

    fn write_char(&mut self, ch: char) {
        let index = self.cursor_y * self.cols + self.cursor_x;
        self.cells[index].ch = ch;
        self.cursor_x += 1;
        if self.cursor_x == self.cols {
            self.cursor_x = 0;
            self.newline();
        }
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
    fn resize_preserves_visible_cells_and_clamps_cursor() {
        let mut terminal = Terminal::new(2, 4);
        terminal.put_str("abcdef");

        terminal.resize(1, 3);

        assert_eq!(terminal.row_text(0), "abc");
        assert_eq!((terminal.rows, terminal.cols), (1, 3));
        assert_eq!((terminal.cursor_x, terminal.cursor_y), (2, 0));
    }
}
