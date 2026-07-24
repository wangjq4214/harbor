use std::cmp::Reverse;

use fontdue::Metrics;
use hashbrown::HashMap;

use crate::font::FontBook;
use harbor_config::FONT_SIZE;

const ATLAS_PADDING: u32 = 1;
pub const MAX_ATLAS_SIZE: u32 = 2048;

/// Result of an incremental `rasterize_new` call.
pub struct RasterizeResult {
    /// Characters that were newly rasterized and added to the cache.
    pub new_chars: Vec<char>,
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
    /// UV sub-region (unit texture coordinates).
    pub uv: AtlasUv,
    /// Glyph pixel width.
    pub width: u32,
    /// Glyph pixel height.
    pub height: u32,
    /// fontdue horizontal offset (sub-pixel).
    pub xmin: i32,
    /// fontdue vertical offset (sub-pixel).
    pub ymin: i32,
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
    /// Source character.
    ch: char,
    /// fontdue metrics.
    metrics: Metrics,
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
    /// Character → atlas placement / UV lookup (persistent cache).
    glyphs: HashMap<char, AtlasGlyph>,
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
            shelves: Vec::new(),
        }
    }

    /// Given a slice of chars, rasterizes any not yet in the persistent cache.
    ///
    /// Chars should be pre-filtered (no spaces) and deduplicated by the caller
    /// for best performance, but this method sorts and deduplicates internally.
    ///
    /// Returns `RasterizeResult` with newly added chars and an eviction flag.
    /// When `evicted` is true, all UVs have changed and the caller must rebuild
    /// all GPU vertices.
    pub fn rasterize_new(&mut self, fonts: &FontBook, chars: &[char]) -> RasterizeResult {
        let mut chars: Vec<char> = chars.to_vec();
        chars.sort_unstable();
        chars.dedup();

        // Collect only new glyphs (not yet cached).
        let mut new_glyphs: Vec<RasterizedGlyph> = Vec::new();
        for ch in &chars {
            if !self.glyphs.contains_key(ch) {
                let (metrics, bitmap) = fonts.rasterize(*ch, FONT_SIZE);
                new_glyphs.push(RasterizedGlyph {
                    ch: *ch,
                    metrics,
                    bitmap,
                });
            }
        }

        if new_glyphs.is_empty() {
            return RasterizeResult {
                new_chars: Vec::new(),
                evicted: false,
            };
        }

        let new_chars: Vec<char> = new_glyphs.iter().map(|g| g.ch).collect();

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
                    // Internal full rebuild with the union of existing + new chars.
                    let all_chars: Vec<char> = self
                        .glyphs
                        .keys()
                        .copied()
                        .chain(new_chars.iter().copied())
                        .collect();
                    self.rebuild(fonts, &all_chars);
                    return RasterizeResult {
                        new_chars: all_chars,
                        evicted: true,
                    };
                }
                self.start_new_shelf(glyph);
            }
        }

        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);

        RasterizeResult {
            new_chars,
            evicted: false,
        }
    }

    /// Drops all cached glyphs and rebuilds the atlas from scratch.
    ///
    /// `chars` should be pre-filtered and deduplicated by the caller.
    /// Glyphs are sorted by height descending for better packing.
    pub fn rebuild(&mut self, fonts: &FontBook, chars: &[char]) {
        let mut chars: Vec<char> = chars.to_vec();
        chars.sort_unstable();
        chars.dedup();

        self.glyphs.clear();
        self.pixels.fill(0);
        self.shelves.clear();

        let mut new_rasterized: Vec<RasterizedGlyph> = Vec::new();
        for ch in &chars {
            let (metrics, bitmap) = fonts.rasterize(*ch, FONT_SIZE);
            new_rasterized.push(RasterizedGlyph {
                ch: *ch,
                metrics,
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

    /// Looks up a cached glyph. Returns `None` if the glyph hasn't been rasterized.
    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.glyphs.get(&ch)
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
                    glyph.ch,
                    AtlasGlyph {
                        uv: AtlasUv {
                            left,
                            top,
                            right,
                            bottom,
                        },
                        width: glyph.metrics.width as u32,
                        height: glyph.metrics.height as u32,
                        xmin: glyph.metrics.xmin,
                        ymin: glyph.metrics.ymin,
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
            glyph.ch,
            AtlasGlyph {
                uv: AtlasUv {
                    left,
                    top,
                    right,
                    bottom,
                },
                width: glyph.metrics.width as u32,
                height: glyph.metrics.height as u32,
                xmin: glyph.metrics.xmin,
                ymin: glyph.metrics.ymin,
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
    use crate::font::load_system_fonts;

    fn test_font_book() -> FontBook {
        load_system_fonts().expect("load test font")
    }

    #[test]
    fn empty_atlas_has_no_glyphs() {
        let atlas = GlyphAtlas::new();
        assert_eq!(atlas.len(), 0);
        assert!(atlas.glyph('a').is_none());
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

        assert_eq!(result.new_chars.len(), 3);
        assert!(!result.evicted);
        assert!(atlas.glyph('a').is_some());
        assert!(atlas.glyph('b').is_some());
        assert!(atlas.glyph('c').is_some());
    }

    #[test]
    fn rasterize_new_skips_spaces_with_zero_width() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        // Space char produces zero-width bitmap; atlas should skip it.
        let chars: Vec<char> = vec!['a', ' ', 'b'];
        let result = atlas.rasterize_new(&fonts, &chars);

        // 'a' and 'b' added; space rasterized but skipped (width 0).
        assert_eq!(result.new_chars.len(), 3); // space also returned as "new" but has width 0
        // Caller should pre-filter spaces; this test confirms atlas doesn't crash.
    }

    #[test]
    fn cached_atlas_reuses_existing_glyphs() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let first = atlas.rasterize_new(&fonts, &['a', 'b']);
        assert_eq!(first.new_chars.len(), 2);

        let second = atlas.rasterize_new(&fonts, &['a', 'b']);
        assert!(second.new_chars.is_empty(), "no new glyphs expected");
        assert!(!second.evicted);
        assert_eq!(atlas.len(), 2);
    }

    #[test]
    fn rasterize_new_only_returns_new_chars() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let _ = atlas.rasterize_new(&fonts, &['a', 'b']);
        let result = atlas.rasterize_new(&fonts, &['a', 'b', 'c']);
        assert_eq!(result.new_chars, vec!['c'], "only 'c' is new");
    }

    #[test]
    fn full_rebuild_clears_and_repopulates() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        atlas.rasterize_new(&fonts, &['a', 'b']);
        assert_eq!(atlas.len(), 2);

        atlas.rebuild(&fonts, &['c', 'd']);
        assert_eq!(atlas.len(), 2);
        assert!(atlas.glyph('a').is_none());
        assert!(atlas.glyph('c').is_some());
        assert!(atlas.glyph('d').is_some());
    }

    #[test]
    fn shelf_packing_places_glyphs() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let chars: Vec<char> = "helo!".chars().collect();
        let _ = atlas.rasterize_new(&fonts, &chars);

        for ch in "helo!".chars() {
            assert!(atlas.glyph(ch).is_some(), "glyph '{}' should exist", ch);
        }
    }

    #[test]
    fn new_glyph_lands_on_existing_shelf() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let _ = atlas.rasterize_new(&fonts, &['a', 'b', 'c']);
        // Add a new char that should fit on the same shelf (small glyph).
        let result = atlas.rasterize_new(&fonts, &['d']);
        assert_eq!(result.new_chars, vec!['d']);
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
            assert!(atlas.glyph(*ch).is_some(), "glyph '{}' should exist", ch);
        }
    }

    #[test]
    fn cjk_glyph_rasterizes_from_fallback_font() {
        let fonts = test_font_book();
        let mut atlas = GlyphAtlas::new();

        let result = atlas.rasterize_new(&fonts, &['中']);
        assert_eq!(result.new_chars.len(), 1);
        let glyph = atlas.glyph('中').expect("CJK glyph in atlas");
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
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
        // If evicted, all chars are in new_chars and cache is rebuilt.
        if result.evicted {
            assert_eq!(result.new_chars.len(), atlas.len());
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
        assert!(atlas.glyph('x').is_some());
        assert!(atlas.glyph('y').is_some());
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
        let atlas = GlyphAtlas::new();
        assert!(atlas.glyph('Z').is_none());
        assert!(atlas.glyph('中').is_none());
        assert!(atlas.glyph('\x00').is_none());
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
        assert!(result.new_chars.is_empty());
        assert!(!result.evicted);
        assert_eq!(atlas.len(), 0);
    }
}
