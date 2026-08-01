//! DirectWrite primary-face backend with system font fallback.
//!
//! Owns a factory, system fallback service, primary descriptor, face registry,
//! and resolution cache. Long-lived state holds only those native handles and
//! caches — never a complete font-file byte buffer.
//!
//! Ownership rules:
//! - System `IDWriteFontCollection` is discovery-only and dropped after primary selection.
//! - `CreateFontFileReference` is consumed by `CreateFontFace` and not stored on `DwriteState`.
//! - `MapCharacters` fonts are registered only when they produce a usable nonzero glyph;
//!   rejected mappings never enter `NativeFaceRegistry`.
//! - Configured paths are opened as process-private faces and are not registered
//!   in system font collections.

use std::{cell::Cell, cell::RefCell, os::windows::ffi::OsStrExt, path::Path};

use anyhow::{Context as _, Result, anyhow, bail};
use hashbrown::HashMap;
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FACE_TYPE, DWRITE_FONT_FILE_TYPE,
    DWRITE_FONT_SIMULATIONS_NONE, DWRITE_FONT_STRETCH, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_GLYPH_METRICS, DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_READING_DIRECTION_LEFT_TO_RIGHT, DWRITE_RENDERING_MODE_ALIASED,
    DWRITE_TEXTURE_ALIASED_1x1, DWriteCreateFactory, IDWriteFactory, IDWriteFactory2,
    IDWriteFontCollection, IDWriteFontFace, IDWriteFontFace1, IDWriteFontFace3,
    IDWriteFontFallback, IDWriteFontFile, IDWriteLocalizedStrings, IDWriteNumberSubstitution,
    IDWriteTextAnalysisSource, IDWriteTextAnalysisSource_Impl,
};
use windows::core::{BOOL, ComObjectInner as _, Interface, implement};

use crate::atlas::{GlyphBitmapBounds, GlyphKey};
use crate::backend::{GlyphResolution, NativeFaceId, ResolutionKey};
use crate::metrics::FontMetrics;

const PRIMARY_FACE_ID: NativeFaceId = NativeFaceId(0);
const LOCALE_NAME_MAX: usize = 85;
/// Keep in sync with `font.rs` and `src/app.rs` (`FONT_LIFECYCLE_TARGET`).
const LIFECYCLE_TARGET: &str = "harbor.font.lifecycle";

/// Primary face identity and style metadata for DirectWrite fallback mapping.
#[derive(Clone)]
struct PrimaryDescriptor {
    #[allow(dead_code)]
    face_id: u64,
    family_name: Vec<u16>,
    weight: DWRITE_FONT_WEIGHT,
    style: DWRITE_FONT_STYLE,
    stretch: DWRITE_FONT_STRETCH,
}

/// Fingerprint for deduplicating native faces across fallback mappings.
#[derive(Clone, PartialEq, Eq, Hash)]
struct FaceFingerprint {
    reference_key: Vec<u8>,
    face_index: u32,
    simulations: u32,
}

/// Session-local stable identity and ownership of native faces used for rendering.
struct NativeFaceRegistry {
    faces: HashMap<u64, IDWriteFontFace>,
    fingerprints: HashMap<FaceFingerprint, u64>,
    next_id: u64,
}

impl NativeFaceRegistry {
    fn with_primary(face: IDWriteFontFace) -> Result<Self> {
        let fingerprint = face_fingerprint(&face)?;
        let mut faces = HashMap::new();
        faces.insert(PRIMARY_FACE_ID.0, face);
        let mut fingerprints = HashMap::new();
        fingerprints.insert(fingerprint, PRIMARY_FACE_ID.0);
        Ok(Self {
            faces,
            fingerprints,
            next_id: 1,
        })
    }

