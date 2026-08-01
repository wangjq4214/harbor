//! Backend-neutral contracts shared by font resolution, rasterization, and atlas storage.

/// Stable identity for a font face within one font session.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FaceId(u64);

impl FaceId {
    pub const PRIMARY: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for FaceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl PartialEq<u64> for FaceId {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

/// Font-specific glyph index within a face.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GlyphId(u32);

impl GlyphId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A finite, positive font size. The bit representation is private to this module.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FontSize(u32);

impl FontSize {
    pub fn new(value: f32) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value.to_bits()))
    }

    #[cfg(test)]
    pub(crate) const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }

    #[cfg(test)]
    pub(crate) const fn bits(self) -> u32 {
        self.0
    }
}

/// Style variant used while resolving a glyph.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FontStyle(u8);

impl FontStyle {
    pub const REGULAR: Self = Self(0);

    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl From<u8> for FontStyle {
    fn from(value: u8) -> Self {
        Self::new(value)
    }
}

/// Stable identity of a rasterized glyph.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GlyphKey {
    pub face_id: FaceId,
    pub glyph_id: GlyphId,
    pub size: FontSize,
    pub style: FontStyle,
}

impl GlyphKey {
    pub const fn new(face_id: FaceId, glyph_id: GlyphId, size: FontSize, style: FontStyle) -> Self {
        Self {
            face_id,
            glyph_id,
            size,
            style,
        }
    }
}

/// Complete identity of one character-resolution request.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ResolutionKey {
    pub scalar: char,
    pub size: FontSize,
    pub style: FontStyle,
}

impl ResolutionKey {
    pub const fn new(scalar: char, size: FontSize, style: FontStyle) -> Self {
        Self {
            scalar,
            size,
            style,
        }
    }
}

/// Deterministic outcome of resolving one [`ResolutionKey`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GlyphResolution {
    Available(GlyphKey),
    Unavailable,
}
