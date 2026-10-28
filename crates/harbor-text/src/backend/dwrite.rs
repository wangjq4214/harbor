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

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[cfg(test)]
use std::{os::windows::ffi::OsStrExt, path::Path};

use anyhow::{Context as _, Result, anyhow, bail};
use hashbrown::HashMap;
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_GLYPH_METRICS,
    DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN, DWRITE_GRID_FIT_MODE_ENABLED,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_READING_DIRECTION_LEFT_TO_RIGHT,
    DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    DWRITE_TEXTURE_ALIASED_1x1, DWriteCreateFactory, IDWriteFactory, IDWriteFactory2,
    IDWriteFontCollection, IDWriteFontFace, IDWriteFontFace1, IDWriteFontFallback, IDWriteFontFile,
    IDWriteLocalizedStrings, IDWriteNumberSubstitution, IDWriteTextAnalysisSource,
    IDWriteTextAnalysisSource_Impl,
};
#[cfg(test)]
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_FACE_TYPE, DWRITE_FONT_FILE_TYPE, DWRITE_FONT_SIMULATIONS_NONE, IDWriteFontFace3,
};
use windows::core::{BOOL, ComObjectInner as _, Interface, implement};

mod sequence;
use crate::atlas::GlyphBitmapBounds;
use crate::contracts::{
    FaceId, FontSize, FontStyle, GlyphId, GlyphKey, GlyphResolution, ResolutionKey,
};
use crate::lifecycle::{FontLifecycleEvent, FontLifecycleSink};
use crate::metrics::FontMetrics;

const PRIMARY_FACE_ID: FaceId = FaceId::PRIMARY;
const LOCALE_NAME_MAX: usize = 85;

/// Primary family and style metadata for DirectWrite fallback mapping.
#[derive(Clone)]
struct PrimaryDescriptor {
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
    faces: HashMap<FaceId, IDWriteFontFace>,
    fingerprints: HashMap<FaceFingerprint, FaceId>,
}

impl NativeFaceRegistry {
    fn with_primary(face: IDWriteFontFace) -> Result<Self> {
        let fingerprint = face_fingerprint(&face)?;
        let mut faces = HashMap::new();
        faces.insert(PRIMARY_FACE_ID, face);
        let mut fingerprints = HashMap::new();
        fingerprints.insert(fingerprint, PRIMARY_FACE_ID);
        Ok(Self {
            faces,
            fingerprints,
        })
    }

    fn register(&mut self, face: IDWriteFontFace) -> Result<FaceId> {
        let fingerprint = face_fingerprint(&face)?;
        if let Some(&id) = self.fingerprints.get(&fingerprint) {
            return Ok(id);
        }
        // IDs start at zero and faces are never removed from this session.
        let id = FaceId::new(self.faces.len() as u64);
        self.fingerprints.insert(fingerprint, id);
        self.faces.insert(id, face);
        Ok(id)
    }

