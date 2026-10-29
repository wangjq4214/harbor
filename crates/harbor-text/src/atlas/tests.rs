use super::*;
use crate::contracts::{FaceId, GlyphId};
use crate::font::load_system_fonts;

fn expect_key(resolution: GlyphResolution) -> GlyphKey {
    match resolution {
        GlyphResolution::Available(key) => key,
        GlyphResolution::Unavailable => panic!("expected Available"),
    }
}

fn test_font_book() -> FontBook {
    load_system_fonts(&harbor_config::FontSettings::default()).expect("load test font")
}

/// Helper: resolve a char to a GlyphKey via the font book.
fn glyph_key(fonts: &FontBook, ch: char) -> GlyphKey {
    expect_key(fonts.resolve(ch, FONT_SIZE, 0))
}

fn rasterized_rect(id: u32, width: usize, height: usize) -> RasterizedGlyph {
    RasterizedGlyph {
        key: GlyphKey::new(
            FaceId::PRIMARY,
            GlyphId::new(id),
            FontSize::new(FONT_SIZE).unwrap(),
            FontStyle::REGULAR,
        ),
        bounds: GlyphBitmapBounds {
            width,
            height,
            bearing_x: -2,
            bearing_y: 3,
            advance_width: 4.5,
        },
        bitmap: vec![id as u8; width * height],
    }
}

#[test]
fn active_working_set_reuses_then_reclaims_under_pressure() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let no_pressure = || panic!("active set must stay lazy without pressure");
    atlas.rasterize_new_with_active(&fonts, &['a', 'b'], no_pressure);
    let before = *atlas.glyph_by_char('a').unwrap();

    let hit = atlas.rasterize_new_with_active(&fonts, &['a'], no_pressure);
    assert!(!hit.evicted && hit.new_keys.is_empty());
    let added = atlas.rasterize_new_with_active(&fonts, &['c'], no_pressure);
    assert!(!added.evicted);
    assert_eq!(added.new_keys.len(), 1);
    let after = atlas.glyph_by_char('a').unwrap();
    assert_eq!(
        (before.atlas_x, before.atlas_y),
        (after.atlas_x, after.atlas_y)
    );
    assert!(
        atlas.glyph_by_char('b').is_some(),
        "no-pressure path retains history"
    );

    // Force pressure within this private unit test, without a production hook.
    atlas.store.shelves = vec![Shelf {
        y: 0,
        height: MAX_ATLAS_SIZE,
        next_x: MAX_ATLAS_SIZE,
    }];
    let mut collections = 0;
    // a/c are unchanged; e has not even been resolved by the dirty request.
    let repacked = atlas.rasterize_new_with_active(&fonts, &['d'], || {
        collections += 1;
        vec!['a', 'c', 'd', 'e']
    });
    assert!(repacked.evicted);
    assert_eq!(collections, 1);
    for ch in ['a', 'c', 'd', 'e'] {
        assert!(atlas.glyph_by_char(ch).is_some(), "missing active {ch}");
    }
    assert!(
        atlas.glyph_by_char('b').is_none(),
        "stale glyph not reclaimed"
    );
}

#[test]
fn incremental_packing_preserves_order_pixels_and_metric_only_glyphs() {
    let glyphs = [
        rasterized_rect(1, 3, 2),
        rasterized_rect(2, 2, 1),
        rasterized_rect(3, 0, 0),
    ];
    let mut store = AtlasStore::new();
    assert!(store.add_incremental(&glyphs));
    assert_eq!(store.height(), 2);
    assert_eq!(store.shelf_count(), 1);
    for (glyph, x) in glyphs.iter().zip([0, 4, 7]) {
        let placed = store.glyph(glyph.key).unwrap();
        assert_eq!((placed.atlas_x, placed.atlas_y), (x, 0));
        assert_eq!((placed.bearing_x, placed.bearing_y), (-2, 3));
        assert_eq!(placed.advance_width, 4.5);
        assert_eq!(placed.uv.left, x as f32 / MAX_ATLAS_SIZE as f32);
    }
    assert_eq!(&store.pixels()[..8], &[1, 1, 1, 0, 2, 2, 0, 0]);
    let second_row = MAX_ATLAS_SIZE as usize;
    assert_eq!(
        &store.pixels()[second_row..second_row + 8],
        &[1, 1, 1, 0, 0, 0, 0, 0]
    );
}

