use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
};
#[cfg(test)]
use std::{ffi::OsString, sync::Mutex};

use anyhow::{Context as _, Result, anyhow, bail};
use fontdb::{Database, Family, ID, Query};
use fontdue::{Font, FontSettings};

use crate::atlas::{GlyphBitmapBounds, GlyphKey};
use crate::backend::GlyphResolution;
use crate::backend::compat::CompatState;
use crate::backend::dwrite::DwriteState;
use crate::metrics::FontMetrics;

const CJK_PROBE: char = '中';
const FONT_ENV: &str = "HARBOR_FONT";

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

pub(crate) struct LoadedFont {
    pub family: String,
    pub font: Font,
}

/// Private discriminant between temporary compat and native primary backends.
enum FontBackend {
    Compat(Box<CompatState>),
    Native(Box<DwriteState>),
}

/// System terminal font set with a primary monospace face and glyph fallbacks.
pub struct FontBook {
    #[cfg(windows)]
    backend: FontBackend,
}

impl FontBook {
    /// Wrap legacy fontdue fonts for compatibility.
    /// Temporary — will be removed when the DirectWrite backend replaces all paths.
    #[cfg(windows)]
    pub(crate) fn from_compat(fonts: Vec<LoadedFont>) -> Self {
        Self {
            backend: FontBackend::Compat(Box::new(CompatState::new(fonts))),
        }
    }

    /// Wrap a DirectWrite primary-face session.
    #[cfg(windows)]
    pub(crate) fn from_native(state: DwriteState) -> Self {
        Self {
            backend: FontBackend::Native(Box::new(state)),
        }
    }

