use super::*;
use crate::render::observations::{buffer_write_bytes, cell_overlay_work, ime_overlay_work};

fn blank(rows: usize, cols: usize) -> TerminalSnapshot {
    (*Terminal::new_headless(rows, cols)
        .read_update(Instant::now())
        .snapshot)
        .clone()
}

fn mark(snap: &mut TerminalSnapshot, row: usize, col: usize, suffix: &str) {
    let cell = &mut snap.cells[row * snap.cols + col];
    cell.ch = 'e';
    cell.suffix = suffix.into();
}

fn prepare(
    layer: &mut Text,
    gpu: TerminalGpuAccess<'_>,
    snap: &TerminalSnapshot,
    dirty: &[DirtyRange],
    viewport: &RenderViewport,
    ime: Option<&Preedit>,
) {
    layer.prepare_with_dirty(gpu, snap, dirty, viewport, ime);
    layer.prepare_preedit(gpu, ime, snap, viewport);
}

fn idle() {
    let cells = cell_overlay_work();
    assert_eq!(cells.inspected_cells, 0, "{cells:?}");
    assert!(
        cells.built_rows.is_empty() && cells.writes.is_empty(),
        "{cells:?}"
    );
    let ime = ime_overlay_work();
    assert_eq!(ime.builds + ime.clears, 0, "{ime:?}");
    assert!(ime.writes.is_empty(), "{ime:?}");
}

fn fresh_pixels(
    layer: &Text,
    gpu: TerminalGpuAccess<'_>,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    ime: Option<&Preedit>,
) -> Vec<u8> {
    let mut fresh = Text::new_at_scale(
        gpu,
        fonts(),
        layer.metrics,
        snap,
        viewport,
        layer.palette,
        layer.raster_scale,
    )
    .unwrap();
    prepare(&mut fresh, gpu, snap, &[], viewport, ime);
    read_draw(gpu.device(), gpu.queue(), |p| fresh.draw(p))
}

