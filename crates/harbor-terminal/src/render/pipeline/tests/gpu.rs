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

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_snapped_output_uploads_follow_engine_damage() {
    use crate::render::observations::buffer_write_bytes;

    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for (rows, cols) in [(24, 80), (50, 160)] {
        let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
            family: Some("Consolas".into()),
            size: 16.0,
        })
        .unwrap();
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        let mut terminal = Terminal::new_headless(rows, cols);
        terminal.put_str("xy");
        terminal.put_bytes(b"\x1b[1;1H");
        let initial = terminal.read_update(Instant::now());
        let mut renderer =
            TerminalRenderPipeline::new(gpu, (1600, 1200), fonts, metrics, &initial).unwrap();
        assert!(renderer.prepare(gpu, &initial, false));
        assert!(terminal.acknowledge_update(&initial));
        for (mode, bytes) in [
            ("one_cell", b"\rx".as_slice()),
            ("control_only", b"\x1b[0m".as_slice()),
        ] {
            terminal.process_output(bytes);
            let update = terminal.read_update(Instant::now());
            let ranges = update_dirty_ranges(&update);
            let snap = &update.snapshot;
            let viewport = renderer.viewport();
            let tint = update.appearance.clear_rgba(update.backdrop_available);
            let actual = [
                buffer_write_bytes(|| {
                    renderer
                        .background
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport, tint)
                }),
                buffer_write_bytes(|| {
                    renderer
                        .boxes
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport)
                }),
                buffer_write_bytes(|| {
                    renderer
                        .text
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport, None)
                }),
                buffer_write_bytes(|| {
                    renderer
                        .decoration
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport)
                }),
            ];
            // Fixed slot layouts; use production geometry builders where constants are private.
            let cell_bytes = [
                6 * std::mem::size_of::<ColoredVertex>(),
                crate::render::box_drawing::BOX_VERTICES_PER_CELL
                    * std::mem::size_of::<ColoredVertex>(),
                6 * std::mem::size_of::<TexturedVertex>(),
                crate::render::decoration::build_underline_vertices(
                    &metrics,
                    snap,
                    &viewport,
                    &update.appearance.palette(),
                )
                .len()
                    / (rows * cols)
                    * std::mem::size_of::<ColoredVertex>()
                    + 12 * std::mem::size_of::<ColoredVertex>(),
            ];
            for (layer, (actual, per_cell)) in actual.into_iter().zip(cell_bytes).enumerate() {
                let plan = UploadPolicy::default().decide(rows, cols, per_cell, &ranges, false);
                let expected = if layer == 1 || layer == 3 {
                    0 // No previous or current box/decoration geometry in this ASCII fixture.
                } else {
                    match plan.mode {
                        UploadMode::None => 0,
                        UploadMode::Incremental => plan.dirty_bytes,
                        UploadMode::Full => plan.full_bytes,
                    }
                };
                assert_eq!(actual, expected, "grid={cols}x{rows} mode={mode}");
            }
            println!(
                "grid={cols}x{rows} mode={mode} actual_buffer_bytes_bg_box_text_decoration={actual:?} total={}",
                actual.iter().sum::<usize>()
            );
            assert!(renderer.prepare(gpu, &update, false));
            assert!(terminal.acknowledge_update(&update));
            queue.submit([]);
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_snapped_local_edits_match_fresh_full_projection() {
    use crate::render::text::sequence_tests::read_draw;
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = || {
        harbor_text::load_system_fonts(&harbor_config::FontSettings {
            family: Some("Consolas".into()),
            size: 16.0,
        })
        .unwrap()
    };
    let metrics = TextMetrics::from_font_metrics(fonts().font_metrics());
    let mut terminal = Terminal::new_headless(4, 16);
    terminal.put_str("\x1b[41;4m┌─┐ A\x1b[0m\r\nxy中\r\nlast");
    let initial = terminal.read_update(Instant::now());
    let mut renderer =
        TerminalRenderPipeline::new(gpu, (256, 128), fonts(), metrics, &initial).unwrap();
    assert!(renderer.prepare(gpu, &initial, false));
    assert!(terminal.acknowledge_update(&initial));
    for bytes in [
        "\x1b[1;2H\x1b[0mz",
        "\x1b[1;2H\x1b[42;4m─",
        "\x1b[2;4H\x1b[0mx",
        "\x1b[0m",
        "\x1b[?1049hALT",
        "\x1b[?1049l",
    ] {
        // A control-only chunk after an edit must not consume pending damage.
        terminal.process_output(bytes.as_bytes());
        terminal.process_output(b"\x1b[0m");
        let update = terminal.read_update(Instant::now());
        assert!(renderer.prepare(gpu, &update, false));
        let mut fresh =
            TerminalRenderPipeline::new(gpu, (256, 128), fonts(), metrics, &update).unwrap();
        assert!(fresh.prepare(gpu, &update, false));
        let actual = read_draw(&device, &queue, |pass| renderer.draw(pass));
        let expected = read_draw(&device, &queue, |pass| fresh.draw(pass));
        let differences = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
        assert_eq!(differences, 0, "rendered bytes differ after {bytes:?}");
        assert!(terminal.acknowledge_update(&update));
    }
}

