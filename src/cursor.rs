use std::time::Instant;

use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{metrics::TEXT_PADDING, render::Render, terminal::Screen};

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

/// A single vertex for the cursor quad.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CursorVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl CursorVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Draws a blinking block cursor over the terminal grid.
pub(crate) struct CursorRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    _texture: wgpu::Texture,
    vertices: wgpu::Buffer,
    vertex_count: u32, // 0 or 6
    visible: bool,
    cell_width: f32,
    line_height: f32,
    ascent: f32,
    glyph_width: f32,
    glyph_height: f32,
    glyph_ymin: f32,
}

impl CursorRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        cell_width: f32,
        line_height: f32,
        ascent: f32,
        cursor_bitmap: &[u8],
        cursor_metrics: fontdue::Metrics,
    ) -> Self {
        let (pipeline, bind_group, _texture) = {
            let bind_group_layout = Self::create_bind_group_layout(device);
            let pipeline = Self::create_pipeline(device, format, &bind_group_layout);
            let gpu_width = cursor_metrics.width.max(1) as u32;
            let gpu_height = cursor_metrics.height.max(1) as u32;
            let texture = device.create_texture_with_data(
                queue,
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
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("cursor glyph sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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

        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cursor vertices"),
            contents: bytemuck::cast_slice(
                &[CursorVertex { position: [0.0; 2], uv: [0.0; 2] }; 6],
            ),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let glyph_width = cursor_metrics.width as f32;
        let glyph_height = cursor_metrics.height as f32;
        let glyph_ymin = cursor_metrics.ymin as f32;
        Self {
            pipeline,
            bind_group,
            _texture,
            vertices,
            vertex_count: 0,
            visible: false,
            cell_width,
            line_height,
            ascent,
            glyph_width,
            glyph_height,
            glyph_ymin,
        }
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cursor glyph bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

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
                buffers: &[Some(CursorVertex::layout())],
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

    /// Sets the cursor visibility position for the current frame.
    pub(crate) fn set_visible(
        &mut self,
        visible: bool,
        screen: &Screen,
        queue: &wgpu::Queue,
        surface_width: u32,
        surface_height: u32,
    ) {
        self.visible = visible;

        if visible && screen.cursor_y() < screen.rows() && screen.cursor_x() < screen.cols() {
            let cell_x = TEXT_PADDING + screen.cursor_x() as f32 * self.cell_width;
            let baseline =
                TEXT_PADDING + self.ascent.ceil() + screen.cursor_y() as f32 * self.line_height;
            let glyph_bottom = baseline - self.glyph_ymin;
            let glyph_top = glyph_bottom - self.glyph_height;
            let left = cell_x;
            let right = cell_x + self.glyph_width;

            let left = left / surface_width as f32 * 2.0 - 1.0;
            let right = right / surface_width as f32 * 2.0 - 1.0;
            let top = 1.0 - glyph_top / surface_height as f32 * 2.0;
            let bottom = 1.0 - glyph_bottom / surface_height as f32 * 2.0;

            let vertices: [CursorVertex; 6] = [
                CursorVertex {
                    position: [left, top],
                    uv: [0.0, 0.0],
                },
                CursorVertex {
                    position: [right, top],
                    uv: [1.0, 0.0],
                },
                CursorVertex {
                    position: [left, bottom],
                    uv: [0.0, 1.0],
                },
                CursorVertex {
                    position: [left, bottom],
                    uv: [0.0, 1.0],
                },
                CursorVertex {
                    position: [right, top],
                    uv: [1.0, 0.0],
                },
                CursorVertex {
                    position: [right, bottom],
                    uv: [1.0, 1.0],
                },
            ];

            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices));
            self.vertex_count = 6;
        } else {
            self.vertex_count = 0;
        }
    }

    /// Resyncs the cursor position using the stored visibility flag.
    pub(crate) fn update(
        &mut self,
        screen: &Screen,
        queue: &wgpu::Queue,
        surface_width: u32,
        surface_height: u32,
    ) {
        self.set_visible(self.visible, screen, queue, surface_width, surface_height);
    }
}

impl<'pass> Render<&mut wgpu::RenderPass<'pass>> for CursorRenderer {
    fn render(&mut self, render_pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertices.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

/// Cursor blink phase — decides visibility based on a 530ms on/off cycle
/// and drives redraw requests when the phase toggles.
pub(crate) struct CursorBlink {
    blink_start: Instant,
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

    /// Whether the cursor should be visible at this instant.
    pub(crate) fn visible(&self) -> bool {
        let millis = self.blink_start.elapsed().as_millis() as u64;
        (millis / BLINK_INTERVAL_MS).is_multiple_of(2)
    }

    /// To be called from `about_to_wait`: requests a redraw if visibility just toggled,
    /// returns the next toggle deadline for `ControlFlow::WaitUntil`.
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

    /// Must be called after the frame is rendered; records the current blink state
    /// so `check_blink` can detect the next toggle.
    pub(crate) fn commit_frame(&mut self) {
        self.last_rendered_visible = self.visible();
    }
}
