//! Pure domain model for text selection state.
//!
//! Owns anchor/cursor tracking, granularity, click-chain detection, and
//! auto-scroll scheduling.  No GPU or window dependencies — testable without
//! a rendering context.

use harbor_types::{SelectionBounds, TerminalSnapshot};
use std::time::{Duration, Instant};

/// Characters that delimit words for double-click word selection.
/// Based on Alacritty's default separator set, extended for CJK punctuation.
const WORD_SEPARATORS: &[char] = &[
    ' ', '\t', '\n', '(', ')', '[', ']', '{', '}', '\'', '"', '`',
    // CJK brackets and punctuation
    '（', '）', '【', '】', '「', '」', '『', '』', '《', '》', '；', '：', '，', '。', '、', '？',
    '！', '‘', '’', '＂',
];

/// Maximum time between consecutive clicks (in ms) for them to count as a
/// multi-click chain (double-click → word, triple-click+ → line).
const MULTI_CLICK_TIMEOUT_MS: u64 = 500;

const AUTO_SCROLL_MARGIN: usize = 3;
const AUTO_SCROLL_INTERVAL_MS: u64 = 16;

/// Returns true if `ch` is a CJK character that gets special word-selection
/// semantics: single-character word boundary during word-wise drag, but
/// consecutive grouping during initial word finding.
fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}'  // CJK Unified Ideographs Extension A
        | '\u{20000}'..='\u{2A6DF}' // CJK Unified Ideographs Extension B
        | '\u{3040}'..='\u{309F}'  // Hiragana
        | '\u{30A0}'..='\u{30FF}'  // Katakana
        | '\u{AC00}'..='\u{D7AF}'  // Hangul Syllables
    )
}

/// The semantic granularity of an active selection, determined by click count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionGranularity {
    /// Free character-level selection (single click + drag).
    #[default]
    Character,
    /// Word-level selection (double-click + word-wise drag).
    Word,
    /// Line-level selection (triple-click + line-wise drag).
    Line,
}

/// Direction of ongoing auto-scroll while dragging at viewport edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoScroll {
    Up,   // into scrollback history
    Down, // toward live content
}

/// Outcome of a [`SelectionModel`] state transition.
///
/// Informs the outer [`Selection`] layer which side-effects to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionOutcome {
    /// No state change.
    None,
    /// Drag started — set dirty + request redraw + suppress PTY scroll snap.
    DragActive,
    /// Drag ended — set dirty + request redraw + re-enable PTY scroll snap.
    DragEnded,
}

/// Tracks the current text selection as a pair of grid coordinates.
/// `anchor` is where the drag started; `cursor` is the current drag endpoint.
/// Both use **generations** (stable scrollback coordinates).
#[derive(Clone, Copy, Debug)]
pub struct SelectionRange {
    pub anchor: (u64, usize), // (generation, col)
    pub cursor: (u64, usize), // (generation, col)
}

impl SelectionRange {
    /// Returns `(start_row, start_col, end_row, end_col)` in row-major reading order.
    /// Guarantees start ≤ end in row-major (generation-major).
    pub fn normalized(&self) -> (u64, usize, u64, usize) {
        if self.anchor.0 < self.cursor.0
            || (self.anchor.0 == self.cursor.0 && self.anchor.1 <= self.cursor.1)
        {
            (self.anchor.0, self.anchor.1, self.cursor.0, self.cursor.1)
        } else {
            (self.cursor.0, self.cursor.1, self.anchor.0, self.anchor.1)
        }
    }
}

/// Pure domain model for text selection state.
///
/// Owns anchor/cursor tracking, granularity, click-chain detection, and
/// auto-scroll scheduling.  No GPU or window dependencies — testable without
/// a rendering context.
#[derive(Clone, Debug)]
pub struct SelectionModel {
    /// None = no active selection.
    pub range: Option<SelectionRange>,
    /// True while left mouse button is held.
    dragging: bool,
    /// Current selection granularity (Character / Word / Line).
    granularity: SelectionGranularity,
    /// Timestamp of the most recent `MouseInput::Pressed` (for click-chain detection).
    last_click_at: Option<Instant>,
    /// Grid cell of the most recent click (gen, col).
    last_click_cell: Option<(u64, usize)>,
    /// Consecutive click count (1 = single, 2 = double, 3+ = triple/line).
    click_count: u32,
    /// Auto-scroll direction while dragging at viewport edge (None = idle).
    auto_scroll: Option<AutoScroll>,
    /// Deadline for the next auto-scroll tick (rate-limited to AUTO_SCROLL_INTERVAL_MS).
    next_auto_scroll_at: Option<Instant>,
}

