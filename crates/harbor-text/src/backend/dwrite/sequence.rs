//! Additive whole-unit path. Native runs/handles never enter the scalar registry.
use super::DirectWriteSession;
use crate::{FaceId, FontSize, presentation::*};
use std::{
    cell::RefCell,
    collections::VecDeque,
    ffi::c_void,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{DWRITE_E_NOCOLOR, E_FAIL, HMODULE},
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::D3D_DRIVER_TYPE_WARP,
            Direct3D11::*,
            DirectWrite::*,
            Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, IDXGIDevice},
        },
    },
    core::{BOOL, ComObjectInner, IUnknown, Interface, PCWSTR, Ref, implement},
};
use windows_numerics::Vector2;

type NativeResult<T> = windows::core::Result<T>;
type ShapeResult = Result<Rc<ShapedUnit>, UnsupportedReason>;
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShapeKey {
    source: String,
    size: FontSize,
    dpi: FontSize,
    style: SequenceStyle,
    intent: PresentationIntent,
}
impl From<&SequenceRequest> for ShapeKey {
    fn from(r: &SequenceRequest) -> Self {
        Self {
            source: r.source.clone(),
            size: r.size,
            dpi: r.dpi,
            style: r.style,
            intent: r.intent,
        }
    }
}

pub(super) struct SequencePresenter {
    generation: PresentationGeneration,
    next_face: u64,
    shapes: VecDeque<(ShapeKey, ShapeResult)>,
    rasters: VecDeque<(SequenceRequest, Arc<SequencePresentation>)>,
    offscreen: Option<Offscreen>,
    offscreen_failed: bool,
    stats: SequenceCacheStats,
}
impl SequencePresenter {
    pub(super) fn new() -> Self {
        Self {
            generation: PresentationGeneration {
                session: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
                revision: 0,
            },
            next_face: 1,
            shapes: VecDeque::new(),
            rasters: VecDeque::new(),
            offscreen: None,
            offscreen_failed: false,
            stats: SequenceCacheStats::default(),
        }
    }
    pub(super) fn generation(&self) -> PresentationGeneration {
        self.generation
    }
    pub(super) fn invalidate(&mut self) {
        self.generation.revision += 1;
        self.shapes.clear();
        self.rasters.clear();
        self.stats.pixel_bytes = 0;
        // Keep the device for ordinary invalidation, but retry a failed creation.
        self.offscreen_failed = false;
    }
    pub(super) fn stats(&self) -> SequenceCacheStats {
        SequenceCacheStats {
            shapes: self.shapes.len(),
            negative_shapes: self.shapes.iter().filter(|(_, s)| s.is_err()).count(),
            rasters: self.rasters.len(),
            ..self.stats
        }
    }
    pub(super) fn capabilities(&mut self, session: &DirectWriteSession) -> SequenceCapabilities {
        self.ensure_offscreen(session);
        self.offscreen
            .as_ref()
            .map_or(SequenceCapabilities::default(), |s| s.capabilities)
    }
    fn ensure_offscreen(&mut self, session: &DirectWriteSession) {
        if self.offscreen.is_none() && !self.offscreen_failed {
            match Offscreen::new(&session.factory) {
                Ok(o) => {
                    self.offscreen = Some(o);
                    self.stats.offscreen_creations += 1;
                }
                Err(_) => self.offscreen_failed = true,
            }
        }
    }
    pub(super) fn present(
        &mut self,
        session: &DirectWriteSession,
        request: &SequenceRequest,
    ) -> Arc<SequencePresentation> {
        let size = request.raster_size();
        // Validate before cloning keys or calling native APIs; unbounded input is
        // never retained. UTF-8 bound makes scanning hostile inputs bounded too.
        if request.source.len() > SEQUENCE_MAX_UTF16 * 4
            || request.source.is_empty()
            || request.source.encode_utf16().count() > SEQUENCE_MAX_UTF16
            || !size.is_finite()
            || size <= 0.0
            || size > 128.0
        {
            return Arc::new(SequencePresentation {
                generation: self.generation,
                outcome: SequenceOutcome::Unsupported(UnsupportedReason::InvalidRequest),
            });
        }
        if let Some(i) = self.rasters.iter().position(|(r, _)| r == request) {
            let entry = self.rasters.remove(i).expect("cache position");
            let result = Arc::clone(&entry.1);
            self.rasters.push_back(entry);
            return result;
        }
        let key = ShapeKey::from(request);
        let shaped = if let Some(i) = self.shapes.iter().position(|(k, _)| k == &key) {
            let entry = self.shapes.remove(i).expect("cache position");
            let result = entry.1.clone();
            self.shapes.push_back(entry);
            result
        } else {
            self.stats.shape_calls += 1;
            let result = shape(session, request, &mut self.next_face).map(Rc::new);
            if self.shapes.len() == SEQUENCE_CACHE_ENTRIES {
                self.shapes.pop_front();
            }
            self.shapes.push_back((key, result.clone()));
            result
        };
        let outcome = match shaped {
            Err(reason) => SequenceOutcome::Unsupported(reason),
            Ok(shaped) => {
                self.ensure_offscreen(session);
                match self.offscreen.as_ref() {
                    None => SequenceOutcome::Unsupported(UnsupportedReason::NativeCapability),
                    Some(o) => {
                        self.stats.raster_calls += 1;
                        match o.raster(&shaped, request) {
                            Ok((kind, tile)) => SequenceOutcome::Complete {
                                kind,
                                tile,
                                runs: shaped.runs.iter().map(|r| r.info.clone()).collect(),
                            },
                            Err(reason) => SequenceOutcome::Unsupported(reason),
                        }
                    }
                }
            }
        };
        let result = Arc::new(SequencePresentation {
            generation: self.generation,
            outcome,
        });
        let bytes = pixel_bytes(&result);
        while !self.rasters.is_empty()
            && (self.rasters.len() >= SEQUENCE_CACHE_ENTRIES
                || self.stats.pixel_bytes + bytes > SEQUENCE_PIXEL_BUDGET)
        {
            let (_, old) = self.rasters.pop_front().expect("nonempty cache");
            self.stats.pixel_bytes -= pixel_bytes(&old);
        }
        self.stats.pixel_bytes += bytes;
        self.rasters
            .push_back((request.clone(), Arc::clone(&result)));
        result
    }
}
fn pixel_bytes(p: &SequencePresentation) -> usize {
    match &p.outcome {
        SequenceOutcome::Complete { tile, .. } => tile.rgba.len(),
        _ => 0,
    }
}
struct ShapedUnit {
    runs: Vec<OwnedRun>,
}
struct OwnedRun {
    face: IDWriteFontFace,
    info: SequenceRun,
    offsets: Vec<DWRITE_GLYPH_OFFSET>,
    sideways: BOOL,
    bidi: u32,
}
impl OwnedRun {
    fn native(&self) -> DWRITE_GLYPH_RUN {
        DWRITE_GLYPH_RUN {
            fontFace: std::mem::ManuallyDrop::new(Some(self.face.clone())),
            fontEmSize: self.info.em_size,
            glyphCount: self.info.glyphs.len() as u32,
            glyphIndices: self.info.glyphs.as_ptr(),
            glyphAdvances: self.info.advances.as_ptr(),
            glyphOffsets: self.offsets.as_ptr(),
            isSideways: self.sideways,
            bidiLevel: self.bidi,
        }
    }
}
// DWRITE_GLYPH_RUN's generated ManuallyDrop field must be released explicitly.
struct RunGuard(DWRITE_GLYPH_RUN);
impl Drop for RunGuard {
    fn drop(&mut self) {
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.0.fontFace);
        }
    }
}

