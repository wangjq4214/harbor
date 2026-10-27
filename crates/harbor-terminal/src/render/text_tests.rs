//! Integration tests over real native fonts, coherent engine updates and GPU readback.
use super::*;
use crate::{Terminal, TerminalRenderPipeline};
use std::time::Instant;

pub(crate) fn test_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    // Opt in to the same DX12 software renderer used by Windows CI, even on
    // developer machines where the default adapter is a Vulkan hardware GPU.
    let warp = match std::env::var("HARBOR_TEST_GPU").as_deref() {
        Ok("dx12-warp") => true,
        Err(std::env::VarError::NotPresent) => false,
        other => panic!("unsupported HARBOR_TEST_GPU: {other:?}"),
    };
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    if warp {
        descriptor.backends = wgpu::Backends::DX12;
    }
    let instance = wgpu::Instance::new(descriptor);
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: warp,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) if warp => panic!("requested DX12 software adapter unavailable: {error}"),
        Err(_) => {
            eprintln!("NOT RUN: no GPU adapter for emoji encode/readback");
            return None;
        }
    };
    if warp {
        let info = adapter.get_info();
        assert_eq!(info.backend, wgpu::Backend::Dx12);
        assert_eq!(info.device_type, wgpu::DeviceType::Cpu);
    }
    eprintln!("emoji test adapter: {:?}", adapter.get_info());
    Some(
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("GPU device"),
    )
}

pub(crate) fn read_draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    draw: impl FnOnce(&mut wgpu::RenderPass),
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("emoji readback target"),
        size: wgpu::Extent3d {
            width: 256,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("emoji readback"),
        size: 256 * 128 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("emoji test pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.25,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(128),
            },
        },
        wgpu::Extent3d {
            width: 256,
            height: 128,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    pixels
}

fn fonts() -> FontBook {
    harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 16.0,
    })
    .unwrap()
}
fn text(gpu: TerminalGpuAccess<'_>, snap: &TerminalSnapshot) -> (Text, RenderViewport) {
    let f = fonts();
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let viewport = RenderViewport::with_surface(
        metrics.cell_width,
        metrics.line_height,
        (256, 128),
        (256, 128),
    );
    (
        Text::new(gpu, f, metrics, snap, &viewport, Palette::default()).unwrap(),
        viewport,
    )
}

#[test]
fn ordinary_only_keeps_r8_and_never_shapes_or_allocates_color_resources() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("ASCII 中 e\u{301}");
    let update = engine.read_update(Instant::now());
    let (mut layer, viewport) = text(gpu, &update.snapshot);
    layer.prepare_with_dirty(
        gpu,
        &update.snapshot,
        &update.snapshot.dirty_ranges,
        &viewport,
        None,
    );
    layer.prepare_preedit(
        gpu,
        Some(&Preedit::new("input", None)),
        &update.snapshot,
        &viewport,
    );
    let stats = layer.presentation_stats();
    assert_eq!(stats.native.shape_calls, 0);
    assert_eq!(stats.native.raster_calls, 0);
    assert_eq!(stats.native.offscreen_creations, 0);
    assert_eq!(stats.color_atlas_bytes, 0);
    assert_eq!(stats.cached_requests, 0);
    assert!(layer.sequences.emoji_cells.is_empty());
    assert!(layer.overlay.count() > 0);
}

