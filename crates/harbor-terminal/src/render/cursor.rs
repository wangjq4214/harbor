use harbor_text::TextMetrics;
use harbor_types::TerminalSnapshot;
use std::time::Instant;

use super::cursor_blink::CursorBlinkState;
use super::gpu::{self, GpuContext, TexturedVertex};
use crate::CursorShape;
use crate::FrameDemand;
use crate::render::RenderViewport;

const CURSOR_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 0.8);
}
"#;

// ── Cursor ──────────────────────────────────────────────────────

/// Snapshot of cursor state used to detect position/shape changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LastCursorState {
    visible: bool,
    x: usize,
    y: usize,
    shape: CursorShape,
}

fn should_render_cursor(
    snap: &TerminalSnapshot,
    blink_visible: bool,
    cursor_focused: bool,
) -> bool {
    snap.cursor_visible && (!snap.cursor_blink || !cursor_focused || blink_visible)
}

fn cursor_frame_demand(
    snap: &TerminalSnapshot,
    blink: &CursorBlinkState,
    now: Instant,
    cursor_focused: bool,
) -> FrameDemand {
    let deadline = if cursor_focused && snap.cursor_visible && snap.cursor_blink {
        Some(blink.next_deadline(now))
    } else {
        None
    };
    FrameDemand {
        redraw_now: cursor_focused && blink.pending_redraw(),
        deadline,
        ordinary_present_eligible: true,
    }
}

/// Combined cursor rendering + blink state machine.
pub struct Cursor {
    /// wgpu render pipeline for the solid-color cursor quad.
    pipeline: wgpu::RenderPipeline,
    /// Pre-allocated 6-vertex quad buffer (rewritten when cursor position or
    /// visibility changes).
    vertex_buffer: wgpu::Buffer,
    /// Number of vertices to draw (0 when cursor is off-snap or hidden).
    vertex_count: u32,
    /// Whether the cursor should be rendered this frame (controlled by blink
    /// timer or steady-on when blinking is disabled).
    visible: bool,
    /// Cursor shape (Block, Underline, Bar).
    shape: CursorShape,
    /// Idle blink phase and pending immediate-redraw flag.
    blink: CursorBlinkState,
    /// Cell dimensions in logical pixels.
    cell_width: f32,
    line_height: f32,
    /// Cached state from last prepare call to avoid re-writing vertex buffer.
    last_cursor: Option<LastCursorState>,
    /// Set true when window size changes or metric updates occur.
    dirty: bool,
}

