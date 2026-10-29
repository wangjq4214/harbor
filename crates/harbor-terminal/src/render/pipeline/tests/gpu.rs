use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_pipeline_prepares_replayed_updates_and_invalidates_projection() {
    let (device, queue) = crate::test_gpu::device();
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let gpu = TerminalGpuAccess::new(&device, &queue, format);
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings::default())
        .expect("test system font");
    let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
    let mut terminal = Terminal::new_headless(4, 8);
    let now = Instant::now();
    let first = terminal.read_update(now);
    let mut renderer =
        TerminalRenderPipeline::new(gpu, (160, 120), fonts, metrics, &first).unwrap();
    renderer.prepare(gpu, &first, true);
    assert!(terminal.acknowledge_update(&first));

    // An unrendered incremental update must still be available on the next live frame.
    terminal.put_str("a");
    let pending = terminal.read_update(now);
    assert!(matches!(pending.damage, UpdateDamage::Ranges(_)));
    renderer.prepare(gpu, &terminal.read_update(now), true);
    assert!(terminal.acknowledge_update(&pending));

    let mut viewport = renderer.viewport();
    viewport.allocation_origin = (3.0, 5.0);
    viewport.surface_size = (256, 256);
    viewport.cell_width *= 1.25; // Physical font metrics after a DPI change.
    viewport.line_height *= 1.25;
    renderer.sync_viewport(viewport, false);
    assert!(renderer.background.is_dirty());
    assert!(renderer.text.is_dirty());
    assert!(renderer.decoration.is_dirty());
    assert!(renderer.selection.is_dirty());
    assert!(renderer.cursor.is_dirty());

    terminal.put_bytes(b"\x1b]11;#445566\x07");
    let updated = terminal.read_update(now);
    renderer.sync_palette(updated.appearance.palette());
    assert!(renderer.background.is_dirty());
    assert!(renderer.text.is_dirty());
    assert!(renderer.decoration.is_dirty());
    renderer.prepare(gpu, &updated, true);
    assert!(terminal.acknowledge_update(&updated));

    let mut recolored = updated.appearance.palette();
    recolored.selection = harbor_config::Rgba::new(0.4, 0.3, 0.2, 0.5);
    renderer.sync_palette(recolored);
    assert!(renderer.selection.is_dirty());
    // The coherent update restores the engine palette before acknowledging.
    renderer.prepare(gpu, &terminal.read_update(now), true);
    assert!(!renderer.selection.is_dirty());

    terminal.put_str("b\nc\nd\ne\nf\n");
    assert!(terminal.read_update(now).snapshot.scroll_count > 0);
    terminal.pointer.set_viewport(viewport);
    for (phase, x) in [
        (crate::TerminalPointerPhase::Down, 20.0),
        (crate::TerminalPointerPhase::Move, 50.0),
        (crate::TerminalPointerPhase::Up, 50.0),
    ] {
        terminal
            .handle_event(crate::TerminalEvent::Pointer(
                crate::TerminalPointerEvent::new(
                    (x, 20.0),
                    phase,
                    crate::TerminalPointerButton::Left,
                    1,
                ),
            ))
            .unwrap();
    }
    assert!(terminal.read_update(now).selection.is_some());

    terminal
        .handle_event(crate::TerminalEvent::Preedit(crate::Preedit::new(
            "ime", None,
        )))
        .unwrap();
    terminal.resize_if_changed(crate::TerminalSize { rows: 5, cols: 9 });
    terminal.invalidate_update(); // Simulate a lost/uncertain projection.
    let replay = terminal.read_update(now);
    assert_eq!(replay.damage, UpdateDamage::FullUpload);
    renderer.sync_viewport(viewport, true);
    renderer.prepare(gpu, &replay, true);
    assert!(terminal.acknowledge_update(&replay));
    assert!(!renderer.text.is_dirty());

    let page = terminal
        .read_content(
            crate::read_model::ReadScope::Visible {
                start_row: 0,
                row_count: 5,
            },
            crate::read_model::ReadBudget::default(),
        )
        .unwrap();
    let ranges: Vec<_> = page
        .fragments
        .iter()
        .filter(|f| !f.units().is_empty())
        .filter_map(|f| f.range(0..1))
        .collect();
    assert!(!ranges.is_empty());
    renderer.prepare_highlights(gpu, &terminal.project_content_ranges(&ranges));
    assert_eq!(terminal.read_update(now).preedit, replay.preedit);
    assert_eq!(terminal.read_update(now).selection, replay.selection);

    // Encode and submit all layers, including preedit and scrollbar, for GPU validation.
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terminal update test target"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terminal update test pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        renderer.draw(&mut pass);
    }
    queue.submit(Some(encoder.finish()));
}