#[test]
#[ignore = "requires supporting Windows Segoe UI Emoji/native color capabilities and GPU; run explicitly"]
fn fragmented_heart_and_zwj_replace_old_pixels_and_converge_after_erase() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for chunks in [vec!["♥", "\u{fe0f}"], vec!["👩", "\u{200d}", "💻"]] {
        let mut engine = Terminal::new_headless(4, 16);
        engine.process_output(b"\x1b[1;4HN\x1b[1;1H");
        let initial = engine.read_update(Instant::now());
        let (mut layer, viewport) = text(gpu, &initial.snapshot);
        engine.acknowledge_update(&initial);
        let mut previous = read_draw(&device, &queue, |p| layer.draw(p));
        for chunk in chunks {
            engine.process_output(chunk.as_bytes());
            let update = engine.read_update(Instant::now());
            assert!(
                update
                    .snapshot
                    .dirty_ranges
                    .iter()
                    .any(|r| r.row == 0 && r.start_col == 0 && r.end_col >= 1)
            );
            assert_eq!(
                engine.read_update(Instant::now()).snapshot.cells,
                update.snapshot.cells,
                "skipped draw retains source"
            );
            layer.prepare_with_dirty(
                gpu,
                &update.snapshot,
                &update.snapshot.dirty_ranges,
                &viewport,
                None,
            );
            layer.prepare_preedit(gpu, None, &update.snapshot, &viewport);
            let actual = read_draw(&device, &queue, |p| layer.draw(p));
            let (mut fresh, _) = text(gpu, &update.snapshot);
            fresh.prepare_preedit(gpu, None, &update.snapshot, &viewport);
            let full = read_draw(&device, &queue, |p| fresh.draw(p));
            assert_eq!(actual, full, "incremental/full converge for {chunk:?}");
            assert_eq!(update.snapshot.cell(0, 3).ch, 'N');
            if update.snapshot.cell(0, 0).raw_text() == "♥️"
                || update.snapshot.cell(0, 0).raw_text() == "👩‍💻"
            {
                assert!(
                    layer.sequences.tile_cells.contains(&0),
                    "named installed Windows fixture must produce a complete tile"
                );
                assert!(layer.sequences.atlas.entries.values().any(|e| e.outcome
                    == super::super::color_atlas::CachedOutcome::Complete(
                        harbor_text::CompleteKind::Color
                    )));
                assert_ne!(actual, previous, "complete source replaces prior artwork");
                let request = layer
                    .sequences
                    .atlas
                    .entries
                    .keys()
                    .find(|r| r.source == update.snapshot.cell(0, 0).raw_text())
                    .unwrap();
                eprintln!(
                    "complete request {}: {:?}",
                    request.source, layer.sequences.atlas.entries[request].outcome
                );
                let calls = layer.fonts.sequence_cache_stats();
                layer.prepare_with_dirty(
                    gpu,
                    &update.snapshot,
                    &update.snapshot.dirty_ranges,
                    &viewport,
                    None,
                );
                assert_eq!(
                    layer.fonts.sequence_cache_stats().shape_calls,
                    calls.shape_calls
                );
                assert_eq!(
                    layer.fonts.sequence_cache_stats().raster_calls,
                    calls.raster_calls
                );
            }
            previous = actual;
            engine.acknowledge_update(&update);
        }
        engine.process_output(b"\x1b[1;1H\x1b[2K");
        let erase = engine.read_update(Instant::now());
        layer.prepare_with_dirty(
            gpu,
            &erase.snapshot,
            &erase.snapshot.dirty_ranges,
            &viewport,
            None,
        );
        layer.prepare_preedit(gpu, None, &erase.snapshot, &viewport);
        let (mut fresh, _) = text(gpu, &erase.snapshot);
        fresh.prepare_preedit(gpu, None, &erase.snapshot, &viewport);
        assert_eq!(
            read_draw(&device, &queue, |p| layer.draw(p)),
            read_draw(&device, &queue, |p| fresh.draw(p))
        );
        assert!(layer.sequences.tile_cells.is_empty());
    }
}

