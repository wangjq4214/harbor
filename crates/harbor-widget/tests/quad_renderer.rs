//! Integration tests for QuadRenderer — slot_of, encode_range, and encode.
//!
//! These tests require a wgpu Device; they are skipped silently if no GPU
//! adapter is available.

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
use harbor_widget::widgets::custom_paint::CustomPaint;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::widgets::stack::Stack;
use harbor_widget::{Border, BorderRadius, BoxDecoration, BoxShadow, ClipBehavior, DecoratedBox};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ── GPU helpers ─────────────────────────────────────────────────────────────

/// Tries to create a wgpu Device + Queue. Returns `None` when no adapter is
/// available (headless CI, etc.), so GPU tests degrade gracefully.
fn try_create_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("test-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .ok()
}

/// Creates a headless RenderPass backed by a throwaway 256×256 texture.
fn draw_gpu<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> ExternalDrawGpu<'a> {
    ExternalDrawGpu::new(device, queue, wgpu::TextureFormat::Bgra8Unorm)
}

fn create_render_pass<'a>(
    device: &wgpu::Device,
    encoder: &'a mut wgpu::CommandEncoder,
) -> wgpu::RenderPass<'a> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test-render-target"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
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
    const BYTES_PER_ROW: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("custom-paint clipping test readback"),
        size: (size * BYTES_PER_ROW) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(BYTES_PER_ROW),
                rows_per_image: Some(size),
            },
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let bytes = slice
        .get_mapped_range()
        .expect("readback buffer should be mapped")
        .to_vec();
    buffer.unmap();
    bytes
}

fn make_viewport() -> Viewport {
    Viewport::new(800, 600, 1.0)
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
    renderer.update(
        queue,
        &SceneDelta {
            added: items,
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    render_retained_quads(device, queue, &renderer, target_size, encode_ids)
}

fn render_retained_quads(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &QuadRenderer,
    target_size: u32,
    encode_ids: Option<&[u64]>,
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("quad primitive render target"),
        size: wgpu::Extent3d {
            width: target_size,
            height: target_size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
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
        if let Some(ids) = encode_ids {
            for id in ids {
                renderer.encode_range(&mut pass, renderer.slot_of(*id).unwrap(), 1);
            }
        } else {
            renderer.encode(&mut pass);
        }
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

#[test]
fn gradient_eight_stops_respect_nested_clips_and_translucent_edge_coverage() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for clipped eight-stop gradient");
        return;
    };
    let stops: Vec<_> = (0..8)
        .map(|i| {
            (
                i as f32 / 7.0,
                Color {
                    a: 0.5,
                    ..if i < 4 { Color::RED } else { Color::BLUE }
                },
            )
        })
        .collect();
    let paint = test_gradient(Point::ZERO, Point::new(1.0, 0.0), &stops);
    for scale in [1.0, 1.5, 2.0] {
        let mut item = make_rounded_item(
            1,
            Primitive::PaintedQuad {
                rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
                paint,
                corner_radii: [4.0; 4],
            },
        );
        for (origin, side, radius) in [(2.0, 28.0, 4.0), (4.0, 24.0, 6.0)] {
            item.clips.push(
                RoundedClip::new(
                    Rect::from_min_size(Point::new(origin, origin), Size::new(side, side)),
                    BorderRadius::all(radius).unwrap(),
                    ClipBehavior::AntiAlias,
                )
                .unwrap(),
            );
        }
        let side = (32.0 * scale) as u32;
        let pixels = render_quad_item(
            &device,
            &queue,
            item,
            Viewport::new(side, side, scale),
            side,
        );
        let at = |x: f32, y: f32| bgra(&pixels, (x * scale) as u32, (y * scale) as u32);
        assert_eq!(at(6.0, 16.0), [0, 0, 128, 128], "first half of stop array");
        assert_eq!(
            at(25.0, 16.0),
            [128, 0, 0, 128],
            "second half of stop array"
        );
        assert_eq!(
            at(4.0, 4.0),
            [0; 4],
            "ancestor rounded corner clips gradient"
        );
        assert_eq!(at(1.0, 16.0), [0; 4], "outside ancestors stays empty");
        let edge = at(4.0, 16.0);
        assert!(
            edge[3] > 0 && edge[3] < 128,
            "antialias scales paint alpha: {edge:?}"
        );
    }
}

#[test]
fn gradient_fill_and_stroke_share_stops_direction_clamping_and_rounded_coverage() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for gradient fill/stroke pixels");
        return;
    };
    for (start, end, reversed) in [
        (Point::ZERO, Point::new(1.0, 0.0), false),
        (Point::new(1.0, 0.0), Point::ZERO, true),
    ] {
        let paint = test_gradient(
            start,
            end,
            &[(0.25, Color::RED), (0.5, Color::GREEN), (0.75, Color::BLUE)],
        );
        let rect = Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0));
        for scale in [1.0, 1.5, 2.0] {
            let side = (32.0 * scale) as u32;
            let viewport = Viewport::new(side, side, scale);
            for border in [false, true] {
                let primitive = if border {
                    Primitive::PaintedBorder {
                        rect,
                        width: 3.0,
                        paint,
                        corner_radii: [6.0; 4],
                    }
                } else {
                    Primitive::PaintedQuad {
                        rect,
                        paint,
                        corner_radii: [6.0; 4],
                    }
                };
                let pixels = render_quad_item(
                    &device,
                    &queue,
                    make_rounded_item(1, primitive),
                    viewport.clone(),
                    side,
                );
                let y = if border { 1.0 } else { 16.0 };
                let left = bgra(&pixels, (6.0 * scale) as u32, (y * scale) as u32);
                let right = bgra(&pixels, (26.0 * scale) as u32, (y * scale) as u32);
                assert_eq!(
                    left,
                    if reversed {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }
                );
                assert_eq!(
                    right,
                    if reversed {
                        [0, 0, 255, 255]
                    } else {
                        [255, 0, 0, 255]
                    }
                );
                let middle = bgra(&pixels, (16.0 * scale) as u32, (y * scale) as u32);
                assert!(
                    middle[1] > 230 && middle[3] == 255,
                    "middle stop: {middle:?}"
                );
                assert_eq!(bgra(&pixels, 0, 0)[3], 0, "rounded corner remains empty");
                if border {
                    assert_eq!(
                        bgra(&pixels, side / 2, side / 2)[3],
                        0,
                        "stroke has no center fill"
                    );
                }
            }
        }
    }
}

#[test]
fn gradient_interpolates_premultiplied_colors_without_transparent_rgb_fringes() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for gradient alpha pixels");
        return;
    };
    let rect = Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0));
    let paint = test_gradient(
        Point::ZERO,
        Point::new(1.0, 0.0),
        &[
            (
                0.0,
                Color {
                    a: 0.0,
                    ..Color::RED
                },
            ),
            (
                1.0,
                Color {
                    a: 0.8,
                    ..Color::BLUE
                },
            ),
        ],
    );
    let ramp = make_rounded_item(
        2,
        Primitive::PaintedQuad {
            rect,
            paint,
            corner_radii: [0.0; 4],
        },
    );
    let viewport = Viewport::new(32, 32, 1.0);
    let pixels = render_quad_item(&device, &queue, ramp.clone(), viewport.clone(), 32);
    let pixel = bgra(&pixels, 16, 16);
    assert!(
        (i32::from(pixel[3]) - 105).abs() <= 1,
        "alpha ramp: {pixel:?}"
    );
    assert_eq!(pixel[2], 0, "transparent red contributes no fringe");
    assert_eq!(pixel[0], pixel[3], "blue is premultiplied exactly once");
    let green = make_rounded_item(
        1,
        Primitive::Quad {
            rect,
            color: Color::GREEN,
            corner_radius: 0.0,
        },
    );
    let pixels = render_quad_items(&device, &queue, vec![green, ramp], viewport, 32, None);
    let pixel = bgra(&pixels, 16, 16);
    assert!((i32::from(pixel[0]) - 105).abs() <= 1 && (i32::from(pixel[1]) - 150).abs() <= 1);
    assert_eq!(
        pixel[2..],
        [0, 255],
        "background remains visible through ramp"
    );
}

#[test]
fn gradient_table_survives_growth_retained_edits_slot_reuse_and_viewport_refresh() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for retained gradient table");
        return;
    };
    let rect = Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0));
    let make = |id, colors: [Color; 2]| {
        make_rounded_item(
            id,
            Primitive::PaintedQuad {
                rect,
                paint: test_gradient(
                    Point::ZERO,
                    Point::new(0.0, 1.0),
                    &[(0.25, colors[0]), (0.75, colors[1])],
                ),
                corner_radii: [0.0; 4],
            },
        )
    };
    let viewport = Viewport::new(32, 32, 1.0);
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let mut items: Vec<_> = (1..=300)
        .map(|id| {
            make(
                id,
                if id == 1 {
                    [Color::RED, Color::GREEN]
                } else {
                    [Color::BLUE, Color::WHITE]
                },
            )
        })
        .collect();
    renderer.update(
        &queue,
        &SceneDelta {
            added: items.clone(),
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, Some(&[1])),
            16,
            2
        ),
        [0, 0, 255, 255]
    );
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, Some(&[300])),
            16,
            2
        ),
        [255, 0, 0, 255]
    );
    let first_slot = renderer.slot_of(1);
    let reused_slot = renderer.slot_of(300);
    items[0] = make(1, [Color::BLACK, Color::WHITE]);
    renderer.update(
        &queue,
        &SceneDelta {
            removed: vec![300],
            modified: vec![items[0].clone()],
            added: vec![],
        },
        &viewport,
    );
    let solid = make_rounded_item(
        400,
        Primitive::PaintedQuad {
            rect,
            paint: Color::GREEN.into(),
            corner_radii: [0.0; 4],
        },
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![solid],
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    assert_eq!(renderer.slot_of(400), reused_slot);
    assert_eq!(renderer.slot_of(1), first_slot);
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, Some(&[400])),
            16,
            16
        ),
        [0, 255, 0, 255]
    );
    assert_eq!(
        bgra(
            &render_retained_quads(&device, &queue, &renderer, 32, Some(&[1])),
            16,
            2
        ),
        [0, 0, 0, 255]
    );
    renderer.refresh_viewport(&queue, &items[..299], &Viewport::new(64, 64, 2.0));
    let pixels = render_retained_quads(&device, &queue, &renderer, 64, Some(&[1]));
    assert_eq!(bgra(&pixels, 32, 60), [255; 4]);
    assert_eq!(bgra(&pixels, 32, 4), [0, 0, 0, 255]);
}