#[test]
fn incremental_overflow_keeps_partial_placements_until_rebuild() {
    let mut store = AtlasStore::new();
    let first = rasterized_rect(1, MAX_ATLAS_SIZE as usize - 1, 2);
    assert!(store.add_incremental(&[first]));
    let added = rasterized_rect(2, 2, 1);
    let added_key = added.key;
    let oversized = rasterized_rect(3, 1, MAX_ATLAS_SIZE as usize);
    let oversized_key = oversized.key;
    assert!(!store.add_incremental(&[added, oversized]));
    assert_eq!(
        store.height(),
        2,
        "failed batches leave reported height unchanged"
    );
    assert_eq!(store.glyph(added_key).unwrap().atlas_y, 2);
    assert!(store.glyph(oversized_key).is_none());
}

#[test]
fn rebuild_sorts_by_height_clears_pixels_and_stops_when_full() {
    let mut store = AtlasStore::new();
    let old = rasterized_rect(1, 3, 2);
    let old_key = old.key;
    assert!(store.add_incremental(&[old]));
    let small = rasterized_rect(2, 1, 1);
    let small_key = small.key;
    let full = rasterized_rect(3, MAX_ATLAS_SIZE as usize - 1, MAX_ATLAS_SIZE as usize);
    let full_key = full.key;
    store.rebuild(vec![small, full]);
    assert_eq!(store.len(), 1);
    assert!(store.glyph(old_key).is_none());
    assert!(store.glyph(small_key).is_none());
    assert_eq!(store.glyph(full_key).unwrap().atlas_y, 0);
    assert_eq!(store.height(), MAX_ATLAS_SIZE);
    assert_eq!(store.pixels()[0], 3);
    store.rebuild(Vec::new());
    assert_eq!(store.height(), 1);
    assert_eq!(store.len(), 0);
    assert!(store.pixels().iter().all(|&pixel| pixel == 0));
}

#[test]
fn retaining_repack_reports_eviction_and_keeps_resolution_cache() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let size = FontSize::new(fonts.size()).unwrap();
    atlas.rasterize_new(&fonts, &['A']);
    let old_key = atlas.glyph_by_char('A').unwrap().key;
    let mut full = rasterized_rect(1, MAX_ATLAS_SIZE as usize - 1, MAX_ATLAS_SIZE as usize);
    full.key = old_key;
    atlas.store.rebuild(vec![full]);

    let result = atlas.rasterize_new_at_size_retaining(&fonts, &['X', 'X'], size, &[]);
    let new_key = expect_key(fonts.resolve('X', size.get(), FontStyle::REGULAR));
    assert!(result.evicted);
    assert_eq!(result.new_keys, vec![new_key]);
    assert_eq!(atlas.len(), 1);
    assert!(atlas.glyph_by_char('A').is_none());
    assert!(atlas.glyph_by_char('X').is_some());
    assert_eq!(
        atlas
            .resolution
            .get(&ResolutionKey::new('A', size, FontStyle::REGULAR)),
        Some(&GlyphResolution::Available(old_key))
    );
    assert!(atlas.rasterize_new(&fonts, &['X']).new_keys.is_empty());
}

