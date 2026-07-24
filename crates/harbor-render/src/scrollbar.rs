use bytemuck::Zeroable;
use harbor_types::TerminalSnapshot;
use wgpu::util::DeviceExt;

use harbor_config::{
    SCROLLBAR_BORDER_RADIUS, SCROLLBAR_COLOR, SCROLLBAR_MARGIN, SCROLLBAR_MIN_THUMB_HEIGHT,
    SCROLLBAR_WIDTH, TEXT_PADDING,
};

use crate::{
    caps::{InteractionResult, UiRequest, WaitResult},
    Component, EventResult,
    gpu::{self, ColoredVertex, GpuContext},
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
/// (alt snap or no scrollback history).
fn build_vertices(snap: &TerminalSnapshot, surf_w: f32, surf_h: f32) -> [ColoredVertex; 6] {
    // No scrollback or alt snap → no thumb to show.
    if snap.is_alt || snap.scroll_count == 0 {
        return [ColoredVertex::default(); 6];
    }

    // Thumb height = visible_rows / total_lines × visible_area_height,
    // clamped to a minimum so it never disappears entirely.
    let visible_area_height = surf_h - 2.0 * TEXT_PADDING;
    let total_scrollable = snap.scroll_count + snap.rows;
    let thumb_height = (snap.rows as f32 / total_scrollable as f32) * visible_area_height;
    let thumb_height = thumb_height.max(SCROLLBAR_MIN_THUMB_HEIGHT);

    // Thumb vertical position: view_offset as a ratio of total scrollback,
    // inverted (top of track = newest history).  `t` goes 0 (top) → 1 (bottom).
    let track_height = visible_area_height - thumb_height;
    let t = snap.view_offset as f32 / snap.scroll_count as f32;
    let thumb_top = TEXT_PADDING + (1.0 - t.clamp(0.0, 1.0)) * track_height;
    let thumb_bottom = thumb_top + thumb_height;

    // Thumb horizontal: right-aligned within `SCROLLBAR_MARGIN` from the edge.
    let left = surf_w - SCROLLBAR_MARGIN - SCROLLBAR_WIDTH;
    let right = surf_w - SCROLLBAR_MARGIN;

    ColoredVertex::from_pixel_rect(
        left,
        thumb_top,
        right,
        thumb_bottom,
        SCROLLBAR_COLOR,
        surf_w,
        surf_h,
    )
}

/// Computes the pixel-space rect and corner radius for the uniform buffer.
fn compute_uniform(snap: &TerminalSnapshot, surf_w: f32, surf_h: f32) -> ScrollbarUniform {
    // Same early-exit and geometry as `build_vertices` — returns zeroed uniform
    // when the scrollbar should be hidden.
    if snap.is_alt || snap.scroll_count == 0 {
        return ScrollbarUniform::zeroed();
    }

    let visible_area_height = surf_h - 2.0 * TEXT_PADDING;
    let total_scrollable = snap.scroll_count + snap.rows;
    let thumb_height = (snap.rows as f32 / total_scrollable as f32) * visible_area_height;
    let thumb_height = thumb_height.max(SCROLLBAR_MIN_THUMB_HEIGHT);

    let track_height = visible_area_height - thumb_height;
    let t = snap.view_offset as f32 / snap.scroll_count as f32;
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
// ── Scrollbar ─────────────────────────────────────────────────────

/// Combined scrollbar rendering + visibility state machine.
/// Replaces ScrollbarLayer in the component tree.
pub struct Scrollbar {
    /// wgpu render pipeline for the scrollbar thumb.
    pipeline: wgpu::RenderPipeline,
    /// Pre-allocated 6-vertex quad buffer (rewritten every `prepare`).
    vertex_buffer: wgpu::Buffer,
    /// Uniform buffer for the rounded-rect SDF shader (rect + corner radius).
    uniform_buffer: wgpu::Buffer,
    /// Bind group connecting the uniform buffer to the fragment shader.
    bind_group: wgpu::BindGroup,
    /// Whether the scrollbar is currently rendered (controlled by mouse activity).
    visible: bool,
    /// Mouse cursor is currently inside the window.
    cursor_inside: bool,
    /// Timestamp of the last mouse move or scroll wheel activity (for auto-hide timeout).
    last_activity: std::time::Instant,
    /// Inputs used for the last scrollbar buffer upload.
    last_upload_key: Option<(usize, usize, usize, usize, bool, u32, u32)>,
}

impl Scrollbar {
    /// Creates the scrollbar: allocates pipeline, vertex buffer, uniform buffer,
    /// and bind group. Uploads initial (degenerate) vertices — the scrollbar
    /// starts hidden until mouse activity triggers `show()`.
    pub fn new(gpu: &GpuContext, snap: &TerminalSnapshot) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());
        let empty = [ColoredVertex::default(); 6];
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);

        let (surf_w, surf_h) = gpu.surface_size();
        let uniform = compute_uniform(snap, surf_w as f32, surf_h as f32);
        let uniform_buffer = gpu
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scrollbar uniform buffer"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = Self::create_bind_group(gpu.device(), &pipeline, &uniform_buffer);

        // Upload initial (degenerate) vertices.
        let vertices = build_vertices(snap, surf_w as f32, surf_h as f32);
        gpu.write_buffer(&vertex_buffer,
        0,
        bytemuck::cast_slice(&vertices),);

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            visible: false,
            cursor_inside: false,
            last_activity: std::time::Instant::now(),
            last_upload_key: None,
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

    /// Show the scrollbar and reset the activity timer.
    fn show(&mut self) {
        self.visible = true;
        self.last_activity = std::time::Instant::now();
    }
}

