use super::*;
use crate::model::DirtyRange;
use crate::render::color_atlas::COLOR_CACHE_ENTRIES;
use crate::render::observations::{sequence_work, text_uploads};
use crate::render::text::sequence_tests::{read_draw, test_gpu};
use harbor_text::{CompleteKind, PresentationTile, SequenceOutcome, SequencePresentation};
use std::time::Instant;

fn fonts() -> FontBook {
    harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 16.0,
    })
    .unwrap()
}
fn viewport() -> RenderViewport {
    RenderViewport::with_surface(9.0, 19.0, (256, 128), (256, 128))
}
fn blank(rows: usize, cols: usize) -> TerminalSnapshot {
    (*crate::Terminal::new_headless(rows, cols)
        .read_update(Instant::now())
        .snapshot)
        .clone()
}
fn cell(source: &str) -> Cell {
    let mut chars = source.chars();
    Cell {
        ch: chars.next().unwrap_or(' '),
        suffix: chars.collect::<String>(),
        width: 2,
        ..Cell::default()
    }
}
fn dirty(row: usize, cols: usize) -> [DirtyRange; 1] {
    [DirtyRange {
        row,
        start_col: 0,
        end_col: cols,
    }]
}
fn bytes<T: bytemuck::Pod>(v: &[T]) -> &[u8] {
    bytemuck::cast_slice(v)
}

// Rebuild classification and geometry from scratch, using the same presentation
// outcomes but fresh atlas placements. This isolates projection correctness from
// platform-dependent native shaping and synthetic pressure fixture sizes.
fn assert_reference(
    gpu: TerminalGpuAccess<'_>,
    fonts: &FontBook,
    scalar: &GlyphAtlas,
    snap: &TerminalSnapshot,
    layer: &SequenceLayer,
) {
    let mut fresh = SequenceLayer::default();
    let mut seen = HashSet::new();
    let requests: Vec<_> = snap
        .cells
        .iter()
        .filter(|c| emoji_candidate(c))
        .filter_map(|c| sequence_request(c, fonts, &Palette::default(), 1.0))
        .filter(|r| seen.insert(r.clone()))
        .take(COLOR_CACHE_ENTRIES)
        .collect();
    fresh
        .atlas
        .sync(&requests, fonts.presentation_generation(), |r| {
            let outcome = match layer.atlas.entries.get(r) {
                Some(e) if e.tile.is_some() => SequenceOutcome::Complete {
                    kind: CompleteKind::Color,
                    tile: e.tile.clone().unwrap(),
                    runs: vec![],
                },
                _ => SequenceOutcome::Unsupported(harbor_text::UnsupportedReason::MissingGlyph),
            };
            SequencePresentation {
                generation: fonts.presentation_generation(),
                outcome,
            }
        });
    fresh.prepare(
        gpu,
        fonts,
        scalar,
        snap,
        &viewport(),
        &Palette::default(),
        1.0,
    );
    assert_eq!(layer.emoji_cells, fresh.emoji_cells);
    assert_eq!(layer.tile_cells, fresh.tile_cells);
    let actual = read_draw(gpu.device(), gpu.queue(), |p| layer.draw(p));
    let expected = read_draw(gpu.device(), gpu.queue(), |p| fresh.draw(p));
    assert!(
        actual == expected,
        "incremental output differs from complete reference"
    );
}

