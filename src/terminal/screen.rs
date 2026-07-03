use unicode_width::UnicodeWidthChar;

/// One visible terminal grid cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    /// Character currently displayed in this cell. Styling is intentionally not stored yet.
    pub(crate) ch: char,
    /// True when this cell is the hidden trailing half of a double-width character.
    wide_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            wide_continuation: false,
        }
    }
}

/// Visible terminal screen state rendered by the text pipeline.
///
/// `Screen` owns only display state: cell contents, dimensions, and cursor position. It does not
/// parse byte streams; `TerminalParser` calls these methods after recognizing control sequences.
#[derive(Debug)]
pub(crate) struct Screen {
    rows: usize,
    cols: usize,
    cursor_x: usize,
    cursor_y: usize,
    /// Row-major backing store indexed as `row * cols + col`.
    cells: Vec<Cell>,
}

impl Screen {
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

    // ── read-only accessors ─────────────────────────────────────────

    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    pub(crate) fn cursor_x(&self) -> usize {
        self.cursor_x
    }

    pub(crate) fn cursor_y(&self) -> usize {
        self.cursor_y
    }

    /// Iterates all visible cells as `(row, col, ch)` in row-major order.
    pub(crate) fn cells(&self) -> impl Iterator<Item = (usize, usize, char)> + '_ {
        self.cells
            .iter()
            .enumerate()
            .map(|(i, cell)| (i / self.cols, i % self.cols, cell.ch))
    }

    /// Resizes the visible grid while preserving the top-left rectangle of existing cells.
    ///
    /// Resize does not touch parser state. Newly exposed cells are blank, and the cursor is
    /// clamped into the new bounds.
    pub(crate) fn resize(&mut self, rows: usize, cols: usize) {
        assert!(rows > 0, "terminal rows must be non-zero");
        assert!(cols > 0, "terminal cols must be non-zero");
        if self.rows == rows && self.cols == cols {
            return;
        }

        // Preserve the top-left visible rectangle and leave newly exposed cells blank.
        // This avoids scrollback semantics in the render grid and keeps resize deterministic.
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

    #[cfg(test)]
    pub(crate) fn row_text(&self, row: usize) -> String {
        assert!(row < self.rows, "terminal row out of bounds");
        let start = row * self.cols;
        self.cells[start..start + self.cols]
            .iter()
            .map(|cell| cell.ch)
            .collect()
    }

    // ── semantic cursor / display operations ────────────────────────

    /// Resets `cursor_x` to column 0, implementing the carriage-return (`\r`) semantics.
    pub(crate) fn carriage_return(&mut self) {
        self.cursor_x = 0;
    }

    pub(crate) fn cursor_up(&mut self, n: usize) {
        self.cursor_y = self.cursor_y.saturating_sub(n);
    }

    pub(crate) fn cursor_down(&mut self, n: usize) {
        self.cursor_y = self.cursor_y.saturating_add(n).min(self.rows - 1);
    }

    pub(crate) fn cursor_left(&mut self, n: usize) {
        self.cursor_x = self.cursor_x.saturating_sub(n);
    }

    pub(crate) fn cursor_right(&mut self, n: usize) {
        self.cursor_x = self.cursor_x.saturating_add(n).min(self.cols - 1);
    }

    /// Expands horizontal tab into spaces up to the next 8-column tab stop or row end.
    pub(crate) fn horizontal_tab(&mut self) {
        let target = ((self.cursor_x / 8) + 1).saturating_mul(8).min(self.cols);
        let spaces = target.saturating_sub(self.cursor_x);
        for _ in 0..spaces {
            self.write_char(' ');
        }
    }

    /// Writes one already-decoded printable character at the cursor and advances by its terminal
    /// cell width.
    pub(crate) fn write_char(&mut self, ch: char) {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0).min(2);
        if width == 0 {
            return;
        }

        if width == 2 && self.cursor_x + 1 >= self.cols {
            self.newline();
        }

        let index = self.cursor_y * self.cols + self.cursor_x;
        self.clear_cell_for_write(index);
        if width == 2 && self.cursor_x + 1 < self.cols {
            self.clear_cell_for_write(index + 1);
        }

        self.cells[index].ch = ch;
        self.cells[index].wide_continuation = false;
        if width == 2 && self.cursor_x + 1 < self.cols {
            self.cells[index + 1] = Cell {
                ch: ' ',
                wide_continuation: true,
            };
        }

        self.cursor_x += width;
        if self.cursor_x >= self.cols {
            self.cursor_x = 0;
            self.newline();
        }
    }

    /// Clears a target cell and any joined cell from an existing double-width glyph.
    fn clear_cell_for_write(&mut self, index: usize) {
        if self.cells[index].wide_continuation {
            self.cells[index - 1] = Cell::default();
            self.cells[index] = Cell::default();
            return;
        }

        if UnicodeWidthChar::width(self.cells[index].ch).unwrap_or(0) == 2
            && index % self.cols + 1 < self.cols
        {
            self.cells[index + 1] = Cell::default();
        }
        self.cells[index] = Cell::default();
    }

    /// Moves to the start of the next row, scrolling up when already on the bottom row.
    pub(crate) fn newline(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y + 1 == self.rows {
            self.scroll_up();
        } else {
            self.cursor_y += 1;
        }
    }

    /// VT non-destructive backspace: move cursor left, skipping wide-continuation cells.
    pub(crate) fn backspace(&mut self) {
        if self.cursor_x == 0 {
            return;
        }
        self.cursor_x -= 1;

        // Step past the continuation half of a wide glyph so the cursor
        // lands at the start column rather than sitting mid-glyph.
        let index = self.cursor_y * self.cols + self.cursor_x;
        if self.cells[index].wide_continuation && self.cursor_x > 0 {
            self.cursor_x -= 1;
        }
    }

    /// Positions the cursor from 1-based ANSI coordinates, clamped to the visible grid.
    pub(crate) fn set_cursor(&mut self, row_1_based: usize, col_1_based: usize) {
        self.cursor_y = row_1_based.saturating_sub(1).min(self.rows - 1);
        self.cursor_x = col_1_based.saturating_sub(1).min(self.cols - 1);
    }

    /// Implements CSI `J` erase-display modes that affect visible cells.
    ///
    /// Mode 0 clears from the cursor through the end, mode 1 clears through the cursor, and mode 2
    /// clears the whole screen and homes the cursor.
    pub(crate) fn erase_display(&mut self, mode: usize) {
        let cursor = self.cursor_y * self.cols + self.cursor_x;
        match mode {
            0 => self.cells[cursor..].fill(Cell::default()),
            1 => self.cells[..=cursor].fill(Cell::default()),
            2 => {
                self.cells.fill(Cell::default());
                self.cursor_x = 0;
                self.cursor_y = 0;
            }
            _ => {}
        }
    }

    /// Implements CSI `K` erase-line modes for the current row.
    pub(crate) fn erase_line(&mut self, mode: usize) {
        let start = self.cursor_y * self.cols;
        let cursor = start + self.cursor_x;
        let end = start + self.cols;
        match mode {
            0 => self.cells[cursor..end].fill(Cell::default()),
            1 => self.cells[start..=cursor].fill(Cell::default()),
            2 => self.cells[start..end].fill(Cell::default()),
            _ => {}
        }
    }

    /// Clears all visible cells and homes the cursor.
    pub(crate) fn reset_display(&mut self) {
        self.cells.fill(Cell::default());
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Implements reverse index (`ESC M`): move up, or scroll the visible grid down at row 0.
    pub(crate) fn reverse_index(&mut self) {
        if self.cursor_y == 0 {
            let len = (self.rows - 1) * self.cols;
            self.cells.copy_within(0..len, self.cols);
            self.cells[..self.cols].fill(Cell::default());
        } else {
            self.cursor_y -= 1;
        }
    }

    /// Scrolls the visible grid up by one row and blanks the last row.
    fn scroll_up(&mut self) {
        self.cells.copy_within(self.cols.., 0);
        let first_blank = (self.rows - 1) * self.cols;
        self.cells[first_blank..].fill(Cell::default());
        self.cursor_y = self.rows - 1;
    }
}
