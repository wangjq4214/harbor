//! Platform-gated font backend types and compat adapter.
//!
//! On Windows, contains the fontdue compatibility adapter (`CompatState`).
//! On non-Windows, emits a compile error — direct platform compilation.

/// Opaque identifier for a native font face.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(crate) struct NativeFaceId(pub u64);

/// Opaque identifier for a native glyph within a face.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(crate) struct NativeGlyphId(pub u32);

#[cfg(not(windows))]
compile_error!("harbor-text currently requires Windows");

#[cfg(windows)]
pub(crate) mod compat;

#[cfg(windows)]
pub(crate) mod dwrite;