#[test]
fn generic_glass_composes_gradient_fill_stroke_live_blur_and_sharp_foreground() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for generic glass composition");
        return;
    };
    let paint = test_gradient(
        Point::ZERO,
        Point::new(0.0, 1.0),
        &[
            (
                0.0,
                Color {
                    a: 0.12,
                    ..Color::WHITE
                },
            ),
            (
                1.0,
                Color {
                    a: 0.04,
                    ..Color::WHITE
                },
            ),
        ],
    );
    let radius = BorderRadius::all(6.0).unwrap();
    let decoration = BoxDecoration::new()
        .try_paint(paint)
        .unwrap()
        .border(Border::with_paint(paint, 1.0).unwrap())
        .border_radius(radius);
    let glass = || {
        Padding::all(4.0).child(
            harbor_widget::BackdropFilter::new(2.0)
                .unwrap()
                .border_radius(radius)
                .child(
                    DecoratedBox::new(decoration.clone())
                        .clip_behavior(ClipBehavior::AntiAlias)
                        .child(SizedBox::new(Size::new(24.0, 24.0))),
                ),
        )
    };
    let root = |background| {
        Stack::new()
            .child(SizedBox::new(Size::new(32.0, 32.0)).color(Color::GREEN))
            .child(
                Padding::new(0.0, 0.0, 0.0, 16.0)
                    .child(SizedBox::new(Size::new(16.0, 32.0)).color(background)),
            )
            .child(glass())
            .child(Padding::all(13.0).child(SizedBox::new(Size::new(6.0, 6.0)).color(Color::RED)))
    };
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    for scale in [1.0, 1.5, 2.0] {
        let viewport = Viewport::new((32.0 * scale) as u32, (32.0 * scale) as u32, scale);
        runtime.set_viewport(viewport.clone());
        runtime.set_root(root(Color::BLUE));
        runtime.update(Instant::now());
        let first = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        let at =
            |pixels: &[u8], x: f32, y: f32| bgra(pixels, (x * scale) as u32, (y * scale) as u32);
        let mixed = at(&first, 15.0, 10.0);
        assert!(
            mixed[0] > 20 && mixed[1] > 80,
            "live background colors survive glass: {mixed:?}"
        );
        assert_eq!(
            at(&first, 16.0, 16.0),
            [0, 0, 255, 255],
            "foreground remains sharp"
        );
        assert_eq!(
            at(&first, 4.0, 4.0),
            [0, 255, 0, 255],
            "round corner unchanged"
        );
        let retained = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        assert_eq!(first, retained, "unchanged glass does not accumulate alpha");
        runtime.set_root(root(Color::RED));
        runtime.update(Instant::now());
        let changed = encode_filter_frame(&device, &queue, &mut runtime, viewport);
        let updated = at(&changed, 15.0, 10.0);
        assert!(
            updated[0] < mixed[0] && updated[2] > mixed[2],
            "new background appears through glass"
        );
    }
}

#[test]
fn should_render_outer_shadow_only_outside_the_shape_with_soft_and_hard_edges() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for outer shadow test");
        return;
    };
    let shadow = |blur_radius| {
        make_rounded_item(
            1,
            Primitive::OuterShadow {
                rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
                shape_rect: Rect::from_min_size(Point::new(16.0, 8.0), Size::new(8.0, 16.0)),
                occluder_rect: Rect::from_min_size(Point::new(17.0, 8.0), Size::new(6.0, 16.0)),
                color: Color::RED,
                corner_radii: [0.0; 4],
                occluder_radii: [0.0; 4],
                blur_radius,
            },
        )
    };
    let pixel = |pixels: &[u8], x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        [
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        ]
    };

    let soft = render_quad_item(&device, &queue, shadow(4.0), Viewport::new(32, 32, 1.0), 32);
    // An outer shadow leaves its spread-adjusted shape transparent even when
    // no separate fill is rendered.
    assert!(pixel(&soft, 20, 16)[3] <= 1);
    assert!(pixel(&soft, 15, 16)[2] >= pixel(&soft, 3, 16)[2]);
    assert!(pixel(&soft, 15, 16)[2] > 100);
    assert!(pixel(&soft, 3, 16)[3] <= 1);

    let hard = render_quad_item(&device, &queue, shadow(0.0), Viewport::new(32, 32, 1.0), 32);
    assert!(pixel(&hard, 20, 16)[3] <= 1);
    assert!(
        pixel(&hard, 16, 16)[3] > 150,
        "hard spread strip must cover the exposed shadow shape: {:?}",
        pixel(&hard, 16, 16)
    );
}

#[test]
fn should_constrain_outer_shadow_to_ancestor_clip_bounds() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for clipped shadow test");
        return;
    };
    let mut item = make_rounded_item(
        1,
        Primitive::OuterShadow {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            shape_rect: Rect::from_min_size(Point::new(12.0, 12.0), Size::new(8.0, 8.0)),
            occluder_rect: Rect::from_min_size(Point::new(12.0, 12.0), Size::new(8.0, 8.0)),
            color: Color::RED,
            corner_radii: [0.0; 4],
            occluder_radii: [0.0; 4],
            blur_radius: 4.0,
        },
    );
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::new(8.0, 8.0), Size::new(16.0, 16.0)),
            BorderRadius::all(8.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };

    assert_eq!(pixel(8, 8), &[0, 0, 0, 0]);
    assert!(
        pixel(10, 16)[2] > 0,
        "shadow inside rounded ancestor clip must remain visible"
    );
}

#[test]
fn should_compose_overlapping_shadows_in_source_order() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for layered shadow test");
        return;
    };
    let shadow = |id, color| SceneItem {
        id,
        primitive: Primitive::OuterShadow {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            shape_rect: Rect::from_min_size(Point::new(12.0, 12.0), Size::new(8.0, 8.0)),
            occluder_rect: Rect::from_min_size(Point::new(12.0, 12.0), Size::new(8.0, 8.0)),
            color,
            corner_radii: [0.0; 4],
            occluder_radii: [0.0; 4],
            blur_radius: 4.0,
        },
        clips: Vec::new(),
        paint_order: id as u32,
    };
    let pixels = render_quad_items(
        &device,
        &queue,
        vec![shadow(1, Color::RED), shadow(2, Color::BLUE)],
        Viewport::new(32, 32, 1.0),
        32,
        None,
    );
    let offset = (16 * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + 10 * 4) as usize;
    let overlap = &pixels[offset..offset + 4];

    assert!(
        overlap[0] > overlap[2],
        "later blue shadow must compose above red: {overlap:?}"
    );
    assert!(
        overlap[2] > 0,
        "bottom red shadow must still contribute: {overlap:?}"
    );
}

#[test]
fn should_encode_raw_shadow_order_when_it_differs_from_slot_order() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for raw-order test");
        return;
    };
    let shadow = |id, color| SceneItem {
        id,
        primitive: Primitive::OuterShadow {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            shape_rect: Rect::from_min_size(Point::new(12.0, 12.0), Size::new(8.0, 8.0)),
            occluder_rect: Rect::from_min_size(Point::new(12.0, 12.0), Size::new(8.0, 8.0)),
            color,
            corner_radii: [0.0; 4],
            occluder_radii: [0.0; 4],
            blur_radius: 4.0,
        },
        clips: Vec::new(),
        paint_order: 0,
    };
    let pixels = render_quad_items(
        &device,
        &queue,
        vec![shadow(1, Color::RED), shadow(2, Color::BLUE)],
        Viewport::new(32, 32, 1.0),
        32,
        Some(&[2, 1]),
    );
    let offset = (16 * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + 10 * 4) as usize;
    let overlap = &pixels[offset..offset + 4];

    assert!(
        overlap[2] > overlap[0],
        "later raw red item must paint above lower-slot blue: {overlap:?}"
    );
}

#[test]
fn should_preserve_shadow_fill_external_border_order_across_flushes() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for flush-order test");
        return;
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow flush order target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let pipeline = create_solid_pipeline(&device);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, pass, _| {
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    let shadow = BoxShadow::new()
        .try_color(Color::RED)
        .unwrap()
        .try_spread_radius(4.0)
        .unwrap();
    let decoration = BoxDecoration::new()
        .shadow(shadow)
        .try_color(Color::WHITE)
        .unwrap()
        .border(Border::all(Color::BLUE, 2.0).unwrap());
    let viewport = Viewport::new(32, 32, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Padding::new(8.0, 8.0, 8.0, 8.0)
            .child(DecoratedBox::new(decoration).child(CustomPaint::new(7).handler(handler))),
    );
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow flush order pass"),
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
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    queue.submit(Some(encoder.finish()));
    let pixels = read_texture(&device, &queue, &target);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };

    assert!(
        pixel(5, 16)[2] > 200,
        "shadow must remain below allocation: {:?}",
        pixel(5, 16)
    );
    assert!(
        pixel(8, 16)[0] > 200,
        "border must paint after external: {:?}",
        pixel(8, 16)
    );
    assert!(
        pixel(16, 16)[1] > 200,
        "external must paint after fill: {:?}",
        pixel(16, 16)
    );
}