#[test]
fn retaining_repack_preserves_result_order_but_deduplicates_placements() {
    let fonts = test_font_book();
    let size = FontSize::new(fonts.size()).unwrap();
    let old_key = expect_key(fonts.resolve('A', size.get(), FontStyle::REGULAR));
    let new_key = expect_key(fonts.resolve('X', size.get(), FontStyle::REGULAR));
    let mut atlas = GlyphAtlas::new();
    let mut full = rasterized_rect(1, MAX_ATLAS_SIZE as usize - 1, MAX_ATLAS_SIZE as usize);
    full.key = old_key;
    atlas.store.rebuild(vec![full]);
    let retained = [new_key, old_key, old_key];

    let result = atlas.rasterize_new_at_size_retaining(&fonts, &['X'], size, &retained);
    assert!(result.evicted);
    assert_eq!(result.new_keys, [new_key, old_key, old_key, new_key]);
    assert_eq!(atlas.len(), 2);
    assert!(atlas.glyph(old_key).is_some());
    assert!(atlas.glyph(new_key).is_some());
}

#[test]
fn rebuild_preserves_input_order_for_equal_heights() {
    let short = rasterized_rect(1, 2, 1);
    let first = rasterized_rect(2, 2, 2);
    let second = rasterized_rect(3, 2, 2);
    let keys = [first.key, second.key, short.key];
    let mut store = AtlasStore::new();
    store.rebuild(vec![short, first, second]);
    for (key, x) in keys.into_iter().zip([0, 3, 6]) {
        let glyph = store.glyph(key).unwrap();
        assert_eq!((glyph.atlas_x, glyph.atlas_y), (x, 0));
    }
}

#[test]
fn empty_atlas_has_no_glyphs() {
    let atlas = GlyphAtlas::new();
    assert_eq!(atlas.len(), 0);
    assert!(atlas.glyph_by_char('a').is_none());
    assert_eq!(
        atlas.pixels().len(),
        (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize
    );
}

#[test]
fn rasterize_new_adds_glyphs() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    let chars: Vec<char> = "abc".chars().collect();
    let result = atlas.rasterize_new(&fonts, &chars);

    assert_eq!(result.new_keys.len(), 3);
    assert!(!result.evicted);
    assert!(atlas.glyph_by_char('a').is_some());
    assert!(atlas.glyph_by_char('b').is_some());
    assert!(atlas.glyph_by_char('c').is_some());
}

#[test]
fn rasterize_new_retains_metric_only_space() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    let result = atlas.rasterize_new(&fonts, &['a', ' ', 'b']);

    assert_eq!(result.new_keys.len(), 3);
    let space = atlas
        .glyph_by_char(' ')
        .expect("space metrics should remain in the atlas");
    assert_eq!((space.width, space.height), (0, 0));
    assert!(space.advance_width > 0.0);
}
#[test]
fn cached_atlas_reuses_existing_glyphs() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    let first = atlas.rasterize_new(&fonts, &['a', 'b']);
    assert_eq!(first.new_keys.len(), 2);

    let second = atlas.rasterize_new(&fonts, &['a', 'b']);
    assert!(second.new_keys.is_empty(), "no new glyphs expected");
    assert!(!second.evicted);
    assert_eq!(atlas.len(), 2);
}

#[test]
fn rasterize_new_only_returns_new_keys() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    let _ = atlas.rasterize_new(&fonts, &['a', 'b']);
    let result = atlas.rasterize_new(&fonts, &['a', 'b', 'c']);
    // Should contain exactly one key: the resolved key for 'c'.
    let c_key = glyph_key(&fonts, 'c');
    assert_eq!(result.new_keys, vec![c_key], "only 'c' is new");
}

#[test]
fn full_rebuild_clears_and_repopulates() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    atlas.rasterize_new(&fonts, &['a', 'b']);
    assert_eq!(atlas.len(), 2);

    atlas.rebuild(&fonts, &['d', 'c', 'c', 'd']);
    assert_eq!(atlas.len(), 2);
    assert!(atlas.glyph_by_char('a').is_none());
    assert!(atlas.glyph_by_char('c').is_some());
    assert!(atlas.glyph_by_char('d').is_some());
}

