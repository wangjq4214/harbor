//! Pure domain model for text selection state.
//!
//! Owns anchor/cursor tracking, granularity, click-chain detection, and
//! auto-scroll scheduling.  No GPU or window dependencies — testable without
//! a rendering context.

use crate::content_anchor::{Affinity, AnchorMutationBatch, ContentAnchor, ContentProjection};
use crate::model::{SelectionBounds, TerminalSnapshot};
use std::time::{Duration, Instant};

/// Returns whether a character belongs to a double-click word.
/// Unicode letters/numbers and underscore are word characters; all other
/// punctuation and whitespace are boundaries.
fn is_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

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
/// Informs the terminal interaction owner which side effects to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionOutcome {
    /// No state change.
    None,
    /// Drag started — set dirty + request redraw + suppress PTY scroll snap.
    DragActive,
    /// Drag ended — set dirty + request redraw + re-enable PTY scroll snap.
    DragEnded,
}

/// A stable cell coordinate in scrollback generation space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GenPos {
    pub generation: u64,
    pub col: usize,
}

impl GenPos {
    pub const fn new(generation: u64, col: usize) -> Self {
        Self { generation, col }
    }
}

impl From<(u64, usize)> for GenPos {
    fn from((generation, col): (u64, usize)) -> Self {
        Self { generation, col }
    }
}

impl From<GenPos> for (u64, usize) {
    fn from(pos: GenPos) -> Self {
        (pos.generation, pos.col)
    }
}

/// Tracks the current text selection as a pair of grid coordinates.
/// `anchor` is where the drag started; `cursor` is the current drag endpoint.
/// Both use **generations** (stable scrollback coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRange {
    pub anchor: GenPos,
    pub cursor: GenPos,
}

impl SelectionRange {
    pub fn new(anchor: impl Into<GenPos>, cursor: impl Into<GenPos>) -> Self {
        Self {
            anchor: anchor.into(),
            cursor: cursor.into(),
        }
    }

    /// Returns `(start_pos, end_pos)` in row-major reading order.
    pub fn normalized_pos(&self) -> (GenPos, GenPos) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    /// Returns `(start_row, start_col, end_row, end_col)` in row-major reading order.
    /// Guarantees start ≤ end in row-major (generation-major).
    pub fn normalized(&self) -> (u64, usize, u64, usize) {
        let (start, end) = self.normalized_pos();
        (start.generation, start.col, end.generation, end.col)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AnchoredSelectionEndpoint {
    anchor: ContentAnchor,
    projection: GenPos,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AnchoredSelectionRange {
    anchor: AnchoredSelectionEndpoint,
    cursor: AnchoredSelectionEndpoint,
}

/// Pure domain model for text selection state.
///
/// Owns anchor/cursor tracking, granularity, click-chain detection, and
/// auto-scroll scheduling.  No GPU or window dependencies — testable without
/// a rendering context.
#[derive(Clone, Debug)]
pub struct SelectionModel {
    /// None = no active selection.
    range: Option<SelectionRange>,
    /// Canonical endpoints for terminal-owned selections; `range` is their current projection.
    anchored_range: Option<AnchoredSelectionRange>,
    /// True while left mouse button is held.
    dragging: bool,
    /// Current selection granularity (Character / Word / Line).
    granularity: SelectionGranularity,
    /// Timestamp of the most recent `MouseInput::Pressed` (for click-chain detection).
    last_click_at: Option<Instant>,
    /// Grid cell of the most recent click (gen, col).
    last_click_cell: Option<GenPos>,
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
            anchored_range: None,
            last_click_at: None,
            last_click_cell: None,
            click_count: 0,
            auto_scroll: None,
            next_auto_scroll_at: None,
        }
    }

    fn stop_auto_scroll(&mut self) {
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
    }

    /// Left-mouse-button press at `cell`.
    ///
    /// Detects click chains (double/triple-click) and sets the selection range
    /// and granularity accordingly.
    pub fn press(
        &mut self,
        cell: impl Into<GenPos>,
        now: Instant,
        screen: &TerminalSnapshot,
    ) -> SelectionOutcome {
        let cell: GenPos = cell.into();
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
                let (start, end) = Self::find_word_range(screen, cell.generation, cell.col);
                self.range = Some(SelectionRange {
                    anchor: start.into(),
                    cursor: end.into(),
                });
            }
            _ => {
                // Triple click and beyond → logical-line selection. The
                // snapshot contains visible soft-wrap markers, so this stays
                // bounded to retained rows available to the UI.
                self.granularity = SelectionGranularity::Line;
                let view_start = screen.history_start
                    + screen.scroll_count.saturating_sub(screen.view_offset) as u64;
                let view_end = view_start + screen.rows.saturating_sub(1) as u64;
                let mut start = cell.generation.clamp(view_start, view_end);
                while start > view_start && screen.is_wrapped_at_generation(start) {
                    start -= 1;
                }
                let mut end = start;
                while end < view_end && screen.is_wrapped_at_generation(end + 1) {
                    end += 1;
                }
                self.range = Some(SelectionRange {
                    anchor: GenPos::new(start, 0),
                    cursor: GenPos::new(end, cols.saturating_sub(1)),
                });
            }
        }