#[test]
fn should_keep_fractional_shadow_coverage_bounded_and_alpha_scaled_across_dpi() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for fractional shadow test");
        return;
    };

    for scale in [1.0, 1.5, 2.0] {
        let target_size = (32.0 * scale) as u32;
        let shadow = make_rounded_item(
            1,
            Primitive::OuterShadow {
                rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
                shape_rect: Rect::from_min_size(Point::new(8.25, 7.5), Size::new(15.5, 16.0)),
                occluder_rect: Rect::from_min_size(Point::new(8.25, 7.5), Size::new(15.5, 16.0)),
                color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 0.5,
                },
                corner_radii: [6.0, 2.0, 5.0, 1.0],
                occluder_radii: [6.0, 2.0, 5.0, 1.0],
                blur_radius: 3.0,
            },
        );
        let pixels = render_quad_item(
            &device,
            &queue,
            shadow,
            Viewport::new(target_size, target_size, scale),
            target_size,
        );
        let pixel = |x: f32, y: f32| {
            let x = (x * scale).floor() as u32;
            let y = (y * scale).floor() as u32;
            let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
            [
                pixels[offset],
                pixels[offset + 1],
                pixels[offset + 2],
                pixels[offset + 3],
            ]
        };

        // The spread-adjusted shape remains transparent; its exterior keeps
        // the supplied half-alpha at fractional coordinates on every DPI.
        let center = pixel(16.0, 16.0);
        assert!(center[3] <= 1, "scale {scale}: {center:?}");
        let edge = pixel(0.0, 16.0);
        assert!(edge[3] <= 16, "scale {scale}: {edge:?}");
        let exterior = pixel(7.0, 16.0);
        // Bgra8Unorm readback stores this blue source in the first channel.
        assert!(exterior[0] > 0, "scale {scale}: {exterior:?}");
        assert!(
            exterior[3] > 0 && exterior[3] < 160,
            "scale {scale}: {exterior:?}"
        );
    }
}

#[test]
fn should_render_a_visible_hard_edge_for_a_zero_blur_decorated_shadow() {
    // Arrange: the public widget path gives the hard shadow a positive spread,
    // so the portion outside its 16×16 fill must remain visible.
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for decorated hard shadow test");
        return;
    };
    let shadow = BoxShadow::new()
        .try_color(Color::BLUE)
        .unwrap()
        .try_spread_radius(4.0)
        .unwrap();
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        DecoratedBox::new(BoxDecoration::new().shadow(shadow))
            .child(SizedBox::new(Size::new(16.0, 16.0)).color(Color::GREEN)),
    );
    let viewport = Viewport::new(32, 32, 1.0);
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("decorated hard shadow test target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    // Act
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("decorated hard shadow test pass"),
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
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    queue.submit(Some(encoder.finish()));

    // Assert: the fill covers its interior while a zero-blur shadow paints its
    // spread-only exterior, rather than being emitted with no drawable area.
    let pixels = read_texture(&device, &queue, &target);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };
    assert_eq!(pixel(8, 8), &[0, 255, 0, 255]);
    assert!(
        pixel(18, 8)[0] > 200 && pixel(18, 8)[3] > 200,
        "the shadow's hard spread must be visible outside the fill; got {:?}",
        pixel(18, 8)
    );

    // A successfully encoded initial shadow delta is consumed. Its following
    // idle turn must have neither retained scene work nor scheduler demand.
    assert!(
        runtime.pending_delta().is_none(),
        "encoding must consume the initial decorated scene delta"
    );
    let idle = runtime.update(Instant::now());
    assert!(!idle.request_redraw);
    assert!(idle.control_flow.is_none());
}

#[test]
fn should_keep_zero_blur_rounded_shadow_outside_its_rounded_shape() {
    // Arrange: zero blur still receives a one-pixel raster allocation, but
    // its hard edge must be evaluated against the rounded shape rather than
    // filling that allocation as a rectangular halo.
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for hard rounded shadow test");
        return;
    };
    let shadow = make_rounded_item(
        1,
        Primitive::OuterShadow {
            rect: Rect::from_min_size(Point::new(7.0, 7.0), Size::new(18.0, 18.0)),
            shape_rect: Rect::from_min_size(Point::new(8.0, 8.0), Size::new(16.0, 16.0)),
            occluder_rect: Rect::from_min_size(Point::new(10.0, 10.0), Size::new(12.0, 12.0)),
            color: Color::RED,
            corner_radii: [6.0; 4],
            occluder_radii: [4.0; 4],
            blur_radius: 0.0,
        },
    );

    // Act
    let pixels = render_quad_item(&device, &queue, shadow, Viewport::new(32, 32, 1.0), 32);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        [
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        ]
    };

    // Assert: the original box center remains transparent, while the exposed
    // spread-adjusted rounded shape forms a hard outer strip.
    assert!(pixel(16, 16)[3] <= 1, "center: {:?}", pixel(16, 16));
    assert!(
        pixel(7, 7)[3] <= 1,
        "outside shadow shape: {:?}",
        pixel(7, 7)
    );
    assert!(pixel(8, 16)[3] > 200, "spread strip: {:?}", pixel(8, 16));
}

#[test]
fn should_render_rounded_fill_inside_shape_and_leave_corner_outside_shape_empty() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for rounded fill test");
        return;
    };

    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = Viewport::new(32, 32, 1.0);
    let item = make_rounded_item(
        1,
        Primitive::RoundedQuad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            color: Color::RED,
            corner_radii: [10.0; 4],
        },
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![item],
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rounded fill test target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    // Act
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rounded fill test pass"),
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
        renderer.encode(&mut pass);
    }
    queue.submit(Some(encoder.finish()));

    // Assert
    let pixels = read_texture(&device, &queue, &target);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };
    assert_eq!(pixel(16, 16), &[0, 0, 255, 255]);
    assert_eq!(pixel(0, 0), &[0, 0, 0, 0]);
}

#[test]
fn should_render_rounded_border_only_on_outer_ring() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for rounded border test");
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = Viewport::new(32, 32, 1.0);
    let item = make_rounded_item(
        1,
        Primitive::RoundedBorder {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            width: 4.0,
            color: Color::BLUE,
            corner_radii: [10.0; 4],
        },
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![item],
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rounded border test target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    // Act
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rounded border test pass"),
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
        renderer.encode(&mut pass);
    }
    queue.submit(Some(encoder.finish()));

    // Assert
    let pixels = read_texture(&device, &queue, &target);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };
    assert!(pixel(16, 2)[0] > 200);
    assert!(pixel(16, 2)[3] > 200);
    assert_eq!(pixel(16, 16), &[0, 0, 0, 0]);
    assert_eq!(pixel(0, 0), &[0, 0, 0, 0]);
}

#[test]
fn should_render_collapsed_inner_border_as_a_bounded_outer_shape() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for collapsed border test");
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = Viewport::new(32, 32, 1.0);
    let item = make_rounded_item(
        1,
        Primitive::RoundedBorder {
            rect: Rect::from_min_size(Point::new(13.0, 13.0), Size::new(6.0, 6.0)),
            width: 4.0,
            color: Color::BLUE,
            corner_radii: [2.0; 4],
        },
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![item],
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("collapsed border test target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    // Act
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("collapsed border test pass"),
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
        renderer.encode(&mut pass);
    }
    queue.submit(Some(encoder.finish()));

    // Assert
    let pixels = read_texture(&device, &queue, &target);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };
    assert!(
        pixel(16, 16)[0] > 200,
        "collapsed border must cover its center"
    );
    assert!(pixel(16, 16)[3] > 200);
    assert_eq!(pixel(0, 0), &[0, 0, 0, 0]);
}

#[test]
fn rounded_fill_keeps_straight_edges_and_selects_asymmetric_corner_radii() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for asymmetric rounded fill test");
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = Viewport::new(32, 32, 1.0);
    let item = make_rounded_item(
        1,
        Primitive::RoundedQuad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            color: Color::RED,
            // Only the top-left corner is rounded.
            corner_radii: [10.0, 0.0, 0.0, 0.0],
        },
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![item],
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("asymmetric rounded fill test target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("asymmetric rounded fill test pass"),
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
        renderer.encode(&mut pass);
    }
    queue.submit(Some(encoder.finish()));

    let pixels = read_texture(&device, &queue, &target);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };
    assert_eq!(pixel(0, 0), &[0, 0, 0, 0]);
    assert!(pixel(31, 0)[2] > 180 && pixel(31, 0)[3] > 180);
    assert!(pixel(0, 16)[2] > 180 && pixel(0, 16)[3] > 180);
}

#[test]
fn should_return_none_for_unknown_id() {
    let Some((device, _queue)) = try_create_device() else {
        return;
    };
    let renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    assert_eq!(renderer.slot_of(999), None);
    assert_eq!(renderer.slot_of(0), None);
    assert_eq!(renderer.slot_of(1), None);
}

#[test]
fn should_return_correct_slot_after_update() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    let item = make_quad_item(0, 0, 10.0, 20.0, 100.0, 50.0);
    let delta = SceneDelta {
        added: vec![item],
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta, &viewport);

    // First allocated slot is 0
    let slot = renderer.slot_of(delta.added[0].id);
    assert!(
        slot.is_some(),
        "slot_of should return Some for just-added item"
    );
    assert_eq!(slot.unwrap(), 0);
}

#[test]
fn should_return_none_after_item_removed() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    // Add item
    let item = make_quad_item(0, 0, 10.0, 10.0, 50.0, 50.0);
    let delta1 = SceneDelta {
        added: vec![item],
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta1, &viewport);
    let assigned_id = delta1.added[0].id;
    assert!(renderer.slot_of(assigned_id).is_some());

    // Remove it
    let delta2 = SceneDelta {
        added: vec![],
        removed: vec![assigned_id],
        modified: vec![],
    };
    renderer.update(&queue, &delta2, &viewport);
    assert_eq!(renderer.slot_of(assigned_id), None);
}

#[test]
fn should_recycle_free_slots() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    // Add item A (slot 0), then item B (slot 1) — use distinct IDs
    let item_a = make_quad_item(100, 0, 0.0, 0.0, 10.0, 10.0);
    let item_b = make_quad_item(200, 1, 20.0, 0.0, 10.0, 10.0);
    let delta1 = SceneDelta {
        added: vec![item_a, item_b],
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta1, &viewport);
    let id_a = delta1.added[0].id;
    let id_b = delta1.added[1].id;
    assert_eq!(renderer.slot_of(id_a), Some(0));
    assert_eq!(renderer.slot_of(id_b), Some(1));

    // Remove A (slot 0 freed), add C — should reuse slot 0
    let item_c = make_quad_item(300, 0, 30.0, 0.0, 10.0, 10.0);
    let delta2 = SceneDelta {
        added: vec![item_c],
        removed: vec![id_a],
        modified: vec![],
    };
    renderer.update(&queue, &delta2, &viewport);
    let id_c = delta2.added[0].id;
    assert_eq!(renderer.slot_of(id_b), Some(1), "B stays in slot 1");
    assert_eq!(renderer.slot_of(id_c), Some(0), "C reuses freed slot 0");
    assert_eq!(renderer.slot_of(id_a), None, "A is removed");
}

