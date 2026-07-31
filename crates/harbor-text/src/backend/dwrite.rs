//! DirectWrite primary-face backend for default Windows font startup.
//!
//! Owns a factory and retained primary face. Discovery-only collections are
//! released after selection. Configured paths are opened as process-private
//! faces and are not registered in system font collections.

use std::{os::windows::ffi::OsStrExt, path::Path};

use anyhow::{Context as _, Result, anyhow, bail};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FACE_TYPE, DWRITE_FONT_FILE_TYPE,
    DWRITE_FONT_SIMULATIONS_NONE, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_GLYPH_METRICS, DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_RENDERING_MODE_ALIASED, DWRITE_TEXTURE_ALIASED_1x1,
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteFontFace, IDWriteFontFace1,
};
use windows::core::{BOOL, Interface, PCWSTR};

use crate::atlas::{GlyphBitmapBounds, GlyphKey};
use crate::backend::NativeFaceId;
use crate::metrics::FontMetrics;

const PRIMARY_FACE_ID: NativeFaceId = NativeFaceId(0);

/// Long-lived DirectWrite primary-face session.
pub(crate) struct DwriteState {
    factory: IDWriteFactory,
    primary_face: IDWriteFontFace,
    face_id: NativeFaceId,
    /// Validated metrics at harbor_config::FONT_SIZE, computed during open.
    primary_metrics: FontMetrics,
}