#[test]
fn dpi_transitions_preserve_physical_metrics_and_scalar_pixels() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("ASCII e\u{301}\r\n\x1b[4;9mstyled\x1b[0m");
    let update = engine.read_update(Instant::now());
    let f = fonts();
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let mut pipeline = TerminalRenderPipeline::new(gpu, (256, 128), f, metrics, &update).unwrap();
    pipeline.prepare(gpu, &update, false);
    let viewport = pipeline.viewport();
    let pixels = read_draw(&device, &queue, |p| pipeline.draw(p));
    let atlas_pixels = pipeline.text.atlas.pixels().to_vec();
    let vertices = pipeline
        .text
        .build_all_vertices(&update.snapshot, &viewport);
    let scalar_pipeline = pipeline.text.pipeline.clone();
    let scalar_texture = pipeline.text.gpu_atlas._texture.clone();
    let scalar_buffer = pipeline.text.vertex_buffer.clone();
    assert!(engine.acknowledge_update(&update));
    let update = engine.read_update(Instant::now());
    for scale in [1.25, 1.5, 2.0, 1.5, 1.0] {
        let generation = pipeline.text.fonts.presentation_generation();
        assert!(pipeline.sync_raster_scale(gpu, scale, &update).unwrap());
        assert_eq!(pipeline.text.pipeline, scalar_pipeline);
        assert_eq!(pipeline.text.gpu_atlas._texture, scalar_texture);
        assert_eq!(pipeline.text.vertex_buffer, scalar_buffer);
        pipeline.prepare(gpu, &update, false);
        assert_eq!(metric_values(pipeline.metrics()), metric_values(&metrics));
        assert_eq!(pipeline.viewport(), viewport);
        assert_eq!(pipeline.text.fonts.size(), 16.0);
        let current_generation = pipeline.text.fonts.presentation_generation();
        assert_eq!(current_generation.session, generation.session);
        assert!(current_generation.revision > generation.revision);
        assert!(
            pipeline.text.atlas.pixels() == atlas_pixels,
            "CPU scalar atlas changed at scale {scale}"
        );
        let current_vertices = pipeline
            .text
            .build_all_vertices(&update.snapshot, &viewport);
        assert!(
            bytemuck::cast_slice::<_, u8>(&current_vertices)
                == bytemuck::cast_slice::<_, u8>(&vertices),
            "scalar vertices changed at scale {scale}"
        );
        assert_pixels_equal(
            &read_draw(&device, &queue, |p| pipeline.draw(p)),
            &pixels,
            scale,
        );
        let stats = pipeline.text.presentation_stats();
        assert_eq!(stats.native.shapes, 0);
        assert_eq!(stats.color_atlas_bytes, 0);
        assert!(!pipeline.sync_raster_scale(gpu, scale, &update).unwrap());
        assert_eq!(
            pipeline.text.fonts.presentation_generation(),
            current_generation
        );
    }
    let generation = pipeline.text.fonts.presentation_generation();
    for scale in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
        assert!(pipeline.sync_raster_scale(gpu, scale, &update).is_err());
        assert_eq!(pipeline.raster_scale(), 1.0);
        assert_eq!(pipeline.text.fonts.presentation_generation(), generation);
        assert_eq!(pipeline.text.pipeline, scalar_pipeline);
    }
}

// Keep byte-exact GPU comparisons, but report bounded diagnostics instead of
// dumping two 128 KiB images into the Actions log.
fn assert_pixels_equal(actual: &[u8], expected: &[u8], scale: f32) {
    assert_eq!(actual.len(), expected.len());
    let differences: Vec<_> = actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0.iter())
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .collect();
    let max_delta = actual
        .iter()
        .zip(expected)
        .map(|(actual, expected)| actual.abs_diff(*expected))
        .max()
        .unwrap_or(0);
    let samples: Vec<_> = differences
        .iter()
        .take(8)
        .map(|(index, (actual, expected))| (index % 256, index / 256, actual, expected))
        .collect();
    assert!(
        differences.is_empty(),
        "pixels changed at scale {scale}: {} pixels, max channel delta {max_delta}, \
         first (x, y, actual, expected): {samples:?}",
        differences.len()
    );
}

fn metric_values(metrics: &TextMetrics) -> [f32; 7] {
    [
        metrics.cell_width,
        metrics.line_height,
        metrics.ascent,
        metrics.underline_position,
        metrics.underline_thickness,
        metrics.strikethrough_position,
        metrics.strikethrough_thickness,
    ]
}

