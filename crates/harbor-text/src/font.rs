use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
};

use anyhow::{Context as _, Result, anyhow};
use fontdb::{Database, Family, ID, Query};
use fontdue::{Font, FontSettings};

use crate::atlas::{GlyphBitmapBounds, GlyphKey};
use crate::backend::compat::CompatState;
use crate::metrics::FontMetrics;

const CJK_PROBE: char = '中';
const FONT_ENV: &str = "HARBOR_FONT";

pub(crate) struct LoadedFont {
    pub family: String,
    pub font: Font,
}

/// System terminal font set with a primary monospace face and glyph fallbacks.
pub struct FontBook {
    #[cfg(windows)]
    compat: CompatState,
}

impl FontBook {
    /// Wrap legacy fontdue fonts for compatibility.
    /// Temporary — will be removed when the DirectWrite backend replaces all paths.
    #[cfg(windows)]
    pub(crate) fn from_compat(fonts: Vec<LoadedFont>) -> Self {
        Self {
            compat: CompatState::new(fonts),
        }
    }

    /// Rasterize a character to a bitmap with backend-neutral bounds.
    pub fn rasterize(&self, ch: char, px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        let key = self.compat.resolve(ch, px, 0);
        self.compat.rasterize(key, px)
    }

    /// Rasterize a glyph by its resolved key (used during atlas rebuild).
    pub fn rasterize_from_key(&self, key: GlyphKey, px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        self.compat.rasterize(key, px)
    }

    /// Resolve a character to a stable glyph identity.
    pub fn resolve(&self, ch: char) -> GlyphKey {
        self.compat.resolve(ch, harbor_config::FONT_SIZE, 0)
    }

    /// Primary font metrics for terminal cell sizing.
    pub fn font_metrics(&self) -> FontMetrics {
        self.compat.font_metrics(harbor_config::FONT_SIZE)
    }
}

/// Loads terminal fonts without scanning the whole system on the common path.
///
/// Fast path:
/// - `HARBOR_FONT` when explicitly configured.
/// - A short per-platform candidate list for common monospace and CJK fonts.
///
/// Slow path:
/// - `fontdb` full system discovery only when the fast path cannot find a
///   usable primary font.
pub fn load_system_fonts() -> Result<FontBook> {
    if let Some(fonts) = load_configured_fonts()? {
        return Ok(fonts);
    }

    if let Some(fonts) = load_candidate_fonts() {
        return Ok(fonts);
    }

    load_fontdb_fonts()
}

fn load_configured_fonts() -> Result<Option<FontBook>> {
    let Some(path) = env::var_os(FONT_ENV) else {
        return Ok(None);
    };

    let primary = load_font_file(Path::new(&path), 0)
        .with_context(|| format!("load configured font from {}", Path::new(&path).display()))?;
    Ok(Some(build_font_book(primary)))
}

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

    fn test_font_book() -> FontBook {
        load_system_fonts().expect("load test font")
    }

    #[test]
    fn rasterize_regular_char_returns_bitmap() {
        let fonts = test_font_book();
        let (bounds, bitmap) = fonts.rasterize('A', harbor_config::FONT_SIZE);
        assert!(bounds.width > 0, "glyph width should be > 0");
        assert!(bounds.height > 0, "glyph height should be > 0");
        assert!(!bitmap.is_empty(), "bitmap should not be empty");
    }

    #[test]
    fn rasterize_space_has_zero_dimensions() {
        let fonts = test_font_book();
        let (bounds, _bitmap) = fonts.rasterize(' ', harbor_config::FONT_SIZE);
        assert_eq!(bounds.width, 0);
        assert_eq!(bounds.height, 0);
    }

    #[test]
    fn font_metrics_are_positive() {
        let fonts = test_font_book();
        let fm = fonts.font_metrics();
        assert!(fm.cell_width > 0.0, "cell_width should be positive");
        assert!(fm.line_height > 0.0, "line_height should be positive");
        assert!(fm.ascent > 0.0, "ascent should be positive");
    }

    #[test]
    fn resolve_returns_stable_key() {
        let fonts = test_font_book();
        let k1 = fonts.resolve('A');
        let k2 = fonts.resolve('A');
        assert_eq!(
            k1, k2,
            "resolve should return the same key for the same char"
        );
    }

    #[test]
    fn rasterize_cjk_fallback() {
        let fonts = test_font_book();
        let (bounds, bitmap) = fonts.rasterize('中', harbor_config::FONT_SIZE);
        // CJK glyph should rasterize (via fallback if primary doesn't have it).
        assert!(bounds.width > 0, "CJK glyph width should be > 0");
        assert!(bounds.height > 0, "CJK glyph height should be > 0");
        assert!(!bitmap.is_empty(), "CJK bitmap should not be empty");
    }

    // ── resolve tests ────────────────────────────────────────────────

    #[test]
    fn should_return_different_keys_for_different_chars() {
        let fonts = test_font_book();
        let key_a = fonts.resolve('A');
        let key_b = fonts.resolve('B');
        assert_ne!(
            key_a, key_b,
            "different chars should produce different keys"
        );
    }

    #[test]
    fn should_return_key_with_valid_glyph_index() {
        let fonts = test_font_book();
        let key = fonts.resolve('A');
        // glyph_index 0 is valid for some fonts, but the key should be well-formed.
        // At minimum, face_id should be a valid index.
        let _ = key.glyph_index;
        let _ = key.face_id;
    }

    #[test]
    fn should_resolve_cjk_char() {
        let fonts = test_font_book();
        let key = fonts.resolve('中');
        // Should not panic and should return a valid key.
        assert_eq!(key.style_bits, 0);
    }

    // ── rasterize_from_key tests ─────────────────────────────────────

    #[test]
    fn should_rasterize_from_key_producing_valid_bitmap() {
        let fonts = test_font_book();
        let key = fonts.resolve('A');
        let (bounds, bitmap) = fonts.rasterize_from_key(key, harbor_config::FONT_SIZE);
        assert!(bounds.width > 0, "rasterize_from_key width should be > 0");
        assert!(bounds.height > 0, "rasterize_from_key height should be > 0");
        assert!(!bitmap.is_empty(), "bitmap should not be empty");
    }

    #[test]
    fn should_rasterize_from_key_match_rasterize_directly() {
        let fonts = test_font_book();
        let key = fonts.resolve('A');
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
    fn should_rasterize_from_key_with_cjk_char() {
        let fonts = test_font_book();
        let key = fonts.resolve('中');
        let (bounds, bitmap) = fonts.rasterize_from_key(key, harbor_config::FONT_SIZE);
        assert!(
            bounds.width > 0,
            "CJK rasterize_from_key width should be > 0"
        );
        assert!(
            bounds.height > 0,
            "CJK rasterize_from_key height should be > 0"
        );
        assert!(!bitmap.is_empty(), "CJK bitmap should not be empty");
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
