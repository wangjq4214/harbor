use crate::model::TerminalSnapshot;
use harbor_config::Rgba;
use harbor_text::TextMetrics;

use super::gpu::{self, ColoredVertex, TerminalGpuAccess};
use crate::CursorShape;
use crate::render::RenderViewport;

// ── Cursor ──────────────────────────────────────────────────────

/// Snapshot of cursor state used to detect position/shape changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LastCursorState {
    x: usize,
    y: usize,
    shape: CursorShape,
}

fn should_render_cursor(
    snap: &TerminalSnapshot,
    blink_visible: bool,
    preedit_active: bool,
) -> bool {
    !preedit_active && snap.cursor_visible && (!snap.cursor_blink || blink_visible)
}

/// Concrete GPU cursor projection; blink timing belongs to the engine.
pub struct Cursor {
    /// wgpu render pipeline for the solid-color cursor quad.
    pipeline: wgpu::RenderPipeline,
    /// Pre-allocated 6-vertex quad buffer (rewritten when cursor position or
    /// visibility changes).
    vertex_buffer: wgpu::Buffer,
    /// A cached visible, in-bounds quad; None means there is nothing to draw.
    /// Invalidation leaves the previous quad drawable until the next prepare.
    last_cursor: Option<LastCursorState>,
    color: Rgba,
    /// Set true when window size, metrics, or color change.
    dirty: bool,
}

impl Cursor {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn new(gpu: TerminalGpuAccess<'_>, _metrics: TextMetrics, color: Rgba) -> Self {
        let pipeline =
            gpu::create_colored_quad_pipeline(gpu.device(), gpu.format(), "cursor pipeline");
        let vertex_buffer =
            gpu::create_colored_vertex_buffer(gpu.device(), &[ColoredVertex::default(); 6]);
        Self {
            pipeline,
            vertex_buffer,
            last_cursor: None,
            color,
            dirty: true,
        }
    }

    pub(crate) fn set_color(&mut self, color: Rgba) {
        self.color = color;
        self.dirty = true;
    }

    pub fn commit_frame(&mut self) {
        self.dirty = false;
    }

    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
        preedit_active: bool,
        blink_visible: bool,
    ) {
        let Some(snap) = snap else {
            self.last_cursor = None;
            return;
        };

        let visible = should_render_cursor(snap, blink_visible, preedit_active);
        let state = LastCursorState {
            x: snap.cursor_x,
            y: snap.cursor_y,
            shape: snap.cursor_shape,
        };

        // Keep the original cache check ahead of the bounds/viewport checks:
        // those inputs only take effect after invalidation or a state change.
        if !self.dirty && visible && self.last_cursor == Some(state) {
            return;
        }

        if visible && snap.cursor_x < snap.cols && snap.cursor_y < snap.rows {
            let (surf_w, surf_h) = viewport.surface_dimensions();
            let (cell_x, cell_y) = viewport.cell_pos(snap.cursor_y, snap.cursor_x);

            let (left, top, right, bottom) = match state.shape {
                CursorShape::Block => (
                    cell_x,
                    cell_y,
                    cell_x + viewport.cell_width,
                    cell_y + viewport.line_height,
                ),
                CursorShape::Underline => {
                    let thickness = (viewport.line_height * 0.1).max(2.0);
                    (
                        cell_x,
                        cell_y + viewport.line_height - thickness,
                        cell_x + viewport.cell_width,
                        cell_y + viewport.line_height,
                    )
                }
                CursorShape::Bar => {
                    let thickness = (viewport.cell_width * 0.15).max(2.0);
                    (
                        cell_x,
                        cell_y,
                        cell_x + thickness,
                        cell_y + viewport.line_height,
                    )
                }
            };

            let vertices = ColoredVertex::from_pixel_rect(
                left,
                top,
                right,
                bottom,
                self.color.components(),
                surf_w,
                surf_h,
            );
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
            self.last_cursor = Some(state);
        } else {
            self.last_cursor = None;
        }
        self.commit_frame();
    }

    /// Sets the pipeline and issues the draw call if a quad is cached.
    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.last_cursor.is_none() {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }

    pub fn invalidate_projection(&mut self) {
        self.dirty = true;
    }
}

#[cfg(test)]
#[path = "cursor_tests/gpu.rs"]
mod cursor_gpu_tests;

#[cfg(test)]
mod tests {
    use super::should_render_cursor;
    use crate::model::TerminalSnapshot;
    use crate::render::CursorBlinkState;
    use crate::{FrameDemand, Terminal};
    use harbor_config::BLINK_INTERVAL_MS;
    use std::time::{Duration, Instant};