    fn get(&self, face_id: FaceId) -> Option<&IDWriteFontFace> {
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

/// Native handles and the session-local face registry shared by its components.
struct DirectWriteSession {
    factory: IDWriteFactory2,
    fallback: IDWriteFontFallback,
    descriptor: PrimaryDescriptor,
    faces: Rc<RefCell<NativeFaceRegistry>>,
    locale: Vec<u16>,
    lifecycle: Rc<dyn FontLifecycleSink>,
}

/// Character resolution, fallback mapping, and resolution caching.
struct GlyphResolver {
    session: Rc<DirectWriteSession>,
    resolutions: RefCell<HashMap<ResolutionKey, GlyphResolution>>,
    first_fallback_emitted: Cell<bool>,
    #[cfg(test)]
    map_calls: Cell<u32>,
}

/// Glyph bitmap rasterization using the session's factory and face registry.
struct GlyphRasterizer {
    factory: IDWriteFactory2,
    faces: Rc<RefCell<NativeFaceRegistry>>,
}

/// Small composition root for one DirectWrite font session.
pub(crate) struct DwriteState {
    session: Rc<DirectWriteSession>,
    resolver: GlyphResolver,
    rasterizer: GlyphRasterizer,
    sequence: RefCell<sequence::SequencePresenter>,
    primary_metrics: FontMetrics,
    primary_warning: Option<String>,
}

impl DwriteState {
    pub(crate) fn primary_warning(&self) -> Option<&str> {
        self.primary_warning.as_deref()
    }
    pub(crate) fn present_sequence(
        &self,
        request: &crate::SequenceRequest,
    ) -> std::sync::Arc<crate::SequencePresentation> {
        self.sequence.borrow_mut().present(&self.session, request)
    }
    pub(crate) fn presentation_generation(&self) -> crate::PresentationGeneration {
        self.sequence.borrow().generation()
    }
    pub(crate) fn invalidate_presentations(&self) {
        self.sequence.borrow_mut().invalidate();
    }
    pub(crate) fn sequence_cache_stats(&self) -> crate::SequenceCacheStats {
        self.sequence.borrow().stats()
    }
    pub(crate) fn sequence_capabilities(&self) -> crate::SequenceCapabilities {
        self.sequence.borrow_mut().capabilities(&self.session)
    }

    fn from_session(session: DirectWriteSession, primary_metrics: FontMetrics) -> Self {
        let session = Rc::new(session);
        let faces = Rc::clone(&session.faces);
        let resolver = GlyphResolver {
            session: Rc::clone(&session),
            resolutions: RefCell::new(HashMap::new()),
            first_fallback_emitted: Cell::new(false),
            #[cfg(test)]
            map_calls: Cell::new(0),
        };
        let rasterizer = GlyphRasterizer {
            factory: session.factory.clone(),
            faces,
        };
        Self {
            session,
            resolver,
            rasterizer,
            sequence: RefCell::new(sequence::SequencePresenter::new()),
            primary_metrics,
            primary_warning: None,
        }
    }

    /// Opens a configured family when usable, otherwise warns and falls back to
    /// the existing system monospace selection policy.
    pub(crate) fn open_primary_with_sink(
        family: Option<&str>,
        size: f32,
        lifecycle: Rc<dyn FontLifecycleSink>,
    ) -> Result<Self> {
        let (factory, fallback, locale) = open_factory_fallback_locale()?;
        let mut warning = None;
        let selected = family
            .map(|family| select_family_primary(&factory, family))
            .transpose();
        let (primary_face, descriptor) = match selected {
            Ok(Some(primary)) => primary,
            Ok(None) => select_system_primary(&factory)?,
            Err(error) => {
                tracing::warn!(family, %error, "configured font family unavailable; using system primary");
                warning = Some(format!(
                    "configured font family {} unavailable; using system primary: {error}",
                    family.unwrap_or_default()
                ));
                select_system_primary(&factory)?
            }
        };
        let primary_metrics = font_metrics_from_face(&primary_face, size)
            .context("measure DirectWrite primary face")?;
        let faces = NativeFaceRegistry::with_primary(primary_face)
            .context("register DirectWrite primary face")?;
        let mut state = Self::from_session(
            DirectWriteSession {
                factory,
                fallback,
                descriptor,
                faces: Rc::new(RefCell::new(faces)),
                locale,
                lifecycle,
            },
            primary_metrics,
        );
        state.primary_warning = warning;
        Ok(state)
    }

    /// Open the system default UI primary face (e.g. Segoe UI) at the requested size.
    pub(crate) fn open_system_ui_primary_with_sink(
        size: f32,
        lifecycle: Rc<dyn FontLifecycleSink>,
    ) -> Result<Self> {
        let (factory, fallback, locale) = open_factory_fallback_locale()?;
        let (primary_face, descriptor) = select_system_ui_primary(&factory)?;
        let primary_metrics = font_metrics_from_face(&primary_face, size)
            .context("measure DirectWrite system UI primary face")?;
        let faces = NativeFaceRegistry::with_primary(primary_face)
            .context("register DirectWrite system UI primary face")?;
        Ok(Self::from_session(
            DirectWriteSession {
                factory,
                fallback,
                descriptor,
                faces: Rc::new(RefCell::new(faces)),
                locale,
                lifecycle,
            },
            primary_metrics,
        ))
    }

    /// Select a system monospace/fixed-pitch primary face and retain it.
    #[cfg(test)]
    pub(crate) fn open_system_primary() -> Result<Self> {
        Self::open_system_primary_with_sink(Rc::new(crate::lifecycle::TracingFontLifecycleSink))
    }

    #[cfg(test)]
    pub(crate) fn open_system_primary_with_sink(
        lifecycle: Rc<dyn FontLifecycleSink>,
    ) -> Result<Self> {
        let (factory, fallback, locale) = open_factory_fallback_locale()?;
        let (primary_face, descriptor) = select_system_primary(&factory)?;
        let primary_metrics = font_metrics_from_face(&primary_face, harbor_config::FONT_SIZE)
            .context("measure DirectWrite system primary face")?;
        let faces = NativeFaceRegistry::with_primary(primary_face)
            .context("register DirectWrite system primary face")?;
        Ok(Self::from_session(
            DirectWriteSession {
                factory,
                fallback,
                descriptor,
                faces: Rc::new(RefCell::new(faces)),
                locale,
                lifecycle,
            },
            primary_metrics,
        ))
    }

    /// Open a process-private primary face from a filesystem font path.
    #[cfg(test)]
    pub(crate) fn open_configured_primary(path: &Path) -> Result<Self> {
        Self::open_configured_primary_with_sink(
            path,
            Rc::new(crate::lifecycle::TracingFontLifecycleSink),
        )
    }

    #[cfg(test)]
    pub(crate) fn open_configured_primary_with_sink(
        path: &Path,
        lifecycle: Rc<dyn FontLifecycleSink>,
    ) -> Result<Self> {
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
        Ok(Self::from_session(
            DirectWriteSession {
                factory,
                fallback,
                descriptor,
                faces: Rc::new(RefCell::new(faces)),
                locale,
                lifecycle,
            },
            primary_metrics,
        ))
    }

    pub fn font_metrics(&self, size: f32) -> FontMetrics {
        let faces = self.session.faces.borrow();
        let Some(primary) = faces.get(PRIMARY_FACE_ID) else {
            return self.primary_metrics;
        };
        font_metrics_from_face(primary, size).unwrap_or(self.primary_metrics)
    }

    pub fn resolve<S: Into<FontStyle>>(&self, ch: char, size: f32, style: S) -> GlyphResolution {
        self.resolver.resolve(ch, size, style.into())
    }
}

impl GlyphResolver {
    fn resolve(&self, ch: char, size: f32, style: FontStyle) -> GlyphResolution {
        let Some(size) = FontSize::new(size) else {
            return GlyphResolution::Unavailable;
        };
        let key = ResolutionKey::new(ch, size, style);
        if let Some(cached) = self.resolutions.borrow().get(&key).copied() {
            return cached;
        }
        let result = self.resolve_uncached(ch, size, style);
        self.resolutions.borrow_mut().insert(key, result);
        result
    }

    fn resolve_uncached(&self, ch: char, size: FontSize, style: FontStyle) -> GlyphResolution {
        let primary_glyph = match self.primary_glyph_index(ch) {
            Ok(glyph) => glyph,
            Err(_) => return GlyphResolution::Unavailable,
        };
        if primary_glyph != 0 {
            return GlyphResolution::Available(GlyphKey {
                face_id: PRIMARY_FACE_ID,
                glyph_id: GlyphId::new(u32::from(primary_glyph)),
                size,
                style,
            });
        }
        self.map_fallback(ch, size, style)
    }

    fn map_fallback(&self, ch: char, size: FontSize, style: FontStyle) -> GlyphResolution {
        #[cfg(test)]
        self.map_calls.set(self.map_calls.get() + 1);

        let source = FallbackTextSource::new(ch, &self.session.locale).into_object();
        let analysis: IDWriteTextAnalysisSource = source.to_interface();
        let text_length = ch.len_utf16() as u32;
        let mut mapped_length = 0u32;
        let mut mapped_font = None;
        let mut scale = 0.0f32;
        let family = windows::core::PCWSTR(self.session.descriptor.family_name.as_ptr());

        let map_result = unsafe {
            self.session.fallback.MapCharacters(
                &analysis,
                0,
                text_length,
                None,
                family,
                self.session.descriptor.weight,
                self.session.descriptor.style,
                self.session.descriptor.stretch,
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
        let face_id = match self.session.faces.borrow_mut().register(face) {
            Ok(id) => id,
            Err(_) => return GlyphResolution::Unavailable,
        };
        let effective_size = size.get() * scale;
        let Some(effective_size) = FontSize::new(effective_size) else {
            return GlyphResolution::Unavailable;
        };
        let key = GlyphKey::new(
            face_id,
            GlyphId::new(u32::from(glyph)),
            effective_size,
            style,
        );
        self.emit_first_fallback(ch, face_id);
        GlyphResolution::Available(key)
    }

    fn emit_first_fallback(&self, ch: char, face_id: FaceId) {
        if self.first_fallback_emitted.replace(true) {
            return;
        }
        self.session
            .lifecycle
            .emit(FontLifecycleEvent::FirstFallback {
                scalar: ch,
                face_id,
            });
    }

    fn primary_glyph_index(&self, ch: char) -> Result<u16> {
        let faces = self.session.faces.borrow();
        let face = faces
            .get(PRIMARY_FACE_ID)
            .ok_or_else(|| anyhow!("primary face missing from registry"))?;
        glyph_index_on_face(face, ch)
    }
}

impl GlyphRasterizer {
    pub fn rasterize(&self, key: GlyphKey) -> (GlyphBitmapBounds, Vec<u8>) {
        match self.rasterize_inner(key) {
            Ok(result) => result,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    glyph = key.glyph_id.get(),
                    face = key.face_id.get(),
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

    fn rasterize_inner(&self, key: GlyphKey) -> Result<(GlyphBitmapBounds, Vec<u8>)> {
        let px = key.size.get();
        let faces = self.faces.borrow();
        let face = faces
            .get(key.face_id)
            .ok_or_else(|| anyhow!("unknown face_id {}", key.face_id.get()))?;
        let glyph = u16::try_from(key.glyph_id.get())
            .map_err(|_| anyhow!("glyph index {} out of u16 range", key.glyph_id.get()))?;
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

        let analysis_result = unsafe {
            self.factory.CreateGlyphRunAnalysis(
                &glyph_run,
                None,
                DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC,
                DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_GRID_FIT_MODE_ENABLED,
                DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
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
}

impl DwriteState {
    pub fn rasterize(&self, key: GlyphKey) -> (GlyphBitmapBounds, Vec<u8>) {
        self.rasterizer.rasterize(key)
    }

    #[cfg(test)]
    pub fn map_call_count(&self) -> u32 {
        self.resolver.map_call_count()
    }

    #[cfg(test)]
    pub fn face_count(&self) -> usize {
        self.session.faces.borrow().len()
    }

    #[cfg(test)]
    pub fn primary_family_name(&self) -> String {
        string_from_wide(&self.session.descriptor.family_name)
    }
}

impl GlyphResolver {
    #[cfg(test)]
    fn map_call_count(&self) -> u32 {
        self.map_calls.get()
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

#[cfg(test)]
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
    let glyph = glyph_index_on_face(face, 'M')?;
    let advance = design_advance(face, glyph)?;
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
    FontMetrics::new(cell_width, line_height, ascent.ceil(), descent, line_gap)
        .ok_or_else(|| anyhow!("DirectWrite primary metrics contained invalid dimensions"))
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
        if !matches!(glyph_index_on_face(&face, 'M'), Ok(glyph) if glyph != 0) {
            continue;
        }
        let family_name = match localized_family_name_from_family(&family) {
            Ok(name) => name,
            Err(_) => continue,
        };
        let descriptor = PrimaryDescriptor {
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

fn select_family_primary(
    factory: &IDWriteFactory2,
    requested: &str,
) -> Result<(IDWriteFontFace, PrimaryDescriptor)> {
    select_family_primary_inner(factory, requested, true)
}

fn select_family_primary_inner(
    factory: &IDWriteFactory2,
    requested: &str,
    require_monospaced: bool,
) -> Result<(IDWriteFontFace, PrimaryDescriptor)> {
    let base: IDWriteFactory = factory.cast()?;
    let mut collection: Option<IDWriteFontCollection> = None;
    unsafe {
        base.GetSystemFontCollection(&mut collection, false)
            .context("GetSystemFontCollection")?;
    }
    let collection = collection.ok_or_else(|| anyhow!("system font collection was null"))?;
    let wide = requested
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut family_index = 0;
    let mut exists = BOOL::default();
    unsafe {
        collection
            .FindFamilyName(
                windows::core::PCWSTR(wide.as_ptr()),
                &mut family_index,
                &mut exists,
            )
            .context("FindFamilyName")?;
    }
    if !exists.as_bool() {
        bail!("font family `{requested}` was not found");
    }
    let family = unsafe { collection.GetFontFamily(family_index) }?;
    let font = unsafe {
        family.GetFirstMatchingFont(
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
        )
    }?;
    if unsafe { font.IsSymbolFont() }.as_bool() {
        bail!("font family `{requested}` is a symbol font");
    }
    let face = unsafe { font.CreateFontFace() }?;
    if require_monospaced {
        let face1: IDWriteFontFace1 = face
            .cast()
            .context("configured font does not expose IDWriteFontFace1")?;
        if !unsafe { face1.IsMonospacedFont() }.as_bool() {
            bail!("font family `{requested}` is not monospaced");
        }
    }
    let mut glyphs = [0u16; 1];
    let codepoints = [u32::from('M')];
    unsafe { face.GetGlyphIndices(codepoints.as_ptr(), 1, glyphs.as_mut_ptr()) }?;
    if glyphs[0] == 0 {
        bail!("font family `{requested}` has no Latin primary glyph");
    }
    let descriptor = PrimaryDescriptor {
        family_name: localized_family_name_from_family(&family)?,
        weight: unsafe { font.GetWeight() },
        style: unsafe { font.GetStyle() },
        stretch: unsafe { font.GetStretch() },
    };
    Ok((face, descriptor))
}

fn select_system_ui_primary(
    factory: &IDWriteFactory2,
) -> Result<(IDWriteFontFace, PrimaryDescriptor)> {
    const CANDIDATES: &[&str] = &["Segoe UI", "Segoe UI Variable Text"];
    for &candidate in CANDIDATES {
        if let Ok(primary) = select_family_primary_inner(factory, candidate, false) {
            return Ok(primary);
        }
    }
    select_system_fallback_ui_primary(factory)
}

fn select_system_fallback_ui_primary(
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
        if !matches!(glyph_index_on_face(&face, 'M'), Ok(glyph) if glyph != 0) {
            continue;
        }
        let family_name = match localized_family_name_from_family(&family) {
            Ok(name) => name,
            Err(_) => continue,
        };
        let descriptor = PrimaryDescriptor {
            family_name,
            weight: unsafe { font.GetWeight() },
            style: unsafe { font.GetStyle() },
            stretch: unsafe { font.GetStretch() },
        };
        selected = Some((face, descriptor));
        break;
    }

    drop(collection);
    selected.ok_or_else(|| anyhow!("no usable system UI face"))
}

#[cfg(test)]
fn describe_face(face: &IDWriteFontFace) -> Result<PrimaryDescriptor> {
    let face3: IDWriteFontFace3 = face
        .cast()
        .context("DirectWrite face does not support IDWriteFontFace3 metadata")?;
    let names = unsafe { face3.GetFamilyNames() }.context("GetFamilyNames")?;
    let family_name = localized_string(&names).context("read primary family name")?;
    Ok(PrimaryDescriptor {
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
mod tests;
