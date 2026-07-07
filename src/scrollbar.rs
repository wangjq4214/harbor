use bytemuck::Zeroable;
use wgpu::util::DeviceExt;

use crate::{
    config::{
        SCROLLBAR_BORDER_RADIUS, SCROLLBAR_COLOR, SCROLLBAR_MARGIN, SCROLLBAR_MIN_THUMB_HEIGHT,
        SCROLLBAR_WIDTH, TEXT_PADDING,
    },
    gpu::{self, ColoredVertex, GpuContext},
    render::Layer,
    terminal::Screen,
};

// ── Scrollbar uniform ─────────────────────────────────────────────────────────

/// Uniform buffer data for scrollbar rounded-rect SDF in the fragment shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScrollbarUniform {
    /// (left, top, right, bottom) of the thumb rectangle in pixel coordinates.
    rect: [f32; 4],
    /// Corner radius in pixels.
    corner_radius: f32,
    _padding: [f32; 3],
}

// ── Scrollbar shader ─────────────────────────────────────────────────────────

/// Renders a per-vertex color quad, then masks to a rounded rectangle via SDF.
const SCROLLBAR_SHADER: &str = r#"
struct Uniform {
    rect: vec4<f32>,
    corner_radius: f32,
}

@group(0) @binding(0) var<uniform> u: Uniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> Varyings {
    var out: Varyings;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    // Rounded-rectangle signed-distance field in pixel space.
    let center = (u.rect.xy + u.rect.zw) * 0.5;
    let half_size = (u.rect.zw - u.rect.xy) * 0.5;
    let p = in.position.xy - center;
    let q = abs(p) - half_size + vec2<f32>(u.corner_radius, u.corner_radius);
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - u.corner_radius;
    let alpha = 1.0 - smoothstep(0.0, fwidth(d), d);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

// ── Vertex builders (free fn, testable without GPU handles) ───────────────────

/// Builds 6 vertices for the scrollbar thumb quad.
/// Returns degenerate (all-zero) vertices when scrollbar should be hidden
/// (alt screen or no scrollback history).
fn build_vertices(screen: &Screen, surf_w: f32, surf_h: f32) -> [ColoredVertex; 6] {
    if screen.is_alt() || screen.normal.scroll_count == 0 {
        return [ColoredVertex::default(); 6];
    }

    let visible_area_height = surf_h - 2.0 * TEXT_PADDING;
    let total_scrollable = screen.normal.scroll_count + screen.normal.visible_rows;
    let thumb_height =
        (screen.normal.visible_rows as f32 / total_scrollable as f32) * visible_area_height;
    let thumb_height = thumb_height.max(SCROLLBAR_MIN_THUMB_HEIGHT);

    let track_height = visible_area_height - thumb_height;
    let t = screen.normal.view_offset as f32 / screen.normal.scroll_count as f32;
    let thumb_top = TEXT_PADDING + (1.0 - t.clamp(0.0, 1.0)) * track_height;
    let thumb_bottom = thumb_top + thumb_height;

    let left = surf_w - SCROLLBAR_MARGIN - SCROLLBAR_WIDTH;
    let right = surf_w - SCROLLBAR_MARGIN;

    ColoredVertex::from_pixel_rect(left, thumb_top, right, thumb_bottom, SCROLLBAR_COLOR, surf_w, surf_h)
}

/// Computes the pixel-space rect and corner radius for the uniform buffer.
fn compute_uniform(screen: &Screen, surf_w: f32, surf_h: f32) -> ScrollbarUniform {
    if screen.is_alt() || screen.normal.scroll_count == 0 {
        return ScrollbarUniform::zeroed();
    }

    let visible_area_height = surf_h - 2.0 * TEXT_PADDING;
    let total_scrollable = screen.normal.scroll_count + screen.normal.visible_rows;
    let thumb_height =
        (screen.normal.visible_rows as f32 / total_scrollable as f32) * visible_area_height;
    let thumb_height = thumb_height.max(SCROLLBAR_MIN_THUMB_HEIGHT);

    let track_height = visible_area_height - thumb_height;
    let t = screen.normal.view_offset as f32 / screen.normal.scroll_count as f32;
    let thumb_top = TEXT_PADDING + (1.0 - t.clamp(0.0, 1.0)) * track_height;
    let thumb_bottom = thumb_top + thumb_height;

    let left = surf_w - SCROLLBAR_MARGIN - SCROLLBAR_WIDTH;
    let right = surf_w - SCROLLBAR_MARGIN;

    ScrollbarUniform {
        rect: [left, thumb_top, right, thumb_bottom],
        corner_radius: SCROLLBAR_BORDER_RADIUS,
        _padding: [0.0; 3],
    }
}

// ── ScrollbarLayer ───────────────────────────────────────────────────────────

/// Draws a vertical scrollbar thumb overlay on top of all other content.
/// Uses a uniform buffer for rounded-rect SDF in the fragment shader.
pub(crate) struct ScrollbarLayer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    visible: bool,
}

