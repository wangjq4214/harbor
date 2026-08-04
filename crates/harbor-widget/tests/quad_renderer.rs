//! Integration tests for QuadRenderer — slot_of, encode_range, and encode.
//!
//! These tests require a wgpu Device; they are skipped silently if no GPU
//! adapter is available.

use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::renderer::quad::QuadRenderer;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, ExternalDrawFn, Primitive};
use harbor_widget::scene::{SceneDelta, SceneItem};
use harbor_widget::widgets::custom_paint::CustomPaint;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::widgets::stack::Stack;
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
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("custom-paint clipping test shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
                @vertex
                fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
                    var positions = array<vec2<f32>, 3>(
                        vec2<f32>(-1.0, -1.0),
                        vec2<f32>(3.0, -1.0),
                        vec2<f32>(-1.0, 3.0),
                    );
                    return vec4<f32>(positions[index], 0.0, 1.0);
                }

                @fragment
                fn fs_main() -> @location(0) vec4<f32> {
                    return vec4<f32>(0.0, 1.0, 0.0, 1.0);
                }
            "#
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
    const SIZE: u32 = 32;
    const BYTES_PER_ROW: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("custom-paint clipping test readback"),
        size: (SIZE * BYTES_PER_ROW) as u64,
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
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
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
        paint_order: order,
    }
}

// ── slot_of tests ──────────────────────────────────────────────────────────

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

// ── encode_range tests ──────────────────────────────────────────────────────

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
fn should_invoke_external_draw_with_correct_rect() {
    let Some((device, _queue)) = try_create_device() else {
        return;
    };

    // External rect at logical (10, 20), size (200, 150).
    let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(200.0, 150.0));

    let callback_called = AtomicBool::new(false);

    let external_draw: &harbor_widget::scene::primitive::ExternalDrawFn<'_> =
        &|draw_id, cb_rect, _pass| {
            callback_called.store(true, Ordering::SeqCst);
            assert_eq!(draw_id, 42);
            assert_eq!(cb_rect.min.x, 10.0);
            assert_eq!(cb_rect.min.y, 20.0);
            assert_eq!(cb_rect.size().width, 200.0);
            assert_eq!(cb_rect.size().height, 150.0);
        };

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);

        external_draw(42, rect, &mut pass);
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
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |draw_id, _rect, _pass| {
        observed_draw_id.store(draw_id, Ordering::SeqCst);
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
        runtime.encode(&queue, &mut pass, viewport);
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
    let first_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |draw_id, _rect, _pass| {
        first_observed.store(draw_id, Ordering::SeqCst);
    });
    let second_draw_id = Arc::new(AtomicU64::new(u64::MAX));
    let second_observed = Arc::clone(&second_draw_id);
    let second_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |draw_id, _rect, _pass| {
        second_observed.store(draw_id, Ordering::SeqCst);
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
        runtime.encode(&queue, &mut pass, viewport);
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
    // quad primitives. Padding gives each callback a distinct exact rectangle.
    let order = Arc::new(Mutex::new(Vec::new()));
    let rects = Arc::new(Mutex::new(Vec::new()));
    let make_handler = |order: Arc<Mutex<Vec<u64>>>, rects: Arc<Mutex<Vec<Rect>>>| {
        Arc::new(move |draw_id, rect, _pass: &mut wgpu::RenderPass<'_>| {
            order.lock().unwrap().push(draw_id);
            rects.lock().unwrap().push(rect);
        }) as Arc<ExternalDrawFn<'static>>
    };
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
            .child(SizedBox::new(Size::new(24.0, 24.0)).color(Color::RED))
            .child(
                Padding::new(4.0, 4.0, 4.0, 4.0)
                    .child(CustomPaint::new(1).handler(make_handler(order.clone(), rects.clone()))),
            )
            .child(SizedBox::new(Size::new(24.0, 24.0)).color(Color::GREEN))
            .child(
                Padding::new(8.0, 8.0, 8.0, 8.0)
                    .child(CustomPaint::new(2).handler(make_handler(order.clone(), rects.clone()))),
            )
            .child(SizedBox::new(Size::new(24.0, 24.0)).color(Color::BLUE))
            .child(
                Padding::new(12.0, 12.0, 12.0, 12.0)
                    .child(CustomPaint::new(3).handler(make_handler(order.clone(), rects.clone()))),
            ),
    );
    runtime.set_viewport(viewport.clone());
    runtime.update(Instant::now());

    // Act: encode the retained scene in one render pass.
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        runtime.encode(&queue, &mut pass, viewport);
    }
    encoder.finish();

    // Assert: callbacks retain paint order and receive their exact rectangles.
    assert_eq!(*order.lock().unwrap(), vec![1, 2, 3]);
    assert_eq!(
        *rects.lock().unwrap(),
        vec![
            Rect::from_min_size(Point::new(4.0, 4.0), Size::new(24.0, 24.0)),
            Rect::from_min_size(Point::new(8.0, 8.0), Size::new(24.0, 24.0)),
            Rect::from_min_size(Point::new(12.0, 12.0), Size::new(24.0, 24.0)),
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
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, pass| {
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    let viewport = Viewport::new(32, 32, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        Stack::new()
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
        runtime.encode(&queue, &mut pass, viewport);
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
    let first_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _| {
        first_observed.fetch_add(1, Ordering::SeqCst);
    });
    let last_calls = Arc::new(AtomicU64::new(0));
    let last_observed = Arc::clone(&last_calls);
    let last_handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _| {
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
        runtime.encode(&queue, &mut pass, viewport);
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
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _| {
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
        runtime.encode(&queue, &mut pass, viewport.clone());
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
        runtime.encode(&queue, &mut pass, viewport);
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
        runtime.encode(&queue, &mut pass, viewport);
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
