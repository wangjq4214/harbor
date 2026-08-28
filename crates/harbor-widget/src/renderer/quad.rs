use crate::decoration::ClipBehavior;
use crate::renderer::Viewport;
use crate::scene::primitive::Primitive;
use crate::scene::{SceneDelta, SceneItem};
use bytemuck::Zeroable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const QUAD_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
}

struct InstanceInput {
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radii: vec4<f32>, // top-left, top-right, bottom-right, bottom-left
    @location(4) border_width: f32,
    @location(5) logical_size: vec2<f32>,
    @location(6) shape_rect: vec4<f32>, // origin x/y and size w/h within the instance
    @location(7) blur_radius: f32,
    @location(8) mode: u32, // 0 = ordinary quad/border, 1 = outer shadow
    @location(9) clip_rect0: vec4<f32>, // local origin x/y and size w/h
    @location(10) clip_radii0: vec4<f32>,
    @location(11) clip_rect1: vec4<f32>,
    @location(12) clip_radii1: vec4<f32>,
    @location(13) clip_meta: vec4<u32>, // count, behavior0, behavior1, reserved
    @location(14) occluder_rect: vec4<f32>,
    @location(15) occluder_radii: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) shape_origin: vec2<f32>,
    @location(6) shape_size: vec2<f32>,
    @location(7) blur_radius: f32,
    @interpolate(flat) @location(8) mode: u32,
    @location(9) clip_rect0: vec4<f32>,
    @location(10) clip_radii0: vec4<f32>,
    @location(11) clip_rect1: vec4<f32>,
    @location(12) clip_radii1: vec4<f32>,
    @interpolate(flat) @location(13) clip_meta: vec4<u32>,
    @location(14) occluder_rect: vec4<f32>,
    @location(15) occluder_radii: vec4<f32>,
}

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    let unit = vert.pos + vec2<f32>(0.5, 0.5);
    let x = inst.rect.x + unit.x * inst.rect.z;
    let y = inst.rect.y + unit.y * inst.rect.w;
    return VertexOutput(
        vec4<f32>(x, y, 0.0, 1.0),
        inst.color,
        unit * inst.logical_size,
        inst.logical_size,
        inst.radii,
        inst.border_width,
        inst.shape_rect.xy,
        inst.shape_rect.zw,
        inst.blur_radius,
        inst.mode,
        inst.clip_rect0,
        inst.clip_radii0,
        inst.clip_rect1,
        inst.clip_radii1,
        inst.clip_meta,
        inst.occluder_rect,
        inst.occluder_radii,
    );
}

