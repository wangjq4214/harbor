use harbor_text::TextMetrics;
use harbor_types::TerminalSnapshot;
use std::time::Instant;

use super::gpu::{self, GpuContext, TexturedVertex};
use crate::CursorShape;
use harbor_config::{BLINK_INTERVAL_MS, TEXT_PADDING};

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
    /// Start time of the current blink cycle (reset on keypress / position change).
    blink_start: Instant,
    /// Last rendered blink visibility state (used to trigger redraws on toggle).
    last_rendered_visible: bool,
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
            blink_start: Instant::now(),
            last_rendered_visible: true,
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            last_cursor: None,
            dirty: true,
        }
    }

    /// Resets the blink timer (makes cursor solid-on immediately).
    pub fn reset_blink(&mut self) {
        self.blink_start = Instant::now();
        self.dirty = true;
    }

    /// Calculates whether cursor is in the visible phase of the blink cycle.
    pub fn blink_visible(&self) -> bool {
        let elapsed_ms = self.blink_start.elapsed().as_millis() as u64;
        (elapsed_ms / BLINK_INTERVAL_MS).is_multiple_of(2)
    }

    pub fn commit_frame(&mut self) {
        self.last_rendered_visible = self.visible;
        self.dirty = false;
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

    pub fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
        let Some(snap) = snap else {
            self.vertex_count = 0;
            self.last_cursor = None;
            return;
        };

        self.visible = should_render_cursor(snap, self.blink_visible());
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

    pub fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::should_render_cursor;
    use crate::Terminal;

    #[test]
    fn dectcem_controls_rendered_cursor_visibility() {
        let mut terminal = Terminal::new_headless(3, 3);
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