#[test]
fn should_track_multiple_items_at_distinct_slots() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    let items: Vec<SceneItem> = (0..5)
        .map(|i| make_quad_item((i + 10) as u64, i, i as f32 * 50.0, 0.0, 40.0, 30.0))
        .collect();

    let delta = SceneDelta {
        added: items,
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta, &viewport);

    for (i, item) in delta.added.iter().enumerate() {
        assert_eq!(
            renderer.slot_of(item.id),
            Some(i as u32),
            "item {i} should be at slot {i}"
        );
    }
}

#[test]
fn should_return_none_for_id_zero_by_default() {
    let Some((device, _queue)) = try_create_device() else {
        return;
    };
    let renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    // id 0 is the default/unset value; it won't be in the map unless added
    assert_eq!(renderer.slot_of(0), None);
}

#[test]
fn grows_instance_buffer_without_losing_live_slots() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();
    let items = (0..257)
        .map(|index| make_quad_item(index + 1, index as u32, 0.0, 0.0, 10.0, 10.0))
        .collect();
    let delta = SceneDelta {
        added: items,
        removed: vec![],
        modified: vec![],
    };

    renderer.update(&queue, &delta, &viewport);

    assert_eq!(renderer.slot_of(1), Some(0));
    assert_eq!(renderer.slot_of(257), Some(256));
}

#[test]
fn should_not_panic_on_encode_range_with_no_instances() {
    let Some((device, _queue)) = try_create_device() else {
        return;
    };
    let renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        // No instances — should be a no-op, not panic
        renderer.encode_range(&mut pass, 0, 1);
        renderer.encode_range(&mut pass, 0, 0);
        renderer.encode_range(&mut pass, 999, 1);
    }
    encoder.finish();
}

#[test]
fn should_not_panic_on_encode_range_with_instances() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    // Add 3 instances
    let items: Vec<SceneItem> = (0..3)
        .map(|i| make_quad_item(0, i, i as f32 * 100.0, 0.0, 80.0, 40.0))
        .collect();
    let delta = SceneDelta {
        added: items,
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta, &viewport);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);

        // Encode all instances via range
        renderer.encode_range(&mut pass, 0, 3);
        // Encode a subset
        renderer.encode_range(&mut pass, 0, 1);
        renderer.encode_range(&mut pass, 1, 2);
    }
    encoder.finish();
}

#[test]
fn should_clamp_count_to_available_instances() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    // Add 1 instance
    let item = make_quad_item(0, 0, 0.0, 0.0, 100.0, 50.0);
    let delta = SceneDelta {
        added: vec![item],
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta, &viewport);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);

        // count=100 but only 1 instance — should clamp to 1 and not panic
        renderer.encode_range(&mut pass, 0, 100);
        // start=5 >= 1 instances — should early-return and not panic
        renderer.encode_range(&mut pass, 5, 10);
    }
    encoder.finish();
}

#[test]
fn should_encode_full_range_via_encode() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    // Add 2 instances
    let items = vec![
        make_quad_item(0, 0, 0.0, 0.0, 100.0, 50.0),
        make_quad_item(0, 1, 200.0, 0.0, 100.0, 50.0),
    ];
    let delta = SceneDelta {
        added: items,
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta, &viewport);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        // Full encode — should not panic
        renderer.encode(&mut pass);
    }
    encoder.finish();
}

#[test]
fn should_encode_single_instance_with_range() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    let item = make_quad_item(0, 0, 0.0, 0.0, 100.0, 50.0);
    let delta = SceneDelta {
        added: vec![item],
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta, &viewport);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        renderer.encode_range(&mut pass, 0, 1);
    }
    encoder.finish();
}

#[test]
fn should_handle_zero_count_gracefully() {
    let Some((device, queue)) = try_create_device() else {
        return;
    };
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    let item = make_quad_item(0, 0, 0.0, 0.0, 100.0, 50.0);
    let delta = SceneDelta {
        added: vec![item],
        removed: vec![],
        modified: vec![],
    };
    renderer.update(&queue, &delta, &viewport);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        // count=0 should be a no-op
        renderer.encode_range(&mut pass, 0, 0);
        renderer.encode_range(&mut pass, 1, 0);
    }
    encoder.finish();
}

// ── Runtime::encode with External primitives ────────────────────────────────

#[test]
fn should_skip_external_draw_when_scissor_rect_is_empty() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: CustomPaint fills the full viewport; handler must not run at zero size.
    let invoked = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&invoked);
    let handler: Arc<ExternalDrawFn<'static>> =
        Arc::new(move |_draw_id, _context, _gpu, _pass, _mode| {
            observed.store(true, Ordering::SeqCst);
        });
    let viewport = Viewport::new(0, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(CustomPaint::new(99).handler(handler));
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    encoder.finish();

    // Assert
    assert!(!invoked.load(Ordering::SeqCst));
}

#[test]
fn should_invoke_external_draw_with_correct_rect() {
    let Some((device, _queue)) = try_create_device() else {
        return;
    };

    // External rect at logical (10, 20), size (200, 150).
    let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(200.0, 150.0));

    let callback_called = AtomicBool::new(false);

    let external_draw: &harbor_widget::scene::primitive::ExternalDrawFn<'_> =
        &|draw_id, cb_rect, _gpu, _pass, _mode| {
            callback_called.store(true, Ordering::SeqCst);
            assert_eq!(draw_id, 42);
            assert_eq!(cb_rect.logical_rect.min.x, 10.0);
            assert_eq!(cb_rect.logical_rect.min.y, 20.0);
            assert_eq!(cb_rect.logical_rect.size().width, 200.0);
            assert_eq!(cb_rect.logical_rect.size().height, 150.0);
        };

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        let context = ExternalDrawContext::new(rect, Viewport::new(256, 256, 1.0));

        external_draw(
            ExternalDrawId::new(42),
            &context,
            draw_gpu(&device, &_queue),
            &mut pass,
            ExternalDrawMode::Live,
        );
    }
    encoder.finish();

    assert!(callback_called.load(Ordering::SeqCst));
}

#[test]
fn should_invoke_registered_custom_paint_handler_during_runtime_encode() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: a Runtime whose root CustomPaint registers a handler.
    let invoked_draw_id = Arc::new(AtomicU64::new(u64::MAX));
    let observed_draw_id = Arc::clone(&invoked_draw_id);
    let handler: Arc<ExternalDrawFn<'static>> =
        Arc::new(move |draw_id, _context, _gpu, _pass, _mode| {
            observed_draw_id.store(draw_id.get(), Ordering::SeqCst);
        });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(CustomPaint::new(42).handler(handler));
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);

        // Act: encode the External primitive without supplying an external callback.
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    encoder.finish();

    // Assert: Runtime resolved and invoked the handler registered during build.
    assert_eq!(invoked_draw_id.load(Ordering::SeqCst), 42);
}

#[test]
fn should_invoke_each_nested_custom_paint_handler_for_its_draw_id() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: two handlers retained by CustomPaint children built through Stack.
    let first_draw_id = Arc::new(AtomicU64::new(u64::MAX));
    let first_observed = Arc::clone(&first_draw_id);
    let first_handler: Arc<ExternalDrawFn<'static>> =
        Arc::new(move |draw_id, _context, _gpu, _pass, _mode| {
            first_observed.store(draw_id.get(), Ordering::SeqCst);
        });
    let second_draw_id = Arc::new(AtomicU64::new(u64::MAX));
    let second_observed = Arc::clone(&second_draw_id);
    let second_handler: Arc<ExternalDrawFn<'static>> =
        Arc::new(move |draw_id, _context, _gpu, _pass, _mode| {
            second_observed.store(draw_id.get(), Ordering::SeqCst);
        });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .child(CustomPaint::new(1).handler(first_handler))
            .child(CustomPaint::new(2).handler(second_handler)),
    );
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    encoder.finish();

    // Assert: each External primitive resolves its matching retained handler.
    assert_eq!(first_draw_id.load(Ordering::SeqCst), 1);
    assert_eq!(second_draw_id.load(Ordering::SeqCst), 2);
}

#[test]
fn should_preserve_retained_custom_paint_order_and_rects_across_widget_primitives() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: retained CustomPaint handlers are interleaved with real Widget
    // quad primitives. Asymmetric padding gives each callback a distinct exact
    // rectangle inside the viewport-filling Stack.
    let order = Arc::new(Mutex::new(Vec::new()));
    let rects = Arc::new(Mutex::new(Vec::new()));
    let make_handler = |order: Arc<Mutex<Vec<u64>>>, rects: Arc<Mutex<Vec<Rect>>>| {
        Arc::new(
            move |draw_id: ExternalDrawId,
                  context: &ExternalDrawContext,
                  _gpu: ExternalDrawGpu<'_>,
                  _pass: &mut wgpu::RenderPass<'_>,
                  _mode| {
                order.lock().unwrap().push(draw_id.get());
                rects.lock().unwrap().push(context.logical_rect);
            },
        ) as Arc<ExternalDrawFn<'static>>
    };
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .child(SizedBox::new(Size::new(24.0, 24.0)).color(Color::RED))
            .child(
                Padding::new(4.0, 8.0, 8.0, 4.0)
                    .child(CustomPaint::new(1).handler(make_handler(order.clone(), rects.clone()))),
            )
            .child(SizedBox::new(Size::new(24.0, 24.0)).color(Color::GREEN))
            .child(
                Padding::new(8.0, 4.0, 4.0, 8.0)
                    .child(CustomPaint::new(2).handler(make_handler(order.clone(), rects.clone()))),
            )
            .child(SizedBox::new(Size::new(24.0, 24.0)).color(Color::BLUE))
            .child(
                Padding::new(12.0, 0.0, 0.0, 12.0)
                    .child(CustomPaint::new(3).handler(make_handler(order.clone(), rects.clone()))),
            ),
    );
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    // Act: encode the retained scene in one render pass.
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    encoder.finish();

    // Assert: callbacks retain paint order and receive their exact rectangles.
    assert_eq!(*order.lock().unwrap(), vec![1, 2, 3]);
    assert_eq!(
        *rects.lock().unwrap(),
        vec![
            Rect::from_min_size(Point::new(4.0, 4.0), Size::new(244.0, 244.0)),
            Rect::from_min_size(Point::new(8.0, 8.0), Size::new(244.0, 244.0)),
            Rect::from_min_size(Point::new(12.0, 12.0), Size::new(244.0, 244.0)),
        ]
    );
}

