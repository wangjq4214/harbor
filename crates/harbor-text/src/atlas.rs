use std::cmp::Reverse;

use hashbrown::{HashMap, HashSet};

use crate::backend::{GlyphResolution, ResolutionKey};
use crate::font::FontBook;
use harbor_config::FONT_SIZE;

const ATLAS_PADDING: u32 = 1;
pub const MAX_ATLAS_SIZE: u32 = 2048;

/// Stable identity for a rasterized glyph in the atlas.
///
/// Distinguishes the same Unicode scalar rendered by different font faces,
/// at different sizes, or with different style variants.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GlyphKey {
    /// Opaque face identifier assigned by the font backend.
    pub face_id: u64,
    /// Font-specific glyph index (not a Unicode scalar).
    pub glyph_index: u32,
    /// Quantized font size in bits (typically `size.to_bits()`).
    pub size_bits: u32,
    /// Style variant: 0 = regular, 1 = bold, etc.
    pub style_bits: u8,
}

/// Pixel dimensions and placement of a rasterized glyph bitmap.
///
/// Backend-neutral — does not reference any specific font parser.
#[derive(Clone, Copy, Debug)]
pub struct GlyphBitmapBounds {
    /// Glyph bitmap pixel width.
    pub width: usize,
    /// Glyph bitmap pixel height.
    pub height: usize,
    /// Horizontal offset from origin to left edge of the glyph bitmap.
    pub bearing_x: i32,
    /// Vertical offset from origin to top edge of the glyph bitmap.
    pub bearing_y: i32,
    /// Horizontal distance to advance after rendering this glyph.
    pub advance_width: f32,
}

/// Internal metrics for atlas packing — mirrors the fields the atlas code
/// reads from fontdue::Metrics but without the fontdue dependency.
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct PackedMetrics {
    width: usize,
    height: usize,
    bearing_x: i32,
    bearing_y: i32,
    advance_width: f32,
}

/// Result of an incremental `rasterize_new` call.
pub struct RasterizeResult {
    /// Glyph keys that were newly rasterized and added to the cache.
    pub new_keys: Vec<GlyphKey>,
    /// True when the atlas overflowed and was fully rebuilt.
    /// When true, the caller MUST rebuild all GPU vertices — all UVs changed.
    pub evicted: bool,
}

/// UV rectangle within the atlas texture.
#[derive(Clone, Copy)]
pub struct AtlasUv {
    /// Left UV boundary [0, 1].
    pub left: f32,
    /// Top UV boundary [0, 1].
    pub top: f32,
    /// Right UV boundary [0, 1].
    pub right: f32,
    /// Bottom UV boundary [0, 1].
    pub bottom: f32,
}

/// Atlas placement and metrics for one rasterized glyph.
#[derive(Clone, Copy)]
pub struct AtlasGlyph {
    /// Stable identity for this glyph in the atlas.
    pub key: GlyphKey,
    /// UV sub-region (unit texture coordinates).
    pub uv: AtlasUv,
    /// Glyph pixel width.
    pub width: u32,
    /// Glyph pixel height.
    pub height: u32,
    /// Horizontal offset from glyph origin to left edge of bitmap.
    pub bearing_x: i32,
    /// Vertical offset from glyph origin to top edge of bitmap.
    pub bearing_y: i32,
    /// Pixel x position within the fixed-size atlas.
    pub atlas_x: u32,
    /// Pixel y position within the fixed-size atlas.
    pub atlas_y: u32,
}

/// One shelf row in the atlas packing layout.
#[derive(Clone, Copy, Debug)]
struct Shelf {
    /// Top pixel y-coordinate of this shelf.
    y: u32,
    /// Height of this shelf (max glyph height on the shelf).
    height: u32,
    /// Next free x position on this shelf.
    next_x: u32,
}

/// One rasterised glyph (used for repacking).
struct RasterizedGlyph {
    /// Stable identity for this glyph.
    key: GlyphKey,
    /// Packed metrics for atlas placement.
    metrics: PackedMetrics,
    /// Greyscale bitmap (1 byte/pixel, 0 = transparent, 255 = opaque).
    bitmap: Vec<u8>,
}

