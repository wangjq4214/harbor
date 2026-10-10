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