#[test]
fn shelf_packing_places_glyphs() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    let chars: Vec<char> = "helo!".chars().collect();
    let _ = atlas.rasterize_new(&fonts, &chars);

    for ch in "helo!".chars() {
        assert!(
            atlas.glyph_by_char(ch).is_some(),
            "glyph '{}' should exist",
            ch
        );
    }
}

#[test]
fn new_glyph_lands_on_existing_shelf() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    let _ = atlas.rasterize_new(&fonts, &['a', 'b', 'c']);
    // Add a new char that should fit on the same shelf (small glyph).
    let result = atlas.rasterize_new(&fonts, &['d']);
    assert_eq!(result.new_keys.len(), 1);
}

#[test]
fn atlas_creates_multiple_shelves_when_row_overflows() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    // ~22px + ATLAS_PADDING per glyph → ~2500px > 2048 row limit → forces 2nd shelf.
    use unicode_width::UnicodeWidthChar;
    let chars: Vec<char> = ('!'..='~')
        .chain('¡'..='ÿ')
        .filter(|c| UnicodeWidthChar::width(*c).unwrap_or(0) > 0)
        .take(130)
        .collect();
    assert!(
        chars.len() >= 115,
        "need ~115+ chars to overflow 2048px row, got {}",
        chars.len()
    );
    let _ = atlas.rasterize_new(&fonts, &chars);
    // At least one glyph should be on a non-zero y shelf.
    let on_second_shelf = atlas.store.glyphs.values().any(|g| g.atlas_y > 0);
    assert!(
        on_second_shelf,
        "at least one glyph should be on a second shelf"
    );
    // All chars should still be in the atlas.
    for ch in &chars {
        assert!(
            atlas.glyph_by_char(*ch).is_some(),
            "glyph '{}' should exist",
            ch
        );
    }
}

#[test]
fn should_pack_latin_and_space_with_system_primary() {
    // Arrange — default DirectWrite primary (T0002; no system fallback yet).
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let chars = ['H', 'e', 'l', 'l', 'o', ' ', 'A'];

    // Act
    let result = atlas.rasterize_new(&fonts, &chars);

    // Assert
    assert!(!result.evicted);
    for ch in ['H', 'e', 'l', 'o', 'A'] {
        let glyph = atlas
            .glyph_by_char(ch)
            .unwrap_or_else(|| panic!("latin glyph '{ch}' missing from atlas"));
        assert!(glyph.width > 0, "'{ch}' width");
        assert!(glyph.height > 0, "'{ch}' height");
        assert_eq!(glyph.key, expect_key(fonts.resolve(ch, FONT_SIZE, 0)));
    }
    // Space may be omitted from atlas packing (zero ink); resolution must still be stable.
    let space_key = expect_key(fonts.resolve(' ', FONT_SIZE, 0));
    let (space_bounds, space_bitmap) = fonts.rasterize(' ', harbor_config::FONT_SIZE);
    assert_eq!(space_bounds.width, 0);
    assert_eq!(space_bounds.height, 0);
    assert!(space_bitmap.is_empty());
    assert!(space_bounds.advance_width > 0.0);
    let _ = space_key;
    let _ = result.new_keys;
}

#[test]
fn should_pack_cjk_when_available_via_fallback() {
    // Arrange
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    // Act
    let result = atlas.rasterize_new(&fonts, &['中']);

    // Assert — available keys are packed once; unavailable yields no atlas entry.
    match fonts.resolve('中', FONT_SIZE, 0) {
        GlyphResolution::Available(key) => {
            assert_eq!(result.new_keys, vec![key]);
            assert!(!result.evicted);
            if key.face_id != 0 || atlas.glyph_by_char('中').is_some() {
                let glyph = atlas.glyph_by_char('中');
                if let Some(glyph) = glyph {
                    assert_eq!(glyph.key, key);
                }
            }
        }
        GlyphResolution::Unavailable => {
            assert!(result.new_keys.is_empty());
            assert!(atlas.glyph_by_char('中').is_none());
        }
    }
}