fn rounded_box_distance(point: vec2<f32>, size: vec2<f32>, radii: vec4<f32>) -> f32 {
    // Select the radius by the point's quadrant, then use the standard
    // rounded-rectangle SDF. In particular, the max(q, 0) term keeps the
    // middle of each edge straight instead of measuring it from a corner.
    let half_size = size * 0.5;
    let centered = point - half_size;
    var radius = radii.x; // top-left
    if centered.x >= 0.0 {
        radius = select(radii.y, radii.z, centered.y >= 0.0);
    } else if centered.y >= 0.0 {
        radius = radii.w; // bottom-left
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.size.x <= 0.0 || in.size.y <= 0.0 {
        discard;
    }
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
    if in.mode == 1u {
        let distance = rounded_box_distance(
            in.local - in.shape_origin,
            in.shape_size,
            in.radii,
        );
        var coverage = 1.0;
        if in.blur_radius <= 0.0 {
            let aa = max(fwidth(distance), 0.001);
            coverage = 1.0 - smoothstep(-aa, aa, distance);
        } else {
            let blur_extent = max(in.blur_radius * 3.0, 0.001);
            coverage = 1.0 - smoothstep(0.0, blur_extent, distance);
        }
        let occluder_distance = rounded_box_distance(
            in.local - in.occluder_rect.xy,
            in.occluder_rect.zw,
            in.occluder_radii,
        );
        let occluder_aa = max(fwidth(occluder_distance), 0.001);
        let exterior = smoothstep(-occluder_aa, occluder_aa, occluder_distance);
        coverage = min(coverage, exterior);
        if coverage <= 0.0 {
            discard;
        }
        return vec4<f32>(in.color.rgb, in.color.a * coverage * clip_alpha);
    }

    let outer_distance = rounded_box_distance(in.local, in.size, in.radii);
    // Derivatives express the signed-distance transition in framebuffer
    // pixels, so the antialiasing width follows both DPI and perspective.
    let outer_aa = max(fwidth(outer_distance), 0.001);
    let outer_alpha = 1.0 - smoothstep(-outer_aa, outer_aa, outer_distance);
    if in.border_width <= 0.0 {
        return vec4<f32>(in.color.rgb, in.color.a * outer_alpha * clip_alpha);
    }

    let inset_size = max(in.size - vec2<f32>(2.0 * in.border_width), vec2<f32>(0.0));
    if inset_size.x <= 0.0 || inset_size.y <= 0.0 {
        return vec4<f32>(in.color.rgb, in.color.a * outer_alpha * clip_alpha);
    }
    let inner_radii = max(in.radii - vec4<f32>(in.border_width), vec4<f32>(0.0));
        let inner_distance = rounded_box_distance(
        in.local - vec2<f32>(in.border_width),
        inset_size,
        inner_radii,
    );
    let inner_aa = max(fwidth(inner_distance), 0.001);
    let inner_alpha = 1.0 - smoothstep(-inner_aa, inner_aa, inner_distance);
    return vec4<f32>(in.color.rgb, in.color.a * outer_alpha * (1.0 - inner_alpha) * clip_alpha);
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadInstance {
    rect: [f32; 4],
    color: [f32; 4],
    /// Corner order is top-left, top-right, bottom-right, bottom-left.
    radii: [f32; 4],
    border_width: f32,
    logical_size: [f32; 2],
    shape_rect: [f32; 4],
    blur_radius: f32,
    mode: u32,
    clip_rect0: [f32; 4],
    clip_radii0: [f32; 4],
    clip_rect1: [f32; 4],
    clip_radii1: [f32; 4],
    clip_meta: [u32; 4],
    occluder_rect: [f32; 4],
    occluder_radii: [f32; 4],
    _pad: [u32; 3],
}

fn finite_difference(lhs: f32, rhs: f32) -> f32 {
    (f64::from(lhs) - f64::from(rhs)).clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32
}

/// Instanced GPU renderer for legacy scalar and independently rounded quads.
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    device: wgpu::Device,
    instance_buffer: wgpu::Buffer,
    instance_count: u32,
    instance_capacity: u32,
    free_slots: Vec<u32>,
    id_to_slot: std::collections::BTreeMap<u64, u32>,
    slot_to_id: std::collections::BTreeMap<u32, u64>,
}

impl QuadRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let vertices: [[f32; 2]; 4] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("widget-quad-vertex"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("widget-quad-index"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-quad-instance"),
            size: instance_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("widget-quad-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUAD_SHADER)),
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
            array_stride: std::mem::size_of::<QuadInstance>() as u64,
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
                    format: wgpu::VertexFormat::Float32,
                    offset: 48,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 52,
                    shader_location: 5,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 60,
                    shader_location: 6,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 76,
                    shader_location: 7,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 80,
                    shader_location: 8,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 84,
                    shader_location: 9,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 100,
                    shader_location: 10,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 116,
                    shader_location: 11,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 132,
                    shader_location: 12,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x4,
                    offset: 148,
                    shader_location: 13,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 164,
                    shader_location: 14,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 180,
                    shader_location: 15,
                },
            ],
        };
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("widget-quad-layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("widget-quad-pipeline"),
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            device: device.clone(),
            instance_buffer,
            instance_count: 0,
            instance_capacity,
            free_slots: Vec::new(),
            id_to_slot: std::collections::BTreeMap::new(),
            slot_to_id: std::collections::BTreeMap::new(),
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, delta: &SceneDelta, viewport: &Viewport) {
        for id in &delta.removed {
            if let Some(slot) = self.id_to_slot.remove(id) {
                self.slot_to_id.remove(&slot);
                self.free_slots.push(slot);
                self.write_instance(queue, slot, QuadInstance::zeroed());
            }
        }
        for item in &delta.added {
            if !Self::is_quad_primitive(&item.primitive) {
                continue;
            }
            let Some(slot) = self.allocate_slot(queue) else {
                continue;
            };
            self.id_to_slot.insert(item.id, slot);
            self.slot_to_id.insert(slot, item.id);
            self.write_instance(queue, slot, self.item_to_instance(item, viewport));
        }
        for item in &delta.modified {
            if Self::is_quad_primitive(&item.primitive) {
                if let Some(slot) = self.id_to_slot.get(&item.id).copied() {
                    self.write_instance(queue, slot, self.item_to_instance(item, viewport));
                } else if let Some(slot) = self.allocate_slot(queue) {
                    self.id_to_slot.insert(item.id, slot);
                    self.slot_to_id.insert(slot, item.id);
                    self.write_instance(queue, slot, self.item_to_instance(item, viewport));
                }
            } else if let Some(slot) = self.id_to_slot.remove(&item.id) {
                self.slot_to_id.remove(&slot);
                self.free_slots.push(slot);
                self.write_instance(queue, slot, QuadInstance::zeroed());
            }
        }
    }

    /// Re-uploads retained instances when viewport conversion changes without
    /// changing renderer-neutral scene primitives.
    pub fn refresh_viewport(&self, queue: &wgpu::Queue, items: &[SceneItem], viewport: &Viewport) {
        for item in items {
            if let Some(slot) = self.id_to_slot.get(&item.id).copied()
                && Self::is_quad_primitive(&item.primitive)
            {
                self.write_instance(queue, slot, self.item_to_instance(item, viewport));
            }
        }
    }

    pub fn encode<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let count = self.instance_count.min(self.instance_capacity);
        if count != 0 {
            self.encode_impl(pass, 0, count);
        }
    }

    pub fn encode_range<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, start: u32, count: u32) {
        let capacity = self.instance_count.min(self.instance_capacity);
        if count != 0 && start < capacity {
            self.encode_impl(pass, start, count.min(capacity - start));
        }
    }

    fn encode_impl<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, start: u32, count: u32) {
        let end = start.saturating_add(count).min(self.instance_capacity);
        if start >= end {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, start..end);
    }

    fn is_quad_primitive(primitive: &Primitive) -> bool {
        matches!(
            primitive,
            Primitive::Quad { .. }
                | Primitive::Border { .. }
                | Primitive::RoundedQuad { .. }
                | Primitive::RoundedBorder { .. }
                | Primitive::OuterShadow { .. }
        )
    }

    fn item_to_instance(&self, item: &SceneItem, viewport: &Viewport) -> QuadInstance {
        let (
            rect,
            shape_rect,
            occluder_rect,
            color,
            radii,
            occluder_radii,
            border_width,
            blur_radius,
            mode,
        ) = match &item.primitive {
            Primitive::Quad {
                rect,
                color,
                corner_radius,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                [*corner_radius; 4],
                [0.0; 4],
                0.0,
                0.0,
                0,
            ),
            Primitive::Border {
                rect,
                width,
                color,
                corner_radius,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                [*corner_radius; 4],
                [0.0; 4],
                *width,
                0.0,
                0,
            ),
            Primitive::RoundedQuad {
                rect,
                color,
                corner_radii,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                *corner_radii,
                [0.0; 4],
                0.0,
                0.0,
                0,
            ),
            Primitive::RoundedBorder {
                rect,
                width,
                color,
                corner_radii,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                *corner_radii,
                [0.0; 4],
                *width,
                0.0,
                0,
            ),
            Primitive::OuterShadow {
                rect,
                shape_rect,
                occluder_rect,
                color,
                corner_radii,
                occluder_radii,
                blur_radius,
            } => (
                *rect,
                *shape_rect,
                *occluder_rect,
                *color,
                *corner_radii,
                *occluder_radii,
                0.0,
                *blur_radius,
                1,
            ),
            _ => return QuadInstance::zeroed(),
        };
        let size = rect.size();
        let shape_size = shape_rect.size();
        let active_clips = item
            .clips
            .iter()
            .filter(|clip| clip.behavior() != ClipBehavior::None)
            .collect::<Vec<_>>();
        let mut clip_rects = [[0.0; 4]; 2];
        let mut clip_radii = [[0.0; 4]; 2];
        let mut clip_behaviors = [0u32; 2];
        let exact_clip = |ancestor: &crate::scene::clip::RoundedClip| {
            let bounds = ancestor.rect();
            (
                [
                    finite_difference(bounds.min.x, rect.min.x),
                    finite_difference(bounds.min.y, rect.min.y),
                    finite_difference(bounds.max.x, bounds.min.x),
                    finite_difference(bounds.max.y, bounds.min.y),
                ],
                ancestor.radii().as_array(),
                match ancestor.behavior() {
                    ClipBehavior::None => 0,
                    ClipBehavior::HardEdge => 1,
                    ClipBehavior::AntiAlias => 2,
                },
            )
        };
        if let Some(first) = active_clips.first() {
            (clip_rects[0], clip_radii[0], clip_behaviors[0]) = exact_clip(first);
        }
        if active_clips.len() == 2 {
            (clip_rects[1], clip_radii[1], clip_behaviors[1]) = exact_clip(active_clips[1]);
        } else if active_clips.len() > 2 {
            // Two rounded clips fit in the portable vertex-attribute budget.
            // Conservatively precompose every remaining clip to an inscribed
            // hard rectangle, so deep nesting may clip extra corner pixels but
            // can never leak outside an authoritative ancestor.
            let mut min_x = -f32::MAX;
            let mut min_y = -f32::MAX;
            let mut max_x = f32::MAX;
            let mut max_y = f32::MAX;
            for ancestor in &active_clips[1..] {
                let bounds = ancestor.rect();
                let inset = ancestor.radii().as_array().into_iter().fold(0.0, f32::max);
                let inset_min_x = (f64::from(bounds.min.x) + f64::from(inset))
                    .clamp(-f64::from(f32::MAX), f64::from(f32::MAX))
                    as f32;
                let inset_min_y = (f64::from(bounds.min.y) + f64::from(inset))
                    .clamp(-f64::from(f32::MAX), f64::from(f32::MAX))
                    as f32;
                let inset_max_x = (f64::from(bounds.max.x) - f64::from(inset))
                    .clamp(-f64::from(f32::MAX), f64::from(f32::MAX))
                    as f32;
                let inset_max_y = (f64::from(bounds.max.y) - f64::from(inset))
                    .clamp(-f64::from(f32::MAX), f64::from(f32::MAX))
                    as f32;
                min_x = min_x.max(inset_min_x);
                min_y = min_y.max(inset_min_y);
                max_x = max_x.min(inset_max_x);
                max_y = max_y.min(inset_max_y);
            }
            clip_rects[1] = [
                finite_difference(min_x, rect.min.x),
                finite_difference(min_y, rect.min.y),
                finite_difference(max_x.max(min_x), min_x),
                finite_difference(max_y.max(min_y), min_y),
            ];
            clip_behaviors[1] = if max_x <= min_x || max_y <= min_y {
                3
            } else {
                1
            };
        }
        let clip_count = active_clips.len().min(2);
        QuadInstance {
            rect: viewport.dp_rect_to_ndc(&rect),
            color: color.to_array(),
            radii,
            border_width,
            logical_size: [size.width, size.height],
            shape_rect: [
                finite_difference(shape_rect.min.x, rect.min.x),
                finite_difference(shape_rect.min.y, rect.min.y),
                shape_size.width,
                shape_size.height,
            ],
            blur_radius,
            mode,
            clip_rect0: clip_rects[0],
            clip_radii0: clip_radii[0],
            clip_rect1: clip_rects[1],
            clip_radii1: clip_radii[1],
            clip_meta: [clip_count as u32, clip_behaviors[0], clip_behaviors[1], 0],
            occluder_rect: [
                finite_difference(occluder_rect.min.x, rect.min.x),
                finite_difference(occluder_rect.min.y, rect.min.y),
                finite_difference(occluder_rect.max.x, occluder_rect.min.x),
                finite_difference(occluder_rect.max.y, occluder_rect.min.y),
            ],
            occluder_radii,
            _pad: [0; 3],
        }
    }

    fn write_instance(&self, queue: &wgpu::Queue, slot: u32, instance: QuadInstance) {
        queue.write_buffer(
            &self.instance_buffer,
            slot as u64 * std::mem::size_of::<QuadInstance>() as u64,
            bytemuck::bytes_of(&instance),
        );
    }

    pub fn slot_of(&self, id: u64) -> Option<u32> {
        self.id_to_slot.get(&id).copied()
    }

    fn allocate_slot(&mut self, queue: &wgpu::Queue) -> Option<u32> {
        if let Some(slot) = self.free_slots.pop() {
            return Some(slot);
        }
        if self.instance_count == self.instance_capacity {
            self.grow_instance_buffer(queue, self.instance_count.checked_add(1)?);
        }
        let slot = self.instance_count;
        self.instance_count += 1;
        Some(slot)
    }

    fn grow_instance_buffer(&mut self, queue: &wgpu::Queue, required_capacity: u32) {
        debug_assert!(required_capacity > self.instance_capacity);
        let new_capacity = self
            .instance_capacity
            .saturating_mul(2)
            .max(required_capacity);
        assert!(new_capacity > self.instance_capacity);

        let new_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-quad-instance-grown"),
            size: new_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("widget-quad-instance-grow"),
            });
        encoder.copy_buffer_to_buffer(
            &self.instance_buffer,
            0,
            &new_buffer,
            0,
            self.instance_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
        );
        queue.submit(Some(encoder.finish()));
        self.instance_buffer = new_buffer;
        self.instance_capacity = new_capacity;
    }
}
