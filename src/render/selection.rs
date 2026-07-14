use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{
    config::{SELECTION_COLOR, TEXT_PADDING},
    render::{
        Component, EventResult, SelectionInput,
        caps::{ModifiersAccess, PtyAccess, RedrawAccess, ScrollAccess, TerminalAccess},
        gpu::{self, ColoredVertex, GpuContext},
    },
    terminal::{Screen, SelectionBounds},
};
use arboard::Clipboard;
use winit::keyboard::{Key, NamedKey};

/// Maximum time between consecutive clicks (in ms) for them to count as a
/// multi-click chain (double-click → word, triple-click+ → line).
const MULTI_CLICK_TIMEOUT_MS: u64 = 500;

/// Characters that delimit words for double-click word selection.
/// Based on Alacritty's default separator set, extended for CJK punctuation.
const WORD_SEPARATORS: &[char] = &[
    ' ', '\t', '\n', '(', ')', '[', ']', '{', '}', '\'', '"', '`',
    // CJK brackets and punctuation
    '（', '）', '【', '】', '「', '」', '『', '』', '《', '》', '；', '：', '，', '。', '、', '？',
    '！', '‘', '’', '＂',
];

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
enum SelectionGranularity {
    /// Free character-level selection (single click + drag).
    #[default]
    Character,
    /// Word-level selection (double-click + word-wise drag).
    Word,
    /// Line-level selection (triple-click + line-wise drag).
    Line,
}

/// Tracks the current text selection as a pair of grid coordinates.
/// `anchor` is where the drag started; `cursor` is the current drag endpoint.
/// Both use **generations** (stable scrollback coordinates).
#[derive(Clone, Copy, Debug)]
struct SelectionRange {
    anchor: (u64, usize), // (generation, col)
    cursor: (u64, usize), // (generation, col)
}

impl SelectionRange {
    /// Returns `(start_row, start_col, end_row, end_col)` in row-major reading order.
    /// Guarantees start ≤ end in row-major (generation-major).
    fn normalized(&self) -> (u64, usize, u64, usize) {
        if self.anchor.0 < self.cursor.0
            || (self.anchor.0 == self.cursor.0 && self.anchor.1 <= self.cursor.1)
        {
            (self.anchor.0, self.anchor.1, self.cursor.0, self.cursor.1)
        } else {
            (self.cursor.0, self.cursor.1, self.anchor.0, self.anchor.1)
        }
    }
}

/// Direction of ongoing auto-scroll while dragging at viewport edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoScroll {
    Up,   // into scrollback history
    Down, // toward live content
}

const AUTO_SCROLL_MARGIN: usize = 3;
const AUTO_SCROLL_INTERVAL_MS: u64 = 16;

/// Outcome of a [`SelectionModel`] state transition.
///
/// Informs the outer [`Selection`] layer which side-effects to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionOutcome {
    /// No state change.
    None,
    /// Drag started — set dirty + request redraw + suppress PTY scroll snap.
    DragActive,
    /// Drag ended — set dirty + request redraw + re-enable PTY scroll snap.
    DragEnded,
}

// ── Selection model ─────────────────────────────────────────────────────────