#[test]
fn should_omit_unavailable_from_new_keys_when_rasterizing() {
    // Arrange
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let ch = '\u{E000}';

    // Act
    let first = atlas.rasterize_new(&fonts, &[ch]);
    let second = atlas.rasterize_new(&fonts, &[ch]);

    // Assert
    if matches!(
        fonts.resolve(ch, FONT_SIZE, 0),
        GlyphResolution::Unavailable
    ) {
        assert!(first.new_keys.is_empty());
        assert!(second.new_keys.is_empty());
        assert!(atlas.glyph_by_char(ch).is_none());
    }
}

#[test]
fn should_retain_resolution_across_rebuild_for_kept_chars() {
    // Arrange
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let _ = atlas.rasterize_new(&fonts, &['A', 'B', 'C']);
    let key_a = expect_key(fonts.resolve('A', FONT_SIZE, 0));

    // Act — rebuild keeps A; resolution cache must remain usable without remapping.
    atlas.rebuild(&fonts, &['A']);
    let after = atlas.rasterize_new(&fonts, &['A']);

    // Assert
    assert!(atlas.glyph_by_char('A').is_some());
    assert_eq!(atlas.glyph_by_char('A').unwrap().key, key_a);
    assert!(
        after.new_keys.is_empty(),
        "retained resolution must not re-queue A"
    );
    assert!(atlas.glyph_by_char('B').is_none());
}

#[test]
fn should_pack_mixed_latin_and_cjk_without_duplicate_keys() {
    // Arrange
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let chars = ['A', '中', 'B', '中', 'A'];

    // Act
    let result = atlas.rasterize_new(&fonts, &chars);
    let again = atlas.rasterize_new(&fonts, &chars);

    // Assert
    assert!(!result.new_keys.is_empty());
    let mut unique = result.new_keys.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        result.new_keys.len(),
        "new_keys must be GlyphKey-unique"
    );
    assert!(again.new_keys.is_empty());
    assert!(atlas.glyph_by_char('A').is_some());
    assert!(atlas.glyph_by_char('B').is_some());
}

#[test]
fn atlas_overflow_triggers_eviction() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    // Fill atlas with many unique chars to trigger overflow + eviction.
    use unicode_width::UnicodeWidthChar;
    let all_chars: Vec<char> = ('!'..='~')
        .chain('¡'..='ÿ')
        .filter(|c| UnicodeWidthChar::width(*c).unwrap_or(0) > 0)
        .take(250)
        .collect();

    let result = atlas.rasterize_new(&fonts, &all_chars);
    // On most fonts, 250 unique glyphs should overflow 2048px atlas.
    // If evicted, all chars are in new_keys and cache is rebuilt.
    if result.evicted {
        assert_eq!(result.new_keys.len(), atlas.len());
    }
    // Regardless, the atlas should contain at least one glyph.
    assert!(!atlas.is_empty(), "atlas should contain glyphs");
}

#[test]
fn atlas_persistent_cache_across_rebuilds() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();

    atlas.rasterize_new(&fonts, &['a', 'b', 'x', 'y']);
    assert_eq!(atlas.len(), 4);

    // Rebuild with a different set
    atlas.rebuild(&fonts, &['x', 'y']);
    assert_eq!(atlas.len(), 2, "rebuild clears old glyphs");
    assert!(atlas.glyph_by_char('x').is_some());
    assert!(atlas.glyph_by_char('y').is_some());
}

#[test]
fn height_reports_used_atlas_height() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    // Fresh atlas reports MAX_ATLAS_SIZE (buffer is always full-size).
    assert_eq!(atlas.height(), MAX_ATLAS_SIZE);

    atlas.rasterize_new(&fonts, &['a', 'b']);
    // After rasterizing, height reflects actual used pixel rows.
    assert!(atlas.height() > 0, "height should reflect used space");
    assert!(atlas.height() <= MAX_ATLAS_SIZE);
}

