use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn independent_ordered_output_tracks_scene_edits_and_translucent_compositing() {
    let (device, queue) = gpu_device();
    // Both opaque occlusion and premultiplied alpha composition must follow
    // paint order. IDs deliberately do not sort into that order.
    for alpha in [1.0, 0.5] {
        let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        let mut scene = harbor_widget::scene::SceneGraph::new();
        let viewport = Viewport::new(32, 32, 1.0);
        let frames: &[&[u64]] = &[
            &[50, 10],     // Reverse allocation order on the initial upload.
            &[10, 50],     // Same IDs, order-only edit.
            &[10, 99, 50], // New slot between storage-adjacent retained items.
            &[10, 50],     // Remove the inserted layer.
            &[50],         // Free the other original slot.
            &[50, 7],      // Reuse it at the top, not its old paint position.
            &[50, 99, 7],  // Reinsert removed content in the middle.
            &[99, 7, 50],  // Reorder all retained layers.
            &[],           // No removed content may remain visible.
        ];
        let color = |id| {
            let mut c = match id {
                50 => Color::RED,
                10 => Color::BLUE,
                99 => Color::GREEN,
                7 => Color::WHITE,
                _ => unreachable!(),
            };
            c.a = alpha;
            c
        };
        for (frame, ids) in frames.iter().enumerate() {
            let items = ids
                .iter()
                .enumerate()
                .map(|(order, &id)| {
                    let mut item = make_rounded_item(
                        id,
                        Primitive::Quad {
                            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
                            color: color(id),
                            corner_radius: 0.0,
                        },
                    );
                    item.paint_order = order as u32;
                    item
                })
                .collect();
            let mut delta = scene.diff(items);
            if frame == 0 {
                delta.added.reverse();
            }
            renderer.update(&queue, &delta, &viewport);
            let paint_order: Vec<_> = scene.items().iter().map(|item| item.id).collect();
            let pixels = render_retained_quads(&device, &queue, &renderer, 32, &paint_order);
            let actual = bgra(&pixels, 16, 16);
            let mut expected = [0.0_f32; 4];
            for &id in ids.iter() {
                let c = color(id);
                for (dst, src) in expected
                    .iter_mut()
                    .zip([c.b * c.a, c.g * c.a, c.r * c.a, c.a])
                {
                    *dst = src + *dst * (1.0 - c.a);
                }
            }
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!(
                    (actual as f32 - expected * 255.0).abs() <= 2.0,
                    "alpha={alpha}, frame={frame}, order={ids:?}: actual={actual}, expected={expected}"
                );
            }
        }
        eprintln!(
            "Independent ordered GPU readback: alpha={alpha}, {} frames executed",
            frames.len()
        );
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_render_rounded_fill_inside_shape_and_leave_corner_outside_shape_empty() {
    // Arrange
    let (device, queue) = gpu_device();

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
        renderer.encode_ordered(&mut pass, [1]);
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_render_rounded_border_only_on_outer_ring() {
    // Arrange
    let (device, queue) = gpu_device();
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
        renderer.encode_ordered(&mut pass, [1]);
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_render_collapsed_inner_border_as_a_bounded_outer_shape() {
    // Arrange
    let (device, queue) = gpu_device();
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
        renderer.encode_ordered(&mut pass, [1]);
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn rounded_fill_keeps_straight_edges_and_selects_asymmetric_corner_radii() {
    let (device, queue) = gpu_device();
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
        renderer.encode_ordered(&mut pass, [1]);
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_pass_retain_or_live_mode_from_eligibility_and_commit() {
    use harbor_widget::scene::primitive::{ExternalScheduleDemand, ExternalScheduleFn};

    let (device, queue) = gpu_device();

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
    runtime.set_root(
        ExternalSurface::new(1)
            .on_draw(handler)
            .schedule_with(schedule),
    );
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_select_live_when_encode_has_no_deferred_ids() {
    let (device, queue) = gpu_device();

    // Arrange — no schedule collect, so missing ids default to Live
    let mode = Arc::new(Mutex::new(None));
    let observed = Arc::clone(&mode);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, draw_mode| {
        *observed.lock().unwrap() = Some(draw_mode);
    });
    let viewport = Viewport::new(256, 256, 1.0);
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    runtime.set_root(ExternalSurface::new(1).on_draw(handler));
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