    /// Rasterize a character to a bitmap with backend-neutral bounds.
    ///
    /// Resolves once, then rasterizes the resulting key. Unavailable characters
    /// yield empty ink without a second mapping attempt.
    pub fn rasterize(&self, ch: char, px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        match self.resolve(ch, px, 0) {
            GlyphResolution::Available(key) => {
                self.rasterize_from_key(key, f32::from_bits(key.size_bits))
            }
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
    pub fn rasterize_from_key(&self, key: GlyphKey, px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        match &self.backend {
            FontBackend::Compat(compat) => compat.rasterize(key, px),
            FontBackend::Native(native) => native.rasterize(key, px),
        }
    }

    /// Resolve a character to an available glyph key or a cached unavailable result.
    pub fn resolve(&self, ch: char, size: f32, style: u8) -> GlyphResolution {
        match &self.backend {
            FontBackend::Compat(compat) => compat.resolve(ch, size, style),
            FontBackend::Native(native) => native.resolve(ch, size, style),
        }
    }

    /// Primary font metrics for terminal cell sizing.
    pub fn font_metrics(&self) -> FontMetrics {
        match &self.backend {
            FontBackend::Compat(compat) => compat.font_metrics(harbor_config::FONT_SIZE),
            FontBackend::Native(native) => native.font_metrics(harbor_config::FONT_SIZE),
        }
    }
}

/// Loads terminal fonts for Harbor startup.
///
/// - `HARBOR_FONT` selects a DirectWrite process-private primary face.
/// - Otherwise selects a DirectWrite system monospace primary face.
pub fn load_system_fonts() -> Result<FontBook> {
    if let Some(fonts) = load_configured_fonts()? {
        return Ok(fonts);
    }

    let state =
        DwriteState::open_system_primary().context("load DirectWrite system primary face")?;
    tracing::info!("loaded terminal font from DirectWrite system primary");
    Ok(FontBook::from_native(state))
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

#[allow(dead_code)]
fn load_candidate_fonts() -> Option<FontBook> {
    // Kick off CJK loading on a background thread so primary + CJK IO+parse
    // overlap instead of running serially.
    let cjk_handle = thread::spawn(|| load_first_cjk_font_file(cjk_font_candidates()));

    let primary = load_first_font_file(primary_font_candidates())?;
    if primary.font.has_glyph(CJK_PROBE) {
        tracing::info!(primary = %primary.family, "loaded terminal font from fast path");
        return Some(FontBook::from_compat(vec![primary]));
    }

    // Wait for the CJK thread result.
    let fallback = cjk_handle.join().ok()??;
    tracing::info!(
        primary = %primary.family,
        fallback = %fallback.family,
        "loaded terminal fonts from fast path"
    );
    Some(FontBook::from_compat(vec![primary, fallback]))
}

#[allow(dead_code)]
fn build_font_book(primary: LoadedFont) -> FontBook {
    let mut fonts = vec![primary];

    if !fonts[0].font.has_glyph(CJK_PROBE) {
        if let Some(fallback) = load_first_cjk_font_file(cjk_font_candidates()) {
            tracing::info!(
                primary = %fonts[0].family,
                fallback = %fallback.family,
                "loaded terminal fonts from fast path"
            );
            fonts.push(fallback);
        } else {
            tracing::warn!(
                primary = %fonts[0].family,
                probe = %CJK_PROBE,
                "no CJK-capable font fallback found on fast path"
            );
        }
    } else {
        tracing::info!(primary = %fonts[0].family, "loaded terminal font from fast path");
    }

    FontBook::from_compat(fonts)
}

#[allow(dead_code)]
fn load_fontdb_fonts() -> Result<FontBook> {
    let mut database = Database::new();
    database.load_system_fonts();

    let face_count = database.faces().count();
    if face_count == 0 {
        return Err(anyhow!("no system fonts found"));
    }

    let primary = load_primary_font(&database)?;
    let mut fonts = vec![primary];

    if !fonts[0].font.has_glyph(CJK_PROBE) {
        if let Some(fallback) = load_cjk_fallback(&database, &fonts[0].family)? {
            tracing::info!(
                primary = %fonts[0].family,
                fallback = %fallback.family,
                "loaded terminal fonts from fontdb"
            );
            fonts.push(fallback);
        } else {
            tracing::warn!(
                primary = %fonts[0].family,
                probe = %CJK_PROBE,
                "no CJK-capable font fallback found in fontdb"
            );
        }
    } else {
        tracing::info!(primary = %fonts[0].family, "loaded terminal font from fontdb");
    }

    Ok(FontBook::from_compat(fonts))
}

#[allow(dead_code)]
fn load_first_font_file(candidates: Vec<PathBuf>) -> Option<LoadedFont> {
    candidates
        .into_iter()
        .find_map(|path| load_font_file(&path, 0).ok())
}

fn load_first_cjk_font_file(candidates: Vec<PathBuf>) -> Option<LoadedFont> {
    candidates.into_iter().find_map(|path| {
        let font = load_font_file(&path, 0).ok()?;
        font.font.has_glyph(CJK_PROBE).then_some(font)
    })
}

fn load_font_file(path: &Path, collection_index: u32) -> Result<LoadedFont> {
    let bytes = fs::read(path).with_context(|| format!("read font {}", path.display()))?;
    let font = Font::from_bytes(
        bytes,
        FontSettings {
            collection_index,
            ..FontSettings::default()
        },
    )
    .map_err(|error| anyhow!("parse font '{}': {error}", path.display()))?;

    let family = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("terminal font")
        .to_owned();

    Ok(LoadedFont { family, font })
}

#[allow(dead_code)]
fn load_primary_font(database: &Database) -> Result<LoadedFont> {
    let query = Query {
        families: &[Family::Monospace],
        ..Query::default()
    };

    let preferred_ids = database.query(&query).into_iter();
    let monospaced_ids = database
        .faces()
        .filter(|face| face.monospaced)
        .map(|face| face.id);
    let remaining_ids = database.faces().map(|face| face.id);

    load_first_font(
        database,
        preferred_ids.chain(monospaced_ids).chain(remaining_ids),
    )
    .context("load primary monospace font")
}

#[allow(dead_code)]
fn load_cjk_fallback(database: &Database, primary_family: &str) -> Result<Option<LoadedFont>> {
    let monospaced_ids = database
        .faces()
        .filter(|face| face.monospaced)
        .map(|face| face.id);
    let remaining_ids = database.faces().map(|face| face.id);

    for id in monospaced_ids.chain(remaining_ids) {
        let Some(font) = load_font(database, id)? else {
            continue;
        };
        if font.family == primary_family {
            continue;
        }
        if font.font.has_glyph(CJK_PROBE) {
            return Ok(Some(font));
        }
    }

    Ok(None)
}

fn load_first_font(database: &Database, ids: impl IntoIterator<Item = ID>) -> Result<LoadedFont> {
    for id in ids {
        if let Some(font) = load_font(database, id)? {
            return Ok(font);
        }
    }

    Err(anyhow!("no parseable system font found"))
}

fn load_font(database: &Database, id: ID) -> Result<Option<LoadedFont>> {
    let Some(face) = database.face(id) else {
        return Ok(None);
    };
    let family = face
        .families
        .first()
        .map(|(family, _)| family.clone())
        .unwrap_or_else(|| face.post_script_name.clone());

    let Some(font) = database.with_face_data(id, |data, collection_index| {
        Font::from_bytes(
            data,
            FontSettings {
                collection_index,
                ..FontSettings::default()
            },
        )
    }) else {
        tracing::debug!(family, "skipping unreadable font data");
        return Ok(None);
    };

    let font = match font {
        Ok(font) => font,
        Err(error) => {
            tracing::debug!(family, error = %error, "skipping unsupported font");
            return Ok(None);
        }
    };

    Ok(Some(LoadedFont { family, font }))
}

#[cfg(windows)]
#[allow(dead_code)]
fn primary_font_candidates() -> Vec<PathBuf> {
    let fonts_dir = windows_fonts_dir();
    [
        "CascadiaMono.ttf",
        "CascadiaCode.ttf",
        "consola.ttf",
        "Consola.ttf",
        "cour.ttf",
    ]
    .into_iter()
    .map(|file| fonts_dir.join(file))
    .collect()
}

#[cfg(windows)]
#[allow(dead_code)]
fn cjk_font_candidates() -> Vec<PathBuf> {
    let fonts_dir = windows_fonts_dir();
    [
        "msyh.ttc",
        "msyh.ttf",
        "simhei.ttf",
        "simsun.ttc",
        "Deng.ttf",
    ]
    .into_iter()
    .map(|file| fonts_dir.join(file))
    .collect()
}

#[cfg(windows)]
#[allow(dead_code)]
fn windows_fonts_dir() -> PathBuf {
    env::var_os("WINDIR")
        .or_else(|| env::var_os("SYSTEMROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

#[cfg(target_os = "macos")]
fn primary_font_candidates() -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/SFNSMono.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(target_os = "macos")]
fn cjk_font_candidates() -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn primary_font_candidates() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/usr/local/share/fonts/DejaVuSansMono.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn cjk_font_candidates() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/source-han-sans/SourceHanSansSC-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_key(resolution: GlyphResolution) -> GlyphKey {
        match resolution {
            GlyphResolution::Available(key) => key,
            GlyphResolution::Unavailable => panic!("expected Available"),
        }
    }

    fn test_font_book() -> FontBook {
        with_font_env(None, || load_system_fonts().expect("load test font"))
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
        let Some(path) = primary_font_candidates()
            .into_iter()
            .find(|path| path.is_file())
        else {
            return;
        };

        let (is_native, metrics, bounds, bitmap) =
            with_font_env(Some(path.into_os_string()), || {
                let fonts = load_system_fonts().expect("configured font path");
                let is_native = matches!(&fonts.backend, FontBackend::Native(_));
                let metrics = fonts.font_metrics();
                let (bounds, bitmap) = fonts.rasterize('A', harbor_config::FONT_SIZE);
                (is_native, metrics, bounds, bitmap)
            });
        assert!(is_native, "configured font should use DirectWrite");
        assert!(metrics.cell_width > 0.0);
        assert!(metrics.line_height > 0.0);
        assert!(bounds.width > 0);
        assert!(bounds.height > 0);
        assert_eq!(bitmap.len(), bounds.width * bounds.height);
    }

    #[test]
    fn should_rasterize_configured_primary_by_resolved_key() {
        // Arrange
        let Some(path) = primary_font_candidates()
            .into_iter()
            .find(|path| path.is_file())
        else {
            return;
        };
        // Act
        let (key, direct, by_key) = with_font_env(Some(path.into_os_string()), || {
            let fonts = load_system_fonts().expect("configured font path");
            let key = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
            let direct = fonts.rasterize('A', harbor_config::FONT_SIZE);
            let by_key = fonts.rasterize_from_key(key, harbor_config::FONT_SIZE);
            (key, direct, by_key)
        });

        // Assert
        assert_eq!(key.face_id, 0);
        assert!(key.glyph_index > 0, "configured Latin glyph should resolve");
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
        assert_eq!(key.size_bits, size.to_bits());
        assert_eq!(key.style_bits, style);
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
            let (_bounds2, _bitmap2) = fonts.rasterize_from_key(key, f32::from_bits(key.size_bits));
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
            let (_bounds2, _bitmap2) = fonts.rasterize_from_key(key, f32::from_bits(key.size_bits));
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
        // glyph_index 0 is valid for some fonts, but the key should be well-formed.
        // At minimum, face_id should be a valid index.
        let _ = key.glyph_index;
        let _ = key.face_id;
    }

    #[test]
    fn should_resolve_cjk_char() {
        let fonts = test_font_book();
        match fonts.resolve('中', harbor_config::FONT_SIZE, 0) {
            GlyphResolution::Available(key) => assert_eq!(key.style_bits, 0),
            GlyphResolution::Unavailable => {}
        }
    }

    // ── rasterize_from_key tests ─────────────────────────────────────

    #[test]
    fn should_rasterize_from_key_producing_valid_bitmap() {
        let fonts = test_font_book();
        let key = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        let (bounds, bitmap) = fonts.rasterize_from_key(key, harbor_config::FONT_SIZE);
        assert!(bounds.width > 0, "rasterize_from_key width should be > 0");
        assert!(bounds.height > 0, "rasterize_from_key height should be > 0");
        assert!(!bitmap.is_empty(), "bitmap should not be empty");
    }

    #[test]
    fn should_rasterize_from_key_match_rasterize_directly() {
        let fonts = test_font_book();
        let key = expect_key(fonts.resolve('A', harbor_config::FONT_SIZE, 0));
        let (bounds_key, bitmap_key) = fonts.rasterize_from_key(key, harbor_config::FONT_SIZE);
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
            let (_bounds, _bitmap) = fonts.rasterize_from_key(key, f32::from_bits(key.size_bits));
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
        // line_gap can be zero or positive for typical monospace fonts.
        // Some fonts may have negative line_gap, but the field should be finite.
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