#[test]
fn appearance_dpi_and_font_session_replace_tiles_without_mutating_engine_source() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("♥️ 👩‍💻");
    let update = engine.read_update(Instant::now());
    let f = fonts();
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let mut pipeline = TerminalRenderPipeline::new(gpu, (256, 128), f, metrics, &update).unwrap();
    pipeline.prepare(gpu, &update, false);
    let before = pipeline.text.fonts.presentation_generation();
    // A DPI-only update must refresh sequence caches without engine damage.
    assert!(engine.acknowledge_update(&update));
    let update = engine.read_update(Instant::now());
    let palette = Palette {
        foreground: harbor_config::Rgba::from_rgba8(0, 255, 0, 128),
        ..Palette::default()
    };
    pipeline.text.set_palette(palette);
    pipeline
        .text
        .prepare_with_dirty(gpu, &update.snapshot, &[], &pipeline.viewport(), None);
    assert!(
        pipeline
            .text
            .sequences
            .atlas
            .entries
            .keys()
            .all(|r| r.foreground == [0, 255, 0, 128])
    );
    assert!(pipeline.sync_raster_scale(gpu, 2.0, &update).unwrap());
    pipeline.prepare(gpu, &update, false);
    let after_dpi = pipeline.text.fonts.presentation_generation();
    assert_eq!(after_dpi.session, before.session);
    assert!(after_dpi.revision > before.revision);
    assert!(
        !pipeline
            .text
            .sequences
            .atlas_generation_changed(&pipeline.text.fonts)
    );
    assert_eq!(metric_values(pipeline.metrics()), metric_values(&metrics));
    assert_eq!(pipeline.text.fonts.size(), 16.0);
    assert!(!pipeline.text.sequences.atlas.entries.is_empty());
    assert!(
        pipeline
            .text
            .sequences
            .atlas
            .entries
            .keys()
            .all(|r| r.dpi.get() == 192.0 && r.size.get() == 8.0 && r.raster_size() == 16.0)
    );
    assert!(!pipeline.sync_raster_scale(gpu, 2.0, &update).unwrap());
    let generation = pipeline.text.fonts.presentation_generation();
    pipeline.text.fonts.invalidate_presentations();
    pipeline
        .text
        .prepare_with_dirty(gpu, &update.snapshot, &[], &pipeline.viewport(), None);
    assert!(pipeline.text.fonts.presentation_generation().revision > generation.revision);
    pipeline.replace_fonts(gpu, fonts(), &update).unwrap();
    pipeline.prepare(gpu, &update, false);
    assert_ne!(
        pipeline.text.fonts.presentation_generation().session,
        generation.session
    );
    assert_eq!(pipeline.raster_scale(), 2.0);
    assert_eq!(metric_values(pipeline.metrics()), metric_values(&metrics));
    assert_eq!(pipeline.text.fonts.size(), 16.0);
    assert!(
        pipeline
            .text
            .sequences
            .atlas
            .entries
            .keys()
            .all(|r| r.raster_size() == 16.0)
    );
    assert!(pipeline.sync_raster_scale(gpu, f32::NAN, &update).is_err());
    assert_eq!(
        engine.read_update(Instant::now()).snapshot.cells,
        update.snapshot.cells
    );
    read_draw(&device, &queue, |p| pipeline.draw(p));
}

#[test]
#[ignore = "requires supporting Windows Segoe UI Emoji/native color capabilities and GPU; run explicitly"]
fn complete_tiles_suppress_combining_overlays_and_conceal_removes_presentation() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("♥️");
    let mut snap = (*engine.read_update(Instant::now()).snapshot).clone();
    let (mut layer, viewport) = text(gpu, &snap);
    assert!(layer.sequences.tile_cells.contains(&0));
    // A mark on a tile-represented cell must not be painted a second time.
    snap.cells[0].suffix.push('\u{301}');
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    assert_eq!(layer.overlay.count(), 0);
    snap.cells[0].attrs.set(CellAttrs::CONCEAL);
    layer.invalidate_projection();
    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, None);
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    assert!(layer.sequences.tile_cells.is_empty());
    assert_eq!(layer.overlay.count(), 0);
    assert_eq!(
        read_draw(&device, &queue, |p| layer.draw(p)),
        read_draw(&device, &queue, |_| {})
    );
}