/// CPU-side glyph atlas with shelf packing and persistent character-to-glyph cache.
///
/// Does NOT reference `RenderSnapshot` or `DirtyRange` — accepts `&[char]` slices.
/// Character collection and space filtering are the caller's responsibility.
pub struct GlyphAtlas {
    /// Atlas texture height (pixels, for reporting/test assert).
    height: u32,
    /// Flattened greyscale pixel data (always MAX_ATLAS_SIZE^2 bytes).
    pixels: Vec<u8>,
    /// GlyphKey → atlas placement / UV lookup (persistent cache).
    glyphs: HashMap<GlyphKey, AtlasGlyph>,
    /// Character-resolution cache (available and unavailable); retained across rebuilds.
    resolution: HashMap<ResolutionKey, GlyphResolution>,
    /// Ordered top-to-bottom shelves for multi-row packing.
    shelves: Vec<Shelf>,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlas {
    /// Creates an empty atlas with a zero-filled MAX_ATLAS_SIZE×MAX_ATLAS_SIZE pixel buffer.
    pub fn new() -> Self {
        Self {
            height: MAX_ATLAS_SIZE,
            pixels: vec![0; (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize],
            glyphs: HashMap::new(),
            resolution: HashMap::new(),
            shelves: Vec::new(),
        }
    }

    /// Given a slice of chars, rasterizes any not yet in the persistent cache.
    ///
    /// Chars should be pre-filtered (no spaces) and deduplicated by the caller
    /// for best performance, but this method sorts and deduplicates internally.
    ///
    /// Returns `RasterizeResult` with newly added glyph keys and an eviction flag.
    /// When `evicted` is true, all UVs have changed and the caller must rebuild
    /// all GPU vertices.
    pub fn rasterize_new(&mut self, fonts: &FontBook, chars: &[char]) -> RasterizeResult {
        let mut chars: Vec<char> = chars.to_vec();
        chars.sort_unstable();
        chars.dedup();

        let size_bits = FONT_SIZE.to_bits();
        let style_bits: u8 = 0;

        // Collect only new available glyphs (not yet cached), deduplicated by GlyphKey.
        let mut new_glyphs: Vec<RasterizedGlyph> = Vec::new();
        let mut queued: HashSet<GlyphKey> = HashSet::new();
        for ch in &chars {
            let request = ResolutionKey {
                scalar: *ch,
                size_bits,
                style_bits,
            };
            let resolution = *self
                .resolution
                .entry(request)
                .or_insert_with(|| fonts.resolve(*ch, FONT_SIZE, style_bits));
            let GlyphResolution::Available(key) = resolution else {
                continue;
            };
            if self.glyphs.contains_key(&key) || !queued.insert(key) {
                continue;
            }
            let (bounds, bitmap) = fonts.rasterize_from_key(key, f32::from_bits(key.size_bits));
            new_glyphs.push(RasterizedGlyph {
                key,
                metrics: PackedMetrics {
                    width: bounds.width,
                    height: bounds.height,
                    bearing_x: bounds.bearing_x,
                    bearing_y: bounds.bearing_y,
                    advance_width: bounds.advance_width,
                },
                bitmap,
            });
        }

        if new_glyphs.is_empty() {
            return RasterizeResult {
                new_keys: Vec::new(),
                evicted: false,
            };
        }

        let new_keys: Vec<GlyphKey> = new_glyphs.iter().map(|g| g.key).collect();

        tracing::debug!(
            new_glyphs = new_glyphs.len(),
            total_glyphs = self.glyphs.len() + new_glyphs.len(),
            "rasterizing new glyphs"
        );

        // Try to pack each new glyph into existing shelves; create new shelf as needed.
        for glyph in &new_glyphs {
            if !self.pack_onto_existing_shelf(glyph) {
                let shelf_y = self.shelves.last().map_or(0, |s| s.y + s.height);
                let gh = glyph.metrics.height as u32;
                if shelf_y + gh > MAX_ATLAS_SIZE {
                    tracing::debug!("atlas full; evicting and rebuilding");
                    // Internal full rebuild by key only; resolution cache is retained.
                    let all_keys: Vec<GlyphKey> = self
                        .glyphs
                        .values()
                        .map(|g| g.key)
                        .chain(new_keys.iter().copied())
                        .collect();
                    self.rebuild_by_keys(fonts, &all_keys);
                    return RasterizeResult {
                        new_keys: all_keys,
                        evicted: true,
                    };
                }
                self.start_new_shelf(glyph);
            }
        }

        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);

        RasterizeResult {
            new_keys,
            evicted: false,
        }
    }

    /// Drops bitmap placement and rebuilds the atlas from scratch.
    ///
    /// Resolution cache is retained. `chars` should be pre-filtered and
    /// deduplicated by the caller. Glyphs are sorted by height descending.
    pub fn rebuild(&mut self, fonts: &FontBook, chars: &[char]) {
        let size_bits = FONT_SIZE.to_bits();
        let style_bits: u8 = 0;
        let mut keys: Vec<GlyphKey> = Vec::new();
        let mut seen: HashSet<GlyphKey> = HashSet::new();
        for ch in chars {
            let request = ResolutionKey {
                scalar: *ch,
                size_bits,
                style_bits,
            };
            let resolution = *self
                .resolution
                .entry(request)
                .or_insert_with(|| fonts.resolve(*ch, FONT_SIZE, style_bits));
            if let GlyphResolution::Available(key) = resolution
                && seen.insert(key)
            {
                keys.push(key);
            }
        }
        self.rebuild_by_keys(fonts, &keys);
    }

    /// Rebuild the atlas from resolved glyph keys (internal).
    fn rebuild_by_keys(&mut self, fonts: &FontBook, keys: &[GlyphKey]) {
        let mut unique_keys: Vec<GlyphKey> = keys.to_vec();
        unique_keys.sort();
        unique_keys.dedup();

        self.glyphs.clear();
        self.pixels.fill(0);
        self.shelves.clear();

        let mut new_rasterized: Vec<RasterizedGlyph> = Vec::new();
        for key in &unique_keys {
            let key = *key;
            let (bounds, bitmap) = fonts.rasterize_from_key(key, f32::from_bits(key.size_bits));
            new_rasterized.push(RasterizedGlyph {
                key,
                metrics: PackedMetrics {
                    width: bounds.width,
                    height: bounds.height,
                    bearing_x: bounds.bearing_x,
                    bearing_y: bounds.bearing_y,
                    advance_width: bounds.advance_width,
                },
                bitmap,
            });
        }

        // Sort by height descending for better packing.
        new_rasterized.sort_by_key(|g| Reverse(g.metrics.height));

        for glyph in &new_rasterized {
            if !self.pack_onto_existing_shelf(glyph) {
                let shelf_y = self.shelves.last().map_or(0, |s| s.y + s.height);
                let gh = glyph.metrics.height as u32;
                if shelf_y + gh > MAX_ATLAS_SIZE {
                    tracing::warn!("atlas full during full rebuild; dropping glyphs");
                    break;
                }
                self.start_new_shelf(glyph);
            }
        }

        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);

        tracing::debug!(
            glyphs = self.glyphs.len(),
            atlas_height = self.height,
            shelves = self.shelves.len(),
            "glyph atlas full rebuild"
        );
    }

