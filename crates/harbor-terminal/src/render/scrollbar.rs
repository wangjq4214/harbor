use harbor_types::TerminalSnapshot;
use wgpu::util::DeviceExt;

use harbor_config::{
    SCROLLBAR_BORDER_RADIUS, SCROLLBAR_COLOR, SCROLLBAR_MARGIN, SCROLLBAR_MIN_THUMB_HEIGHT,
    SCROLLBAR_WIDTH,
};

use super::gpu::{self, ColoredVertex, GpuContext};
use crate::render::RenderViewport;

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
///
/// Track geometry is relative to the render allocation, not the full surface, so
/// inset CustomPaint regions keep the scrollbar on the allocation's right edge.
pub fn compute_thumb_rect(snap: &TerminalSnapshot, viewport: &RenderViewport) -> Option<[f32; 4]> {
    if snap.is_alt || snap.scroll_count == 0 {
        return None;
    }

    let (origin_x, origin_y) = viewport.allocation_origin;
    let alloc_w = viewport.allocation_size.0 as f32;
    let alloc_h = viewport.allocation_size.1 as f32;
    let padding = viewport.padding;

    let track_top = origin_y + padding;
    let track_bottom = origin_y + alloc_h - padding;
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

    let right = origin_x + alloc_w - SCROLLBAR_MARGIN;
    let left = right - SCROLLBAR_WIDTH;

    Some([left, thumb_top, right, thumb_top + thumb_height])
}