    fn register(&mut self, face: IDWriteFontFace) -> Result<u64> {
        let fingerprint = face_fingerprint(&face)?;
        if let Some(&id) = self.fingerprints.get(&fingerprint) {
            return Ok(id);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.fingerprints.insert(fingerprint, id);
        self.faces.insert(id, face);
        Ok(id)
    }

    fn get(&self, face_id: u64) -> Option<&IDWriteFontFace> {
        self.faces.get(&face_id)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.faces.len()
    }
}

/// Minimal COM text-analysis source for one Unicode scalar.
#[implement(IDWriteTextAnalysisSource)]
struct FallbackTextSource {
    text: Vec<u16>,
    locale: Vec<u16>,
}

impl FallbackTextSource {
    fn new(ch: char, locale: &[u16]) -> Self {
        let mut text = vec![0u16; ch.len_utf16()];
        ch.encode_utf16(&mut text);
        Self {
            text,
            locale: locale.to_vec(),
        }
    }

    #[cfg(test)]
    fn utf16_len(&self) -> u32 {
        self.text.len() as u32
    }
}

impl IDWriteTextAnalysisSource_Impl for FallbackTextSource_Impl {
    fn GetTextAtPosition(
        &self,
        textposition: u32,
        textstring: *mut *mut u16,
        textlength: *mut u32,
    ) -> windows::core::Result<()> {
        unsafe {
            if (textposition as usize) < self.text.len() {
                *textstring = self.text.as_ptr().add(textposition as usize) as *mut u16;
                *textlength = self.text.len() as u32 - textposition;
            } else {
                *textstring = std::ptr::null_mut();
                *textlength = 0;
            }
        }
        Ok(())
    }

    fn GetTextBeforePosition(
        &self,
        textposition: u32,
        textstring: *mut *mut u16,
        textlength: *mut u32,
    ) -> windows::core::Result<()> {
        unsafe {
            if textposition == 0 || textposition as usize > self.text.len() {
                *textstring = std::ptr::null_mut();
                *textlength = 0;
            } else {
                *textstring = self.text.as_ptr() as *mut u16;
                *textlength = textposition;
            }
        }
        Ok(())
    }

    fn GetParagraphReadingDirection(
        &self,
    ) -> windows::Win32::Graphics::DirectWrite::DWRITE_READING_DIRECTION {
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }

    fn GetLocaleName(
        &self,
        textposition: u32,
        textlength: *mut u32,
        localename: *mut *mut u16,
    ) -> windows::core::Result<()> {
        unsafe {
            if (textposition as usize) < self.text.len() {
                *localename = self.locale.as_ptr() as *mut u16;
                *textlength = self.text.len() as u32 - textposition;
            } else {
                *localename = std::ptr::null_mut();
                *textlength = 0;
            }
        }
        Ok(())
    }

    fn GetNumberSubstitution(
        &self,
        _textposition: u32,
        textlength: *mut u32,
        numbersubstitution: windows::core::OutRef<'_, IDWriteNumberSubstitution>,
    ) -> windows::core::Result<()> {
        unsafe {
            *textlength = 0;
        }
        numbersubstitution.write(None).ok();
        Ok(())
    }
}

/// Long-lived DirectWrite primary-face session with system fallback.
pub(crate) struct DwriteState {
    factory: IDWriteFactory2,
    fallback: IDWriteFontFallback,
    descriptor: PrimaryDescriptor,
    faces: RefCell<NativeFaceRegistry>,
    resolutions: RefCell<HashMap<ResolutionKey, GlyphResolution>>,
    locale: Vec<u16>,
    /// Validated metrics at harbor_config::FONT_SIZE, computed during open.
    primary_metrics: FontMetrics,
    /// Once-only gate for the `first_fallback` lifecycle marker.
    first_fallback_emitted: Cell<bool>,
    #[cfg(test)]
    map_calls: Cell<u32>,
}

impl DwriteState {
    /// Select a system monospace/fixed-pitch primary face and retain it.
    pub fn open_system_primary() -> Result<Self> {
        let (factory, fallback, locale) = open_factory_fallback_locale()?;
        let (primary_face, descriptor) = select_system_primary(&factory)?;
        let primary_metrics = font_metrics_from_face(&primary_face, harbor_config::FONT_SIZE)
            .context("measure DirectWrite system primary face")?;
        let faces = NativeFaceRegistry::with_primary(primary_face)
            .context("register DirectWrite system primary face")?;
        Ok(Self {
            factory,
            fallback,
            descriptor,
            faces: RefCell::new(faces),
            resolutions: RefCell::new(HashMap::new()),
            locale,
            primary_metrics,
            first_fallback_emitted: Cell::new(false),
            #[cfg(test)]
            map_calls: Cell::new(0),
        })
    }