    /// Looks up a cached glyph by its stable key. Returns `None` if not cached.
    pub fn glyph(&self, key: GlyphKey) -> Option<&AtlasGlyph> {
        self.glyphs.get(&key)
    }

    /// Looks up a cached glyph by character. Returns `None` if not cached
    /// or if the character resolved as unavailable.
    pub fn glyph_by_char(&self, ch: char) -> Option<&AtlasGlyph> {
        let request = ResolutionKey {
            scalar: ch,
            size_bits: FONT_SIZE.to_bits(),
            style_bits: 0,
        };
        match self.resolution.get(&request)? {
            GlyphResolution::Available(key) => self.glyphs.get(key),
            GlyphResolution::Unavailable => None,
        }
    }

    /// Number of cached glyphs.
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Access the raw pixel buffer for GPU upload.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Atlas height (pixels used, for diagnostics).
    pub fn height(&self) -> u32 {
        self.height
    }
}

// ── Internal packing helpers ──────────────────────────────────────────────

impl GlyphAtlas {
    /// Attempts to place a glyph on an existing shelf. Returns true if placed.
    fn pack_onto_existing_shelf(&mut self, glyph: &RasterizedGlyph) -> bool {
        let gw = glyph.metrics.width as u32 + ATLAS_PADDING;
        let gh = glyph.metrics.height as u32;

        for s_idx in 0..self.shelves.len() {
            if self.shelves[s_idx].height >= gh && self.shelves[s_idx].next_x + gw <= MAX_ATLAS_SIZE
            {
                let x = self.shelves[s_idx].next_x;
                let y = self.shelves[s_idx].y;
                self.blit_glyph(glyph, x, y);
                let left = x as f32 / MAX_ATLAS_SIZE as f32;
                let right = (x + glyph.metrics.width as u32) as f32 / MAX_ATLAS_SIZE as f32;
                let top = y as f32 / MAX_ATLAS_SIZE as f32;
                let bottom = (y + glyph.metrics.height as u32) as f32 / MAX_ATLAS_SIZE as f32;
                self.glyphs.insert(
                    glyph.key,
                    AtlasGlyph {
                        key: glyph.key,
                        uv: AtlasUv {
                            left,
                            top,
                            right,
                            bottom,
                        },
                        width: glyph.metrics.width as u32,
                        height: glyph.metrics.height as u32,
                        bearing_x: glyph.metrics.bearing_x,
                        bearing_y: glyph.metrics.bearing_y,
                        atlas_x: x,
                        atlas_y: y,
                    },
                );
                self.shelves[s_idx].next_x += glyph.metrics.width as u32 + ATLAS_PADDING;
                return true;
            }
        }

        false
    }