#[test]
fn classification_observes_selectors_beyond_the_native_request_limit() {
    let mut c = cell(&format!("😀{}\u{fe0e}", "\u{301}".repeat(200)));
    assert!(!emoji_candidate(&c));
    c = cell(&format!("♥{}\u{fe0f}", "\u{301}".repeat(200)));
    assert!(emoji_candidate(&c));
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn bounded_text_edits_retain_sequence_rows_and_local_scalar_writes() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for rows in [8, 80, 240] {
        let mut engine = crate::Terminal::new_headless(rows, 80);
        engine.put_str("xy\r\n😀");
        let update = engine.read_update(Instant::now());
        let f = fonts();
        let metrics = harbor_text::TextMetrics::from_font_metrics(f.font_metrics());
        let mut text = crate::render::text::Text::new(
            gpu,
            f,
            metrics,
            &update.snapshot,
            &viewport(),
            Palette::default(),
        )
        .unwrap();
        engine.acknowledge_update(&update);
        let before = text.presentation_stats().native;
        let clean = engine.read_update(Instant::now());
        text.prepare_with_dirty(gpu, &clean.snapshot, &[], &viewport(), None);
        assert_eq!(sequence_work().classified_cells, 0);
        assert!(sequence_work().built_rows.is_empty());
        assert!(sequence_work().writes.is_empty());
        assert!(text_uploads().is_empty());
        assert_eq!(sequence_work().texture_bytes, 0);
        for i in 0..8 {
            engine.put_bytes(format!("\x1b[1;1H{}", if i % 2 == 0 { "y" } else { "x" }).as_bytes());
            let edit = engine.read_update(Instant::now());
            text.prepare_with_dirty(
                gpu,
                &edit.snapshot,
                &edit.snapshot.dirty_ranges,
                &viewport(),
                None,
            );
            assert_eq!(sequence_work().classified_cells, 80);
            assert!(
                sequence_work().built_rows.is_empty(),
                "{:?}",
                sequence_work()
            );
            assert!(sequence_work().writes.is_empty());
            assert_eq!(sequence_work().texture_bytes, 0);
            assert_eq!(
                edit.snapshot.dirty_ranges,
                vec![DirtyRange {
                    row: 0,
                    start_col: 0,
                    end_col: 2
                }]
            );
            assert_eq!(
                text_uploads(),
                vec![(0, 12 * std::mem::size_of::<TexturedVertex>())]
            );
            assert_eq!(
                text.presentation_stats().native.shape_calls,
                before.shape_calls
            );
            assert_eq!(
                text.presentation_stats().native.raster_calls,
                before.raster_calls
            );
            engine.acknowledge_update(&edit);
        }
        eprintln!(
            "grid={rows}x80 bounded edit: {:?}; scalar={:?}",
            sequence_work(),
            text_uploads()
        );
    }
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn local_tile_and_missing_churn_retains_other_row_and_clears_removals() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = fonts();
    let scalar = GlyphAtlas::new();
    let overlong = format!("👩{}", "\u{200d}\u{301}".repeat(128));
    for persistent in ["😀", overlong.as_str()] {
        let mut snap = blank(4, 16);
        snap.cells[16] = cell(persistent);
        let mut layer = SequenceLayer::default();
        layer.prepare(
            gpu,
            &fonts,
            &scalar,
            &snap,
            &viewport(),
            &Palette::default(),
            1.0,
        );
        let saved = (
            bytes(&layer.rows[1].vertices).to_vec(),
            bytes(&layer.rows[1].missing).to_vec(),
        );
        for source in ["😀", "👩‍💻", overlong.as_str(), "", "😀", ""] {
            snap.cells[0] = cell(source);
            layer.prepare_with_dirty(
                gpu,
                &fonts,
                &scalar,
                &snap,
                &viewport(),
                &Palette::default(),
                1.0,
                &dirty(0, 16),
                false,
                false,
            );
            assert!(sequence_work().built_rows.iter().all(|&r| r == 0));
            assert!(sequence_work().writes.iter().all(|&(r, _, _)| r == 0));
            assert_eq!(bytes(&layer.rows[1].vertices), saved.0);
            assert_eq!(bytes(&layer.rows[1].missing), saved.1);
            if source == overlong {
                assert_eq!(
                    sequence_work().writes,
                    vec![(0, 0, 24 * std::mem::size_of::<ColoredVertex>())]
                );
            }
            if source.is_empty() {
                assert!(layer.rows[0].quads.is_none() && layer.rows[0].cues.is_none());
                assert!(!layer.tile_cells.contains(&0) && !layer.emoji_cells.contains(&0));
            }
            assert_reference(gpu, &fonts, &scalar, &snap, &layer);
            layer.prepare_with_dirty(
                gpu,
                &fonts,
                &scalar,
                &snap,
                &viewport(),
                &Palette::default(),
                1.0,
                &dirty(0, 16),
                false,
                false,
            );
            assert!(
                sequence_work().built_rows.is_empty() && sequence_work().writes.is_empty(),
                "replaying unacknowledged content damage must retain identical geometry"
            );
            let stats = fonts.sequence_cache_stats();
            layer.prepare_with_dirty(
                gpu,
                &fonts,
                &scalar,
                &snap,
                &viewport(),
                &Palette::default(),
                1.0,
                &[],
                false,
                false,
            );
            assert_eq!(sequence_work().classified_cells, 0);
            assert!(sequence_work().built_rows.is_empty() && sequence_work().writes.is_empty());
            assert_eq!(sequence_work().texture_bytes, 0);
            assert_eq!(fonts.sequence_cache_stats().shape_calls, stats.shape_calls);
            assert_eq!(
                fonts.sequence_cache_stats().raster_calls,
                stats.raster_calls
            );
        }
    }
}