#[test]
fn unsupported_sequence_keeps_a_fitted_leading_glyph_and_source_then_overwrites_cleanly() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("👩");
    let mut snap = (*engine.read_update(Instant::now()).snapshot).clone();
    snap.cells[0].suffix = "\u{200d}A".into();
    let (mut layer, viewport) = text(gpu, &snap);
    assert!(layer.sequences.emoji_cells.contains(&0));
    assert!(
        !layer.sequences.tile_cells.contains(&0),
        "usable leading glyph fallback uses R8"
    );
    assert!(layer.sequences.atlas.entries.values().any(|e| matches!(
        e.outcome,
        super::super::color_atlas::CachedOutcome::Unsupported(_)
    )));
    let actual = read_draw(&device, &queue, |p| layer.draw(p));
    assert_ne!(actual, read_draw(&device, &queue, |_| {}));
    assert_eq!(snap.cells[0].raw_text(), "👩‍A");
    let base = layer.build_all_vertices(&snap, &viewport);
    for v in &base[..6] {
        let x = (v.position[0] + 1.0) * 128.0;
        let y = (1.0 - v.position[1]) * 64.0;
        assert!((15.99..=34.01).contains(&x) && (15.99..=35.01).contains(&y));
    }
    snap.cells[0].ch = 'x';
    snap.cells[0].suffix.clear();
    snap.cells[0].width = 1;
    snap.cells[1] = Cell::default();
    layer.prepare_with_dirty(
        gpu,
        &snap,
        &[DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 2,
        }],
        &viewport,
        None,
    );
    let (fresh, _) = text(gpu, &snap);
    assert_eq!(
        read_draw(&device, &queue, |p| layer.draw(p)),
        read_draw(&device, &queue, |p| fresh.draw(p))
    );
}

#[test]
#[ignore = "requires supporting Windows Segoe UI Emoji/native color capabilities and GPU; run explicitly"]
fn preedit_paints_above_complete_tiles_at_an_overlapping_cursor() {
    let Some((device, queue)) = test_gpu() else {
        return;
    };
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("♥️");
    engine.process_output(b"\x1b[1;1H");
    let snap = engine.read_update(Instant::now()).snapshot;
    let (mut layer, viewport) = text(gpu, &snap);
    let before = read_draw(&device, &queue, |p| layer.draw(p));
    let preedit = Preedit::new("M", None);
    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, Some(&preedit));
    layer.prepare_preedit(gpu, Some(&preedit), &snap, &viewport);
    assert!(layer.overlay.count() > 0);
    let actual = read_draw(&device, &queue, |p| layer.draw(p));
    // An explicit scalar overlay drawn after a sequence-only frame is the reference.
    let expected = read_draw(&device, &queue, |pass| {
        layer.sequences.draw(pass);
        pass.set_pipeline(&layer.pipeline);
        pass.set_bind_group(0, &layer.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, layer.overlay.buffer().slice(..));
        pass.draw(0..layer.overlay.count(), 0..1);
    });
    assert_eq!(actual, expected);
    assert_ne!(actual, before);
}