fn shape(
    session: &DirectWriteSession,
    r: &SequenceRequest,
    next_face: &mut u64,
) -> ShapeResultOwned {
    use icu_properties::{CodePointSetData, props::Emoji};
    let first = r
        .source
        .chars()
        .next()
        .ok_or(UnsupportedReason::InvalidRequest)?;
    if !CodePointSetData::new::<Emoji>().contains(first) {
        return Err(UnsupportedReason::NotEmojiUnit);
    }
    let layout = make_layout(session, r).map_err(|_| UnsupportedReason::NativeFailure)?;
    let captured = capture(&layout).map_err(|_| UnsupportedReason::NativeFailure)?;
    let mut runs = captured;
    if runs.is_empty() || runs.iter().any(|run| run.info.glyphs.contains(&0)) {
        return Err(UnsupportedReason::MissingGlyph);
    }
    let len = r.source.encode_utf16().count() as u32;
    let mut covered = vec![false; len as usize];
    for run in &runs {
        let info = &run.info;
        if info.utf16_start + info.utf16_len > len
            || info.clusters.len() != info.utf16_len as usize
            || info
                .clusters
                .iter()
                .any(|&c| usize::from(c) >= info.glyphs.len())
        {
            return Err(UnsupportedReason::UnjoinedSequence);
        }
        for position in info.utf16_start..info.utf16_start + info.utf16_len {
            if covered[position as usize] {
                return Err(UnsupportedReason::UnjoinedSequence);
            }
            covered[position as usize] = true;
        }
    }
    if covered.contains(&false) {
        return Err(UnsupportedReason::UnjoinedSequence);
    }
    // An atomic retained emoji must occupy one shaping cluster, including every
    // UTF-16 code unit. This deliberately declines decomposed representations.
    if runs.len() != 1
        || runs[0]
            .info
            .clusters
            .iter()
            .any(|&c| c != runs[0].info.clusters[0])
    {
        return Err(UnsupportedReason::UnjoinedSequence);
    }
    // For a ZWJ candidate also require a substitution, not simply ignored joiners
    // or a single missing/leading glyph. Shape a control with joiners removed.
    // This is structural evidence; semantic support claims still need fixtures.
    if r.source.contains('\u{200d}') {
        use icu_properties::props::ExtendedPictographic;
        if r.source.split('\u{200d}').any(|segment| {
            !segment
                .chars()
                .next()
                .is_some_and(|ch| CodePointSetData::new::<ExtendedPictographic>().contains(ch))
        }) {
            return Err(UnsupportedReason::UnjoinedSequence);
        }
        let mut control = r.clone();
        control.source = r.source.replace('\u{200d}', "");
        let control_layout =
            make_layout(session, &control).map_err(|_| UnsupportedReason::NativeFailure)?;
        let control_runs =
            capture(&control_layout).map_err(|_| UnsupportedReason::NativeFailure)?;
        let joined: Vec<_> = runs
            .iter()
            .flat_map(|r| r.info.glyphs.iter().copied())
            .collect();
        let unjoined: Vec<_> = control_runs
            .iter()
            .flat_map(|r| r.info.glyphs.iter().copied())
            .collect();
        if joined == unjoined {
            return Err(UnsupportedReason::UnjoinedSequence);
        }
    }
    for run in &mut runs {
        run.info.face = FaceId::new(*next_face);
        *next_face += 1;
    }
    Ok(ShapedUnit { runs })
}
type ShapeResultOwned = Result<ShapedUnit, UnsupportedReason>;

