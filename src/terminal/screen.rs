use unicode_width::UnicodeWidthChar;

/// Terminal color value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Color {
    /// Use the terminal's default foreground/background.
    Default,
    /// Standard ANSI colors 0-7.
    Named(u8),
    /// Bright ANSI colors 0-7 (rendered as palette entries 8-15).
    Bright(u8),
    /// 256-color palette index 0-255.
    Indexed(u8),
    /// Truecolor RGB.
    Rgb(u8, u8, u8),
}

/// Text style attributes, stored as a compact bitset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CellAttrs(u8);

impl CellAttrs {
    pub(crate) const BOLD: u8 = 1 << 0;
    pub(crate) const DIM: u8 = 1 << 1;
    pub(crate) const ITALIC: u8 = 1 << 2;
    pub(crate) const UNDERLINE: u8 = 1 << 3;
    pub(crate) const BLINK: u8 = 1 << 4;
    pub(crate) const INVERSE: u8 = 1 << 5;
    pub(crate) const STRIKETHROUGH: u8 = 1 << 6;

    #[allow(dead_code)]
    pub(crate) fn contains(self, bits: u8) -> bool {
        self.0 & bits != 0
    }
    pub(crate) fn set(&mut self, bits: u8) {
        self.0 |= bits;
    }
    pub(crate) fn clear(&mut self, bits: u8) {
        self.0 &= !bits;
    }
    #[allow(dead_code)]
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One visible terminal grid cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    /// Character currently displayed in this cell.
    pub(crate) ch: char,
    /// True when this cell is the hidden trailing half of a double-width character.
    wide_continuation: bool,
    /// Foreground color.
    pub(crate) fg: Color,
    /// Background color.
    pub(crate) bg: Color,
    /// Text style attributes.
    pub(crate) attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            wide_continuation: false,
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
        }
    }
}

/// Visible terminal screen state rendered by the text pipeline.
///
/// `Screen` owns only display state: cell contents, dimensions, and cursor position. It does not
/// parse byte streams; `TerminalParser` calls these methods after recognizing control sequences.
#[derive(Debug)]
pub(crate) struct Screen {
    /// Number of visible rows in the terminal grid.
    rows: usize,
    /// Number of visible columns in the terminal grid.
    cols: usize,
    /// 0-based column within the current row.
    cursor_x: usize,
    /// 0-based row index.
    cursor_y: usize,
    /// Row-major backing store indexed as `row * cols + col`.
    cells: Vec<Cell>,
    /// Tracks which rows were modified since last clear.
    dirty_rows: Vec<bool>,
    /// Current SGR foreground applied to each character written.
    current_fg: Color,
    /// Current SGR background applied to each character written.
    current_bg: Color,
    /// Current SGR attributes applied to each character written.
    current_attrs: CellAttrs,
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
            dirty_rows: vec![true; rows],
            current_fg: Color::Default,
            current_bg: Color::Default,
            current_attrs: CellAttrs::default(),
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

    /// Returns the character at the given `(row, col)` grid position.
    pub(crate) fn cell_char(&self, row: usize, col: usize) -> char {
        self.cells[row * self.cols + col].ch
    }

    /// Returns a reference to the cell at the given grid position.
    #[allow(dead_code)]
    pub(crate) fn cell(&self, row: usize, col: usize) -> &Cell {
        &self.cells[row * self.cols + col]
    }

