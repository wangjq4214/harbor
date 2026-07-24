use crate::renderer::Viewport;
use crate::scene::primitive::Primitive;
use crate::scene::{SceneDelta, SceneItem};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

// ── WGSL Shader ─────────────────────────────────────────────────────────────

const QUAD_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
}

struct InstanceInput {
    @location(1) rect: vec4<f32>,    // x, y, w, h in NDC
    @location(2) color: vec4<f32>,   // rgba
    @location(3) radius: f32,        // corner radius (unused until Phase 2)
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    // Transform unit quad to rect
    let x = inst.rect.x + (vert.pos.x + 0.5) * inst.rect.z;
    let y = inst.rect.y + (vert.pos.y + 0.5) * inst.rect.w;
    return VertexOutput(vec4<f32>(x, y, 0.0, 1.0), inst.color);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

// ── Instance Data (GPU layout) ──────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadInstance {
    rect: [f32; 4],  // x, y, w, h in NDC
    color: [f32; 4], // rgba
    radius: f32,     // corner radius
    _pad: [f32; 3],  // padding for alignment
}

// ── QuadRenderer ────────────────────────────────────────────────────────────

/// Instanced quad GPU renderer.
///
/// Owns the pipeline, vertex/index buffers (one unit quad), and a dynamic
/// instance buffer. Applies SceneDeltas to update the instance buffer and
/// encodes instanced draw calls into a RenderPass.
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_count: u32,
    instance_capacity: u32,
    /// Free slots reclaimed from removed items, popped before allocating new ones.
    free_slots: Vec<u32>,
    /// Maps SceneItem id -> instance buffer slot for lookups on modify/remove.
    id_to_slot: std::collections::BTreeMap<u64, u32>,
    /// Reverse map: slot -> id
    slot_to_id: std::collections::BTreeMap<u32, u64>,
}

impl QuadRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        // Vertex data: unit quad centered at origin
        let vertices: [[f32; 2]; 4] = [
            [-0.5, -0.5], // bottom-left
            [0.5, -0.5],  // bottom-right
            [0.5, 0.5],   // top-right
            [-0.5, 0.5],  // top-left
        ];
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

        // Start with capacity for 256 instances
        let instance_capacity = 256u32;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-quad-instance"),
            size: (instance_capacity as u64) * std::mem::size_of::<QuadInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
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
                    offset: 4 * std::mem::size_of::<f32>() as u64,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 8 * std::mem::size_of::<f32>() as u64,
                    shader_location: 3,
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

        QuadRenderer {
            pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            instance_count: 0,
            instance_capacity,
            free_slots: Vec::new(),
            id_to_slot: std::collections::BTreeMap::new(),
            slot_to_id: std::collections::BTreeMap::new(),
        }
    }

    /// Applies a SceneDelta to the instance buffer, translating dp Rects to
    /// NDC via the Viewport.
    pub fn update(&mut self, queue: &wgpu::Queue, delta: &SceneDelta, viewport: &Viewport) {
        // For removed items, clear the slot for reuse
        for id in &delta.removed {
            if let Some(slot) = self.id_to_slot.remove(id) {
                self.slot_to_id.remove(&slot);
                // Reclaim the slot for future allocations
                self.free_slots.push(slot);
                // Write zeroed instance data to clear the slot
                let instance = QuadInstance {
                    rect: [0.0, 0.0, 0.0, 0.0],
                    color: [0.0, 0.0, 0.0, 0.0],
                    radius: 0.0,
                    _pad: [0.0, 0.0, 0.0],
                };
                let offset = slot as u64 * std::mem::size_of::<QuadInstance>() as u64;
                queue.write_buffer(&self.instance_buffer, offset, bytemuck::bytes_of(&instance));
            }
        }

        // For added items, allocate new slots
        for item in &delta.added {
            let instance = self.item_to_instance(item, viewport);
            let slot = self.allocate_slot();
            if slot >= self.instance_capacity {
                // Buffer full — item skipped. With free-list recycling, this
                // only occurs when >256 quads are alive simultaneously.
                // TODO: grow buffer by recreating with double capacity.
                continue;
            }
            self.id_to_slot.insert(item.id, slot);
            self.slot_to_id.insert(slot, item.id);
            let offset = slot as u64 * std::mem::size_of::<QuadInstance>() as u64;
            queue.write_buffer(&self.instance_buffer, offset, bytemuck::bytes_of(&instance));
        }

        // For modified items, update in place
        for item in &delta.modified {
            if let Some(slot) = self.id_to_slot.get(&item.id) {
                let instance = self.item_to_instance(item, viewport);
                let offset = *slot as u64 * std::mem::size_of::<QuadInstance>() as u64;
                queue.write_buffer(&self.instance_buffer, offset, bytemuck::bytes_of(&instance));
            }
        }
    }

    /// Encodes instanced draw calls into the RenderPass.
    pub fn encode<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.instance_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..self.instance_count);
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn item_to_instance(&self, item: &SceneItem, viewport: &Viewport) -> QuadInstance {
        match &item.primitive {
            Primitive::Quad {
                rect,
                color,
                corner_radius,
            } => {
                let ndc = viewport.dp_rect_to_ndc(rect);
                QuadInstance {
                    rect: ndc,
                    color: color.to_array(),
                    radius: *corner_radius,
                    _pad: [0.0, 0.0, 0.0],
                }
            }
            // Non-Quad primitives produce zeroed instances (no-ops)
            _ => QuadInstance {
                rect: [0.0, 0.0, 0.0, 0.0],
                color: [0.0, 0.0, 0.0, 0.0],
                radius: 0.0,
                _pad: [0.0, 0.0, 0.0],
            },
        }
    }

    fn allocate_slot(&mut self) -> u32 {
        if let Some(slot) = self.free_slots.pop() {
            return slot;
        }
        let slot = self.instance_count;
        self.instance_count += 1;
        slot
    }
}
