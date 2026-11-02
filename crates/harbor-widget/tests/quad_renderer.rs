//! Public ordered QuadRenderer and Runtime GPU integration tests.
//!
//! GPU cases are opt-in; explicitly selected cases fail if a device is unavailable.

#[path = "quad_renderer/gpu.rs"]
mod gpu;

use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::renderer::quad::QuadRenderer;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::clip::RoundedClip;
use harbor_widget::scene::primitive::{
    Color, ExternalDrawContext, ExternalDrawFn, ExternalDrawGpu, ExternalDrawId, ExternalDrawMode,
    Primitive,
};
use harbor_widget::scene::{SceneDelta, SceneItem};
use harbor_widget::widgets::external_surface::ExternalSurface;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::widgets::stack::Stack;
use harbor_widget::{
    BackdropRim, BackdropTone, Border, BorderRadius, BoxDecoration, BoxShadow, ClipBehavior,
    DecoratedBox,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ── GPU helpers ─────────────────────────────────────────────────────────────

#[path = "../../../tests/support/gpu.rs"]
mod test_gpu;
use test_gpu::device as gpu_device;

/// Creates a headless RenderPass backed by a throwaway 256×256 texture.
fn draw_gpu<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> ExternalDrawGpu<'a> {
    ExternalDrawGpu::new(device, queue, wgpu::TextureFormat::Bgra8Unorm)
}

fn create_render_pass<'a>(
    device: &wgpu::Device,
    encoder: &'a mut wgpu::CommandEncoder,
) -> wgpu::RenderPass<'a> {
    let texture = test_gpu::readback::target(
        device,
        "test-render-target",
        256,
        256,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("test-pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
        multiview_mask: None,
    })
}

fn create_solid_pipeline(device: &wgpu::Device) -> Arc<wgpu::RenderPipeline> {
    create_solid_pipeline_rgb(device, 0.0, 1.0, 0.0)
}

fn create_solid_pipeline_rgb(
    device: &wgpu::Device,
    r: f32,
    g: f32,
    b: f32,
) -> Arc<wgpu::RenderPipeline> {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("custom-paint clipping test shader"),
        source: wgpu::ShaderSource::Wgsl(
            format!(
                r#"
                @vertex
                fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {{
                    var positions = array<vec2<f32>, 3>(
                        vec2<f32>(-1.0, -1.0),
                        vec2<f32>(3.0, -1.0),
                        vec2<f32>(-1.0, 3.0),
                    );
                    return vec4<f32>(positions[index], 0.0, 1.0);
                }}

                @fragment
                fn fs_main() -> @location(0) vec4<f32> {{
                    return vec4<f32>({r}, {g}, {b}, 1.0);
                }}
            "#
            )
            .into(),
        ),
    });
    Arc::new(
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("custom-paint clipping test pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        }),
    )
}

fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    read_texture_sized(device, queue, texture, 32)
}

fn read_texture_sized(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: u32,
) -> Vec<u8> {
    assert_eq!((texture.width(), texture.height()), (size, size));
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    test_gpu::readback::read(
        device,
        queue,
        encoder,
        texture,
        test_gpu::readback::Rows::Padded(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
    )
}

fn make_quad_item(id: u64, order: u32, x: f32, y: f32, w: f32, h: f32) -> SceneItem {
    SceneItem {
        id,
        primitive: Primitive::Quad {
            rect: Rect::from_min_size(Point::new(x, y), Size::new(w, h)),
            color: Color::RED,
            corner_radius: 0.0,
        },
        clips: Vec::new(),
        paint_order: order,
    }
}

fn make_rounded_item(id: u64, primitive: Primitive) -> SceneItem {
    SceneItem {
        id,
        primitive,
        clips: Vec::new(),
        paint_order: 0,
    }
}

fn bgra(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

fn clipped_child_quad(behavior: ClipBehavior, origin: Point, size: f32, radius: f32) -> SceneItem {
    let mut item = make_quad_item(1, 0, origin.x, origin.y, size, size);
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(origin, Size::new(size, size)),
            BorderRadius::all(radius).unwrap(),
            behavior,
        )
        .unwrap(),
    );
    item
}

fn render_quad_item(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    item: SceneItem,
    viewport: Viewport,
    target_size: u32,
) -> Vec<u8> {
    render_quad_items(device, queue, vec![item], viewport, target_size, None)
}

