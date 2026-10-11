use super::super::DwriteState;
use super::*;

fn state() -> DwriteState {
    DwriteState::open_primary_with_sink(
        Some("Consolas"),
        32.0,
        Rc::new(crate::lifecycle::TracingFontLifecycleSink),
    )
    .unwrap()
}
fn request(s: &str) -> SequenceRequest {
    SequenceRequest::new(
        s,
        FontSize::new(32.0).unwrap(),
        FontSize::new(96.0).unwrap(),
    )
}

// Use a real native face, but control metadata independently of installed shaping.
fn validation_run() -> OwnedRun {
    let s = state();
    let layout = make_layout(&s.session, &request("♥")).unwrap();
    let mut run = capture(&layout).unwrap().remove(0);
    run.info.utf16_start = 0;
    run.info.utf16_len = 2;
    run.info.glyphs = vec![1, 2];
    run.info.clusters = vec![0, 0];
    run
}

#[test]
fn validation_requires_one_run_covering_the_entire_source() {
    assert_eq!(validate_runs(&[validation_run()], 2), Ok(()));
    for (start, len) in [(1, 1), (0, 1), (0, 3), (2, 2), (0, 0)] {
        let mut run = validation_run();
        run.info.utf16_start = start;
        run.info.utf16_len = len;
        run.info.clusters = vec![0; len as usize];
        assert_eq!(
            validate_runs(&[run], 2),
            Err(UnsupportedReason::UnjoinedSequence),
            "range {start}..{}",
            start + len
        );
    }
    // Even disjoint runs with full coverage are not an atomic retained unit.
    let mut first = validation_run();
    first.info.utf16_len = 1;
    first.info.clusters = vec![0];
    let mut second = validation_run();
    second.info.utf16_start = 1;
    second.info.utf16_len = 1;
    second.info.clusters = vec![0];
    assert_eq!(
        validate_runs(&[first, second], 2),
        Err(UnsupportedReason::UnjoinedSequence)
    );
    assert_eq!(
        validate_runs(&[validation_run(), validation_run()], 2),
        Err(UnsupportedReason::UnjoinedSequence)
    );
}

#[test]
fn validation_requires_valid_identical_clusters_not_one_glyph() {
    for clusters in [
        vec![],
        vec![0],
        vec![0, 0, 0],
        vec![2, 2],
        vec![0, 2],
        vec![0, 1],
    ] {
        let mut run = validation_run();
        run.info.clusters = clusters;
        assert_eq!(
            validate_runs(&[run], 2),
            Err(UnsupportedReason::UnjoinedSequence)
        );
    }
    // A shared nonzero cluster index and multiple glyphs remain valid.
    let mut run = validation_run();
    run.info.clusters = vec![1, 1];
    assert_eq!(validate_runs(&[run], 2), Ok(()));
    let mut run = validation_run();
    run.info.glyphs.clear();
    assert_eq!(
        validate_runs(&[run], 2),
        Err(UnsupportedReason::UnjoinedSequence)
    );
}

#[test]
fn validation_missing_glyph_wins_over_range_cluster_and_run_errors() {
    assert_eq!(validate_runs(&[], 2), Err(UnsupportedReason::MissingGlyph));
    let mut missing = validation_run();
    missing.info.glyphs = vec![0];
    missing.info.utf16_start = 1;
    missing.info.clusters = vec![9];
    assert_eq!(
        validate_runs(&[missing], 2),
        Err(UnsupportedReason::MissingGlyph)
    );
    let mut invalid = validation_run();
    invalid.info.clusters.clear();
    let mut missing = validation_run();
    missing.info.glyphs.push(0);
    assert_eq!(
        validate_runs(&[invalid, missing], 2),
        Err(UnsupportedReason::MissingGlyph)
    );
}

#[test]
fn rejected_shapes_do_not_consume_face_ids() {
    let s = state();
    let mut next_face = 41;
    for source in ["♥\u{10ffff}", "😀😀", "👩\u{200d}", "👩\u{200d}A"] {
        assert!(
            shape(&s.session, &request(source), &mut next_face).is_err(),
            "{source:?}"
        );
        assert_eq!(next_face, 41, "{source:?}");
    }
    let shaped = shape(&s.session, &request("♥"), &mut next_face).unwrap();
    assert_eq!(next_face, 42);
    assert_eq!(shaped.runs[0].info.face, FaceId::new(41));
}