fn tile(fonts: &FontBook, side: u32) -> SequencePresentation {
    SequencePresentation {
        generation: fonts.presentation_generation(),
        outcome: SequenceOutcome::Complete {
            kind: CompleteKind::Color,
            tile: PresentationTile {
                bounds: TileBounds {
                    left: 0,
                    top: 0,
                    width: side,
                    height: side,
                },
                image_formats: 0,
                rgba: [128, 0, 0, 128].repeat((side * side) as usize),
            },
            runs: vec![],
        },
    }
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn placement_repack_updates_untouched_sequence_without_scalar_damage() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = fonts();
    let scalar = GlyphAtlas::new();
    let palette = Palette::default();
    let mut snap = blank(4, 16);
    snap.cells[16] = cell("😀");
    let mut requests: Vec<_> = (0..128)
        .map(|n| {
            let mut r = sequence_request(&snap.cells[16], &fonts, &palette, 1.0).unwrap();
            r.foreground = [n, 0, 0, 255];
            r
        })
        .collect();
    requests[127] = sequence_request(&snap.cells[16], &fonts, &palette, 1.0).unwrap();
    let mut layer = SequenceLayer::default();
    layer
        .atlas
        .sync(&requests, fonts.presentation_generation(), |_| {
            tile(&fonts, 16)
        });
    layer.prepare(gpu, &fonts, &scalar, &snap, &viewport(), &palette, 1.0);
    let old = bytes(&layer.rows[1].vertices).to_vec();
    let revision = layer.atlas.revision;
    snap.cells[0] = cell("👩‍💻");
    let extra = layer.prepare_with_dirty(
        gpu,
        &fonts,
        &scalar,
        &snap,
        &viewport(),
        &palette,
        1.0,
        &dirty(0, 16),
        false,
        false,
    );
    assert!(layer.atlas.revision > revision);
    assert!(sequence_work().built_rows.contains(&1));
    assert_ne!(bytes(&layer.rows[1].vertices), old);
    assert!(
        !extra.iter().any(|r| r.row == 1),
        "UV movement does not change scalar suppression"
    );
    assert_eq!(
        sequence_work().texture_bytes,
        (COLOR_ATLAS_SIDE * COLOR_ATLAS_SIDE * 4) as usize
    );
    assert_reference(gpu, &fonts, &scalar, &snap, &layer);
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn capacity_release_retries_fallback_on_an_untouched_row() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = fonts();
    for usable in [false, true] {
        let mut scalar = GlyphAtlas::new();
        if usable {
            scalar.rebuild(&fonts, &['😀']);
        }
        let mut snap = blank(4, 16);
        snap.cells[0] = cell("😀");
        snap.cells[16] = cell("😀");
        snap.cells[16].fg = harbor_config::Color::Rgb(255, 0, 0);
        let palette = Palette::default();
        let requests: Vec<_> = [0, 16]
            .iter()
            .map(|&idx| sequence_request(&snap.cells[idx], &fonts, &palette, 1.0).unwrap())
            .collect();
        let mut layer = SequenceLayer::default();
        layer
            .atlas
            .sync(&requests, fonts.presentation_generation(), |_| {
                tile(&fonts, 512)
            });
        layer.prepare(gpu, &fonts, &scalar, &snap, &viewport(), &palette, 1.0);
        assert_eq!(
            layer.rows[1].cells[0].resolved,
            Some(if usable {
                Resolved::Scalar
            } else {
                Resolved::Missing
            })
        );
        snap.cells[0] = cell("");
        let extra = layer.prepare_with_dirty(
            gpu,
            &fonts,
            &scalar,
            &snap,
            &viewport(),
            &palette,
            1.0,
            &dirty(0, 16),
            false,
            false,
        );
        assert!(sequence_work().built_rows.contains(&1));
        assert!(matches!(
            layer.rows[1].cells[0].resolved,
            Some(Resolved::Tile(..))
        ));
        assert_eq!(extra.iter().any(|r| r.row == 1), usable);
        assert!(
            layer.rows[1].cues.is_none(),
            "obsolete missing cue must be removed"
        );
        assert_reference(gpu, &fonts, &scalar, &snap, &layer);
    }
}