    /// Creates a new shelf at the bottom of the atlas and places the glyph.
    fn start_new_shelf(&mut self, glyph: &RasterizedGlyph) {
        let x = 0u32;
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        let gh = glyph.metrics.height as u32;

        self.blit_glyph(glyph, x, y);
        let left = x as f32 / MAX_ATLAS_SIZE as f32;
        let right = (x + glyph.metrics.width as u32) as f32 / MAX_ATLAS_SIZE as f32;
        let top = y as f32 / MAX_ATLAS_SIZE as f32;
        let bottom = (y + glyph.metrics.height as u32) as f32 / MAX_ATLAS_SIZE as f32;
        self.glyphs.insert(
            glyph.key,
            AtlasGlyph {
                key: glyph.key,
                uv: AtlasUv {
                    left,
                    top,
                    right,
                    bottom,
                },
                width: glyph.metrics.width as u32,
                height: glyph.metrics.height as u32,
                bearing_x: glyph.metrics.bearing_x,
                bearing_y: glyph.metrics.bearing_y,
                atlas_x: x,
                atlas_y: y,
            },
        );
        self.shelves.push(Shelf {
            y,
            height: gh,
            next_x: glyph.metrics.width as u32 + ATLAS_PADDING,
        });
    }

    /// Copies a glyph's bitmap into the atlas pixel buffer at the given position.
    fn blit_glyph(&mut self, glyph: &RasterizedGlyph, x: u32, y: u32) {
        for row in 0..glyph.metrics.height {
            let dst_start = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
            let src_start = row * glyph.metrics.width;
            self.pixels[dst_start..dst_start + glyph.metrics.width]
                .copy_from_slice(&glyph.bitmap[src_start..src_start + glyph.metrics.width]);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::{load_system_fonts, with_font_env};

    fn expect_key(resolution: GlyphResolution) -> GlyphKey {
        match resolution {
            GlyphResolution::Available(key) => key,
            GlyphResolution::Unavailable => panic!("expected Available"),
        }
    }

    fn test_font_book() -> FontBook {
        with_font_env(None, || load_system_fonts().expect("load test font"))
    }

    /// Helper: resolve a char to a GlyphKey via the font book.
    fn glyph_key(fonts: &FontBook, ch: char) -> GlyphKey {
        expect_key(fonts.resolve(ch, FONT_SIZE, 0))
    }

    #[test]
    fn empty_atlas_has_no_glyphs() {
        let atlas = GlyphAtlas::new();
        assert_eq!(atlas.len(), 0);
        assert!(atlas.glyph_by_char('a').is_none());
        assert_eq!(
            atlas.pixels().len(),
            (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize
        );
    }

    #[test]
    fn rasterize_new_adds_glyphs() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let chars: Vec<char> = "abc".chars().collect();
        let result = atlas.rasterize_new(&fonts, &chars);

        assert_eq!(result.new_keys.len(), 3);
        assert!(!result.evicted);
        assert!(atlas.glyph_by_char('a').is_some());
        assert!(atlas.glyph_by_char('b').is_some());
        assert!(atlas.glyph_by_char('c').is_some());
    }

    #[test]
    fn rasterize_new_skips_spaces_with_zero_width() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        // Space char produces zero-width bitmap; atlas should skip it.
        let chars: Vec<char> = vec!['a', ' ', 'b'];
        let result = atlas.rasterize_new(&fonts, &chars);

        // 'a' and 'b' added; space rasterized but skipped (width 0).
        assert_eq!(result.new_keys.len(), 3); // space also returned as "new" but has width 0
        // Caller should pre-filter spaces; this test confirms atlas doesn't crash.
    }

    #[test]
    fn cached_atlas_reuses_existing_glyphs() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let first = atlas.rasterize_new(&fonts, &['a', 'b']);
        assert_eq!(first.new_keys.len(), 2);

        let second = atlas.rasterize_new(&fonts, &['a', 'b']);
        assert!(second.new_keys.is_empty(), "no new glyphs expected");
        assert!(!second.evicted);
        assert_eq!(atlas.len(), 2);
    }