#[test]
fn controlled_native_unavailability_is_cached_then_invalidated() {
    let s = state();
    s.sequence.borrow_mut().offscreen_failed = true;
    let r = request("♥");
    assert_eq!(
        s.present_sequence(&r).outcome,
        SequenceOutcome::Unsupported(UnsupportedReason::NativeCapability)
    );
    assert_eq!(s.sequence_cache_stats().offscreen_creations, 0);
    assert_eq!(s.sequence_cache_stats().shape_calls, 1);
    s.present_sequence(&r);
    assert_eq!(s.sequence_cache_stats().shape_calls, 1);
    s.invalidate_presentations();
    assert!(matches!(
        s.present_sequence(&r).outcome,
        SequenceOutcome::Complete {
            kind: CompleteKind::Monochrome,
            ..
        }
    ));
}

#[test]
fn native_monochrome_honors_foreground_and_alpha() {
    let s = state();
    let mut r = request("♥\u{fe0e}");
    r.foreground = [255, 0, 0, 128];
    let p = s.present_sequence(&r);
    let SequenceOutcome::Complete {
        kind: CompleteKind::Monochrome,
        tile,
        runs,
    } = &p.outcome
    else {
        panic!("{p:?}");
    };
    assert_eq!(runs.iter().map(|r| r.utf16_len).sum::<u32>(), 2);
    assert_ne!(tile.image_formats, 0);
    assert!(
        tile.rgba
            .chunks_exact(4)
            .all(|p| p[1] == 0 && p[2] == 0 && p[0] == p[3] && p[3] <= 128)
    );
    assert!(tile.rgba.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 128));
    let mut green = r.clone();
    green.foreground = [0, 255, 0, 128];
    let g = s.present_sequence(&green);
    let SequenceOutcome::Complete { tile: g, .. } = &g.outcome else {
        panic!();
    };
    assert_ne!(tile.rgba, g.rgba);
    assert_eq!(s.sequence_cache_stats().shape_calls, 1);
    let mut transparent = r.clone();
    transparent.foreground[3] = 0;
    let p = s.present_sequence(&transparent);
    assert!(
        matches!(&p.outcome, SequenceOutcome::Complete { tile, .. } if tile.rgba.iter().all(|&p| p==0))
    );
}

#[test]
fn native_modern_and_legacy_no_color_keep_outline_pixels_and_formats() {
    let s = state();
    let mut r = request("♥");
    r.intent = PresentationIntent::Emoji;
    r.foreground = [0, 255, 0, 128];
    // Shape the primary monochrome face before requesting color dispatch.
    let shaped = shape(&s.session, &request("♥"), &mut 1).unwrap();
    let mut o = Offscreen::new(&s.session.factory).unwrap();
    let modern = o.raster_pass(&shaped, &r, true).unwrap();
    assert_eq!(modern.0, CompleteKind::Monochrome);
    assert_ne!(modern.1.image_formats, 0);
    o.capabilities.colr_v1 = false;
    o.capabilities.bitmap_svg = false;
    o.context7 = None;
    o.context4 = None;
    assert_eq!(o.raster_pass(&shaped, &r, true).unwrap(), modern);
    assert_eq!(o.raster_pass(&shaped, &r, false).unwrap(), modern);
}

#[test]
fn palette_and_current_foreground_layers_use_distinct_native_brushes() {
    let s = state();
    let o = Offscreen::new(&s.session.factory).unwrap();
    unsafe {
        let foreground = o
            .context
            .CreateSolidColorBrush(&rgba_color([0, 255, 0, 128]), None)
            .unwrap();
        let fixed = DWRITE_COLOR_F {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 0.5,
        };
        let current = o.layer_brush(fixed, u16::MAX, &foreground).unwrap();
        assert_eq!(current, foreground);
        let c = current.GetColor();
        assert_eq!((c.r, c.g, c.b, c.a), (0.0, 1.0, 0.0, 128.0 / 255.0));
        let b = o.layer_brush(fixed, 0, &foreground).unwrap();
        let c = b.GetColor();
        assert_eq!((c.r, c.g, c.b, c.a), (1.0, 0.0, 0.0, 0.5));
    }
}

#[test]
fn unavailable_color_keeps_a_complete_monochrome_outline() {
    let s = state();
    let r = request("♥"); // primary outline
    let mut next_face = 1;
    let shaped = shape(&s.session, &r, &mut next_face).unwrap();
    let o = Offscreen::new(&s.session.factory).unwrap();
    let mut emoji = r.clone();
    emoji.intent = PresentationIntent::Emoji;
    // An unsupported palette on this controlled outline run must not change the
    // source or discard its complete monochrome presentation.
    emoji.palette = u32::MAX;
    let (kind, tile) = o.raster(&shaped, &emoji).unwrap();
    assert_eq!(kind, CompleteKind::Monochrome);
    assert!(!tile.rgba.is_empty());
}