impl Component for Scrollbar {
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>) {
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
        gpu.write_buffer(&self.vertex_buffer,
        0,
        bytemuck::cast_slice(&vertices),);

        let uniform = compute_uniform(snap, surf_w as f32, surf_h as f32);
        gpu.write_buffer(&self.uniform_buffer,
        0,
        bytemuck::bytes_of(&uniform),);
    }

    /// Draw the scrollbar thumb (no-op when hidden).
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

impl Scrollbar {
    pub fn handle_event(&mut self, event: &winit::event::WindowEvent, _snapshot: &TerminalSnapshot) -> InteractionResult {
        let mut result = InteractionResult::continue_();
        match event {
            winit::event::WindowEvent::CursorEntered { .. } => { self.cursor_inside = true; self.show(); result.event = EventResult::Handled; result.requests.push(UiRequest::Redraw); }
            winit::event::WindowEvent::CursorMoved { .. } => { self.last_activity = std::time::Instant::now(); if !self.visible && self.cursor_inside { self.show(); result.requests.push(UiRequest::Redraw); } result.event = EventResult::Handled; }
            winit::event::WindowEvent::CursorLeft { .. } => self.cursor_inside = false,
            winit::event::WindowEvent::MouseWheel { .. } => { self.last_activity = std::time::Instant::now(); if !self.visible { self.show(); result.requests.push(UiRequest::Redraw); } }
            _ => {}
        }
        result
    }

    pub fn on_about_to_wait(&mut self) -> WaitResult {
        if !self.visible { return WaitResult::default(); }
        let hide_delay = std::time::Duration::from_millis(harbor_config::SCROLLBAR_HIDE_DELAY_MS);
        if self.last_activity.elapsed() >= hide_delay { self.visible = false; return WaitResult { deadline: None, requests: vec![UiRequest::Redraw] }; }
        WaitResult { deadline: Some(self.last_activity + hide_delay), requests: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_terminal::Terminal;

    #[test]
    fn build_vertices_returns_degenerate_when_no_scrollback() {
        let term = Terminal::new(24, 80);
        // scroll_count == 0 by default
        let vertices = build_vertices(&term.snapshot(), 800.0, 600.0);
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

        let vertices = build_vertices(&term.snapshot(), 800.0, 600.0);
        // At least one vertex should have a non-zero position.
        let has_non_degenerate = vertices.iter().any(|v| v.position != [0.0, 0.0]);
        assert!(
            has_non_degenerate,
            "expected non-degenerate vertices with scrollback"
        );
    }
}