    /// Open a process-private primary face from a filesystem font path.
    pub fn open_configured_primary(path: &Path) -> Result<Self> {
        let (factory, fallback, locale) = open_factory_fallback_locale().with_context(|| {
            format!(
                "create DirectWrite factory for configured font {}",
                path.display()
            )
        })?;
        let base: IDWriteFactory = factory.cast().context("cast factory for configured face")?;
        let primary_face = create_face_from_path(&base, path)
            .with_context(|| format!("load configured font from {}", path.display()))?;
        let descriptor = describe_face(&primary_face)
            .with_context(|| format!("describe configured font {}", path.display()))?;
        let primary_metrics = font_metrics_from_face(&primary_face, harbor_config::FONT_SIZE)
            .with_context(|| format!("measure configured font {}", path.display()))?;
        let faces = NativeFaceRegistry::with_primary(primary_face)
            .with_context(|| format!("register configured font {}", path.display()))?;
        Ok(Self {
            factory,
            fallback,
            descriptor,
            faces: RefCell::new(faces),
            resolutions: RefCell::new(HashMap::new()),
            locale,
            primary_metrics,
            first_fallback_emitted: Cell::new(false),
            #[cfg(test)]
            map_calls: Cell::new(0),
        })
    }

    pub fn font_metrics(&self, size: f32) -> FontMetrics {
        if size.to_bits() == harbor_config::FONT_SIZE.to_bits() {
            return self.primary_metrics;
        }
        let faces = self.faces.borrow();
        let Some(primary) = faces.get(PRIMARY_FACE_ID.0) else {
            return self.primary_metrics;
        };
        font_metrics_from_face(primary, size).unwrap_or(self.primary_metrics)
    }

    pub fn resolve(&self, ch: char, size: f32, style: u8) -> GlyphResolution {
        let key = ResolutionKey {
            scalar: ch,
            size_bits: size.to_bits(),
            style_bits: style,
        };
        if let Some(cached) = self.resolutions.borrow().get(&key).copied() {
            return cached;
        }
        let result = self.resolve_uncached(ch, size, style);
        self.resolutions.borrow_mut().insert(key, result);
        result
    }

    fn resolve_uncached(&self, ch: char, size: f32, style: u8) -> GlyphResolution {
        let primary_glyph = match self.primary_glyph_index(ch) {
            Ok(glyph) => glyph,
            Err(_) => return GlyphResolution::Unavailable,
        };
        if primary_glyph != 0 {
            return GlyphResolution::Available(GlyphKey {
                face_id: PRIMARY_FACE_ID.0,
                glyph_index: u32::from(primary_glyph),
                size_bits: size.to_bits(),
                style_bits: style,
            });
        }
        self.map_fallback(ch, size, style)
    }

