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

// ── Selection model ─────────────────────────────────────────────────────────

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

// ── Selection ────────────────────────────────────────────────

pub(crate) struct Selection {
    pipeline: Arc<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    /// Number of vertices to draw (0 when no selection).
    vertex_count: u32,
    /// Current vertex buffer capacity (rows * cols * 6).
    vertex_cap: usize,
    /// None = no active selection.
    selection: Option<SelectionRange>,
    /// True while left mouse button is held.
    dragging: bool,
    /// Cached from the most recent CursorMoved event (physical pixels).
    /// Needed because winit 0.30 MouseInput does not carry a position.
    last_cursor_pos: Option<(f64, f64)>,
    cell_width: f32,
    line_height: f32,
    /// Whether vertex buffer needs re-upload.
    dirty: bool,
    /// System clipboard handle (None when clipboard is unavailable, e.g. headless).
    clipboard: Option<Clipboard>,
    /// Auto-scroll direction while dragging at viewport edge (None = idle).
    auto_scroll: Option<AutoScroll>,
    /// Deadline for the next auto-scroll tick (rate-limited to AUTO_SCROLL_INTERVAL_MS).
    next_auto_scroll_at: Option<Instant>,
    /// Current selection granularity (Character / Word / Line).
    granularity: SelectionGranularity,
    /// Timestamp of the most recent `MouseInput::Pressed` (for click-chain detection).
    last_click_at: Option<Instant>,
    /// Grid cell of the most recent click (gen, col).
    last_click_cell: Option<(u64, usize)>,
    /// Consecutive click count (1 = single, 2 = double, 3+ = triple/line).
    click_count: u32,
}