#[test]
fn should_clip_custom_paint_and_restore_scissor_for_following_widget_primitive() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: draw a full-target triangle through a CustomPaint padded into a 16×16 region.
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("custom-paint clipping test target"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let pipeline = create_solid_pipeline(&device);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, pass, _| {
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    let viewport = Viewport::new(32, 32, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .child(SizedBox::new(Size::new(32.0, 32.0)))
            .child(Padding::new(8.0, 8.0, 8.0, 8.0).child(CustomPaint::new(1).handler(handler)))
            .child(SizedBox::new(Size::new(8.0, 8.0)).color(Color::RED)),
    );
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    // Act: encode the full-target external draw through Runtime's scissor boundary.
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("custom-paint clipping test pass"),
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
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    queue.submit(Some(encoder.finish()));

    // Assert: the external draw is clipped to 8..24, and the following quad
    // paints at the origin after Runtime restores the full viewport scissor.
    let pixels = read_texture(&device, &queue, &target);
    let pixel = |x: u32, y: u32| {
        let offset = (y * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT + x * 4) as usize;
        &pixels[offset..offset + 4]
    };
    assert_eq!(pixel(16, 16), &[0, 255, 0, 255]);
    assert_eq!(pixel(4, 4), &[0, 0, 255, 255]);
    assert_eq!(pixel(24, 16), &[0, 0, 0, 0]);
}

#[test]
fn should_use_last_registered_handler_when_custom_paints_share_draw_id() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: two CustomPaint children register distinct handlers for one draw ID.
    let first_calls = Arc::new(AtomicU64::new(0));
    let first_observed = Arc::clone(&first_calls);
    let first_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, _| {
        first_observed.fetch_add(1, Ordering::SeqCst);
    });
    let last_calls = Arc::new(AtomicU64::new(0));
    let last_observed = Arc::clone(&last_calls);
    let last_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, _| {
        last_observed.fetch_add(1, Ordering::SeqCst);
    });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .child(CustomPaint::new(42).handler(first_handler))
            .child(CustomPaint::new(42).handler(last_handler)),
    );
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    // Act: encode both External primitives sharing the same draw ID.
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    encoder.finish();

    // Assert: HashMap registration semantics retain only the final handler.
    assert_eq!(first_calls.load(Ordering::SeqCst), 0);
    assert_eq!(last_calls.load(Ordering::SeqCst), 2);
}

#[test]
fn should_remove_handler_when_custom_paint_is_replaced() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: a Runtime with an initially registered CustomPaint handler.
    let calls = Arc::new(AtomicU64::new(0));
    let observed_calls = Arc::clone(&calls);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, _| {
        observed_calls.fetch_add(1, Ordering::SeqCst);
    });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(CustomPaint::new(42).handler(handler));
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    let mut initial_encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut initial_encoder);
        runtime.encode(
            draw_gpu(&device, &queue),
            &mut pass,
            viewport.clone(),
            false,
        );
    }
    initial_encoder.finish();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Act: replace the CustomPaint with an unregistered instance using the same draw ID.
    runtime.set_root(CustomPaint::new(42));
    runtime.update(Instant::now());
    let mut replacement_encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut replacement_encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    replacement_encoder.finish();

    // Assert: the removed CustomPaint's handler is not retained across rebuilds.
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn should_skip_external_primitive_when_custom_paint_has_no_handler() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange: a Runtime whose CustomPaint has the default, handler-free configuration.
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(CustomPaint::new(42));
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);

        // Act: encode the External primitive without a registered handler.
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }

    // Assert: the unhandled External primitive is skipped without a validation error or panic.
    encoder.finish();
}

#[test]
fn should_compute_scissor_at_1x_scale() {
    let viewport = Viewport::new(800, 600, 1.0);
    let rect = Rect::from_min_size(Point::new(200.0, 150.0), Size::new(400.0, 300.0));

    let phys_x = (rect.min.x * viewport.scale_factor) as u32;
    let phys_y = (rect.min.y * viewport.scale_factor) as u32;
    let phys_w = ((rect.size().width * viewport.scale_factor).ceil() as u32).max(1);
    let phys_h = ((rect.size().height * viewport.scale_factor).ceil() as u32).max(1);

    assert_eq!(phys_x, 200);
    assert_eq!(phys_y, 150);
    assert_eq!(phys_w, 400);
    assert_eq!(phys_h, 300);
}

#[test]
fn should_pass_retain_or_live_mode_from_eligibility_and_commit() {
    use harbor_widget::scene::primitive::{ExternalScheduleDemand, ExternalScheduleFn};

    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    let mode = Arc::new(Mutex::new(None));
    let observed = Arc::clone(&mode);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, draw_mode| {
        *observed.lock().unwrap() = Some(draw_mode);
    });
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(CustomPaint::new(1).handler(handler).schedule(schedule));
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(
            draw_gpu(&device, &queue),
            &mut pass,
            viewport.clone(),
            false,
        );
    }
    encoder.finish();
    assert_eq!(*mode.lock().unwrap(), Some(ExternalDrawMode::Retain));

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, true);
    }
    encoder.finish();
    assert_eq!(*mode.lock().unwrap(), Some(ExternalDrawMode::Live));
}

#[test]
fn should_select_live_when_encode_has_no_deferred_ids() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange — no schedule collect, so missing ids default to Live
    let mode = Arc::new(Mutex::new(None));
    let observed = Arc::clone(&mode);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, draw_mode| {
        *observed.lock().unwrap() = Some(draw_mode);
    });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(CustomPaint::new(1).handler(handler));
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    // Act
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    encoder.finish();

    // Assert
    assert_eq!(*mode.lock().unwrap(), Some(ExternalDrawMode::Live));
}

#[test]
fn should_select_distinct_modes_when_two_externals_differ_in_eligibility() {
    use harbor_widget::scene::primitive::{ExternalScheduleDemand, ExternalScheduleFn};

    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Runtime encode test");
        return;
    };

    // Arrange
    let live_mode = Arc::new(Mutex::new(None));
    let retain_mode = Arc::new(Mutex::new(None));
    let live_observed = Arc::clone(&live_mode);
    let retain_observed = Arc::clone(&retain_mode);
    let live_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, draw_mode| {
        *live_observed.lock().unwrap() = Some(draw_mode);
    });
    let retain_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, draw_mode| {
        *retain_observed.lock().unwrap() = Some(draw_mode);
    });
    let eligible: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
    let deferred: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .child(CustomPaint::new(1).handler(live_handler).schedule(eligible))
            .child(
                CustomPaint::new(2)
                    .handler(retain_handler)
                    .schedule(deferred),
            ),
    );
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    // Act
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    encoder.finish();

    // Assert
    assert_eq!(*live_mode.lock().unwrap(), Some(ExternalDrawMode::Live));
    assert_eq!(*retain_mode.lock().unwrap(), Some(ExternalDrawMode::Retain));
}

#[test]
fn should_discard_child_pixels_outside_arc_when_clip_is_hard_edge() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for hard-edge clip test");
        return;
    };
    let item = clipped_child_quad(ClipBehavior::HardEdge, Point::ZERO, 32.0, 8.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 1, 1), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
    assert!(
        bgra(&pixels, 16, 0)[2] > 200,
        "straight top edge must remain covered: {:?}",
        bgra(&pixels, 16, 0)
    );
}

#[test]
fn should_apply_fractional_coverage_when_clip_is_anti_alias() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for anti-alias clip test");
        return;
    };
    let item = clipped_child_quad(ClipBehavior::AntiAlias, Point::ZERO, 32.0, 8.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    let mut saw_fractional = false;
    for y in 0..8 {
        for x in 0..8 {
            let alpha = bgra(&pixels, x, y)[3];
            if alpha > 0 && alpha < 255 {
                saw_fractional = true;
            }
        }
    }
    assert!(
        saw_fractional,
        "anti-aliased clip must produce fractional coverage along the arc"
    );
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

#[test]
fn should_clip_child_quad_when_viewport_scale_is_one_and_a_half() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for 1.5x clip test");
        return;
    };
    let item = clipped_child_quad(ClipBehavior::HardEdge, Point::new(0.25, 0.25), 20.0, 8.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(48, 48, 1.5), 48);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert!(
        bgra(&pixels, 16, 16)[2] > 200,
        "interior at 1.5x must keep child color: {:?}",
        bgra(&pixels, 16, 16)
    );
}

#[test]
fn should_clip_child_quad_when_viewport_scale_is_two() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for 2x clip test");
        return;
    };
    let item = clipped_child_quad(ClipBehavior::HardEdge, Point::new(0.5, 0.5), 16.0, 6.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(64, 64, 2.0), 64);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert!(
        bgra(&pixels, 16, 16)[2] > 200,
        "interior at 2x must keep child color: {:?}",
        bgra(&pixels, 16, 16)
    );
}

#[test]
fn should_keep_unclipped_wrapper_pixels_in_cutout_when_child_clip_is_active() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for wrapper cutout test");
        return;
    };
    let fill = SceneItem {
        id: 1,
        primitive: Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            color: Color::BLUE,
            corner_radius: 0.0,
        },
        clips: Vec::new(),
        paint_order: 0,
    };
    let mut child = make_quad_item(2, 1, 0.0, 0.0, 32.0, 32.0);
    child.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            BorderRadius::all(8.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );

    // Act
    let pixels = render_quad_items(
        &device,
        &queue,
        vec![fill, child],
        Viewport::new(32, 32, 1.0),
        32,
        None,
    );

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [255, 0, 0, 255]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

