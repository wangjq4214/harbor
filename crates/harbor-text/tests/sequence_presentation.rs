use harbor_text::*;
use std::sync::Arc;

fn fonts() -> FontBook {
    load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 32.0,
    })
    .unwrap()
}
fn request(text: &str) -> SequenceRequest {
    SequenceRequest::new(
        text,
        FontSize::new(32.0).unwrap(),
        FontSize::new(96.0).unwrap(),
    )
}
fn complete(p: &SequencePresentation) -> (&CompleteKind, &PresentationTile, &[SequenceRun]) {
    match &p.outcome {
        SequenceOutcome::Complete { kind, tile, runs } => (kind, tile, runs),
        other => panic!("expected complete native fixture, got {other:?}"),
    }
}

fn capture_tile(text: &str, tile: &PresentationTile) {
    let Some(dir) = std::env::var_os("HARBOR_SEQUENCE_CAPTURE_DIR") else {
        return;
    };
    let mut image = image::RgbImage::new(tile.bounds.width * 8, tile.bounds.height * 4);
    // Left: linear composition over black; right: over white. Encode to sRGB
    // only after composition, matching the documented renderer contract.
    for y in 0..tile.bounds.height {
        for x in 0..tile.bounds.width {
            let p = &tile.rgba[((y * tile.bounds.width + x) * 4) as usize..][..4];
            for background in 0..2 {
                let rgb = [0, 1, 2].map(|c| {
                    let v = f32::from(p[c]) / 255.0
                        + background as f32 * (1.0 - f32::from(p[3]) / 255.0);
                    let s = if v <= 0.0031308 {
                        12.92 * v
                    } else {
                        1.055 * v.powf(1.0 / 2.4) - 0.055
                    };
                    (s.clamp(0.0, 1.0) * 255.0).round() as u8
                });
                for dy in 0..4 {
                    for dx in 0..4 {
                        image.put_pixel(
                            (x + background * tile.bounds.width) * 4 + dx,
                            y * 4 + dy,
                            image::Rgb(rgb),
                        );
                    }
                }
            }
        }
    }
    std::fs::create_dir_all(&dir).unwrap();
    let name = text
        .chars()
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-");
    image
        .save(std::path::Path::new(&dir).join(format!("{name}.png")))
        .unwrap();
}

/// Explicit native support test. Not silently skipped on unsupported machines:
/// run separately only where the documented Segoe UI Emoji fixture is installed.
#[test]
#[ignore = "requires documented supporting Segoe UI Emoji/Windows native capabilities"]
fn supporting_windows_font_shapes_complete_fixtures() {
    let fonts = fonts();
    println!("capabilities: {:?}", fonts.sequence_capabilities());
    for text in ["♥️", "👩‍💻"] {
        let r = request(text);
        let p = fonts.present_sequence(&r);
        let (kind, tile, runs) = complete(&p);
        println!(
            "source={text:?}, kind={kind:?}, bounds={:?}, formats={:#x}, runs={runs:?}",
            tile.bounds, tile.image_formats
        );
        assert_eq!(*kind, CompleteKind::Color);
        assert_ne!(tile.image_formats, 0);
        capture_tile(text, tile);
        assert_eq!(
            runs.iter().map(|r| r.utf16_len).sum::<u32>(),
            text.encode_utf16().count() as u32
        );
        assert!(runs.iter().all(|r| r.family == "Segoe UI Emoji"));
        assert!(runs[0].clusters.iter().all(|&c| c == runs[0].clusters[0]));
        assert!(!runs[0].glyphs.contains(&0));
        assert_eq!(
            tile.rgba.len(),
            (tile.bounds.width * tile.bounds.height * 4) as usize
        );
        // Counts/color alone aren't acceptance: full source/cluster/fallback above
        // plus the ZWJ substitution control and external visual fixture evidence.
        let warm = fonts.present_sequence(&r);
        assert!(Arc::ptr_eq(&p, &warm));
        assert_eq!(fonts.sequence_cache_stats().offscreen_creations, 1);
    }
    let bad = fonts.present_sequence(&request("👩‍A"));
    assert!(matches!(bad.outcome, SequenceOutcome::Unsupported(_)));
}

#[test]
fn ordinary_scalar_loading_is_lazy_and_unchanged() {
    let fonts = fonts();
    let key = fonts.resolve('A', 32.0, 0);
    assert!(matches!(key, GlyphResolution::Available(_)));
    assert!(!fonts.rasterize('A', 32.0).1.is_empty());
    assert_eq!(fonts.sequence_cache_stats(), SequenceCacheStats::default());
    assert_eq!(
        fonts.present_sequence(&request("中")).outcome,
        SequenceOutcome::Unsupported(UnsupportedReason::NotEmojiUnit)
    );
    assert_eq!(fonts.sequence_cache_stats().offscreen_creations, 0);
}