impl Selection {
    pub(crate) fn new(gpu: &GpuContext, cell_width: f32, line_height: f32) -> Self {
        let pipeline = gpu.colored_quad_pipeline();
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &[]);
        Self {
            pipeline,
            vertex_buffer,
            vertex_count: 0,
            vertex_cap: 0,
            selection: None,
            dragging: false,
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
            auto_scroll: None,
            next_auto_scroll_at: None,
            granularity: SelectionGranularity::default(),
            last_click_at: None,
            last_click_cell: None,
            click_count: 0,
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
        let sel = self.selection.unwrap();
        // A zero-length range (anchor == cursor) has no area — nothing to render.
        if sel.anchor == sel.cursor {
            return Vec::new();
        }
        let (sg, sc, eg, ec) = sel.normalized();
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
        let sel = self.selection?;
        // A zero-length range has no text to copy.
        if sel.anchor == sel.cursor {
            return None;
        }

        let (start_row, start_col, end_row, end_col) = sel.normalized();
        let text = screen.selected_text(SelectionBounds {
            start_row,
            start_col,
            end_row,
            end_col,
        });
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

    // ── Multi-click word/line boundary helpers ──────────────────────

    /// Returns `((start_gen, start_col), (end_gen, end_col))` of the word
    /// at `(generation, col)`.  When the clicked cell is a separator, the word
    /// range is zero-width (just that cell).
    fn find_word_range(
        screen: &Screen,
        generation: u64,
        col: usize,
    ) -> ((u64, usize), (u64, usize)) {
        let cols = screen.cols();
        let is_sep = |c: usize| {
            screen
                .cell_at_generation(generation, c)
                .is_none_or(|cell| WORD_SEPARATORS.contains(&cell.ch))
        };

        if is_sep(col) {
            return ((generation, col), (generation, col));
        }

        // Scan left to word start
        let mut left = col;
        while left > 0 && !is_sep(left - 1) {
            left -= 1;
        }

        // Scan right to word end
        let mut right = col;
        while right < cols - 1 && !is_sep(right + 1) {
            right += 1;
        }

        ((generation, left), (generation, right))
    }

    /// Snap cursor column for word-wise drag.  Returns `(generation, snapped_col)`.
    fn snap_word_cursor(
        screen: &Screen,
        generation: u64,
        col: usize,
        anchor: (u64, usize),
    ) -> (u64, usize) {
        let is_sep = |c: usize| {
            screen
                .cell_at_generation(generation, c)
                .is_none_or(|cell| WORD_SEPARATORS.contains(&cell.ch))
        };
        let cols = screen.cols();

        if (generation, col) >= anchor {
            // Expanding forward → snap to right boundary.
            let mut c = col;
            // Skip forward past any separator run.
            while c < cols - 1 && is_sep(c) {
                c += 1;
            }
            // Find end of the word at/before c.
            while c < cols - 1 && !is_sep(c + 1) {
                c += 1;
            }
            (generation, c)
        } else {
            // Expanding backward → snap to left boundary.
            let mut c = col;
            // Skip backward past any separator run.
            while c > 0 && is_sep(c) {
                c -= 1;
            }
            // Find start of the word at/before c.
            while c > 0 && !is_sep(c - 1) {
                c -= 1;
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

    fn handle_cursor_moved(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        caps: &mut (impl RedrawAccess + TerminalAccess),
    ) -> EventResult {
        self.last_cursor_pos = Some((position.x, position.y));

        if !self.dragging {
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
        if let Some(sel) = &mut self.selection
            && sel.cursor != (g, col)
        {
            let anchor = sel.anchor; // Copy to avoid borrow conflict with self.snap_*
            let new_cursor = match self.granularity {
                SelectionGranularity::Word => Self::snap_word_cursor(screen, g, col, anchor),
                SelectionGranularity::Line => Self::snap_line_cursor(screen, g, col, anchor),
                SelectionGranularity::Character => (g, col),
            };
            sel.cursor = new_cursor;
            self.dirty = true;
            caps.request_redraw();
        }

        // Auto-scroll direction detection
        let rows = screen.rows();
        let view_offset = screen.view_offset();
        let scroll_count = screen.scroll_count();
        let display_row = ((g - screen.history_start()) as usize + view_offset)
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

        EventResult::Handled
    }

    /// Cancel an in-progress drag, resetting auto-scroll state and
    /// re-enabling the PTY scroll-to-bottom snap.
    fn cancel_drag(&mut self, caps: &mut impl ScrollAccess) {
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
        caps.set_auto_scrolling(false);
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

                    // ── Click chain detection ──────────────────────────
                    let now = Instant::now();
                    let in_timeout = self.last_click_at.is_some_and(|t| {
                        now.duration_since(t).as_millis() < MULTI_CLICK_TIMEOUT_MS as u128
                    });
                    let same_cell = self.last_click_cell == Some((g, col));

                    if in_timeout && same_cell {
                        self.click_count = self.click_count.saturating_add(1);
                    } else {
                        self.click_count = 1;
                    }
                    self.last_click_at = Some(now);
                    self.last_click_cell = Some((g, col));

                    // ── Set selection range by click count ─────────────
                    let cols = screen.cols();
                    match self.click_count {
                        1 => {
                            self.granularity = SelectionGranularity::Character;
                            self.selection = Some(SelectionRange {
                                anchor: (g, col),
                                cursor: (g, col),
                            });
                        }
                        2 => {
                            self.granularity = SelectionGranularity::Word;
                            let (start, end) = Self::find_word_range(screen, g, col);
                            self.selection = Some(SelectionRange {
                                anchor: start,
                                cursor: end,
                            });
                        }
                        _ => {
                            // Triple click and beyond → line selection.
                            self.granularity = SelectionGranularity::Line;
                            let last = cols.saturating_sub(1);
                            self.selection = Some(SelectionRange {
                                anchor: (g, 0),
                                cursor: (g, last),
                            });
                        }
                    }

                    self.dragging = true;
                    caps.set_auto_scrolling(true);
                    self.dirty = true;
                    caps.request_redraw();
                }
                EventResult::Handled
            }
            winit::event::ElementState::Released => {
                if self.dragging {
                    self.cancel_drag(caps);
                    // Click without drag → clear selection.
                    if let Some(sel) = self.selection
                        && sel.anchor == sel.cursor
                    {
                        self.selection = None;
                        self.dirty = true;
                        caps.request_redraw();
                    }
                }
                EventResult::Handled
            }
        }
    }
}

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
                if !is_modifier_key(&kbd.logical_key) {
                    self.cancel_drag(caps);
                    self.selection = None;
                    self.dirty = true;
                    caps.request_redraw();
                }
                kb_result.unwrap_or(EventResult::Continue)
            }

