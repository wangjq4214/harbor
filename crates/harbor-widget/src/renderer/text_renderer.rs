use crate::layout::{Point, Rect, Size};
use crate::renderer::{Viewport, pack_active_clips};
use crate::scene::SceneGraph;
use crate::scene::clip::RoundedClip;
use crate::scene::primitive::Color;
use crate::text::{GlyphLayout, TextRunCache};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

// ── WGSL Shader ─────────────────────────────────────────────────────────────

const TEXT_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
}

struct InstanceInput {
    @location(1) rect: vec4<f32>,
    @location(2) uv_rect: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) clip_rect0: vec4<f32>,
    @location(5) clip_radii0: vec4<f32>,
    @location(6) clip_rect1: vec4<f32>,
    @location(7) clip_radii1: vec4<f32>,
    @location(8) clip_meta: vec4<u32>,
    @location(9) logical_size: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) clip_rect0: vec4<f32>,
    @location(4) clip_radii0: vec4<f32>,
    @location(5) clip_rect1: vec4<f32>,
    @location(6) clip_radii1: vec4<f32>,
    @interpolate(flat) @location(7) clip_meta: vec4<u32>,
}

@group(0) @binding(0) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(1) var glyph_sampler: sampler;

fn rounded_box_distance(point: vec2<f32>, size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let centered = point - half_size;
    var radius = radii.x;
    if centered.x >= 0.0 {
        radius = select(radii.y, radii.z, centered.y >= 0.0);
    } else if centered.y >= 0.0 {
        radius = radii.w;
    }
    let q = abs(centered) - half_size + vec2<f32>(radius, radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn rounded_clip_coverage(
    point: vec2<f32>,
    rect: vec4<f32>,
    radii: vec4<f32>,
    behavior: u32,
) -> f32 {
    if behavior == 0u {
        return 1.0;
    }
    if behavior == 3u {
        return 0.0;
    }
    let distance = rounded_box_distance(point - rect.xy, rect.zw, radii);
    if behavior == 1u {
        return select(1.0, 0.0, distance > 0.0);
    }
    let aa = max(fwidth(distance), 0.001);
    return 1.0 - smoothstep(-aa, aa, distance);
}

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    let unit = vert.pos + vec2<f32>(0.5, 0.5);
    let x = inst.rect.x + unit.x * inst.rect.z;
    let y = inst.rect.y + unit.y * inst.rect.w;
    let tx = vert.pos.x + 0.5;
    let ty = vert.pos.y + 0.5;
    let u = inst.uv_rect.x + tx * (inst.uv_rect.z - inst.uv_rect.x);
    let v = inst.uv_rect.y + ty * (inst.uv_rect.w - inst.uv_rect.y);

    return VertexOutput(
        vec4<f32>(x, y, 0.0, 1.0),
        vec2<f32>(u, v),
        inst.color,
        unit * inst.logical_size,
        inst.clip_rect0,
        inst.clip_radii0,
        inst.clip_rect1,
        inst.clip_radii1,
        inst.clip_meta,
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var clip_alpha = 1.0;
    if in.clip_meta.x > 0u {
        clip_alpha *= rounded_clip_coverage(in.local, in.clip_rect0, in.clip_radii0, in.clip_meta.y);
    }
    if in.clip_meta.x > 1u {
        clip_alpha *= rounded_clip_coverage(in.local, in.clip_rect1, in.clip_radii1, in.clip_meta.z);
    }
    if clip_alpha <= 0.0 {
        discard;
    }
    let sampled = textureSample(glyph_atlas, glyph_sampler, in.uv);
    return vec4<f32>(in.color.rgb, in.color.a * sampled.r * clip_alpha);
}
"#;

// ── Instance Data (GPU layout) ──────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    clip_rect0: [f32; 4],
    clip_radii0: [f32; 4],
    clip_rect1: [f32; 4],
    clip_radii1: [f32; 4],
    clip_meta: [u32; 4],
    logical_size: [f32; 2],
    _pad: [f32; 2],
}

#[derive(Copy, Clone, Debug)]
struct TextInstanceBatch<'a> {
    glyphs: &'a [GlyphLayout],
    origin: Point,
    color: Color,
    clips: &'a [RoundedClip],
}

// ── TextRenderer ────────────────────────────────────────────────────────────

/// GPU renderer for textured glyph quads.
///
/// Owns a textured-quad pipeline (sampling a glyph atlas), unit-quad
/// vertex/index buffers, and a dynamic glyph instance buffer.
pub struct TextRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u32,
    /// Maps SceneItem id → instance buffer offset (in GlyphInstance units).
    id_to_offset: std::collections::BTreeMap<u64, (u32, u32)>,
}

