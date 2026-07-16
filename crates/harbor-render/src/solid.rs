use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::RenderEnvironment;
use harbor_types::Rect;

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
        Self { pipeline, vertices }
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
}
