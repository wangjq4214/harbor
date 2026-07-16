use anyhow::{Result, anyhow};
use bytemuck::{Pod, Zeroable};
use fontdb::{Database, Family, Query};
use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

use crate::{PaintCommand, RenderEnvironment};

const ATLAS_SIZE: u32 = 2048;
const ATLAS_PADDING: u32 = 1;

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}
@group(0) @binding(0) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(1) var glyph_sampler: sampler;
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(glyph_atlas, glyph_sampler, in.tex_coords).r;
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
            ],
        }
    }

    fn quad(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        uv: [f32; 4],
        color: [f32; 4],
        size: (f32, f32),
    ) -> [Self; 6] {
        let x0 = left / size.0 * 2.0 - 1.0;
        let x1 = right / size.0 * 2.0 - 1.0;
        let y0 = 1.0 - top / size.1 * 2.0;
        let y1 = 1.0 - bottom / size.1 * 2.0;
        let [u0, v0, u1, v1] = uv;
        [
            Self {
                position: [x0, y0],
                uv: [u0, v0],
                color,
            },
            Self {
                position: [x0, y1],
                uv: [u0, v1],
                color,
            },
            Self {
                position: [x1, y1],
                uv: [u1, v1],
                color,
            },
            Self {
                position: [x0, y0],
                uv: [u0, v0],
                color,
            },
            Self {
                position: [x1, y1],
                uv: [u1, v1],
                color,
            },
            Self {
                position: [x1, y0],
                uv: [u1, v0],
                color,
            },
        ]
    }
}

struct Glyph {
    width: u32,
    height: u32,
    xmin: i32,
    ymin: i32,
    uv: [f32; 4],
}

#[derive(Clone, Copy)]
struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

/// Renderer-owned font fallback set, glyph atlas, and text GPU resources.
pub(crate) struct TextRenderer {
    fonts: Vec<Font>,
    cell_width: f32,
    line_height: f32,
    ascent: f32,
    glyphs: HashMap<char, Glyph>,
    shelves: Vec<Shelf>,
    atlas: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    vertices: Option<wgpu::Buffer>,
    vertex_capacity: usize,
}

impl TextRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self> {
        let fonts = load_fonts()?;
        let metrics = fonts[0].metrics('M', harbor_config::FONT_SIZE);
        let (cell_width, line_height, ascent) = fonts[0]
            .horizontal_line_metrics(harbor_config::FONT_SIZE)
            .map(|line| {
                (
                    metrics.advance_width.ceil(),
                    line.new_line_size.ceil(),
                    line.ascent.ceil(),
                )
            })
            .unwrap_or((
                metrics.advance_width.ceil(),
                metrics.bounds.height as f32 + 4.0,
                metrics.bounds.height as f32,
            ));
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("harbor renderer glyph atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("harbor renderer glyph layout"),
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
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("harbor renderer glyph bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("harbor renderer text shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("harbor renderer text pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("harbor renderer text pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(Vertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            fonts,
            cell_width,
            line_height,
            ascent,
            glyphs: HashMap::new(),
            shelves: Vec::new(),
            atlas,
            bind_group,
            pipeline,
            vertices: None,
            vertex_capacity: 0,
        })
    }

    pub(crate) fn metrics(&self) -> (f32, f32, f32) {
        (self.cell_width, self.line_height, self.ascent)
    }

    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        command: &PaintCommand<'_>,
        environment: RenderEnvironment,
    ) {
        let PaintCommand::Text {
            origin,
            text,
            color,
            font_size,
            line_height,
            ..
        } = command
        else {
            return;
        };
        let scale = *font_size / harbor_config::FONT_SIZE;
        let mut vertices = Vec::with_capacity(text.chars().count() * 6);
        let cell_width = self.cell_width;
        let ascent = self.ascent;
        let baseline = origin.1 + ascent * scale;
        for (column, character) in text.chars().enumerate() {
            let Some(glyph) = self.glyph(queue, character) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let left = origin.0 + column as f32 * cell_width * scale + glyph.xmin as f32 * scale;
            let bottom = baseline - glyph.ymin as f32 * scale;
            vertices.extend_from_slice(&Vertex::quad(
                left,
                bottom - glyph.height as f32 * scale,
                left + glyph.width as f32 * scale,
                bottom,
                glyph.uv,
                color.0,
                environment.logical_size(),
            ));
        }
        if vertices.is_empty() {
            return;
        }
        if vertices.len() > self.vertex_capacity {
            self.vertices = Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("harbor renderer text vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }),
            );
            self.vertex_capacity = vertices.len();
        } else if let Some(buffer) = &self.vertices {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
        }
        let Some(buffer) = &self.vertices else {
            return;
        };
        let _ = line_height;
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    fn glyph(&mut self, queue: &wgpu::Queue, character: char) -> Option<&Glyph> {
        if !self.glyphs.contains_key(&character) {
            let (metrics, pixels) = self
                .fonts
                .iter()
                .find(|font| font.has_glyph(character))
                .unwrap_or(&self.fonts[0])
                .rasterize(character, harbor_config::FONT_SIZE);
            let width = metrics.width as u32;
            let height = metrics.height as u32;
            let (x, y) = self.allocate(width, height)?;
            if width > 0 && height > 0 {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.atlas,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            self.glyphs.insert(
                character,
                Glyph {
                    width,
                    height,
                    xmin: metrics.xmin,
                    ymin: metrics.ymin,
                    uv: [
                        x as f32 / ATLAS_SIZE as f32,
                        y as f32 / ATLAS_SIZE as f32,
                        (x + width) as f32 / ATLAS_SIZE as f32,
                        (y + height) as f32 / ATLAS_SIZE as f32,
                    ],
                },
            );
        }
        self.glyphs.get(&character)
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        for shelf in &mut self.shelves {
            if shelf.height >= height && shelf.next_x + width <= ATLAS_SIZE {
                let point = (shelf.next_x, shelf.y);
                shelf.next_x += width + ATLAS_PADDING;
                return Some(point);
            }
        }
        let y = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.y + shelf.height);
        (y + height <= ATLAS_SIZE).then(|| {
            self.shelves.push(Shelf {
                y,
                height,
                next_x: width + ATLAS_PADDING,
            });
            (0, y)
        })
    }
}

fn load_fonts() -> Result<Vec<Font>> {
    let mut database = Database::new();
    database.load_system_fonts();
    let query = Query {
        families: &[Family::Monospace],
        ..Query::default()
    };
    let mut ids = database.query(&query).into_iter().collect::<Vec<_>>();
    ids.extend(database.faces().map(|face| face.id));
    let mut fonts = Vec::new();
    for id in ids {
        if fonts.len() == 8 {
            break;
        }
        let font = database.with_face_data(id, |data, collection_index| {
            Font::from_bytes(
                data,
                FontSettings {
                    collection_index,
                    ..FontSettings::default()
                },
            )
        });
        if let Some(Ok(font)) = font {
            fonts.push(font);
        }
    }
    (!fonts.is_empty())
        .then_some(fonts)
        .ok_or_else(|| anyhow!("no parseable system font found"))
}
