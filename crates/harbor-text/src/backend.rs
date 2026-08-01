//! Platform-gated font backend types and DirectWrite session.
//!
//! Backend-neutral resolution contracts live in [`crate::contracts`]. This
//! module only owns platform selection and the Windows DirectWrite backend.

pub use crate::contracts::{GlyphKey, GlyphResolution, ResolutionKey};

#[cfg(not(windows))]
compile_error!("harbor-text currently requires Windows");

#[cfg(windows)]
pub(crate) mod dwrite;

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::{GlyphKey, GlyphResolution, ResolutionKey};
    use crate::contracts::{FaceId, FontSize, FontStyle, GlyphId};

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn size(bits: u32) -> FontSize {
        FontSize::from_bits(bits)
    }

    #[test]
    fn should_equal_when_all_resolution_key_dimensions_match() {
        let a = ResolutionKey::new('中', size(0x41800000), FontStyle::new(1));
        let b = ResolutionKey::new('中', size(0x41800000), FontStyle::new(1));
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn should_not_equal_when_resolution_key_scalar_differs() {
        let bmp = ResolutionKey::new('A', size(10), FontStyle::REGULAR);
        let supplementary = ResolutionKey::new('\u{1F600}', size(10), FontStyle::REGULAR);
        assert_ne!(bmp, supplementary);
    }

    #[test]
    fn should_not_equal_when_resolution_key_size_bits_differs() {
        let a = ResolutionKey::new('A', size(10), FontStyle::REGULAR);
        let b = ResolutionKey::new('A', size(20), FontStyle::REGULAR);
        assert_ne!(a, b);
    }

    #[test]
    fn should_not_equal_when_resolution_key_style_bits_differs() {
        let a = ResolutionKey::new('A', size(10), FontStyle::new(0));
        let b = ResolutionKey::new('A', size(10), FontStyle::new(1));
        assert_ne!(a, b);
    }

    #[test]
    fn should_preserve_complete_key_when_glyph_resolution_available() {
        let key = GlyphKey::new(
            FaceId::new(2),
            GlyphId::new(99),
            size(0x41800000),
            FontStyle::new(3),
        );
        let resolution = GlyphResolution::Available(key);
        match resolution {
            GlyphResolution::Available(stored) => assert_eq!(stored, key),
            GlyphResolution::Unavailable => panic!("expected Available"),
        }
    }

    #[test]
    fn should_distinguish_unavailable_from_glyph_zero_available() {
        let glyph_zero = GlyphResolution::Available(GlyphKey::new(
            FaceId::PRIMARY,
            GlyphId::new(0),
            size(0x3f800000),
            FontStyle::REGULAR,
        ));
        let unavailable = GlyphResolution::Unavailable;
        assert_ne!(glyph_zero, unavailable);
        assert_eq!(unavailable, GlyphResolution::Unavailable);
        assert_eq!(
            hash_of(&unavailable),
            hash_of(&GlyphResolution::Unavailable)
        );
    }
}