    fn demand_from_blink(
        snap: &TerminalSnapshot,
        blink: &CursorBlinkState,
        now: Instant,
    ) -> FrameDemand {
        let deadline = if snap.cursor_visible && snap.cursor_blink {
            Some(blink.next_deadline(now))
        } else {
            None
        };
        FrameDemand {
            redraw_now: blink.pending_redraw(),
            deadline,
            ordinary_present_eligible: true,
        }
    }

    #[test]
    fn preedit_and_dectcem_control_rendered_cursor_visibility() {
        let mut terminal = Terminal::new_headless(3, 3);
        assert!(should_render_cursor(&terminal.snapshot(), true, false));
        assert!(!should_render_cursor(&terminal.snapshot(), false, false));
        assert!(!should_render_cursor(&terminal.snapshot(), true, true));
        assert!(!should_render_cursor(&terminal.snapshot(), false, true));

        terminal.put_bytes(b"\x1b[2 q");
        assert!(should_render_cursor(&terminal.snapshot(), false, false));
        assert!(!should_render_cursor(&terminal.snapshot(), false, true));

        terminal.put_bytes(b"\x1b[?25l");
        assert!(!should_render_cursor(&terminal.snapshot(), true, false));
        assert!(!should_render_cursor(&terminal.snapshot(), true, true));

        terminal.put_bytes(b"\x1b[?25h");
        assert!(should_render_cursor(&terminal.snapshot(), true, false));
    }

    #[test]
    fn should_include_deadline_when_cursor_visible_and_blinking() {
        // Arrange
        let terminal = Terminal::new_headless(3, 3);
        let t0 = Instant::now();
        let blink = CursorBlinkState::new(t0);

        // Act
        let demand = demand_from_blink(&terminal.snapshot(), &blink, t0);

        // Assert
        assert!(!demand.redraw_now);
        assert_eq!(
            demand.deadline,
            Some(t0 + Duration::from_millis(BLINK_INTERVAL_MS))
        );
        assert!(demand.ordinary_present_eligible);
    }

    #[test]
    fn should_omit_deadline_when_cursor_style_is_steady() {
        // Arrange
        let mut terminal = Terminal::new_headless(3, 3);
        let t0 = Instant::now();
        let blink = CursorBlinkState::new(t0);
        terminal.put_bytes(b"\x1b[2 q"); // steady block

        // Act
        let demand = demand_from_blink(&terminal.snapshot(), &blink, t0);

        // Assert
        assert!(demand.deadline.is_none());
        assert!(!demand.redraw_now);
    }

    #[test]
    fn should_omit_deadline_when_dectcem_hides_cursor() {
        // Arrange
        let mut terminal = Terminal::new_headless(3, 3);
        let t0 = Instant::now();
        let blink = CursorBlinkState::new(t0);
        terminal.put_bytes(b"\x1b[0 q"); // blinking block
        terminal.put_bytes(b"\x1b[?25l");

        // Act
        let demand = demand_from_blink(&terminal.snapshot(), &blink, t0);

        // Assert
        assert!(demand.deadline.is_none());
        assert!(!demand.redraw_now);
    }

    #[test]
    fn should_mark_redraw_and_restart_deadline_when_reset_from_hidden_phase() {
        // Arrange
        let t0 = Instant::now();
        let mut blink = CursorBlinkState::new(t0);
        let hidden_at = t0 + Duration::from_millis(BLINK_INTERVAL_MS);
        blink.reset(hidden_at);
        let snap = Terminal::new_headless(3, 3).snapshot();

        // Act
        let demand = demand_from_blink(&snap, &blink, hidden_at);

        // Assert
        assert!(demand.redraw_now);
        assert_eq!(
            demand.deadline,
            Some(hidden_at + Duration::from_millis(BLINK_INTERVAL_MS))
        );
        assert!(blink.phase_visible(hidden_at));
    }

    #[test]
    fn should_gate_draw_visibility_with_same_phase_as_frame_demand() {
        // Arrange — prepare uses should_render_cursor(snap, blink.phase_visible(now))
        let mut terminal = Terminal::new_headless(3, 3);
        let t0 = Instant::now();
        let blink = CursorBlinkState::new(t0);
        let hidden_at = t0 + Duration::from_millis(BLINK_INTERVAL_MS);
        let snap = terminal.snapshot();

        // Act + Assert — blinking visible phase
        assert!(should_render_cursor(&snap, blink.phase_visible(t0), false));
        // blinking hidden phase
        assert!(!should_render_cursor(
            &snap,
            blink.phase_visible(hidden_at),
            false
        ));
        terminal.put_bytes(b"\x1b[2 q"); // steady: draw even when phase would be hidden
        let steady = terminal.snapshot();
        assert!(should_render_cursor(
            &steady,
            blink.phase_visible(hidden_at),
            false
        ));
    }
}
