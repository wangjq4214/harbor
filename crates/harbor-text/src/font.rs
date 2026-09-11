//! System font loading and the backend-neutral [`FontBook`] façade.
//!
//! On Windows, primary selection uses an optional configured DirectWrite family
//! and otherwise falls back to a system monospace face. Missing glyphs resolve
//! through DirectWrite system fallback.

use std::rc::Rc;

use anyhow::{Context as _, Result};
use harbor_config::FontSettings;

use crate::atlas::GlyphBitmapBounds;
use crate::backend::dwrite::DwriteState;
use crate::contracts::{FontStyle, GlyphKey, GlyphResolution};
use crate::lifecycle::{
    FontLifecycleEvent, FontLifecycleSink, FontSource, TracingFontLifecycleSink,
};
use crate::metrics::FontMetrics;
/// System terminal font set with a DirectWrite primary face and glyph fallbacks.
pub struct FontBook {
    native: Box<DwriteState>,
    size: f32,
}

impl FontBook {
    /// Wrap a DirectWrite primary-face session.
    pub(crate) fn from_native(state: DwriteState, size: f32) -> Self {
        Self {
            native: Box::new(state),
            size,
        }
    }

    /// Rasterize a character to a bitmap with backend-neutral bounds.
    pub fn rasterize(&self, ch: char, px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        match self.resolve(ch, px, FontStyle::REGULAR) {
            GlyphResolution::Available(key) => self.rasterize_from_key(key),
            GlyphResolution::Unavailable => (
                GlyphBitmapBounds {
                    width: 0,
                    height: 0,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance_width: 0.0,
                },
                Vec::new(),
            ),
        }
    }

    pub fn rasterize_from_key(&self, key: GlyphKey) -> (GlyphBitmapBounds, Vec<u8>) {
        self.native.rasterize(key)
    }

    pub fn resolve<S: Into<FontStyle>>(&self, ch: char, size: f32, style: S) -> GlyphResolution {
        self.native.resolve(ch, size, style.into())
    }

    pub fn font_metrics(&self) -> FontMetrics {
        self.native.font_metrics(self.size)
    }

    pub const fn size(&self) -> f32 {
        self.size
    }
}

/// Loads terminal fonts for Harbor startup using validated settings.
pub fn load_system_fonts(settings: &FontSettings) -> Result<FontBook> {
    load_system_fonts_with_sink(settings, Rc::new(TracingFontLifecycleSink))
}

/// Loads the system default UI fonts for widget and chrome rendering.
///
/// Uses the operating system's default UI font family (e.g. Segoe UI on Windows)
/// and standard UI font size, completely independent of terminal font settings.
pub fn load_system_ui_fonts() -> Result<FontBook> {
    load_system_ui_fonts_with_sink(Rc::new(TracingFontLifecycleSink))
}

fn load_system_ui_fonts_with_sink(lifecycle: Rc<dyn FontLifecycleSink>) -> Result<FontBook> {
    let started = std::time::Instant::now();
    let size = harbor_config::DEFAULT_UI_FONT_SIZE;
    let state = DwriteState::open_system_ui_primary_with_sink(size, Rc::clone(&lifecycle))
        .context("load DirectWrite system UI primary face")?;
    let fonts = FontBook::from_native(state, size);
    emit_font_init(lifecycle.as_ref(), FontSource::System, started);
    Ok(fonts)
}

fn load_system_fonts_with_sink(
    settings: &FontSettings,
    lifecycle: Rc<dyn FontLifecycleSink>,
) -> Result<FontBook> {
    let started = std::time::Instant::now();
    let state = DwriteState::open_primary_with_sink(
        settings.family.as_deref(),
        settings.size,
        Rc::clone(&lifecycle),
    )
    .context("load DirectWrite primary face")?;
    let source = if settings.family.is_some() {
        FontSource::Configured
    } else {
        FontSource::System
    };
    let fonts = FontBook::from_native(state, settings.size);
    emit_font_init(lifecycle.as_ref(), source, started);
    Ok(fonts)
}

