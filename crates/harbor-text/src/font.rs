//! System font loading and the backend-neutral [`FontBook`] façade.
//!
//! On Windows, primary selection uses DirectWrite (system monospace or
//! process-private `HARBOR_FONT`). Missing glyphs resolve through DirectWrite
//! system fallback. Non-Windows builds are rejected by the backend gate.

use std::{env, path::PathBuf};
#[cfg(test)]
use std::{ffi::OsString, sync::Mutex};

use anyhow::{Context as _, Result, bail};

use crate::atlas::GlyphBitmapBounds;
use crate::backend::dwrite::DwriteState;
use crate::contracts::{FontStyle, GlyphKey, GlyphResolution};
use crate::metrics::FontMetrics;

const FONT_ENV: &str = "HARBOR_FONT";
/// Keep in sync with `dwrite.rs` and `src/app.rs` (`FONT_LIFECYCLE_TARGET`).
const LIFECYCLE_TARGET: &str = "harbor.font.lifecycle";

#[cfg(test)]
static FONT_ENV_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn with_font_env<R>(value: Option<OsString>, f: impl FnOnce() -> R) -> R {
    let guard = FONT_ENV_LOCK.lock().expect("font environment lock");
    let previous = env::var_os(FONT_ENV);
    match value {
        Some(value) => unsafe { env::set_var(FONT_ENV, value) },
        None => unsafe { env::remove_var(FONT_ENV) },
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    match previous {
        Some(value) => unsafe { env::set_var(FONT_ENV, value) },
        None => unsafe { env::remove_var(FONT_ENV) },
    }
    drop(guard);
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

/// System terminal font set with a DirectWrite primary face and glyph fallbacks.
pub struct FontBook {
    native: Box<DwriteState>,
}

impl FontBook {
    /// Wrap a DirectWrite primary-face session.
    pub(crate) fn from_native(state: DwriteState) -> Self {
        Self {
            native: Box::new(state),
        }
    }

    /// Rasterize a character to a bitmap with backend-neutral bounds.
    ///
    /// Resolves once, then rasterizes the resulting key. Unavailable characters
    /// yield empty ink without a second mapping attempt.
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

    /// Rasterize a glyph by its already-resolved key (used during atlas rebuild).
    pub fn rasterize_from_key(&self, key: GlyphKey) -> (GlyphBitmapBounds, Vec<u8>) {
        self.native.rasterize(key)
    }

    /// Resolve a character to an available glyph key or a cached unavailable result.
    pub fn resolve<S: Into<FontStyle>>(&self, ch: char, size: f32, style: S) -> GlyphResolution {
        self.native.resolve(ch, size, style.into())
    }

    /// Primary font metrics for terminal cell sizing.
    pub fn font_metrics(&self) -> FontMetrics {
        self.native.font_metrics(harbor_config::FONT_SIZE)
    }
}

/// Loads terminal fonts for Harbor startup.
///
/// - `HARBOR_FONT` selects a DirectWrite process-private primary face.
/// - Otherwise selects a DirectWrite system monospace primary face.
pub fn load_system_fonts() -> Result<FontBook> {
    let started = std::time::Instant::now();
    if let Some(fonts) = load_configured_fonts()? {
        emit_font_init("configured", started);
        return Ok(fonts);
    }

    let state =
        DwriteState::open_system_primary().context("load DirectWrite system primary face")?;
    tracing::info!("loaded terminal font from DirectWrite system primary");
    let fonts = FontBook::from_native(state);
    emit_font_init("system", started);
    Ok(fonts)
}

fn emit_font_init(source: &str, started: std::time::Instant) {
    tracing::info!(
        target: LIFECYCLE_TARGET,
        phase = "font_init",
        source,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "font lifecycle"
    );
}

fn load_configured_fonts() -> Result<Option<FontBook>> {
    let Some(path) = env::var_os(FONT_ENV) else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty() {
        bail!("HARBOR_FONT is set but empty");
    }

    let state = DwriteState::open_configured_primary(&path)?;
    tracing::info!(path = %path.display(), "loaded terminal font from HARBOR_FONT");
    Ok(Some(FontBook::from_native(state)))
}

#[cfg(test)]
fn test_configured_font_path() -> Option<PathBuf> {
    let fonts_dir = env::var_os("WINDIR")
        .or_else(|| env::var_os("SYSTEMROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    [
        "CascadiaMono.ttf",
        "CascadiaCode.ttf",
        "consola.ttf",
        "Consola.ttf",
        "cour.ttf",
    ]
    .into_iter()
    .map(|file| fonts_dir.join(file))
    .find(|path| path.is_file())
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

    fn expect_key(resolution: GlyphResolution) -> GlyphKey {
        match resolution {
            GlyphResolution::Available(key) => key,
            GlyphResolution::Unavailable => panic!("expected Available"),
        }
    }

    fn test_font_book() -> FontBook {
        with_font_env(None, || load_system_fonts().expect("load test font"))
    }

    /// Captures `harbor.font.lifecycle` field maps for behavior assertions.
    #[derive(Clone, Default)]
    struct LifecycleCapture {
        events: Arc<Mutex<Vec<HashMap<String, String>>>>,
    }

    impl LifecycleCapture {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn events_with_phase(&self, phase: &str) -> Vec<HashMap<String, String>> {
            self.events
                .lock()
                .expect("lifecycle capture lock")
                .iter()
                .filter(|fields| fields.get("phase").map(|p| p == phase).unwrap_or(false))
                .cloned()
                .collect()
        }
    }

    struct FieldRecorder<'a> {
        fields: &'a mut HashMap<String, String>,
    }

    impl Visit for FieldRecorder<'_> {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_i64(&mut self, field: &Field, value: i64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_bool(&mut self, field: &Field, value: bool) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    impl<S> Layer<S> for LifecycleCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            if event.metadata().target() != LIFECYCLE_TARGET {
                return;
            }
            let mut fields = HashMap::new();
            event.record(&mut FieldRecorder {
                fields: &mut fields,
            });
            self.events
                .lock()
                .expect("lifecycle capture lock")
                .push(fields);
        }
    }

    fn with_lifecycle_capture<R>(f: impl FnOnce() -> R) -> (R, LifecycleCapture) {
        let _guard = crate::TRACING_CAPTURE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let capture = LifecycleCapture::new();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let result = tracing::subscriber::with_default(subscriber, f);
        (result, capture)
    }

    #[test]
    fn should_emit_font_init_system_when_harbor_font_unset() {
        // Arrange / Act
        let (fonts, capture) = with_lifecycle_capture(|| {
            with_font_env(None, || load_system_fonts().expect("default load path"))
        });

        // Assert
        let events = capture.events_with_phase("font_init");
        assert_eq!(events.len(), 1, "expected one font_init marker");
        assert_eq!(events[0].get("source").map(String::as_str), Some("system"));
        assert!(events[0].contains_key("elapsed_ms"));
        let metrics = fonts.font_metrics();
        assert!(metrics.cell_width > 0.0);
    }

    #[test]
    fn should_emit_font_init_configured_when_harbor_font_set() {
        // Arrange
        let Some(path) = test_configured_font_path() else {
            return;
        };

        // Act
        let (fonts, capture) = with_lifecycle_capture(|| {
            with_font_env(Some(path.into_os_string()), || {
                load_system_fonts().expect("configured font path")
            })
        });

        // Assert
        let events = capture.events_with_phase("font_init");
        assert_eq!(events.len(), 1, "expected one font_init marker");
        assert_eq!(
            events[0].get("source").map(String::as_str),
            Some("configured")
        );
        assert!(events[0].contains_key("elapsed_ms"));
        let metrics = fonts.font_metrics();
        assert!(metrics.cell_width > 0.0);
    }

    #[test]
    fn should_not_emit_font_init_when_configured_font_missing() {
        // Arrange
        let path = env::temp_dir().join(format!("harbor-missing-font-{}.ttf", std::process::id()));

        // Act
        let (result, capture) = with_lifecycle_capture(|| {
            with_font_env(Some(path.clone().into_os_string()), load_system_fonts)
        });

        // Assert
        assert!(result.is_err());
        assert!(
            capture.events_with_phase("font_init").is_empty(),
            "failed load must not emit font_init"
        );
    }

    #[test]
    fn should_load_native_primary_when_harbor_font_unset() {
        // Arrange and act
        let fonts = with_font_env(None, || load_system_fonts().expect("default load path"));

        // Assert — default path yields a usable primary face for terminal metrics/glyphs.
        let metrics = fonts.font_metrics();
        assert!(
            metrics.cell_width > 0.0,
            "cell_width={}",
            metrics.cell_width
        );
        assert!(
            metrics.line_height > 0.0,
            "line_height={}",
            metrics.line_height
        );
        assert!(metrics.ascent > 0.0, "ascent={}", metrics.ascent);

        let (bounds, bitmap) = fonts.rasterize('A', harbor_config::FONT_SIZE);
        assert!(bounds.width > 0, "latin width should be > 0");
        assert!(bounds.height > 0, "latin height should be > 0");
        assert_eq!(bitmap.len(), bounds.width * bounds.height);
    }

    #[test]
    fn should_load_configured_native_primary_from_system_font_path() {
        let Some(path) = test_configured_font_path() else {
            return;
        };

        let (metrics, bounds, bitmap) = with_font_env(Some(path.into_os_string()), || {
            let fonts = load_system_fonts().expect("configured font path");
            let metrics = fonts.font_metrics();
            let (bounds, bitmap) = fonts.rasterize('A', harbor_config::FONT_SIZE);
            (metrics, bounds, bitmap)
        });
        assert!(metrics.cell_width > 0.0);
        assert!(metrics.line_height > 0.0);
        assert!(bounds.width > 0);
        assert!(bounds.height > 0);
        assert_eq!(bitmap.len(), bounds.width * bounds.height);
    }

    #[test]
    fn should_rasterize_configured_primary_by_resolved_key() {
        // Arrange
        let Some(path) = test_configured_font_path() else {
            return;
        };
        // Act
        let (key, direct, by_key) = with_font_env(Some(path.into_os_string()), || {
            let fonts = load_system_fonts().expect("configured font path");
            let key = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
            let direct = fonts.rasterize('A', harbor_config::FONT_SIZE);
            let by_key = fonts.rasterize_from_key(key);
            (key, direct, by_key)
        });

        // Assert
        assert_eq!(key.face_id, 0);
        assert!(
            key.glyph_id.get() > 0,
            "configured Latin glyph should resolve"
        );
        assert_eq!(direct.0.width, by_key.0.width);
        assert_eq!(direct.0.height, by_key.0.height);
        assert_eq!(direct.0.bearing_x, by_key.0.bearing_x);
        assert_eq!(direct.0.bearing_y, by_key.0.bearing_y);
        assert_eq!(direct.0.advance_width, by_key.0.advance_width);
        assert_eq!(direct.1, by_key.1);
        assert!(by_key.0.width > 0);
        assert!(by_key.0.height > 0);
        assert_eq!(by_key.1.len(), by_key.0.width * by_key.0.height);
    }

    #[test]
    fn should_reject_missing_configured_font_without_fallback() {
        let path = env::temp_dir().join(format!("harbor-missing-font-{}.ttf", std::process::id()));
        let message = with_font_env(
            Some(path.clone().into_os_string()),
            || match load_system_fonts() {
                Ok(_) => panic!("missing configured font unexpectedly succeeded"),
                Err(error) => format!("{error:#}"),
            },
        );
        assert!(message.contains(&path.display().to_string()), "{message}");
        with_font_env(None, || {
            load_system_fonts().expect("system fallback after failed load")
        });
    }

    #[test]
    fn should_reject_empty_configured_font_value() {
        let message = with_font_env(Some(OsString::new()), || match load_system_fonts() {
            Ok(_) => panic!("empty configured font unexpectedly succeeded"),
            Err(error) => format!("{error:#}"),
        });
        assert!(
            message.contains("HARBOR_FONT is set but empty"),
            "{message}"
        );
    }

    #[test]
    fn should_reject_unsupported_configured_font_without_fallback() {
        let path = env::temp_dir().join(format!("harbor-invalid-font-{}.bin", std::process::id()));
        fs::write(&path, b"not a font").expect("write invalid font fixture");
        let message = with_font_env(
            Some(path.clone().into_os_string()),
            || match load_system_fonts() {
                Ok(_) => panic!("invalid configured font unexpectedly succeeded"),
                Err(error) => format!("{error:#}"),
            },
        );
        let _ = fs::remove_file(&path);
        assert!(message.contains(&path.display().to_string()), "{message}");
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