    fn map_fallback(&self, ch: char, size: f32, style: u8) -> GlyphResolution {
        #[cfg(test)]
        self.map_calls.set(self.map_calls.get() + 1);

        let source = FallbackTextSource::new(ch, &self.locale).into_object();
        let analysis: IDWriteTextAnalysisSource = source.to_interface();
        let text_length = ch.len_utf16() as u32;
        let mut mapped_length = 0u32;
        let mut mapped_font = None;
        let mut scale = 0.0f32;
        let family = windows::core::PCWSTR(self.descriptor.family_name.as_ptr());

        let map_result = unsafe {
            self.fallback.MapCharacters(
                &analysis,
                0,
                text_length,
                None,
                family,
                self.descriptor.weight,
                self.descriptor.style,
                self.descriptor.stretch,
                &mut mapped_length,
                &mut mapped_font,
                &mut scale,
            )
        };
        if map_result.is_err() {
            return GlyphResolution::Unavailable;
        }
        if mapped_length < text_length || scale <= 0.0 {
            return GlyphResolution::Unavailable;
        }
        let Some(font) = mapped_font else {
            return GlyphResolution::Unavailable;
        };
        let face = match unsafe { font.CreateFontFace() } {
            Ok(face) => face,
            Err(_) => return GlyphResolution::Unavailable,
        };
        let glyph = match glyph_index_on_face(&face, ch) {
            Ok(glyph) if glyph != 0 => glyph,
            _ => return GlyphResolution::Unavailable,
        };
        let face_id = match self.faces.borrow_mut().register(face) {
            Ok(id) => id,
            Err(_) => return GlyphResolution::Unavailable,
        };
        let effective_size = size * scale;
        let key = GlyphKey {
            face_id,
            glyph_index: u32::from(glyph),
            size_bits: effective_size.to_bits(),
            style_bits: style,
        };
        self.emit_first_fallback(ch, face_id);
        GlyphResolution::Available(key)
    }

    fn emit_first_fallback(&self, ch: char, face_id: u64) {
        if self.first_fallback_emitted.replace(true) {
            return;
        }
        tracing::info!(
            target: LIFECYCLE_TARGET,
            phase = "first_fallback",
            scalar = %ch,
            face_id,
            "font lifecycle"
        );
    }

