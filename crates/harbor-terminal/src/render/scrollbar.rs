use bytemuck::Zeroable;
use harbor_types::TerminalSnapshot;
use wgpu::util::DeviceExt;

use harbor_config::{
    SCROLLBAR_BORDER_RADIUS, SCROLLBAR_COLOR, SCROLLBAR_MARGIN, SCROLLBAR_MIN_THUMB_HEIGHT,
    SCROLLBAR_WIDTH, TEXT_PADDING,
};

use super::gpu::{self, ColoredVertex, GpuContext};

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
    let pos = in.position.xy;
    let center = (u.rect.xy + u.rect.zw) * 0.5;
    let half_size = (u.rect.zw - u.rect.xy) * 0.5 - vec2<f32>(u.corner_radius);

    let d = abs(pos - center) - half_size;
    let dist = length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0) - u.corner_radius;

    // Smooth anti-aliased edge over 1 pixel boundary.
    let alpha = 1.0 - smoothstep(-0.5, 0.5, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

// ── Helper functions (testable without GPU handles) ──────────────────────────

/// Computes the thumb bounding rectangle (left, top, right, bottom) in pixel coordinates.
/// Returns None when the thumb should not be drawn (alt screen or no scrollback).
pub fn compute_thumb_rect(snap: &TerminalSnapshot, surf_w: f32, surf_h: f32) -> Option<[f32; 4]> {
    if snap.is_alt || snap.scroll_count == 0 {
        return None;
    }

    let track_top = TEXT_PADDING;
    let track_bottom = surf_h - TEXT_PADDING;
    let track_height = track_bottom - track_top;
    if track_height <= 0.0 {
        return None;
    }

    let total_rows = snap.rows + snap.scroll_count;
    let thumb_height = ((snap.rows as f32 / total_rows as f32) * track_height)
        .max(SCROLLBAR_MIN_THUMB_HEIGHT)
        .min(track_height);

    let max_view_offset = snap.scroll_count;
    let scroll_fraction = 1.0 - (snap.view_offset as f32 / max_view_offset as f32);
    let max_thumb_top = track_height - thumb_height;
    let thumb_top = track_top + scroll_fraction * max_thumb_top;

    let right = surf_w - SCROLLBAR_MARGIN;
    let left = right - SCROLLBAR_WIDTH;

    Some([left, thumb_top, right, thumb_top + thumb_height])
}

/// Builds quad vertices for the scrollbar thumb.
fn build_vertices(snap: &TerminalSnapshot, surf_w: f32, surf_h: f32) -> [ColoredVertex; 6] {
    match compute_thumb_rect(snap, surf_w, surf_h) {
        Some([left, top, right, bottom]) => ColoredVertex::from_pixel_rect(
            left,
            top,
            right,
            bottom,
            SCROLLBAR_COLOR,
            surf_w,
            surf_h,
        ),
        None => [ColoredVertex::default(); 6],
    }
}

/// Computes the uniform data for the scrollbar shader.
fn compute_uniform(snap: &TerminalSnapshot, surf_w: f32, surf_h: f32) -> ScrollbarUniform {
    let rect = compute_thumb_rect(snap, surf_w, surf_h).unwrap_or([0.0; 4]);
    ScrollbarUniform {
        rect,
        corner_radius: SCROLLBAR_BORDER_RADIUS,
        _padding: [0.0; 3],
    }
}

// ── Scrollbar ─────────────────────────────────────────────────────────────────

/// Scrollbar component: renders a rounded-rectangle thumb on the right edge.
pub struct Scrollbar {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Cached (rows, cols, scroll_count, view_offset, is_alt, surf_w, surf_h)
    last_upload_key: Option<(usize, usize, usize, usize, bool, u32, u32)>,
    /// Whether the thumb is currently visible (fades out after inactivity).
    visible: bool,
    /// Last user activity timestamp (cursor move, scroll event).
    last_activity: std::time::Instant,
    /// Tracks whether the mouse cursor is currently inside the window surface.
    #[allow(dead_code)]
    cursor_inside: bool,
}

impl Scrollbar {
    pub fn new(gpu: &GpuContext, snap: &TerminalSnapshot) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());
        let (surf_w, surf_h) = gpu.surface_size();

        let initial_vertices = build_vertices(snap, surf_w as f32, surf_h as f32);
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &initial_vertices);

        let initial_uniform = compute_uniform(snap, surf_w as f32, surf_h as f32);
        let uniform_buffer = gpu
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scrollbar uniform buffer"),
                contents: bytemuck::bytes_of(&initial_uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = Self::create_bind_group(gpu.device(), &pipeline, &uniform_buffer);

        let initial_key = (
            snap.rows,
            snap.cols,
            snap.scroll_count,
            snap.view_offset,
            snap.is_alt,
            surf_w,
            surf_h,
        );

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            last_upload_key: Some(initial_key),
            visible: false,
            last_activity: std::time::Instant::now(),
            cursor_inside: false,
        }
    }

    fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scrollbar shader"),
            source: wgpu::ShaderSource::Wgsl(SCROLLBAR_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scrollbar bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
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

    /// Show the scrollbar and reset the activity timer.
    pub fn show(&mut self) {
        self.visible = true;
        self.last_activity = std::time::Instant::now();
    }

    pub fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
        let Some(snap) = snap else {
            return;
        };
        let (surf_w, surf_h) = gpu.surface_size();
        let key = (
            snap.rows,
            snap.cols,
            snap.scroll_count,
            snap.view_offset,
            snap.is_alt,
            surf_w,
            surf_h,
        );
        if self.last_upload_key == Some(key) {
            return;
        }
        self.last_upload_key = Some(key);

        let vertices = build_vertices(snap, surf_w as f32, surf_h as f32);
        gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        let uniform = compute_uniform(snap, surf_w as f32, surf_h as f32);
        gpu.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Draw the scrollbar thumb (no-op when hidden).
    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if !self.visible {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }

    pub fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.last_upload_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terminal;

    #[test]
    fn scrollbar_hidden_in_alt_screen() {
        let mut terminal = Terminal::new_headless(24, 80);
        terminal.put_bytes(b"\x1b[?1049h");
        let snap = terminal.screen().terminal_snapshot();
        assert!(compute_thumb_rect(&snap, 800.0, 600.0).is_none());
    }

    #[test]
    fn scrollbar_hidden_when_no_scrollback() {
        let terminal = Terminal::new_headless(24, 80);
        let snap = terminal.screen().terminal_snapshot();
        assert!(compute_thumb_rect(&snap, 800.0, 600.0).is_none());
    }

    #[test]
    fn scrollbar_visible_when_scrollback_exists() {
        let mut terminal = Terminal::new_headless(2, 5);
        terminal.put_str("1\n2\n3\n");
        let snap = terminal.screen().terminal_snapshot();

        let rect = compute_thumb_rect(&snap, 800.0, 600.0);
        assert!(
            rect.is_some(),
            "thumb rect should be present when scrollback > 0"
        );
        let [left, top, right, bottom] = rect.unwrap();
        assert!(right > left, "thumb width must be positive");
        assert!(bottom > top, "thumb height must be positive");
    }
}