impl Cursor {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn new(gpu: &GpuContext, metrics: TextMetrics) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());
        let vertex_buffer =
            gpu::create_vertex_buffer(gpu.device(), &[TexturedVertex::default(); 6]);
        Self {
            pipeline,
            vertex_buffer,
            vertex_count: 0,
            visible: true,
            shape: CursorShape::Block,
            blink: CursorBlinkState::new(Instant::now()),
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            last_cursor: None,
            dirty: true,
        }
    }

    /// Resets the blink timer (makes cursor solid-on immediately).
    pub fn reset_blink(&mut self, now: Instant) {
        self.blink.reset(now);
        self.dirty = true;
    }

    /// Host-neutral frame demand derived from blink state and screen cursor flags.
    pub fn frame_demand(
        &self,
        snap: &TerminalSnapshot,
        now: Instant,
        cursor_focused: bool,
    ) -> FrameDemand {
        cursor_frame_demand(snap, &self.blink, now, cursor_focused)
    }

    pub fn commit_frame(&mut self) {
        self.dirty = false;
        self.blink.take_pending_redraw();
    }

    fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cursor shader"),
            source: wgpu::ShaderSource::Wgsl(CURSOR_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cursor pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cursor pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(TexturedVertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
        now: Instant,
        cursor_focused: bool,
    ) {
        let Some(snap) = snap else {
            self.vertex_count = 0;
            self.last_cursor = None;
            return;
        };

        self.visible = should_render_cursor(snap, self.blink.phase_visible(now), cursor_focused);
        self.shape = snap.cursor_shape;

        let state_changed = self.last_cursor.is_none_or(|last| {
            last.visible != self.visible
                || last.x != snap.cursor_x
                || last.y != snap.cursor_y
                || last.shape != self.shape
        });

        if !self.dirty && !state_changed {
            return;
        }

        if self.visible && snap.cursor_x < snap.cols && snap.cursor_y < snap.rows {
            let (surf_w, surf_h) = viewport.surface_dimensions();
            let (cell_x, cell_y) = viewport.cell_pos(snap.cursor_y, snap.cursor_x);

            let (left, top, right, bottom) = match self.shape {
                CursorShape::Block => (
                    cell_x,
                    cell_y,
                    cell_x + self.cell_width,
                    cell_y + self.line_height,
                ),
                CursorShape::Underline => {
                    let thickness = (self.line_height * 0.1).max(2.0);
                    (
                        cell_x,
                        cell_y + self.line_height - thickness,
                        cell_x + self.cell_width,
                        cell_y + self.line_height,
                    )
                }
                CursorShape::Bar => {
                    let thickness = (self.cell_width * 0.15).max(2.0);
                    (
                        cell_x,
                        cell_y,
                        cell_x + thickness,
                        cell_y + self.line_height,
                    )
                }
            };

            let vertices = TexturedVertex::from_pixel_rect(
                left, top, right, bottom, 0.0, 0.0, 1.0,
                1.0, // UV unused, shader outputs solid color
                [1.0; 4], surf_w, surf_h,
            );
            gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
            self.vertex_count = 6;
            self.last_cursor = Some(LastCursorState {
                visible: self.visible,
                x: snap.cursor_x,
                y: snap.cursor_y,
                shape: self.shape,
            });
        } else {
            self.vertex_count = 0;
            self.last_cursor = None;
        }
        self.commit_frame();
    }

    /// Sets the pipeline and issues the draw call. No-op when vertex_count is 0.
    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }

    pub fn invalidate_projection(&mut self) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{cursor_frame_demand, should_render_cursor};
    use crate::render::CursorBlinkState;
    use crate::{FrameDemand, Terminal};
    use harbor_config::BLINK_INTERVAL_MS;
    use harbor_types::TerminalSnapshot;
    use std::time::{Duration, Instant};

    fn demand_from_blink(
        snap: &TerminalSnapshot,
        blink: &CursorBlinkState,
        now: Instant,
    ) -> FrameDemand {
        cursor_frame_demand(snap, blink, now, true)
    }

    #[test]
    fn dectcem_controls_rendered_cursor_visibility() {
        let mut terminal = Terminal::new_headless(3, 3);
        assert!(should_render_cursor(&terminal.snapshot(), true, true));
        assert!(!should_render_cursor(&terminal.snapshot(), false, true));
        assert!(should_render_cursor(&terminal.snapshot(), true, false));
        assert!(should_render_cursor(&terminal.snapshot(), false, false));

        terminal.put_bytes(b"\x1b[2 q");
        assert!(should_render_cursor(&terminal.snapshot(), false, true));
        assert!(should_render_cursor(&terminal.snapshot(), false, false));

        terminal.put_bytes(b"\x1b[?25l");
        assert!(!should_render_cursor(&terminal.snapshot(), true, true));
        assert!(!should_render_cursor(&terminal.snapshot(), true, false));

        terminal.put_bytes(b"\x1b[?25h");
        assert!(should_render_cursor(&terminal.snapshot(), true, true));
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
    fn should_omit_cursor_deadline_and_redraw_when_pane_is_not_focused() {
        let terminal = Terminal::new_headless(3, 3);
        let t0 = Instant::now();
        let mut blink = CursorBlinkState::new(t0);
        blink.reset(t0);

        let demand = cursor_frame_demand(&terminal.snapshot(), &blink, t0, false);

        assert_eq!(demand, FrameDemand::empty());
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
        assert!(should_render_cursor(&snap, blink.phase_visible(t0), true));
        // blinking hidden phase
        assert!(!should_render_cursor(
            &snap,
            blink.phase_visible(hidden_at),
            true
        ));
        // Unfocused panes retain a steady cursor without scheduling blink frames.
        assert!(should_render_cursor(
            &snap,
            blink.phase_visible(hidden_at),
            false
        ));

        terminal.put_bytes(b"\x1b[2 q"); // steady: draw even when phase would be hidden
        let steady = terminal.snapshot();
        assert!(should_render_cursor(
            &steady,
            blink.phase_visible(hidden_at),
            true
        ));
    }
}
