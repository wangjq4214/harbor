use std::{collections::HashMap, mem};

use anyhow::Result;
use fontdue::Font;
use wgpu::util::DeviceExt;

use crate::{font::load_system_font, render::Render, terminal::Terminal};

const FONT_SIZE: f32 = 24.0;
const TEXT_PADDING: f32 = 16.0;
const ATLAS_PADDING: u32 = 1;
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

    /// Builds one glyph quad from pixel-space bounds and atlas UV bounds.
    fn glyph_quad(
        surface_width: f32,
        surface_height: f32,
        left_px: f32,
        top_px: f32,
        right_px: f32,
        bottom_px: f32,
        uv: AtlasUv,
    ) -> [Self; 6] {
        let left = left_px / surface_width * 2.0 - 1.0;
        let right = right_px / surface_width * 2.0 - 1.0;
        let top = 1.0 - top_px / surface_height * 2.0;
        let bottom = 1.0 - bottom_px / surface_height * 2.0;

        [
            Self {
                position: [left, top],
                tex_coords: [uv.left, uv.top],
            },
            Self {
                position: [left, bottom],
                tex_coords: [uv.left, uv.bottom],
            },
            Self {
                position: [right, bottom],
                tex_coords: [uv.right, uv.bottom],
            },
            Self {
                position: [left, top],
                tex_coords: [uv.left, uv.top],
            },
            Self {
                position: [right, bottom],
                tex_coords: [uv.right, uv.bottom],
            },
            Self {
                position: [right, top],
                tex_coords: [uv.right, uv.top],
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
    /// Builds a glyph atlas texture and batched cell quads for the terminal grid.
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        font: &Font,
        terminal: &Terminal,
        surface_width: u32,
        surface_height: u32,
    ) -> Self {
        let atlas = GlyphAtlas::new(font, terminal);
        let vertices = atlas.vertices(terminal, surface_width as f32, surface_height as f32);
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("glyph atlas texture"),
                size: wgpu::Extent3d {
                    width: atlas.width,
                    height: atlas.height,
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
            &atlas.pixels,
        );
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph atlas bind group"),
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
        let vertex_count = vertices.len() as u32;
        let vertices = if vertices.is_empty() {
            vec![Vertex {
                position: [0.0, 0.0],
                tex_coords: [0.0, 0.0],
            }]
        } else {
            vertices
        };
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terminal cell vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            vertices,
            vertex_count,
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
    /// Loads the font, creates the pipeline, and prepares terminal draw data.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        terminal: &Terminal,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let font = load_system_font()?;
        let bind_group_layout = Self::create_bind_group_layout(device);
        let pipeline = Self::create_pipeline(device, format, &bind_group_layout);
        let draw = TextDraw::new(
            device,
            queue,
            &bind_group_layout,
            &font,
            terminal,
            width,
            height,
        );

        Ok(Self {
            font,
            pipeline,
            bind_group_layout,
            draw,
        })
    }

    /// Rebuilds text resources after terminal contents or surface size changes.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terminal: &Terminal,
        width: u32,
        height: u32,
    ) {
        self.draw = TextDraw::new(
            device,
            queue,
            &self.bind_group_layout,
            &self.font,
            terminal,
            width,
            height,
        );
    }
}

impl<'pass> Render<&mut wgpu::RenderPass<'pass>> for TextRenderer {
    /// Binds the glyph atlas and issues the batched cell draw call.
    fn render(&mut self, render_pass: &mut wgpu::RenderPass<'pass>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.draw.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.draw.vertices.slice(..));
        render_pass.draw(0..self.draw.vertex_count, 0..1);
    }
}