#[test]
fn negative_cache_and_hostile_requests_are_bounded() {
    let fonts = fonts();
    let r = request("not an emoji");
    let p = fonts.present_sequence(&r);
    assert!(Arc::ptr_eq(&p, &fonts.present_sequence(&r)));
    assert_eq!(fonts.sequence_cache_stats().shape_calls, 1);
    for n in 0..SEQUENCE_CACHE_ENTRIES * 3 {
        fonts.present_sequence(&request(&format!("plain {n}")));
    }
    let stats = fonts.sequence_cache_stats();
    assert_eq!(stats.shapes, SEQUENCE_CACHE_ENTRIES);
    assert_eq!(stats.negative_shapes, SEQUENCE_CACHE_ENTRIES);
    assert_eq!(stats.rasters, SEQUENCE_CACHE_ENTRIES);
    assert_eq!(stats.pixel_bytes, 0);
    let huge = request(&"😀".repeat(SEQUENCE_MAX_UTF16));
    assert_eq!(
        fonts.present_sequence(&huge).outcome,
        SequenceOutcome::Unsupported(UnsupportedReason::InvalidRequest)
    );
    assert_eq!(fonts.sequence_cache_stats(), stats);
}

#[test]
fn source_size_dpi_style_intent_and_generation_separate_cache_entries() {
    let fonts = fonts();
    let r = request("invalid unit"); // deterministic negative path, no installed font assumption
    let old = fonts.present_sequence(&r);
    let mut variants = Vec::new();
    let mut v = r.clone();
    v.source.push('!');
    variants.push(v);
    let mut v = r.clone();
    v.size = FontSize::new(33.0).unwrap();
    variants.push(v);
    let mut v = r.clone();
    v.dpi = FontSize::new(120.0).unwrap();
    variants.push(v);
    let mut v = r.clone();
    v.style.bold = true;
    variants.push(v);
    let mut v = r.clone();
    v.style.italic = true;
    variants.push(v);
    let mut v = r.clone();
    v.intent = PresentationIntent::Text;
    variants.push(v);
    let mut v = r.clone();
    v.foreground = [255, 0, 0, 128];
    variants.push(v);
    let mut v = r.clone();
    v.palette = 1;
    variants.push(v);
    for v in variants {
        assert!(!Arc::ptr_eq(&old, &fonts.present_sequence(&v)));
    }
    assert_eq!(fonts.sequence_cache_stats().shape_calls, 7); // foreground/palette share shape
    assert_eq!(fonts.sequence_cache_stats().rasters, 9);
    fonts.invalidate_presentations();
    assert_eq!(fonts.sequence_cache_stats().shapes, 0);
    assert_ne!(old.generation, fonts.presentation_generation());
    assert_ne!(old.generation, fonts.present_sequence(&r).generation);
    assert_ne!(
        old.generation.session,
        self::fonts().presentation_generation().session
    );
}

#[test]
#[ignore = "requires documented supporting Segoe UI Emoji/Windows native capabilities"]
fn warm_rasters_foreground_dpi_and_cpu_churn() {
    let fonts = fonts();
    let r = request("♥️");
    let p = fonts.present_sequence(&r);
    complete(&p);
    let before = fonts.sequence_cache_stats();
    for _ in 0..100 {
        assert!(Arc::ptr_eq(&p, &fonts.present_sequence(&r)));
    }
    assert_eq!(fonts.sequence_cache_stats(), before);
    let mut fg = r.clone();
    fg.foreground = [0, 255, 0, 255];
    // Fixed native color heart must not become green (monochrome path does).
    assert_eq!(complete(&p).1, complete(&fonts.present_sequence(&fg)).1);
    let mut dpi = r.clone();
    dpi.dpi = FontSize::new(192.0).unwrap();
    assert!(complete(&fonts.present_sequence(&dpi)).1.bounds.width > complete(&p).1.bounds.width);
    for n in 0..SEQUENCE_CACHE_ENTRIES * 2 {
        let mut variant = r.clone();
        variant.size = FontSize::new(32.0 + n as f32 * 0.1).unwrap();
        fonts.present_sequence(&variant);
    }
    let stats = fonts.sequence_cache_stats();
    assert!(stats.shapes <= SEQUENCE_CACHE_ENTRIES && stats.rasters <= SEQUENCE_CACHE_ENTRIES);
    assert!(stats.pixel_bytes <= SEQUENCE_PIXEL_BUDGET);
    assert_eq!(stats.offscreen_creations, 1);
    fonts.invalidate_presentations();
    fonts.present_sequence(&r);
    assert_eq!(fonts.sequence_cache_stats().offscreen_creations, 1);
    println!("bounded native cache after churn: {stats:?}");
}
