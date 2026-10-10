//! Whole retained-unit presentation contracts. No terminal width, GPU or native handles.
use crate::{FaceId, FontSize};
use icu_properties::{
    CodePointSetData,
    props::{Emoji, EmojiPresentation, ExtendedPictographic},
};

/// Presentation intent, independent of the model-assigned cell width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PresentationIntent {
    #[default]
    Auto,
    Text,
    Emoji,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Ordinary,
    Text,
    Emoji,
}

/// Selectors take precedence over Auto; an explicit caller intent takes precedence
/// over classification but never rewrites the original source.
pub fn classify_unit(source: &str, intent: PresentationIntent) -> UnitKind {
    match intent {
        PresentationIntent::Text => return UnitKind::Text,
        PresentationIntent::Emoji => return UnitKind::Emoji,
        PresentationIntent::Auto => {}
    }
    let Some(first) = source.chars().next() else {
        return UnitKind::Ordinary;
    };
    if !CodePointSetData::new::<Emoji>().contains(first) {
        return UnitKind::Ordinary;
    }
    if source.contains('\u{fe0e}') {
        return UnitKind::Text;
    }
    if source.contains('\u{fe0f}')
        || CodePointSetData::new::<EmojiPresentation>().contains(first)
        || (source.contains('\u{200d}')
            && CodePointSetData::new::<ExtendedPictographic>().contains(first))
        || source.contains('\u{20e3}')
    {
        UnitKind::Emoji
    } else {
        UnitKind::Text
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SequenceStyle {
    pub bold: bool,
    pub italic: bool,
}

/// Limits: source <= SEQUENCE_MAX_UTF16 code units, effective raster size <=128px,
/// native bounds <=512x512px. Over-limit inputs return explicit unsupported.
/// Complete source and all raster inputs. Size is logical DIP; DPI is finite and
/// positive. The raster size is size * dpi / 96. No background is baked in.
/// Foreground is straight sRGB RGBA8 and is used ONLY for current-foreground font
/// layers (and monochrome). Fixed palette colors are never tinted by it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SequenceRequest {
    pub source: String,
    pub size: FontSize,
    pub dpi: FontSize,
    pub style: SequenceStyle,
    pub intent: PresentationIntent,
    pub foreground: [u8; 4],
    pub palette: u32,
}

impl SequenceRequest {
    pub fn new(source: impl Into<String>, size: FontSize, dpi: FontSize) -> Self {
        Self {
            source: source.into(),
            size,
            dpi,
            style: SequenceStyle::default(),
            intent: PresentationIntent::Auto,
            foreground: [255; 4],
            palette: 0,
        }
    }
    pub fn kind(&self) -> UnitKind {
        classify_unit(&self.source, self.intent)
    }
    pub fn raster_size(&self) -> f32 {
        self.size.get() * self.dpi.get() / 96.0
    }
}

/// Must be compared as a pair. A newly opened FontBook has a different session,
/// even if its primary family and local face IDs happen to match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PresentationGeneration {
    pub session: u64,
    pub revision: u64,
}

/// Observed actual run, not a scalar-resolution guess. Offsets and advances are
/// physical pixels within this unit; NEVER use them to advance terminal cells.
/// Face IDs are diagnostic, session/generation-local, not scalar atlas keys.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceRun {
    pub face: FaceId,
    pub family: String,
    pub utf16_start: u32,
    pub utf16_len: u32,
    pub clusters: Vec<u16>,
    pub glyphs: Vec<u16>,
    pub advances: Vec<f32>,
    pub offsets: Vec<[f32; 2]>,
    pub baseline: [f32; 2],
    pub em_size: f32,
    /// Advertised image/outline formats; older format probes may omit paint trees.
    /// PresentationTile::image_formats records the actual translated formats.
    pub image_formats: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompleteKind {
    Color,
    Monochrome,
}

/// Tight ink bounds relative to the layout origin (x right, y down), in physical
/// pixels. The renderer uniformly fits and clips this rectangle to assigned cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileBounds {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// Row-major tightly packed RGBA8, LINEAR light, PREMULTIPLIED alpha. Native D2D
/// readback is BGRA8 premultiplied sRGB; we unpremultiply in sRGB, decode RGB to
/// linear, then premultiply once. Upload as Rgba8Unorm (NOT sRGB), sample without
/// foreground tint, blend src=ONE dst=ONE_MINUS_SRC_ALPHA in a linear target.
/// Alpha is coverage/opacity, not gamma-encoded. Background composition belongs
/// to the renderer. Tiles already include foreground-dependent layers; changing
/// foreground requires a new request. Never multiply RGB by alpha a second time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationTile {
    pub bounds: TileBounds,
    /// Actual formats selected by DirectWrite color translation (or outline
    /// format for monochrome), not inferred from the font's COLR table version.
    pub image_formats: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    InvalidRequest,
    NotEmojiUnit,
    MissingGlyph,
    UnjoinedSequence,
    NativeCapability,
    ImageFormat,
    RasterLimit,
    NativeFailure,
}

