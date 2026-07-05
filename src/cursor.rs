use std::time::Instant;

use winit::window::Window;

use crate::{
    config::{BLINK_INTERVAL_MS, TEXT_PADDING},
    gpu::{self, GpuContext, TexturedVertex},
    metrics::TextMetrics,
    render::Layer,
    terminal::{CursorShape, Screen},
};

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

// ── CursorLayer ───────────────────────────────────────────────────────────

/// Draws a solid-color cursor (block/underline/bar) over the terminal grid.
/// Uses a `dirty` flag to defer GPU vertex uploads.
pub(crate) struct CursorLayer {
    /// wgpu render pipeline.
    pipeline: wgpu::RenderPipeline,
    /// Vertex buffer with COPY_DST for partial write_buffer updates.
    vertex_buffer: wgpu::Buffer,
    /// Current vertex count (0 or 6).
    vertex_count: u32,
    /// Cursor visibility set by the client (controlled by CursorBlink).
    visible: bool,
    /// Whether vertices need to be re-uploaded to the GPU.
    dirty: bool,
    /// Cell width derived from font metrics.
    cell_width: f32,
    /// Line height.
    line_height: f32,
    /// Current cursor shape from DECSCUSR.
    shape: CursorShape,
    /// Snapshot from the last `prepare`: `(visible, cursor_x, cursor_y, shape)`.
    /// Used to skip uploads when nothing changed.
    last_cursor: Option<(bool, usize, usize, CursorShape)>,
}

impl CursorLayer {
    /// Creates the cursor layer: initialises the render pipeline and vertex buffer.
    pub(crate) fn new(gpu: &GpuContext, metrics: TextMetrics) -> Self {
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
        }
    }

    /// Creates the cursor render pipeline using the shared `TexturedVertex` layout.
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

    /// Updates the cursor visibility flag and reads the current shape from
    /// the screen.  Compares the `(visible, cursor_x, cursor_y, shape)` tuple
    /// against the snapshot recorded by the last `prepare`; sets `dirty =
    /// true` only when something changed.
    pub(crate) fn set_visible(&mut self, visible: bool, screen: &Screen) {
        self.visible = visible;
        self.shape = screen.cursor_shape();
        let current =
            if visible && screen.cursor_y() < screen.rows() && screen.cursor_x() < screen.cols() {
                (visible, screen.cursor_x(), screen.cursor_y(), self.shape)
            } else {
                (false, 0, 0, self.shape)
            };
        if self.last_cursor != Some(current) {
            self.dirty = true;
        }
    }

    /// Forces a vertex upload on the next `prepare` (called after resize).
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl Layer for CursorLayer {
    /// If `dirty`, computes cell-aligned vertex rectangles for the current
    /// shape (block/underline/bar) and uploads them via `write_buffer`.
    /// Returns immediately when nothing changed.
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let Some(screen) = screen else {
            self.vertex_count = 0;
            self.last_cursor = None;
            return;
        };

        if self.visible && screen.cursor_y() < screen.rows() && screen.cursor_x() < screen.cols() {
            let (surf_w, surf_h) = gpu.surface_size();
            let cell_x = TEXT_PADDING + screen.cursor_x() as f32 * self.cell_width;
            let cell_y = TEXT_PADDING + screen.cursor_y() as f32 * self.line_height;

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
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
            self.vertex_count = 6;
            self.last_cursor = Some((
                self.visible,
                screen.cursor_x(),
                screen.cursor_y(),
                self.shape,
            ));
        } else {
            self.vertex_count = 0;
            self.last_cursor = None;
        }
    }

    /// Sets the pipeline and issues the draw call.
    fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

// ── CursorBlink ───────────────────────────────────────────────────────────

/// Cursor blink phase state machine.  Decides visibility based on a configured
/// on/off cycle and requests redraws when the phase toggles.
pub(crate) struct CursorBlink {
    /// Blink timer start (set to `Instant::now` on creation).
    blink_start: Instant,
    /// Visibility state from the last rendered frame, used to detect toggles.
    last_rendered_visible: bool,
}

impl CursorBlink {
    pub(crate) fn new() -> Self {
        Self {
            blink_start: Instant::now(),
            last_rendered_visible: false,
        }
    }

    /// Whether the cursor should be visible at this instant (time-based,
    /// independent of render state).
    pub(crate) fn visible(&self) -> bool {
        let millis = self.blink_start.elapsed().as_millis() as u64;
        (millis / BLINK_INTERVAL_MS).is_multiple_of(2)
    }

    /// Called from `about_to_wait`: requests a redraw if visibility just
    /// toggled, returns the next toggle deadline for `ControlFlow::WaitUntil`.
    pub(crate) fn check_blink(&mut self, window: Option<&Window>) -> Instant {
        let visible = self.visible();
        if visible != self.last_rendered_visible
            && let Some(window) = window
        {
            window.request_redraw();
        }
        let millis = self.blink_start.elapsed().as_millis() as u64;
        let next_toggle_ms = ((millis / BLINK_INTERVAL_MS) + 1) * BLINK_INTERVAL_MS;
        self.blink_start + std::time::Duration::from_millis(next_toggle_ms)
    }

    /// Called after each frame is rendered: records the current blink state
    /// so `check_blink` can detect the next toggle.
    pub(crate) fn commit_frame(&mut self) {
        self.last_rendered_visible = self.visible();
    }
}
