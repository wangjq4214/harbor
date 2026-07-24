use harbor_types::TerminalSnapshot;
use std::sync::Arc;
use std::time::Instant;

use crate::{
    caps::{InteractionResult, UiRequest, WaitResult},
    gpu::{self, ColoredVertex, GpuContext},
    Component, EventResult,
};
use arboard::Clipboard;
use harbor_config::{SELECTION_COLOR, TEXT_PADDING};
use harbor_terminal::{self, PasteDisposition};
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
    /// Request id awaiting an asynchronous worker copy response.
    last_cursor_pos: Option<(f64, f64)>,
    pending_copy: Option<u64>,
    cell_width: f32,
    line_height: f32,
    /// Whether vertex buffer needs re-upload.
    dirty: bool,
    /// System clipboard handle (None when clipboard is unavailable, e.g. headless).
    clipboard: Option<Clipboard>,
}

impl Selection {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

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
            pending_copy: None,
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

    /// Completes a worker copy request and updates the UI-owned clipboard.
    pub fn apply_copy_result(&mut self, result: harbor_types::CopySelectionResult) -> bool {
        if self.pending_copy != Some(result.request_id) {
            return false;
        }
        self.pending_copy = None;
        if result.text.is_empty() {
            return true;
        }
        if let Some(clipboard) = self.clipboard.as_mut()
            && let Err(error) = clipboard.set_text(result.text)
        {
            tracing::warn!(%error, "failed to set clipboard text");
        }
        true
    }

    pub fn set_copy_pending(&mut self, request_id: u64) {
        self.pending_copy = Some(request_id);
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
        snap: &TerminalSnapshot,
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

    fn outcome_requests(&mut self, outcome: SelectionOutcome, requests: &mut Vec<UiRequest>) {
        match outcome {
            SelectionOutcome::None => {}
            SelectionOutcome::DragActive => { self.dirty = true; requests.extend([UiRequest::Redraw, UiRequest::SetSelectionDragActive(true)]); }
            SelectionOutcome::DragEnded => { self.dirty = true; requests.extend([UiRequest::Redraw, UiRequest::SetSelectionDragActive(false)]); }
        }
    }

    pub fn handle_event(&mut self, event: &winit::event::WindowEvent, snapshot: &TerminalSnapshot, modifiers: winit::keyboard::ModifiersState) -> InteractionResult {
        let mut requests = Vec::new();
        let event_result = match event {
            winit::event::WindowEvent::KeyboardInput { event: key, .. } if key.state == winit::event::ElementState::Pressed => {
                let ctrl = modifiers.control_key();
                let shift = modifiers.shift_key();
                if ctrl && matches!(&key.logical_key, Key::Character(ch) if ch.eq_ignore_ascii_case("c")) {
                    if self.pending_copy.is_none() && !self.model.is_range_empty() {
                        if let Some(bounds) = self.model.bounds() { requests.push(UiRequest::Copy(bounds)); }
                    }
                    EventResult::Handled
                } else if (ctrl && matches!(&key.logical_key, Key::Character(ch) if ch.eq_ignore_ascii_case("v"))) || (shift && matches!(&key.logical_key, Key::Named(NamedKey::Insert))) {
                    if let Some(text) = self.clipboard.as_mut().and_then(|clipboard| clipboard.get_text().ok()) {
                        match PasteDisposition::decide(snapshot.input_modes, &text) {
                            PasteDisposition::SendDirect => requests.push(UiRequest::Paste(text)),
                            PasteDisposition::Confirm { raw_text } => return InteractionResult { event: EventResult::ConfirmPaste(raw_text), requests },
                        }
                    }
                    EventResult::Handled
                } else {
                    if !is_modifier_key(&key.logical_key) && self.model.on_key_press() { self.dirty = true; requests.push(UiRequest::Redraw); }
                    EventResult::Continue
                }
            }
            _ if snapshot.is_alt => { let outcome = self.model.cancel(); self.outcome_requests(outcome, &mut requests); EventResult::Continue }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_pos = Some((position.x, position.y));
                if !self.model.is_dragging() { EventResult::Continue } else {
                    let cell = self.pixel_to_cell(position.x, position.y, snapshot.history_start, snapshot.scroll_count, snapshot.view_offset, snapshot.rows, snapshot.cols);
                    if self.model.drag_to(cell, snapshot) { self.dirty = true; requests.push(UiRequest::Redraw); }
                    EventResult::Handled
                }
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } if *button == winit::event::MouseButton::Left => {
                let outcome = match state {
                    winit::event::ElementState::Pressed => self.last_cursor_pos.map(|(x,y)| { let cell=self.pixel_to_cell(x,y,snapshot.history_start,snapshot.scroll_count,snapshot.view_offset,snapshot.rows,snapshot.cols); self.model.press(cell, Instant::now(), snapshot) }).unwrap_or(SelectionOutcome::None),
                    winit::event::ElementState::Released => self.model.release(),
                };
                self.outcome_requests(outcome, &mut requests); EventResult::Handled
            }
            winit::event::WindowEvent::Focused(false) | winit::event::WindowEvent::Resized(_) => { let outcome = self.model.cancel(); self.outcome_requests(outcome, &mut requests); EventResult::Continue }
            _ => EventResult::Continue,
        };
        InteractionResult { event: event_result, requests }
    }

    pub fn on_about_to_wait(&mut self, snapshot: &TerminalSnapshot) -> WaitResult {
        let mut result = WaitResult::default();
        if self.model.is_dragging() && snapshot.is_alt { let outcome = self.model.cancel(); self.outcome_requests(outcome, &mut result.requests); return result; }
        self.model.auto_scroll_direction();
        let now = Instant::now();
        if let Some(deadline) = self.model.auto_scroll_deadline() && deadline > now { result.deadline = Some(deadline); return result; }
        if let Some((direction, cursor)) = self.model.compute_auto_scroll_cursor(now, snapshot) {
            result.requests.push(UiRequest::Scroll(match direction { AutoScroll::Up => -1, AutoScroll::Down => 1 }));
            if let Some(range) = self.model.range.as_mut() { range.cursor = cursor; }
            self.dirty = true; result.requests.push(UiRequest::Redraw); result.deadline = self.model.auto_scroll_deadline();
        }
        result
    }
}

// ── Component impl ───────────────────────────────────────────────────────

impl Component for Selection {
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
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
            gpu.write_buffer(&self.vertex_buffer,
            0,
            bytemuck::cast_slice(&verts),);
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