            // Alt-screen mode: cancel any in-flight drag, pass through to app.
            _ if caps.terminal().is_alt_screen() => {
                if self.dragging {
                    self.cancel_drag(caps);
                }
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
                if self.dragging {
                    self.cancel_drag(caps);
                }
                EventResult::Continue
            }
            // Resize clears selection state — cancel any in-flight drag.
            winit::event::WindowEvent::Resized(_) => {
                if self.dragging {
                    self.cancel_drag(caps);
                }
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
        if self.dragging && caps.terminal().is_alt_screen() {
            self.cancel_drag(caps);
            return None;
        }

        let scroll = self.auto_scroll?;
        if !self.dragging {
            self.auto_scroll = None;
            self.next_auto_scroll_at = None;
            return None;
        }
        // Rate limit: don't scroll faster than AUTO_SCROLL_INTERVAL_MS.
        if let Some(deadline) = self.next_auto_scroll_at
            && deadline > Instant::now()
        {
            return Some(deadline);
        }
        // Extract scroll conditions before any mutable caps calls to avoid
        // conflicting borrows through `screen`.
        let can_scroll_up = {
            let s = caps.terminal().screen();
            s.view_offset() < s.scroll_count()
        };
        let can_scroll_down = {
            let s = caps.terminal().screen();
            s.view_offset() > 0
        };

        match scroll {
            AutoScroll::Up if can_scroll_up => {
                caps.scroll_viewport_up(1);
                // Advance selection cursor to include the newly revealed row above.
                if let Some(sel) = &mut self.selection {
                    // Decrease generation to extend selection upward (into scrollback).
                    sel.cursor.0 = sel.cursor.0.saturating_sub(1);
                    // Re-snap column when in word or line mode.
                    let anchor = sel.anchor;
                    let cur = sel.cursor;
                    let snapped = match self.granularity {
                        SelectionGranularity::Word => {
                            let screen = caps.terminal().screen();
                            Self::snap_word_cursor(screen, cur.0, cur.1, anchor)
                        }
                        SelectionGranularity::Line => {
                            let screen = caps.terminal().screen();
                            Self::snap_line_cursor(screen, cur.0, cur.1, anchor)
                        }
                        SelectionGranularity::Character => cur,
                    };
                    sel.cursor = snapped;
                }
                self.dirty = true;
                caps.request_redraw();
            }
            AutoScroll::Down if can_scroll_down => {
                caps.scroll_viewport_down(1);
                // Increase generation to extend selection downward (toward live content).
                if let Some(sel) = &mut self.selection {
                    sel.cursor.0 = sel.cursor.0.saturating_add(1);
                    // Re-snap column when in word or line mode.
                    let anchor = sel.anchor;
                    let cur = sel.cursor;
                    let snapped = match self.granularity {
                        SelectionGranularity::Word => {
                            let screen = caps.terminal().screen();
                            Self::snap_word_cursor(screen, cur.0, cur.1, anchor)
                        }
                        SelectionGranularity::Line => {
                            let screen = caps.terminal().screen();
                            Self::snap_line_cursor(screen, cur.0, cur.1, anchor)
                        }
                        SelectionGranularity::Character => cur,
                    };
                    sel.cursor = snapped;
                }
                self.dirty = true;
                caps.request_redraw();
            }
            _ => {
                self.auto_scroll = None;
                self.next_auto_scroll_at = None;
                return None;
            }
        }
        let deadline = Instant::now() + Duration::from_millis(AUTO_SCROLL_INTERVAL_MS);
        self.next_auto_scroll_at = Some(deadline);
        Some(deadline)
    }
}

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

        if let Some(_sel) = self.selection {
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
        self.selection = None;
        self.dragging = false;
        self.auto_scroll = None;
        self.next_auto_scroll_at = None;
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::is_modifier_key;
    use winit::keyboard::{Key, NamedKey};

    #[test]
    fn modifier_keys_are_detected() {
        // Chord keys — must NOT clear selection.
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
        // Character keys — MUST clear selection.
        assert!(!is_modifier_key(&Key::Character("a".into())));
        assert!(!is_modifier_key(&Key::Character("c".into())));
        assert!(!is_modifier_key(&Key::Character("A".into())));
    }

    #[test]
    fn named_non_modifier_keys_are_not_modifiers() {
        // Named keys that produce terminal output — MUST clear selection.
        assert!(!is_modifier_key(&Key::Named(NamedKey::Enter)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::Backspace)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::Tab)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::Escape)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::ArrowUp)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::ArrowDown)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::F1)));
        assert!(!is_modifier_key(&Key::Named(NamedKey::F12)));
    }
}