impl Default for SelectionModel {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionModel {
    pub fn new() -> Self {
        Self {
            range: None,
            dragging: false,
            granularity: SelectionGranularity::default(),
            last_click_at: None,
            last_click_cell: None,
            click_count: 0,
            auto_scroll: None,
            next_auto_scroll_at: None,
        }
    }

    /// Left-mouse-button press at `cell`.
    ///
    /// Detects click chains (double/triple-click) and sets the selection range
    /// and granularity accordingly.
    pub fn press(
        &mut self,
        cell: (u64, usize),
        now: Instant,
        screen: &TerminalSnapshot,
    ) -> SelectionOutcome {
        // ── Click chain detection ──────────────────────────
        let in_timeout = self
            .last_click_at
            .is_some_and(|t| now.duration_since(t).as_millis() < MULTI_CLICK_TIMEOUT_MS as u128);
        let same_cell = self.last_click_cell == Some(cell);

        if in_timeout && same_cell {
            self.click_count = self.click_count.saturating_add(1);
        } else {
            self.click_count = 1;
        }
        self.last_click_at = Some(now);
        self.last_click_cell = Some(cell);

        // ── Set selection range by click count ─────────────
        let cols = screen.cols;
        match self.click_count {
            1 => {
                self.granularity = SelectionGranularity::Character;
                self.range = Some(SelectionRange {
                    anchor: cell,
                    cursor: cell,
                });
            }
            2 => {
                self.granularity = SelectionGranularity::Word;
                let (start, end) = Self::find_word_range(screen, cell.0, cell.1);
                self.range = Some(SelectionRange {
                    anchor: start,
                    cursor: end,
                });
            }
            _ => {
                // Triple click and beyond → line selection.
                self.granularity = SelectionGranularity::Line;
                let last = cols.saturating_sub(1);
                self.range = Some(SelectionRange {
                    anchor: (cell.0, 0),
                    cursor: (cell.0, last),
                });
            }
        }

        self.dragging = true;
        SelectionOutcome::DragActive
    }

    /// Mouse drag to `cell` during an active selection.
    /// Returns true if the cursor position changed.
    pub fn drag_to(&mut self, cell: (u64, usize), screen: &TerminalSnapshot) -> bool {
        let Some(ref mut sel) = self.range else {
            return false;
        };
        if sel.cursor == cell {
            return false;
        }

        let anchor = sel.anchor;
        let new_cursor = match self.granularity {
            SelectionGranularity::Word => Self::snap_word_cursor(screen, cell.0, cell.1, anchor),
            SelectionGranularity::Line => Self::snap_line_cursor(screen, cell.0, cell.1, anchor),
            SelectionGranularity::Character => cell,
        };
        sel.cursor = new_cursor;

        // ── Auto-scroll direction detection ────────────────
        let rows = screen.rows;
        let view_offset = screen.view_offset;
        let scroll_count = screen.scroll_count;
        let display_row = ((cell.0 - screen.history_start) as usize + view_offset)
            .saturating_sub(scroll_count)
            .min(rows - 1);
        let new_auto_scroll = if display_row < AUTO_SCROLL_MARGIN && view_offset < scroll_count {
            Some(AutoScroll::Up)
        } else if display_row >= rows.saturating_sub(AUTO_SCROLL_MARGIN) && view_offset > 0 {
            Some(AutoScroll::Down)
        } else {
            None
        };
        if new_auto_scroll != self.auto_scroll {
            self.auto_scroll = new_auto_scroll;
            self.next_auto_scroll_at = None;
        }

        true
    }

    /// Left-mouse-button release.
    pub fn release(&mut self) -> SelectionOutcome {
        if !self.dragging {
            return SelectionOutcome::None;
        }
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;

        // Click without drag → clear selection.
        let is_zero_width = self.range.is_some_and(|sel| sel.anchor == sel.cursor);
        if is_zero_width {
            self.range = None;
        }
        SelectionOutcome::DragEnded
    }

    /// Cancel an in-progress drag (focus loss, alt screen, resize, keyboard).
    pub fn cancel(&mut self) -> SelectionOutcome {
        if !self.dragging {
            return SelectionOutcome::None;
        }
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
        SelectionOutcome::DragEnded
    }

    /// Clear all selection state (terminal resize).
    pub fn clear(&mut self) {
        self.range = None;
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
        self.click_count = 0;
        self.last_click_at = None;
        self.last_click_cell = None;
    }

    /// Handle a non-modifier key press — always cancels selection.
    /// Returns true if selection was active (caller should redraw).
    pub fn on_key_press(&mut self) -> bool {
        let had_selection = self.range.is_some() || self.dragging;
        self.range = None;
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
        had_selection
    }