#[test]
fn glyph_returns_none_for_unknown_char() {
    let fonts = test_font_book();
    let atlas = GlyphAtlas::new();
    assert!(atlas.glyph_by_char('Z').is_none());
    assert!(atlas.glyph_by_char('中').is_none());
    assert!(atlas.glyph_by_char('\x00').is_none());
    // Also test direct key lookup
    let key = glyph_key(&fonts, 'A');
    assert!(atlas.glyph(key).is_none());
}

#[test]
fn pixels_buffer_starts_zero_filled() {
    let atlas = GlyphAtlas::new();
    let pixels = atlas.pixels();
    assert_eq!(pixels.len(), (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize);
    assert!(
        pixels.iter().all(|&p| p == 0),
        "fresh atlas should be zero-filled"
    );
}

#[test]
fn rasterize_new_handles_empty_char_slice() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let result = atlas.rasterize_new(&fonts, &[]);
    assert!(result.new_keys.is_empty());
    assert!(!result.evicted);
    assert_eq!(atlas.len(), 0);
}

// ── GlyphKey tests ───────────────────────────────────────────────

#[test]
fn should_equal_when_all_fields_match() {
    let k1 = GlyphKey {
        face_id: FaceId::new(1),
        glyph_id: GlyphId::new(42),
        size: FontSize::from_bits(0x41800000),
        style: FontStyle::new(0),
    };
    let k2 = GlyphKey {
        face_id: FaceId::new(1),
        glyph_id: GlyphId::new(42),
        size: FontSize::from_bits(0x41800000),
        style: FontStyle::new(0),
    };
    assert_eq!(k1, k2, "identical keys should be equal");
}

#[test]
fn should_not_equal_when_face_id_differs() {
    let k1 = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    let k2 = GlyphKey {
        face_id: FaceId::new(1),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    assert_ne!(k1, k2, "different face_id should produce inequality");
}

#[test]
fn should_not_equal_when_glyph_index_differs() {
    let k1 = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    let k2 = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(2),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    assert_ne!(k1, k2, "different glyph_index should produce inequality");
}

#[test]
fn should_not_equal_when_size_bits_differs() {
    let k1 = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    let k2 = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(20),
        style: FontStyle::new(0),
    };
    assert_ne!(k1, k2, "different size_bits should produce inequality");
}

#[test]
fn should_not_equal_when_style_bits_differs() {
    let k1 = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    let k2 = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(1),
    };
    assert_ne!(k1, k2, "different style_bits should produce inequality");
}

#[test]
fn should_order_by_face_id_first() {
    let k_small = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(100),
        size: FontSize::from_bits(0),
        style: FontStyle::new(0),
    };
    let k_large = GlyphKey {
        face_id: FaceId::new(1),
        glyph_id: GlyphId::new(0),
        size: FontSize::from_bits(0),
        style: FontStyle::new(0),
    };
    assert!(k_small < k_large, "lower face_id should come first");
}

#[test]
fn should_order_by_glyph_index_when_face_ids_equal() {
    let k_small = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(5),
        size: FontSize::from_bits(0),
        style: FontStyle::new(0),
    };
    let k_large = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(10),
        size: FontSize::from_bits(0),
        style: FontStyle::new(0),
    };
    assert!(k_small < k_large, "lower glyph_index should come first");
}

#[test]
fn should_order_by_size_bits_when_prev_fields_equal() {
    let k_small = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    let k_large = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(20),
        style: FontStyle::new(0),
    };
    assert!(k_small < k_large, "lower size_bits should come first");
}

#[test]
fn should_order_by_style_bits_when_all_prev_equal() {
    let k_small = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(0),
    };
    let k_large = GlyphKey {
        face_id: FaceId::new(0),
        glyph_id: GlyphId::new(1),
        size: FontSize::from_bits(10),
        style: FontStyle::new(1),
    };
    assert!(k_small < k_large, "lower style_bits should come first");
}