#[test]
#[ignore = "Windows native fonts; run explicitly"]
fn sequence_request_rejects_only_sources_above_the_utf16_limit() {
    let fonts = fonts();
    let palette = Palette::default();
    let source = format!("😀{}", "\u{301}".repeat(SEQUENCE_MAX_UTF16 - 2));
    let at_limit = cell(&source);
    assert!(emoji_candidate(&at_limit));
    assert_eq!(
        sequence_request(&at_limit, &fonts, &palette, 1.0)
            .unwrap()
            .source,
        source
    );
    let above_limit = cell(&format!("{source}\u{301}"));
    assert!(emoji_candidate(&above_limit));
    assert!(sequence_request(&above_limit, &fonts, &palette, 1.0).is_none());
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn duplicates_and_oversize_do_not_block_row_major_promotion() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = fonts();
    let scalar = GlyphAtlas::new();
    let palette = Palette::default();
    let source = format!("😀{}", "\u{301}".repeat(SEQUENCE_MAX_UTF16 - 2));
    let oversized = format!("{source}\u{301}");
    let mut snap = blank(3, 160);
    for col in 0..32 {
        snap.cells[col] = cell(&source);
    }
    snap.cells[32] = cell(&oversized);
    for col in 33..160 {
        snap.cells[col] = cell("😀");
        snap.cells[col].fg = harbor_config::Color::Rgb((col - 33) as u8, 0, 0);
    }
    snap.cells[160] = cell(&source);
    snap.cells[161] = cell("👩‍💻");

    let accepted: Vec<_> = std::iter::once(&snap.cells[0])
        .chain((33..160).map(|col| &snap.cells[col]))
        .map(|c| sequence_request(c, &fonts, &palette, 1.0).unwrap())
        .collect();
    let later = sequence_request(&snap.cells[161], &fonts, &palette, 1.0).unwrap();
    assert_eq!(accepted.len(), COLOR_CACHE_ENTRIES);
    let mut layer = SequenceLayer::default();
    layer
        .atlas
        .sync(&accepted, fonts.presentation_generation(), |_| {
            tile(&fonts, 8)
        });
    layer.prepare(gpu, &fonts, &scalar, &snap, &viewport(), &palette, 1.0);
    assert_eq!(layer.atlas.entries.len(), COLOR_CACHE_ENTRIES);
    assert!(accepted.iter().all(|r| layer.atlas.entries.contains_key(r)));
    assert!(!layer.atlas.entries.contains_key(&later));
    assert!(layer.rows[0].cells[32].request.is_none());
    assert!(matches!(
        layer.rows[1].cells[0].resolved,
        Some(Resolved::Tile(..))
    ));
    assert_eq!(layer.rows[1].cells[1].resolved, Some(Resolved::Missing));

    let revision = layer.atlas.revision;
    snap.cells[33] = Cell::default();
    layer.prepare_with_dirty(
        gpu,
        &fonts,
        &scalar,
        &snap,
        &viewport(),
        &palette,
        1.0,
        &dirty(0, 160),
        false,
        false,
    );
    assert_eq!(layer.atlas.entries.len(), COLOR_CACHE_ENTRIES);
    assert!(!layer.atlas.entries.contains_key(&accepted[1]));
    assert!(layer.atlas.entries.contains_key(&later));
    assert!(layer.atlas.revision > revision);
    assert!(sequence_work().built_rows.contains(&1));
    assert!(matches!(
        layer.rows[1].cells[1].resolved,
        Some(Resolved::Tile(..))
    ));
    assert_reference(gpu, &fonts, &scalar, &snap, &layer);
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn admission_overflow_promotes_untouched_rows_and_churn_stays_bounded() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = fonts();
    let scalar = GlyphAtlas::new();
    let palette = Palette::default();
    let mut snap = blank(4, 160);
    for col in 0..128 {
        snap.cells[col] = cell("😀");
        snap.cells[col].fg = harbor_config::Color::Rgb(col as u8, 0, 0);
    }
    snap.cells[160] = cell("👩‍💻");
    let requests: Vec<_> = (0..128)
        .map(|col| sequence_request(&snap.cells[col], &fonts, &palette, 1.0).unwrap())
        .collect();
    let mut layer = SequenceLayer::default();
    layer
        .atlas
        .sync(&requests, fonts.presentation_generation(), |_| {
            tile(&fonts, 16)
        });
    layer.prepare(gpu, &fonts, &scalar, &snap, &viewport(), &palette, 1.0);
    assert_eq!(layer.atlas.entries.len(), 128);
    assert_eq!(layer.rows[1].cells[0].resolved, Some(Resolved::Missing));
    for col in 0..128 {
        snap.cells[col] = Cell::default();
    }
    layer.prepare_with_dirty(
        gpu,
        &fonts,
        &scalar,
        &snap,
        &viewport(),
        &palette,
        1.0,
        &dirty(0, 160),
        false,
        false,
    );
    assert!(sequence_work().built_rows.contains(&1));
    assert!(matches!(
        layer.rows[1].cells[0].resolved,
        Some(Resolved::Tile(..))
    ));
    assert_reference(gpu, &fonts, &scalar, &snap, &layer);
    for n in 0..140 {
        snap.cells[0] = cell("😀");
        snap.cells[0].fg = harbor_config::Color::Rgb(0, n, 255);
        layer.prepare_with_dirty(
            gpu,
            &fonts,
            &scalar,
            &snap,
            &viewport(),
            &palette,
            1.0,
            &dirty(0, 160),
            false,
            false,
        );
        assert_eq!(layer.rows.iter().map(|r| r.cells.len()).sum::<usize>(), 2);
        assert_eq!(layer.atlas.entries.len(), 2);
        assert!(
            layer
                .atlas
                .entries
                .values()
                .filter_map(|e| e.tile.as_ref())
                .map(|t| t.rgba.len())
                .sum::<usize>()
                <= harbor_text::SEQUENCE_PIXEL_BUDGET
        );
        let stats = fonts.sequence_cache_stats();
        assert!(stats.shapes <= harbor_text::SEQUENCE_CACHE_ENTRIES);
        assert!(stats.rasters <= harbor_text::SEQUENCE_CACHE_ENTRIES);
        assert!(stats.pixel_bytes <= harbor_text::SEQUENCE_PIXEL_BUDGET);
    }
    assert_reference(gpu, &fonts, &scalar, &snap, &layer);
}

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn structural_and_failed_updates_converge_without_consuming_pending_damage() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = crate::Terminal::new_headless(4, 16);
    engine.put_str("😀\r\n♥️");
    let first = engine.read_update(Instant::now());
    let f = fonts();
    let metrics = harbor_text::TextMetrics::from_font_metrics(f.font_metrics());
    let mut pipeline =
        crate::TerminalRenderPipeline::new(gpu, (256, 128), f, metrics, &first).unwrap();
    assert!(pipeline.prepare(gpu, &first, false));
    assert!(engine.acknowledge_update(&first));
    for step in 0..7 {
        match step {
            0 => engine.put_str("\x1b[1;1H👩‍💻\x1b[3;1Hx"),
            1 => engine.put_str("\r\n😀\r\n👩‍💻\r\n♥️\r\nend"),
            2 => {
                engine.command_scroll_to_top();
            }
            3 => {
                engine.command_scroll_to_bottom();
            }
            4 => engine.put_str("\x1b[?1049h😀"),
            5 => engine.put_str("\x1b[?1049l"),
            _ => engine.resize(3, 12),
        }
        let skipped = engine.read_update(Instant::now());
        let mut invalid = skipped.clone();
        std::sync::Arc::make_mut(&mut invalid.projection.snapshot).cols = 0;
        assert!(!pipeline.prepare(gpu, &invalid, false));
        assert!(!engine.acknowledge_update(&invalid));
        engine.put_str("\x1b[2;4Hx");
        assert!(!engine.acknowledge_update(&skipped));
        let current = engine.read_update(Instant::now());
        assert!(pipeline.prepare(gpu, &current, false));
        let f = fonts();
        let metrics = harbor_text::TextMetrics::from_font_metrics(f.font_metrics());
        let fresh = crate::Text::new(
            gpu,
            f,
            metrics,
            &current.snapshot,
            &pipeline.viewport(),
            current.appearance.palette(),
        )
        .unwrap();
        let actual = read_draw(&device, &queue, |p| pipeline.text_for_test().draw(p));
        let reference = read_draw(&device, &queue, |p| fresh.draw(p));
        assert!(
            actual == reference,
            "structural step {step} differs from complete preparation"
        );
        assert!(engine.acknowledge_update(&current));
    }
}
