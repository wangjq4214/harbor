//! Shared CPU text core for Harbor — font discovery, glyph rasterization,
//! atlas data, metrics, and text-run caching.
//!
//! No wgpu, winit, or `harbor-widget` dependency.

pub mod atlas;
pub mod backend;
pub mod contracts;
pub mod font;
pub mod metrics;

#[cfg(test)]
pub(crate) static TRACING_CAPTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub use atlas::{AtlasGlyph, AtlasUv, GlyphAtlas, GlyphBitmapBounds, RasterizeResult};
pub use contracts::{
    FaceId, FontSize, FontStyle, GlyphId, GlyphKey, GlyphResolution, ResolutionKey,
};
pub use font::{FontBook, load_system_fonts};
pub use metrics::{FontMetrics, TextMetrics};
