//! Temporary fontdue compatibility adapter.
//!
//! This module wraps existing `fontdue` fonts behind the new backend-neutral
//! signatures. It will be deleted entirely in T0005 when the DirectWrite backend
//! replaces all behavior paths.

use crate::atlas::{GlyphBitmapBounds, GlyphKey};
use crate::font::LoadedFont;
use crate::metrics::FontMetrics;

/// Backend-neutral state wrapping existing fontdue font data.
///
/// Assigned sequential face IDs for multi-face font sets. Single-face
/// sets always return `face_id = 0`.
pub(crate) struct CompatState {
    fonts: Vec<LoadedFont>,
}

impl CompatState {
    pub fn new(fonts: Vec<LoadedFont>) -> Self {
        Self { fonts }
    }

    /// Resolve a character to a stable glyph identity.
    ///
    /// The first font in the set that contains the character gets assigned
    /// face_id 0 (or its sequential index). Characters absent from all fonts
    /// fall back to face 0.
    pub fn resolve(&self, ch: char, size: f32, style: u8) -> GlyphKey {
        let face_idx = self
            .fonts
            .iter()
            .position(|f| f.font.has_glyph(ch))
            .unwrap_or(0);
        let glyph_index = self.fonts[face_idx].font.lookup_glyph_index(ch) as u32;
        GlyphKey {
            face_id: face_idx as u64,
            glyph_index,
            size_bits: size.to_bits(),
            style_bits: style,
        }
    }

    /// Rasterize a glyph by its resolved key.
    pub fn rasterize(&self, key: GlyphKey, px: f32) -> (GlyphBitmapBounds, Vec<u8>) {
        let font = &self.fonts[key.face_id as usize].font;
        let (metrics, bitmap) = font.rasterize_indexed(key.glyph_index as u16, px);
        let bounds = GlyphBitmapBounds {
            width: metrics.width,
            height: metrics.height,
            bearing_x: metrics.xmin,
            bearing_y: metrics.ymin,
            advance_width: metrics.advance_width,
        };
        (bounds, bitmap)
    }

    /// Primary font metrics for terminal cell sizing.
    pub fn font_metrics(&self, size: f32) -> FontMetrics {
        let font = &self.fonts[0].font;
        let m = font.metrics('M', size);
        let lm = font.horizontal_line_metrics(size);
        FontMetrics {
            cell_width: m.advance_width.ceil(),
            line_height: lm.map_or(m.bounds.height.ceil() + 4.0, |l| l.new_line_size.ceil()),
            ascent: lm.map_or(m.bounds.height.ceil(), |l| l.ascent.ceil()),
            descent: lm.map_or(0.0, |l| l.descent.abs()),
            line_gap: lm.map_or(0.0, |l| l.line_gap),
        }
    }
}