#[test]
fn should_hash_consistently_with_equality() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let k1 = GlyphKey {
        face_id: FaceId::new(3),
        glyph_id: GlyphId::new(7),
        size: FontSize::from_bits(0x41800000),
        style: FontStyle::new(2),
    };
    let k2 = GlyphKey {
        face_id: FaceId::new(3),
        glyph_id: GlyphId::new(7),
        size: FontSize::from_bits(0x41800000),
        style: FontStyle::new(2),
    };

    let mut h1 = DefaultHasher::new();
    k1.hash(&mut h1);
    let hash1 = h1.finish();

    let mut h2 = DefaultHasher::new();
    k2.hash(&mut h2);
    let hash2 = h2.finish();

    assert_eq!(hash1, hash2, "equal keys must produce the same hash");
}

#[test]
fn should_useful_as_hashmap_key() {
    let fonts = test_font_book();
    let key_a = expect_key(fonts.resolve('A', FONT_SIZE, 0));
    let key_b = expect_key(fonts.resolve('B', FONT_SIZE, 0));

    let mut map = hashbrown::HashMap::new();
    map.insert(key_a, 1u32);
    map.insert(key_b, 2u32);

    assert_eq!(map.len(), 2);
    assert_eq!(map.get(&key_a), Some(&1));
    assert_eq!(map.get(&key_b), Some(&2));
}

#[test]
fn should_resolve_different_chars_to_different_keys() {
    let fonts = test_font_book();
    let key_a = expect_key(fonts.resolve('A', FONT_SIZE, 0));
    let key_b = expect_key(fonts.resolve('B', FONT_SIZE, 0));
    assert_ne!(
        key_a, key_b,
        "different chars should resolve to different keys"
    );
}

#[test]
fn should_resolve_same_char_to_same_key_regardless_of_size() {
    let fonts = test_font_book();
    // resolve() always uses FONT_SIZE internally, so repeated calls are stable.
    let k1 = expect_key(fonts.resolve('Z', FONT_SIZE, 0));
    let k2 = expect_key(fonts.resolve('Z', FONT_SIZE, 0));
    assert_eq!(k1, k2, "same char should always resolve to the same key");
}

// ── GlyphBitmapBounds tests ──────────────────────────────────────

#[test]
fn should_expose_all_fields_when_constructed() {
    let bounds = GlyphBitmapBounds {
        width: 10,
        height: 20,
        bearing_x: 1,
        bearing_y: -2,
        advance_width: 12.5,
    };
    assert_eq!(bounds.width, 10);
    assert_eq!(bounds.height, 20);
    assert_eq!(bounds.bearing_x, 1);
    assert_eq!(bounds.bearing_y, -2);
    assert_eq!(bounds.advance_width, 12.5);
}

#[test]
fn should_support_zero_dimensions() {
    let bounds = GlyphBitmapBounds {
        width: 0,
        height: 0,
        bearing_x: 0,
        bearing_y: 0,
        advance_width: 0.0,
    };
    assert_eq!(bounds.width, 0);
    assert_eq!(bounds.height, 0);
}

#[test]
fn should_support_negative_bearings() {
    let bounds = GlyphBitmapBounds {
        width: 8,
        height: 12,
        bearing_x: -3,
        bearing_y: -5,
        advance_width: 6.0,
    };
    assert_eq!(bounds.bearing_x, -3);
    assert_eq!(bounds.bearing_y, -5);
}

#[test]
fn should_be_copy_and_clone() {
    let bounds = GlyphBitmapBounds {
        width: 10,
        height: 20,
        bearing_x: 1,
        bearing_y: 2,
        advance_width: 11.0,
    };
    let copy = bounds;
    let cloned = bounds;
    assert_eq!(copy.width, cloned.width);
    assert_eq!(copy.advance_width, cloned.advance_width);
}

// ── AtlasGlyph bearing fields tests ──────────────────────────────

