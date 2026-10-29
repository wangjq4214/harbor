use std::cmp::Reverse;

use hashbrown::{HashMap, HashSet};

use crate::contracts::{FontSize, FontStyle, GlyphResolution, ResolutionKey};
use crate::font::FontBook;
use harbor_config::FONT_SIZE;

const ATLAS_PADDING: u32 = 1;
pub const MAX_ATLAS_SIZE: u32 = 2048;

// Compatibility path retained for callers that historically imported the key
// through `atlas`.
pub use crate::contracts::GlyphKey;

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
    /// Horizontal advance in physical pixels at this glyph's raster size.
    pub advance_width: f32,
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
    /// Raster bounds and placement metrics.
    bounds: GlyphBitmapBounds,
    /// Greyscale bitmap (1 byte/pixel, 0 = transparent, 255 = opaque).
    bitmap: Vec<u8>,
}

impl RasterizedGlyph {
    fn rasterize(fonts: &FontBook, key: GlyphKey) -> Self {
        let (bounds, bitmap) = fonts.rasterize_from_key(key);
        Self {
            key,
            bounds,
            bitmap,
        }
    }
}

/// Shelf-packed placement and pixel storage for the CPU atlas.
///
/// This component has no knowledge of character resolution or font backends.
/// It owns all mutable layout state, including full rebuilds.
struct AtlasStore {
    height: u32,
    pixels: Vec<u8>,
    glyphs: HashMap<GlyphKey, AtlasGlyph>,
    shelves: Vec<Shelf>,
}

impl AtlasStore {
    fn new() -> Self {
        Self {
            height: MAX_ATLAS_SIZE,
            pixels: vec![0; (MAX_ATLAS_SIZE * MAX_ATLAS_SIZE) as usize],
            glyphs: HashMap::new(),
            shelves: Vec::new(),
        }
    }

    fn glyph(&self, key: GlyphKey) -> Option<&AtlasGlyph> {
        self.glyphs.get(&key)
    }

    fn rebuild(&mut self, mut glyphs: Vec<RasterizedGlyph>) {
        glyphs.sort_by_key(|glyph| Reverse(glyph.bounds.height));
        self.glyphs.clear();
        self.pixels.fill(0);
        self.shelves.clear();

        for glyph in &glyphs {
            if !self.try_pack(glyph) {
                tracing::warn!("atlas full during full rebuild; dropping glyphs");
                break;
            }
        }
        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);
    }

    fn add_incremental(&mut self, glyphs: &[RasterizedGlyph]) -> bool {
        for glyph in glyphs {
            if !self.try_pack(glyph) {
                return false;
            }
        }
        self.height = self.shelves.last().map_or(1, |s| s.y + s.height);
        true
    }

    fn len(&self) -> usize {
        self.glyphs.len()
    }

    fn glyph_keys(&self) -> Vec<GlyphKey> {
        self.glyphs.values().map(|glyph| glyph.key).collect()
    }

    fn shelf_count(&self) -> usize {
        self.shelves.len()
    }

    fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    fn height(&self) -> u32 {
        self.height
    }
}

impl AtlasStore {
    fn try_pack(&mut self, glyph: &RasterizedGlyph) -> bool {
        if self.pack_onto_existing_shelf(glyph) {
            return true;
        }
        let shelf_y = self.shelves.last().map_or(0, |s| s.y + s.height);
        if shelf_y + glyph.bounds.height as u32 > MAX_ATLAS_SIZE {
            return false;
        }
        self.start_new_shelf(glyph);
        true
    }

    fn pack_onto_existing_shelf(&mut self, glyph: &RasterizedGlyph) -> bool {
        let gw = glyph.bounds.width as u32 + ATLAS_PADDING;
        let gh = glyph.bounds.height as u32;
        for index in 0..self.shelves.len() {
            if self.shelves[index].height < gh || self.shelves[index].next_x + gw > MAX_ATLAS_SIZE {
                continue;
            }
            let x = self.shelves[index].next_x;
            let y = self.shelves[index].y;
            self.blit_glyph(glyph, x, y);
            self.insert_placement(glyph, x, y);
            self.shelves[index].next_x += glyph.bounds.width as u32 + ATLAS_PADDING;
            return true;
        }
        false
    }