#[test]
fn t0002_native_changed_metrics_replacement_and_failed_preparation_are_coherent() {
    let (device, queue) = test_gpu().expect("BLOCKED: T0002 requires an actual GPU adapter");
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(3, 10);
    engine.put_str("\x1b[4:1;9;53mAX 界\r\n\x1b[4:3;58;2;255;0;0mABC\r\n\x1b[4:2;59mXYZ");
    let initial = engine.read_update(Instant::now());
    let f = fonts();
    let m = TextMetrics::from_font_metrics(f.font_metrics());
    let mut pipeline = TerminalRenderPipeline::new(gpu, (256, 128), f, m, &initial).unwrap();
    let view = RenderViewport {
        allocation_origin: (3.25, 4.75),
        ..pipeline.viewport()
    };
    pipeline.sync_viewport(view, false);
    pipeline.prepare(gpu, &initial, false);
    assert!(engine.acknowledge_update(&initial));
    let update = engine.read_update(Instant::now());
    assert!(
        matches!(&update.damage, crate::model::UpdateDamage::Ranges(ranges) if ranges.is_empty())
    );
    let source = update.snapshot.cells.clone();
    let before_text = read_draw(&device, &queue, |pass| pipeline.text.draw(pass));
    let before_decoration = read_draw(&device, &queue, |pass| pipeline.decoration.draw(pass));
    let before_all = read_draw(&device, &queue, |pass| pipeline.draw(pass));
    let generation = pipeline.text.fonts.presentation_generation();
    let old_text_pipeline = pipeline.text.pipeline.clone();
    // Same-metrics replacement changes session/resources but not geometry/pixels.
    pipeline.replace_fonts(gpu, fonts(), &update).unwrap();
    assert_eq!(metric_values(pipeline.metrics()), metric_values(&m));
    assert_ne!(
        pipeline.text.fonts.presentation_generation().session,
        generation.session
    );
    assert_ne!(pipeline.text.pipeline, old_text_pipeline);
    assert_eq!(
        read_draw(&device, &queue, |pass| pipeline.draw(pass)),
        before_all
    );
    let atlas = pipeline.text.gpu_atlas._texture.clone();
    let buffer = pipeline.text.vertex_buffer.clone();
    let scalar_pipeline = pipeline.text.pipeline.clone();
    // Even extremely small/large accepted DPI identities leave physical pixels alone.
    for scale in [f32::from_bits(1), 1.25, 1.5, 2.0, f32::MAX / 192.0, 1.0] {
        assert!(pipeline.sync_raster_scale(gpu, scale, &update).unwrap());
        assert_eq!(pipeline.viewport(), view);
        assert_eq!(metric_values(pipeline.metrics()), metric_values(&m));
        assert_eq!(pipeline.text.gpu_atlas._texture, atlas);
        assert_eq!(pipeline.text.vertex_buffer, buffer);
        assert_eq!(pipeline.text.pipeline, scalar_pipeline);
        assert_eq!(
            read_draw(&device, &queue, |pass| pipeline.draw(pass)),
            before_all
        );
    }
    let generation = pipeline.text.fonts.presentation_generation();
    for scale in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        -1.0,
        f32::MAX,
    ] {
        assert!(pipeline.sync_raster_scale(gpu, scale, &update).is_err());
        assert_eq!(pipeline.raster_scale(), 1.0);
        assert_eq!(pipeline.text.fonts.presentation_generation(), generation);
        assert_eq!(pipeline.text.gpu_atlas._texture, atlas);
    }
    let changed_fonts = || {
        harbor_text::load_system_fonts(&harbor_config::FontSettings {
            family: Some("Consolas".into()),
            size: 23.0,
        })
        .expect("native changed-size font session")
    };
    let candidate = changed_fonts();
    let changed_metrics = TextMetrics::from_font_metrics(candidate.font_metrics());
    assert_ne!(
        m.cell_width, changed_metrics.cell_width,
        "must genuinely change metrics"
    );
    assert_ne!(m.line_height, changed_metrics.line_height);
    assert_ne!(m.ascent, changed_metrics.ascent);
    eprintln!(
        "T0002 native Consolas 16 -> 23 metrics: {:?} -> {:?}",
        metric_values(&m),
        metric_values(&changed_metrics)
    );
    pipeline.sync_raster_scale(gpu, 1.5, &update).unwrap();
    pipeline.replace_fonts(gpu, candidate, &update).unwrap();
    assert_eq!(pipeline.raster_scale(), 1.5);
    assert_eq!(pipeline.text.raster_scale, 1.5);
    assert_eq!(
        pipeline.viewport().allocation_origin,
        view.allocation_origin
    );
    assert_eq!(
        metric_values(pipeline.metrics()),
        metric_values(&changed_metrics)
    );
    assert_eq!(pipeline.viewport().cell_width, changed_metrics.cell_width);
    assert_eq!(pipeline.viewport().line_height, changed_metrics.line_height);
    assert_ne!(pipeline.text.gpu_atlas._texture, atlas);
    assert_ne!(pipeline.text.vertex_buffer, buffer);
    assert_ne!(pipeline.text.pipeline, scalar_pipeline);
    assert_ne!(
        read_draw(&device, &queue, |pass| pipeline.text.draw(pass)),
        before_text
    );
    assert_ne!(
        read_draw(&device, &queue, |pass| pipeline.decoration.draw(pass)),
        before_decoration
    );
    assert_ne!(
        read_draw(&device, &queue, |pass| pipeline.draw(pass)),
        before_all
    );
    let mut fresh =
        TerminalRenderPipeline::new(gpu, (256, 128), changed_fonts(), changed_metrics, &update)
            .unwrap();
    fresh.sync_raster_scale(gpu, 1.5, &update).unwrap();
    fresh.sync_viewport(pipeline.viewport(), false);
    fresh.prepare(gpu, &update, false);
    for layer in 0..3 {
        let draw = |p: &TerminalRenderPipeline, pass: &mut wgpu::RenderPass| match layer {
            0 => p.text.draw(pass),
            1 => p.decoration.draw(pass),
            _ => p.draw(pass),
        };
        assert_eq!(
            read_draw(&device, &queue, |pass| draw(&pipeline, pass)),
            read_draw(&device, &queue, |pass| draw(&fresh, pass)),
            "changed resource layer {layer}"
        );
    }
    // Capture all public presentation and scalar resource identity before errors.
    let valid_view = pipeline.viewport();
    let valid_metrics = metric_values(pipeline.metrics());
    let valid_atlas = pipeline.text.gpu_atlas._texture.clone();
    let valid_pipeline = pipeline.text.pipeline.clone();
    let valid_buffer = pipeline.text.vertex_buffer.clone();
    let valid_generation = pipeline.text.fonts.presentation_generation();
    let valid_pixels = read_draw(&device, &queue, |pass| pipeline.draw(pass));
    // Rejected resource preparation: malformed/oversized grids fail before publish.
    for (rows, cols) in [(0, 10), (usize::MAX, 10), (3, usize::MAX)] {
        let mut invalid = update.clone();
        std::sync::Arc::make_mut(&mut invalid.snapshot).rows = rows;
        std::sync::Arc::make_mut(&mut invalid.snapshot).cols = cols;
        assert!(pipeline.replace_fonts(gpu, fonts(), &invalid).is_err());
        assert_eq!(pipeline.viewport(), valid_view);
        assert_eq!(metric_values(pipeline.metrics()), valid_metrics);
        assert_eq!(pipeline.text.gpu_atlas._texture, valid_atlas);
        assert_eq!(pipeline.text.pipeline, valid_pipeline);
        assert_eq!(pipeline.text.vertex_buffer, valid_buffer);
        assert_eq!(
            pipeline.text.fonts.presentation_generation(),
            valid_generation
        );
        assert_eq!(
            read_draw(&device, &queue, |pass| pipeline.draw(pass)),
            valid_pixels
        );
        assert!(!pipeline.prepare(gpu, &invalid, false));
        assert!(pipeline.sync_raster_scale(gpu, 2.0, &invalid).is_err());
        assert_eq!(pipeline.raster_scale(), 1.5);
        assert_eq!(
            pipeline.text.fonts.presentation_generation(),
            valid_generation
        );
    }
    // Force a real GPU resource-validation error after CPU preflight, without
    // losing the device: a depth format is not a valid terminal color target.
    let invalid_gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Depth32Float);
    let error = pipeline
        .replace_fonts(invalid_gpu, fonts(), &update)
        .expect_err("GPU validation must reject candidate");
    assert!(error.to_string().contains("resource preparation failed"));
    assert_eq!(pipeline.viewport(), valid_view);
    assert_eq!(metric_values(pipeline.metrics()), valid_metrics);
    assert_eq!(pipeline.text.gpu_atlas._texture, valid_atlas);
    assert_eq!(pipeline.text.pipeline, valid_pipeline);
    assert_eq!(pipeline.text.vertex_buffer, valid_buffer);
    assert_eq!(
        pipeline.text.fonts.presentation_generation(),
        valid_generation
    );
    assert_eq!(
        read_draw(&device, &queue, |pass| pipeline.draw(pass)),
        valid_pixels
    );
    for invalid in [
        RenderViewport {
            line_height: f32::NAN,
            ..valid_view
        },
        RenderViewport {
            cell_width: -1.0,
            ..valid_view
        },
        RenderViewport {
            padding: f32::MAX,
            ..valid_view
        },
        RenderViewport {
            allocation_origin: (f32::INFINITY, 0.0),
            ..valid_view
        },
        RenderViewport {
            surface_size: (0, 128),
            ..valid_view
        },
        RenderViewport {
            allocation_size: (256, 0),
            ..valid_view
        },
    ] {
        assert!(!pipeline.sync_viewport(invalid, true));
        assert_eq!(pipeline.viewport(), valid_view);
        assert_eq!(
            read_draw(&device, &queue, |pass| pipeline.draw(pass)),
            valid_pixels
        );
    }
    // New allocation/origin/surface actually reprojects despite empty engine damage.
    let shifted = RenderViewport {
        allocation_origin: (7.5, 1.25),
        allocation_size: (230, 120),
        surface_size: (240, 120),
        ..valid_view
    };
    pipeline.sync_viewport(shifted, false);
    assert!(pipeline.decoration.is_dirty());
    pipeline.prepare(gpu, &update, false);
    fresh.sync_viewport(shifted, false);
    fresh.prepare(gpu, &update, false);
    assert_eq!(
        read_draw(&device, &queue, |pass| pipeline.draw(pass)),
        read_draw(&device, &queue, |pass| fresh.draw(pass))
    );
    assert_ne!(
        read_draw(&device, &queue, |pass| pipeline.draw(pass)),
        valid_pixels
    );
    assert_eq!(engine.snapshot().cells, source);
    assert!(
        engine
            .read_update(Instant::now())
            .snapshot
            .dirty_ranges
            .is_empty()
    );
    // Invalid public metrics and non-drawable initial targets must reject safely.
    for field in 0..7 {
        for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut bad = m;
            *match field {
                0 => &mut bad.cell_width,
                1 => &mut bad.line_height,
                2 => &mut bad.ascent,
                3 => &mut bad.underline_thickness,
                4 => &mut bad.strikethrough_thickness,
                5 => &mut bad.underline_position,
                _ => &mut bad.strikethrough_position,
            } = value;
            // Finite offsets, including zero/negative ones, are valid clamped placements.
            if super::super::decoration::valid_metrics(&bad) {
                continue;
            }
            assert!(TerminalRenderPipeline::new(gpu, (256, 128), fonts(), bad, &update).is_err());
        }
    }
    for bad in [
        TextMetrics {
            cell_width: f32::MAX,
            ..m
        },
        TextMetrics {
            line_height: f32::MAX,
            ..m
        },
        TextMetrics {
            ascent: f32::MAX,
            ..m
        },
    ] {
        assert!(TerminalRenderPipeline::new(gpu, (256, 128), fonts(), bad, &update).is_err());
    }
    assert!(TerminalRenderPipeline::new(gpu, (0, 128), fonts(), m, &update).is_err());
    eprintln!(
        "PASS: T0002 DirectWrite changed-metrics GPU replacement, same-metrics session, scale-only physical pixels, rejected preparation retains metrics/atlas/resources/pixels"
    );
}