/// Pure domain model for text selection state.
///
/// Owns anchor/cursor tracking, granularity, click-chain detection, and
/// auto-scroll scheduling.  No GPU or window dependencies — testable without
/// a rendering context.
#[derive(Clone, Debug)]
struct SelectionModel {
    /// None = no active selection.
    range: Option<SelectionRange>,
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

impl SelectionModel {
    fn new() -> Self {
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
    fn press(&mut self, cell: (u64, usize), now: Instant, screen: &Screen) -> SelectionOutcome {
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
        let cols = screen.cols();
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
    fn drag_to(&mut self, cell: (u64, usize), screen: &Screen) -> bool {
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
        let rows = screen.rows();
        let view_offset = screen.view_offset();
        let scroll_count = screen.scroll_count();
        let display_row = ((cell.0 - screen.history_start()) as usize + view_offset)
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
    fn release(&mut self) -> SelectionOutcome {
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
    fn cancel(&mut self) -> SelectionOutcome {
        if !self.dragging {
            return SelectionOutcome::None;
        }
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
        SelectionOutcome::DragEnded
    }

    /// Clear all selection state (terminal resize).
    fn clear(&mut self) {
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
    fn on_key_press(&mut self) -> bool {
        let had_selection = self.range.is_some() || self.dragging;
        self.range = None;
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
        had_selection
    }

    /// Returns normalized [`SelectionBounds`] for the active selection, or `None`.
    fn bounds(&self) -> Option<SelectionBounds> {
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
    fn is_range_empty(&self) -> bool {
        self.range.is_some_and(|sel| sel.anchor == sel.cursor)
    }

    /// Whether a selection is active.
    fn has_selection(&self) -> bool {
        self.range.is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn auto_scroll_direction(&self) -> Option<AutoScroll> {
        self.auto_scroll
    }

    fn auto_scroll_deadline(&self) -> Option<Instant> {
        self.next_auto_scroll_at
    }

    /// Compute the new cursor position after one auto-scroll tick.
    ///
    /// Returns `(direction, new_cursor)` if auto-scroll should proceed.
    /// The caller is responsible for actually scrolling the viewport via
    /// [`ScrollAccess`] — this method only calculates what the cursor would
    /// become.
    fn compute_auto_scroll_cursor(
        &mut self,
        now: Instant,
        screen: &Screen,
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

        let can_scroll_up = screen.view_offset() < screen.scroll_count();
        let can_scroll_down = screen.view_offset() > 0;

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
    fn find_word_range(
        screen: &Screen,
        generation: u64,
        col: usize,
    ) -> ((u64, usize), (u64, usize)) {
        let cols = screen.cols();
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
        // - wide_continuation cells are transparent (inherit from prev char)
        // - CJK → only consecutive CJK chars (stop at separators or non-CJK)
        // - Latin/etc → only non-separator non-CJK chars (stop at separators or CJK)
        let in_same_word = |c: usize| -> bool {
            let Some(cell) = cell_at(c) else { return false };
            if cell.wide_continuation {
                return true; // transparent — part of the preceding char's word
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

    fn snap_word_cursor(
        screen: &Screen,
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
        let cols = screen.cols();

        if (generation, col) >= anchor {
            // Expanding forward → snap to right boundary.
            let mut c = col;
            // Skip past true separators.
            while c < cols - 1 && is_sep(c) {
                c += 1;
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
                    let next = cell_at(c + 1);
                    if next.is_none_or(|cell| {
                        cell.wide_continuation
                            || WORD_SEPARATORS.contains(&cell.ch)
                            || is_cjk(cell.ch)
                    }) {
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
                    let prev = cell_at(c - 1);
                    if prev.is_none_or(|cell| {
                        cell.wide_continuation
                            || WORD_SEPARATORS.contains(&cell.ch)
                            || is_cjk(cell.ch)
                    }) {
                        break;
                    }
                    c -= 1;
                }
            }
            (generation, c)
        }
    }

    /// Snap cursor column for line-wise drag.
    fn snap_line_cursor(
        screen: &Screen,
        generation: u64,
        col: usize,
        anchor: (u64, usize),
    ) -> (u64, usize) {
        let last_col = screen.cols().saturating_sub(1);
        if (generation, col) >= anchor {
            (generation, last_col)
        } else {
            (generation, 0)
        }
    }
}

// ── Selection (outer — GPU + events) ──────────────────────────────────────

pub(crate) struct Selection {
    model: SelectionModel,
    pipeline: Arc<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    /// Number of vertices to draw (0 when no selection).
    vertex_count: u32,
    /// Current vertex buffer capacity (rows * cols * 6).
    vertex_cap: usize,
    /// Cached from the most recent CursorMoved event (physical pixels).
    /// Needed because winit 0.30 MouseInput does not carry a position.
    last_cursor_pos: Option<(f64, f64)>,
    cell_width: f32,
    line_height: f32,
    /// Whether vertex buffer needs re-upload.
    dirty: bool,
    /// System clipboard handle (None when clipboard is unavailable, e.g. headless).
    clipboard: Option<Clipboard>,
}

impl Selection {
    pub(crate) fn new(gpu: &GpuContext, cell_width: f32, line_height: f32) -> Self {
        let pipeline = gpu.colored_quad_pipeline();
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &[]);
        Self {
            model: SelectionModel::new(),
            pipeline,
            vertex_buffer,
            vertex_count: 0,
            vertex_cap: 0,
            last_cursor_pos: None,
            cell_width,
            line_height,
            dirty: false,
            clipboard: {
                let cb = Clipboard::new();
                if cb.is_err() {
                    tracing::warn!("clipboard unavailable; copy/paste will be disabled");
                }
                cb.ok()
            },
        }
    }

    /// Convert a physical-pixel cursor position to a generation+col.
    /// Clamps to grid bounds; never returns an out-of-range pair.
    #[allow(clippy::too_many_arguments)]
    fn pixel_to_cell(
        &self,
        x: f64,
        y: f64,
        hist_start: u64,
        scroll_count: usize,
        view_offset: usize,
        rows: usize,
        cols: usize,
    ) -> (u64, usize) {
        let col_f = ((x as f32 - TEXT_PADDING) / self.cell_width).floor();
        let row_f = ((y as f32 - TEXT_PADDING) / self.line_height).floor();
        let col = col_f.clamp(0.0, cols.saturating_sub(1) as f32) as usize;
        let display_row = row_f.clamp(0.0, rows.saturating_sub(1) as f32) as usize;
        let g = hist_start + (scroll_count.saturating_sub(view_offset)) as u64 + display_row as u64;
        let max_g = hist_start + (scroll_count + rows) as u64 - 1;
        (g.min(max_g), col)
    }

    /// Grow the vertex buffer if the current capacity is too small for the grid.
    fn ensure_capacity(&mut self, gpu: &GpuContext, rows: usize, cols: usize) {
        let needed = rows * cols * 6;
        if needed > self.vertex_cap {
            self.vertex_buffer = gpu::create_colored_vertex_buffer(
                gpu.device(),
                &vec![ColoredVertex::default(); needed],
            );
            self.vertex_cap = needed;
        }
    }

    /// Build ColoredVertex quads for every cell in the current selection.
    /// Renders only the intersection of the selection range with the current viewport.
    fn build_vertices(&self, screen: &Screen, surf_w: f32, surf_h: f32) -> Vec<ColoredVertex> {
        let Some(ref range) = self.model.range else {
            return Vec::new();
        };
        // A zero-length range (anchor == cursor) has no area — nothing to render.
        if range.anchor == range.cursor {
            return Vec::new();
        }
        let (sg, sc, eg, ec) = range.normalized();
        let rows = screen.rows();
        let cols = screen.cols();
        let view_offset = screen.view_offset();
        let scroll_count = screen.scroll_count();
        let hist_start = screen.history_start();

        // Viewport generation range
        let view_start = hist_start + (scroll_count.saturating_sub(view_offset)) as u64;
        let view_end = view_start + rows as u64 - 1;
        // Clamp selection to viewport
        let loop_start = sg.max(view_start);
        let loop_end = eg.min(view_end);

        let mut verts = if loop_start <= loop_end {
            let visible_rows = (loop_end - loop_start + 1) as usize;
            Vec::with_capacity(visible_rows * cols * 6)
        } else {
            return Vec::new();
        };

        for g in loop_start..=loop_end {
            let display_row = (g - view_start) as usize;
            let col_start = if g == sg { sc } else { 0 };
            let col_end = if g == eg { ec } else { cols.saturating_sub(1) };

            for col in col_start..=col_end {
                let left = TEXT_PADDING + col as f32 * self.cell_width;
                let top = TEXT_PADDING + display_row as f32 * self.line_height;
                let right = left + self.cell_width;
                let bottom = top + self.line_height;
                let quad = ColoredVertex::from_pixel_rect(
                    left,
                    top,
                    right,
                    bottom,
                    SELECTION_COLOR,
                    surf_w,
                    surf_h,
                );
                verts.extend_from_slice(&quad);
            }
        }
        verts
    }

    /// Returns the currently selected text, or `None` when there is no active selection.
    fn selected_text(&self, screen: &Screen) -> Option<String> {
        if self.model.is_range_empty() {
            return None;
        }
        let bounds = self.model.bounds()?;
        let text = screen.selected_text(bounds);
        if text.is_empty() { None } else { Some(text) }
    }

    /// Intercepts Ctrl+C (copy selection) and Ctrl+V (paste). Returns
    /// `None` when the event is not a keyboard shortcut we handle.
    fn try_handle_keyboard<C>(
        &mut self,
        event: &winit::event::WindowEvent,
        caps: &mut C,
    ) -> Option<EventResult>
    where
        C: TerminalAccess + PtyAccess + ModifiersAccess,
    {
        let winit::event::WindowEvent::KeyboardInput { event: kbd, .. } = event else {
            return None;
        };
        if kbd.state != winit::event::ElementState::Pressed || !caps.modifiers().control_key() {
            return None;
        }

        match &kbd.logical_key {
            winit::keyboard::Key::Character(ch) if ch == "c" || ch == "C" => {
                let Some(text) = self.selected_text(caps.terminal().screen()) else {
                    return Some(EventResult::Continue);
                };
                if let Some(clipboard) = self.clipboard.as_mut()
                    && let Err(e) = clipboard.set_text(text)
                {
                    tracing::warn!(error = %e, "failed to set clipboard text");
                }
                Some(EventResult::Handled)
            }
            winit::keyboard::Key::Character(ch) if ch == "v" || ch == "V" => {
                if let Some(clipboard) = self.clipboard.as_mut() {
                    match clipboard.get_text() {
                        Ok(text) => caps.pty().write(text.as_bytes()),
                        Err(e) => tracing::warn!(error = %e, "failed to read clipboard text"),
                    }
                }
                // Always Handled — never send \x16 to the PTY.
                Some(EventResult::Handled)
            }
            _ => None,
        }
    }

    /// Apply [`SelectionOutcome`] — sets dirty flag on visual changes,
    fn apply_outcome(
        &mut self,
        outcome: SelectionOutcome,
        caps: &mut (impl ScrollAccess + RedrawAccess),
    ) {
        match outcome {
            SelectionOutcome::None => {}
            SelectionOutcome::DragActive => {
                self.dirty = true;
                caps.request_redraw();
                caps.set_auto_scrolling(true);
            }
            SelectionOutcome::DragEnded => {
                self.dirty = true;
                caps.request_redraw();
                caps.set_auto_scrolling(false);
            }
        }
    }

    fn handle_cursor_moved(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        caps: &mut (impl RedrawAccess + TerminalAccess),
    ) -> EventResult {
        self.last_cursor_pos = Some((position.x, position.y));

        if !self.model.is_dragging() {
            return EventResult::Continue;
        }

        let screen = caps.terminal().screen();
        let (g, col) = self.pixel_to_cell(
            position.x,
            position.y,
            screen.history_start(),
            screen.scroll_count(),
            screen.view_offset(),
            screen.rows(),
            screen.cols(),
        );
        if self.model.drag_to((g, col), screen) {
            self.dirty = true;
            caps.request_redraw();
        }

        EventResult::Handled
    }

    fn handle_mouse_input(
        &mut self,
        state: winit::event::ElementState,
        button: winit::event::MouseButton,
        caps: &mut (impl RedrawAccess + ScrollAccess + TerminalAccess),
    ) -> EventResult {
        if button != winit::event::MouseButton::Left {
            return EventResult::Continue;
        }

        match state {
            winit::event::ElementState::Pressed => {
                if let Some((x, y)) = self.last_cursor_pos {
                    let screen = caps.terminal().screen();
                    let (g, col) = self.pixel_to_cell(
                        x,
                        y,
                        screen.history_start(),
                        screen.scroll_count(),
                        screen.view_offset(),
                        screen.rows(),
                        screen.cols(),
                    );
                    let now = Instant::now();
                    let outcome = self.model.press((g, col), now, screen);
                    self.apply_outcome(outcome, caps);
                }
                EventResult::Handled
            }
            winit::event::ElementState::Released => {
                let outcome = self.model.release();
                self.apply_outcome(outcome, caps);
                EventResult::Handled
            }
        }
    }
}

// ── SelectionInput impl ──────────────────────────────────────────────────

impl SelectionInput for Selection {
    fn handle_event<C>(&mut self, event: &winit::event::WindowEvent, caps: &mut C) -> EventResult
    where
        C: TerminalAccess + RedrawAccess + PtyAccess + ModifiersAccess + ScrollAccess,
    {
        match event {
            // Keyboard press: try copy/paste first, then clear selection state.
            winit::event::WindowEvent::KeyboardInput { event: kbd, .. }
                if kbd.state == winit::event::ElementState::Pressed =>
            {
                let kb_result = self.try_handle_keyboard(event, caps);
                // Bare modifier keys (Ctrl, Shift, Alt, Super, etc.) are
                // part of a chord — don't clear selection until the actual
                // character key arrives.  Otherwise pressing Ctrl alone
                // would destroy the selection before Ctrl+C can copy.
                if !is_modifier_key(&kbd.logical_key) && self.model.on_key_press() {
                    self.dirty = true;
                    caps.request_redraw();
                }
                kb_result.unwrap_or(EventResult::Continue)
            }

            // Alt-screen mode: cancel any in-flight drag, pass through to app.
            _ if caps.terminal().is_alt_screen() => {
                let outcome = self.model.cancel();
                self.apply_outcome(outcome, caps);
                EventResult::Continue
            }

            winit::event::WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(*position, caps)
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(*state, *button, caps)
            }
            // Focus-loss mid-drag: release may go to another window.
            winit::event::WindowEvent::Focused(false) => {
                let outcome = self.model.cancel();
                self.apply_outcome(outcome, caps);
                EventResult::Continue
            }
            // Resize clears selection state — cancel any in-flight drag.
            winit::event::WindowEvent::Resized(_) => {
                let outcome = self.model.cancel();
                self.apply_outcome(outcome, caps);
                EventResult::Continue // don't consume — let UiRoot::resize fire
            }
            _ => EventResult::Continue,
        }
    }

    fn on_about_to_wait<C>(&mut self, caps: &mut C) -> Option<Instant>
    where
        C: TerminalAccess + ScrollAccess + RedrawAccess,
    {
        // Alt screen activated while dragging — cancel immediately.
        if self.model.is_dragging() && caps.terminal().is_alt_screen() {
            let outcome = self.model.cancel();
            self.apply_outcome(outcome, caps);
            return None;
        }

        // Early exit when no auto-scroll is active.
        self.model.auto_scroll_direction()?;

        let now = Instant::now();

        // Rate-limit check — return existing deadline if not yet due.
        if let Some(deadline) = self.model.auto_scroll_deadline()
            && deadline > now
        {
            return Some(deadline);
        }

        let screen = caps.terminal().screen();
        let (direction, new_cursor) = self.model.compute_auto_scroll_cursor(now, screen)?;

        // Execute the viewport scroll.
        match direction {
            AutoScroll::Up => caps.scroll_viewport_up(1),
            AutoScroll::Down => caps.scroll_viewport_down(1),
        }

        // Apply the new cursor position computed by the model.
        if let Some(ref mut sel) = self.model.range {
            sel.cursor = new_cursor;
        }

        self.dirty = true;
        caps.request_redraw();

        self.model.auto_scroll_deadline()
    }
}

// ── Component impl ───────────────────────────────────────────────────────

impl Component for Selection {
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let Some(screen) = screen else {
            self.vertex_count = 0;
            return;
        };

        if self.model.has_selection() {
            let rows = screen.rows();
            let cols = screen.cols();
            self.ensure_capacity(gpu, rows, cols);

            let (surf_w, surf_h) = gpu.surface_size();
            let verts = self.build_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.vertex_count = verts.len() as u32;
        } else {
            self.vertex_count = 0;
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }

    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        // Grid dimensions changed; old selection coordinates are stale.
        self.model.clear();
        self.dirty = true;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

/// Returns `true` when the logical key is a bare modifier or lock key.
/// These are chord keys — they don't produce terminal input on their own
/// and shouldn't clear the text selection.
fn is_modifier_key(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::Control
                | NamedKey::Shift
                | NamedKey::Alt
                | NamedKey::Super
                | NamedKey::AltGraph
                | NamedKey::Fn
                | NamedKey::FnLock
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::Symbol
                | NamedKey::SymbolLock
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, NamedKey};

    // ── is_modifier_key tests ─────────────────────────────────

    #[test]
    fn modifier_keys_are_detected() {
        assert!(is_modifier_key(&Key::Named(NamedKey::Control)));
        assert!(is_modifier_key(&Key::Named(NamedKey::Shift)));
        assert!(is_modifier_key(&Key::Named(NamedKey::Alt)));
        assert!(is_modifier_key(&Key::Named(NamedKey::Super)));
        assert!(is_modifier_key(&Key::Named(NamedKey::AltGraph)));
        assert!(is_modifier_key(&Key::Named(NamedKey::Fn)));
        assert!(is_modifier_key(&Key::Named(NamedKey::FnLock)));
        assert!(is_modifier_key(&Key::Named(NamedKey::Meta)));
        assert!(is_modifier_key(&Key::Named(NamedKey::Hyper)));
        assert!(is_modifier_key(&Key::Named(NamedKey::Symbol)));
        assert!(is_modifier_key(&Key::Named(NamedKey::SymbolLock)));
        assert!(is_modifier_key(&Key::Named(NamedKey::CapsLock)));
        assert!(is_modifier_key(&Key::Named(NamedKey::NumLock)));
        assert!(is_modifier_key(&Key::Named(NamedKey::ScrollLock)));
    }

    #[test]
    fn ordinary_keys_are_not_modifiers() {
        assert!(!is_modifier_key(&Key::Character("a".into())));
        assert!(!is_modifier_key(&Key::Character("c".into())));
        assert!(!is_modifier_key(&Key::Character("A".into())));
    }

    #[test]
    fn named_non_modifier_keys_are_not_modifiers() {
        assert!(!is_modifier_key(&Key::Named(NamedKey::Enter)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::Backspace)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::Tab)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::Escape)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::ArrowUp)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::ArrowDown)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::F1)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::F12)));
    }

    // ── SelectionModel unit tests ─────────────────────────────

    /// Helper: create a tiny screen with known content.
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

        let outcome = model.press((0, 3), now, &screen);

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
        model.press((0, 3), now, &screen);
        model.release();

        // Second click (within timeout) at same cell.
        let outcome = model.press((0, 3), now, &screen);

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
        model.press((0, 3), now, &screen);
        model.release();
        // Click 2.
        model.press((0, 3), now, &screen);
        model.release();
        // Click 3.
        let outcome = model.press((0, 3), now, &screen);

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

        model.press((0, 3), now, &screen);
        model.release();

        // Different cell — should reset to single click.
        let outcome = model.press((1, 5), now, &screen);
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

        let changed = model.drag_to((0, 5), &screen);
        assert!(!changed);
        assert!(!model.has_selection());
    }

    #[test]
    fn drag_to_updates_cursor() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen);