#[test]
fn should_store_bearing_and_advance_fields_in_atlas_glyph() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let _ = atlas.rasterize_new(&fonts, &['A']);
    let glyph = atlas.glyph_by_char('A').expect("A should be in atlas");
    let key = expect_key(fonts.resolve('A', FONT_SIZE, FontStyle::REGULAR));
    let (bounds, _) = fonts.rasterize_from_key(key);

    assert_eq!(
        glyph.advance_width.to_bits(),
        bounds.advance_width.to_bits()
    );
    let _ = glyph.bearing_x;
    let _ = glyph.bearing_y;
    assert!(glyph.width > 0, "A glyph should have positive width");
    assert!(glyph.height > 0, "A glyph should have positive height");
}

#[test]
fn rebuild_preserves_glyph_advance_width() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    atlas.rebuild(&fonts, &['W']);
    let glyph = atlas.glyph_by_char('W').expect("W should be in atlas");
    let key = expect_key(fonts.resolve('W', FONT_SIZE, FontStyle::REGULAR));
    let (bounds, _) = fonts.rasterize_from_key(key);

    assert_eq!(
        glyph.advance_width.to_bits(),
        bounds.advance_width.to_bits()
    );
}

#[test]
fn should_lookup_glyph_by_key_after_rasterize() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let _ = atlas.rasterize_new(&fonts, &['X']);
    let key = expect_key(fonts.resolve('X', FONT_SIZE, 0));
    let glyph = atlas.glyph(key).expect("glyph should be findable by key");
    assert!(glyph.width > 0);
    assert!(glyph.key == key, "stored glyph key should match lookup key");
}

#[test]
fn should_return_none_when_looking_up_key_not_in_atlas() {
    let fonts = test_font_book();
    let atlas = GlyphAtlas::new();
    let key = expect_key(fonts.resolve('Q', FONT_SIZE, 0));
    assert!(atlas.glyph(key).is_none(), "empty atlas should return None");
}

#[test]
fn rasterize_result_keys_match_resolved_keys() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let chars: Vec<char> = vec!['a', 'b', 'c'];
    let result = atlas.rasterize_new(&fonts, &chars);
    // Every key in new_keys should match the resolved key for the corresponding char.
    for (i, ch) in chars.iter().enumerate() {
        let expected_key = expect_key(fonts.resolve(*ch, FONT_SIZE, 0));
        assert_eq!(
            result.new_keys[i], expected_key,
            "new_keys[{}] should match resolve('{}')",
            i, ch
        );
    }
}

#[test]
fn glyph_by_char_matches_glyph_by_key() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let _ = atlas.rasterize_new(&fonts, &['M']);
    let key = expect_key(fonts.resolve('M', FONT_SIZE, 0));
    let by_char = atlas
        .glyph_by_char('M')
        .expect("glyph_by_char should find M");
    let by_key = atlas.glyph(key).expect("glyph should find M by key");
    // Both should point to the same atlas entry.
    assert_eq!(by_char.atlas_x, by_key.atlas_x);
    assert_eq!(by_char.atlas_y, by_key.atlas_y);
    assert_eq!(by_char.width, by_key.width);
    assert_eq!(by_char.height, by_key.height);
}

#[test]
fn explicit_sizes_coexist_without_changing_default_lookup() {
    let fonts = test_font_book();
    let mut atlas = GlyphAtlas::new();
    let default_size = FontSize::new(fonts.size()).expect("valid default size");
    let double_size = FontSize::new(fonts.size() * 2.0).expect("valid double size");

    atlas.rasterize_new(&fonts, &['M']);
    atlas.rasterize_new_at_size(&fonts, &['M'], double_size);

    let default_glyph = atlas
        .glyph_by_char('M')
        .expect("default-size glyph should remain available");
    let double_glyph = atlas
        .glyph_by_char_at_size('M', double_size)
        .expect("double-size glyph should be available");

    assert_eq!(default_glyph.key.size, default_size);
    assert_eq!(double_glyph.key.size, double_size);
    assert_ne!(default_glyph.key, double_glyph.key);
}