/// Unsupported does not carry substituted text or a falsely certified tile.
/// Consumers use their existing bounded leading-pictograph fallback, or a visible
/// missing-glyph cue; retain the exact model source and assigned rectangle.
#[derive(Clone, Debug, PartialEq)]
pub enum SequenceOutcome {
    Complete {
        kind: CompleteKind,
        tile: PresentationTile,
        runs: Vec<SequenceRun>,
    },
    Unsupported(UnsupportedReason),
}

/// A tile's usable cache identity is (the complete SequenceRequest, generation,
/// resolved run faces). Results must not be shared between FontBooks on matching
/// local FaceId alone. Resource/font fallback changes require invalidation or a
/// replacement FontBook; request size/DPI/style/appearance changes key naturally.
#[derive(Clone, Debug, PartialEq)]
pub struct SequencePresentation {
    pub generation: PresentationGeneration,
    pub outcome: SequenceOutcome,
}

/// Capabilities of the actual lazily created native resources. COLR v1 is only
/// enabled when factory8/context7 and paint feature levels allow it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SequenceCapabilities {
    pub color_v0: bool,
    pub bitmap_svg: bool,
    pub colr_v1: bool,
    pub paint_feature_level: u32,
}

/// Retained backend resources only. Caller-owned Arc results can outlive eviction;
/// consumers must bound their own retained results (including atlas storage).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SequenceCacheStats {
    pub shapes: usize,
    pub negative_shapes: usize,
    pub rasters: usize,
    pub pixel_bytes: usize,
    pub shape_calls: u64,
    pub raster_calls: u64,
    pub offscreen_creations: u64,
}

pub const SEQUENCE_CACHE_ENTRIES: usize = 128;
pub const SEQUENCE_PIXEL_BUDGET: usize = 8 * 1024 * 1024;
pub const SEQUENCE_MAX_UTF16: usize = 128;
pub const SEQUENCE_TILE_SIDE: u32 = 512;

pub(crate) fn linear_premultiplied_rgba(bgra: [u8; 4]) -> [u8; 4] {
    let a = f32::from(bgra[3]) / 255.0;
    let decode = |c: u8| {
        if a == 0.0 {
            return 0;
        }
        let s = (f32::from(c) / 255.0 / a).min(1.0);
        let linear = if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        };
        (linear * a * 255.0).round() as u8
    };
    [decode(bgra[2]), decode(bgra[1]), decode(bgra[0]), bgra[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classification_is_not_a_width_heuristic() {
        for text in ["A", "中", "e\u{301}", ""] {
            assert_eq!(
                classify_unit(text, PresentationIntent::Auto),
                UnitKind::Ordinary
            );
        }
        for text in ["♥", "♥\u{fe0e}"] {
            assert_eq!(
                classify_unit(text, PresentationIntent::Auto),
                UnitKind::Text
            );
        }
        for text in ["♥️", "👩‍💻", "😀", "1\u{fe0f}\u{20e3}"] {
            assert_eq!(
                classify_unit(text, PresentationIntent::Auto),
                UnitKind::Emoji
            );
        }
        assert_eq!(
            classify_unit("😀", PresentationIntent::Text),
            UnitKind::Text
        );
    }
    #[test]
    fn native_edges_are_decoded_before_linear_composition() {
        assert_eq!(linear_premultiplied_rgba([200, 90, 70, 0]), [0; 4]);
        assert_eq!(
            linear_premultiplied_rgba([0, 0, 128, 128]),
            [128, 0, 0, 128]
        );
        let p = linear_premultiplied_rgba([64, 64, 64, 128]);
        assert!((27..=28).contains(&p[0])); // half-opacity middle sRGB, not double-alpha
        let over_white = f32::from(p[0]) / 255.0 + (1.0 - f32::from(p[3]) / 255.0);
        assert!((0.60..0.61).contains(&over_white));
        assert_eq!(
            linear_premultiplied_rgba([0, 0, 255, 255]),
            [255, 0, 0, 255]
        );
    }
}