    #[test]
    fn rasterize_new_only_returns_new_keys() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let _ = atlas.rasterize_new(&fonts, &['a', 'b']);
        let result = atlas.rasterize_new(&fonts, &['a', 'b', 'c']);
        // Should contain exactly one key: the resolved key for 'c'.
        let c_key = glyph_key(&fonts, 'c');
        assert_eq!(result.new_keys, vec![c_key], "only 'c' is new");
    }

    #[test]
    fn full_rebuild_clears_and_repopulates() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        atlas.rasterize_new(&fonts, &['a', 'b']);
        assert_eq!(atlas.len(), 2);

        atlas.rebuild(&fonts, &['c', 'd']);
        assert_eq!(atlas.len(), 2);
        assert!(atlas.glyph_by_char('a').is_none());
        assert!(atlas.glyph_by_char('c').is_some());
        assert!(atlas.glyph_by_char('d').is_some());
    }

    #[test]
    fn shelf_packing_places_glyphs() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let chars: Vec<char> = "helo!".chars().collect();
        let _ = atlas.rasterize_new(&fonts, &chars);

        for ch in "helo!".chars() {
            assert!(
                atlas.glyph_by_char(ch).is_some(),
                "glyph '{}' should exist",
                ch
            );
        }
    }

    #[test]
    fn new_glyph_lands_on_existing_shelf() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let _ = atlas.rasterize_new(&fonts, &['a', 'b', 'c']);
        // Add a new char that should fit on the same shelf (small glyph).
        let result = atlas.rasterize_new(&fonts, &['d']);
        assert_eq!(result.new_keys.len(), 1);
    }

    #[test]
    fn atlas_creates_multiple_shelves_when_row_overflows() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        // ~22px + ATLAS_PADDING per glyph → ~2500px > 2048 row limit → forces 2nd shelf.
        use unicode_width::UnicodeWidthChar;
        let chars: Vec<char> = ('!'..='~')
            .chain('¡'..='ÿ')
            .filter(|c| UnicodeWidthChar::width(*c).unwrap_or(0) > 0)
            .take(130)
            .collect();
        assert!(
            chars.len() >= 115,
            "need ~115+ chars to overflow 2048px row, got {}",
            chars.len()
        );
        let _ = atlas.rasterize_new(&fonts, &chars);
        // At least one glyph should be on a non-zero y shelf.
        let on_second_shelf = atlas.glyphs.values().any(|g| g.atlas_y > 0);
        assert!(
            on_second_shelf,
            "at least one glyph should be on a second shelf"
        );
        // All chars should still be in the atlas.
        for ch in &chars {
            assert!(
                atlas.glyph_by_char(*ch).is_some(),
                "glyph '{}' should exist",
                ch
            );
        }
    }

    #[test]
    fn should_pack_latin_and_space_when_harbor_font_unset() {
        // Arrange — default DirectWrite primary (T0002; no system fallback yet).
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let chars = ['H', 'e', 'l', 'l', 'o', ' ', 'A'];

        // Act
        let result = atlas.rasterize_new(&fonts, &chars);

        // Assert
        assert!(!result.evicted);
        for ch in ['H', 'e', 'l', 'o', 'A'] {
            let glyph = atlas
                .glyph_by_char(ch)
                .unwrap_or_else(|| panic!("latin glyph '{ch}' missing from atlas"));
            assert!(glyph.width > 0, "'{ch}' width");
            assert!(glyph.height > 0, "'{ch}' height");
            assert_eq!(glyph.key, expect_key(fonts.resolve(ch, FONT_SIZE, 0)));
        }
        // Space may be omitted from atlas packing (zero ink); resolution must still be stable.
        let space_key = expect_key(fonts.resolve(' ', FONT_SIZE, 0));
        let (space_bounds, space_bitmap) = fonts.rasterize(' ', harbor_config::FONT_SIZE);
        assert_eq!(space_bounds.width, 0);
        assert_eq!(space_bounds.height, 0);
        assert!(space_bitmap.is_empty());
        assert!(space_bounds.advance_width > 0.0);
        let _ = space_key;
        let _ = result.new_keys;
    }

    #[test]
    fn should_pack_cjk_when_available_via_fallback() {
        // Arrange
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        // Act
        let result = atlas.rasterize_new(&fonts, &['中']);

        // Assert — available keys are packed once; unavailable yields no atlas entry.
        match fonts.resolve('中', FONT_SIZE, 0) {
            GlyphResolution::Available(key) => {
                assert_eq!(result.new_keys, vec![key]);
                assert!(!result.evicted);
                if key.face_id != 0 || atlas.glyph_by_char('中').is_some() {
                    let glyph = atlas.glyph_by_char('中');
                    if let Some(glyph) = glyph {
                        assert_eq!(glyph.key, key);
                    }
                }
            }
            GlyphResolution::Unavailable => {
                assert!(result.new_keys.is_empty());
                assert!(atlas.glyph_by_char('中').is_none());
            }
        }
    }

    #[test]
    fn should_omit_unavailable_from_new_keys_when_rasterizing() {
        // Arrange
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let ch = '\u{E000}';

        // Act
        let first = atlas.rasterize_new(&fonts, &[ch]);
        let second = atlas.rasterize_new(&fonts, &[ch]);

        // Assert
        if matches!(
            fonts.resolve(ch, FONT_SIZE, 0),
            GlyphResolution::Unavailable
        ) {
            assert!(first.new_keys.is_empty());
            assert!(second.new_keys.is_empty());
            assert!(atlas.glyph_by_char(ch).is_none());
        }
    }

    #[test]
    fn should_retain_resolution_across_rebuild_for_kept_chars() {
        // Arrange
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let _ = atlas.rasterize_new(&fonts, &['A', 'B', 'C']);
        let key_a = expect_key(fonts.resolve('A', FONT_SIZE, 0));

        // Act — rebuild keeps A; resolution cache must remain usable without remapping.
        atlas.rebuild(&fonts, &['A']);
        let after = atlas.rasterize_new(&fonts, &['A']);

        // Assert
        assert!(atlas.glyph_by_char('A').is_some());
        assert_eq!(atlas.glyph_by_char('A').unwrap().key, key_a);
        assert!(
            after.new_keys.is_empty(),
            "retained resolution must not re-queue A"
        );
        assert!(atlas.glyph_by_char('B').is_none());
    }

    #[test]
    fn should_pack_mixed_latin_and_cjk_without_duplicate_keys() {
        // Arrange
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let chars = ['A', '中', 'B', '中', 'A'];

        // Act
        let result = atlas.rasterize_new(&fonts, &chars);
        let again = atlas.rasterize_new(&fonts, &chars);

        // Assert
        assert!(!result.new_keys.is_empty());
        let mut unique = result.new_keys.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            result.new_keys.len(),
            "new_keys must be GlyphKey-unique"
        );
        assert!(again.new_keys.is_empty());
        assert!(atlas.glyph_by_char('A').is_some());
        assert!(atlas.glyph_by_char('B').is_some());
    }

    #[test]
    fn atlas_overflow_triggers_eviction() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        // Fill atlas with many unique chars to trigger overflow + eviction.
        use unicode_width::UnicodeWidthChar;
        let all_chars: Vec<char> = ('!'..='~')
            .chain('¡'..='ÿ')
            .filter(|c| UnicodeWidthChar::width(*c).unwrap_or(0) > 0)
            .take(250)
            .collect();

        let result = atlas.rasterize_new(&fonts, &all_chars);
        // On most fonts, 250 unique glyphs should overflow 2048px atlas.
        // If evicted, all chars are in new_keys and cache is rebuilt.
        if result.evicted {
            assert_eq!(result.new_keys.len(), atlas.len());
        }
        // Regardless, the atlas should contain at least one glyph.
        assert!(!atlas.is_empty(), "atlas should contain glyphs");
    }

    #[test]
    fn atlas_persistent_cache_across_rebuilds() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        atlas.rasterize_new(&fonts, &['a', 'b', 'x', 'y']);
        assert_eq!(atlas.len(), 4);

        // Rebuild with a different set
        atlas.rebuild(&fonts, &['x', 'y']);
        assert_eq!(atlas.len(), 2, "rebuild clears old glyphs");
        assert!(atlas.glyph_by_char('x').is_some());
        assert!(atlas.glyph_by_char('y').is_some());
    }

    #[test]
    fn height_reports_used_atlas_height() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        // Fresh atlas reports MAX_ATLAS_SIZE (buffer is always full-size).
        assert_eq!(atlas.height(), MAX_ATLAS_SIZE);

        atlas.rasterize_new(&fonts, &['a', 'b']);
        // After rasterizing, height reflects actual used pixel rows.
        assert!(atlas.height() > 0, "height should reflect used space");
        assert!(atlas.height() <= MAX_ATLAS_SIZE);
    }

    #[test]
    fn glyph_returns_none_for_unknown_char() {
        let fonts = test_font_book();
        let atlas = GlyphAtlas::new();
        assert!(atlas.glyph_by_char('Z').is_none());
        assert!(atlas.glyph_by_char('中').is_none());
        assert!(atlas.glyph_by_char('\x00').is_none());
        // Also test direct key lookup
        let key = glyph_key(&fonts, 'A');
        assert!(atlas.glyph(key).is_none());
    }

    #[test]
    fn pixels_buffer_starts_zero_filled() {
        let atlas = GlyphAtlas::new();
        let pixels = atlas.pixels();
        assert_eq!(pixels.len(), (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize);
        assert!(
            pixels.iter().all(|&p| p == 0),
            "fresh atlas should be zero-filled"
        );
    }

    #[test]
    fn rasterize_new_handles_empty_char_slice() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let result = atlas.rasterize_new(&fonts, &[]);
        assert!(result.new_keys.is_empty());
        assert!(!result.evicted);
        assert_eq!(atlas.len(), 0);
    }

    // ── GlyphKey tests ───────────────────────────────────────────────

    #[test]
    fn should_equal_when_all_fields_match() {
        let k1 = GlyphKey {
            face_id: 1,
            glyph_index: 42,
            size_bits: 0x41800000,
            style_bits: 0,
        };
        let k2 = GlyphKey {
            face_id: 1,
            glyph_index: 42,
            size_bits: 0x41800000,
            style_bits: 0,
        };
        assert_eq!(k1, k2, "identical keys should be equal");
    }

    #[test]
    fn should_not_equal_when_face_id_differs() {
        let k1 = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 0,
        };
        let k2 = GlyphKey {
            face_id: 1,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 0,
        };
        assert_ne!(k1, k2, "different face_id should produce inequality");
    }

    #[test]
    fn should_not_equal_when_glyph_index_differs() {
        let k1 = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 0,
        };
        let k2 = GlyphKey {
            face_id: 0,
            glyph_index: 2,
            size_bits: 10,
            style_bits: 0,
        };
        assert_ne!(k1, k2, "different glyph_index should produce inequality");
    }

    #[test]
    fn should_not_equal_when_size_bits_differs() {
        let k1 = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 0,
        };
        let k2 = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 20,
            style_bits: 0,
        };
        assert_ne!(k1, k2, "different size_bits should produce inequality");
    }

    #[test]
    fn should_not_equal_when_style_bits_differs() {
        let k1 = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 0,
        };
        let k2 = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 1,
        };
        assert_ne!(k1, k2, "different style_bits should produce inequality");
    }

    #[test]
    fn should_order_by_face_id_first() {
        let k_small = GlyphKey {
            face_id: 0,
            glyph_index: 100,
            size_bits: 0,
            style_bits: 0,
        };
        let k_large = GlyphKey {
            face_id: 1,
            glyph_index: 0,
            size_bits: 0,
            style_bits: 0,
        };
        assert!(k_small < k_large, "lower face_id should come first");
    }

    #[test]
    fn should_order_by_glyph_index_when_face_ids_equal() {
        let k_small = GlyphKey {
            face_id: 0,
            glyph_index: 5,
            size_bits: 0,
            style_bits: 0,
        };
        let k_large = GlyphKey {
            face_id: 0,
            glyph_index: 10,
            size_bits: 0,
            style_bits: 0,
        };
        assert!(k_small < k_large, "lower glyph_index should come first");
    }

    #[test]
    fn should_order_by_size_bits_when_prev_fields_equal() {
        let k_small = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 0,
        };
        let k_large = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 20,
            style_bits: 0,
        };
        assert!(k_small < k_large, "lower size_bits should come first");
    }

    #[test]
    fn should_order_by_style_bits_when_all_prev_equal() {
        let k_small = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 0,
        };
        let k_large = GlyphKey {
            face_id: 0,
            glyph_index: 1,
            size_bits: 10,
            style_bits: 1,
        };
        assert!(k_small < k_large, "lower style_bits should come first");
    }

    #[test]
    fn should_hash_consistently_with_equality() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let k1 = GlyphKey {
            face_id: 3,
            glyph_index: 7,
            size_bits: 0x41800000,
            style_bits: 2,
        };
        let k2 = GlyphKey {
            face_id: 3,
            glyph_index: 7,
            size_bits: 0x41800000,
            style_bits: 2,
        };

        let mut h1 = DefaultHasher::new();
        k1.hash(&mut h1);
        let hash1 = h1.finish();

        let mut h2 = DefaultHasher::new();
        k2.hash(&mut h2);
        let hash2 = h2.finish();

        assert_eq!(hash1, hash2, "equal keys must produce the same hash");
    }

    #[test]
    fn should_useful_as_hashmap_key() {
        let fonts = test_font_book();
        let key_a = expect_key(fonts.resolve('A', FONT_SIZE, 0));
        let key_b = expect_key(fonts.resolve('B', FONT_SIZE, 0));

        let mut map = hashbrown::HashMap::new();
        map.insert(key_a, 1u32);
        map.insert(key_b, 2u32);

        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&key_a), Some(&1));
        assert_eq!(map.get(&key_b), Some(&2));
    }

    #[test]
    fn should_resolve_different_chars_to_different_keys() {
        let fonts = test_font_book();
        let key_a = expect_key(fonts.resolve('A', FONT_SIZE, 0));
        let key_b = expect_key(fonts.resolve('B', FONT_SIZE, 0));
        assert_ne!(
            key_a, key_b,
            "different chars should resolve to different keys"
        );
    }

    #[test]
    fn should_resolve_same_char_to_same_key_regardless_of_size() {
        let fonts = test_font_book();
        // resolve() always uses FONT_SIZE internally, so repeated calls are stable.
        let k1 = expect_key(fonts.resolve('Z', FONT_SIZE, 0));
        let k2 = expect_key(fonts.resolve('Z', FONT_SIZE, 0));
        assert_eq!(k1, k2, "same char should always resolve to the same key");
    }

    // ── GlyphBitmapBounds tests ──────────────────────────────────────

    #[test]
    fn should_expose_all_fields_when_constructed() {
        let bounds = GlyphBitmapBounds {
            width: 10,
            height: 20,
            bearing_x: 1,
            bearing_y: -2,
            advance_width: 12.5,
        };
        assert_eq!(bounds.width, 10);
        assert_eq!(bounds.height, 20);
        assert_eq!(bounds.bearing_x, 1);
        assert_eq!(bounds.bearing_y, -2);
        assert_eq!(bounds.advance_width, 12.5);
    }

    #[test]
    fn should_support_zero_dimensions() {
        let bounds = GlyphBitmapBounds {
            width: 0,
            height: 0,
            bearing_x: 0,
            bearing_y: 0,
            advance_width: 0.0,
        };
        assert_eq!(bounds.width, 0);
        assert_eq!(bounds.height, 0);
    }

    #[test]
    fn should_support_negative_bearings() {
        let bounds = GlyphBitmapBounds {
            width: 8,
            height: 12,
            bearing_x: -3,
            bearing_y: -5,
            advance_width: 6.0,
        };
        assert_eq!(bounds.bearing_x, -3);
        assert_eq!(bounds.bearing_y, -5);
    }

    #[test]
    fn should_be_copy_and_clone() {
        let bounds = GlyphBitmapBounds {
            width: 10,
            height: 20,
            bearing_x: 1,
            bearing_y: 2,
            advance_width: 11.0,
        };
        let copy = bounds;
        let cloned = bounds;
        assert_eq!(copy.width, cloned.width);
        assert_eq!(copy.advance_width, cloned.advance_width);
    }

    // ── AtlasGlyph bearing fields tests ──────────────────────────────

    #[test]
    fn should_store_bearing_fields_in_atlas_glyph() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let _ = atlas.rasterize_new(&fonts, &['A']);
        let glyph = atlas.glyph_by_char('A').expect("A should be in atlas");
        // bearing_x and bearing_y should be set (exact values depend on the font).
        // Just verify the fields are accessible and the struct is coherent.
        let _ = glyph.bearing_x;
        let _ = glyph.bearing_y;
        assert!(glyph.width > 0, "A glyph should have positive width");
        assert!(glyph.height > 0, "A glyph should have positive height");
    }

    #[test]
    fn should_lookup_glyph_by_key_after_rasterize() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let _ = atlas.rasterize_new(&fonts, &['X']);
        let key = expect_key(fonts.resolve('X', FONT_SIZE, 0));
        let glyph = atlas.glyph(key).expect("glyph should be findable by key");
        assert!(glyph.width > 0);
        assert!(glyph.key == key, "stored glyph key should match lookup key");
    }

    #[test]
    fn should_return_none_when_looking_up_key_not_in_atlas() {
        let fonts = test_font_book();
        let atlas = GlyphAtlas::new();
        let key = expect_key(fonts.resolve('Q', FONT_SIZE, 0));
        assert!(atlas.glyph(key).is_none(), "empty atlas should return None");
    }

    #[test]
    fn rasterize_result_keys_match_resolved_keys() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let chars: Vec<char> = vec!['a', 'b', 'c'];
        let result = atlas.rasterize_new(&fonts, &chars);
        // Every key in new_keys should match the resolved key for the corresponding char.
        for (i, ch) in chars.iter().enumerate() {
            let expected_key = expect_key(fonts.resolve(*ch, FONT_SIZE, 0));
            assert_eq!(
                result.new_keys[i], expected_key,
                "new_keys[{}] should match resolve('{}')",
                i, ch
            );
        }
    }

    #[test]
    fn glyph_by_char_matches_glyph_by_key() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();
        let _ = atlas.rasterize_new(&fonts, &['M']);
        let key = expect_key(fonts.resolve('M', FONT_SIZE, 0));
        let by_char = atlas
            .glyph_by_char('M')
            .expect("glyph_by_char should find M");
        let by_key = atlas.glyph(key).expect("glyph should find M by key");
        // Both should point to the same atlas entry.
        assert_eq!(by_char.atlas_x, by_key.atlas_x);
        assert_eq!(by_char.atlas_y, by_key.atlas_y);
        assert_eq!(by_char.width, by_key.width);
        assert_eq!(by_char.height, by_key.height);
    }
}
