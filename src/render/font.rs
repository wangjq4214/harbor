use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
};

use anyhow::{Context as _, Result, anyhow};
use fontdb::{Database, Family, ID, Query};
use fontdue::{Font, FontSettings, Metrics};

const CJK_PROBE: char = '中';
const FONT_ENV: &str = "HARBOR_FONT";

struct LoadedFont {
    family: String,
    font: Font,
}

/// System terminal font set with a primary monospace face and glyph fallbacks.
pub(crate) struct FontBook {
    fonts: Vec<LoadedFont>,
}

impl FontBook {
    pub(crate) fn rasterize(&self, ch: char, px: f32) -> (Metrics, Vec<u8>) {
        self.font_for(ch).rasterize(ch, px)
    }

    /// Font-derived measurements for terminal cell sizing.
    /// `cell_width` comes from the primary monospace face; full-width terminal
    /// cells are handled by the terminal grid, not by doubling font metrics.
    pub(crate) fn terminal_metrics(&self) -> (f32, f32, f32) {
        let font = &self.fonts[0].font;
        let metrics = font.metrics('M', crate::config::FONT_SIZE);
        let cell_width = metrics.advance_width.ceil();
        if let Some(line) = font.horizontal_line_metrics(crate::config::FONT_SIZE) {
            (cell_width, line.new_line_size.ceil(), line.ascent.ceil())
        } else {
            // Fallback: approximate from a representative glyph.
            let h = metrics.bounds.height.ceil();
            (cell_width, h + 4.0, h)
        }
    }

    /// Horizontal line metrics (ascent, descent, line_gap, new_line_size) for the primary
    /// monospace font. Returns `None` only when the font lacks this metric table.
    pub(crate) fn primary_horizontal_line_metrics(
        &self,
        size: f32,
    ) -> Option<fontdue::LineMetrics> {
        self.fonts[0].font.horizontal_line_metrics(size)
    }

    fn font_for(&self, ch: char) -> &Font {
        let font = self
            .fonts
            .iter()
            .find(|font| font.font.has_glyph(ch))
            .unwrap_or(&self.fonts[0]);
        &font.font
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
pub(crate) fn load_system_fonts() -> Result<FontBook> {
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
        return Some(FontBook {
            fonts: vec![primary],
        });
    }

    // Wait for the CJK thread result.
    let fallback = cjk_handle.join().ok()??;
    tracing::info!(
        primary = %primary.family,
        fallback = %fallback.family,
        "loaded terminal fonts from fast path"
    );
    Some(FontBook {
        fonts: vec![primary, fallback],
    })
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

    FontBook { fonts }
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

    Ok(FontBook { fonts })
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
