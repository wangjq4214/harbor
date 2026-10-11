//! Integration tests over real native fonts, coherent engine updates and GPU readback.
use super::*;
use crate::{Terminal, TerminalRenderPipeline};
use std::time::Instant;

pub(crate) fn test_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("NOT RUN: no GPU adapter for emoji encode/readback");
        return None;
    };
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
    assert!(layer.overlay_vertex_count > 0);
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
    assert_ne!(
        pipeline.text.fonts.presentation_generation().session,
        before.session
    );
    assert!(pipeline.metrics().cell_width > metrics.cell_width);
    assert!(
        pipeline
            .text
            .sequences
            .atlas
            .entries
            .keys()
            .all(|r| r.dpi.get() == 192.0 && r.size.get() == 16.0)
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
    let mut snap = engine.read_update(Instant::now()).snapshot;
    let (mut layer, viewport) = text(gpu, &snap);
    assert!(layer.sequences.tile_cells.contains(&0));
    // A mark on a tile-represented cell must not be painted a second time.
    snap.cells[0].suffix.push('\u{301}');
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    assert_eq!(layer.overlay_vertex_count, 0);
    snap.cells[0].attrs.set(CellAttrs::CONCEAL);
    layer.invalidate_projection();
    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, None);
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    assert!(layer.sequences.tile_cells.is_empty());
    assert_eq!(layer.overlay_vertex_count, 0);
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
    let mut snap = engine.read_update(Instant::now()).snapshot;
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
    assert!(layer.overlay_vertex_count > 0);
    let actual = read_draw(&device, &queue, |p| layer.draw(p));
    // An explicit scalar overlay drawn after a sequence-only frame is the reference.
    let expected = read_draw(&device, &queue, |pass| {
        layer.sequences.draw(pass);
        pass.set_pipeline(&layer.pipeline);
        pass.set_bind_group(0, &layer.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, layer.overlay_vertex_buffer.slice(..));
        pass.draw(0..layer.overlay_vertex_count, 0..1);
    });
    assert_eq!(actual, expected);
    assert_ne!(actual, before);
}