fn assert_fresh(
    layer: &Text,
    gpu: TerminalGpuAccess<'_>,
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    ime: Option<&Preedit>,
) {
    let actual = read_draw(gpu.device(), gpu.queue(), |p| layer.draw(p));
    assert_pixels_equal(&actual, &fresh_pixels(layer, gpu, snap, viewport, ime), 1.0);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn warm_empty_and_populated_overlays_do_no_grid_work_or_writes() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for rows in [8, 80, 240] {
        for populated in [false, true] {
            let mut snap = blank(rows, 80);
            if populated {
                mark(&mut snap, 1, 0, "\u{301}\u{327}");
                snap.cells[160].ch = '\u{301}';
                snap.cells[160].isolated_mark = true;
            }
            let (mut layer, viewport) = text(gpu, &snap);
            let composition = Preedit::new("MN", None);
            let ime = populated.then_some(&composition);
            prepare(&mut layer, gpu, &snap, &[], &viewport, ime);
            if populated {
                assert!(layer.cell_overlays.count() > 0 && layer.ime_overlay.count() > 0);
            }
            let before = read_draw(&device, &queue, |p| layer.draw(p));
            for _ in 0..3 {
                let bytes =
                    buffer_write_bytes(|| prepare(&mut layer, gpu, &snap, &[], &viewport, ime));
                idle();
                assert_eq!(bytes, 0);
            }
            assert_pixels_equal(&read_draw(&device, &queue, |p| layer.draw(p)), &before, 1.0);
            let replayed: Vec<_> = (0..snap.rows)
                .map(|row| DirtyRange {
                    row,
                    start_col: 0,
                    end_col: snap.cols,
                })
                .collect();
            prepare(&mut layer, gpu, &snap, &replayed, &viewport, ime);
            idle();
            // Row-local ordinary edits must not upload persistent mark geometry.
            snap.cells[0].ch = 'e'; // Already cached in the populated fixture.
            prepare(
                &mut layer,
                gpu,
                &snap,
                &[DirtyRange {
                    row: 0,
                    start_col: 0,
                    end_col: 1,
                }],
                &viewport,
                ime,
            );
            let work = cell_overlay_work();
            assert_eq!(work.inspected_cells, 80);
            assert_eq!(work.built_rows, [0]);
            assert!(work.writes.is_empty());
            assert_eq!(ime_overlay_work().builds, 0);
            assert_fresh(&layer, gpu, &snap, &viewport, ime);
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn local_mark_churn_uploads_only_changed_row_and_clears_removed_paint() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut snap = blank(4, 16);
    mark(&mut snap, 0, 0, "\u{301}");
    mark(&mut snap, 2, 0, "\u{301}\u{327}");
    let (mut layer, viewport) = text(gpu, &snap);
    let dirty = [DirtyRange {
        row: 0,
        start_col: 0,
        end_col: 1,
    }];
    for suffix in ["\u{301}\u{327}", "\u{327}", ""] {
        snap.cells[0].suffix = suffix.into();
        prepare(&mut layer, gpu, &snap, &dirty, &viewport, None);
        let work = cell_overlay_work();
        assert_eq!(work.inspected_cells, 16);
        assert_eq!(work.built_rows, [0]);
        assert!(work.writes.iter().all(|(row, _)| *row == 0));
        if !suffix.is_empty() {
            assert_eq!(work.writes.len(), 1);
        }
        assert_fresh(&layer, gpu, &snap, &viewport, None);
        // Replayed damage reuses the shared source row without reconstruction.
        prepare(&mut layer, gpu, &snap, &dirty, &viewport, None);
        idle();
    }
    // Both isolated-mark paint and filtering still match full preparation.
    snap.cells[0].ch = '\u{301}';
    snap.cells[0].isolated_mark = true;
    for hidden in [0, CellAttrs::CONCEAL] {
        snap.cells[0].attrs = CellAttrs::default();
        snap.cells[0].attrs.set(hidden);
        prepare(&mut layer, gpu, &snap, &dirty, &viewport, None);
        assert_fresh(&layer, gpu, &snap, &viewport, None);
    }
    snap.cells[0].attrs = CellAttrs::default();
    snap.cells[0].wide_continuation = true;
    prepare(&mut layer, gpu, &snap, &dirty, &viewport, None);
    assert_fresh(&layer, gpu, &snap, &viewport, None);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn ime_changes_and_cancellation_never_rescan_cell_overlays() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut snap = blank(4, 16);
    mark(&mut snap, 2, 0, "\u{301}");
    snap.cells[32 + 1].ch = 'M';
    snap.cells[32 + 2].ch = 'N';
    let (mut layer, viewport) = text(gpu, &snap);
    for input in ["M", "N", "MN"] {
        let preedit = Preedit::new(input, None);
        prepare(&mut layer, gpu, &snap, &[], &viewport, Some(&preedit));
        assert_eq!(cell_overlay_work().inspected_cells, 0);
        assert!(cell_overlay_work().writes.is_empty());
        assert_eq!(ime_overlay_work().builds, 1);
        assert_fresh(&layer, gpu, &snap, &viewport, Some(&preedit));
        let caret_only = Preedit::new(input, Some((0, 0)));
        prepare(&mut layer, gpu, &snap, &[], &viewport, Some(&caret_only));
        idle();
    }
    snap.cursor_x = 3;
    let preedit = Preedit::new("MN", None);
    prepare(&mut layer, gpu, &snap, &[], &viewport, Some(&preedit));
    assert_eq!(cell_overlay_work().inspected_cells, 0);
    assert_eq!(ime_overlay_work().builds, 1);
    assert_fresh(&layer, gpu, &snap, &viewport, Some(&preedit));
    let empty = Preedit::new("", None);
    for ime in [Some(&empty), None, Some(&empty), None] {
        let was_visible = layer.ime_overlay.count() > 0;
        let bytes = buffer_write_bytes(|| prepare(&mut layer, gpu, &snap, &[], &viewport, ime));
        assert_eq!(bytes, 0);
        assert_eq!(ime_overlay_work().clears, usize::from(was_visible));
        assert_eq!(ime_overlay_work().builds, 0);
        assert_eq!(layer.ime_overlay.count(), 0);
        assert_eq!(cell_overlay_work().inspected_cells, 0);
        assert_fresh(&layer, gpu, &snap, &viewport, ime);
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn palette_geometry_metrics_and_resize_refresh_both_projections() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut snap = blank(4, 16);
    mark(&mut snap, 1, 0, "\u{301}");
    let (mut layer, mut viewport) = text(gpu, &snap);
    let ime = Preedit::new("M", None);
    prepare(&mut layer, gpu, &snap, &[], &viewport, Some(&ime));
    for transition in 0..4 {
        match transition {
            0 => layer.set_palette(Palette {
                foreground: harbor_config::Rgba::from_rgba8(0, 255, 0, 128),
                ..Palette::default()
            }),
            1 => {
                viewport.allocation_origin = (5.0, 7.0);
                layer.invalidate_projection();
            }
            2 => {
                layer.metrics.ascent += 1.0;
                layer.invalidate_projection();
            }
            _ => {
                snap = blank(3, 20);
                mark(&mut snap, 1, 0, "\u{301}");
            }
        }
        prepare(&mut layer, gpu, &snap, &[], &viewport, Some(&ime));
        assert_eq!(cell_overlay_work().built_rows.len(), snap.rows);
        assert_eq!(ime_overlay_work().builds, 1);
        assert_fresh(&layer, gpu, &snap, &viewport, Some(&ime));
        prepare(&mut layer, gpu, &snap, &[], &viewport, Some(&ime));
        idle();
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn referenced_glyph_admission_and_repack_refresh_without_cell_damage() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut snap = blank(4, 16);
    mark(&mut snap, 1, 0, "\u{301}");
    mark(&mut snap, 2, 0, "\u{327}");
    let f = fonts();
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let viewport = RenderViewport::with_surface(
        metrics.cell_width,
        metrics.line_height,
        (256, 128),
        (256, 128),
    );
    let mut atlas = GlyphAtlas::new();
    let palette = Palette::default();
    let mut cells = CellOverlays::default();
    let mut ime = ImeOverlay::default();
    let composition = Preedit::new("M", None);
    let suppressed = std::collections::HashSet::new();
    let update = |cells: &mut CellOverlays, ime: &mut ImeOverlay, atlas: &GlyphAtlas| {
        cells.prepare(
            gpu,
            &snap,
            OverlayView {
                atlas,
                metrics: &metrics,
                palette: &palette,
                viewport: &viewport,
            },
            &suppressed,
            &[],
            false,
        );
        ime.prepare(
            gpu,
            &snap,
            Some(&composition),
            OverlayView {
                atlas,
                metrics: &metrics,
                palette: &palette,
                viewport: &viewport,
            },
        );
    };
    update(&mut cells, &mut ime, &atlas);
    assert_eq!(cells.count() + ime.count(), 0);
    atlas.rasterize_new(&f, &['\u{301}', '\u{327}', 'M']);
    update(&mut cells, &mut ime, &atlas);
    assert_eq!(cell_overlay_work().built_rows, [1, 2]);
    assert_eq!(cell_overlay_work().writes.len(), 2);
    assert_eq!(ime_overlay_work().builds, 1);
    assert!(cells.count() > 0 && ime.count() > 0);
    let old = *atlas.glyph_by_char('M').unwrap();
    atlas.rasterize_new(&f, &['Z']);
    let bytes = buffer_write_bytes(|| update(&mut cells, &mut ime, &atlas));
    idle();
    assert_eq!(
        bytes, 0,
        "unrelated additions must not upload dependent geometry"
    );
    atlas.rebuild(&f, &['\u{301}', '\u{327}', 'M', 'Á', '中', 'Q', 'g', 'W']);
    let new = atlas.glyph_by_char('M').unwrap();
    assert_ne!((old.atlas_x, old.atlas_y), (new.atlas_x, new.atlas_y));
    update(&mut cells, &mut ime, &atlas);
    assert_eq!(cell_overlay_work().built_rows, [1, 2]);
    assert_eq!(ime_overlay_work().builds, 1);
    // Fresh retained projections with the current atlas are the full oracle.
    let group_layout = crate::render::gpu::create_texture_bind_group_layout(&device);
    let gpu_atlas = GpuGlyphAtlas::new(&device, &queue, &group_layout, &atlas);
    let pipeline = Text::create_pipeline(gpu.device(), gpu.format(), &group_layout);
    let actual = read_draw(&device, &queue, |p| {
        cells.draw(p, &pipeline, &gpu_atlas.bind_group);
        ime.draw(p, &pipeline, &gpu_atlas.bind_group);
    });
    let mut fresh_cells = CellOverlays::default();
    let mut fresh_ime = ImeOverlay::default();
    update(&mut fresh_cells, &mut fresh_ime, &atlas);
    let expected = read_draw(&device, &queue, |p| {
        fresh_cells.draw(p, &pipeline, &gpu_atlas.bind_group);
        fresh_ime.draw(p, &pipeline, &gpu_atlas.bind_group);
    });
    assert_pixels_equal(&actual, &expected, 1.0);
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn sequence_capacity_release_invalidates_marks_beyond_content_damage() {
    use harbor_text::{
        CompleteKind, PresentationTile, SequenceOutcome, SequencePresentation, TileBounds,
    };
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut snap = blank(4, 16);
    for row in [0, 1] {
        let cell = &mut snap.cells[row * 16];
        cell.ch = '😀';
        cell.width = 2;
        cell.suffix = "\u{301}".into();
    }
    snap.cells[16].fg = Color::Rgb(255, 0, 0);
    mark(&mut snap, 2, 0, "\u{327}");
    let (mut layer, viewport) = text(gpu, &snap);
    let mut requests: Vec<_> = layer.sequences.atlas.entries.keys().cloned().collect();
    assert_eq!(requests.len(), 2);
    requests.sort_by_key(|r| r.foreground == [255, 0, 0, 255]);
    layer.sequences = Default::default();
    layer
        .sequences
        .atlas
        .sync(&requests, layer.fonts.presentation_generation(), |_| {
            SequencePresentation {
                generation: layer.fonts.presentation_generation(),
                outcome: SequenceOutcome::Complete {
                    kind: CompleteKind::Color,
                    tile: PresentationTile {
                        bounds: TileBounds {
                            left: 0,
                            top: 0,
                            width: 512,
                            height: 512,
                        },
                        image_formats: 0,
                        rgba: [128, 0, 0, 128].repeat(512 * 512),
                    },
                    runs: vec![],
                },
            }
        });
    layer.invalidate_projection();
    prepare(&mut layer, gpu, &snap, &[], &viewport, None);
    assert!(layer.sequences.tile_cells.contains(&0));
    assert!(!layer.sequences.tile_cells.contains(&16));
    let previous = layer.cell_overlays.count();
    assert!(previous >= 12);
    snap.cells[0] = Cell::default();
    prepare(
        &mut layer,
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
    assert!(layer.sequences.tile_cells.contains(&16));
    assert_eq!(cell_overlay_work().built_rows, [0, 1]);
    assert_eq!(cell_overlay_work().inspected_cells, 32);
    assert_eq!(layer.cell_overlays.count(), previous - 6);
    assert!(
        cell_overlay_work().writes.is_empty(),
        "suppression clears drawings without uploading unaffected rows"
    );
    let mut full = CellOverlays::default();
    full.prepare(
        gpu,
        &snap,
        OverlayView {
            atlas: &layer.atlas,
            metrics: &layer.metrics,
            palette: &layer.palette,
            viewport: &viewport,
        },
        &layer.sequences.tile_cells,
        &[],
        true,
    );
    let actual = read_draw(&device, &queue, |p| layer.draw(p));
    let expected = read_draw(&device, &queue, |p| {
        p.set_pipeline(&layer.pipeline);
        p.set_bind_group(0, &layer.gpu_atlas.bind_group, &[]);
        p.set_vertex_buffer(0, layer.vertex_buffer.slice(..));
        p.draw(0..(snap.rows * snap.cols * 6) as u32, 0..1);
        layer.sequences.draw(p);
        full.draw(p, &layer.pipeline, &layer.gpu_atlas.bind_group);
    });
    assert_pixels_equal(&actual, &expected, 1.0);
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn structural_and_rejected_updates_preserve_overlay_damage_until_resume() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("e\u{301}\r\n\u{301}");
    let first = engine.read_update(Instant::now());
    let f = fonts();
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let mut pipeline = TerminalRenderPipeline::new(gpu, (256, 128), f, metrics, &first).unwrap();
    assert!(pipeline.prepare(gpu, &first, false));
    assert!(engine.acknowledge_update(&first));
    for step in 0..7 {
        match step {
            0 => engine.put_str("\x1b[1;1He\u{327}\x1b[3;1H\u{301}"),
            1 => engine.put_str("\r\ne\u{301}\r\ne\u{327}\r\n\u{301}\r\nend"),
            2 => {
                engine.command_scroll_to_top();
            }
            3 => {
                engine.command_scroll_to_bottom();
            }
            4 => engine.put_str("\x1b[?1049he\u{327}"),
            5 => engine.put_str("\x1b[?1049l"),
            _ => engine.resize(3, 12),
        }
        let skipped = engine.read_update(Instant::now());
        let mut rejected = skipped.clone();
        std::sync::Arc::make_mut(&mut rejected.projection.snapshot).cols = 0;
        assert!(!pipeline.prepare(gpu, &rejected, false));
        assert!(!engine.acknowledge_update(&rejected));
        engine.put_str("\x1b[2;4He\u{327}");
        assert!(!engine.acknowledge_update(&skipped));
        let current = engine.read_update(Instant::now());
        assert!(pipeline.prepare(gpu, &current, false));
        assert!(pipeline.text_for_test().cell_overlays.count() > 0);
        assert_fresh(
            pipeline.text_for_test(),
            gpu,
            &current.snapshot,
            &pipeline.viewport(),
            current.preedit.as_ref(),
        );
        assert!(engine.acknowledge_update(&current));
        let clean = engine.read_update(Instant::now());
        assert!(pipeline.prepare(gpu, &clean, false));
        // Even review's conservative full damage can reuse shared source rows.
        idle();
    }
}