/// Paired before/after payload measurement, through the real per-layer queue writes.
#[test]
#[ignore = "gpu: run explicitly for vertex-staging performance evidence"]
fn gpu_vertex_staging_payloads() {
    use crate::render::observations::buffer_write_bytes;
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for (rows, cols) in [(24, 80), (50, 160)] {
        let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
            family: Some("Consolas".into()),
            size: 16.0,
        })
        .unwrap();
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        let mut terminal = Terminal::new_headless(rows, cols);
        for row in 1..=rows {
            terminal.put_str(&format!("\x1b[{row};1H{}", "xy".repeat(cols / 2)));
        }
        let initial = terminal.read_update(Instant::now());
        let mut renderer =
            TerminalRenderPipeline::new(gpu, (1800, 1400), fonts, metrics, &initial).unwrap();
        assert!(renderer.prepare(gpu, &initial, false));
        assert!(terminal.acknowledge_update(&initial));
        for mode in [
            "ascii_full",
            "ascii_edit",
            "ascii_scroll",
            "mixed_full",
            "clear_full",
            "cleared_full",
        ] {
            match mode {
                "ascii_full" | "cleared_full" => terminal.invalidate_update(),
                "ascii_edit" => terminal.process_output(b"\x1b[1;1Hy"),
                "ascii_scroll" => terminal.process_output(b"\x1b[999;1H\r\nxy"),
                "mixed_full" => {
                    for row in 1..=rows {
                        terminal.put_str(&format!(
                            "\x1b[{row};1H\x1b[41;4:3;9;53m{}",
                            "┼".repeat(cols)
                        ));
                    }
                    terminal.put_str("\x1b[0m");
                    terminal.invalidate_update();
                }
                "clear_full" => terminal.process_output(b"\x1b[0m\x1b[2J"),
                _ => unreachable!(),
            }
            let update = terminal.read_update(Instant::now());
            let ranges = update_dirty_ranges(&update);
            let snap = &update.snapshot;
            let viewport = renderer.viewport();
            let tint = update.appearance.clear_rgba(update.backdrop_available);
            let actual = [
                buffer_write_bytes(|| {
                    renderer
                        .background
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport, tint)
                }),
                buffer_write_bytes(|| {
                    renderer
                        .boxes
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport)
                }),
                buffer_write_bytes(|| {
                    renderer
                        .text
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport, None)
                }),
                buffer_write_bytes(|| {
                    renderer
                        .decoration
                        .prepare_with_dirty(gpu, snap, &ranges, &viewport)
                }),
            ];
            println!(
                "grid={cols}x{rows} mode={mode} bg_box_text_decoration_bytes={actual:?} total={}",
                actual.iter().sum::<usize>()
            );
            assert!(actual.iter().sum::<usize>() <= rows * cols * 6672);
            if matches!(
                mode,
                "ascii_full" | "ascii_edit" | "ascii_scroll" | "cleared_full"
            ) {
                assert_eq!(
                    (actual[1], actual[3]),
                    (0, 0),
                    "empty slots need no writes: {mode}"
                );
            } else {
                assert_eq!(actual[1], rows * cols * 3744, "{mode}");
                assert_eq!(actual[3], rows * cols * 2592, "{mode}");
            }
            assert!(terminal.acknowledge_update(&update));
            queue.submit([]);
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
    }
}

#[test]
#[ignore = "gpu: sparse slot clearing and capacity reuse pixel readback"]
fn gpu_sparse_slots_clear_hidden_tail_after_shrink_and_regrow() {
    use crate::render::{observations::buffer_write_bytes, text::sequence_tests::read_draw};
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = || {
        harbor_text::load_system_fonts(&harbor_config::FontSettings {
            family: Some("Consolas".into()),
            size: 16.0,
        })
        .unwrap()
    };
    let metrics = TextMetrics::from_font_metrics(fonts().font_metrics());
    let mut terminal = Terminal::new_headless(4, 16);
    terminal.put_str("\x1b[?1049h\x1b[4;12H\x1b[41;4:3;9;53m┼┼┼\x1b[0m");
    let first = terminal.read_update(Instant::now());
    let mut renderer =
        TerminalRenderPipeline::new(gpu, (256, 128), fonts(), metrics, &first).unwrap();
    assert!(renderer.prepare(gpu, &first, false));
    assert!(terminal.acknowledge_update(&first));
    // Rectangular alternate-screen resize discards the last rows. The GPU allocation
    // retains their old nonzero slots while the current grid is smaller.
    for (rows, cols) in [(2, 16), (4, 8), (4, 16), (5, 20), (4, 16)] {
        terminal.resize_if_changed(crate::TerminalSize { rows, cols });
        terminal.process_output(b"\x1b[0m\x1b[2J");
        let update = terminal.read_update(Instant::now());
        assert!(renderer.prepare(gpu, &update, false));
        let mut fresh =
            TerminalRenderPipeline::new(gpu, (256, 128), fonts(), metrics, &update).unwrap();
        assert!(fresh.prepare(gpu, &update, false));
        assert_eq!(
            read_draw(&device, &queue, |p| renderer.draw(p)),
            read_draw(&device, &queue, |p| fresh.draw(p)),
            "old geometry after resize to {cols}x{rows}"
        );
        assert!(terminal.acknowledge_update(&update));
        terminal.invalidate_update();
        let empty = terminal.read_update(Instant::now());
        let view = renderer.viewport();
        renderer.boxes.invalidate_projection();
        assert_eq!(
            buffer_write_bytes(|| renderer.boxes.prepare_with_dirty(
                gpu,
                &empty.snapshot,
                &[],
                &view
            )),
            0
        );
        renderer.decoration.invalidate_projection();
        assert_eq!(
            buffer_write_bytes(|| renderer.decoration.prepare_with_dirty(
                gpu,
                &empty.snapshot,
                &[],
                &view
            )),
            0
        );
        assert!(terminal.acknowledge_update(&empty));
    }
}