    /// Applies SGR (Select Graphic Rendition) parameters to the current pen state.
    ///
    /// `None` parameters are treated as reset (same as `0`). Multi-parameter sub-sequences
    /// (`38`/`48`) are validated fully; on partial or out-of-range params the pen state is
    /// left unchanged.
    pub(crate) fn set_sgr(&mut self, params: &[Option<usize>]) {
        let mut i = 0usize;
        while i < params.len() {
            let n = params[i].unwrap_or_default();
            match n {
                0 => {
                    self.current_fg = Color::Default;
                    self.current_bg = Color::Default;
                    self.current_attrs = CellAttrs::default();
                }
                1 => self.current_attrs.set(CellAttrs::BOLD),
                2 => self.current_attrs.set(CellAttrs::DIM),
                3 => self.current_attrs.set(CellAttrs::ITALIC),
                4 => self.current_attrs.set(CellAttrs::UNDERLINE),
                5 => self.current_attrs.set(CellAttrs::BLINK),
                7 => self.current_attrs.set(CellAttrs::INVERSE),
                9 => self.current_attrs.set(CellAttrs::STRIKETHROUGH),
                22 => self.current_attrs.clear(CellAttrs::BOLD | CellAttrs::DIM),
                23 => self.current_attrs.clear(CellAttrs::ITALIC),
                24 => self.current_attrs.clear(CellAttrs::UNDERLINE),
                25 => self.current_attrs.clear(CellAttrs::BLINK),
                27 => self.current_attrs.clear(CellAttrs::INVERSE),
                29 => self.current_attrs.clear(CellAttrs::STRIKETHROUGH),
                30..=37 => self.current_fg = Color::Named((n - 30) as u8),
                40..=47 => self.current_bg = Color::Named((n - 40) as u8),
                39 => self.current_fg = Color::Default,
                49 => self.current_bg = Color::Default,
                90..=97 => self.current_fg = Color::Bright((n - 90) as u8),
                100..=107 => self.current_bg = Color::Bright((n - 100) as u8),
                38 | 48 => {
                    let is_fg = n == 38;
                    if i + 1 >= params.len() {
                        break;
                    }
                    let sub = params[i + 1].unwrap_or_default();
                    match sub {
                        5 => {
                            // 256-color: 38;5;N  or  48;5;N
                            if i + 2 >= params.len() {
                                break;
                            }
                            if let Some(val) = params[i + 2]
                                && val <= 255
                            {
                                if is_fg {
                                    self.current_fg = Color::Indexed(val as u8);
                                } else {
                                    self.current_bg = Color::Indexed(val as u8);
                                }
                            }
                            i += 2;
                        }
                        2 => {
                            // Truecolor: 38;2;R;G;B  or  48;2;R;G;B
                            if i + 4 >= params.len() {
                                break;
                            }
                            if let (Some(r), Some(g), Some(b)) =
                                (params[i + 2], params[i + 3], params[i + 4])
                                && r <= 255
                                && g <= 255
                                && b <= 255
                            {
                                if is_fg {
                                    self.current_fg = Color::Rgb(r as u8, g as u8, b as u8);
                                } else {
                                    self.current_bg = Color::Rgb(r as u8, g as u8, b as u8);
                                }
                            }
                            i += 4;
                        }
                        _ => {
                            // Unknown sub-type; consume sub only (i+1).
                            i += 1;
                        }
                    }
                }
                _ => { /* unknown SGR code — silently ignore */ }
            }
            i += 1;
        }
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
        self.dirty_rows = vec![true; rows];
    }