        let changed = model.drag_to((0, 7), &screen);
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

        model.press((0, 2), now, &screen);
        let changed = model.drag_to((0, 2), &screen);
        assert!(!changed);
    }

    #[test]
    fn drag_in_reverse_swaps_normalized_range() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // Press at col 7, drag left to col 2.
        model.press((0, 7), now, &screen);
        model.drag_to((0, 2), &screen);

        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 2);
        assert_eq!(bounds.end_col, 7);
    }

    #[test]
    fn release_during_drag_ends_drag() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen);
        assert!(model.is_dragging());
        model.drag_to((0, 5), &screen); // extend to non-zero-width

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

        model.press((0, 2), now, &screen);
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

        model.press((0, 2), now, &screen);
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

        model.press((0, 2), now, &screen);
        model.drag_to((0, 5), &screen);
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

        model.press((0, 2), now, &screen);
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

        model.press((0, 2), now, &screen);
        assert!(model.is_range_empty());
    }

    #[test]
    fn is_range_empty_false_after_drag() {
        let screen = test_screen(3, 10);
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen);
        model.drag_to((0, 5), &screen);
        assert!(!model.is_range_empty());
    }

    #[test]
    fn find_word_range_middle_of_word() {
        let screen = test_screen(3, 10);
        // "abcdefghij" — clicking 'd' (col 3) spans 0..9.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 3);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 9));
    }

    #[test]
    fn find_word_range_at_separator() {
        let mut screen = test_screen(3, 10);
        screen.cell_mut(0, 3).ch = ' ';
        // Clicking a space at col 3 gives zero-width word.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 3);
        assert_eq!(start, (0, 3));
        assert_eq!(end, (0, 3));
    }

    #[test]
    fn snap_word_cursor_forward_lands_at_word_end() {
        let screen = test_screen(3, 10);
        // "abcdefghij" — anchor at 2, cursor moving to 5.
        // Forward (>= anchor): lands at the end of the word (col 9).
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 5, (0, 2));
        assert_eq!(snapped, (0, 9));
    }

    #[test]
    fn snap_word_cursor_backward_lands_at_word_start() {
        let screen = test_screen(3, 10);
        // "abcdefghij" — anchor at 5, cursor moving to 2.
        // Backward (< anchor): lands at start of word (col 0).
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 2, (0, 5));
        assert_eq!(snapped, (0, 0));
    }

    #[test]
    fn snap_line_cursor_forward_lands_at_last_col() {
        let screen = test_screen(3, 10);
        let snapped = SelectionModel::snap_line_cursor(&screen, 0, 5, (0, 2));
        assert_eq!(snapped, (0, 9));
    }

    #[test]
    fn snap_line_cursor_backward_lands_at_col_zero() {
        let screen = test_screen(3, 10);
        let snapped = SelectionModel::snap_line_cursor(&screen, 0, 2, (0, 5));
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
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 1); // click '好'
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 3));
    }

    #[test]
    fn find_word_range_cjk_stops_at_separator() {
        // "你好 世界" — space at col 2.
        let screen = screen_with_text("你好 世界");
        // Click '好' (col 1) — should select only "你好" (cols 0-1).
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 1);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 1));
    }

    #[test]
    fn find_word_range_cjk_stops_at_latin() {
        // "hello世界" — Latin then CJK.
        let screen = screen_with_text("hello世界");
        // Click '世' (col 5) — CJK group: only cols 5-6.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 5);
        assert_eq!(start, (0, 5));
        assert_eq!(end, (0, 6));
        // Click 'o' (col 4) — Latin group: cols 0-4.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 4);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 4));
    }

    #[test]
    fn find_word_range_latin_stops_at_cjk() {
        // "你好world" — CJK then Latin.
        let screen = screen_with_text("你好world");
        // Click '你' (col 0) — CJK group: cols 0-1.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 0);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 1));
        // Click 'w' (col 2) — Latin group: cols 2-6.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 2);
        assert_eq!(start, (0, 2));
        assert_eq!(end, (0, 6));
    }

    #[test]
    fn find_word_range_cjk_punctuation_is_zero_width() {
        // CJK punctuation is in WORD_SEPARATORS, not in is_cjk.
        let screen = screen_with_text("你好，世界");
        // Click '，' (col 2) — separator → zero-width.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 2);
        assert_eq!(start, (0, 2));
        assert_eq!(end, (0, 2));
    }

    #[test]
    fn snap_word_cursor_cjk_forward_stays_at_char() {
        // "你好世界abcdef"
        let screen = screen_with_text("你好世界abcdef");
        // anchor at 0, cursor dragged to col 2 (世) → should stay at 2.
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 2, (0, 0));
        assert_eq!(snapped, (0, 2));
    }

    #[test]
    fn snap_word_cursor_cjk_forward_skips_sep_to_cjk() {
        // "你好 世界" — cols: 0=你, 1=好, 2=' ', 3=世, 4=界
        let screen = screen_with_text("你好 世界");
        // anchor at 0, cursor dragged to col 2 (space) →
        // forward snap skips space, lands on 世 (col 3), CJK → stay.
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 2, (0, 0));
        assert_eq!(snapped, (0, 3));
    }

    #[test]
    fn snap_word_cursor_cjk_forward_to_latin_expands() {
        // "你好world"
        let screen = screen_with_text("你好world");
        // anchor at 0, cursor dragged to col 2 (w) →
        // skip seps? none. w is Latin → expand to end of "world".
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 2, (0, 0));
        assert_eq!(snapped, (0, 6));
    }

    #[test]
    fn snap_word_cursor_cjk_backward_stays_at_char() {
        // "你好世界"
        let screen = screen_with_text("你好世界");
        // anchor at 3, cursor dragged to col 1 (好) backward.
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 1, (0, 3));
        assert_eq!(snapped, (0, 1));
    }

    #[test]
    fn double_click_cjk_selects_consecutive_run() {
        // "你好世界hello"
        let screen = screen_with_text("你好世界hello");
        let mut model = SelectionModel::new();
        let now = Instant::now();

        // First click: char selection.
        model.press((0, 1), now, &screen); // click '好'
        // Second click within timeout at same cell → double-click → Word mode.
        let now2 = now + Duration::from_millis(200);
        let outcome = model.press((0, 1), now2, &screen);
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
        model.press((0, 1), now, &screen);
        let now2 = now + Duration::from_millis(200);
        model.press((0, 1), now2, &screen);

        // Initial: entire "你好世界" (0..3).
        assert_eq!(model.bounds().unwrap().start_col, 0);
        assert_eq!(model.bounds().unwrap().end_col, 3);

        // Drag to col 2 (世) — forward snap, CJK stays at 2.
        model.drag_to((0, 2), &screen);
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 2);

        // Drag to col 1 (好) — selection should be 0..1.
        model.drag_to((0, 1), &screen);
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 1);

        // Drag to col 0 (你) — zero-width (anchor == cursor).
        model.drag_to((0, 0), &screen);
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
                UnicodeWidthChar::width(ch).unwrap_or(0).max(1).min(2)
            })
            .sum();
        let mut s = Screen::new(2, total_cols);
        let mut col = 0;
        for &ch in &chars {
            use unicode_width::UnicodeWidthChar;
            let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1).min(2);
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
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 2);
        assert_eq!(start, (0, 0)); // 你
        assert_eq!(end, (0, 7)); // 界's wide_continuation
    }

    #[test]
    fn find_word_range_wide_cjk_click_on_continuation_redirects() {
        // Click on wide_continuation cell of '好' (col 3).
        let screen = screen_with_wide_text("你好世界");
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 3);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 7));
    }

    #[test]
    fn find_word_range_wide_cjk_stops_at_separator() {
        // "你好 世界" — space at cols 4-4, then 世界 at 5-8.
        let screen = screen_with_wide_text("你好 世界");
        // Click '好' (col 2) → should select "你好" (cols 0-3).
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 2);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 3)); // includes 好's wide_continuation
    }

    #[test]
    fn find_word_range_wide_cjk_stops_at_latin() {
        // "hello世" — "hello" cols 0-4, "世" cols 5-6.
        let screen = screen_with_wide_text("hello世");
        // Click '世' (col 5) → CJK group: cols 5-6 only.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 5);
        assert_eq!(start, (0, 5));
        assert_eq!(end, (0, 6));
        // Click 'o' (col 4) → Latin group: cols 0-4.
        let (start, end) = SelectionModel::find_word_range(&screen, 0, 4);
        assert_eq!(start, (0, 0));
        assert_eq!(end, (0, 4));
    }

    #[test]
    fn snap_word_cursor_wide_cjk_forward_snaps_to_char_end() {
        // "你好世界" → 8 cells: 你(0),wc(1),好(2),wc(3),世(4),wc(5),界(6),wc(7)
        let screen = screen_with_wide_text("你好世界");
        // anchor at 0, cursor dragged to col 4 (世's real cell)
        // → CJK: advance to wide_continuation at col 5.
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 4, (0, 0));
        assert_eq!(snapped, (0, 5));
    }

    #[test]
    fn snap_word_cursor_wide_cjk_backward_stays_at_char_start() {
        let screen = screen_with_wide_text("你好世界");
        // anchor at 7 (界's wc), cursor dragged to col 3 (好's wc).
        // backward: skip seps, step off wc to col 2 (好), CJK → stay.
        let snapped = SelectionModel::snap_word_cursor(&screen, 0, 3, (0, 7));
        assert_eq!(snapped, (0, 2));
    }

    #[test]
    fn double_click_wide_cjk_selects_full_run() {
        let screen = screen_with_wide_text("你好世界");
        let mut model = SelectionModel::new();
        let now = Instant::now();

        model.press((0, 2), now, &screen); // click '好' at col 2
        let now2 = now + Duration::from_millis(200);
        let outcome = model.press((0, 2), now2, &screen);
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
        model.press((0, 2), now, &screen);
        let now2 = now + Duration::from_millis(200);
        model.press((0, 2), now2, &screen);

        // Initial: entire run (0..7).
        assert_eq!(model.bounds().unwrap().start_col, 0);
        assert_eq!(model.bounds().unwrap().end_col, 7);

        // Drag to col 4 (世) → snaps to col 5 (wc).
        model.drag_to((0, 4), &screen);
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 5); // 你好世

        // Drag to col 2 (好) → snaps to col 3 (wc).
        model.drag_to((0, 2), &screen);
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 3); // 你好

        // Drag to col 0 (你) → snaps to col 1 (wc).
        model.drag_to((0, 0), &screen);
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds.start_col, 0);
        assert_eq!(bounds.end_col, 1); // 你
    }
}