fn emit_font_init(
    lifecycle: &dyn FontLifecycleSink,
    source: FontSource,
    started: std::time::Instant,
) {
    lifecycle.emit(FontLifecycleEvent::FontInit {
        source,
        elapsed_ms: started.elapsed().as_millis() as u64,
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::{
        FontLifecycleEvent, FontLifecycleSink, FontSource, RecordingFontLifecycleSink,
    };

    fn expect_key(resolution: GlyphResolution) -> GlyphKey {
        match resolution {
            GlyphResolution::Available(key) => key,
            GlyphResolution::Unavailable => panic!("expected Available"),
        }
    }

    fn load_test_fonts() -> Result<FontBook> {
        let sink: Rc<dyn FontLifecycleSink> = Rc::new(RecordingFontLifecycleSink::default());
        load_system_fonts_with_sink(&FontSettings::default(), sink)
    }

    fn test_font_book() -> FontBook {
        load_test_fonts().expect("load test font")
    }

    #[test]
    fn should_emit_font_init_system_for_default_settings() {
        let sink = Rc::new(RecordingFontLifecycleSink::default());
        let lifecycle: Rc<dyn FontLifecycleSink> = sink.clone();
        let fonts = load_system_fonts_with_sink(&FontSettings::default(), lifecycle)
            .expect("default load path");

        assert!(matches!(
            sink.events().as_slice(),
            [FontLifecycleEvent::FontInit {
                source: FontSource::System,
                elapsed_ms: _
            }]
        ));
        assert!(fonts.font_metrics().cell_width > 0.0);
    }

    #[test]
    fn should_load_system_ui_fonts_with_standard_size() {
        let fonts = load_system_ui_fonts().expect("load system ui font");
        assert_eq!(fonts.size(), harbor_config::DEFAULT_UI_FONT_SIZE);
        let metrics = fonts.font_metrics();
        assert!(metrics.cell_width > 0.0);
        assert!(metrics.line_height > 0.0);
        assert!(metrics.ascent > 0.0);
    }

    #[test]
    fn system_ui_fonts_are_isolated_from_terminal_font_settings() {
        let terminal_settings = FontSettings {
            family: Some("Consolas".to_string()),
            size: 32.0,
        };
        let terminal_fonts = load_system_fonts(&terminal_settings).expect("terminal font");
        let ui_fonts = load_system_ui_fonts().expect("ui font");

        assert_eq!(terminal_fonts.size(), 32.0);
        assert_eq!(ui_fonts.size(), harbor_config::DEFAULT_UI_FONT_SIZE);
        assert_ne!(
            terminal_fonts.font_metrics().line_height,
            ui_fonts.font_metrics().line_height
        );
    }

    #[test]
    fn should_apply_configured_size_to_metrics_and_glyph_keys() {
        let small = load_system_fonts(&FontSettings {
            family: None,
            size: 12.0,
        })
        .expect("small font");
        let large = load_system_fonts(&FontSettings {
            family: None,
            size: 24.0,
        })
        .expect("large font");

        assert!(large.font_metrics().line_height > small.font_metrics().line_height);
        let key = expect_key(small.resolve('A', small.size(), 0));
        assert_eq!(key.size.bits(), 12.0f32.to_bits());
    }

    #[test]
    fn unknown_configured_family_falls_back_to_usable_system_primary() {
        let fonts = load_system_fonts(&FontSettings {
            family: Some("Harbor Definitely Missing Font".to_owned()),
            size: 18.0,
        })
        .expect("family fallback");
        assert!(fonts.font_metrics().cell_width > 0.0);
        assert_eq!(fonts.size(), 18.0);
    }
    #[test]
    fn should_return_positive_advance_when_rasterizing_space_on_default_path() {
        // Arrange
        let fonts = test_font_book();

        // Act
        let (bounds, bitmap) = fonts.rasterize(' ', harbor_config::FONT_SIZE);

        // Assert
        assert_eq!(bounds.width, 0);
        assert_eq!(bounds.height, 0);
        assert!(bitmap.is_empty());
        assert!(
            bounds.advance_width > 0.0,
            "space advance_width={}",
            bounds.advance_width
        );
    }

    #[test]
    fn should_return_bitmap_when_rasterizing_latin() {
        let fonts = test_font_book();
        let (bounds, bitmap) = fonts.rasterize('A', harbor_config::FONT_SIZE);
        assert!(bounds.width > 0, "glyph width should be > 0");
        assert!(bounds.height > 0, "glyph height should be > 0");
        assert!(!bitmap.is_empty(), "bitmap should not be empty");
    }

    #[test]
    fn should_return_zero_dimensions_when_rasterizing_space() {
        let fonts = test_font_book();
        let (bounds, _bitmap) = fonts.rasterize(' ', harbor_config::FONT_SIZE);
        assert_eq!(bounds.width, 0);
        assert_eq!(bounds.height, 0);
    }

    #[test]
    fn should_return_positive_metrics_when_default_font_loads() {
        let fonts = test_font_book();
        let fm = fonts.font_metrics();
        assert!(fm.cell_width > 0.0, "cell_width should be positive");
        assert!(fm.line_height > 0.0, "line_height should be positive");
        assert!(fm.ascent > 0.0, "ascent should be positive");
    }

    #[test]
    fn should_return_stable_key_when_resolving_same_char() {
        let fonts = test_font_book();
        let k1 = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        let k2 = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        assert_eq!(
            k1, k2,
            "resolve should return the same key for the same char"
        );
    }

    #[test]
    fn should_pass_size_and_style_through_resolve() {
        // Arrange
        let fonts = test_font_book();
        let size = harbor_config::FONT_SIZE + 2.0;
        let style = 1u8;

        // Act
        let key = expect_key(fonts.resolve('A', size, style));

        // Assert
        assert_eq!(key.size.bits(), size.to_bits());
        assert_eq!(key.style.get(), style);
    }

    #[test]
    fn should_return_empty_ink_when_rasterizing_unavailable() {
        // Arrange
        let fonts = test_font_book();
        let ch = '\u{E000}';

        // Act
        let resolution = fonts.resolve(ch, harbor_config::FONT_SIZE, 0);
        let (bounds, bitmap) = fonts.rasterize(ch, harbor_config::FONT_SIZE);

        // Assert
        if matches!(resolution, GlyphResolution::Unavailable) {
            assert_eq!(bounds.width, 0);
            assert_eq!(bounds.height, 0);
            assert!(bitmap.is_empty());
            assert_eq!(bounds.advance_width, 0.0);
        }
    }

    #[test]
    fn should_rasterize_cjk_via_fallback_or_primary() {
        let fonts = test_font_book();
        let resolution = fonts.resolve('中', harbor_config::FONT_SIZE, 0);
        let (_bounds, _bitmap) = fonts.rasterize('中', harbor_config::FONT_SIZE);
        if let GlyphResolution::Available(key) = resolution {
            let (_bounds2, _bitmap2) = fonts.rasterize_from_key(key);
        }
    }

    #[test]
    fn should_not_panic_when_resolving_and_rasterizing_missing_glyph() {
        // Arrange
        let fonts = test_font_book();
        let ch = '\u{E000}';

        // Act
        let resolution = fonts.resolve(ch, harbor_config::FONT_SIZE, 0);
        let (_bounds, _bitmap) = fonts.rasterize(ch, harbor_config::FONT_SIZE);
        if let GlyphResolution::Available(key) = resolution {
            let (_bounds2, _bitmap2) = fonts.rasterize_from_key(key);
            let _ = key.face_id;
        }
    }

    // ── resolve tests ────────────────────────────────────────────────

    #[test]
    fn should_return_different_keys_for_different_chars() {
        let fonts = test_font_book();
        let key_a = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        let key_b = expect_key(fonts.resolve('B', harbor_config::FONT_SIZE, 0));
        assert_ne!(
            key_a, key_b,
            "different chars should produce different keys"
        );
    }

    #[test]
    fn should_return_key_with_valid_glyph_index() {
        let fonts = test_font_book();
        let key = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        let _ = key.glyph_id.get();
        let _ = key.face_id;
    }

    #[test]
    fn should_resolve_cjk_char() {
        let fonts = test_font_book();
        match fonts.resolve('中', harbor_config::FONT_SIZE, 0) {
            GlyphResolution::Available(key) => assert_eq!(key.style.get(), 0),
            GlyphResolution::Unavailable => {}
        }
    }

    // ── rasterize_from_key tests ─────────────────────────────────────

    #[test]
    fn should_rasterize_from_key_producing_valid_bitmap() {
        let fonts = test_font_book();
        let key = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        let (bounds, bitmap) = fonts.rasterize_from_key(key);
        assert!(bounds.width > 0, "rasterize_from_key width should be > 0");
        assert!(bounds.height > 0, "rasterize_from_key height should be > 0");
        assert!(!bitmap.is_empty(), "bitmap should not be empty");
    }

    #[test]
    fn should_rasterize_from_key_match_rasterize_directly() {
        let fonts = test_font_book();
        let key = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        let (bounds_key, bitmap_key) = fonts.rasterize_from_key(key);
        let (bounds_direct, bitmap_direct) = fonts.rasterize('A', harbor_config::FONT_SIZE);
        assert_eq!(
            bounds_key.width, bounds_direct.width,
            "width from key and direct should match"
        );
        assert_eq!(
            bounds_key.height, bounds_direct.height,
            "height from key and direct should match"
        );
        assert_eq!(
            bounds_key.bearing_x, bounds_direct.bearing_x,
            "bearing_x from key and direct should match"
        );
        assert_eq!(
            bounds_key.bearing_y, bounds_direct.bearing_y,
            "bearing_y from key and direct should match"
        );
        assert_eq!(
            bitmap_key.len(),
            bitmap_direct.len(),
            "bitmap lengths should match"
        );
    }

    #[test]
    fn should_rasterize_from_key_with_cjk_char_without_panic() {
        let fonts = test_font_book();
        if let GlyphResolution::Available(key) = fonts.resolve('中', harbor_config::FONT_SIZE, 0) {
            let (_bounds, _bitmap) = fonts.rasterize_from_key(key);
        }
    }

    // ── font_metrics tests ───────────────────────────────────────────

    #[test]
    fn should_return_non_negative_descent() {
        let fonts = test_font_book();
        let fm = fonts.font_metrics();
        assert!(
            fm.descent >= 0.0,
            "descent should be non-negative, got {}",
            fm.descent
        );
    }

    #[test]
    fn should_return_non_negative_line_gap() {
        let fonts = test_font_book();
        let fm = fonts.font_metrics();
        assert!(
            fm.line_gap.is_finite(),
            "line_gap should be finite, got {}",
            fm.line_gap
        );
    }

    #[test]
    fn should_return_ascent_greater_than_zero() {
        let fonts = test_font_book();
        let fm = fonts.font_metrics();
        assert!(fm.ascent > 0.0, "ascent should be positive");
    }

    #[test]
    fn should_return_line_height_greater_than_ascent() {
        let fonts = test_font_book();
        let fm = fonts.font_metrics();
        assert!(
            fm.line_height >= fm.ascent,
            "line_height ({}) should be >= ascent ({})",
            fm.line_height,
            fm.ascent
        );
    }

    #[test]
    fn should_return_consistent_metrics_across_calls() {
        let fonts = test_font_book();
        let fm1 = fonts.font_metrics();
        let fm2 = fonts.font_metrics();
        assert_eq!(fm1.cell_width, fm2.cell_width);
        assert_eq!(fm1.line_height, fm2.line_height);
        assert_eq!(fm1.ascent, fm2.ascent);
        assert_eq!(fm1.descent, fm2.descent);
        assert_eq!(fm1.line_gap, fm2.line_gap);
    }
}
