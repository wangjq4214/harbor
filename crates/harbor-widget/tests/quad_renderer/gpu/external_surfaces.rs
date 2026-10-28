use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_skip_external_draw_when_scissor_rect_is_empty() {
    let (device, queue) = gpu_device();

    // Arrange: ExternalSurface fills the full viewport; handler must not run at zero size.
    let invoked = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&invoked);
    let handler: Arc<ExternalDrawFn<'static>> =
        Arc::new(move |_draw_id, _context, _gpu, _pass, _mode| {
            observed.store(true, Ordering::SeqCst);
        });
    let viewport = Viewport::new(0, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(ExternalSurface::new(99).on_draw(handler));
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_invoke_external_draw_with_correct_rect() {
    let (device, _queue) = gpu_device();

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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_invoke_registered_external_surface_handler_during_runtime_encode() {
    let (device, queue) = gpu_device();

    // Arrange: a Runtime whose root ExternalSurface registers a handler.
    let invoked_draw_id = Arc::new(AtomicU64::new(u64::MAX));
    let observed_draw_id = Arc::clone(&invoked_draw_id);
    let handler: Arc<ExternalDrawFn<'static>> =
        Arc::new(move |draw_id, _context, _gpu, _pass, _mode| {
            observed_draw_id.store(draw_id.get(), Ordering::SeqCst);
        });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(ExternalSurface::new(42).on_draw(handler));
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_invoke_each_nested_external_surface_handler_for_its_draw_id() {
    let (device, queue) = gpu_device();

    // Arrange: two handlers retained by ExternalSurface children built through Stack.
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
            .push_child(ExternalSurface::new(1).on_draw(first_handler))
            .push_child(ExternalSurface::new(2).on_draw(second_handler)),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_preserve_retained_external_surface_order_and_rects_across_widget_primitives() {
    let (device, queue) = gpu_device();

    // Arrange: retained ExternalSurface handlers are interleaved with real Widget
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
            .push_child(
                SizedBox::new(Size::new(24.0, 24.0))
                    .unwrap()
                    .background(Color::RED)
                    .expect("valid color"),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 4.0,
                    right: 8.0,
                    bottom: 8.0,
                    left: 4.0,
                })
                .unwrap()
                .child(ExternalSurface::new(1).on_draw(make_handler(order.clone(), rects.clone()))),
            )
            .push_child(
                SizedBox::new(Size::new(24.0, 24.0))
                    .unwrap()
                    .background(Color::GREEN)
                    .expect("valid color"),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 8.0,
                    right: 4.0,
                    bottom: 4.0,
                    left: 8.0,
                })
                .unwrap()
                .child(ExternalSurface::new(2).on_draw(make_handler(order.clone(), rects.clone()))),
            )
            .push_child(
                SizedBox::new(Size::new(24.0, 24.0))
                    .unwrap()
                    .background(Color::BLUE)
                    .expect("valid color"),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 12.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 12.0,
                })
                .unwrap()
                .child(ExternalSurface::new(3).on_draw(make_handler(order.clone(), rects.clone()))),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clip_external_surface_and_restore_scissor_for_following_widget_primitive() {
    let (device, queue) = gpu_device();

    // Arrange: draw a full-target triangle through a ExternalSurface padded into a 16×16 region.
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
            .push_child(SizedBox::new(Size::new(32.0, 32.0)).unwrap())
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 8.0,
                    right: 8.0,
                    bottom: 8.0,
                    left: 8.0,
                })
                .unwrap()
                .child(ExternalSurface::new(1).on_draw(handler)),
            )
            .push_child(
                SizedBox::new(Size::new(8.0, 8.0))
                    .unwrap()
                    .background(Color::RED)
                    .expect("valid color"),
            ),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_use_last_registered_handler_when_external_surfaces_share_draw_id() {
    let (device, queue) = gpu_device();

    // Arrange: two ExternalSurface children register distinct handlers for one draw ID.
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
            .push_child(ExternalSurface::new(42).on_draw(first_handler))
            .push_child(ExternalSurface::new(42).on_draw(last_handler)),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_remove_handler_when_external_surface_is_replaced() {
    let (device, queue) = gpu_device();

    // Arrange: a Runtime with an initially registered ExternalSurface handler.
    let calls = Arc::new(AtomicU64::new(0));
    let observed_calls = Arc::clone(&calls);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, _| {
        observed_calls.fetch_add(1, Ordering::SeqCst);
    });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(ExternalSurface::new(42).on_draw(handler));
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

    // Act: replace the ExternalSurface with an unregistered instance using the same draw ID.
    runtime.set_root(ExternalSurface::new(42));
    runtime.update(Instant::now());
    let mut replacement_encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = create_render_pass(&device, &mut replacement_encoder);
        runtime.encode(draw_gpu(&device, &queue), &mut pass, viewport, false);
    }
    replacement_encoder.finish();

    // Assert: the removed ExternalSurface's handler is not retained across rebuilds.
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_skip_external_primitive_when_external_surface_has_no_handler() {
    let (device, queue) = gpu_device();

    // Arrange: a Runtime whose ExternalSurface has the default, handler-free configuration.
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(ExternalSurface::new(42));
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_select_distinct_modes_when_two_externals_differ_in_eligibility() {
    use harbor_widget::scene::primitive::{ExternalScheduleDemand, ExternalScheduleFn};

    let (device, queue) = gpu_device();

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
            .push_child(
                ExternalSurface::new(1)
                    .on_draw(live_handler)
                    .schedule_with(eligible),
            )
            .push_child(
                ExternalSurface::new(2)
                    .on_draw(retain_handler)
                    .schedule_with(deferred),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_leave_external_surface_corners_when_clip_policy_is_none() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_discard_external_surface_pixels_outside_arc_when_clip_is_hard_edge() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_apply_fractional_coverage_when_external_surface_clip_is_anti_alias() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_keep_external_surface_callback_geometry_when_rounded_clip_is_active() {
    let (device, queue) = gpu_device();
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
            .child(ExternalSurface::new(7).on_draw(handler)),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_not_punch_following_widget_outside_external_surface_allocation() {
    let (device, queue) = gpu_device();
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .push_child(
                SizedBox::new(Size::new(8.0, 8.0))
                    .unwrap()
                    .background(Color::RED)
                    .expect("valid color"),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 8.0,
                    right: 8.0,
                    bottom: 8.0,
                    left: 8.0,
                })
                .unwrap()
                .child(clipped_external(&device, ClipBehavior::HardEdge, 4.0)),
            ),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 255, 255]);
    assert_eq!(bgra(&pixels, 8, 8), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clip_nested_external_surface_to_outer_rounded_bound() {
    let (device, queue) = gpu_device();
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(ClipBehavior::HardEdge)
            .child(
                Padding::all(4.0).unwrap().child(
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clip_external_surface_when_viewport_scale_is_two() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_mask_external_surface_corners_when_encode_mode_is_retain() {
    use harbor_widget::scene::primitive::{ExternalScheduleDemand, ExternalScheduleFn};

    let (device, queue) = gpu_device();
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
            .child(
                ExternalSurface::new(1)
                    .on_draw(handler)
                    .schedule_with(schedule),
            ),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_apply_independent_masks_when_two_clipped_external_surfaces_share_a_pass() {
    let (device, queue) = gpu_device();
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 0.0,
                    right: 16.0,
                    bottom: 0.0,
                    left: 0.0,
                })
                .unwrap()
                .child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external(&device, 1)),
                ),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 16.0,
                })
                .unwrap()
                .child(
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_dest_in_underlapping_pixels_inside_external_surface_allocation_aabb() {
    let (device, queue) = gpu_device();
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .push_child(
                SizedBox::new(Size::new(32.0, 32.0))
                    .unwrap()
                    .background(Color::RED)
                    .expect("valid color"),
            )
            .push_child(clipped_external(&device, ClipBehavior::HardEdge, 8.0)),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clip_external_surface_when_viewport_scale_is_one_and_a_half() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_keep_independent_masks_when_more_than_eight_externals_share_a_pass() {
    let (device, queue) = gpu_device();
    let mut root = Stack::new();
    for row in 0..3 {
        for col in 0..3 {
            let index = row * 3 + col;
            let (r, g, b) = match index {
                0 => (0.0, 1.0, 0.0),
                8 => (1.0, 0.0, 0.0),
                _ => (0.0, 0.0, 1.0),
            };
            root = root.push_child(
                Padding::new(harbor_widget::Insets {
                    top: row as f32 * 16.0,
                    right: (2 - col) as f32 * 16.0,
                    bottom: (2 - row) as f32 * 16.0,
                    left: col as f32 * 16.0,
                })
                .unwrap()
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_apply_later_mask_slot_when_unmasked_external_is_between() {
    let (device, queue) = gpu_device();
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        Stack::new()
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 0.0,
                    right: 22.0,
                    bottom: 0.0,
                    left: 0.0,
                })
                .unwrap()
                .child(
                    DecoratedBox::new(
                        BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                    )
                    .clip_behavior(ClipBehavior::HardEdge)
                    .child(solid_external(&device, 1)),
                ),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 0.0,
                    right: 11.0,
                    bottom: 0.0,
                    left: 11.0,
                })
                .unwrap()
                .child(solid_external_rgb(&device, 2, 0.0, 0.0, 1.0)),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 22.0,
                })
                .unwrap()
                .child(
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