fn make_layout(
    session: &DirectWriteSession,
    r: &SequenceRequest,
) -> NativeResult<IDWriteTextLayout> {
    let base: IDWriteFactory = session.factory.cast()?;
    let emoji: Vec<u16> = "Segoe UI Emoji\0".encode_utf16().collect();
    let family = if r.kind() == UnitKind::Emoji {
        &emoji
    } else {
        &session.descriptor.family_name
    };
    unsafe {
        let format = base.CreateTextFormat(
            PCWSTR(family.as_ptr()),
            None,
            if r.style.bold {
                DWRITE_FONT_WEIGHT_BOLD
            } else {
                session.descriptor.weight
            },
            if r.style.italic {
                DWRITE_FONT_STYLE_ITALIC
            } else {
                DWRITE_FONT_STYLE_NORMAL
            },
            session.descriptor.stretch,
            r.raster_size(),
            PCWSTR(session.locale.as_ptr()),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let source: Vec<u16> = r.source.encode_utf16().collect();
        let layout = base.CreateTextLayout(&source, &format, 16384.0, 16384.0)?;
        let layout2: IDWriteTextLayout2 = layout.cast()?;
        layout2.SetFontFallback(&session.fallback)?;
        Ok(layout)
    }
}
fn capture(layout: &IDWriteTextLayout) -> NativeResult<Vec<OwnedRun>> {
    let object = Capture {
        runs: RefCell::new(Vec::new()),
    }
    .into_object();
    let renderer: IDWriteTextRenderer = object.to_interface();
    unsafe {
        layout.Draw(None, &renderer, 0.0, 0.0)?;
    }
    Ok(object.runs.borrow_mut().drain(..).collect())
}

#[implement(IDWriteTextRenderer)]
struct Capture {
    runs: RefCell<Vec<OwnedRun>>,
}
impl IDWritePixelSnapping_Impl for Capture_Impl {
    fn IsPixelSnappingDisabled(&self, _: *const c_void) -> NativeResult<BOOL> {
        Ok(true.into())
    }
    fn GetCurrentTransform(
        &self,
        _: *const c_void,
        transform: *mut DWRITE_MATRIX,
    ) -> NativeResult<()> {
        unsafe {
            *transform = DWRITE_MATRIX {
                m11: 1.0,
                m22: 1.0,
                ..Default::default()
            };
        }
        Ok(())
    }
    fn GetPixelsPerDip(&self, _: *const c_void) -> NativeResult<f32> {
        Ok(1.0)
    }
}
impl IDWriteTextRenderer_Impl for Capture_Impl {
    fn DrawGlyphRun(
        &self,
        _: *const c_void,
        x: f32,
        y: f32,
        _: DWRITE_MEASURING_MODE,
        run: *const DWRITE_GLYPH_RUN,
        description: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _: Ref<IUnknown>,
    ) -> NativeResult<()> {
        if run.is_null() || description.is_null() {
            return Err(E_FAIL.into());
        }
        // These pointers are borrowed only during the synchronous Draw callback.
        // Clone the face and copy each buffer before returning to DirectWrite.
        let (run, d) = unsafe { (&*run, &*description) };
        let face = run
            .fontFace
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(E_FAIL))?
            .clone();
        if run.glyphCount == 0
            || run.glyphCount > 3 * SEQUENCE_MAX_UTF16 as u32
            || d.stringLength == 0
            || d.stringLength > SEQUENCE_MAX_UTF16 as u32
            || run.glyphIndices.is_null()
            || run.glyphAdvances.is_null()
            || d.clusterMap.is_null()
        {
            return Err(E_FAIL.into());
        }
        let glyphs =
            unsafe { std::slice::from_raw_parts(run.glyphIndices, run.glyphCount as usize) }
                .to_vec();
        let advances =
            unsafe { std::slice::from_raw_parts(run.glyphAdvances, run.glyphCount as usize) }
                .to_vec();
        // DirectWrite's simple-run fast path legitimately omits zero offsets.
        let offsets = if run.glyphOffsets.is_null() {
            vec![DWRITE_GLYPH_OFFSET::default(); run.glyphCount as usize]
        } else {
            unsafe { std::slice::from_raw_parts(run.glyphOffsets, run.glyphCount as usize) }
                .to_vec()
        };
        let clusters =
            unsafe { std::slice::from_raw_parts(d.clusterMap, d.stringLength as usize) }.to_vec();
        let family = face
            .cast::<IDWriteFontFace3>()
            .and_then(|f| unsafe { f.GetFamilyNames() })
            .and_then(|s| {
                let len = unsafe { s.GetStringLength(0)? };
                let mut text = vec![0; len as usize + 1];
                unsafe {
                    s.GetString(0, &mut text)?;
                }
                Ok(String::from_utf16_lossy(&text[..len as usize]))
            })
            .unwrap_or_default();
        let mut image_formats = 0;
        if let Ok(f) = face.cast::<IDWriteFontFace4>() {
            for &glyph in &glyphs {
                image_formats |= unsafe {
                    f.GetGlyphImageFormats(
                        glyph,
                        run.fontEmSize.ceil() as u32,
                        run.fontEmSize.ceil() as u32,
                    )?
                }
                .0 as u32;
            }
            // The per-glyph API reports bitmap/SVG data, not outline formats.
            // Whole-face flags advertise outline availability; actual nonempty
            // complete monochrome rasterization is still checked by crop.
            image_formats |= unsafe { f.GetGlyphImageFormats2() }.0 as u32
                & (DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF).0 as u32;
        } else {
            image_formats = DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE.0 as u32;
        }
        self.runs.borrow_mut().push(OwnedRun {
            face,
            info: SequenceRun {
                face: FaceId::PRIMARY,
                family,
                utf16_start: d.textPosition,
                utf16_len: d.stringLength,
                clusters,
                glyphs,
                advances,
                offsets: offsets
                    .iter()
                    .map(|o| [o.advanceOffset, o.ascenderOffset])
                    .collect(),
                baseline: [x, y],
                em_size: run.fontEmSize,
                image_formats,
            },
            offsets,
            sideways: run.isSideways,
            bidi: run.bidiLevel,
        });
        Ok(())
    }
    fn DrawUnderline(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_UNDERLINE,
        _: Ref<IUnknown>,
    ) -> NativeResult<()> {
        Err(E_FAIL.into())
    }
    fn DrawStrikethrough(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_STRIKETHROUGH,
        _: Ref<IUnknown>,
    ) -> NativeResult<()> {
        Err(E_FAIL.into())
    }
    fn DrawInlineObject(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: Ref<IDWriteInlineObject>,
        _: BOOL,
        _: BOOL,
        _: Ref<IUnknown>,
    ) -> NativeResult<()> {
        Err(E_FAIL.into())
    }
}

