use std::mem;

use anyhow::Result;
use fontdue::Font;
use wgpu::util::DeviceExt;

use crate::font::load_system_font;

const TEXT: &str = "Hello Terminal";
const FONT_SIZE: f32 = 64.0;
const TEXT_PADDING: u32 = 4;

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@group(0) @binding(0) var text_texture: texture_2d<f32>;
@group(0) @binding(1) var text_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.tex_coords = input.tex_coords;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(text_texture, text_sampler, input.tex_coords).r;
    return vec4<f32>(0.95, 0.90, 0.80, alpha);
}
"#;

/// A GPU vertex containing clip-space position and texture coordinates.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    /// Returns the wgpu vertex buffer layout matching `Vertex` memory layout.
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// Builds a centered rectangle made from two triangles.
    fn quad(
        surface_width: f32,
        surface_height: f32,
        text_width: f32,
        text_height: f32,
    ) -> [Self; 6] {
        // Center in pixel space first, then convert to wgpu's [-1, 1] clip space.
        let x = ((surface_width - text_width) * 0.5).max(0.0);
        let y = ((surface_height - text_height) * 0.5).max(0.0);
        let left = x / surface_width * 2.0 - 1.0;
        let right = (x + text_width) / surface_width * 2.0 - 1.0;
        let top = 1.0 - y / surface_height * 2.0;
        let bottom = 1.0 - (y + text_height) / surface_height * 2.0;

        [
            Self {
                position: [left, top],
                tex_coords: [0.0, 0.0],
            },
            Self {
                position: [left, bottom],
                tex_coords: [0.0, 1.0],
            },
            Self {
                position: [right, bottom],
                tex_coords: [1.0, 1.0],
            },
            Self {
                position: [left, top],
                tex_coords: [0.0, 0.0],
            },
            Self {
                position: [right, bottom],
                tex_coords: [1.0, 1.0],
            },
            Self {
                position: [right, top],
                tex_coords: [1.0, 0.0],
            },
        ]
    }
}

/// GPU resources needed for one text draw call.
struct TextDraw {
    vertices: wgpu::Buffer,
    vertex_count: u32,
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl TextDraw {
    /// Rasterizes text and creates the matching GPU texture and vertex buffer.
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        font: &Font,
        surface_width: u32,
        surface_height: u32,
    ) -> Self {
        let rasterized = RasterizedText::new(font);
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("text texture"),
                size: wgpu::Extent3d {
                    width: rasterized.width,
                    height: rasterized.height,
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
            &rasterized.pixels,
        );
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("text sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text bind group"),
            layout: bind_group_layout,
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
        let vertices = Vertex::quad(
            surface_width as f32,
            surface_height as f32,
            rasterized.width as f32,
            rasterized.height as f32,
        );
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            vertices,
            vertex_count: 6,
            _texture: texture,
            bind_group,
        }
    }
}

/// Holds the text render pipeline and size-dependent draw resources.
pub(crate) struct TextRenderer {
    font: Font,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    draw: TextDraw,
}

impl TextRenderer {
    /// Loads the font, creates the pipeline, and prepares text draw data for the current size.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let font = load_system_font()?;
        let bind_group_layout = Self::create_bind_group_layout(device);
        let pipeline = Self::create_pipeline(device, format, &bind_group_layout);
        let draw = TextDraw::new(device, queue, &bind_group_layout, &font, width, height);

        Ok(Self {
            font,
            pipeline,
            bind_group_layout,
            draw,
        })
    }

    /// Rebuilds text vertices after resize so the text stays centered.
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) {
        self.draw = TextDraw::new(
            device,
            queue,
            &self.bind_group_layout,
            &self.font,
            width,
            height,
        );
    }

    /// Binds text rendering resources and issues the draw command.
    pub(crate) fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.draw.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.draw.vertices.slice(..));
        render_pass.draw(0..self.draw.vertex_count, 0..1);
    }

    /// Creates the bind layout used by the fragment shader for the text texture and sampler.
    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text bind group layout"),
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

    /// Creates the text render pipeline from the embedded WGSL shader.
    fn create_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::layout()],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
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
            multiview_mask: None,
            cache: None,
        })
    }
}

/// CPU-rasterized text bitmap uploaded as a single-channel alpha texture.
struct RasterizedText {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RasterizedText {
    /// Rasterizes the fixed text into a compact alpha bitmap, glyph by glyph.
    fn new(font: &Font) -> Self {
        let metrics = font
            .horizontal_line_metrics(FONT_SIZE)
            .expect("system font should have horizontal metrics");
        // Compute the baseline from font ascent so all glyphs share the same baseline.
        let baseline = TEXT_PADDING as i32 + metrics.ascent.ceil() as i32;
        let text_width = TEXT
            .chars()
            .map(|ch| font.metrics(ch, FONT_SIZE).advance_width)
            .sum::<f32>()
            .ceil() as u32;
        let text_height = (metrics.ascent - metrics.descent).ceil() as u32;
        let width = (text_width + TEXT_PADDING * 2).max(1);
        let height = (text_height + TEXT_PADDING * 2).max(1);
        let mut pixels = vec![0; (width * height) as usize];
        let mut pen_x = TEXT_PADDING as f32;

        // Composite glyphs into the target bitmap; keep the larger alpha for overlapping pixels.
        for ch in TEXT.chars() {
            let (glyph_metrics, bitmap) = font.rasterize(ch, FONT_SIZE);
            let glyph_x = pen_x.round() as i32 + glyph_metrics.xmin;
            let glyph_y = baseline - glyph_metrics.ymin - glyph_metrics.height as i32;

            for row in 0..glyph_metrics.height {
                for col in 0..glyph_metrics.width {
                    let dst_x = glyph_x + col as i32;
                    let dst_y = glyph_y + row as i32;
                    if dst_x < 0 || dst_y < 0 || dst_x >= width as i32 || dst_y >= height as i32 {
                        continue;
                    }
                    let src = bitmap[row * glyph_metrics.width + col];
                    let dst = &mut pixels[dst_y as usize * width as usize + dst_x as usize];
                    *dst = (*dst).max(src);
                }
            }

            pen_x += glyph_metrics.advance_width;
        }

        Self {
            width,
            height,
            pixels,
        }
    }
}
