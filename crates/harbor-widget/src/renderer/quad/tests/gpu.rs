use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_return_none_for_unknown_id() {
    let (device, _queue) = gpu_device();
    let renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    assert_eq!(renderer.slot_of(999), None);
    assert_eq!(renderer.slot_of(0), None);
    assert_eq!(renderer.slot_of(1), None);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_return_correct_slot_after_update() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_return_none_after_item_removed() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_recycle_free_slots() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_track_multiple_items_at_distinct_slots() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_return_none_for_id_zero_by_default() {
    let (device, _queue) = gpu_device();
    let renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    // id 0 is the default/unset value; it won't be in the map unless added
    assert_eq!(renderer.slot_of(0), None);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn grows_instance_buffer_without_losing_live_slots() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_not_panic_on_encode_range_with_no_instances() {
    let (device, _queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_not_panic_on_encode_range_with_instances() {
    let (device, queue) = gpu_device();
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    // Add 3 instances
    let items: Vec<SceneItem> = (0..3)
        .map(|i| make_quad_item(i as u64 + 1, i, i as f32 * 100.0, 0.0, 80.0, 40.0))
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clamp_count_to_available_instances() {
    let (device, queue) = gpu_device();
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_encode_retained_items_in_order() {
    let (device, queue) = gpu_device();
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    // Add 2 instances
    let items = vec![
        make_quad_item(1, 0, 0.0, 0.0, 100.0, 50.0),
        make_quad_item(2, 1, 200.0, 0.0, 100.0, 50.0),
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
        renderer.encode_ordered(&mut pass, delta.added.iter().map(|item| item.id));
    }
    encoder.finish();
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_encode_single_instance_with_range() {
    let (device, queue) = gpu_device();
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = make_viewport();

    let item = make_quad_item(1, 0, 0.0, 0.0, 100.0, 50.0);
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_handle_zero_count_gracefully() {
    let (device, queue) = gpu_device();
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

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn lucide_icon_updates_reuse_slots_and_switch_safely_between_primitive_types() {
    let (device, queue) = gpu_device();
    let viewport = Viewport::new(32, 32, 1.0);
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let mut item = make_rounded_item(
        7,
        Primitive::Icon {
            icon: crate::LucideIcon::Plus,
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
        icon: crate::LucideIcon::X,
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn ordered_drawing_retains_resources_slots_and_only_adjacent_batches() {
    let (device, queue) = gpu_device();
    let mut renderer = QuadRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let mut items: Vec<_> = (0..3)
        .map(|i| make_quad_item(i as u64 + 1, i, 0.0, 0.0, 32.0, 32.0))
        .collect();
    let viewport = Viewport::new(32, 32, 1.0);
    renderer.update(
        &queue,
        &SceneDelta {
            added: items.clone(),
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    let resources = renderer.resource_handles();
    items.reverse();
    for (order, item) in items.iter_mut().enumerate() {
        item.paint_order = order as u32;
    }
    renderer.update(
        &queue,
        &SceneDelta {
            modified: items,
            added: vec![],
            removed: vec![],
        },
        &viewport,
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = create_render_pass(&device, &mut encoder);
        renderer.encode_ordered(&mut pass, [3, 2, 1]);
        let mut run = QuadRun::new();
        run.extend(&renderer, &mut pass, 0);
        run.extend(&renderer, &mut pass, 1);
        assert_eq!((run.start, run.count), (Some(0), 2), "adjacent slots batch");
        run.extend(&renderer, &mut pass, 0);
        assert_eq!(
            (run.start, run.count),
            (Some(0), 1),
            "reverse order flushes"
        );
        run.extend(&renderer, &mut pass, 2);
        assert_eq!((run.start, run.count), (Some(2), 1), "a gap flushes");
        run.flush(&renderer, &mut pass);
        run.extend(&renderer, &mut pass, 1);
        assert_eq!(
            (run.start, run.count),
            (Some(1), 1),
            "explicit boundaries flush"
        );
        run.flush(&renderer, &mut pass);
    }
    queue.submit(Some(encoder.finish()));
    assert_eq!(renderer.resource_handles(), resources);
    assert_eq!(
        [
            renderer.slot_of(1),
            renderer.slot_of(2),
            renderer.slot_of(3)
        ],
        [Some(0), Some(1), Some(2)]
    );
    renderer.update(
        &queue,
        &SceneDelta {
            removed: vec![2],
            added: vec![],
            modified: vec![],
        },
        &viewport,
    );
    renderer.update(
        &queue,
        &SceneDelta {
            added: vec![make_quad_item(4, 2, 0.0, 0.0, 32.0, 32.0)],
            removed: vec![],
            modified: vec![],
        },
        &viewport,
    );
    assert_eq!(renderer.slot_of(4), Some(1), "freed slot reused");
    assert_eq!(
        renderer.resource_handles(),
        resources,
        "reuse must not rebuild resources"
    );
}
