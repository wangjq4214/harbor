use std::time::Instant;

use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{
    gpu::{self, GpuContext, TexturedVertex},
    metrics::{TEXT_PADDING, TextMetrics},
    render::Layer,
    terminal::Screen,
};

const CURSOR_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@group(0) @binding(0) var cursor_texture: texture_2d<f32>;
@group(0) @binding(1) var cursor_sampler: sampler;
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(cursor_texture, cursor_sampler, in.uv).r;
    return vec4<f32>(1.0, 1.0, 1.0, alpha * 0.8);
}
"#;

// ── CursorLayer ───────────────────────────────────────────────────────────

/// Draws a blinking block cursor over the terminal grid.
/// Uses a `dirty` flag to defer GPU vertex uploads.
pub(crate) struct CursorLayer {
    /// wgpu render pipeline.
    pipeline: wgpu::RenderPipeline,
    /// Cursor texture bind group (texture + sampler).
    bind_group: wgpu::BindGroup,
    /// Cursor greyscale texture (held alive by this field).
    _texture: wgpu::Texture,
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
    /// Baseline ascent.
    ascent: f32,
    /// Cursor glyph pixel width.
    glyph_width: f32,
    /// Cursor glyph pixel height.
    glyph_height: f32,
    /// fontdue vertical offset.
    glyph_ymin: f32,
    /// Snapshot from the last `prepare`: `(visible, cursor_x, cursor_y)`.
    /// Used to skip uploads when nothing changed.
    last_cursor: Option<(bool, usize, usize)>,
}

impl CursorLayer {
    /// Creates the cursor layer: initialises the render pipeline, cursor
    /// texture/sampler bind group, and vertex buffer with COPY_DST.
    pub(crate) fn new(
        gpu: &GpuContext,
        metrics: TextMetrics,
        cursor_bitmap: &[u8],
        cursor_metrics: fontdue::Metrics,
    ) -> Self {
        let (pipeline, bind_group, _texture) = {
            let bind_group_layout = gpu::create_texture_bind_group_layout(gpu.device());
            let pipeline = Self::create_pipeline(gpu.device(), gpu.format(), &bind_group_layout);
            let gpu_width = cursor_metrics.width.max(1) as u32;
            let gpu_height = cursor_metrics.height.max(1) as u32;
            let texture = gpu.device().create_texture_with_data(
                gpu.queue(),
                &wgpu::TextureDescriptor {
                    label: Some("cursor glyph texture"),
                    size: wgpu::Extent3d {
                        width: gpu_width,
                        height: gpu_height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                cursor_bitmap,
            );
            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
                label: Some("cursor glyph sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cursor glyph bind group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            (pipeline, bind_group, texture)
        };

        let vertex_buffer = gpu::create_vertex_buffer(
            gpu.device(),
            &[TexturedVertex {
                position: [0.0; 2],
                tex_coords: [0.0; 2],
            }; 6],
        );

        let glyph_width = cursor_metrics.width as f32;
        let glyph_height = cursor_metrics.height as f32;
        let glyph_ymin = cursor_metrics.ymin as f32;
        Self {
            pipeline,
            bind_group,
            _texture,
            vertex_buffer,
            vertex_count: 0,
            visible: false,
            dirty: false,
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            ascent: metrics.ascent,
            glyph_width,
            glyph_height,
            glyph_ymin,
            last_cursor: None,
        }
    }

    /// Creates the cursor render pipeline using the shared `TexturedVertex`
    /// layout and bind group layout.
    fn create_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cursor shader"),
            source: wgpu::ShaderSource::Wgsl(CURSOR_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cursor pipeline layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
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

    /// Updates the cursor visibility flag.  Compares the `(visible, cursor_x,
    /// cursor_y)` tuple against the snapshot recorded by the last `prepare`;
    /// sets `dirty = true` only when something changed.
    /// GPU vertex upload is deferred to `Layer::prepare`.
    pub(crate) fn set_visible(&mut self, visible: bool, screen: &Screen) {
        self.visible = visible;
        let current =
            if visible && screen.cursor_y() < screen.rows() && screen.cursor_x() < screen.cols() {
                (visible, screen.cursor_x(), screen.cursor_y())
            } else {
                (false, 0, 0)
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
    /// If `dirty`, computes pixel → NDC transformed vertices and uploads them
    /// via `write_buffer`. Returns immediately when nothing changed.
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
            let baseline =
                TEXT_PADDING + self.ascent.ceil() + screen.cursor_y() as f32 * self.line_height;
            let glyph_bottom = baseline - self.glyph_ymin;
            let glyph_top = glyph_bottom - self.glyph_height;
            let left = cell_x;
            let right = cell_x + self.glyph_width;

            let vertices = TexturedVertex::from_pixel_rect(
                left,
                glyph_top,
                right,
                glyph_bottom,
                0.0,
                0.0,
                1.0,
                1.0,
                surf_w as f32,
                surf_h as f32,
            );

            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
            self.vertex_count = 6;
            self.last_cursor = Some((self.visible, screen.cursor_x(), screen.cursor_y()));
        } else {
            self.vertex_count = 0;
            self.last_cursor = None;
        }
    }

    /// Sets the pipeline + bind group and issues the draw call.
    fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

// ── CursorBlink ───────────────────────────────────────────────────────────

/// Cursor blink phase state machine.  Decides visibility based on a 530ms
/// on/off cycle and requests redraws when the phase toggles.
pub(crate) struct CursorBlink {
    /// Blink timer start (set to `Instant::now` on creation).
    blink_start: Instant,
    /// Visibility state from the last rendered frame, used to detect toggles.
    last_rendered_visible: bool,
}

const BLINK_INTERVAL_MS: u64 = 530;

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

// ── Cursor rasterisation ──────────────────────────────────────────────────

/// Rasterises the block-cursor glyph at the given size.  Tries the box-drawing
/// vertical bar `│` (U+2502) first, falling back to ASCII pipe `|` (U+007C).
pub(crate) fn rasterize_cursor(
    fonts: &crate::font::FontBook,
    size: f32,
) -> anyhow::Result<(fontdue::Metrics, Vec<u8>)> {
    for ch in ['\u{2502}', '|'] {
        let (metrics, bitmap) = fonts.rasterize(ch, size);
        if metrics.width > 0 && metrics.height > 0 && bitmap.iter().any(|&b| b != 0) {
            return Ok((metrics, bitmap));
        }
    }
    anyhow::bail!(
        "primary font lacks both '│' (U+2502) and '|' (U+007C); unsuitable for a terminal cursor"
    )
}