#[test]
fn should_intersect_nested_clips_when_two_rounded_ancestors_are_active() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for nested clip test");
        return;
    };
    let mut item = make_quad_item(1, 0, 0.0, 0.0, 32.0, 32.0);
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            BorderRadius::all(8.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::new(4.0, 4.0), Size::new(24.0, 24.0)),
            BorderRadius::all(4.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    assert_eq!(bgra(&pixels, 4, 4), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

#[test]
fn should_not_leak_outside_outer_bound_when_more_than_two_clips_are_packed() {
    // Arrange
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for packed clip test");
        return;
    };
    let mut item = make_quad_item(1, 0, 0.0, 0.0, 32.0, 32.0);
    for (origin, size, radius) in [
        (Point::ZERO, 32.0, 8.0),
        (Point::new(2.0, 2.0), 28.0, 4.0),
        (Point::new(4.0, 4.0), 24.0, 4.0),
    ] {
        item.clips.push(
            RoundedClip::new(
                Rect::from_min_size(origin, Size::new(size, size)),
                BorderRadius::all(radius).unwrap(),
                ClipBehavior::HardEdge,
            )
            .unwrap(),
        );
    }

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 1, 16), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

fn encode_root_to_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    root: impl harbor_widget::view::Component + 'static,
    viewport: Viewport,
    size: u32,
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("custom-paint rounded clip target"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
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

fn solid_external(device: &wgpu::Device, draw_id: u64) -> CustomPaint {
    solid_external_rgb(device, draw_id, 0.0, 1.0, 0.0)
}

fn solid_external_rgb(device: &wgpu::Device, draw_id: u64, r: f32, g: f32, b: f32) -> CustomPaint {
    let pipeline = create_solid_pipeline_rgb(device, r, g, b);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, pass, _| {
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    CustomPaint::new(draw_id).handler(handler)
}

fn clipped_external(device: &wgpu::Device, behavior: ClipBehavior, radius: f32) -> DecoratedBox {
    DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(radius).unwrap()))
        .clip_behavior(behavior)
        .child(solid_external(device, 1))
}

#[test]
fn should_leave_custom_paint_corners_when_clip_policy_is_none() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        clipped_external(&device, ClipBehavior::None, 8.0),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 255, 0, 255]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
fn should_discard_custom_paint_pixels_outside_arc_when_clip_is_hard_edge() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        clipped_external(&device, ClipBehavior::HardEdge, 8.0),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 1, 1), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
    assert!(
        bgra(&pixels, 16, 0)[1] > 200,
        "straight top edge must remain covered: {:?}",
        bgra(&pixels, 16, 0)
    );
}

#[test]
fn should_apply_fractional_coverage_when_custom_paint_clip_is_anti_alias() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        clipped_external(&device, ClipBehavior::AntiAlias, 8.0),
        Viewport::new(32, 32, 1.0),
        32,
    );
    let mut saw_fractional = false;
    for y in 0..8 {
        for x in 0..8 {
            let alpha = bgra(&pixels, x, y)[3];
            if alpha > 0 && alpha < 255 {
                saw_fractional = true;
            }
        }
    }
    assert!(
        saw_fractional,
        "anti-aliased external clip must produce fractional coverage along the arc"
    );
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
fn should_keep_custom_paint_callback_geometry_when_rounded_clip_is_active() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let observed_id = Arc::new(AtomicU64::new(u64::MAX));
    let observed_rect = Arc::new(Mutex::new(None));
    let observed_mode = Arc::new(Mutex::new(None));
    let id = Arc::clone(&observed_id);
    let rects = Arc::clone(&observed_rect);
    let modes = Arc::clone(&observed_mode);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |draw_id, context, gpu, _, mode| {
        assert_eq!(gpu.target_format(), wgpu::TextureFormat::Bgra8Unorm);
        id.store(draw_id.get(), Ordering::SeqCst);
        *rects.lock().unwrap() = Some(context.logical_rect);
        *modes.lock().unwrap() = Some(mode);
        assert_eq!(context.surface_size(), (32, 32));
        assert_eq!(context.scale_factor(), 1.0);
    });
    encode_root_to_pixels(
        &device,
        &queue,
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(ClipBehavior::HardEdge)
            .child(CustomPaint::new(7).handler(handler)),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(observed_id.load(Ordering::SeqCst), 7);
    assert_eq!(
        *observed_rect.lock().unwrap(),
        Some(Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)))
    );
    assert_eq!(*observed_mode.lock().unwrap(), Some(ExternalDrawMode::Live));
}

#[test]
fn should_not_punch_following_widget_outside_custom_paint_allocation() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .child(SizedBox::new(Size::new(8.0, 8.0)).color(Color::RED))
            .child(Padding::new(8.0, 8.0, 8.0, 8.0).child(clipped_external(
                &device,
                ClipBehavior::HardEdge,
                4.0,
            ))),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 255, 255]);
    assert_eq!(bgra(&pixels, 8, 8), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
fn should_clip_nested_custom_paint_to_outer_rounded_bound() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(ClipBehavior::HardEdge)
            .child(
                Padding::all(4.0).child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external(&device, 1)),
                ),
            ),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
fn should_clip_custom_paint_when_viewport_scale_is_two() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        clipped_external(&device, ClipBehavior::HardEdge, 8.0),
        Viewport::new(64, 64, 2.0),
        64,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 32, 32), [0, 255, 0, 255]);
}

#[test]
fn should_mask_custom_paint_corners_when_encode_mode_is_retain() {
    use harbor_widget::scene::primitive::{ExternalScheduleDemand, ExternalScheduleFn};

    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pipeline = create_solid_pipeline(&device);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, pass, mode| {
        assert_eq!(mode, ExternalDrawMode::Retain);
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
        ordinary_present_eligible: false,
        ..ExternalScheduleDemand::empty()
    });
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(ClipBehavior::HardEdge)
            .child(CustomPaint::new(1).handler(handler).schedule(schedule)),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
fn should_apply_independent_masks_when_two_clipped_custom_paints_share_a_pass() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .child(
                Padding::new(0.0, 16.0, 0.0, 0.0).child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external(&device, 1)),
                ),
            )
            .child(
                Padding::new(0.0, 0.0, 0.0, 16.0).child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external_rgb(&device, 2, 1.0, 0.0, 0.0)),
                ),
            ),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 31, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 8, 16), [0, 255, 0, 255]);
    assert_eq!(bgra(&pixels, 24, 16), [0, 0, 255, 255]);
}

#[test]
fn should_dest_in_underlapping_pixels_inside_custom_paint_allocation_aabb() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .child(SizedBox::new(Size::new(32.0, 32.0)).color(Color::RED))
            .child(clipped_external(&device, ClipBehavior::HardEdge, 8.0)),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
fn should_clip_custom_paint_when_viewport_scale_is_one_and_a_half() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        clipped_external(&device, ClipBehavior::HardEdge, 8.0),
        Viewport::new(48, 48, 1.5),
        48,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 24, 24), [0, 255, 0, 255]);
}

#[test]
fn should_keep_independent_masks_when_more_than_eight_externals_share_a_pass() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let mut root = Stack::new();
    for row in 0..3 {
        for col in 0..3 {
            let index = row * 3 + col;
            let (r, g, b) = match index {
                0 => (0.0, 1.0, 0.0),
                8 => (1.0, 0.0, 0.0),
                _ => (0.0, 0.0, 1.0),
            };
            root = root.child(
                Padding::new(
                    row as f32 * 16.0,
                    (2 - col) as f32 * 16.0,
                    (2 - row) as f32 * 16.0,
                    col as f32 * 16.0,
                )
                .child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external_rgb(
                        &device,
                        index as u64 + 1,
                        r,
                        g,
                        b,
                    )),
                ),
            );
        }
    }
    let pixels = encode_root_to_pixels(&device, &queue, root, Viewport::new(48, 48, 1.0), 48);
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 8, 8), [0, 255, 0, 255]);
    assert_eq!(bgra(&pixels, 32, 32), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 40, 40), [0, 0, 255, 255]);
}

#[test]
fn should_apply_later_mask_slot_when_unmasked_external_is_between() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .child(
                Padding::new(0.0, 22.0, 0.0, 0.0).child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external(&device, 1)),
                ),
            )
            .child(
                Padding::new(0.0, 11.0, 0.0, 11.0)
                    .child(solid_external_rgb(&device, 2, 0.0, 0.0, 1.0)),
            )
            .child(
                Padding::new(0.0, 0.0, 0.0, 22.0).child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external_rgb(&device, 3, 1.0, 0.0, 0.0)),
                ),
            ),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 5, 16), [0, 255, 0, 255]);
    assert_eq!(bgra(&pixels, 11, 0), [255, 0, 0, 255]);
    assert_eq!(bgra(&pixels, 31, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 27, 16), [0, 0, 255, 255]);
}

#[test]
fn should_keep_rounded_mask_when_handler_rewrites_scissor_from_context() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for custom-paint clip test");
        return;
    };
    let pipeline = create_solid_pipeline(&device);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, context, _, pass, _| {
        let scissor = context.scissor_rect();
        pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(ClipBehavior::HardEdge)
            .child(CustomPaint::new(1).handler(handler)),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
fn lucide_icons_render_at_multiple_dpi_with_tint_and_single_coverage() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Lucide icon test");
        return;
    };
    eprintln!("GPU available: executing Lucide pixel assertions");
    use harbor_widget::LucideIcon;
    for scale in [1.0, 1.5, 2.0] {
        let target_size = (32.0 * scale) as u32;
        for (icon, sample) in [
            (LucideIcon::Terminal, (16.0, 19.0)),
            (LucideIcon::Plus, (12.0, 12.0)),
            (LucideIcon::X, (12.0, 12.0)),
            (LucideIcon::Dot, (12.0, 12.0)),
            (LucideIcon::ChevronUp, (12.0, 9.0)),
            (LucideIcon::ChevronDown, (12.0, 15.0)),
        ] {
            let item = make_rounded_item(
                1,
                Primitive::Icon {
                    icon,
                    rect: Rect::from_min_size(Point::new(4.0, 4.0), Size::new(24.0, 24.0)),
                    color: Color {
                        a: 0.5,
                        ..Color::RED
                    },
                },
            );
            let pixels = render_quad_item(
                &device,
                &queue,
                item,
                Viewport::new(target_size, target_size, scale),
                target_size,
            );
            let sample = bgra(
                &pixels,
                ((sample.0 + 4.0) * scale) as u32,
                ((sample.1 + 4.0) * scale) as u32,
            );
            assert!(
                sample[2] > 80 && (90..=130).contains(&sample[3]),
                "{icon:?} at {scale}x: {sample:?}"
            );
            assert_eq!(
                bgra(&pixels, (5.0 * scale) as u32, (5.0 * scale) as u32),
                [0; 4]
            );
        }
    }
}

