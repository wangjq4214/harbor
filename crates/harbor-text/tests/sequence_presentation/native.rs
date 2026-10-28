use super::*;

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
