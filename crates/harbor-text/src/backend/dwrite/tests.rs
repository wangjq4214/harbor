use std::rc::Rc;
use std::{env, fs, path::PathBuf};

use crate::lifecycle::{FontLifecycleEvent, FontLifecycleSink, RecordingFontLifecycleSink};

use super::*;

#[test]
fn should_assign_contiguous_registry_ids_without_consuming_ids_for_duplicates() {
    let (factory, _, _) = open_factory_fallback_locale().expect("open factory");
    let base: IDWriteFactory = factory.cast().expect("base factory");
    let mut collection = None;
    unsafe { base.GetSystemFontCollection(&mut collection, false) }.expect("system collection");
    let collection = collection.expect("non-null collection");
    let mut fonts = Vec::new();
    let mut fingerprints = Vec::new();
    for index in 0..unsafe { collection.GetFontFamilyCount() } {
        let family = unsafe { collection.GetFontFamily(index) }.expect("system family");
        let font = unsafe {
            family.GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )
        }
        .expect("matching font");
        let face = unsafe { font.CreateFontFace() }.expect("system face");
        let fingerprint = face_fingerprint(&face).expect("fingerprint");
        if !fingerprints.contains(&fingerprint) {
            fonts.push(font);
            fingerprints.push(fingerprint);
            if fonts.len() == 3 {
                break;
            }
        }
    }
    assert_eq!(fonts.len(), 3, "need three distinct system faces");
    let primary = unsafe { fonts[0].CreateFontFace() }.expect("primary face");
    let mut registry = NativeFaceRegistry::with_primary(primary.clone()).expect("registry");
    assert_eq!(registry.len(), 1);
    assert_eq!(
        registry.register(primary).expect("duplicate primary"),
        PRIMARY_FACE_ID
    );
    let reopened = unsafe { fonts[0].CreateFontFace() }.expect("reopen primary");
    assert_eq!(
        registry.register(reopened).expect("reopened primary"),
        PRIMARY_FACE_ID
    );
    assert_eq!(registry.len(), 1);

    let second = unsafe { fonts[1].CreateFontFace() }.expect("second face");
    assert_eq!(
        registry.register(second.clone()).expect("register second"),
        FaceId::new(1)
    );
    assert_eq!(
        registry.register(second).expect("duplicate second"),
        FaceId::new(1)
    );
    let reopened = unsafe { fonts[1].CreateFontFace() }.expect("reopen second");
    assert_eq!(
        registry.register(reopened).expect("reopened second"),
        FaceId::new(1)
    );
    assert_eq!(registry.len(), 2);

    let third = unsafe { fonts[2].CreateFontFace() }.expect("third face");
    assert_eq!(
        registry.register(third).expect("register third"),
        FaceId::new(2)
    );
    assert_eq!(registry.len(), 3);
    for (index, fingerprint) in fingerprints.iter().enumerate() {
        let retained = registry
            .get(FaceId::new(index as u64))
            .expect("retained face");
        assert!(face_fingerprint(retained).expect("retained fingerprint") == *fingerprint);
    }
    assert!(registry.get(FaceId::new(3)).is_none());
    assert!(registry.get(FaceId::new(u64::MAX)).is_none());
}

#[test]
fn should_reject_unknown_face_before_out_of_range_glyph_when_rasterizing() {
    let state = open_primary();
    let key = GlyphKey {
        face_id: FaceId::new(u64::MAX),
        glyph_id: GlyphId::new(u32::MAX),
        size: FontSize::from_bits(harbor_config::FONT_SIZE.to_bits()),
        style: FontStyle::new(0),
    };
    let error = state.rasterizer.rasterize_inner(key).unwrap_err();
    assert_eq!(error.to_string(), format!("unknown face_id {}", u64::MAX));
    let (bounds, bitmap) = state.rasterize(key);
    assert_eq!(bounds.width, 0);
    assert_eq!(bounds.height, 0);
    assert_eq!(bounds.bearing_x, 0);
    assert_eq!(bounds.bearing_y, 0);
    assert_eq!(bounds.advance_width, 0.0);
    assert!(bitmap.is_empty());
}