    /// Returns normalized [`SelectionBounds`] for the active selection, or `None`.
    pub fn bounds(&self) -> Option<SelectionBounds> {
        let sel = self.range?;
        let (start_row, start_col, end_row, end_col) = sel.normalized();
        Some(SelectionBounds {
            start_row,
            start_col,
            end_row,
            end_col,
        })
    }

    /// Whether the current selection range is zero-width (anchor == cursor).
    pub fn is_range_empty(&self) -> bool {
        self.range.is_some_and(|sel| sel.anchor == sel.cursor)
    }

    /// Whether a selection is active.
    pub fn has_selection(&self) -> bool {
        self.range.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn auto_scroll_direction(&self) -> Option<AutoScroll> {
        self.auto_scroll
    }

    pub fn auto_scroll_deadline(&self) -> Option<Instant> {
        self.next_auto_scroll_at
    }

    /// Compute the new cursor position after one auto-scroll tick.
    ///
    /// Returns `(direction, new_cursor)` if auto-scroll should proceed.
    /// The caller executes the returned scroll direction; this method only
    /// calculates what the cursor would become.
    pub fn compute_auto_scroll_cursor(
        &mut self,
        now: Instant,
        screen: &TerminalSnapshot,
    ) -> Option<(AutoScroll, (u64, usize))> {
        let scroll = self.auto_scroll?;
        if !self.dragging {
            self.auto_scroll = None;
            self.next_auto_scroll_at = None;
            return None;
        }
        // Rate limit.
        if let Some(deadline) = self.next_auto_scroll_at
            && deadline > now
        {
            return None;
        }

        let can_scroll_up = screen.view_offset < screen.scroll_count;
        let can_scroll_down = screen.view_offset > 0;

        let (direction, new_gen) = match scroll {
            AutoScroll::Up if can_scroll_up => {
                (AutoScroll::Up, self.range?.cursor.0.saturating_sub(1))
            }
            AutoScroll::Down if can_scroll_down => {
                (AutoScroll::Down, self.range?.cursor.0.saturating_add(1))
            }
            _ => {
                // Can't scroll in this direction — stop.
                self.auto_scroll = None;
                self.next_auto_scroll_at = None;
                return None;
            }
        };

        // Re-snap column when in word or line mode.
        let anchor = self.range?.anchor;
        let cur_col = self.range?.cursor.1;
        let snapped = match self.granularity {
            SelectionGranularity::Word => Self::snap_word_cursor(screen, new_gen, cur_col, anchor),
            SelectionGranularity::Line => Self::snap_line_cursor(screen, new_gen, cur_col, anchor),
            SelectionGranularity::Character => (new_gen, cur_col),
        };

        let deadline = Instant::now() + Duration::from_millis(AUTO_SCROLL_INTERVAL_MS);
        self.next_auto_scroll_at = Some(deadline);

        Some((direction, snapped))
    }

    // ── Word / line boundary helpers ─────────────────────────

    /// Returns `((start_gen, start_col), (end_gen, end_col))` of the word
    /// at `(generation, col)`.  When the clicked cell is a separator, the word
    /// range is zero-width (just that cell).
    pub fn find_word_range(
        screen: &TerminalSnapshot,
        generation: u64,
        col: usize,
    ) -> ((u64, usize), (u64, usize)) {
        let cols = screen.cols;
        let cell_at = |c: usize| screen.cell_at_generation(generation, c);
        let cell_ch = |c: usize| cell_at(c).map(|cell| cell.ch);

        // If we clicked on a wide_continuation cell, redirect to the
        // real character cell (the preceding column).
        let effective_col = if col > 0 && cell_at(col).is_some_and(|cell| cell.wide_continuation) {
            col.saturating_sub(1)
        } else {
            col
        };

        let Some(clicked_ch) = cell_ch(effective_col) else {
            return ((generation, effective_col), (generation, effective_col));
        };

        // Separator → zero-width.
        if WORD_SEPARATORS.contains(&clicked_ch) {
            return ((generation, effective_col), (generation, effective_col));
        }

        let clicked_is_cjk = is_cjk(clicked_ch);

        // Decide what qualifies as "in the same word":
        // - wide_continuation cells are transparent only when grouping CJK chars
        // - CJK → only consecutive CJK chars (stop at separators or non-CJK)
        // - Latin/etc → only non-separator non-CJK chars (stop at separators or CJK)
        let in_same_word = |c: usize| -> bool {
            let Some(cell) = cell_at(c) else { return false };
            if cell.wide_continuation {
                return clicked_is_cjk;
            }
            if WORD_SEPARATORS.contains(&cell.ch) {
                return false;
            }
            if clicked_is_cjk {
                is_cjk(cell.ch)
            } else {
                !is_cjk(cell.ch)
            }
        };

        // Scan left from the effective click column.
        let mut left = effective_col;
        while left > 0 && in_same_word(left - 1) {
            left -= 1;
        }

        // Scan right from the effective click column.
        let mut right = effective_col;
        while right < cols - 1 && in_same_word(right + 1) {
            right += 1;
        }

        ((generation, left), (generation, right))
    }

    pub fn snap_word_cursor(
        screen: &TerminalSnapshot,
        generation: u64,
        col: usize,
        anchor: (u64, usize),
    ) -> (u64, usize) {
        let cell_at = |c: usize| screen.cell_at_generation(generation, c);
        // True separator: in WORD_SEPARATORS AND not a wide_continuation cell.
        let is_sep = |c: usize| {
            cell_at(c)
                .is_none_or(|cell| !cell.wide_continuation && WORD_SEPARATORS.contains(&cell.ch))
        };
        // True CJK char cell (not wide_continuation).
        let is_cjk_cell =
            |c: usize| cell_at(c).is_some_and(|cell| !cell.wide_continuation && is_cjk(cell.ch));
        let cols = screen.cols;
        // True word boundary: separator, CJK char, or wide_continuation cell.
        let is_boundary = |c: usize| {
            cell_at(c).is_none_or(|cell| {
                cell.wide_continuation || WORD_SEPARATORS.contains(&cell.ch) || is_cjk(cell.ch)
            })
        };

        if (generation, col) >= anchor {
            // Expanding forward → snap to right boundary.
            let mut c = col;
            // Skip past true separators.
            while c < cols - 1 && is_sep(c) {
                c += 1;
            }
            // If we landed on a CJK wide_continuation, we're already at the
            // character's right edge — no further expansion.
            if cell_at(c).is_some_and(|cell| cell.wide_continuation) {
                return (generation, c);
            }
            // If CJK: advance past its wide_continuation to include the full char.
            if is_cjk_cell(c)
                && c < cols - 1
                && cell_at(c + 1).is_some_and(|cell| cell.wide_continuation)
            {
                c += 1;
            } else if !is_cjk_cell(c) {
                // Latin word char: expand through consecutive non-sep non-CJK non-wide_cont chars.
                while c < cols - 1 {
                    if is_boundary(c + 1) {
                        break;
                    }
                    c += 1;
                }
            }
            (generation, c)
        } else {
            // Expanding backward → snap to left boundary.
            let mut c = col;
            // Skip past true separators going left.
            while c > 0 && is_sep(c) {
                c -= 1;
            }
            // If on a wide_continuation, step back to the real char.
            if cell_at(c).is_some_and(|cell| cell.wide_continuation) && c > 0 {
                c -= 1;
            }
            // CJK: stay at real char (word start). Latin: scan left to word start.
            if !is_cjk_cell(c) {
                while c > 0 {
                    if is_boundary(c - 1) {
                        break;
                    }
                    c -= 1;
                }
            }
            (generation, c)
        }
    }

    /// Snap cursor column for line-wise drag.
    pub fn snap_line_cursor(
        screen: &TerminalSnapshot,
        generation: u64,
        col: usize,
        anchor: (u64, usize),
    ) -> (u64, usize) {
        let last_col = screen.cols.saturating_sub(1);
        if (generation, col) >= anchor {
            (generation, last_col)
        } else {
            (generation, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::Screen;
    use std::time::Instant;

    /// Helper: create a tiny snapshot with known content.
    fn test_screen(rows: usize, cols: usize) -> Screen {
        let mut s = Screen::new(rows, cols);
        // Fill row 0 with "abcd...".
        for (c, ch) in "abcdefghij".chars().enumerate().take(cols) {
            s.cell_mut(0, c).ch = ch;
        }
        if rows > 1 {
            for (c, ch) in "ABCDEFGHIJ".chars().enumerate().take(cols) {
                s.cell_mut(1, c).ch = ch;
            }
        }
        s
    }

    #[test]
    fn press_single_click_creates_character_selection() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        let outcome = model.press((0, 3), now, &screen.terminal_snapshot());

        assert_eq!(outcome, SelectionOutcome::DragActive);
        assert!(model.is_dragging());
        assert!(model.has_selection());
        assert!(model.is_range_empty()); // anchor == cursor
        assert_eq!(model.click_count, 1);
    }

    #[test]
    fn double_click_selects_word() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // First click.
        model.press((0, 3), now, &screen.terminal_snapshot());
        model.release();

        // Second click (within timeout) at same cell.
        let outcome = model.press((0, 3), now, &screen.terminal_snapshot());

        assert_eq!(outcome, SelectionOutcome::DragActive);
        assert_eq!(model.granularity, SelectionGranularity::Word);
        // The word at col 3 in "abcdefghij" — col 3 is 'd', word spans 0..9.
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 9);
    }

    #[test]
    fn triple_click_selects_full_line() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // Click 1.
        model.press((0, 3), now, &screen.terminal_snapshot());
        model.release();
        // Click 2.
        model.press((0, 3), now, &screen.terminal_snapshot());
        model.release();
        // Click 3.
        let outcome = model.press((0, 3), now, &screen.terminal_snapshot());

        assert_eq!(outcome, SelectionOutcome::DragActive);
        assert_eq!(model.granularity, SelectionGranularity::Line);
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 9); // cols - 1
    }