#[test]
fn lucide_icon_uses_runtime_frame_encoding_and_ancestor_clip() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Lucide runtime clip test");
        return;
    };
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        harbor_widget::Icon::new(harbor_widget::LucideIcon::Plus)
            .size(24.0)
            .color(Color::GREEN),
        Viewport::new(24, 24, 1.0),
        24,
    );
    assert!(bgra(&pixels, 12, 12)[1] > 200);
    assert_eq!(bgra(&pixels, 2, 2), [0; 4]);

    let mut item = make_rounded_item(
        1,
        Primitive::Icon {
            icon: harbor_widget::LucideIcon::Plus,
            rect: Rect::from_min_size(Point::ZERO, Size::new(24.0, 24.0)),
            color: Color::GREEN,
        },
    );
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::new(12.0, 0.0), Size::new(12.0, 24.0)),
            BorderRadius::default(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);
    assert_eq!(bgra(&pixels, 8, 12), [0; 4]);
    assert!(bgra(&pixels, 16, 12)[1] > 200);
}

#[test]
fn lucide_icon_updates_reuse_slots_and_switch_safely_between_primitive_types() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Lucide retention test");
        return;
    };
    let viewport = Viewport::new(32, 32, 1.0);
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let mut item = make_rounded_item(
        7,
        Primitive::Icon {
            icon: harbor_widget::LucideIcon::Plus,
            rect: Rect::from_min_size(Point::ZERO, Size::new(24.0, 24.0)),
            color: Color::WHITE,
        },
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![item.clone()],
            modified: vec![],
            removed: vec![],
        },
        &viewport,
    );
    let slot = renderer.slot_of(7).unwrap();
    item.primitive = Primitive::Icon {
        icon: harbor_widget::LucideIcon::X,
        rect: Rect::from_min_size(Point::ZERO, Size::new(16.0, 16.0)),
        color: Color::RED,
    };
    renderer.update(
        &queue,
        &SceneDelta {
            modified: vec![item.clone()],
            added: vec![],
            removed: vec![],
        },
        &viewport,
    );
    assert_eq!(renderer.slot_of(7), Some(slot));
    renderer.refresh_viewport(&queue, &[item.clone()], &Viewport::new(64, 64, 2.0));
    item.primitive = Primitive::Text {
        text: Arc::from("label"),
        origin: Point::ZERO,
        color: Color::WHITE,
    };
    renderer.update(
        &queue,
        &SceneDelta {
            modified: vec![item],
            added: vec![],
            removed: vec![],
        },
        &viewport,
    );
    assert_eq!(renderer.slot_of(7), None);
    renderer.update(
        &queue,
        &SceneDelta {
            removed: vec![7],
            added: vec![],
            modified: vec![],
        },
        &viewport,
    );
}

#[test]
fn lucide_axis_strokes_match_pixel_coverage_without_a_blur_halo() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for Lucide sharpness test");
        return;
    };
    for side in [24.0, 20.0, 16.0] {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for phase in [0.0, 0.25, 0.5] {
                let origin = 4.0 + phase;
                let target_size = (32.0 * scale) as u32;
                let item = make_rounded_item(
                    1,
                    Primitive::Icon {
                        icon: harbor_widget::LucideIcon::Plus,
                        rect: Rect::from_min_size(
                            Point::new(origin, origin),
                            Size::new(side, side),
                        ),
                        color: Color::WHITE,
                    },
                );
                let pixels = render_quad_item(
                    &device,
                    &queue,
                    item,
                    Viewport::new(target_size, target_size, scale),
                    target_size,
                );
                // Sample away from caps and the crossing. A 2/24-wide SVG stroke
                // must cover exactly its geometric intersection with each pixel.
                let x = ((origin + side * 2.0 / 3.0) * scale) as u32;
                let center = (origin + side / 2.0) * scale;
                let half_width = side * scale / 24.0;
                let lo = center - half_width;
                let hi = center + half_width;
                for y in (lo.floor() as u32 - 1)..=(hi.ceil() as u32 + 1) {
                    let expected = ((hi.min(y as f32 + 1.0) - lo.max(y as f32)).clamp(0.0, 1.0)
                        * 255.0)
                        .round() as i32;
                    let actual = i32::from(bgra(&pixels, x, y)[3]);
                    assert!(
                        (actual - expected).abs() <= 4,
                        "side={side}, scale={scale}, phase={phase}, pixel=({x},{y}): alpha={actual}, expected={expected}"
                    );
                }
            }
        }
    }
}

// The coordinate ramp makes displacement observable independently of tint or
// highlight painting. These are real encoder readbacks, not shader math mocks.
#[test]
fn backdrop_refraction_moves_edges_and_corners_but_not_center_outside_or_foreground() {
    use harbor_widget::{BackdropFilter, EdgeRefraction, GradientStop, LinearGradient};
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for edge refraction");
        return;
    };
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    for scale in [1.0, 1.5, 2.0] {
        let side = (32.0 * scale) as u32;
        let viewport = Viewport::new(side, side, scale);
        runtime.set_viewport(viewport.clone());
        for sigma in [0.0, 2.0, 8.0] {
            for alpha in [0.5, 1.0] {
                let root = |displacement, flipped| {
                    let start = Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: alpha,
                    };
                    let end = Color {
                        a: alpha,
                        ..Color::WHITE
                    };
                    let (start, end) = if flipped { (end, start) } else { (start, end) };
                    let ramp = LinearGradient::new(
                        Point::ZERO,
                        Point::new(1.0, 1.0),
                        &[GradientStop::new(0.0, start), GradientStop::new(1.0, end)],
                    )
                    .unwrap();
                    Stack::new()
                        .child(
                            DecoratedBox::new(BoxDecoration::new().try_paint(ramp.into()).unwrap())
                                .child(SizedBox::new(Size::new(32.0, 32.0))),
                        )
                        .child(
                            Padding::all(4.0).child(
                                BackdropFilter::new(sigma)
                                    .unwrap()
                                    .edge_refraction(
                                        EdgeRefraction::new(5.0, displacement).unwrap(),
                                    )
                                    .border_radius(BorderRadius::all(6.0).unwrap())
                                    .child(SizedBox::new(Size::new(24.0, 24.0))),
                            ),
                        )
                        // A sharp foreground marker lies directly over the edge band.
                        .child(
                            Padding::new(4.0, 0.0, 0.0, 14.0)
                                .child(SizedBox::new(Size::new(4.0, 2.0)).color(Color::RED)),
                        )
                };
                runtime.set_root(root(0.0, false));
                runtime.update(Instant::now());
                let baseline = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                runtime.set_root(root(3.0, false));
                runtime.update(Instant::now());
                let refracted =
                    encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                let at = |pixels: &[u8], x: f32, y: f32| {
                    bgra(pixels, (x * scale) as u32, (y * scale) as u32)
                };
                for (x, y, direction) in [
                    (5.0, 16.0, 1),
                    (26.0, 16.0, -1),
                    (12.0, 5.0, 1),
                    (16.0, 26.0, -1),
                    (7.0, 7.0, 1),
                ] {
                    let before = at(&baseline, x, y);
                    let after = at(&refracted, x, y);
                    assert!(
                        (i32::from(after[0]) - i32::from(before[0])) * direction > 1,
                        "edge/corner displacement at ({x},{y}), scale={scale}, sigma={sigma}, alpha={alpha}: {before:?} -> {after:?}"
                    );
                    // Resampling can move the backdrop's antialiased outer alpha
                    // ramp; it must not add opacity or break premultiplication.
                    assert!(f32::from(after[3]) <= (alpha * 255.0).round() + 1.0);
                    assert!(after[..3].iter().all(|channel| *channel <= after[3]));
                }
                for (x, y) in [(16.0, 16.0), (2.0, 16.0), (4.0, 4.0), (15.0, 5.0)] {
                    let before = at(&baseline, x, y);
                    let after = at(&refracted, x, y);
                    assert!(
                        before
                            .iter()
                            .zip(after)
                            .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= 1),
                        "center/outside/corner-cutout/foreground unchanged ({x},{y}): {before:?} -> {after:?}"
                    );
                }
                let retained = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                assert_eq!(
                    refracted, retained,
                    "retained frames do not compound distortion"
                );
                runtime.set_root(root(3.0, true));
                runtime.update(Instant::now());
                let live = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                assert_ne!(
                    at(&live, 7.0, 7.0),
                    at(&refracted, 7.0, 7.0),
                    "live background updates"
                );
                assert_eq!(
                    at(&live, 15.0, 5.0),
                    at(&refracted, 15.0, 5.0),
                    "foreground remains sharp"
                );
            }
        }
    }
}

// Encoder-level filter tests deliberately use the public non-terminal widget API.
fn encode_filter_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    runtime: &mut Runtime,
    viewport: Viewport,
) -> Vec<u8> {
    let size = viewport.physical_size.0;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("backdrop pixel evidence"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    runtime.encode_frame(
        draw_gpu(device, queue),
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

#[test]
fn backdrop_large_sigma_softens_a_thin_line_without_a_sparse_sampling_comb() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for blur sampling quality");
        return;
    };
    for scale in [1.0, 1.5, 2.0] {
        let side = (32.0 * scale) as u32;
        let viewport = Viewport::new(side, side, scale);
        let mut runtime = Runtime::new();
        runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
        let stripe = || {
            Padding::new(0.0, 0.0, 0.0, 16.0)
                .child(SizedBox::new(Size::new(1.0, 32.0)).color(Color::WHITE))
        };
        runtime.set_viewport(viewport.clone());
        runtime.set_root(Stack::new().child(stripe()));
        runtime.update(Instant::now());
        let source_pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        // Account for the source shape's DPI-dependent edge coverage rather
        // than assuming a 1dp shape covers an integer number of physical pixels.
        let source_energy: u32 = (0..side)
            .map(|x| u32::from(bgra(&source_pixels, x, side / 2)[3]))
            .sum();
        let expected_peak =
            source_energy as f32 / ((2.0 * std::f32::consts::PI).sqrt() * 8.0 * scale);
        runtime.set_root(
            Stack::new().child(stripe()).child(
                harbor_widget::BackdropFilter::new(8.0)
                    .unwrap()
                    .child(SizedBox::new(Size::new(32.0, 32.0))),
            ),
        );
        runtime.update(Instant::now());
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport);
        let center = (16.5 * scale) as u32;
        let profile: Vec<_> = (0..side).map(|x| bgra(&pixels, x, side / 2)[3]).collect();
        let peak = *profile.iter().max().unwrap();
        assert!(
            (f32::from(peak) - expected_peak).abs() <= 2.0,
            "sigma=8 should spread a thin line, not repeat sparse taps at {scale}x: expected peak={expected_peak}, {profile:?}"
        );
        for x in 2..center {
            assert!(
                profile[x as usize] <= profile[x as usize + 1] + 1,
                "left Gaussian profile must be smooth: {profile:?}"
            );
        }
        for x in center + 1..side - 2 {
            assert!(
                profile[x as usize + 1] <= profile[x as usize] + 1,
                "right Gaussian profile must be smooth: {profile:?}"
            );
        }
    }
}

