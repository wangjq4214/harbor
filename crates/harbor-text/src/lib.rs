//! Shared CPU text core for Harbor — font discovery, glyph rasterization,
//! atlas data, metrics, and text-run caching.
//!
//! No wgpu, winit, `harbor-render`, or `harbor-widget` dependency.

pub mod atlas;
pub mod font;
pub mod metrics;

pub use atlas::{AtlasGlyph, AtlasUv, GlyphAtlas, RasterizeResult};
pub use font::{FontBook, load_system_fonts};
pub use metrics::TextMetrics;