    pub fn rasterize(&self, key: GlyphKey, _px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        let px = f32::from_bits(key.size_bits);
        match self.rasterize_inner(key, px) {
            Ok(result) => result,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    glyph = key.glyph_index,
                    face = key.face_id,
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
        let faces = self.faces.borrow();
        let face = faces
            .get(key.face_id)
            .ok_or_else(|| anyhow!("unknown face_id {}", key.face_id))?;
        let glyph = u16::try_from(key.glyph_index)
            .map_err(|_| anyhow!("glyph index {} out of u16 range", key.glyph_index))?;
        let advance = {
            let mut metrics = Default::default();
            unsafe {
                face.GetMetrics(&mut metrics);
            }
            if metrics.designUnitsPerEm == 0 {
                bail!("DirectWrite face reported zero designUnitsPerEm");
            }
            let scale = px / f32::from(metrics.designUnitsPerEm);
            design_advance(face, glyph)? * scale
        };

        let glyph_indices = [glyph];
        let glyph_advances = [advance];
        let glyph_offsets = [DWRITE_GLYPH_OFFSET {
            advanceOffset: 0.0,
            ascenderOffset: 0.0,
        }];
        let mut glyph_run = DWRITE_GLYPH_RUN {
            fontFace: std::mem::ManuallyDrop::new(Some(face.clone())),
            fontEmSize: px,
            glyphCount: 1,
            glyphIndices: glyph_indices.as_ptr(),
            glyphAdvances: glyph_advances.as_ptr(),
            glyphOffsets: glyph_offsets.as_ptr(),
            isSideways: false.into(),
            bidiLevel: 0,
        };

        let base_factory: IDWriteFactory = self.factory.cast()?;
        let analysis_result = unsafe {
            base_factory.CreateGlyphRunAnalysis(
                &glyph_run,
                1.0,
                None,
                DWRITE_RENDERING_MODE_ALIASED,
                DWRITE_MEASURING_MODE_NATURAL,
                0.0,
                0.0,
            )
        };
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

    fn primary_glyph_index(&self, ch: char) -> Result<u16> {
        let faces = self.faces.borrow();
        let face = faces
            .get(PRIMARY_FACE_ID.0)
            .ok_or_else(|| anyhow!("primary face missing from registry"))?;
        glyph_index_on_face(face, ch)
    }

    #[cfg(test)]
    pub fn map_call_count(&self) -> u32 {
        self.map_calls.get()
    }

    #[cfg(test)]
    pub fn face_count(&self) -> usize {
        self.faces.borrow().len()
    }

    #[cfg(test)]
    pub fn primary_family_name(&self) -> String {
        string_from_wide(&self.descriptor.family_name)
    }
}

fn open_factory_fallback_locale() -> Result<(IDWriteFactory2, IDWriteFontFallback, Vec<u16>)> {
    let factory: IDWriteFactory = unsafe {
        DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).context("create DirectWrite factory")?
    };
    let factory2: IDWriteFactory2 = factory
        .cast()
        .context("DirectWrite factory does not support IDWriteFactory2")?;
    let fallback = unsafe {
        factory2
            .GetSystemFontFallback()
            .context("GetSystemFontFallback")?
    };
    let locale = windows_user_locale().context("read Windows user locale")?;
    Ok((factory2, fallback, locale))
}

fn windows_user_locale() -> Result<Vec<u16>> {
    let mut buffer = vec![0u16; LOCALE_NAME_MAX];
    let written = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if written <= 0 {
        bail!("GetUserDefaultLocaleName failed");
    }
    buffer.truncate(written as usize);
    if buffer.last().copied() != Some(0) {
        buffer.push(0);
    }
    Ok(buffer)
}

fn create_face_from_path(factory: &IDWriteFactory, path: &Path) -> Result<IDWriteFontFace> {
    if path.as_os_str().is_empty() {
        bail!("configured font path is empty");
    }

    let path_display = path.display().to_string();
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let font_file = unsafe {
        factory
            .CreateFontFileReference(windows::core::PCWSTR(wide_path.as_ptr()), None)
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

fn select_system_primary(
    factory: &IDWriteFactory2,
) -> Result<(IDWriteFontFace, PrimaryDescriptor)> {
    let base: IDWriteFactory = factory.cast()?;
    let mut collection: Option<IDWriteFontCollection> = None;
    unsafe {
        base.GetSystemFontCollection(&mut collection, false)
            .context("GetSystemFontCollection")?;
    }
    let collection = collection.ok_or_else(|| anyhow!("system font collection was null"))?;

    let family_count = unsafe { collection.GetFontFamilyCount() };
    let mut selected: Option<(IDWriteFontFace, PrimaryDescriptor)> = None;

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
        let mut glyphs = [0u16; 1];
        let codepoints = [u32::from('M')];
        if unsafe { face.GetGlyphIndices(codepoints.as_ptr(), 1, glyphs.as_mut_ptr()) }.is_err()
            || glyphs[0] == 0
        {
            continue;
        }
        let family_name = match localized_family_name_from_family(&family) {
            Ok(name) => name,
            Err(_) => continue,
        };
        let descriptor = PrimaryDescriptor {
            face_id: PRIMARY_FACE_ID.0,
            family_name,
            weight: unsafe { font.GetWeight() },
            style: unsafe { font.GetStyle() },
            stretch: unsafe { font.GetStretch() },
        };
        selected = Some((face, descriptor));
        break;
    }

    drop(collection);
    selected.ok_or_else(|| anyhow!("no usable system monospace face"))
}

fn describe_face(face: &IDWriteFontFace) -> Result<PrimaryDescriptor> {
    let face3: IDWriteFontFace3 = face
        .cast()
        .context("DirectWrite face does not support IDWriteFontFace3 metadata")?;
    let names = unsafe { face3.GetFamilyNames() }.context("GetFamilyNames")?;
    let family_name = localized_string(&names).context("read primary family name")?;
    Ok(PrimaryDescriptor {
        face_id: PRIMARY_FACE_ID.0,
        family_name,
        weight: unsafe { face3.GetWeight() },
        style: unsafe { face3.GetStyle() },
        stretch: unsafe { face3.GetStretch() },
    })
}

fn localized_family_name_from_family(
    family: &windows::Win32::Graphics::DirectWrite::IDWriteFontFamily,
) -> Result<Vec<u16>> {
    let names = unsafe { family.GetFamilyNames() }.context("GetFamilyNames")?;
    localized_string(&names)
}

fn localized_string(names: &IDWriteLocalizedStrings) -> Result<Vec<u16>> {
    let count = unsafe { names.GetCount() };
    if count == 0 {
        bail!("localized string list is empty");
    }
    let index = 0u32;
    let length = unsafe { names.GetStringLength(index) }.context("GetStringLength")?;
    let mut buffer = vec![0u16; length as usize + 1];
    unsafe {
        names.GetString(index, &mut buffer).context("GetString")?;
    }
    if buffer.last().copied() != Some(0) {
        buffer.push(0);
    }
    Ok(buffer)
}

fn glyph_index_on_face(face: &IDWriteFontFace, ch: char) -> Result<u16> {
    let codepoints = [u32::from(ch)];
    let mut glyphs = [0u16; 1];
    unsafe {
        face.GetGlyphIndices(codepoints.as_ptr(), 1, glyphs.as_mut_ptr())
            .context("GetGlyphIndices")?;
    }
    Ok(glyphs[0])
}

fn design_advance(face: &IDWriteFontFace, glyph: u16) -> Result<f32> {
    let glyphs = [glyph];
    let mut metrics = [DWRITE_GLYPH_METRICS::default(); 1];
    unsafe {
        face.GetDesignGlyphMetrics(glyphs.as_ptr(), 1, metrics.as_mut_ptr(), false)
            .context("GetDesignGlyphMetrics")?;
    }
    Ok(metrics[0].advanceWidth as f32)
}

fn face_fingerprint(face: &IDWriteFontFace) -> Result<FaceFingerprint> {
    let mut number_of_files = 0u32;
    unsafe {
        face.GetFiles(&mut number_of_files, None)
            .context("GetFiles count")?;
    }
    if number_of_files == 0 {
        bail!("DirectWrite face reported zero font files");
    }
    let mut files: Vec<Option<IDWriteFontFile>> = vec![None; number_of_files as usize];
    unsafe {
        face.GetFiles(&mut number_of_files, Some(files.as_mut_ptr()))
            .context("GetFiles")?;
    }
    let mut reference_key = Vec::new();
    for file in files.into_iter().flatten() {
        let mut key_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut key_size = 0u32;
        unsafe {
            file.GetReferenceKey(&mut key_ptr as *mut *mut std::ffi::c_void, &mut key_size)
                .context("GetReferenceKey")?;
        }
        if !key_ptr.is_null() && key_size > 0 {
            let bytes =
                unsafe { std::slice::from_raw_parts(key_ptr as *const u8, key_size as usize) };
            reference_key.extend_from_slice(bytes);
        }
        reference_key.push(0xff);
    }
    let face_index = unsafe { face.GetIndex() };
    let simulations = unsafe { face.GetSimulations() };
    Ok(FaceFingerprint {
        reference_key,
        face_index,
        simulations: simulations.0 as u32,
    })
}

#[cfg(test)]
fn string_from_wide(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::{env, fs, path::PathBuf};

    use tracing::field::{Field, Visit};
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

    use super::*;

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
        let capture = LifecycleCapture::new();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let result = tracing::subscriber::with_default(subscriber, f);
        (result, capture)
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
    #[ignore = "manual E2E: cold-start Harbor with HARBOR_FONT unset and verify first Latin frame"]
    fn should_present_first_latin_frame_when_harbor_font_unset() {
        let _ = DwriteState::open_system_primary().expect("system primary available for E2E");
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
    fn should_open_system_primary_when_no_harbor_font() {
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
        assert_eq!(first.face_id, PRIMARY_FACE_ID.0);
        assert_eq!(state.map_call_count(), 0);
    }

    #[test]
    fn should_return_non_empty_bitmap_when_rasterizing_latin() {
        let state = open_primary();
        let key = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
        let (bounds, bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);
        assert!(bounds.width > 0);
        assert!(bounds.height > 0);
        assert_eq!(bitmap.len(), bounds.width * bounds.height);
    }

    #[test]
    fn should_return_zero_ink_with_positive_advance_when_rasterizing_space() {
        let state = open_primary();
        let key = expect_available(state.resolve(' ', harbor_config::FONT_SIZE, 0));
        let (bounds, bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);
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
        if key.face_id != PRIMARY_FACE_ID.0 {
            assert_eq!(state.map_call_count(), 1);
            assert!(state.face_count() >= before_faces);
            let again = expect_available(state.resolve('中', harbor_config::FONT_SIZE, 0));
            assert_eq!(again, key);
            assert_eq!(state.map_call_count(), 1);
            let (bounds, bitmap) = state.rasterize(key, f32::from_bits(key.size_bits));
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
        let state = open_primary();

        // Act
        let (_, capture) = with_lifecycle_capture(|| {
            let _ = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
        });

        // Assert
        assert!(
            capture.events_with_phase("first_fallback").is_empty(),
            "primary Latin resolve must not emit first_fallback"
        );
    }

    #[test]
    fn should_emit_first_fallback_once_when_mapping_non_primary_face() {
        // Arrange
        let state = open_primary();

        // Act
        let (fallback_face, capture) = with_lifecycle_capture(|| {
            let resolution = state.resolve('中', harbor_config::FONT_SIZE, 0);
            let GlyphResolution::Available(key) = resolution else {
                return None;
            };
            if key.face_id == PRIMARY_FACE_ID.0 {
                return None;
            }
            let _ = expect_available(state.resolve('国', harbor_config::FONT_SIZE, 0));
            Some(key.face_id)
        });

        // Assert — skip when primary already covers CJK (no fallback path).
        let Some(face_id) = fallback_face else {
            return;
        };
        let events = capture.events_with_phase("first_fallback");
        assert_eq!(events.len(), 1, "first_fallback must emit once");
        let expected_face = face_id.to_string();
        assert_eq!(events[0].get("face_id"), Some(&expected_face));
        assert!(events[0].contains_key("scalar"));
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
        if key.face_id != PRIMARY_FACE_ID.0 {
            assert_eq!(state.map_call_count(), 1);
            let (bounds, _) = state.rasterize(key, f32::from_bits(key.size_bits));
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
        assert_eq!(key.size_bits, size.to_bits());
        assert_eq!(key.style_bits, style);
        assert_eq!(key.face_id, PRIMARY_FACE_ID.0);
    }

    #[test]
    fn should_return_different_keys_when_resolving_different_chars() {
        let state = open_primary();
        let key_a = expect_available(state.resolve('A', harbor_config::FONT_SIZE, 0));
        let key_b = expect_available(state.resolve('B', harbor_config::FONT_SIZE, 0));
        assert_ne!(key_a, key_b);
        assert_ne!(key_a.glyph_index, key_b.glyph_index);
    }

    #[test]
    fn should_return_empty_bitmap_when_rasterizing_out_of_range_glyph() {
        let state = open_primary();
        let key = GlyphKey {
            face_id: PRIMARY_FACE_ID.0,
            glyph_index: u32::from(u16::MAX) + 1,
            size_bits: harbor_config::FONT_SIZE.to_bits(),
            style_bits: 0,
        };
        let (bounds, bitmap) = state.rasterize(key, harbor_config::FONT_SIZE);
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
            face_id: cjk_key.face_id,
            glyph_index: latin.glyph_index,
            size_bits: cjk_key.size_bits,
            style_bits: cjk_key.style_bits,
        };
        assert_ne!(latin, colliding);
    }
}