/// Builds quad vertices for the scrollbar thumb.
fn build_vertices(snap: &TerminalSnapshot, viewport: &RenderViewport) -> [ColoredVertex; 6] {
    let (surf_w, surf_h) = viewport.surface_dimensions();
    match compute_thumb_rect(snap, viewport) {
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
fn compute_uniform(snap: &TerminalSnapshot, viewport: &RenderViewport) -> ScrollbarUniform {
    let rect = compute_thumb_rect(snap, viewport).unwrap_or([0.0; 4]);
    ScrollbarUniform {
        rect,
        corner_radius: SCROLLBAR_BORDER_RADIUS,
        _padding: [0.0; 3],
    }
}

// ── Scrollbar ─────────────────────────────────────────────────────────────────

/// Cached identity for whether scrollbar vertex/uniform uploads can be skipped.
///
/// Changing any field forces a rebuild of the thumb vertices and SDF uniform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScrollbarUploadKey {
    /// Visible grid row count from the terminal snapshot.
    rows: usize,
    /// Visible grid column count from the terminal snapshot.
    cols: usize,
    /// Number of rows in scrollback (affects thumb height and presence).
    scroll_count: usize,
    /// How far the view is scrolled into scrollback (affects thumb Y).
    view_offset: usize,
    /// Alt-screen flag; scrollbar is hidden while true.
    is_alt: bool,
    /// Allocation origin X in pixels, stored as `f32::to_bits`.
    allocation_origin_x_bits: u32,
    /// Allocation origin Y in pixels, stored as `f32::to_bits`.
    allocation_origin_y_bits: u32,
    /// Allocation width in physical pixels (track sits on its right edge).
    allocation_width: u32,
    /// Allocation height in physical pixels (track spans this height).
    allocation_height: u32,
    /// Full surface width in physical pixels (NDC projection).
    surface_width: u32,
    /// Full surface height in physical pixels (NDC projection).
    surface_height: u32,
}

impl ScrollbarUploadKey {
    fn from_snapshot(snap: &TerminalSnapshot, viewport: &RenderViewport) -> Self {
        Self {
            rows: snap.rows,
            cols: snap.cols,
            scroll_count: snap.scroll_count,
            view_offset: snap.view_offset,
            is_alt: snap.is_alt,
            allocation_origin_x_bits: viewport.allocation_origin.0.to_bits(),
            allocation_origin_y_bits: viewport.allocation_origin.1.to_bits(),
            allocation_width: viewport.allocation_size.0,
            allocation_height: viewport.allocation_size.1,
            surface_width: viewport.surface_size.0,
            surface_height: viewport.surface_size.1,
        }
    }
}

/// Scrollbar component: renders a rounded-rectangle thumb on the right edge.
pub struct Scrollbar {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Cached upload key including scroll state and viewport geometry.
    last_upload_key: Option<ScrollbarUploadKey>,
    /// Whether the thumb is currently visible (fades out after inactivity).
    visible: bool,
    /// Last user activity timestamp (cursor move, scroll event).
    last_activity: std::time::Instant,
    /// Tracks whether the mouse cursor is currently inside the window surface.
    #[allow(dead_code)]
    cursor_inside: bool,
}

impl Scrollbar {
    pub fn new(gpu: &GpuContext, snap: &TerminalSnapshot, viewport: &RenderViewport) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());

        let initial_vertices = build_vertices(snap, viewport);
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &initial_vertices);

        let initial_uniform = compute_uniform(snap, viewport);
        let uniform_buffer = gpu
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scrollbar uniform buffer"),
                contents: bytemuck::bytes_of(&initial_uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = Self::create_bind_group(gpu.device(), &pipeline, &uniform_buffer);

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            last_upload_key: Some(ScrollbarUploadKey::from_snapshot(snap, viewport)),
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

    pub fn invalidate_projection(&mut self) {
        self.last_upload_key = None;
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        snap: Option<&TerminalSnapshot>,
        viewport: &RenderViewport,
    ) {
        let Some(snap) = snap else {
            return;
        };
        let key = ScrollbarUploadKey::from_snapshot(snap, viewport);
        if self.last_upload_key == Some(key) {
            return;
        }
        self.last_upload_key = Some(key);

        let vertices = build_vertices(snap, viewport);
        gpu.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        let uniform = compute_uniform(snap, viewport);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terminal;
    use crate::render::RenderViewport;

    fn full_surface_viewport(width: u32, height: u32) -> RenderViewport {
        RenderViewport::with_surface(10.0, 20.0, (width, height), (width, height))
    }

    #[test]
    fn scrollbar_hidden_in_alt_screen() {
        let mut terminal = Terminal::new_headless(24, 80);
        terminal.put_bytes(b"\x1b[?1049h");
        let snap = terminal.screen().terminal_snapshot();
        assert!(compute_thumb_rect(&snap, &full_surface_viewport(800, 600)).is_none());
    }

    #[test]
    fn scrollbar_hidden_when_no_scrollback() {
        let terminal = Terminal::new_headless(24, 80);
        let snap = terminal.screen().terminal_snapshot();
        assert!(compute_thumb_rect(&snap, &full_surface_viewport(800, 600)).is_none());
    }

    #[test]
    fn scrollbar_visible_when_scrollback_exists() {
        let mut terminal = Terminal::new_headless(2, 5);
        terminal.put_str("1\n2\n3\n");
        let snap = terminal.screen().terminal_snapshot();

        let rect = compute_thumb_rect(&snap, &full_surface_viewport(800, 600));
        assert!(
            rect.is_some(),
            "thumb rect should be present when scrollback > 0"
        );
        let [left, top, right, bottom] = rect.unwrap();
        assert!(right > left, "thumb width must be positive");
        assert!(bottom > top, "thumb height must be positive");
    }

    #[test]
    fn scrollbar_uses_allocation_origin_for_inset_regions() {
        let mut terminal = Terminal::new_headless(2, 5);
        terminal.put_str("1\n2\n3\n");
        let snap = terminal.screen().terminal_snapshot();

        let mut inset = RenderViewport::with_surface(10.0, 20.0, (400, 300), (800, 600));
        inset.allocation_origin = (100.0, 50.0);

        let [left, top, right, bottom] =
            compute_thumb_rect(&snap, &inset).expect("thumb should exist");
        assert!(
            (right - (100.0 + 400.0 - SCROLLBAR_MARGIN)).abs() < 0.01,
            "right edge follows allocation + width"
        );
        assert!(
            (left - (right - SCROLLBAR_WIDTH)).abs() < 0.01,
            "thumb width is SCROLLBAR_WIDTH"
        );
        assert!(top >= 50.0, "thumb stays below allocation origin y");
        assert!(
            bottom <= 50.0 + 300.0,
            "thumb stays within allocation height"
        );
    }

    #[test]
    fn should_hide_thumb_when_allocation_track_height_is_non_positive() {
        // Arrange: allocation height too small for padding on both edges.
        let mut terminal = Terminal::new_headless(2, 5);
        terminal.put_str("1\n2\n3\n");
        let snap = terminal.screen().terminal_snapshot();
        let mut viewport = RenderViewport::with_surface(10.0, 20.0, (400, 20), (800, 600));
        viewport.allocation_origin = (100.0, 50.0);
        viewport.padding = 16.0;

        // Act
        let rect = compute_thumb_rect(&snap, &viewport);

        // Assert
        assert!(rect.is_none());
    }

    #[test]
    fn should_offset_thumb_x_by_allocation_origin_not_full_surface() {
        // Arrange
        let mut terminal = Terminal::new_headless(2, 5);
        terminal.put_str("1\n2\n3\n");
        let snap = terminal.screen().terminal_snapshot();
        let full = full_surface_viewport(800, 600);
        let mut inset = RenderViewport::with_surface(10.0, 20.0, (400, 300), (800, 600));
        inset.allocation_origin = (120.0, 40.0);

        // Act
        let [full_left, _, full_right, _] =
            compute_thumb_rect(&snap, &full).expect("full-surface thumb");
        let [inset_left, _, inset_right, _] =
            compute_thumb_rect(&snap, &inset).expect("inset thumb");

        // Assert: inset thumb sits on the allocation's right edge, not the surface.
        assert!((full_right - (800.0 - SCROLLBAR_MARGIN)).abs() < 0.01);
        assert!((inset_right - (120.0 + 400.0 - SCROLLBAR_MARGIN)).abs() < 0.01);
        assert!((inset_left - inset_right + SCROLLBAR_WIDTH).abs() < 0.01);
        assert!((full_left - full_right + SCROLLBAR_WIDTH).abs() < 0.01);
        assert!((inset_right - full_right).abs() > 1.0);
    }
}
