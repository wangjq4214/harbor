//! Platform-gated font backend types and DirectWrite session.
//!
//! On Windows, exposes the DirectWrite-backed native session (`dwrite`).
//! On non-Windows, emits a compile error — Harbor text currently requires Windows.

use crate::atlas::GlyphKey;

/// Opaque identifier for a native font face.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(crate) struct NativeFaceId(pub u64);

/// Opaque identifier for a native glyph within a face.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(crate) struct NativeGlyphId(pub u32);

/// Complete identity of one character-resolution request.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ResolutionKey {
    /// Unicode scalar to resolve.
    pub scalar: char,
    /// Quantized font size in bits (`size.to_bits()`).
    pub size_bits: u32,
    /// Style variant bits (0 = regular, …).
    pub style_bits: u8,
}

/// Deterministic outcome of resolving one [`ResolutionKey`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GlyphResolution {
    /// Character maps to a concrete face/glyph/effective-size/style key.
    Available(GlyphKey),
    /// Character has no usable mapping; may be cached to avoid remapping.
    Unavailable,
}

#[cfg(not(windows))]
compile_error!("harbor-text currently requires Windows");

#[cfg(windows)]
pub(crate) mod dwrite;

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::{GlyphResolution, ResolutionKey};
    use crate::atlas::GlyphKey;

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn should_equal_when_all_resolution_key_dimensions_match() {
        // Arrange
        let a = ResolutionKey {
            scalar: '中',
            size_bits: 0x41800000,
            style_bits: 1,
        };
        let b = ResolutionKey {
            scalar: '中',
            size_bits: 0x41800000,
            style_bits: 1,
        };

        // Act / Assert
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn should_not_equal_when_resolution_key_scalar_differs() {
        // Arrange
        let bmp = ResolutionKey {
            scalar: 'A',
            size_bits: 10,
            style_bits: 0,
        };
        let supplementary = ResolutionKey {
            scalar: '\u{1F600}',
            size_bits: 10,
            style_bits: 0,
        };

        // Act / Assert
        assert_ne!(bmp, supplementary);
    }

    #[test]
    fn should_not_equal_when_resolution_key_size_bits_differs() {
        // Arrange
        let a = ResolutionKey {
            scalar: 'A',
            size_bits: 10,
            style_bits: 0,
        };
        let b = ResolutionKey {
            scalar: 'A',
            size_bits: 20,
            style_bits: 0,
        };

        // Act / Assert
        assert_ne!(a, b);
    }

    #[test]
    fn should_not_equal_when_resolution_key_style_bits_differs() {
        // Arrange
        let a = ResolutionKey {
            scalar: 'A',
            size_bits: 10,
            style_bits: 0,
        };
        let b = ResolutionKey {
            scalar: 'A',
            size_bits: 10,
            style_bits: 1,
        };

        // Act / Assert
        assert_ne!(a, b);
    }

    #[test]
    fn should_preserve_complete_key_when_glyph_resolution_available() {
        // Arrange
        let key = GlyphKey {
            face_id: 2,
            glyph_index: 99,
            size_bits: 0x41800000,
            style_bits: 3,
        };

        // Act
        let resolution = GlyphResolution::Available(key);

        // Assert
        match resolution {
            GlyphResolution::Available(stored) => assert_eq!(stored, key),
            GlyphResolution::Unavailable => panic!("expected Available"),
        }
    }

    #[test]
    fn should_distinguish_unavailable_from_glyph_zero_available() {
        // Arrange
        let glyph_zero = GlyphResolution::Available(GlyphKey {
            face_id: 0,
            glyph_index: 0,
            size_bits: 0,
            style_bits: 0,
        });
        let unavailable = GlyphResolution::Unavailable;

        // Act / Assert
        assert_ne!(glyph_zero, unavailable);
        assert_eq!(unavailable, GlyphResolution::Unavailable);
        assert_eq!(
            hash_of(&unavailable),
            hash_of(&GlyphResolution::Unavailable)
        );
    }
}
