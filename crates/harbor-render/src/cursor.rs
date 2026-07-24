use harbor_types::TerminalSnapshot;
use std::time::Instant;

use crate::{
    caps::{InteractionResult, UiRequest, WaitResult},
    Component, EventResult,
    gpu::{self, GpuContext, TexturedVertex},
    text::TextMetrics,
};
use harbor_config::{BLINK_INTERVAL_MS, TEXT_PADDING};
use harbor_terminal::CursorShape;

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

fn should_render_cursor(snap: &TerminalSnapshot, blink_visible: bool) -> bool {
    snap.cursor_visible && (!snap.cursor_blink || blink_visible)
}

/// Combined cursor rendering + blink state machine.
/// Replaces CursorLayer and CursorBlink in the component tree.
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
    /// Whether vertices need to be re-uploaded to the GPU.
    dirty: bool,
    /// Cell width derived from font metrics, used to compute cursor quad position.
    cell_width: f32,
    /// Line height, used for cursor quad height and underline/bar thickness.
    line_height: f32,
    /// Current cursor shape (block / underline / bar), set by DECSCUSR.
    shape: CursorShape,
    /// Snapshot from the last upload, used to skip uploads when nothing changed.
    last_cursor: Option<LastCursorState>,
    /// Blink timer start (set to `Instant::now` on construction).
    blink_start: Instant,
    /// Visibility state from the last committed frame, used to detect blink toggles.
    last_rendered_visible: bool,
}

impl Cursor {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Creates the cursor: pipeline, vertex buffer, and blink timer at `Instant::now`.
    pub fn new(gpu: &GpuContext, metrics: TextMetrics) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());
        let vertex_buffer =
            gpu::create_vertex_buffer(gpu.device(), &[TexturedVertex::default(); 6]);
        Self {
            pipeline,
            vertex_buffer,
            vertex_count: 0,
            visible: false,
            dirty: false,
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            shape: CursorShape::Bar,
            last_cursor: None,
            blink_start: Instant::now(),
            last_rendered_visible: false,
        }
    }

    /// Compiles the solid-color cursor shader into a render pipeline.
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

    /// Whether the cursor should be visible right now (blink-phase check).
    fn blink_visible(&self) -> bool {
        let millis = self.blink_start.elapsed().as_millis() as u64;
        (millis / BLINK_INTERVAL_MS).is_multiple_of(2)
    }

    /// Snapshots the current blink phase so the next `on_about_to_wait`
    /// can detect a toggle.
    fn commit_frame(&mut self) {
        self.last_rendered_visible = self.blink_visible();
    }
    fn set_visible(&mut self, visible: bool, snap: &TerminalSnapshot) {
        self.visible = visible;
        self.shape = snap.cursor_shape;
        let current = if visible && snap.cursor_y < snap.rows && snap.cursor_x < snap.cols {
            LastCursorState {
                visible,
                x: snap.cursor_x,
                y: snap.cursor_y,
                shape: self.shape,
            }
        } else {
            LastCursorState {
                visible: false,
                x: 0,
                y: 0,
                shape: self.shape,
            }
        };
        if self.last_cursor != Some(current) {
            self.dirty = true;
        }
    }
}

impl Component for Cursor {
    /// If dirty, computes cell-aligned vertex quad for the current cursor
    /// shape and uploads it.
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
        let Some(snap) = snap else {
            self.vertex_count = 0;
            self.last_cursor = None;
            return;
        };

        let visible = should_render_cursor(snap, self.blink_visible());
        self.set_visible(visible, snap);

        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.visible && snap.cursor_y < snap.rows && snap.cursor_x < snap.cols {
            let (surf_w, surf_h) = gpu.surface_size();
            let cell_x = TEXT_PADDING + snap.cursor_x as f32 * self.cell_width;
            let cell_y = TEXT_PADDING + snap.cursor_y as f32 * self.line_height;

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
                left,
                top,
                right,
                bottom,
                0.0,
                0.0,
                1.0,
                1.0, // UV unused, shader outputs solid color
                [1.0; 4],
                surf_w as f32,
                surf_h as f32,
            );
            gpu.write_buffer(&self.vertex_buffer,
            0,
            bytemuck::cast_slice(&vertices),);
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
    fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }

    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.dirty = true;
    }
}

impl Cursor {
    pub fn handle_event(&mut self, _event: &winit::event::WindowEvent, _snapshot: &TerminalSnapshot) -> InteractionResult {
        InteractionResult::continue_()
    }

    pub fn on_about_to_wait(&mut self, snapshot: &TerminalSnapshot) -> WaitResult {
        if !snapshot.cursor_visible || !snapshot.cursor_blink { return WaitResult::default(); }
        let visible = self.blink_visible();
        let mut result = WaitResult::default();
        if visible != self.last_rendered_visible { self.dirty = true; result.requests.push(UiRequest::Redraw); }
        let millis = self.blink_start.elapsed().as_millis() as u64;
        let next_toggle_ms = ((millis / BLINK_INTERVAL_MS) + 1) * BLINK_INTERVAL_MS;
        result.deadline = Some(self.blink_start + std::time::Duration::from_millis(next_toggle_ms));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::should_render_cursor;
    use harbor_terminal::Terminal;

    #[test]
    fn dectcem_controls_rendered_cursor_visibility() {
        let mut terminal = Terminal::new(3, 3);
        assert!(should_render_cursor(&terminal.snapshot(), true));
        assert!(!should_render_cursor(&terminal.snapshot(), false));

        terminal.put_bytes(b"\x1b[2 q");
        assert!(should_render_cursor(&terminal.snapshot(), false));

        terminal.put_bytes(b"\x1b[?25l");
        assert!(!should_render_cursor(&terminal.snapshot(), true));

        terminal.put_bytes(b"\x1b[?25h");
        assert!(should_render_cursor(&terminal.snapshot(), true));
    }
}
