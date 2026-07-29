use harbor_types::TerminalSnapshot;
use std::sync::Arc;

use super::gpu::{self, ColoredVertex, GpuContext};
use arboard::Clipboard;
use harbor_config::{SELECTION_COLOR, TEXT_PADDING};

use crate::{SelectionGranularity, SelectionModel};

// ── Selection (outer — GPU) ──────────────────────────────────────

pub struct Selection {
    model: SelectionModel,
    pipeline: Arc<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    /// Number of vertices to draw (0 when no selection).
    vertex_count: u32,
    /// Current vertex buffer capacity (rows * cols * 6).
    vertex_cap: usize,
    /// Cached from the most recent CursorMoved event (physical pixels).
    #[allow(dead_code)]
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
            tracing::warn!(?error, "failed to copy text to clipboard");
        }
        true
    }

    pub fn set_copy_pending(&mut self, request_id: u64) {
        self.pending_copy = Some(request_id);
    }

    /// Converts physical pixel coordinates `(x, y)` to grid `(row, col)`
    /// relative to global line space.
    #[allow(dead_code)]
    fn pixel_to_cell(
        &self,
        px: f64,
        py: f64,
        history_start: usize,
        scroll_count: usize,
        view_offset: usize,
        rows: usize,
        cols: usize,
    ) -> (isize, usize) {
        let content_x = (px as f32 - TEXT_PADDING).max(0.0);
        let content_y = (py as f32 - TEXT_PADDING).max(0.0);

        let display_row = (content_y / self.line_height).floor() as usize;
        let display_row = display_row.min(rows.saturating_sub(1));

        let col = (content_x / self.cell_width).floor() as usize;
        let col = col.min(cols.saturating_sub(1));

        let view_start_g = history_start + scroll_count - view_offset;
        let global_line = (view_start_g + display_row) as isize;

        (global_line, col)
    }

    /// Ensures the vertex buffer capacity can hold `rows * cols * 6` vertices.
    fn ensure_capacity(&mut self, gpu: &GpuContext, rows: usize, cols: usize) {
        let required = rows * cols * 6;
        if required > self.vertex_cap {
            let cap = required.max(64);
            self.vertex_buffer = gpu::create_colored_vertex_buffer(
                gpu.device(),
                &vec![ColoredVertex::default(); cap],
            );
            self.vertex_cap = cap;
        }
    }

    /// Builds solid-color quad vertices for all cells covered by the selection.
    fn build_vertices(
        &self,
        snap: &TerminalSnapshot,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        let Some(bounds) = self.model.bounds() else {
            return Vec::new();
        };

        let sg = bounds.start_row;
        let sc = bounds.start_col;
        let eg = bounds.end_row;
        let ec = bounds.end_col;

        let history_start = snap.history_start;
        let scroll_count = snap.scroll_count;
        let view_offset = snap.view_offset;
        let rows = snap.rows;
        let cols = snap.cols;

        let view_start = history_start + (scroll_count.saturating_sub(view_offset)) as u64;
        let view_end = view_start + rows as u64 - 1;

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

    pub fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
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
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.vertex_count = verts.len() as u32;
        } else {
            self.vertex_count = 0;
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }

    pub fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        // Grid dimensions changed; old selection coordinates are stale.
        self.model.clear();
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_initial_state() {
        // Simple sanity test for Selection struct
        assert_eq!(std::mem::size_of::<SelectionGranularity>(), 1);
    }
}