#[test]
fn backdrop_blurs_external_pixels_once_per_frame_and_keeps_foreground_sharp() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for backdrop pixel readback");
        return;
    };
    for displacement in [0.0, 1.0] {
        let green = create_solid_pipeline_rgb(&device, 0.0, 1.0, 0.0);
        let blue = create_solid_pipeline_rgb(&device, 0.0, 0.0, 1.0);
        let red = create_solid_pipeline_rgb(&device, 1.0, 0.0, 0.0);
        for scale in [1.0, 1.5, 2.0] {
            let calls = Arc::new(AtomicU64::new(0));
            let changed = Arc::new(AtomicBool::new(false));
            let observed = calls.clone();
            let flip = changed.clone();
            let (green, blue, red) = (green.clone(), blue.clone(), red.clone());
            let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, pass, _| {
                observed.fetch_add(1, Ordering::SeqCst);
                let size = (32.0 * scale) as u32;
                pass.set_scissor_rect(0, 0, size / 2, size);
                pass.set_pipeline(&green);
                pass.draw(0..3, 0..1);
                pass.set_scissor_rect(size / 2, 0, size / 2, size);
                pass.set_pipeline(if flip.load(Ordering::SeqCst) {
                    &red
                } else {
                    &blue
                });
                pass.draw(0..3, 0..1);
            });
            let side = (32.0 * scale) as u32;
            let viewport = Viewport::new(side, side, scale);
            let mut runtime = Runtime::new();
            runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
            runtime.set_root(
                Stack::new()
                    .child(CustomPaint::new(1).handler(handler))
                    .child(
                        Padding::all(4.0).child(
                            harbor_widget::BackdropFilter::new(2.0)
                                .unwrap()
                                .edge_refraction(
                                    harbor_widget::EdgeRefraction::new(4.0, displacement).unwrap(),
                                )
                                .border_radius(BorderRadius::all(6.0).unwrap())
                                .child(SizedBox::new(Size::new(24.0, 24.0))),
                        ),
                    )
                    .child(
                        Padding::all(13.0)
                            .child(SizedBox::new(Size::new(6.0, 6.0)).color(Color::RED)),
                    ),
            );
            runtime.set_viewport(viewport.clone());
            runtime.update(Instant::now());
            let first = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
            let at = |pixels: &[u8], x: f32, y: f32| {
                bgra(pixels, (x * scale) as u32, (y * scale) as u32)
            };
            let mixed = at(&first, 15.0, 10.0);
            assert!(
                mixed[0] > 20 && mixed[1] > 80,
                "real background blur at {scale}x: {mixed:?}"
            );
            assert_eq!(
                at(&first, 2.0, 10.0),
                [0, 255, 0, 255],
                "outside filter unchanged"
            );
            assert_eq!(
                at(&first, 4.0, 4.0),
                [0, 255, 0, 255],
                "rounded corner unchanged"
            );
            assert_eq!(
                at(&first, 16.0, 16.0),
                [0, 0, 255, 255],
                "foreground stays sharp"
            );
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "no replay for background capture"
            );
            assert!(runtime.pending_delta().is_none());
            let retained = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
            assert_eq!(
                first, retained,
                "unchanged scene survives retained encoding"
            );
            changed.store(true, Ordering::SeqCst);
            let live = encode_filter_frame(&device, &queue, &mut runtime, viewport);
            let updated = at(&live, 15.0, 10.0);
            assert_eq!(updated[0], 0);
            assert!(updated[2] > 20, "live external output appears through blur");
            assert_eq!(calls.load(Ordering::SeqCst), 3);
        }
    }
}

#[test]
fn backdrop_generic_composition_preserves_alpha_clips_and_resize() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for backdrop alpha/resize");
        return;
    };
    for displacement in [0.0, 1.0] {
        let mut runtime = Runtime::new();
        runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
        let translucent = Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 0.5,
        };
        runtime.set_root(
            Stack::new()
                .child(SizedBox::new(Size::new(32.0, 32.0)).color(translucent))
                .child(
                    Padding::all(4.0).child(
                        DecoratedBox::new(
                            BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()),
                        )
                        .clip_behavior(ClipBehavior::AntiAlias)
                        .child(
                            harbor_widget::BackdropFilter::new(3.0)
                                .unwrap()
                                .edge_refraction(
                                    harbor_widget::EdgeRefraction::new(4.0, displacement).unwrap(),
                                )
                                .child(SizedBox::new(Size::new(24.0, 24.0))),
                        ),
                    ),
                ),
        );
        for (side, scale) in [(32, 1.0), (64, 2.0), (48, 1.5), (32, 1.0)] {
            let viewport = Viewport::new(side, side, scale);
            runtime.set_viewport(viewport.clone());
            runtime.update(Instant::now());
            let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport);
            for (x, y) in [(16.0, 16.0), (4.0, 4.0), (2.0, 16.0)] {
                let pixel = bgra(&pixels, (x * scale) as u32, (y * scale) as u32);
                assert!(
                    (i32::from(pixel[3]) - 128).abs() <= 1,
                    "alpha not blended twice: {pixel:?}"
                );
                assert_eq!(pixel[0], 0);
                assert_eq!(pixel[2], 0);
            }
        }
        // Removing the filter returns to the direct target path, without stale blur.
        runtime.set_root(SizedBox::new(Size::new(32.0, 32.0)).color(Color::BLUE));
        runtime.update(Instant::now());
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, Viewport::new(32, 32, 1.0));
        assert_eq!(bgra(&pixels, 16, 16), [255, 0, 0, 255]);
    }
}

#[test]
fn backdrop_multiple_regions_use_distinct_uniforms_and_respect_ancestor_clip() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for multiple backdrop regions");
        return;
    };
    for displacement in [0.0, 1.0] {
        let mut runtime = Runtime::new();
        runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
        runtime.set_root(
            Stack::new()
                .child(SizedBox::new(Size::new(32.0, 32.0)).color(Color::GREEN))
                .child(
                    Padding::new(0.0, 0.0, 0.0, 16.0)
                        .child(SizedBox::new(Size::new(16.0, 32.0)).color(Color::BLUE)),
                )
                .child(
                    Padding::new(4.0, 18.0, 4.0, 4.0).child(
                        harbor_widget::BackdropFilter::new(3.0)
                            .unwrap()
                            .edge_refraction(
                                harbor_widget::EdgeRefraction::new(4.0, displacement).unwrap(),
                            )
                            .child(SizedBox::new(Size::new(10.0, 24.0))),
                    ),
                )
                .child(
                    Padding::new(4.0, 4.0, 4.0, 18.0).child(
                        DecoratedBox::new(
                            BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                        )
                        .clip_behavior(ClipBehavior::HardEdge)
                        .child(
                            harbor_widget::BackdropFilter::new(2.0)
                                .unwrap()
                                .edge_refraction(
                                    harbor_widget::EdgeRefraction::new(2.0, displacement * 0.5)
                                        .unwrap(),
                                )
                                .child(SizedBox::new(Size::new(10.0, 24.0))),
                        ),
                    ),
                ),
        );
        let viewport = Viewport::new(32, 32, 1.0);
        runtime.set_viewport(viewport.clone());
        runtime.update(Instant::now());
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport);
        let left = bgra(&pixels, 13, 16);
        let right = bgra(&pixels, 18, 16);
        assert!(
            left[0] > 10 && left[1] > 100,
            "first filter not replaced by second params: {left:?}"
        );
        assert!(
            right[0] > 100 && right[1] > 5,
            "second filter samples independently: {right:?}"
        );
        assert_eq!(bgra(&pixels, 15, 16), [0, 255, 0, 255], "gap remains sharp");
        assert_eq!(bgra(&pixels, 17, 16), [255, 0, 0, 255], "gap remains sharp");
        assert_eq!(
            bgra(&pixels, 18, 4),
            [255, 0, 0, 255],
            "rounded ancestor is authoritative"
        );
    }
}

#[test]
fn transparent_icon_button_has_no_zero_width_border_fill() {
    let Some((device, queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter for transparent button evidence");
        return;
    };
    let mut style = harbor_widget::Theme::default().icon_button;
    style.min_height = 32.0;
    style.border_width = 0.0;
    for colors in [
        &mut style.normal,
        &mut style.hovered,
        &mut style.pressed,
        &mut style.disabled,
        &mut style.selected,
    ] {
        colors.background = Color::TRANSPARENT;
    }
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .child(SizedBox::new(Size::new(32.0, 32.0)).color(Color::GREEN))
            .child(
                harbor_widget::IconButton::new(harbor_widget::LucideIcon::Plus, "transparent")
                    .style(style),
            ),
    );
    let viewport = Viewport::new(32, 32, 1.0);
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());
    for event in [
        None,
        Some(harbor_widget::input::event::PointerPhase::Move),
        Some(harbor_widget::input::event::PointerPhase::Down),
        Some(harbor_widget::input::event::PointerPhase::Up),
    ] {
        if let Some(phase) = event {
            runtime.dispatch(harbor_widget::input::event::UiEvent::Pointer(
                harbor_widget::input::event::PointerEvent::new(
                    Point::new(16.0, 16.0),
                    phase,
                    harbor_widget::input::event::PointerButton::Left,
                    7,
                ),
            ));
            runtime.update(Instant::now());
        }
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        assert_eq!(
            bgra(&pixels, 8, 8),
            [0, 255, 0, 255],
            "transparent idle/hover/pressed button is not a fill"
        );
    }
}