    fn start_new_shelf(&mut self, glyph: &RasterizedGlyph) {
        let x = 0;
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        self.blit_glyph(glyph, x, y);
        self.insert_placement(glyph, x, y);
        self.shelves.push(Shelf {
            y,
            height: glyph.bounds.height as u32,
            next_x: glyph.bounds.width as u32 + ATLAS_PADDING,
        });
    }

    fn insert_placement(&mut self, glyph: &RasterizedGlyph, x: u32, y: u32) {
        self.glyphs.insert(
            glyph.key,
            AtlasGlyph {
                key: glyph.key,
                uv: AtlasUv {
                    left: x as f32 / MAX_ATLAS_SIZE as f32,
                    top: y as f32 / MAX_ATLAS_SIZE as f32,
                    right: (x + glyph.bounds.width as u32) as f32 / MAX_ATLAS_SIZE as f32,
                    bottom: (y + glyph.bounds.height as u32) as f32 / MAX_ATLAS_SIZE as f32,
                },
                width: glyph.bounds.width as u32,
                height: glyph.bounds.height as u32,
                bearing_x: glyph.bounds.bearing_x,
                bearing_y: glyph.bounds.bearing_y,
                advance_width: glyph.bounds.advance_width,
                atlas_x: x,
                atlas_y: y,
            },
        );
    }

    fn blit_glyph(&mut self, glyph: &RasterizedGlyph, x: u32, y: u32) {
        for row in 0..glyph.bounds.height {
            let dst_start = ((y + row as u32) * MAX_ATLAS_SIZE + x) as usize;
            let src_start = row * glyph.bounds.width;
            self.pixels[dst_start..dst_start + glyph.bounds.width]
                .copy_from_slice(&glyph.bitmap[src_start..src_start + glyph.bounds.width]);
        }
    }
}

/// CPU-side glyph resolution cache and rasterization orchestration.
///
/// Shelf packing, placements, pixels, and rebuild layout are delegated to
/// the private `AtlasStore`. Character collection and space filtering remain the caller's
/// responsibility.
pub struct GlyphAtlas {
    resolution: HashMap<ResolutionKey, GlyphResolution>,
    store: AtlasStore,
    font_size: FontSize,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self {
            resolution: HashMap::new(),
            store: AtlasStore::new(),
            font_size: FontSize::new(FONT_SIZE).expect("default font size must be valid"),
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
        let size = FontSize::new(fonts.size()).expect("configured font size must be valid");
        self.font_size = size;
        self.rasterize_new_at_size(fonts, chars, size)
    }

    /// Adds requested glyphs incrementally, collecting the full active set only
    /// when packing fails. The callback must include unchanged visible glyphs and
    /// overlays; entries outside that set are reclaimable during the repack.
    pub fn rasterize_new_with_active(
        &mut self,
        fonts: &FontBook,
        chars: &[char],
        active: impl FnOnce() -> Vec<char>,
    ) -> RasterizeResult {
        let size = FontSize::new(fonts.size()).expect("configured font size must be valid");
        self.font_size = size;
        self.rasterize_new_at_size_with_retention(fonts, chars, size, |atlas| {
            active()
                .into_iter()
                .filter_map(|ch| {
                    let request = ResolutionKey::new(ch, size, FontStyle::REGULAR);
                    match *atlas
                        .resolution
                        .entry(request)
                        .or_insert_with(|| fonts.resolve(ch, size.get(), FontStyle::REGULAR))
                    {
                        GlyphResolution::Available(key) => Some(key),
                        GlyphResolution::Unavailable => None,
                    }
                })
                .collect()
        })
    }

    /// Rasterizes uncached glyphs at an explicit physical font size.
    ///
    /// Unlike [`Self::rasterize_new`], this does not change the default size used
    /// by the legacy character lookup path.
    pub fn rasterize_new_at_size(
        &mut self,
        fonts: &FontBook,
        chars: &[char],
        size: FontSize,
    ) -> RasterizeResult {
        let retained_keys = self.store.glyph_keys();
        self.rasterize_new_at_size_retaining(fonts, chars, size, &retained_keys)
    }