impl TextRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
        bind_group: &wgpu::BindGroup,
    ) -> Self {
        let vertices: [[f32; 2]; 4] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("widget-text-vertex"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("widget-text-index"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_capacity = 1024u32;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-text-instance"),
            size: (instance_capacity as u64) * std::mem::size_of::<GlyphInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("widget-text-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXT_SHADER)),
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: 2 * std::mem::size_of::<f32>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        let instance_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 64,
                    shader_location: 5,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 80,
                    shader_location: 6,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 96,
                    shader_location: 7,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x4,
                    offset: 112,
                    shader_location: 8,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 128,
                    shader_location: 9,
                },
            ],
        };

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("widget-text-layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("widget-text-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_buffer_layout), Some(instance_buffer_layout)],
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
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        TextRenderer {
            pipeline,
            bind_group: bind_group.clone(),
            vertex_buffer,
            index_buffer,
            instance_buffer,
            instance_capacity,
            id_to_offset: std::collections::BTreeMap::new(),
        }
    }

    /// Updates the texture bind group to match the active glyph atlas.
    pub fn set_bind_group(&mut self, bind_group: &wgpu::BindGroup) {
        self.bind_group = bind_group.clone();
    }

    /// Rebuilds text instance offsets from the retained scene.
    ///
    /// Repacking every changed frame makes modified runs safe even when their
    /// glyph counts change, and drops offsets for removed scene items.
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        scene_graph: &SceneGraph,
        run_cache: &TextRunCache,
        viewport: &Viewport,
    ) {
        self.id_to_offset.clear();
        let mut next_offset = 0;

        for item in scene_graph.items() {
            if let crate::scene::primitive::Primitive::Text { origin, color, .. } = &item.primitive
                && let Some(run_data) = run_cache.get(item.id)
            {
                let count = run_data.glyphs.len() as u32;
                if count == 0 {
                    continue;
                }

                self.write_instances(
                    queue,
                    next_offset,
                    TextInstanceBatch {
                        glyphs: run_data.glyphs.as_slice(),
                        origin: *origin,
                        color: *color,
                        clips: &item.clips,
                    },
                    viewport,
                );
                self.id_to_offset.insert(item.id, (next_offset, count));
                next_offset += count;
            }
        }
    }

    /// Encodes a text run draw call into the RenderPass.
    ///
    /// Looks up the SceneItem by id and draws its glyph instances.
    pub fn encode_item<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, item_id: u64) {
        if let Some(&(start, count)) = self.id_to_offset.get(&item_id) {
            if count == 0 {
                return;
            }
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..6, 0, start..(start + count));
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn write_instances(
        &self,
        queue: &wgpu::Queue,
        start: u32,
        batch: TextInstanceBatch<'_>,
        viewport: &Viewport,
    ) {
        let instances: Vec<GlyphInstance> = batch
            .glyphs
            .iter()
            .map(|g| {
                let glyph_origin =
                    Point::new(batch.origin.x + g.origin.x, batch.origin.y + g.origin.y);
                let dp_rect = Rect::from_min_size(glyph_origin, Size::new(g.width, g.height));
                let ndc = viewport.dp_rect_to_ndc(&dp_rect);
                let packed = pack_active_clips(batch.clips, glyph_origin);
                GlyphInstance {
                    rect: ndc,
                    uv: [g.uv_left, g.uv_top, g.uv_right, g.uv_bottom],
                    color: batch.color.to_array(),
                    clip_rect0: packed.clip_rect0,
                    clip_radii0: packed.clip_radii0,
                    clip_rect1: packed.clip_rect1,
                    clip_radii1: packed.clip_radii1,
                    clip_meta: packed.clip_meta,
                    logical_size: [g.width, g.height],
                    _pad: [0.0; 2],
                }
            })
            .collect();

        let offset = start as u64 * std::mem::size_of::<GlyphInstance>() as u64;
        let size = instances.len() as u64 * std::mem::size_of::<GlyphInstance>() as u64;
        let capacity = self.instance_capacity as u64 * std::mem::size_of::<GlyphInstance>() as u64;
        if offset + size <= capacity {
            queue.write_buffer(
                &self.instance_buffer,
                offset,
                bytemuck::cast_slice(&instances),
            );
        }
        // Capacity exceeded: instances silently dropped.
        // 1024 glyph instances suffice for the current static widget tree.
    }
}
