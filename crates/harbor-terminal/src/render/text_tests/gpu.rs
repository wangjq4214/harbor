#[path = "overlay.rs"]
mod overlay;

use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn cached_resize_preserves_atlas_and_matches_fresh_with_preedit() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("AB e\u{301}\r\nCD");
    let first = engine.read_update(Instant::now());
    let (mut layer, viewport) = text(gpu, &first.snapshot);
    let preedit = Preedit::new("AB", None);
    layer.prepare_with_dirty(gpu, &first.snapshot, &[], &viewport, Some(&preedit));
    let pixels = layer.atlas.pixels().to_vec();
    let glyph = *layer.atlas.glyph_by_char('A').unwrap();
    let bytes_per_cell = 6 * std::mem::size_of::<TexturedVertex>();
    let mut previous_cells = layer.rows * layer.cols;
    let mut previous_buffer = layer.vertex_buffer.clone();
    for (rows, cols, capacity) in [
        (8, 8, 64),
        (3, 12, 64),
        (4, 16, 64),
        (2, 8, 64),
        (5, 20, 100),
    ] {
        engine.resize(rows, cols);
        let update = engine.read_update(Instant::now());
        layer.prepare_with_dirty(gpu, &update.snapshot, &[], &viewport, Some(&preedit));
        assert_eq!(
            crate::render::observations::text_uploads(),
            [(0, rows * cols * bytes_per_cell)]
        );
        assert_eq!((layer.rows, layer.cols), (rows, cols));
        assert_eq!(
            layer.vertex_buffer.size(),
            (capacity * bytes_per_cell) as u64
        );
        if rows * cols > previous_cells {
            assert_ne!(layer.vertex_buffer, previous_buffer);
        } else {
            assert_eq!(layer.vertex_buffer, previous_buffer);
        }
        previous_cells = rows * cols;
        previous_buffer = layer.vertex_buffer.clone();
        assert!(!layer.is_dirty());
        layer.prepare_preedit(gpu, Some(&preedit), &update.snapshot, &viewport);
        assert_eq!(layer.atlas.pixels(), pixels);
        let now = layer.atlas.glyph_by_char('A').unwrap();
        assert_eq!((now.atlas_x, now.atlas_y), (glyph.atlas_x, glyph.atlas_y));
        let (mut fresh, _) = text(gpu, &update.snapshot);
        fresh.prepare_with_dirty(gpu, &update.snapshot, &[], &viewport, Some(&preedit));
        fresh.prepare_preedit(gpu, Some(&preedit), &update.snapshot, &viewport);
        assert_eq!(
            read_draw(&device, &queue, |p| layer.draw(p)),
            read_draw(&device, &queue, |p| fresh.draw(p)),
            "mismatch at {rows}x{cols}"
        );
        layer.prepare_with_dirty(gpu, &update.snapshot, &[], &viewport, Some(&preedit));
        assert!(crate::render::observations::text_uploads().is_empty());
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn resize_discovers_history_suffixes_and_preedit_without_damage() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(2, 8);
    engine.put_str("e\u{301}\r\nAB\r\nCD");
    let initial = engine.read_update(Instant::now());
    let (mut layer, viewport) = text(gpu, &initial.snapshot);
    assert!(
        layer.atlas.glyph_by_char('e').is_none(),
        "history starts outside the visible atlas"
    );
    let old = *layer.atlas.glyph_by_char('A').unwrap();
    engine.resize(4, 8);
    let update = engine.read_update(Instant::now());
    let preedit = Preedit::new("Q", None);
    // Deliberately omit damage: new geometry must discover every paint input.
    layer.prepare_with_dirty(gpu, &update.snapshot, &[], &viewport, Some(&preedit));
    layer.prepare_preedit(gpu, Some(&preedit), &update.snapshot, &viewport);
    for ch in ['e', '\u{301}', 'Q'] {
        assert!(
            layer.atlas.glyph_by_char(ch).is_some(),
            "missing required {ch}"
        );
    }
    let new = layer.atlas.glyph_by_char('A').unwrap();
    assert_eq!((new.atlas_x, new.atlas_y), (old.atlas_x, old.atlas_y));
    let (mut fresh, _) = text(gpu, &update.snapshot);
    fresh.prepare_with_dirty(gpu, &update.snapshot, &[], &viewport, Some(&preedit));
    fresh.prepare_preedit(gpu, Some(&preedit), &update.snapshot, &viewport);
    assert_eq!(
        read_draw(&device, &queue, |p| layer.draw(p)),
        read_draw(&device, &queue, |p| fresh.draw(p))
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn scalar_uploads_preserve_incremental_full_and_noop_plans() {
    use crate::render::observations::text_uploads;

    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("AB\r\nAB\r\nAB");
    let mut snap = (*engine.read_update(Instant::now()).snapshot).clone();
    let (mut layer, viewport) = text(gpu, &snap);
    let bytes_per_cell = 6 * std::mem::size_of::<TexturedVertex>();
    let assert_fresh = |layer: &Text, snap: &TerminalSnapshot| {
        let (fresh, _) = text(gpu, snap);
        assert_eq!(
            read_draw(&device, &queue, |p| layer.draw(p)),
            read_draw(&device, &queue, |p| fresh.draw(p))
        );
    };

    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, None);
    assert!(text_uploads().is_empty());
    snap.cells[2 * 16 + 5].ch = 'A';
    snap.cells[2 * 16 + 6].ch = 'B';
    snap.cells[16 + 3].ch = 'Z';
    assert!(layer.atlas.glyph_by_char('Z').is_none());
    let local = [
        DirtyRange {
            row: 2,
            start_col: 5,
            end_col: 7,
        },
        DirtyRange {
            row: 1,
            start_col: 3,
            end_col: 4,
        },
    ];
    layer.prepare_with_dirty(gpu, &snap, &local, &viewport, None);
    assert_eq!(
        text_uploads(),
        [
            ((2 * 16 + 5) * bytes_per_cell, 2 * bytes_per_cell),
            ((16 + 3) * bytes_per_cell, bytes_per_cell)
        ]
    );
    assert!(layer.atlas.glyph_by_char('Z').is_some());
    assert_fresh(&layer, &snap);

    snap.cells[3 * 16 + 2].ch = 'A';
    layer.invalidate_projection();
    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, None);
    assert_eq!(text_uploads(), [(0, 4 * 16 * bytes_per_cell)]);
    assert!(!layer.is_dirty());
    assert_fresh(&layer, &snap);
    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, None);
    assert!(text_uploads().is_empty());

    snap.cells[2].ch = 'B';
    let broad = [
        DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 16,
        },
        DirtyRange {
            row: 1,
            start_col: 0,
            end_col: 16,
        },
    ];
    layer.prepare_with_dirty(gpu, &snap, &broad, &viewport, None);
    assert_eq!(text_uploads(), [(0, 4 * 16 * bytes_per_cell)]);
    assert_fresh(&layer, &snap);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn scalar_repack_invalidates_all_vertex_uvs_even_for_local_damage() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("AB\r\nBA");
    let snap = engine.read_update(Instant::now()).snapshot.clone();
    let (mut layer, viewport) = text(gpu, &snap);
    let before = read_draw(&device, &queue, |p| layer.draw(p));
    let old_positions: Vec<_> = ['A', 'B']
        .iter()
        .map(|&ch| {
            let glyph = layer.atlas.glyph_by_char(ch).unwrap();
            (glyph.atlas_x, glyph.atlas_y)
        })
        .collect();

    // Rebuild real atlas placements, then inject the same invalidation result
    // that pressure-driven eviction supplies to the renderer.
    layer.atlas.rebuild(
        &layer.fonts,
        &[
            'A', 'B', 'Á', 'g', 'm', 'Q', 'j', 'M', 'W', 'x', '~', 'Å', 'É', '中',
        ],
    );
    let new_positions: Vec<_> = ['A', 'B']
        .iter()
        .map(|&ch| {
            let glyph = layer.atlas.glyph_by_char(ch).unwrap();
            (glyph.atlas_x, glyph.atlas_y)
        })
        .collect();
    assert_ne!(old_positions, new_positions);
    let key = layer.atlas.glyph_by_char('A').unwrap().key;
    layer.apply_rasterize_result(
        gpu,
        harbor_text::RasterizeResult {
            new_keys: vec![key],
            evicted: true,
        },
    );
    assert!(layer.is_dirty());
    layer.prepare_with_dirty(
        gpu,
        &snap,
        &[DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 1,
        }],
        &viewport,
        None,
    );
    assert_eq!(
        crate::render::observations::text_uploads(),
        [(0, 4 * 16 * 6 * std::mem::size_of::<TexturedVertex>())]
    );
    assert!(!layer.is_dirty());
    assert_eq!(read_draw(&device, &queue, |p| layer.draw(p)), before);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn mixed_suffix_overlays_match_filtered_marks_and_skip_hidden_cells() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut snap = (*Terminal::new_headless(2, 8)
        .read_update(Instant::now())
        .snapshot)
        .clone();
    let mixed = "\u{301}B\u{fe0f}\u{200d}中\u{e0100}\u{327}\u{7}";
    snap.cells[0].ch = 'e';
    snap.cells[0].suffix = mixed.into();
    snap.cells[1].ch = '\u{301}';
    snap.cells[1].isolated_mark = true;
    snap.cells[1].suffix = "\u{327}B\u{fe0e}\u{200d}".into();
    snap.cells[2] = snap.cells[1].clone();
    snap.cells[2].attrs.set(CellAttrs::CONCEAL);
    snap.cells[3] = snap.cells[1].clone();
    snap.cells[3].wide_continuation = true;
    // Seed the atlas with nonzero-width suffix scalars.
    snap.cells[4].ch = 'B';
    snap.cells[5].ch = '中';
    let (mut layer, viewport) = text(gpu, &snap);
    let spacing_glyph = layer.atlas.glyph_by_char('B').unwrap();
    assert!(spacing_glyph.width > 0 && spacing_glyph.height > 0);
    assert!(!layer.sequences.tile_cells.contains(&0));
    assert!(!layer.sequences.tile_cells.contains(&1));
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    let count = layer.cell_overlays.count();
    assert!(count > 0);
    let mixed_pixels = read_draw(&device, &queue, |p| layer.draw(p));

    let mut expected = Vec::new();
    for (col, glyphs) in [
        (0, &[('\u{301}', true), ('\u{327}', true)][..]),
        (
            1,
            &[('\u{25cc}', false), ('\u{327}', true), ('\u{301}', true)][..],
        ),
    ] {
        let cell = snap.cell(0, col);
        let (x, y) = viewport.cell_pos(0, col);
        let view = OverlayCell {
            x,
            baseline: y + layer.metrics.ascent.ceil(),
            color: glyph_color_with_palette(&layer.palette, cell.fg, cell.bg, cell.attrs),
            surface: viewport.surface_dimensions(),
            width: cell.grid_width(),
        };
        for &(ch, center) in glyphs {
            append_overlay_glyph(
                &layer.atlas,
                &layer.metrics,
                &mut expected,
                ch,
                &view,
                center,
            );
        }
    }
    assert_eq!(count as usize, expected.len());
    let reference_buffer = crate::render::gpu::create_vertex_buffer(&device, &expected);
    let reference = read_draw(&device, &queue, |pass| {
        pass.set_pipeline(&layer.pipeline);
        pass.set_bind_group(0, &layer.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, layer.vertex_buffer.slice(..));
        pass.draw(0..(snap.rows * snap.cols * 6) as u32, 0..1);
        layer.sequences.draw(pass);
        pass.set_pipeline(&layer.pipeline);
        pass.set_bind_group(0, &layer.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, reference_buffer.slice(..));
        pass.draw(0..expected.len() as u32, 0..1);
    });
    assert_eq!(mixed_pixels, reference);

    snap.cells[0].suffix = "\u{301}\u{327}".into();
    snap.cells[1].suffix = "\u{327}".into();
    snap.cells[2].suffix.clear();
    snap.cells[2].isolated_mark = false;
    snap.cells[3].suffix.clear();
    snap.cells[3].isolated_mark = false;
    layer.prepare_with_dirty(
        gpu,
        &snap,
        &[DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 4,
        }],
        &viewport,
        None,
    );
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    assert_eq!(layer.cell_overlays.count(), count);
    assert_eq!(read_draw(&device, &queue, |p| layer.draw(p)), mixed_pixels);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn ordinary_only_keeps_r8_and_never_shapes_or_allocates_color_resources() {
    let (device, queue) = test_gpu();
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
    assert!(layer.cell_overlays.count() + layer.ime_overlay.count() > 0);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn dpi_transitions_preserve_physical_metrics_and_scalar_pixels() {
    let (device, queue) = test_gpu();
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
    let atlas_pixels = pipeline.text_for_test().atlas.pixels().to_vec();
    let vertices = pipeline
        .text_for_test()
        .build_all_vertices(&update.snapshot, &viewport);
    let scalar_pipeline = pipeline.text_for_test().pipeline.clone();
    let scalar_texture = pipeline.text_for_test().gpu_atlas._texture.clone();
    let scalar_buffer = pipeline.text_for_test().vertex_buffer.clone();
    assert!(engine.acknowledge_update(&update));
    let update = engine.read_update(Instant::now());
    for scale in [1.25, 1.5, 2.0, 1.5, 1.0] {
        let generation = pipeline.text_for_test().fonts.presentation_generation();
        assert!(pipeline.sync_raster_scale(gpu, scale, &update).unwrap());
        assert_eq!(pipeline.text_for_test().pipeline, scalar_pipeline);
        assert_eq!(pipeline.text_for_test().gpu_atlas._texture, scalar_texture);
        assert_eq!(pipeline.text_for_test().vertex_buffer, scalar_buffer);
        pipeline.prepare(gpu, &update, false);
        assert_eq!(metric_values(pipeline.metrics()), metric_values(&metrics));
        assert_eq!(pipeline.viewport(), viewport);
        assert_eq!(pipeline.text_for_test().fonts.size(), 16.0);
        let current_generation = pipeline.text_for_test().fonts.presentation_generation();
        assert_eq!(current_generation.session, generation.session);
        assert!(current_generation.revision > generation.revision);
        assert!(
            pipeline.text_for_test().atlas.pixels() == atlas_pixels,
            "CPU scalar atlas changed at scale {scale}"
        );
        let current_vertices = pipeline
            .text_for_test()
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
        let stats = pipeline.presentation_stats();
        assert_eq!(stats.native.shapes, 0);
        assert_eq!(stats.color_atlas_bytes, 0);
        assert!(!pipeline.sync_raster_scale(gpu, scale, &update).unwrap());
        assert_eq!(
            pipeline.text_for_test().fonts.presentation_generation(),
            current_generation
        );
    }
    let generation = pipeline.text_for_test().fonts.presentation_generation();
    for scale in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
        assert!(pipeline.sync_raster_scale(gpu, scale, &update).is_err());
        assert_eq!(pipeline.raster_scale(), 1.0);
        assert_eq!(
            pipeline.text_for_test().fonts.presentation_generation(),
            generation
        );
        assert_eq!(pipeline.text_for_test().pipeline, scalar_pipeline);
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn appearance_dpi_and_font_session_replace_tiles_without_mutating_engine_source() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("♥️ 👩‍💻");
    let update = engine.read_update(Instant::now());
    let f = fonts();
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let mut pipeline = TerminalRenderPipeline::new(gpu, (256, 128), f, metrics, &update).unwrap();
    pipeline.prepare(gpu, &update, false);
    let before = pipeline.text_for_test().fonts.presentation_generation();
    // A DPI-only update must refresh sequence caches without engine damage.
    assert!(engine.acknowledge_update(&update));
    let update = engine.read_update(Instant::now());
    let palette = Palette {
        foreground: harbor_config::Rgba::from_rgba8(0, 255, 0, 128),
        ..Palette::default()
    };
    pipeline.text_mut_for_test().set_palette(palette);
    assert!(pipeline.prepare(gpu, &update, false));
    assert!(
        pipeline
            .text_for_test()
            .sequences
            .atlas
            .entries
            .keys()
            .all(|r| r.foreground == [0, 255, 0, 128])
    );
    assert!(pipeline.sync_raster_scale(gpu, 2.0, &update).unwrap());
    pipeline.prepare(gpu, &update, false);
    let after_dpi = pipeline.text_for_test().fonts.presentation_generation();
    assert_eq!(after_dpi.session, before.session);
    assert!(after_dpi.revision > before.revision);
    assert!(
        !pipeline
            .text_for_test()
            .sequences
            .atlas_generation_changed(&pipeline.text_for_test().fonts)
    );
    assert_eq!(metric_values(pipeline.metrics()), metric_values(&metrics));
    assert_eq!(pipeline.text_for_test().fonts.size(), 16.0);
    assert!(!pipeline.text_for_test().sequences.atlas.entries.is_empty());
    assert!(
        pipeline
            .text_for_test()
            .sequences
            .atlas
            .entries
            .keys()
            .all(|r| r.dpi.get() == 192.0 && r.size.get() == 8.0 && r.raster_size() == 16.0)
    );
    assert!(!pipeline.sync_raster_scale(gpu, 2.0, &update).unwrap());
    let generation = pipeline.text_for_test().fonts.presentation_generation();
    pipeline
        .text_mut_for_test()
        .fonts
        .invalidate_presentations();
    assert!(pipeline.prepare(gpu, &update, false));
    assert!(
        pipeline
            .text_for_test()
            .fonts
            .presentation_generation()
            .revision
            > generation.revision
    );
    pipeline.replace_fonts(gpu, fonts(), &update).unwrap();
    pipeline.prepare(gpu, &update, false);
    assert_ne!(
        pipeline
            .text_for_test()
            .fonts
            .presentation_generation()
            .session,
        generation.session
    );
    assert_eq!(pipeline.raster_scale(), 2.0);
    assert_eq!(metric_values(pipeline.metrics()), metric_values(&metrics));
    assert_eq!(pipeline.text_for_test().fonts.size(), 16.0);
    assert!(
        pipeline
            .text_for_test()
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn unsupported_sequence_keeps_a_fitted_leading_glyph_and_source_then_overwrites_cleanly() {
    let (device, queue) = test_gpu();
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
        crate::render::color_atlas::CachedOutcome::Unsupported(_)
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
#[ignore = "gpu: run the matching explicit test recipe"]
fn t0002_native_changed_metrics_replacement_and_failed_preparation_are_coherent() {
    let (device, queue) = test_gpu();
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
    let before_text = read_draw(&device, &queue, |pass| pipeline.text_for_test().draw(pass));
    let before_decoration = read_draw(&device, &queue, |pass| {
        pipeline.decoration_for_test().draw(pass)
    });
    let before_all = read_draw(&device, &queue, |pass| pipeline.draw(pass));
    let generation = pipeline.text_for_test().fonts.presentation_generation();
    let old_text_pipeline = pipeline.text_for_test().pipeline.clone();
    // Same-metrics replacement changes session/resources but not geometry/pixels.
    pipeline.replace_fonts(gpu, fonts(), &update).unwrap();
    assert_eq!(metric_values(pipeline.metrics()), metric_values(&m));
    assert_ne!(
        pipeline
            .text_for_test()
            .fonts
            .presentation_generation()
            .session,
        generation.session
    );
    assert_ne!(pipeline.text_for_test().pipeline, old_text_pipeline);
    assert_eq!(
        read_draw(&device, &queue, |pass| pipeline.draw(pass)),
        before_all
    );
    let atlas = pipeline.text_for_test().gpu_atlas._texture.clone();
    let buffer = pipeline.text_for_test().vertex_buffer.clone();
    let scalar_pipeline = pipeline.text_for_test().pipeline.clone();
    // Even extremely small/large accepted DPI identities leave physical pixels alone.
    for scale in [f32::from_bits(1), 1.25, 1.5, 2.0, f32::MAX / 192.0, 1.0] {
        assert!(pipeline.sync_raster_scale(gpu, scale, &update).unwrap());
        assert_eq!(pipeline.viewport(), view);
        assert_eq!(metric_values(pipeline.metrics()), metric_values(&m));
        assert_eq!(pipeline.text_for_test().gpu_atlas._texture, atlas);
        assert_eq!(pipeline.text_for_test().vertex_buffer, buffer);
        assert_eq!(pipeline.text_for_test().pipeline, scalar_pipeline);
        assert_eq!(
            read_draw(&device, &queue, |pass| pipeline.draw(pass)),
            before_all
        );
    }
    let generation = pipeline.text_for_test().fonts.presentation_generation();
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
        assert_eq!(
            pipeline.text_for_test().fonts.presentation_generation(),
            generation
        );
        assert_eq!(pipeline.text_for_test().gpu_atlas._texture, atlas);
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
    assert_eq!(pipeline.text_for_test().raster_scale, 1.5);
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
    assert_ne!(pipeline.text_for_test().gpu_atlas._texture, atlas);
    assert_ne!(pipeline.text_for_test().vertex_buffer, buffer);
    assert_ne!(pipeline.text_for_test().pipeline, scalar_pipeline);
    assert_ne!(
        read_draw(&device, &queue, |pass| pipeline.text_for_test().draw(pass)),
        before_text
    );
    assert_ne!(
        read_draw(&device, &queue, |pass| pipeline
            .decoration_for_test()
            .draw(pass)),
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
            0 => p.text_for_test().draw(pass),
            1 => p.decoration_for_test().draw(pass),
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
    let valid_atlas = pipeline.text_for_test().gpu_atlas._texture.clone();
    let valid_pipeline = pipeline.text_for_test().pipeline.clone();
    let valid_buffer = pipeline.text_for_test().vertex_buffer.clone();
    let valid_generation = pipeline.text_for_test().fonts.presentation_generation();
    let valid_pixels = read_draw(&device, &queue, |pass| pipeline.draw(pass));
    // Rejected resource preparation: malformed/oversized grids fail before publish.
    for (rows, cols) in [(0, 10), (usize::MAX, 10), (3, usize::MAX)] {
        let mut invalid = update.clone();
        std::sync::Arc::make_mut(&mut invalid.projection.snapshot).rows = rows;
        std::sync::Arc::make_mut(&mut invalid.projection.snapshot).cols = cols;
        assert!(pipeline.replace_fonts(gpu, fonts(), &invalid).is_err());
        assert_eq!(pipeline.viewport(), valid_view);
        assert_eq!(metric_values(pipeline.metrics()), valid_metrics);
        assert_eq!(pipeline.text_for_test().gpu_atlas._texture, valid_atlas);
        assert_eq!(pipeline.text_for_test().pipeline, valid_pipeline);
        assert_eq!(pipeline.text_for_test().vertex_buffer, valid_buffer);
        assert_eq!(
            pipeline.text_for_test().fonts.presentation_generation(),
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
            pipeline.text_for_test().fonts.presentation_generation(),
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
    assert_eq!(pipeline.text_for_test().gpu_atlas._texture, valid_atlas);
    assert_eq!(pipeline.text_for_test().pipeline, valid_pipeline);
    assert_eq!(pipeline.text_for_test().vertex_buffer, valid_buffer);
    assert_eq!(
        pipeline.text_for_test().fonts.presentation_generation(),
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
    assert!(pipeline.decoration_for_test().is_dirty());
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
            if crate::render::decoration::valid_metrics(&bad) {
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