    /// Rasterizes at an explicit size and, if repacking is required, evicts
    /// entries not present in `retained_keys`.
    pub fn rasterize_new_at_size_retaining(
        &mut self,
        fonts: &FontBook,
        chars: &[char],
        size: FontSize,
        retained_keys: &[GlyphKey],
    ) -> RasterizeResult {
        self.rasterize_new_at_size_with_retention(fonts, chars, size, |_| retained_keys.to_vec())
    }

    fn rasterize_new_at_size_with_retention(
        &mut self,
        fonts: &FontBook,
        chars: &[char],
        size: FontSize,
        retained_keys: impl FnOnce(&mut Self) -> Vec<GlyphKey>,
    ) -> RasterizeResult {
        let mut chars: Vec<char> = chars.to_vec();
        chars.sort_unstable();
        chars.dedup();

        let style = FontStyle::REGULAR;

        // Collect only new available glyphs (not yet cached), deduplicated by GlyphKey.
        let mut new_glyphs: Vec<RasterizedGlyph> = Vec::new();
        let mut queued: HashSet<GlyphKey> = HashSet::new();
        for ch in &chars {
            let request = ResolutionKey::new(*ch, size, style);
            let resolution = *self
                .resolution
                .entry(request)
                .or_insert_with(|| fonts.resolve(*ch, size.get(), style));
            let GlyphResolution::Available(key) = resolution else {
                continue;
            };
            if self.store.glyph(key).is_some() || !queued.insert(key) {
                continue;
            }
            new_glyphs.push(RasterizedGlyph::rasterize(fonts, key));
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
            total_glyphs = self.store.len() + new_glyphs.len(),
            "rasterizing new glyphs"
        );

        if !self.store.add_incremental(&new_glyphs) {
            tracing::debug!("atlas full; evicting and rebuilding");
            let all_keys: Vec<GlyphKey> = retained_keys(self)
                .into_iter()
                .chain(new_keys.iter().copied())
                .collect();
            self.rebuild_by_keys(fonts, &all_keys);
            return RasterizeResult {
                new_keys: all_keys,
                evicted: true,
            };
        }

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
        let size = FontSize::new(fonts.size()).expect("configured font size must be valid");
        self.font_size = size;
        let style = FontStyle::REGULAR;
        let mut keys: Vec<GlyphKey> = Vec::new();
        let mut seen: HashSet<GlyphKey> = HashSet::new();
        for ch in chars {
            let request = ResolutionKey::new(*ch, size, style);
            let resolution = *self
                .resolution
                .entry(request)
                .or_insert_with(|| fonts.resolve(*ch, size.get(), style));
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

        let glyphs = unique_keys
            .into_iter()
            .map(|key| RasterizedGlyph::rasterize(fonts, key))
            .collect();
        self.store.rebuild(glyphs);

        tracing::debug!(
            glyphs = self.store.len(),
            atlas_height = self.store.height(),
            shelves = self.store.shelf_count(),
            "glyph atlas full rebuild"
        );
    }

    /// Looks up a cached glyph by its stable key. Returns `None` if not cached.
    pub fn glyph(&self, key: GlyphKey) -> Option<&AtlasGlyph> {
        self.store.glyph(key)
    }

    /// Looks up a cached glyph by character. Returns `None` if not cached
    /// or if the character resolved as unavailable.
    pub fn glyph_by_char(&self, ch: char) -> Option<&AtlasGlyph> {
        self.glyph_by_char_at_size(ch, self.font_size)
    }

    /// Looks up a cached glyph by character at an explicit physical font size.
    pub fn glyph_by_char_at_size(&self, ch: char, size: FontSize) -> Option<&AtlasGlyph> {
        let request = ResolutionKey::new(ch, size, FontStyle::REGULAR);
        match self.resolution.get(&request)? {
            GlyphResolution::Available(key) => self.store.glyph(*key),
            GlyphResolution::Unavailable => None,
        }
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn pixels(&self) -> &[u8] {
        self.store.pixels()
    }

    pub fn height(&self) -> u32 {
        self.store.height()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
