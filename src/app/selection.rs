use harbor_types::RenderSnapshot;
use std::sync::Arc;
use std::time::Instant;

use super::{
    EventResult,
    caps::{
        ModifiersAccess, PtyAccess, RedrawAccess, ScrollAccess, SelectionInput, TerminalAccess,
    },
};
use arboard::Clipboard;
use harbor_config::{SELECTION_COLOR, TEXT_PADDING};
use harbor_gpu::{
    GpuContext,
    gpu::{self, ColoredVertex},
};
use harbor_terminal::{self, PasteDisposition, Screen};
use harbor_ui::Component;
use winit::keyboard::{Key, NamedKey};

use harbor_terminal::{AutoScroll, SelectionModel, SelectionOutcome};

// ── Selection (outer — GPU + events) ──────────────────────────────────────

pub struct Selection {
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
    pub fn new(gpu: &GpuContext, cell_width: f32, line_height: f32) -> Self {
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
    fn build_vertices(
        &self,
        snap: &RenderSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        let Some(ref range) = self.model.range else {
            return Vec::new();
        };
        // A zero-length range (anchor == cursor) has no area — nothing to render.
        if range.anchor == range.cursor {
            return Vec::new();
        }
        let (sg, sc, eg, ec) = range.normalized();
        let rows = snap.rows;
        let cols = snap.cols;
        let view_offset = snap.view_offset;
        let scroll_count = snap.scroll_count;
        let hist_start = snap.history_start;

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
        if kbd.state != winit::event::ElementState::Pressed {
            return None;
        }
        let ctrl = caps.modifiers().control_key();
        let shift = caps.modifiers().shift_key();
        if !ctrl && !shift {
            return None;
        }

        /// Reads clipboard and returns the paste disposition or None on error.
        fn read_paste_text(clipboard: &mut Option<arboard::Clipboard>) -> Option<String> {
            clipboard.as_mut().and_then(|cb| match cb.get_text() {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to read clipboard text");
                    None
                }
            })
        }

        // Ctrl+V paste
        if ctrl {
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
                    return Some(EventResult::Handled);
                }
                winit::keyboard::Key::Character(ch) if ch == "v" || ch == "V" => {
                    if let Some(text) = read_paste_text(&mut self.clipboard) {
                        let modes = caps.terminal().screen().input_modes();
                        match PasteDisposition::decide(modes, &text) {
                            PasteDisposition::SendDirect => {
                                caps.pty().write(&modes.paste(text.as_bytes()));
                            }
                            PasteDisposition::Confirm { raw_text } => {
                                return Some(EventResult::ConfirmPaste(raw_text));
                            }
                        }
                    }
                    return Some(EventResult::Handled);
                }
                _ => {}
            }
        }

        // Shift+Insert paste
        if shift
            && let winit::keyboard::Key::Named(winit::keyboard::NamedKey::Insert) = &kbd.logical_key
        {
            if let Some(text) = read_paste_text(&mut self.clipboard) {
                let modes = caps.terminal().screen().input_modes();
                match PasteDisposition::decide(modes, &text) {
                    PasteDisposition::SendDirect => {
                        caps.pty().write(&modes.paste(text.as_bytes()));
                    }
                    PasteDisposition::Confirm { raw_text } => {
                        return Some(EventResult::ConfirmPaste(raw_text));
                    }
                }
            }
            return Some(EventResult::Handled);
        }

        None
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

        let snap = caps.terminal().screen();
        let (g, col) = self.pixel_to_cell(
            position.x,
            position.y,
            snap.history_start(),
            snap.scroll_count(),
            snap.view_offset(),
            snap.rows(),
            snap.cols(),
        );
        if self.model.drag_to((g, col), snap) {
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
                    let snap = caps.terminal().screen();
                    let (g, col) = self.pixel_to_cell(
                        x,
                        y,
                        snap.history_start(),
                        snap.scroll_count(),
                        snap.view_offset(),
                        snap.rows(),
                        snap.cols(),
                    );
                    let now = Instant::now();
                    let outcome = self.model.press((g, col), now, snap);
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

            // Alt-snap mode: cancel any in-flight drag, pass through to app.
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
        // Alt snap activated while dragging — cancel immediately.
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

        let snap = caps.terminal().screen();
        let (direction, new_cursor) = self.model.compute_auto_scroll_cursor(now, snap)?;

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
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&RenderSnapshot>) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let Some(snap) = snap else {
            self.vertex_count = 0;
            return;
        };

        if self.model.has_selection() {
            let rows = snap.rows;
            let cols = snap.cols;
            self.ensure_capacity(gpu, rows, cols);

            let (surf_w, surf_h) = gpu.surface_size();
            let verts = self.build_vertices(snap, surf_w as f32, surf_h as f32);
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
}