impl DwriteState {
    /// Select a system monospace/fixed-pitch primary face and retain it.
    pub fn open_system_primary() -> Result<Self> {
        let factory: IDWriteFactory = unsafe {
            DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).context("create DirectWrite factory")?
        };
        let primary_face = select_system_primary(&factory)?;
        let primary_metrics = font_metrics_from_face(&primary_face, harbor_config::FONT_SIZE)
            .context("measure DirectWrite system primary face")?;
        Ok(Self {
            factory,
            primary_face,
            face_id: PRIMARY_FACE_ID,
            primary_metrics,
        })
    }

    /// Open a process-private primary face from a filesystem font path.
    pub fn open_configured_primary(path: &Path) -> Result<Self> {
        let factory: IDWriteFactory = unsafe {
            DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).with_context(|| {
                format!(
                    "create DirectWrite factory for configured font {}",
                    path.display()
                )
            })?
        };
        let primary_face = create_face_from_path(&factory, path)
            .with_context(|| format!("load configured font from {}", path.display()))?;
        let primary_metrics = font_metrics_from_face(&primary_face, harbor_config::FONT_SIZE)
            .with_context(|| format!("measure configured font {}", path.display()))?;
        Ok(Self {
            factory,
            primary_face,
            face_id: PRIMARY_FACE_ID,
            primary_metrics,
        })
    }

    pub fn font_metrics(&self, size: f32) -> FontMetrics {
        if size.to_bits() == harbor_config::FONT_SIZE.to_bits() {
            return self.primary_metrics;
        }
        // Non-default sizes are rare; open already proved the face is measurable.
        font_metrics_from_face(&self.primary_face, size).unwrap_or(self.primary_metrics)
    }

    pub fn resolve(&self, ch: char, size: f32, style: u8) -> GlyphKey {
        let glyph_index = self.glyph_index(ch).unwrap_or(0) as u32;
        GlyphKey {
            face_id: self.face_id.0,
            glyph_index,
            size_bits: size.to_bits(),
            style_bits: style,
        }
    }

    pub fn rasterize(&self, key: GlyphKey, px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        match self.rasterize_inner(key, px) {
            Ok(result) => result,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    glyph = key.glyph_index,
                    "DirectWrite rasterize failed"
                );
                (
                    GlyphBitmapBounds {
                        width: 0,
                        height: 0,
                        bearing_x: 0,
                        bearing_y: 0,
                        advance_width: 0.0,
                    },
                    Vec::new(),
                )
            }
        }
    }

    fn rasterize_inner(&self, key: GlyphKey, px: f32) -> Result<(GlyphBitmapBounds, Vec<u8>)> {
        let glyph = u16::try_from(key.glyph_index)
            .map_err(|_| anyhow!("glyph index {} out of u16 range", key.glyph_index))?;
        let advance = {
            let mut metrics = Default::default();
            unsafe {
                self.primary_face.GetMetrics(&mut metrics);
            }
            if metrics.designUnitsPerEm == 0 {
                bail!("DirectWrite face reported zero designUnitsPerEm");
            }
            let scale = px / f32::from(metrics.designUnitsPerEm);
            self.design_advance(glyph)? * scale
        };

        let glyph_indices = [glyph];
        let glyph_advances = [advance];
        let glyph_offsets = [DWRITE_GLYPH_OFFSET {
            advanceOffset: 0.0,
            ascenderOffset: 0.0,
        }];
        let mut glyph_run = DWRITE_GLYPH_RUN {
            fontFace: std::mem::ManuallyDrop::new(Some(self.primary_face.clone())),
            fontEmSize: px,
            glyphCount: 1,
            glyphIndices: glyph_indices.as_ptr(),
            glyphAdvances: glyph_advances.as_ptr(),
            glyphOffsets: glyph_offsets.as_ptr(),
            isSideways: false.into(),
            bidiLevel: 0,
        };

        let analysis_result = unsafe {
            self.factory.CreateGlyphRunAnalysis(
                &glyph_run,
                1.0,
                None,
                DWRITE_RENDERING_MODE_ALIASED,
                DWRITE_MEASURING_MODE_NATURAL,
                0.0,
                0.0,
            )
        };
        // Always release the temporary face clone held in the glyph run.
        drop(unsafe { std::mem::ManuallyDrop::take(&mut glyph_run.fontFace) });
        let analysis = analysis_result.context("CreateGlyphRunAnalysis")?;

        let bounds = unsafe { analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1) }
            .context("GetAlphaTextureBounds")?;
        let width = (bounds.right - bounds.left).max(0) as usize;
        let height = (bounds.bottom - bounds.top).max(0) as usize;
        if width == 0 || height == 0 {
            return Ok((
                GlyphBitmapBounds {
                    width: 0,
                    height: 0,
                    bearing_x: 0,
                    // Match fontdue ymin semantics used by the terminal baseline math.
                    bearing_y: 0,
                    advance_width: advance,
                },
                Vec::new(),
            ));
        }

        let mut pixels = vec![0u8; width * height];
        unsafe {
            analysis
                .CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &bounds, &mut pixels)
                .context("CreateAlphaTexture")?;
        }

        // Terminal placement: baseline - bearing_y is the glyph bottom.
        // DirectWrite texture bounds use baseline origin at (0,0) with y-down,
        // so bottom is bounds.bottom and top is bounds.top.
        // fontdue ymin is the distance from baseline to bitmap bottom (often negative).
        let bearing_x = bounds.left;
        let bearing_y = -bounds.bottom;

        Ok((
            GlyphBitmapBounds {
                width,
                height,
                bearing_x,
                bearing_y,
                advance_width: advance,
            },
            pixels,
        ))
    }

    fn glyph_index(&self, ch: char) -> Result<u16> {
        let codepoints = [u32::from(ch)];
        let mut glyphs = [0u16; 1];
        unsafe {
            self.primary_face
                .GetGlyphIndices(codepoints.as_ptr(), 1, glyphs.as_mut_ptr())
                .context("GetGlyphIndices")?;
        }
        Ok(glyphs[0])
    }

    fn design_advance(&self, glyph: u16) -> Result<f32> {
        let glyphs = [glyph];
        let mut metrics = [DWRITE_GLYPH_METRICS::default(); 1];
        unsafe {
            self.primary_face
                .GetDesignGlyphMetrics(glyphs.as_ptr(), 1, metrics.as_mut_ptr(), false)
                .context("GetDesignGlyphMetrics")?;
        }
        Ok(metrics[0].advanceWidth as f32)
    }
}