fn render_quad_items(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    items: Vec<SceneItem>,
    viewport: Viewport,
    target_size: u32,
    encode_ids: Option<&[u64]>,
) -> Vec<u8> {
    let mut renderer = QuadRenderer::new(device, wgpu::TextureFormat::Bgra8Unorm);
    let mut ordered = items.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|item| item.paint_order);
    let scene_ids: Vec<_> = ordered.iter().map(|item| item.id).collect();
    renderer.update(
        queue,
        &SceneDelta {
            added: items,
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    render_retained_quads(
        device,
        queue,
        &renderer,
        target_size,
        encode_ids.unwrap_or(&scene_ids),
    )
}

fn render_retained_quads(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &QuadRenderer,
    target_size: u32,
    paint_order: &[u64],
) -> Vec<u8> {
    let target = test_gpu::readback::target(
        device,
        "quad primitive render target",
        target_size,
        target_size,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("quad primitive render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        renderer.encode_ordered(&mut pass, paint_order.iter().copied());
    }
    queue.submit(Some(encoder.finish()));
    read_texture_sized(device, queue, &target, target_size)
}

fn test_gradient(start: Point, end: Point, stops: &[(f32, Color)]) -> harbor_widget::Paint {
    let stops: Vec<_> = stops
        .iter()
        .map(|(offset, color)| harbor_widget::GradientStop::new(*offset, *color))
        .collect();
    harbor_widget::LinearGradient::new(start, end, &stops)
        .unwrap()
        .into()
}

// ── Runtime::encode with External primitives ────────────────────────────────

fn encode_root_to_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    root: impl harbor_widget::view::Component + 'static,
    viewport: Viewport,
    size: u32,
) -> Vec<u8> {
    let target = test_gpu::readback::target(
        device,
        "custom-paint rounded clip target",
        size,
        size,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let mut runtime = Runtime::new();
    runtime.init_renderer(device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(root);
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("custom-paint rounded clip pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        runtime.encode(draw_gpu(device, queue), &mut pass, viewport, false);
    }
    queue.submit(Some(encoder.finish()));
    read_texture_sized(device, queue, &target, size)
}

fn solid_external(device: &wgpu::Device, draw_id: u64) -> ExternalSurface {
    solid_external_rgb(device, draw_id, 0.0, 1.0, 0.0)
}

fn solid_external_rgb(
    device: &wgpu::Device,
    draw_id: u64,
    r: f32,
    g: f32,
    b: f32,
) -> ExternalSurface {
    let pipeline = create_solid_pipeline_rgb(device, r, g, b);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, pass, _| {
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    ExternalSurface::new(draw_id).on_draw(handler)
}

fn clipped_external(device: &wgpu::Device, behavior: ClipBehavior, radius: f32) -> DecoratedBox {
    DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(radius).unwrap()))
        .clip_behavior(behavior)
        .child(solid_external(device, 1))
}

// The coordinate ramp makes displacement observable independently of tint or
// highlight painting. These are real encoder readbacks, not shader math mocks.

// Encoder-level filter tests deliberately use the public non-terminal widget API.
fn encode_filter_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    runtime: &mut Runtime,
    viewport: Viewport,
) -> Vec<u8> {
    encode_filter_frame_format(
        device,
        queue,
        runtime,
        viewport,
        wgpu::TextureFormat::Bgra8Unorm,
    )
}

fn encode_filter_frame_format(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    runtime: &mut Runtime,
    viewport: Viewport,
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    let size = viewport.physical_size.0;
    let target = test_gpu::readback::target(device, "backdrop pixel evidence", size, size, format);
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    runtime.encode_frame(
        ExternalDrawGpu::new(device, queue, format),
        &mut encoder,
        harbor_widget::renderer::FrameTarget {
            view: &view,
            clear_color: wgpu::Color::TRANSPARENT,
        },
        viewport,
        false,
    );
    queue.submit(Some(encoder.finish()));
    read_texture_sized(device, queue, &target, size)
}

fn material_test_scene(start: Color, end: Color, filter: harbor_widget::BackdropFilter) -> Stack {
    use harbor_widget::{GradientStop, LinearGradient};
    let ramp = LinearGradient::new(
        Point::ZERO,
        Point::new(1.0, 1.0),
        &[GradientStop::new(0.0, start), GradientStop::new(1.0, end)],
    )
    .unwrap();
    Stack::new()
        .push_child(
            DecoratedBox::new(BoxDecoration::new().try_paint(ramp.into()).unwrap())
                .child(SizedBox::new(Size::new(32.0, 32.0)).unwrap()),
        )
        .push_child(
            Padding::all(4.0).unwrap().child(
                filter
                    .border_radius(BorderRadius::all(6.0).unwrap())
                    .child(SizedBox::new(Size::new(24.0, 24.0)).unwrap()),
            ),
        )
        .push_child(
            Padding::new(harbor_widget::Insets {
                top: 4.0,
                right: 0.0,
                bottom: 0.0,
                left: 14.0,
            })
            .unwrap()
            .child(
                SizedBox::new(Size::new(4.0, 2.0))
                    .unwrap()
                    .background(Color::RED)
                    .expect("valid color"),
            ),
        )
}