    /// Yields indices of rows modified since last `clear_dirty()`.
    pub(crate) fn dirty_rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.dirty_rows
            .iter()
            .enumerate()
            .filter_map(|(i, &d)| d.then_some(i))
    }

    /// Resets all dirty flags to false.
    pub(crate) fn clear_dirty(&mut self) {
        self.dirty_rows.fill(false);
    }

    fn mark_row_dirty(&mut self, row: usize) {
        self.dirty_rows[row] = true;
    }

    fn mark_all_dirty(&mut self) {
        self.dirty_rows.fill(true);
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
        self.mark_row_dirty(self.cursor_y);

        let index = self.cursor_y * self.cols + self.cursor_x;
        self.clear_cell_for_write(index);
        if width == 2 && self.cursor_x + 1 < self.cols {
            self.clear_cell_for_write(index + 1);
        }

        self.cells[index].ch = ch;
        self.cells[index].wide_continuation = false;
        self.cells[index].fg = self.current_fg;
        self.cells[index].bg = self.current_bg;
        self.cells[index].attrs = self.current_attrs;
        if width == 2 && self.cursor_x + 1 < self.cols {
            self.cells[index + 1] = Cell {
                ch: ' ',
                wide_continuation: true,
                fg: self.current_fg,
                bg: self.current_bg,
                attrs: self.current_attrs,
            };
        }

        self.cursor_x += width;
        if self.cursor_x >= self.cols {
            self.cursor_x = 0;
            self.newline();
        }
    }

    /// Clears the target cell *and* any joined cell from a double-width glyph that overlaps it.
    ///
    /// Three cases are handled:
    /// 1. The target is a wide-continuation cell → clear both halves (the base at `index - 1`
    ///    and this continuation cell).
    /// 2. The target itself is the start of a double-width glyph that extends into the next
    ///    column → clear both cells.
    /// 3. Otherwise → clear only the target cell.
    fn clear_cell_for_write(&mut self, index: usize) {
        debug_assert!(
            index > 0 || !self.cells[index].wide_continuation,
            "wide_continuation at column 0 is invalid"
        );
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
    pub(crate) fn erase_display(&mut self, mode: usize) {
        let cursor = self.cursor_y * self.cols + self.cursor_x;
        match mode {
            0 => {
                self.cells[cursor..].fill(Cell::default());
                for row in self.cursor_y..self.rows {
                    self.mark_row_dirty(row);
                }
            }
            1 => {
                self.cells[..=cursor].fill(Cell::default());
                for row in 0..=self.cursor_y {
                    self.mark_row_dirty(row);
                }
            }
            2 => {
                self.cells.fill(Cell::default());
                self.cursor_x = 0;
                self.cursor_y = 0;
                self.mark_all_dirty();
            }
            _ => {}
        }
    }

    /// Implements CSI `K` erase-line modes for the current row.
    pub(crate) fn erase_line(&mut self, mode: usize) {
        let start = self.cursor_y * self.cols;
        let cursor = start + self.cursor_x;
        let end = start + self.cols;
        self.mark_row_dirty(self.cursor_y);
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
        self.mark_all_dirty();
    }

    /// Implements reverse index (`ESC M`): move up, or scroll the visible grid down at row 0.
    pub(crate) fn reverse_index(&mut self) {
        if self.cursor_y == 0 {
            self.mark_all_dirty();
            // Shift rows 0..rows-1 down by one row, then blank the top row.
            let len = (self.rows - 1) * self.cols;
            self.cells.copy_within(0..len, self.cols);
            self.cells[..self.cols].fill(Cell::default());
        } else {
            self.cursor_y -= 1;
        }
    }

    /// Scrolls the visible grid up by one row: shifts every row upward (row N → N-1) and
    /// fills the newly exposed bottom row with blank cells. The cursor stays on the bottom row.
    fn scroll_up(&mut self) {
        self.mark_all_dirty();
        self.cells.copy_within(self.cols.., 0);
        let first_blank = (self.rows - 1) * self.cols;
        self.cells[first_blank..].fill(Cell::default());
        self.cursor_y = self.rows - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_char_marks_its_row() {
        let mut screen = Screen::new(3, 4);
        // After new, all rows are dirty.
        screen.clear_dirty();
        screen.write_char('x');
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(dirty, vec![0], "write_char at row 0 should mark row 0");
    }

    #[test]
    fn clear_dirty_resets_all() {
        let mut screen = Screen::new(2, 4);
        // All rows initially dirty.
        assert_eq!(screen.dirty_rows().count(), 2);
        screen.clear_dirty();
        assert_eq!(screen.dirty_rows().count(), 0);
    }

    #[test]
    fn scroll_up_marks_all_rows() {
        let mut screen = Screen::new(3, 4);
        // Fill all rows, then scroll up via newline on bottom row.
        screen.clear_dirty();
        screen.cursor_y = 2; // bottom row
        screen.newline(); // triggers scroll_up
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(dirty.len(), 3, "scroll_up should mark all rows");
    }

    #[test]
    fn erase_line_marks_cursor_row() {
        let mut screen = Screen::new(4, 4);
        screen.cursor_y = 2;
        screen.clear_dirty();
        screen.erase_line(2);
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(dirty, vec![2], "erase_line should mark only cursor row");
    }

    #[test]
    fn cursor_movement_does_not_mark_dirty() {
        let mut screen = Screen::new(3, 4);
        screen.clear_dirty();
        screen.cursor_up(1);
        screen.cursor_down(1);
        screen.cursor_left(1);
        screen.cursor_right(1);
        screen.carriage_return();
        screen.set_cursor(2, 2);
        assert_eq!(
            screen.dirty_rows().count(),
            0,
            "cursor-only ops should not mark rows dirty"
        );
    }

    #[test]
    fn erase_display_mode_0_marks_from_cursor_to_end() {
        let mut screen = Screen::new(5, 4);
        screen.cursor_y = 2;
        screen.clear_dirty();
        screen.erase_display(0);
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(
            dirty,
            vec![2, 3, 4],
            "erase_display(0) should mark rows cursor..end"
        );
    }

    #[test]
    fn erase_display_mode_1_marks_from_start_to_cursor() {
        let mut screen = Screen::new(5, 4);
        screen.cursor_y = 2;
        screen.clear_dirty();
        screen.erase_display(1);
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(
            dirty,
            vec![0, 1, 2],
            "erase_display(1) should mark rows 0..=cursor"
        );
    }

    #[test]
    fn erase_display_mode_2_marks_all() {
        let mut screen = Screen::new(5, 4);
        screen.clear_dirty();
        screen.erase_display(2);
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(dirty.len(), 5, "erase_display(2) should mark all rows");
    }

    #[test]
    fn reset_display_marks_all_rows() {
        let mut screen = Screen::new(3, 4);
        screen.clear_dirty();
        screen.reset_display();
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(dirty.len(), 3, "reset_display should mark all rows");
    }

    #[test]
    fn reverse_index_scroll_marks_all_rows() {
        let mut screen = Screen::new(3, 4);
        screen.clear_dirty();
        screen.cursor_y = 0;
        screen.reverse_index(); // triggers scroll
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(
            dirty.len(),
            3,
            "reverse_index with scroll should mark all rows"
        );
    }

    #[test]
    fn backspace_does_not_mark_dirty() {
        let mut screen = Screen::new(1, 4);
        screen.cursor_x = 2;
        screen.clear_dirty();
        screen.backspace();
        assert_eq!(
            screen.dirty_rows().count(),
            0,
            "backspace should not mark rows dirty"
        );
    }

    #[test]
    fn newline_no_scroll_does_not_mark_dirty() {
        let mut screen = Screen::new(3, 4);
        screen.cursor_y = 1;
        screen.clear_dirty();
        screen.newline();
        assert_eq!(
            screen.dirty_rows().count(),
            0,
            "newline without scroll should not mark dirty"
        );
    }

    #[test]
    fn resize_rebuilds_dirty_all_true() {
        let mut screen = Screen::new(2, 4);
        screen.clear_dirty();
        screen.resize(4, 4);
        let dirty: Vec<usize> = screen.dirty_rows().collect();
        assert_eq!(
            dirty.len(),
            4,
            "resize should rebuild dirty_rows with all true"
        );
    }
}