impl ScrollbarLayer {
    /// Creates the scrollbar render pipeline and allocates vertex + uniform buffers.
    pub(crate) fn new(gpu: &GpuContext, screen: &Screen) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());
        let empty = [ColoredVertex::default(); 6];
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);

        let (surf_w, surf_h) = gpu.surface_size();
        let uniform = compute_uniform(screen, surf_w as f32, surf_h as f32);
        let uniform_buffer = gpu.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scrollbar uniform buffer"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = Self::create_bind_group(gpu.device(), &pipeline, &uniform_buffer);

        // Upload initial (degenerate) vertices.
        let vertices = build_vertices(screen, surf_w as f32, surf_h as f32);
        gpu.queue()
            .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            visible: false,
        }
    }

    fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scrollbar shader"),
            source: wgpu::ShaderSource::Wgsl(SCROLLBAR_SHADER.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scrollbar bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZero::new(
                            std::mem::size_of::<ScrollbarUniform>() as u64,
                        ),
                    },
                    count: None,
                }],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scrollbar pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scrollbar pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(ColoredVertex::layout())],
            },
            primitive: wgpu::PrimitiveState::default(),
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

    fn create_bind_group(
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let bind_group_layout = pipeline.get_bind_group_layout(0);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scrollbar bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        })
    }

    /// Sets visibility flag.
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}

impl Layer for ScrollbarLayer {
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        let Some(screen) = screen else {
            return;
        };
        let (surf_w, surf_h) = gpu.surface_size();

        // Upload vertices (6 verts, cheap).
        let vertices = build_vertices(screen, surf_w as f32, surf_h as f32);
        gpu.queue()
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        // Upload uniform data for rounded-rect SDF.
        let uniform = compute_uniform(screen, surf_w as f32, surf_h as f32);
        gpu.queue().write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&uniform),
        );
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        if !self.visible {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Terminal;

    #[test]
    fn build_vertices_returns_degenerate_when_no_scrollback() {
        let term = Terminal::new(24, 80);
        // scroll_count == 0 by default
        let vertices = build_vertices(term.screen(), 800.0, 600.0);
        // All 6 vertices should be degenerate (position is [0, 0])
        for v in &vertices {
            assert_eq!(v.position, [0.0, 0.0], "expected degenerate vertex");
        }
    }

    #[test]
    fn build_vertices_returns_non_degenerate_with_scrollback() {
        let mut term = Terminal::new(24, 80);
        // Write enough lines to create scrollback.
        for _ in 0..50 {
            term.put_bytes(b"hello world\n");
        }
        // Move viewport up by scrolling.
        term.scroll_viewport_up(10);

        let vertices = build_vertices(term.screen(), 800.0, 600.0);
        // At least one vertex should have a non-zero position.
        let has_non_degenerate = vertices.iter().any(|v| v.position != [0.0, 0.0]);
        assert!(has_non_degenerate, "expected non-degenerate vertices with scrollback");
    }
}