fn windows_fonts_dir() -> PathBuf {
    env::var_os("WINDIR")
        .or_else(|| env::var_os("SYSTEMROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

fn configured_font_path() -> Option<PathBuf> {
    let fonts_dir = windows_fonts_dir();
    [
        "CascadiaMono.ttf",
        "CascadiaCode.ttf",
        "consola.ttf",
        "cour.ttf",
    ]
    .into_iter()
    .map(|name| fonts_dir.join(name))
    .find(|path| path.is_file())
}

fn configured_collection_path() -> Option<PathBuf> {
    let fonts_dir = windows_fonts_dir();
    ["msyh.ttc", "simsun.ttc"]
        .into_iter()
        .map(|name| fonts_dir.join(name))
        .find(|path| path.is_file())
}

fn collection_family_count() -> u32 {
    let factory: IDWriteFactory = unsafe {
        DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).expect("create DirectWrite factory")
    };
    let mut collection = None;
    unsafe {
        factory
            .GetSystemFontCollection(&mut collection, false)
            .expect("get system font collection");
    }
    unsafe {
        collection
            .expect("system font collection")
            .GetFontFamilyCount()
    }
}

fn open_primary() -> DwriteState {
    DwriteState::open_system_primary().expect("open system primary")
}

fn expect_available(resolution: GlyphResolution) -> GlyphKey {
    match resolution {
        GlyphResolution::Available(key) => key,
        GlyphResolution::Unavailable => panic!("expected Available resolution"),
    }
}

#[test]
fn should_open_system_primary_for_latin_smoke() {
    let _ = DwriteState::open_system_primary().expect("system primary available for font smoke");
}

#[test]
fn should_open_configured_primary_from_system_font_path() {
    let Some(path) = configured_font_path() else {
        return;
    };

    let state = DwriteState::open_configured_primary(&path).expect("configured font opens");
    let metrics = state.font_metrics(harbor_config::FONT_SIZE);
    assert!(metrics.cell_width > 0.0);
    assert!(metrics.line_height > 0.0);

    let key = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
    let (bounds, bitmap) = state.rasterize(key);
    assert!(bounds.width > 0);
    assert!(bounds.height > 0);
    assert_eq!(bitmap.len(), bounds.width * bounds.height);
}

#[test]
fn should_open_first_face_of_available_collection() {
    let Some(path) = configured_collection_path() else {
        return;
    };

    let state = DwriteState::open_configured_primary(&path).expect("configured collection opens");
    let metrics = state.font_metrics(harbor_config::FONT_SIZE);
    assert!(metrics.cell_width > 0.0);
    assert!(metrics.line_height > 0.0);
    let _ = state.resolve('A', harbor_config::FONT_SIZE, 0);
}

#[test]
fn should_leave_system_collection_unchanged_for_configured_primary() {
    let Some(path) = configured_font_path() else {
        return;
    };
    let before = collection_family_count();
    {
        let _state = DwriteState::open_configured_primary(&path).expect("configured font opens");
        assert_eq!(collection_family_count(), before);
    }
    assert_eq!(collection_family_count(), before);
}

#[test]
fn should_reject_empty_configured_path_before_directwrite_call() {
    let path = Path::new("");
    let error = match DwriteState::open_configured_primary(path) {
        Ok(_) => panic!("empty configured path unexpectedly opened"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(
        message.contains("configured font path is empty"),
        "{message}"
    );
}

#[test]
fn should_include_missing_configured_path_in_error() {
    let path = env::temp_dir().join(format!("harbor-missing-font-{}.ttf", std::process::id()));
    let _ = fs::remove_file(&path);
    let error = match DwriteState::open_configured_primary(&path) {
        Ok(_) => panic!("missing font unexpectedly opened"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(message.contains(&path.display().to_string()), "{message}");
}

#[test]
fn should_include_unsupported_configured_path_in_error() {
    let path = env::temp_dir().join(format!("harbor-invalid-font-{}.bin", std::process::id()));
    fs::write(&path, b"not a font").expect("write invalid font fixture");
    let result = DwriteState::open_configured_primary(&path);
    let error = match result {
        Ok(_) => panic!("invalid font unexpectedly opened"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(message.contains(&path.display().to_string()), "{message}");
    let _ = fs::remove_file(path);
}

#[test]
fn should_open_system_primary_without_configured_family() {
    DwriteState::open_system_primary().expect("expected system primary face");
}

#[test]
fn should_return_positive_metrics_when_system_primary_opens() {
    let state = open_primary();
    let metrics = state.font_metrics(harbor_config::FONT_SIZE);
    assert!(metrics.cell_width > 0.0);
    assert!(metrics.line_height > 0.0);
    assert!(metrics.ascent > 0.0);
}

#[test]
fn should_return_stable_metrics_when_queried_repeatedly() {
    let state = open_primary();
    let first = state.font_metrics(harbor_config::FONT_SIZE);
    let second = state.font_metrics(harbor_config::FONT_SIZE);
    assert_eq!(first.cell_width, second.cell_width);
    assert_eq!(first.line_height, second.line_height);
    assert_eq!(first.ascent, second.ascent);
    assert_eq!(first.descent, second.descent);
    assert_eq!(first.line_gap, second.line_gap);
}

#[test]
fn should_return_stable_key_when_resolving_same_latin_char() {
    let state = open_primary();
    let first = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
    let second = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
    assert_eq!(first, second);
    assert_eq!(first.face_id, PRIMARY_FACE_ID);
    assert_eq!(state.map_call_count(), 0);
}

#[test]
fn should_return_non_empty_bitmap_when_rasterizing_latin() {
    let state = open_primary();
    let key = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
    let (bounds, bitmap) = state.rasterize(key);
    assert!(bounds.width > 0);
    assert!(bounds.height > 0);
    assert_eq!(bitmap.len(), bounds.width * bounds.height);
}

#[test]
fn should_return_partial_coverage_when_rasterizing_latin() {
    let state = open_primary();
    let key = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
    let (_, bitmap) = state.rasterize(key);
    let min = bitmap.iter().copied().min();
    let max = bitmap.iter().copied().max();
    let partial_count = bitmap
        .iter()
        .filter(|&&alpha| alpha > 0 && alpha < u8::MAX)
        .count();
    assert!(
        partial_count > 0,
        "grayscale antialiasing should produce partial pixel coverage; min={min:?}, max={max:?}, partial_count={partial_count}, len={}",
        bitmap.len()
    );
}

#[test]
fn should_return_zero_ink_with_positive_advance_when_rasterizing_space() {
    let state = open_primary();
    let key = expect_available(state.resolve(' ', harbor_config::FONT_SIZE, 0));
    let (bounds, bitmap) = state.rasterize(key);
    assert_eq!(bounds.width, 0);
    assert_eq!(bounds.height, 0);
    assert!(bitmap.is_empty());
    assert!(bounds.advance_width > 0.0);
}

#[test]
fn should_cache_unavailable_without_repeated_map_calls() {
    let state = open_primary();
    let ch = '\u{E000}';
    let first = state.resolve(ch, harbor_config::FONT_SIZE, 0);
    let calls_after_first = state.map_call_count();
    let second = state.resolve(ch, harbor_config::FONT_SIZE, 0);
    assert_eq!(first, second);
    assert_eq!(state.map_call_count(), calls_after_first);
    if matches!(first, GlyphResolution::Unavailable) {
        assert_eq!(calls_after_first, 1);
    }
}

#[test]
fn should_map_cjk_fallback_once_and_rasterize() {
    let state = open_primary();
    let before_faces = state.face_count();
    let metrics_before = state.font_metrics(harbor_config::FONT_SIZE);
    let resolution = state.resolve('中', harbor_config::FONT_SIZE, 0);
    let GlyphResolution::Available(key) = resolution else {
        // Some primaries already cover CJK; still exercise resolve/cache.
        assert_eq!(state.map_call_count(), 0);
        return;
    };
    if key.face_id != PRIMARY_FACE_ID {
        assert_eq!(state.map_call_count(), 1);
        assert!(state.face_count() >= before_faces);
        let again = expect_available(state.resolve('中', harbor_config::FONT_SIZE, 0));
        assert_eq!(again, key);
        assert_eq!(state.map_call_count(), 1);
        let (bounds, bitmap) = state.rasterize(key);
        assert!(bounds.width > 0);
        assert!(bounds.height > 0);
        assert_eq!(bitmap.len(), bounds.width * bounds.height);
    }
    let metrics_after = state.font_metrics(harbor_config::FONT_SIZE);
    assert_eq!(metrics_before.cell_width, metrics_after.cell_width);
    assert_eq!(metrics_before.line_height, metrics_after.line_height);
    assert_eq!(metrics_before.ascent, metrics_after.ascent);
}

#[test]
fn should_not_emit_first_fallback_when_resolving_latin_on_primary() {
    // Arrange
    let sink = Rc::new(RecordingFontLifecycleSink::default());
    let lifecycle: Rc<dyn FontLifecycleSink> = sink.clone();
    let state = DwriteState::open_system_primary_with_sink(lifecycle).expect("open system primary");

    // Act
    let _ = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));

    // Assert
    assert!(
        sink.events().is_empty(),
        "primary Latin resolve must not emit fallback"
    );
}

#[test]
fn should_emit_first_fallback_once_when_mapping_non_primary_face() {
    // Arrange
    let sink = Rc::new(RecordingFontLifecycleSink::default());
    let lifecycle: Rc<dyn FontLifecycleSink> = sink.clone();
    let state = DwriteState::open_system_primary_with_sink(lifecycle).expect("open system primary");

    // Act
    let resolution = state.resolve('中', harbor_config::FONT_SIZE, 0);
    let GlyphResolution::Available(key) = resolution else {
        // Some primaries already cover CJK; there is no fallback event to assert.
        return;
    };
    if key.face_id == PRIMARY_FACE_ID {
        return;
    }
    let _ = expect_available(state.resolve('国', harbor_config::FONT_SIZE, 0));

    // Assert
    assert_eq!(
        sink.events(),
        vec![FontLifecycleEvent::FirstFallback {
            scalar: '中',
            face_id: key.face_id,
        }],
        "first_fallback must emit once with the first mapped face"
    );
}

#[test]
fn should_map_configured_primary_cjk_via_system_fallback() {
    let Some(path) = configured_font_path() else {
        return;
    };
    let state = DwriteState::open_configured_primary(&path).expect("configured font opens");
    assert!(!state.primary_family_name().is_empty());
    let metrics_before = state.font_metrics(harbor_config::FONT_SIZE);
    let resolution = state.resolve('中', harbor_config::FONT_SIZE, 0);
    let GlyphResolution::Available(key) = resolution else {
        return;
    };
    if key.face_id != PRIMARY_FACE_ID {
        assert_eq!(state.map_call_count(), 1);
        let (bounds, _) = state.rasterize(key);
        assert!(bounds.width > 0);
        assert!(bounds.height > 0);
    }
    let metrics_after = state.font_metrics(harbor_config::FONT_SIZE);
    assert_eq!(metrics_before.cell_width, metrics_after.cell_width);
    assert_eq!(metrics_before.line_height, metrics_after.line_height);
}

#[test]
fn should_expose_supplementary_scalar_in_analysis_source() {
    let ch = '\u{1F600}';
    let locale = windows_user_locale().expect("locale");
    let source = FallbackTextSource::new(ch, &locale);
    assert_eq!(source.utf16_len(), 2);
    let object = source.into_object();
    let analysis: IDWriteTextAnalysisSource = object.to_interface();
    let mut ptr: *mut u16 = std::ptr::null_mut();
    let mut len = 0u32;
    unsafe {
        analysis
            .GetTextAtPosition(0, &mut ptr, &mut len)
            .expect("GetTextAtPosition");
    }
    assert!(!ptr.is_null());
    assert_eq!(len, 2);
}

#[test]
fn should_expose_bmp_ranges_locale_and_ltr_in_analysis_source() {
    // Arrange
    let ch = '中';
    let locale = windows_user_locale().expect("locale");
    let source = FallbackTextSource::new(ch, &locale);
    assert_eq!(source.utf16_len(), 1);
    let object = source.into_object();
    let analysis: IDWriteTextAnalysisSource = object.to_interface();

    // Act / Assert — text at start
    let mut ptr: *mut u16 = std::ptr::null_mut();
    let mut len = 0u32;
    unsafe {
        analysis
            .GetTextAtPosition(0, &mut ptr, &mut len)
            .expect("GetTextAtPosition(0)");
    }
    assert!(!ptr.is_null());
    assert_eq!(len, 1);

    // Act / Assert — out of range yields null
    unsafe {
        analysis
            .GetTextAtPosition(1, &mut ptr, &mut len)
            .expect("GetTextAtPosition(1)");
    }
    assert!(ptr.is_null());
    assert_eq!(len, 0);

    // Act / Assert — text before position
    unsafe {
        analysis
            .GetTextBeforePosition(1, &mut ptr, &mut len)
            .expect("GetTextBeforePosition(1)");
    }
    assert!(!ptr.is_null());
    assert_eq!(len, 1);
    unsafe {
        analysis
            .GetTextBeforePosition(0, &mut ptr, &mut len)
            .expect("GetTextBeforePosition(0)");
    }
    assert!(ptr.is_null());
    assert_eq!(len, 0);

    // Act / Assert — locale coverage + LTR + no number substitution
    let mut locale_ptr: *mut u16 = std::ptr::null_mut();
    unsafe {
        analysis
            .GetLocaleName(0, &mut len, &mut locale_ptr)
            .expect("GetLocaleName");
    }
    assert!(!locale_ptr.is_null());
    assert_eq!(len, 1);
    assert_eq!(
        unsafe { analysis.GetParagraphReadingDirection() },
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    );
    let mut substitution: Option<IDWriteNumberSubstitution> = None;
    unsafe {
        analysis
            .GetNumberSubstitution(0, &mut len, &mut substitution)
            .expect("GetNumberSubstitution");
    }
    assert_eq!(len, 0);
    assert!(substitution.is_none());
}

#[test]
fn should_return_positive_metrics_when_size_differs_from_default() {
    let state = open_primary();
    let other_size = harbor_config::FONT_SIZE + 4.0;
    let metrics = state.font_metrics(other_size);
    assert!(metrics.cell_width > 0.0);
    assert!(metrics.line_height > 0.0);
    assert!(metrics.ascent > 0.0);
}

#[test]
fn should_embed_size_and_style_in_key_when_resolving() {
    let state = open_primary();
    let size = harbor_config::FONT_SIZE;
    let style = 1u8;
    let key = expect_available(state.resolve('A', size, style));
    assert_eq!(key.size.bits(), size.to_bits());
    assert_eq!(key.style.get(), style);
    assert_eq!(key.face_id, PRIMARY_FACE_ID);
}

#[test]
fn should_return_different_keys_when_resolving_different_chars() {
    let state = open_primary();
    let key_a = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
    let key_b = expect_available(state.resolve('B', harbor_config::FONT_SIZE, 0));
    assert_ne!(key_a, key_b);
    assert_ne!(key_a.glyph_id.get(), key_b.glyph_id.get());
}

#[test]
fn should_return_empty_bitmap_when_rasterizing_out_of_range_glyph() {
    let state = open_primary();
    let key = GlyphKey {
        face_id: PRIMARY_FACE_ID,
        glyph_id: GlyphId::new(u32::from(u16::MAX) + 1),
        size: FontSize::from_bits(harbor_config::FONT_SIZE.to_bits()),
        style: FontStyle::new(0),
    };
    let (bounds, bitmap) = state.rasterize(key);
    assert_eq!(bounds.width, 0);
    assert_eq!(bounds.height, 0);
    assert!(bitmap.is_empty());
    assert_eq!(bounds.advance_width, 0.0);
}

#[test]
fn should_keep_distinct_face_ids_for_equal_glyph_indices() {
    let state = open_primary();
    let latin = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
    let cjk = state.resolve('中', harbor_config::FONT_SIZE, 0);
    let GlyphResolution::Available(cjk_key) = cjk else {
        return;
    };
    if cjk_key.face_id == latin.face_id {
        return;
    }
    // Force equal glyph_index collision across faces in the atlas key space.
    let colliding = GlyphKey {
        glyph_id: latin.glyph_id,
        ..cjk_key
    };
    assert_ne!(latin, colliding);
}
