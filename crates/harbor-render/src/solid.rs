use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::cache::{CachedGrid, GridCache};
use crate::{RectPatch, RenderEnvironment, RenderIdentity};
use harbor_types::Rect;
use std::collections::HashSet;

const SHADER: &str = r#"
struct Input { @location(0) position: vec2<f32>, @location(1) color: vec4<f32>, }
struct Output { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32>, }
@vertex fn vs_main(input: Input) -> Output { var output: Output; output.position = vec4<f32>(input.position, 0.0, 1.0); output.color = input.color; return output; }
@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> { return input.color; }
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// Renderer-owned solid-rectangle GPU primitive.
pub(crate) struct SolidRenderer {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    grids: GridCache,
}

impl SolidRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("harbor renderer solid shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("harbor renderer solid pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("harbor renderer solid pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                })],
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
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("harbor renderer solid vertices"),
            contents: bytemuck::cast_slice(&[Vertex::zeroed(); 6]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            vertices,
            grids: GridCache::new(),
        }
    }

    pub(crate) fn draw(
        &self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        rect: Rect,
        color: [f32; 4],
        environment: RenderEnvironment,
    ) {
        let (width, height) = environment.logical_size();
        let x0 = rect.x / width * 2.0 - 1.0;
        let x1 = (rect.x + rect.width) / width * 2.0 - 1.0;
        let y0 = 1.0 - rect.y / height * 2.0;
        let y1 = 1.0 - (rect.y + rect.height) / height * 2.0;
        let vertices = [
            Vertex {
                position: [x0, y0],
                color,
            },
            Vertex {
                position: [x0, y1],
                color,
            },
            Vertex {
                position: [x1, y1],
                color,
            },
            Vertex {
                position: [x0, y0],
                color,
            },
            Vertex {
                position: [x1, y1],
                color,
            },
            Vertex {
                position: [x1, y0],
                color,
            },
        ];
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices));
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..6, 0..1);
    }

    pub(crate) fn draw_patch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        patch: &RectPatch,
        environment: RenderEnvironment,
    ) {
        let reset = self
            .grids
            .get(patch.identity)
            .is_none_or(|grid| grid.slots != patch.slots);
        if reset {
            let vertices = vec![Vertex::zeroed(); patch.slots.max(1) * 6];
            self.grids.insert(
                patch.identity,
                CachedGrid {
                    slots: patch.slots,
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("harbor renderer cached rectangle grid"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    }),
                },
            );
        }
        let grid = self.grids.get(patch.identity).expect("grid inserted");
        for update in &patch.updates {
            if update.slot >= grid.slots {
                continue;
            }
            let vertices = update.rect.map_or([Vertex::zeroed(); 6], |rect| {
                vertices(rect, update.color.0, environment)
            });
            queue.write_buffer(
                &grid.vertices,
                (update.slot * 6 * std::mem::size_of::<Vertex>()) as u64,
                bytemuck::cast_slice(&vertices),
            );
        }
        if grid.slots > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, grid.vertices.slice(..));
            pass.draw(0..(grid.slots * 6) as u32, 0..1);
        }
    }

    pub(crate) fn retain_identities(&mut self, identities: &HashSet<RenderIdentity>) {
        self.grids.retain(identities);
    }
}

fn vertices(rect: Rect, color: [f32; 4], environment: RenderEnvironment) -> [Vertex; 6] {
    let (width, height) = environment.logical_size();
    let x0 = rect.x / width * 2.0 - 1.0;
    let x1 = (rect.x + rect.width) / width * 2.0 - 1.0;
    let y0 = 1.0 - rect.y / height * 2.0;
    let y1 = 1.0 - (rect.y + rect.height) / height * 2.0;
    [
        Vertex { position: [x0, y0], color },
        Vertex { position: [x0, y1], color },
        Vertex { position: [x1, y1], color },
        Vertex { position: [x0, y0], color },
        Vertex { position: [x1, y1], color },
        Vertex { position: [x1, y0], color },
    ]
}