        self.anchored_range = None;
        self.dragging = true;
        SelectionOutcome::DragActive
    }

    /// Mouse drag to `cell` during an active selection.
    /// Returns true if the cursor position changed.
    pub fn drag_to(&mut self, cell: impl Into<GenPos>, screen: &TerminalSnapshot) -> bool {
        let cell: GenPos = cell.into();
        let Some(ref mut sel) = self.range else {
            return false;
        };

        let anchor = sel.anchor;
        let new_cursor = match self.granularity {
            SelectionGranularity::Word => {
                Self::snap_word_cursor(screen, cell.generation, cell.col, anchor.into()).into()
            }
            SelectionGranularity::Line => {
                Self::snap_line_cursor(screen, cell.generation, cell.col, anchor.into()).into()
            }
            SelectionGranularity::Character => cell,
        };
        let changed = sel.cursor != new_cursor;
        sel.cursor = new_cursor;
        self.anchored_range = None;

        // ── Auto-scroll direction detection ────────────────
        let rows = screen.rows;
        let view_offset = screen.view_offset;
        let scroll_count = screen.scroll_count;
        let display_row = (cell.generation.saturating_sub(screen.history_start) as usize
            + view_offset)
            .saturating_sub(scroll_count)
            .min(rows.saturating_sub(1));
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

        changed
    }

    /// Left-mouse-button release.
    pub fn release(&mut self) -> SelectionOutcome {
        if !self.dragging {
            return SelectionOutcome::None;
        }
        self.dragging = false;
        self.stop_auto_scroll();

        // Click without drag → clear selection.
        if self.is_range_empty() {
            self.range = None;
            self.anchored_range = None;
        }
        SelectionOutcome::DragEnded
    }

    /// Cancel an in-progress drag (focus loss, alt screen, resize, keyboard).
    pub fn cancel(&mut self) -> SelectionOutcome {
        if !self.dragging {
            return SelectionOutcome::None;
        }
        self.dragging = false;
        self.stop_auto_scroll();
        SelectionOutcome::DragEnded
    }

    /// Clear all selection state (terminal resize).
    pub fn clear(&mut self) {
        self.range = None;
        self.anchored_range = None;
        self.dragging = false;
        self.stop_auto_scroll();
        self.click_count = 0;
        self.last_click_at = None;
        self.last_click_cell = None;
    }

    /// Handle a non-modifier key press — always cancels selection.
    /// Returns true if selection was active (caller should redraw).
    pub fn on_key_press(&mut self) -> bool {
        let had_selection = self.range.is_some() || self.dragging;
        self.range = None;
        self.anchored_range = None;
        self.dragging = false;
        self.stop_auto_scroll();
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
    pub(crate) fn commit_anchors(&mut self, projection: &ContentProjection) -> bool {
        let Some(range) = self.range else {
            self.anchored_range = None;
            return true;
        };
        let (anchor_affinity, cursor_affinity) = if range.anchor == range.cursor {
            (Affinity::Before, Affinity::Before)
        } else if range.anchor < range.cursor {
            (Affinity::Before, Affinity::After)
        } else {
            (Affinity::After, Affinity::Before)
        };
        let Some(anchor) = projection.to_anchor(range.anchor, anchor_affinity) else {
            self.clear();
            return false;
        };
        let Some(cursor) = projection.to_anchor(range.cursor, cursor_affinity) else {
            self.clear();
            return false;
        };
        self.anchored_range = Some(AnchoredSelectionRange {
            anchor: AnchoredSelectionEndpoint {
                anchor,
                projection: range.anchor,
            },
            cursor: AnchoredSelectionEndpoint {
                anchor: cursor,
                projection: range.cursor,
            },
        });
        true
    }

    pub(crate) fn reconcile_anchors(
        &mut self,
        mutations: &AnchorMutationBatch,
        projection: &ContentProjection,
    ) -> bool {
        let Some(mut anchored) = self.anchored_range else {
            if self.range.is_some() {
                self.clear();
                return true;
            }
            return false;
        };
        let previous = self.range;
        let Some(anchor) = mutations.apply(anchored.anchor.anchor) else {
            self.clear();
            return true;
        };
        let Some(cursor) = mutations.apply(anchored.cursor.anchor) else {
            self.clear();
            return true;
        };
        let project_endpoint = |adjusted: ContentAnchor, old: AnchoredSelectionEndpoint| {
            if adjusted == old.anchor && !mutations.affects_line(adjusted.line_id) {
                if projection.contains_generation(adjusted.line_id, old.projection.generation) {
                    Some(old.projection)
                } else {
                    projection
                        .resolve_selection(adjusted)
                        .map(|position| GenPos::new(position.generation, old.projection.col))
                }
            } else {
                projection.resolve_selection(adjusted)
            }
        };
        let anchor_projection = project_endpoint(anchor, anchored.anchor);
        let cursor_projection = project_endpoint(cursor, anchored.cursor);
        let (Some(anchor_projection), Some(cursor_projection)) =
            (anchor_projection, cursor_projection)
        else {
            self.clear();
            return true;
        };
        anchored.anchor = AnchoredSelectionEndpoint {
            anchor,
            projection: anchor_projection,
        };
        anchored.cursor = AnchoredSelectionEndpoint {
            anchor: cursor,
            projection: cursor_projection,
        };
        self.range = Some(SelectionRange::new(anchor_projection, cursor_projection));
        self.anchored_range = Some(anchored);
        self.range != previous
    }

    pub(crate) fn prepared_against(
        &self,
        project: impl Fn(ContentAnchor) -> Option<(GenPos, ContentAnchor)>,
    ) -> (Self, bool) {
        let mut prepared = self.clone();
        let Some(mut anchored) = prepared.anchored_range else {
            let invalidated = prepared.range.is_some();
            if invalidated {
                prepared.clear();
            }
            return (prepared, invalidated);
        };
        let Some((anchor_projection, anchor)) = project(anchored.anchor.anchor) else {
            prepared.clear();
            return (prepared, true);
        };
        let Some((cursor_projection, cursor)) = project(anchored.cursor.anchor) else {
            prepared.clear();
            return (prepared, true);
        };
        anchored.anchor = AnchoredSelectionEndpoint {
            anchor,
            projection: anchor_projection,
        };
        anchored.cursor = AnchoredSelectionEndpoint {
            anchor: cursor,
            projection: cursor_projection,
        };
        prepared.range = Some(SelectionRange::new(anchor_projection, cursor_projection));
        prepared.anchored_range = Some(anchored);
        (prepared, false)
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
            AutoScroll::Up if can_scroll_up => (
                AutoScroll::Up,
                self.range?.cursor.generation.saturating_sub(1),
            ),
            AutoScroll::Down if can_scroll_down => (
                AutoScroll::Down,
                self.range?.cursor.generation.saturating_add(1),
            ),
            _ => {
                // Can't scroll in this direction — stop.
                self.stop_auto_scroll();
                return None;
            }
        };

        // Re-snap column when in word or line mode.
        let anchor = self.range?.anchor;
        let cur_col = self.range?.cursor.col;
        let snapped = match self.granularity {
            SelectionGranularity::Word => {
                Self::snap_word_cursor(screen, new_gen, cur_col, anchor.into())
            }
            SelectionGranularity::Line => {
                Self::snap_line_cursor(screen, new_gen, cur_col, anchor.into())
            }
            SelectionGranularity::Character => (new_gen, cur_col),
        };

        let deadline = now + Duration::from_millis(AUTO_SCROLL_INTERVAL_MS);
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

        // Punctuation and whitespace are boundaries → zero-width.
        if !is_word_char(clicked_ch) {
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
            if !is_word_char(cell.ch) {
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
        // True separator: a non-word cell that is not a wide continuation.
        let is_sep = |c: usize| {
            cell_at(c).is_none_or(|cell| !cell.wide_continuation && !is_word_char(cell.ch))
        };
        // True CJK char cell (not wide_continuation).
        let is_cjk_cell =
            |c: usize| cell_at(c).is_some_and(|cell| !cell.wide_continuation && is_cjk(cell.ch));
        let cols = screen.cols;
        // True word boundary: separator, CJK char, or wide_continuation cell.
        let is_boundary = |c: usize| {
            cell_at(c).is_none_or(|cell| {
                cell.wide_continuation || !is_word_char(cell.ch) || is_cjk(cell.ch)
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
mod tests;