struct Offscreen {
    context: ID2D1DeviceContext,
    context4: Option<ID2D1DeviceContext4>,
    context7: Option<ID2D1DeviceContext7>,
    factory: IDWriteFactory2,
    target: ID2D1Bitmap1,
    readback: ID2D1Bitmap1,
    capabilities: SequenceCapabilities,
}
impl Offscreen {
    unsafe fn layer_brush(
        &self,
        color: DWRITE_COLOR_F,
        index: u16,
        foreground: &ID2D1SolidColorBrush,
    ) -> NativeResult<ID2D1SolidColorBrush> {
        if index == u16::MAX {
            Ok(foreground.clone())
        } else {
            unsafe {
                self.context
                    .CreateSolidColorBrush(&native_color(color), None)
            }
        }
    }
    fn new(factory: &IDWriteFactory2) -> NativeResult<Self> {
        unsafe {
            let mut device = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
            let dxgi: IDXGIDevice = device
                .ok_or_else(|| windows::core::Error::from(E_FAIL))?
                .cast()?;
            let d2d = D2D1CreateDevice(&dxgi, None)?;
            d2d.SetMaximumTextureMemory(SEQUENCE_PIXEL_BUDGET as u64);
            let context = d2d.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            context.SetDpi(96.0, 96.0);
            context.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let props = |options| D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: options,
                ..Default::default()
            };
            let size = D2D_SIZE_U {
                width: SEQUENCE_TILE_SIDE,
                height: SEQUENCE_TILE_SIDE,
            };
            let target = context.CreateBitmap(size, None, 0, &props(D2D1_BITMAP_OPTIONS_TARGET))?;
            let readback = context.CreateBitmap(
                size,
                None,
                0,
                &props(D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW),
            )?;
            context.SetTarget(&target);
            let context4 = context.cast::<ID2D1DeviceContext4>().ok();
            let context7 = context.cast::<ID2D1DeviceContext7>().ok();
            let level = context7
                .as_ref()
                .map_or(0, |c| c.GetPaintFeatureLevel().0 as u32);
            let capabilities = SequenceCapabilities {
                color_v0: true,
                bitmap_svg: context4.is_some() && factory.cast::<IDWriteFactory4>().is_ok(),
                colr_v1: context7.is_some()
                    && factory.cast::<IDWriteFactory8>().is_ok()
                    && level > 0,
                paint_feature_level: level,
            };
            Ok(Self {
                context,
                context4,
                context7,
                factory: factory.clone(),
                target,
                readback,
                capabilities,
            })
        }
    }
    fn raster(
        &self,
        shaped: &ShapedUnit,
        request: &SequenceRequest,
    ) -> Result<(CompleteKind, PresentationTile), UnsupportedReason> {
        let color = request.kind() == UnitKind::Emoji;
        match self.raster_pass(shaped, request, color) {
            Ok(tile) => Ok(tile),
            Err(_) if color && shaped.runs.iter().all(has_outline) => {
                self.raster_pass(shaped, request, false)
            }
            Err(reason) => Err(reason),
        }
    }
    fn raster_pass(
        &self,
        shaped: &ShapedUnit,
        request: &SequenceRequest,
        color: bool,
    ) -> Result<(CompleteKind, PresentationTile), UnsupportedReason> {
        unsafe {
            let brush = self
                .context
                .CreateSolidColorBrush(&rgba_color(request.foreground), None)
                .map_err(|_| UnsupportedReason::NativeFailure)?;
            let commands = self
                .context
                .CreateCommandList()
                .map_err(|_| UnsupportedReason::NativeFailure)?;
            self.context.SetTarget(&commands);
            self.context.BeginDraw();
            let mut any_color = false;
            let mut formats = 0;
            let result = shaped.runs.iter().try_for_each(|run| {
                let glyph_run = RunGuard(run.native());
                let origin = Vector2 {
                    X: run.info.baseline[0],
                    Y: run.info.baseline[1],
                };
                if color
                    && let Some(selected) =
                        self.draw_color(origin, &glyph_run.0, &brush, request.palette)?
                {
                    any_color = true;
                    formats |= selected;
                    return Ok(());
                }
                if !has_outline(run) {
                    return Err(UnsupportedReason::ImageFormat);
                }
                formats |= run.info.image_formats
                    & (DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF).0
                        as u32;
                self.context.DrawGlyphRun(
                    origin,
                    &glyph_run.0,
                    None,
                    &brush,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                Ok(())
            });
            // Always balance BeginDraw. A failed color pass is cleared before a
            // complete monochrome retry; never overlay it on partial color layers.
            let end = self
                .context
                .EndDraw(None, None)
                .map_err(|_| UnsupportedReason::NativeFailure);
            self.context.SetTarget(&self.target);
            result?;
            end?;
            commands
                .Close()
                .map_err(|_| UnsupportedReason::NativeFailure)?;
            // D2D computes bounds for the actual translated color operations,
            // including paint trees and negative bearings. Do not infer color
            // bounds from the base glyph's monochrome outline or cell width.
            let bounds = self
                .context
                .GetImageLocalBounds(&commands)
                .map_err(|_| UnsupportedReason::NativeFailure)?;
            let left = bounds.left.floor() - 1.0;
            let top = bounds.top.floor() - 1.0;
            let width = bounds.right.ceil() + 1.0 - left;
            let height = bounds.bottom.ceil() + 1.0 - top;
            if ![left, top, width, height].iter().all(|v| v.is_finite())
                || width <= 0.0
                || height <= 0.0
                || width > SEQUENCE_TILE_SIDE as f32
                || height > SEQUENCE_TILE_SIDE as f32
            {
                return Err(UnsupportedReason::RasterLimit);
            }
            self.context.BeginDraw();
            self.context.Clear(Some(&D2D1_COLOR_F::default()));
            self.context.SetTransform(&windows_numerics::Matrix3x2 {
                M11: 1.0,
                M22: 1.0,
                M31: -left,
                M32: -top,
                ..Default::default()
            });
            self.context.DrawImage(
                &commands,
                None,
                None,
                D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
            );
            let end = self
                .context
                .EndDraw(None, None)
                .map_err(|_| UnsupportedReason::NativeFailure);
            self.context.SetTransform(&windows_numerics::Matrix3x2 {
                M11: 1.0,
                M22: 1.0,
                ..Default::default()
            });
            end?;
            self.readback
                .CopyFromBitmap(None, &self.target, None)
                .map_err(|_| UnsupportedReason::NativeFailure)?;
            let mapped = self
                .readback
                .Map(D2D1_MAP_OPTIONS_READ)
                .map_err(|_| UnsupportedReason::NativeFailure)?;
            let tile = crop(
                mapped,
                formats,
                [left as i32, top as i32],
                request.foreground[3] == 0,
            );
            self.readback
                .Unmap()
                .map_err(|_| UnsupportedReason::NativeFailure)?;
            tile.map(|tile| {
                (
                    if any_color {
                        CompleteKind::Color
                    } else {
                        CompleteKind::Monochrome
                    },
                    tile,
                )
            })
        }
    }
    unsafe fn draw_color(
        &self,
        origin: Vector2,
        run: &DWRITE_GLYPH_RUN,
        brush: &ID2D1SolidColorBrush,
        palette: u32,
    ) -> Result<Option<u32>, UnsupportedReason> {
        unsafe {
            // Translate with exactly the APIs/formats the actual context can draw.
            // Paint-tree translation checks the context's feature level too.
            let enumerator = if self.capabilities.colr_v1 {
                self.factory
                    .cast::<IDWriteFactory8>()
                    .map_err(|_| UnsupportedReason::NativeCapability)?
                    .TranslateColorGlyphRun(
                        origin,
                        run,
                        None,
                        all_formats(),
                        DWRITE_PAINT_FEATURE_LEVEL(self.capabilities.paint_feature_level as i32),
                        DWRITE_MEASURING_MODE_NATURAL,
                        None,
                        palette,
                    )
            } else if self.capabilities.bitmap_svg {
                self.factory
                    .cast::<IDWriteFactory4>()
                    .map_err(|_| UnsupportedReason::NativeCapability)?
                    .TranslateColorGlyphRun(
                        origin,
                        run,
                        None,
                        DWRITE_GLYPH_IMAGE_FORMATS(
                            all_formats().0 & !DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE.0,
                        ),
                        DWRITE_MEASURING_MODE_NATURAL,
                        None,
                        palette,
                    )
            } else {
                return self.draw_color_v0(origin, run, brush, palette);
            };
            let layers = match enumerator {
                Ok(layers) => layers,
                Err(error) if error.code() == DWRITE_E_NOCOLOR => return Ok(None),
                Err(_) => return Err(UnsupportedReason::ImageFormat),
            };
            let mut formats = 0;
            while layers
                .MoveNext()
                .map_err(|_| UnsupportedReason::NativeFailure)?
                .as_bool()
            {
                let layer = &*layers
                    .GetCurrentRun()
                    .map_err(|_| UnsupportedReason::NativeFailure)?;
                let base = &layer.Base;
                let p = Vector2 {
                    X: base.baselineOriginX,
                    Y: base.baselineOriginY,
                };
                let format = layer.glyphImageFormat;
                if format == DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE {
                    let c = self
                        .context7
                        .as_ref()
                        .ok_or(UnsupportedReason::NativeCapability)?;
                    c.DrawPaintGlyphRun(
                        p,
                        &base.glyphRun,
                        brush,
                        palette,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    formats |= format.0 as u32;
                } else if format == DWRITE_GLYPH_IMAGE_FORMATS_SVG {
                    self.context4
                        .as_ref()
                        .ok_or(UnsupportedReason::NativeCapability)?
                        .DrawSvgGlyphRun(
                            p,
                            &base.glyphRun,
                            brush,
                            None,
                            palette,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    formats |= format.0 as u32;
                } else if (format.0 & bitmap_formats().0) != 0 {
                    self.context4
                        .as_ref()
                        .ok_or(UnsupportedReason::NativeCapability)?
                        .DrawColorBitmapGlyphRun(
                            format,
                            p,
                            &base.glyphRun,
                            DWRITE_MEASURING_MODE_NATURAL,
                            D2D1_COLOR_BITMAP_GLYPH_SNAP_OPTION_DISABLE,
                        );
                    formats |= format.0 as u32;
                } else if (format.0
                    & (DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE
                        | DWRITE_GLYPH_IMAGE_FORMATS_CFF
                        | DWRITE_GLYPH_IMAGE_FORMATS_COLR)
                        .0)
                    != 0
                {
                    let layer_brush = self
                        .layer_brush(base.runColor, base.paletteIndex, brush)
                        .map_err(|_| UnsupportedReason::NativeFailure)?;
                    self.context.DrawGlyphRun(
                        p,
                        &base.glyphRun,
                        None,
                        &layer_brush,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    formats |= format.0 as u32;
                } else {
                    return Err(UnsupportedReason::ImageFormat);
                }
            }
            if formats == 0 {
                Err(UnsupportedReason::ImageFormat)
            } else {
                Ok(Some(formats))
            }
        }
    }
    unsafe fn draw_color_v0(
        &self,
        origin: Vector2,
        run: &DWRITE_GLYPH_RUN,
        brush: &ID2D1SolidColorBrush,
        palette: u32,
    ) -> Result<Option<u32>, UnsupportedReason> {
        unsafe {
            let layers = match self.factory.TranslateColorGlyphRun(
                origin.X,
                origin.Y,
                run,
                None,
                DWRITE_MEASURING_MODE_NATURAL,
                None,
                palette,
            ) {
                Ok(l) => l,
                Err(e) if e.code() == DWRITE_E_NOCOLOR => return Ok(None),
                Err(_) => return Err(UnsupportedReason::ImageFormat),
            };
            let mut formats = 0;
            while layers
                .MoveNext()
                .map_err(|_| UnsupportedReason::NativeFailure)?
                .as_bool()
            {
                let layer = &*layers
                    .GetCurrentRun()
                    .map_err(|_| UnsupportedReason::NativeFailure)?;
                let b = self
                    .layer_brush(layer.runColor, layer.paletteIndex, brush)
                    .map_err(|_| UnsupportedReason::NativeFailure)?;
                self.context.DrawGlyphRun(
                    Vector2 {
                        X: layer.baselineOriginX,
                        Y: layer.baselineOriginY,
                    },
                    &layer.glyphRun,
                    None,
                    &b,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                formats |= DWRITE_GLYPH_IMAGE_FORMATS_COLR.0 as u32;
            }
            if formats == 0 {
                Err(UnsupportedReason::ImageFormat)
            } else {
                Ok(Some(formats))
            }
        }
    }
}
fn has_outline(run: &OwnedRun) -> bool {
    run.info.image_formats
        & (DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF).0 as u32
        != 0
}
fn bitmap_formats() -> DWRITE_GLYPH_IMAGE_FORMATS {
    DWRITE_GLYPH_IMAGE_FORMATS_PNG
        | DWRITE_GLYPH_IMAGE_FORMATS_JPEG
        | DWRITE_GLYPH_IMAGE_FORMATS_TIFF
        | DWRITE_GLYPH_IMAGE_FORMATS_PREMULTIPLIED_B8G8R8A8
}
fn all_formats() -> DWRITE_GLYPH_IMAGE_FORMATS {
    bitmap_formats()
        | DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE
        | DWRITE_GLYPH_IMAGE_FORMATS_CFF
        | DWRITE_GLYPH_IMAGE_FORMATS_COLR
        | DWRITE_GLYPH_IMAGE_FORMATS_SVG
        | DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE
}
fn native_color(c: DWRITE_COLOR_F) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}
fn rgba_color(rgba: [u8; 4]) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(rgba[0]) / 255.0,
        g: f32::from(rgba[1]) / 255.0,
        b: f32::from(rgba[2]) / 255.0,
        a: f32::from(rgba[3]) / 255.0,
    }
}
unsafe fn crop(
    mapped: D2D1_MAPPED_RECT,
    image_formats: u32,
    origin: [i32; 2],
    allow_transparent: bool,
) -> Result<PresentationTile, UnsupportedReason> {
    let side = SEQUENCE_TILE_SIDE as usize;
    let mut left = side;
    let mut top = side;
    let mut right = 0;
    let mut bottom = 0;
    for y in 0..side {
        let row = unsafe {
            std::slice::from_raw_parts(mapped.bits.add(y * mapped.pitch as usize), side * 4)
        };
        for x in 0..side {
            if row[x * 4 + 3] != 0 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
    }
    if left == side {
        return if allow_transparent {
            Ok(PresentationTile {
                bounds: TileBounds {
                    left: origin[0],
                    top: origin[1],
                    width: 1,
                    height: 1,
                },
                image_formats,
                rgba: vec![0; 4],
            })
        } else {
            Err(UnsupportedReason::ImageFormat)
        };
    }
    if left == 0 || top == 0 || right == side || bottom == side {
        return Err(UnsupportedReason::RasterLimit);
    }
    let mut rgba = Vec::with_capacity((right - left) * (bottom - top) * 4);
    for y in top..bottom {
        let row = unsafe {
            std::slice::from_raw_parts(mapped.bits.add(y * mapped.pitch as usize), side * 4)
        };
        for x in left..right {
            rgba.extend_from_slice(&linear_premultiplied_rgba(
                row[x * 4..x * 4 + 4].try_into().expect("pixel"),
            ));
        }
    }
    Ok(PresentationTile {
        bounds: TileBounds {
            left: left as i32 + origin[0],
            top: top as i32 + origin[1],
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        },
        image_formats,
        rgba,
    })
}

#[cfg(test)]
mod tests;
