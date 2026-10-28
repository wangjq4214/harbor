use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_render_outer_shadow_only_outside_the_shape_with_soft_and_hard_edges() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_constrain_outer_shadow_to_ancestor_clip_bounds() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_compose_overlapping_shadows_in_source_order() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_encode_raw_shadow_order_when_it_differs_from_slot_order() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_preserve_shadow_fill_external_border_order_across_flushes() {
    let (device, queue) = gpu_device();
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
        Padding::new(harbor_widget::Insets {
            top: 8.0,
            right: 8.0,
            bottom: 8.0,
            left: 8.0,
        })
        .unwrap()
        .child(DecoratedBox::new(decoration).child(ExternalSurface::new(7).on_draw(handler))),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_keep_fractional_shadow_coverage_bounded_and_alpha_scaled_across_dpi() {
    let (device, queue) = gpu_device();

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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_render_a_visible_hard_edge_for_a_zero_blur_decorated_shadow() {
    // Arrange: the public widget path gives the hard shadow a positive spread,
    // so the portion outside its 16×16 fill must remain visible.
    let (device, queue) = gpu_device();
    let shadow = BoxShadow::new()
        .try_color(Color::BLUE)
        .unwrap()
        .try_spread_radius(4.0)
        .unwrap();
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(
        DecoratedBox::new(BoxDecoration::new().shadow(shadow)).child(
            SizedBox::new(Size::new(16.0, 16.0))
                .unwrap()
                .background(Color::GREEN)
                .expect("valid color"),
        ),
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_keep_zero_blur_rounded_shadow_outside_its_rounded_shape() {
    // Arrange: zero blur still receives a one-pixel raster allocation, but
    // its hard edge must be evaluated against the rounded shape rather than
    // filling that allocation as a rectangular halo.
    let (device, queue) = gpu_device();
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