    #[test]
    fn click_on_different_cell_resets_click_chain() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 3), now, &screen.terminal_snapshot());
        model.release();

        // Different cell — should reset to single click.
        let outcome = model.press((1, 5), now, &screen.terminal_snapshot());
        assert_eq!(model.click_count, 1);
        assert_eq!(outcome, SelectionOutcome::DragActive);
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_row, 1);
        assert_eq!(bounds.start_col, 5);
    }

    #[test]
    fn drag_to_without_press_is_noop() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();

        let changed = model.drag_to((0, 5), &screen.terminal_snapshot());
        assert!(!changed);
        assert!(!model.has_selection());
    }

    #[test]
    fn drag_to_updates_cursor() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());

        let changed = model.drag_to((0, 7), &screen.terminal_snapshot());
        assert!(changed);

        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_row, 0);
        assert_eq!(bounds.start_col, 2);
        assert_eq!(bounds.end_row, 0);
        assert_eq!(bounds.end_col, 7);
    }

    #[test]
    fn drag_to_same_cell_returns_false() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        let changed = model.drag_to((0, 2), &screen.terminal_snapshot());
        assert!(!changed);
    }

    #[test]
    fn drag_in_reverse_swaps_normalized_range() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // Press at col 7, drag left to col 2.
        model.press((0, 7), now, &screen.terminal_snapshot());
        model.drag_to((0, 2), &screen.terminal_snapshot());

        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 2);
        assert_eq!(bounds.end_col, 7);
    }

    #[test]
    fn release_during_drag_ends_drag() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        assert!(model.is_dragging());
        model.drag_to((0, 5), &screen.terminal_snapshot()); // extend to non-zero-width

        let outcome = model.release();
        assert_eq!(outcome, SelectionOutcome::DragEnded);
        assert!(!model.is_dragging());
        assert!(model.has_selection()); // non-zero-width survives
    }

    #[test]
    fn release_without_drag_returns_none() {
        let mut model = SelectionModel::new();
        assert_eq!(model.release(), SelectionOutcome::None);
    }

    #[test]
    fn click_and_release_without_drag_clears_selection() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        // Zero-width (anchor == cursor) — release should clear.
        let outcome = model.release();
        assert_eq!(outcome, SelectionOutcome::DragEnded);
        assert!(!model.has_selection());
    }

    #[test]
    fn cancel_clears_drag_state() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        assert!(model.is_dragging());

        let outcome = model.cancel();
        assert_eq!(outcome, SelectionOutcome::DragEnded);
        assert!(!model.is_dragging());
        // Selection range is still there (unlike release, cancel doesn't clear range).
    }

    #[test]
    fn cancel_without_drag_returns_none() {
        let mut model = SelectionModel::new();
        assert_eq!(model.cancel(), SelectionOutcome::None);
    }

    #[test]
    fn clear_resets_everything() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        model.drag_to((0, 5), &screen.terminal_snapshot());
        assert!(model.has_selection());
        assert!(model.is_dragging());

        model.clear();
        assert!(!model.has_selection());
        assert!(!model.is_dragging());
        assert_eq!(model.click_count, 0);
        assert!(model.last_click_at.is_none());
    }

    #[test]
    fn on_key_press_clears_selection() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        assert!(model.on_key_press());
        assert!(!model.has_selection());
        assert!(!model.is_dragging());
    }

    #[test]
    fn on_key_press_without_selection_returns_false() {
        let mut model = SelectionModel::new();
        assert!(!model.on_key_press());
    }

    #[test]
    fn bounds_returns_none_when_no_selection() {
        let model = SelectionModel::new();
        assert!(model.bounds().is_none());
    }

    #[test]
    fn is_range_empty_true_when_anchor_equals_cursor() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        assert!(model.is_range_empty());
    }

    #[test]
    fn is_range_empty_false_after_drag() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot());
        model.drag_to((0, 5), &screen.terminal_snapshot());
        assert!(!model.is_range_empty());
    }

    #[test]
    fn find_word_range_middle_of_word() {
        let screen = test_screen(3, 10);
        // "abcdefghij" — clicking 'd' (col 3) spans 0..9.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 3);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 9));
    }

    #[test]
    fn find_word_range_at_separator() {
        let mut screen = test_screen(3, 10);
        screen.cell_mut(0, 3).ch = ' ';
        // Clicking a space at col 3 gives zero-width word.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 3);
        assert_eq!(start, (0, 3));
        assert_eq!(end, (0, 3));
    }

    #[test]
    fn snap_word_cursor_forward_lands_at_word_end() {
        let screen = test_screen(3, 10);
        // "abcdefghij" — anchor at 2, cursor moving to 5.
        // Forward (>= anchor): lands at the end of the word (col 9).
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 5, (0, 2));
        assert_eq!(snapped, (0, 9));
    }

    #[test]
    fn snap_word_cursor_backward_lands_at_word_start() {
        let screen = test_screen(3, 10);
        // "abcdefghij" — anchor at 5, cursor moving to 2.
        // Backward (< anchor): lands at start of word (col 0).
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 2, (0, 5));
        assert_eq!(snapped, (0, 0));
    }

    #[test]
    fn snap_line_cursor_forward_lands_at_last_col() {
        let screen = test_screen(3, 10);
        let snapped = SelectionModel::snap_line_cursor(&screen.terminal_snapshot(), 0, 5, (0, 2));
        assert_eq!(snapped, (0, 9));
    }

    #[test]
    fn snap_line_cursor_backward_lands_at_col_zero() {
        let screen = test_screen(3, 10);
        let snapped = SelectionModel::snap_line_cursor(&screen.terminal_snapshot(), 0, 2, (0, 5));
        assert_eq!(snapped, (0, 0));
    }

    // ── CJK word selection tests ────────────────────────────

    /// Create a tiny screen and fill row 0 with the given string.
    fn screen_with_text(text: &str) -> Screen {
        let chars: Vec<char> = text.chars().collect();
        let cols = chars.len();
        let mut s = Screen::new(2, cols);
        for (c, &ch) in chars.iter().enumerate() {
            s.cell_mut(0, c).ch = ch;
        }
        s
    }

    #[test]
    fn is_cjk_detects_ideographs() {
        assert!(is_cjk('你'));
        assert!(is_cjk('好'));
        assert!(is_cjk('世'));
        assert!(is_cjk('界'));
        assert!(!is_cjk('a'));
        assert!(!is_cjk(' '));
        assert!(!is_cjk('1'));
    }

    #[test]
    fn is_cjk_detects_hiragana() {
        assert!(is_cjk('あ'));
        assert!(is_cjk('い'));
    }

    #[test]
    fn is_cjk_detects_katakana() {
        assert!(is_cjk('ア'));
        assert!(is_cjk('イ'));
    }

    #[test]
    fn is_cjk_detects_hangul() {
        assert!(is_cjk('한'));
        assert!(is_cjk('글'));
    }

    #[test]
    fn find_word_range_cjk_groups_consecutive() {
        // "你好世界" — all CJK, should select entire run.
        let screen = screen_with_text("你好世界");
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 1); // click '好'
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 3));
    }

    #[test]
    fn find_word_range_cjk_stops_at_separator() {
        // "你好 世界" — space at col 2.
        let screen = screen_with_text("你好 世界");
        // Click '好' (col 1) — should select only "你好" (cols 0-1).
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 1);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 1));
    }

    #[test]
    fn find_word_range_cjk_stops_at_latin() {
        // "hello世界" — Latin then CJK.
        let screen = screen_with_text("hello世界");
        // Click '世' (col 5) — CJK group: only cols 5-6.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 5);
        assert_eq!(start, (0, 5));
        assert_eq!(end, (0, 6));
        // Click 'o' (col 4) — Latin group: cols 0-4.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 4);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 4));
    }

    #[test]
    fn find_word_range_latin_stops_at_cjk() {
        // "你好world" — CJK then Latin.
        let screen = screen_with_text("你好world");
        // Click '你' (col 0) — CJK group: cols 0-1.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 0);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 1));
        // Click 'w' (col 2) — Latin group: cols 2-6.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 2);
        assert_eq!(start, (0, 2));
        assert_eq!(end, (0, 6));
    }

    #[test]
    fn find_word_range_cjk_punctuation_is_zero_width() {
        // CJK punctuation is in WORD_SEPARATORS, not in is_cjk.
        let screen = screen_with_text("你好，世界");
        // Click '，' (col 2) — separator → zero-width.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 2);
        assert_eq!(start, (0, 2));
        assert_eq!(end, (0, 2));
    }

    #[test]
    fn snap_word_cursor_cjk_forward_stays_at_char() {
        // "你好世界abcdef"
        let screen = screen_with_text("你好世界abcdef");
        // anchor at 0, cursor dragged to col 2 (世) → should stay at 2.
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 2, (0, 0));
        assert_eq!(snapped, (0, 2));
    }

    #[test]
    fn snap_word_cursor_cjk_forward_skips_sep_to_cjk() {
        // "你好 世界" — cols: 0=你, 1=好, 2=' ', 3=世, 4=界
        let screen = screen_with_text("你好 世界");
        // anchor at 0, cursor dragged to col 2 (space) →
        // forward snap skips space, lands on 世 (col 3), CJK → stay.
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 2, (0, 0));
        assert_eq!(snapped, (0, 3));
    }

    #[test]
    fn snap_word_cursor_cjk_forward_to_latin_expands() {
        // "你好world"
        let screen = screen_with_text("你好world");
        // anchor at 0, cursor dragged to col 2 (w) →
        // skip seps? none. w is Latin → expand to end of "world".
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 2, (0, 0));
        assert_eq!(snapped, (0, 6));
    }

    #[test]
    fn snap_word_cursor_cjk_backward_stays_at_char() {
        // "你好世界"
        let screen = screen_with_text("你好世界");
        // anchor at 3, cursor dragged to col 1 (好) backward.
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 1, (0, 3));
        assert_eq!(snapped, (0, 1));
    }

    #[test]
    fn double_click_cjk_selects_consecutive_run() {
        // "你好世界hello"
        let screen = screen_with_text("你好世界hello");
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // First click: char selection.
        model.press((0, 1), now, &screen.terminal_snapshot()); // click '好'
        // Second click within timeout at same cell → double-click → Word mode.
        let now2 = now + Duration::from_millis(200);
        let outcome = model.press((0, 1), now2, &screen.terminal_snapshot());
        assert_eq!(outcome, SelectionOutcome::DragActive);

        let bounds = model.bounds().unwrap();
        // Should select "你好世界" (CJK run, cols 0-3).
        assert_eq!(bounds.start_row, 0);
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_row, 0);
        assert_eq!(bounds.end_col, 3);
    }

    #[test]
    fn double_click_cjk_then_drag_char_by_char() {
        // "你好世界"
        let screen = screen_with_text("你好世界");
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // Double-click '好' (col 1).
        model.press((0, 1), now, &screen.terminal_snapshot());
        let now2 = now + Duration::from_millis(200);
        model.press((0, 1), now2, &screen.terminal_snapshot());

        // Initial: entire "你好世界" (0..3).
        assert_eq!(model.bounds().unwrap().start_col, 0);
        assert_eq!(model.bounds().unwrap().end_col, 3);

        // Drag to col 2 (世) — forward snap, CJK stays at 2.
        model.drag_to((0, 2), &screen.terminal_snapshot());
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 2);

        // Drag to col 1 (好) — selection should be 0..1.
        model.drag_to((0, 1), &screen.terminal_snapshot());
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 1);

        // Drag to col 0 (你) — zero-width (anchor == cursor).
        model.drag_to((0, 0), &screen.terminal_snapshot());
        assert!(model.is_range_empty());
    }

    // ── Wide-character (CJK) regression tests ──────────────
    //
    // In the real terminal, CJK characters occupy 2 cells:
    //   col 0: ch='你', wide_continuation=false
    //   col 1: ch=' ',  wide_continuation=true
    //   col 2: ch='好', wide_continuation=false
    //   col 3: ch=' ',  wide_continuation=true
    //   ...
    // The wide_continuation cell's ch=' ' was falsely matching
    // WORD_SEPARATORS, breaking word boundary detection.

    /// Create a screen with proper wide_continuation cells for CJK characters.
    fn screen_with_wide_text(text: &str) -> Screen {
        let chars: Vec<char> = text.chars().collect();
        // Compute total cols: each CJK char is 2 wide, others 1.
        let total_cols: usize = chars
            .iter()
            .map(|&ch| {
                use unicode_width::UnicodeWidthChar;
                UnicodeWidthChar::width(ch).unwrap_or(0).clamp(1, 2)
            })
            .sum();
        let mut s = Screen::new(2, total_cols);
        let mut col = 0;
        for &ch in &chars {
            use unicode_width::UnicodeWidthChar;
            let w = UnicodeWidthChar::width(ch).unwrap_or(0).clamp(1, 2);
            s.cell_mut(0, col).ch = ch;
            for offset in 1..w {
                let cont = s.cell_mut(0, col + offset);
                cont.ch = ' ';
                cont.wide_continuation = true;
            }
            col += w;
        }
        s
    }

    #[test]
    fn find_word_range_wide_cjk_groups_consecutive() {
        // "你好世界" as 8 wide cells.
        let screen = screen_with_wide_text("你好世界");
        // Click '好' at col 2 (real char cell).
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 2);
        assert_eq!(start, (0, 0)); // 你
        assert_eq!(end, (0, 7)); // 界's wide_continuation
    }

    #[test]
    fn find_word_range_wide_cjk_click_on_continuation_redirects() {
        // Click on wide_continuation cell of '好' (col 3).
        let screen = screen_with_wide_text("你好世界");
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 3);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 7));
    }

    #[test]
    fn find_word_range_wide_cjk_stops_at_separator() {
        // "你好 世界" — space at cols 4-4, then 世界 at 5-8.
        let screen = screen_with_wide_text("你好 世界");
        // Click '好' (col 2) → should select "你好" (cols 0-3).
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 2);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 3)); // includes 好's wide_continuation
    }

    #[test]
    fn find_word_range_wide_cjk_stops_at_latin() {
        // "hello世" — "hello" cols 0-4, "世" cols 5-6.
        let screen = screen_with_wide_text("hello世");
        // Click '世' (col 5) → CJK group: cols 5-6 only.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 5);
        assert_eq!(start, (0, 5));
        assert_eq!(end, (0, 6));
        // Click 'o' (col 4) → Latin group: cols 0-4.
        let (start, end) = SelectionModel::find_word_range(&screen.terminal_snapshot(), 0, 4);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 4));
    }

    #[test]
    fn snap_word_cursor_wide_cjk_forward_snaps_to_char_end() {
        // "你好世界" → 8 cells: 你(0),wc(1),好(2),wc(3),世(4),wc(5),界(6),wc(7)
        let screen = screen_with_wide_text("你好世界");
        // anchor at 0, cursor dragged to col 4 (世's real cell)
        // → CJK: advance to wide_continuation at col 5.
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 4, (0, 0));
        assert_eq!(snapped, (0, 5));
    }

    #[test]
    fn snap_word_cursor_wide_cjk_backward_stays_at_char_start() {
        let screen = screen_with_wide_text("你好世界");
        // anchor at 7 (界's wc), cursor dragged to col 3 (好's wc).
        // backward: skip seps, step off wc to col 2 (好), CJK → stay.
        let snapped = SelectionModel::snap_word_cursor(&screen.terminal_snapshot(), 0, 3, (0, 7));
        assert_eq!(snapped, (0, 2));
    }

    #[test]
    fn double_click_wide_cjk_selects_full_run() {
        let screen = screen_with_wide_text("你好世界");
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen.terminal_snapshot()); // click '好' at col 2
        let now2 = now + Duration::from_millis(200);
        let outcome = model.press((0, 2), now2, &screen.terminal_snapshot());
        assert_eq!(outcome, SelectionOutcome::DragActive);

        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 7);
    }

    #[test]
    fn double_click_wide_cjk_then_drag_char_by_char() {
        // "你好世界" → 8 cells.
        let screen = screen_with_wide_text("你好世界");
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // Double-click '好' (col 2).
        model.press((0, 2), now, &screen.terminal_snapshot());
        let now2 = now + Duration::from_millis(200);
        model.press((0, 2), now2, &screen.terminal_snapshot());

        // Initial: entire run (0..7).
        assert_eq!(model.bounds().unwrap().start_col, 0);
        assert_eq!(model.bounds().unwrap().end_col, 7);

        // Drag to col 4 (世) → snaps to col 5 (wc).
        model.drag_to((0, 4), &screen.terminal_snapshot());
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 5); // 你好世

        // Drag to col 2 (好) → snaps to col 3 (wc).
        model.drag_to((0, 2), &screen.terminal_snapshot());
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 3); // 你好

        // Drag to col 0 (你) → snaps to col 1 (wc).
        model.drag_to((0, 0), &screen.terminal_snapshot());
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 1); // 你
    }

    #[test]
    fn test_selection_model_states() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();
        let snapshot = screen.terminal_snapshot();

        // 1. Initially (range = None)
        assert!(!model.has_selection());
        assert!(!model.is_range_empty()); // range is None, so is_range_empty returns false!

        // 2. Pressed but not dragged (range is Some and empty)
        model.press((0, 2), now, &snapshot);
        assert!(model.has_selection());
        assert!(model.is_range_empty());

        // 3. Dragged (range is Some and not empty)
        model.drag_to((0, 5), &snapshot);
        assert!(model.has_selection());
        assert!(!model.is_range_empty());
    }
}