fn create_face_from_path(factory: &IDWriteFactory, path: &Path) -> Result<IDWriteFontFace> {
    if path.as_os_str().is_empty() {
        bail!("configured font path is empty");
    }

    let path_display = path.display().to_string();
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let font_file = unsafe {
        factory
            .CreateFontFileReference(PCWSTR(wide_path.as_ptr()), None)
            .with_context(|| format!("CreateFontFileReference for {path_display}"))?
    };

    let mut is_supported = BOOL(0);
    let mut file_type = DWRITE_FONT_FILE_TYPE(0);
    let mut face_type = DWRITE_FONT_FACE_TYPE(0);
    let mut number_of_faces = 0;
    unsafe {
        font_file
            .Analyze(
                &mut is_supported,
                &mut file_type,
                Some(&mut face_type),
                &mut number_of_faces,
            )
            .with_context(|| format!("Analyze configured font {path_display}"))?;
    }
    if !is_supported.as_bool() || number_of_faces == 0 {
        bail!("configured font {path_display} is unsupported or contains no faces");
    }

    let font_files = [Some(font_file)];
    unsafe {
        factory
            .CreateFontFace(face_type, &font_files, 0, DWRITE_FONT_SIMULATIONS_NONE)
            .with_context(|| format!("CreateFontFace for configured font {path_display}"))
    }
}

fn font_metrics_from_face(face: &IDWriteFontFace, size: f32) -> Result<FontMetrics> {
    let mut metrics = Default::default();
    unsafe {
        face.GetMetrics(&mut metrics);
    }
    if metrics.designUnitsPerEm == 0 {
        bail!("DirectWrite face reported zero designUnitsPerEm");
    }
    let scale = size / f32::from(metrics.designUnitsPerEm);
    let codepoints = [u32::from('M')];
    let mut glyphs = [0u16; 1];
    unsafe {
        face.GetGlyphIndices(codepoints.as_ptr(), 1, glyphs.as_mut_ptr())
            .context("GetGlyphIndices")?;
    }
    let mut glyph_metrics = [DWRITE_GLYPH_METRICS::default(); 1];
    unsafe {
        face.GetDesignGlyphMetrics(glyphs.as_ptr(), 1, glyph_metrics.as_mut_ptr(), false)
            .context("GetDesignGlyphMetrics")?;
    }
    let advance = glyph_metrics[0].advanceWidth as f32;
    let cell_width = (advance * scale).ceil();
    let ascent = f32::from(metrics.ascent) * scale;
    let descent = f32::from(metrics.descent) * scale;
    let line_gap = f32::from(metrics.lineGap) * scale;
    let line_height = (ascent + descent + line_gap).ceil();
    if cell_width <= 0.0 || line_height <= 0.0 || ascent <= 0.0 {
        bail!(
            "DirectWrite primary metrics non-positive: cell_width={cell_width}, line_height={line_height}, ascent={ascent}"
        );
    }
    Ok(FontMetrics {
        cell_width,
        line_height,
        ascent: ascent.ceil(),
        descent,
        line_gap,
    })
}

