//! Shared CPU text core for Harbor — font discovery, glyph rasterization,
//! atlas data, metrics, and text-run caching.
//!
//! No wgpu, winit, or `harbor-widget` dependency.

pub mod atlas;
pub mod backend;
pub mod font;
pub mod metrics;

pub use atlas::{AtlasGlyph, AtlasUv, GlyphAtlas, GlyphBitmapBounds, GlyphKey, RasterizeResult};
pub use backend::{GlyphResolution, ResolutionKey};
pub use font::{FontBook, load_system_fonts};
pub use metrics::{FontMetrics, TextMetrics};