impl TextRenderer {
    /// Creates the bind layout used by the fragment shader for the atlas texture and sampler.
    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph atlas bind group layout"),
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
            label: Some("glyph atlas shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph atlas pipeline layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glyph atlas pipeline"),
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

/// Atlas placement and metrics for one rasterized glyph.
#[derive(Clone, Copy)]
struct AtlasGlyph {
    uv: AtlasUv,
    width: u32,
    height: u32,
    xmin: i32,
    ymin: i32,
}

#[derive(Clone, Copy)]
struct AtlasUv {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

/// CPU-built glyph atlas plus enough metrics to build cell quads.
struct GlyphAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    glyphs: HashMap<char, AtlasGlyph>,
    cell_width: f32,
    line_height: f32,
    ascent: f32,
}

impl GlyphAtlas {
    /// Rasterizes each distinct visible character once and packs glyphs into one atlas row.
    fn new(font: &Font, terminal: &Terminal) -> Self {
        let line_metrics = font
            .horizontal_line_metrics(FONT_SIZE)
            .expect("system font should have horizontal metrics");
        let cell_width = font.metrics('M', FONT_SIZE).advance_width.ceil().max(1.0);
        let line_height = (line_metrics.ascent - line_metrics.descent + line_metrics.line_gap)
            .ceil()
            .max(1.0);
        let mut chars = terminal
            .rows_text()
            .flat_map(|row| row.chars().collect::<Vec<_>>())
            .filter(|ch| *ch != ' ')
            .collect::<Vec<_>>();
        chars.sort_unstable();
        chars.dedup();
        let rasterized = chars
            .into_iter()
            .map(|ch| {
                let (metrics, bitmap) = font.rasterize(ch, FONT_SIZE);
                (ch, metrics, bitmap)
            })
            .collect::<Vec<_>>();

        if rasterized.is_empty() {
            return Self {
                width: 1,
                height: 1,
                pixels: vec![0],
                glyphs: HashMap::new(),
                cell_width,
                line_height,
                ascent: line_metrics.ascent,
            };
        }

        let width = rasterized
            .iter()
            .map(|(_, metrics, _)| metrics.width as u32 + ATLAS_PADDING)
            .sum::<u32>()
            .max(1);
        let height = rasterized
            .iter()
            .map(|(_, metrics, _)| metrics.height as u32)
            .max()
            .unwrap_or(1)
            .max(1);
        let mut pixels = vec![0; (width * height) as usize];
        let mut glyphs = HashMap::with_capacity(rasterized.len());
        let mut atlas_x = 0;

        for (ch, metrics, bitmap) in rasterized {
            for row in 0..metrics.height {
                let dst_start = row * width as usize + atlas_x as usize;
                let src_start = row * metrics.width;
                let src_end = src_start + metrics.width;
                pixels[dst_start..dst_start + metrics.width]
                    .copy_from_slice(&bitmap[src_start..src_end]);
            }

            let left = atlas_x as f32 / width as f32;
            let right = (atlas_x + metrics.width as u32) as f32 / width as f32;
            glyphs.insert(
                ch,
                AtlasGlyph {
                    uv: AtlasUv {
                        left,
                        top: 0.0,
                        right,
                        bottom: metrics.height as f32 / height as f32,
                    },
                    width: metrics.width as u32,
                    height: metrics.height as u32,
                    xmin: metrics.xmin,
                    ymin: metrics.ymin,
                },
            );
            atlas_x += metrics.width as u32 + ATLAS_PADDING;
        }

        Self {
            width,
            height,
            pixels,
            glyphs,
            cell_width,
            line_height,
            ascent: line_metrics.ascent,
        }
    }

    /// Converts non-empty terminal cells into one batched vertex list using atlas UVs.
    fn vertices(
        &self,
        terminal: &Terminal,
        surface_width: f32,
        surface_height: f32,
    ) -> Vec<Vertex> {
        let mut vertices = Vec::new();

        for (row, text) in terminal.rows_text().enumerate() {
            let baseline = TEXT_PADDING + self.ascent.ceil() + row as f32 * self.line_height;
            for (col, ch) in text.chars().enumerate() {
                let Some(glyph) = self.glyphs.get(&ch) else {
                    continue;
                };
                if glyph.width == 0 || glyph.height == 0 {
                    continue;
                }

                let cell_x = TEXT_PADDING + col as f32 * self.cell_width;
                let glyph_left = cell_x + glyph.xmin as f32;
                let glyph_bottom = baseline - glyph.ymin as f32;
                let glyph_top = glyph_bottom - glyph.height as f32;
                let glyph_right = glyph_left + glyph.width as f32;

                vertices.extend_from_slice(&Vertex::glyph_quad(
                    surface_width,
                    surface_height,
                    glyph_left,
                    glyph_top,
                    glyph_right,
                    glyph_bottom,
                    glyph.uv,
                ));
            }
        }

        vertices
    }
}

#[cfg(test)]
mod tests {
    use super::GlyphAtlas;
    use crate::{font::load_system_font, terminal::Terminal};

    #[test]
    fn atlas_contains_each_visible_glyph_once() {
        let font = load_system_font().expect("load monospace test font");
        let mut terminal = Terminal::new(2, 5);

        terminal.put_str("aa b\nc a");
        let atlas = GlyphAtlas::new(&font, &terminal);

        assert_eq!(atlas.glyphs.len(), 3);
        assert!(atlas.glyphs.contains_key(&'a'));
        assert!(atlas.glyphs.contains_key(&'b'));
        assert!(atlas.glyphs.contains_key(&'c'));
        assert!(!atlas.glyphs.contains_key(&' '));
        assert_eq!(atlas.pixels.len(), (atlas.width * atlas.height) as usize);
    }

    #[test]
    fn vertices_emit_one_quad_per_visible_cell() {
        let font = load_system_font().expect("load monospace test font");
        let mut terminal = Terminal::new(2, 4);

        terminal.put_str("a b\n c ");
        let atlas = GlyphAtlas::new(&font, &terminal);
        let vertices = atlas.vertices(&terminal, 800.0, 600.0);

        assert_eq!(vertices.len(), 18);
        assert!(vertices.iter().all(|vertex| {
            vertex.position[0] >= -1.0
                && vertex.position[0] <= 1.0
                && vertex.position[1] >= -1.0
                && vertex.position[1] <= 1.0
        }));
        assert!(vertices.iter().all(|vertex| {
            vertex.tex_coords[0] >= 0.0
                && vertex.tex_coords[0] <= 1.0
                && vertex.tex_coords[1] >= 0.0
                && vertex.tex_coords[1] <= 1.0
        }));
    }

    #[test]
    fn empty_grid_builds_empty_draw_batch() {
        let font = load_system_font().expect("load monospace test font");
        let terminal = Terminal::new(2, 4);

        let atlas = GlyphAtlas::new(&font, &terminal);
        let vertices = atlas.vertices(&terminal, 800.0, 600.0);

        assert!(atlas.glyphs.is_empty());
        assert_eq!((atlas.width, atlas.height), (1, 1));
        assert_eq!(atlas.pixels, vec![0]);
        assert!(vertices.is_empty());
    }
}