fn select_system_primary(factory: &IDWriteFactory) -> Result<IDWriteFontFace> {
    let mut collection: Option<IDWriteFontCollection> = None;
    unsafe {
        factory
            .GetSystemFontCollection(&mut collection, false)
            .context("GetSystemFontCollection")?;
    }
    let collection = collection.ok_or_else(|| anyhow!("system font collection was null"))?;

    let family_count = unsafe { collection.GetFontFamilyCount() };
    let mut selected: Option<IDWriteFontFace> = None;

    for family_index in 0..family_count {
        let family = unsafe { collection.GetFontFamily(family_index) }
            .with_context(|| format!("GetFontFamily({family_index})"))?;
        let font = match unsafe {
            family.GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )
        } {
            Ok(font) => font,
            Err(_) => continue,
        };
        if unsafe { font.IsSymbolFont() }.as_bool() {
            continue;
        }
        let face = match unsafe { font.CreateFontFace() } {
            Ok(face) => face,
            Err(_) => continue,
        };
        let face1: IDWriteFontFace1 = match face.cast() {
            Ok(face1) => face1,
            Err(_) => continue,
        };
        if !unsafe { face1.IsMonospacedFont() }.as_bool() {
            continue;
        }
        // Prefer faces that can render basic Latin for the terminal.
        let mut glyphs = [0u16; 1];
        let codepoints = [u32::from('M')];
        if unsafe { face.GetGlyphIndices(codepoints.as_ptr(), 1, glyphs.as_mut_ptr()) }.is_err()
            || glyphs[0] == 0
        {
            continue;
        }
        selected = Some(face);
        break;
    }

    // Drop discovery-only collection (and any temporaries) by falling out of scope.
    drop(collection);

    selected.ok_or_else(|| anyhow!("no usable system monospace face"))
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use super::*;

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

    /// Manual E2E checklist for first Latin presentation (spec 0003 / T0002).
    ///
    /// Run with: `cargo run` (or the packaged Harbor binary) on Windows with
    /// `HARBOR_FONT` unset. Confirm the first terminal frame shows Latin text
    /// through the existing WGPU R8 atlas (no blank/missing glyphs for ASCII).
    #[test]
    #[ignore = "manual E2E: cold-start Harbor with HARBOR_FONT unset and verify first Latin frame"]
    fn should_present_first_latin_frame_when_harbor_font_unset() {
        // Arrange / Act — exercised by the running Harbor app, not unit harness.
        // Assert — operator verifies visible Latin text on first presentation.
        let _ = DwriteState::open_system_primary().expect("system primary available for E2E");
    }

    fn open_primary() -> DwriteState {
        DwriteState::open_system_primary().expect("open system primary")
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

        let key = state.resolve('A', harbor_config::FONT_SIZE, 0);
        let (bounds, bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);
        assert!(bounds.width > 0);
        assert!(bounds.height > 0);
        assert_eq!(bitmap.len(), bounds.width * bounds.height);
    }

    #[test]
    fn should_open_first_face_of_available_collection() {
        let Some(path) = configured_collection_path() else {
            return;
        };

        let state =
            DwriteState::open_configured_primary(&path).expect("configured collection opens");
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
            let _state =
                DwriteState::open_configured_primary(&path).expect("configured font opens");
            assert_eq!(collection_family_count(), before);
        }
        assert_eq!(collection_family_count(), before);
    }

    #[test]
    fn should_reject_empty_configured_path_before_directwrite_call() {
        // Arrange
        let path = Path::new("");

        // Act
        let error = match DwriteState::open_configured_primary(path) {
            Ok(_) => panic!("empty configured path unexpectedly opened"),
            Err(error) => error,
        };

        // Assert
        let message = format!("{error:#}");
        assert!(
            message.contains("configured font path is empty"),
            "{message}"
        );
    }

    #[test]
    fn should_include_missing_configured_path_in_error() {
        // Arrange
        let path = env::temp_dir().join(format!("harbor-missing-font-{}.ttf", std::process::id()));
        let _ = fs::remove_file(&path);

        // Act
        let error = match DwriteState::open_configured_primary(&path) {
            Ok(_) => panic!("missing font unexpectedly opened"),
            Err(error) => error,
        };

        // Assert
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
    fn should_open_system_primary_when_no_harbor_font() {
        // Arrange

        // Act
        let result = DwriteState::open_system_primary();

        // Assert
        result.expect("expected system primary face");
    }

    #[test]
    fn should_return_positive_metrics_when_system_primary_opens() {
        // Arrange
        let state = open_primary();

        // Act
        let metrics = state.font_metrics(harbor_config::FONT_SIZE);

        // Assert
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
    }

    #[test]
    fn should_return_stable_metrics_when_queried_repeatedly() {
        // Arrange
        let state = open_primary();

        // Act
        let first = state.font_metrics(harbor_config::FONT_SIZE);
        let second = state.font_metrics(harbor_config::FONT_SIZE);

        // Assert
        assert_eq!(first.cell_width, second.cell_width);
        assert_eq!(first.line_height, second.line_height);
        assert_eq!(first.ascent, second.ascent);
        assert_eq!(first.descent, second.descent);
        assert_eq!(first.line_gap, second.line_gap);
    }

    #[test]
    fn should_return_stable_key_when_resolving_same_latin_char() {
        // Arrange
        let state = open_primary();

        // Act
        let first = state.resolve('A', harbor_config::FONT_SIZE, 0);
        let second = state.resolve('A', harbor_config::FONT_SIZE, 0);

        // Assert
        assert_eq!(first, second);
        assert_eq!(first.face_id, PRIMARY_FACE_ID.0);
    }

    #[test]
    fn should_return_non_empty_bitmap_when_rasterizing_latin() {
        // Arrange
        let state = open_primary();
        let key = state.resolve('A', harbor_config::FONT_SIZE, 0);

        // Act
        let (bounds, bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);

        // Assert
        assert!(
            bounds.width > 0,
            "A glyph empty: key={key:?}, bounds={bounds:?}, bitmap_len={}",
            bitmap.len()
        );
        assert!(bounds.height > 0);
        assert_eq!(bitmap.len(), bounds.width * bounds.height);
    }

    #[test]
    fn should_return_zero_ink_with_positive_advance_when_rasterizing_space() {
        // Arrange
        let state = open_primary();
        let key = state.resolve(' ', harbor_config::FONT_SIZE, 0);

        // Act
        let (bounds, bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);

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
    fn should_not_panic_when_resolving_and_rasterizing_missing_glyph() {
        // Arrange
        let state = open_primary();
        // Private-use scalar is almost certainly missing from a monospace face.
        let ch = '\u{E000}';

        // Act
        let key = state.resolve(ch, harbor_config::FONT_SIZE, 0);
        let (_bounds, _bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);

        // Assert — completing without panic is the observable contract for T0002.
        let _ = key.face_id;
    }

    #[test]
    fn should_return_positive_metrics_when_size_differs_from_default() {
        // Arrange
        let state = open_primary();
        let other_size = harbor_config::FONT_SIZE + 4.0;

        // Act
        let metrics = state.font_metrics(other_size);

        // Assert
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
    }

    #[test]
    fn should_embed_size_and_style_in_key_when_resolving() {
        // Arrange
        let state = open_primary();
        let size = harbor_config::FONT_SIZE;
        let style = 1u8;

        // Act
        let key = state.resolve('A', size, style);

        // Assert
        assert_eq!(key.size_bits, size.to_bits());
        assert_eq!(key.style_bits, style);
        assert_eq!(key.face_id, PRIMARY_FACE_ID.0);
    }

    #[test]
    fn should_return_different_keys_when_resolving_different_chars() {
        // Arrange
        let state = open_primary();

        // Act
        let key_a = state.resolve('A', harbor_config::FONT_SIZE, 0);
        let key_b = state.resolve('B', harbor_config::FONT_SIZE, 0);

        // Assert
        assert_ne!(key_a, key_b);
        assert_ne!(key_a.glyph_index, key_b.glyph_index);
    }

    #[test]
    fn should_return_empty_bitmap_when_rasterizing_out_of_range_glyph() {
        // Arrange
        let state = open_primary();
        let key = GlyphKey {
            face_id: PRIMARY_FACE_ID.0,
            glyph_index: u32::from(u16::MAX) + 1,
            size_bits: harbor_config::FONT_SIZE.to_bits(),
            style_bits: 0,
        };

        // Act
        let (bounds, bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);

        // Assert — public contract: rasterize never panics; failure yields empty ink.
        assert_eq!(bounds.width, 0);
        assert_eq!(bounds.height, 0);
        assert!(bitmap.is_empty());
        assert_eq!(bounds.advance_width, 0.0);
    }
}