#[test]
fn partial_or_decomposed_zwj_is_not_certified_complete() {
    let s = state();
    for source in ["👩\u{200d}", "👩\u{200d}\u{200d}💻", "👩\u{200d}A", "😀😀"] {
        let p = s.present_sequence(&request(source));
        assert!(
            matches!(p.outcome, SequenceOutcome::Unsupported(_)),
            "{source:?}: {p:?}"
        );
    }
}

#[test]
fn missing_native_glyph_is_explicit_without_source_replacement() {
    let s = state();
    let r = request("♥\u{10ffff}");
    assert_eq!(
        s.present_sequence(&r).outcome,
        SequenceOutcome::Unsupported(UnsupportedReason::MissingGlyph)
    );
    assert_eq!(r.source, "♥\u{10ffff}");
}

#[test]
fn controlled_older_api_cannot_certify_paint_tree_support() {
    let s = state();
    let r = request("♥️");
    let shaped = shape(&s.session, &r, &mut 1).unwrap();
    let mut o = Offscreen::new(&s.session.factory).unwrap();
    o.context7 = None;
    o.context4 = None;
    o.capabilities.colr_v1 = false;
    o.capabilities.bitmap_svg = false;
    match o.raster(&shaped, &r) {
        Ok((_, tile)) => assert_eq!(
            tile.image_formats & DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE.0 as u32,
            0
        ),
        Err(e) => assert!(matches!(
            e,
            UnsupportedReason::ImageFormat | UnsupportedReason::NativeFailure
        )),
    }
}

#[test]
fn native_bounds_and_crop_reject_overflow_without_clipping() {
    let s = state();
    let r = request("♥\u{fe0e}");
    let mut shaped = shape(&s.session, &r, &mut 1).unwrap();
    // Simulate extreme native bearings/geometry without expanding the font-size
    // public limit. D2D sees these run offsets when computing command bounds.
    shaped.runs[0].info.em_size = 1024.0;
    let o = Offscreen::new(&s.session.factory).unwrap();
    assert_eq!(o.raster(&shaped, &r), Err(UnsupportedReason::RasterLimit));
    let pitch = SEQUENCE_TILE_SIDE * 4 + 16;
    let mut pixels = vec![0; (pitch * SEQUENCE_TILE_SIDE) as usize];
    pixels[(pitch * 2 + 4 * 3) as usize..][..4].copy_from_slice(&[0, 0, 128, 128]);
    let map = D2D1_MAPPED_RECT {
        pitch,
        bits: pixels.as_mut_ptr(),
    };
    let tile = unsafe { crop(map, 1, [-7, -11], false) }.unwrap();
    assert_eq!(
        tile.bounds,
        TileBounds {
            left: -4,
            top: -9,
            width: 1,
            height: 1
        }
    );
    assert_eq!(tile.rgba, [128, 0, 0, 128]);
    pixels[3] = 255;
    assert_eq!(
        unsafe { crop(map, 1, [0, 0], false) },
        Err(UnsupportedReason::RasterLimit)
    );
}

#[test]
fn byte_budget_evicts_before_entry_limit_and_releases_on_invalidation() {
    let s = state();
    // Controlled maximum-size tiles fill the CPU byte budget with only eight
    // entries. Exercise the actual presentation insertion/eviction logic, not a
    // second test-only cache implementation or an entry-count-only assertion.
    {
        let mut cache = s.sequence.borrow_mut();
        for n in 0..8 {
            let r = request(&format!("synthetic {n}"));
            let result = Arc::new(SequencePresentation {
                generation: cache.generation,
                outcome: SequenceOutcome::Complete {
                    kind: CompleteKind::Color,
                    tile: PresentationTile {
                        bounds: TileBounds {
                            left: 0,
                            top: 0,
                            width: SEQUENCE_TILE_SIDE,
                            height: SEQUENCE_TILE_SIDE,
                        },
                        image_formats: 0x100,
                        rgba: vec![0; (SEQUENCE_TILE_SIDE * SEQUENCE_TILE_SIDE * 4) as usize],
                    },
                    runs: Vec::new(),
                },
            });
            cache.stats.pixel_bytes += pixel_bytes(&result);
            cache.rasters.push_back((r, result));
        }
        assert_eq!(cache.stats.pixel_bytes, SEQUENCE_PIXEL_BUDGET);
    }
    assert!(matches!(
        s.present_sequence(&request("♥")).outcome,
        SequenceOutcome::Complete { .. }
    ));
    let stats = s.sequence_cache_stats();
    assert_eq!(stats.rasters, 8);
    assert!(stats.pixel_bytes < SEQUENCE_PIXEL_BUDGET);
    s.invalidate_presentations();
    let stats = s.sequence_cache_stats();
    assert_eq!((stats.shapes, stats.rasters, stats.pixel_bytes), (0, 0, 0));
}
