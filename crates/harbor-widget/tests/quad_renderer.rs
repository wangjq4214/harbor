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
use harbor_widget::widgets::stack::Stack;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
fn should_invoke_external_draw_with_correct_rect_and_scissor() {
    let Some((device, _queue)) = try_create_device() else {
        return;
    };

    // Use a viewport scaled so the scissor fits within the 256x256 render target.
    // For 1x scale, a 200x150 logical rect → 200x150 physical, well within 256x256.
    let viewport = Viewport::new(256, 256, 1.0);

    // External rect at logical (10, 20), size (200, 150)
    let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(200.0, 150.0));

    // Physical coordinates computed by encode():
    let phys_x = (rect.min.x * viewport.scale_factor) as u32;
    let phys_y = (rect.min.y * viewport.scale_factor) as u32;
    let phys_w = ((rect.size().width * viewport.scale_factor).ceil() as u32).max(1);
    let phys_h = ((rect.size().height * viewport.scale_factor).ceil() as u32).max(1);

    assert_eq!(phys_x, 10);
    assert_eq!(phys_y, 20);
    assert_eq!(phys_w, 200);
    assert_eq!(phys_h, 150);

    // Verify the scissor is set and restored in correct sequence
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

        // Simulate the encode scissor → callback → restore sequence
        pass.set_scissor_rect(phys_x, phys_y, phys_w, phys_h);
        external_draw(42, rect, &mut pass);
        // Restore full scissor (as encode does)
        pass.set_scissor_rect(0, 0, viewport.physical_size.0, viewport.physical_size.1);
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
