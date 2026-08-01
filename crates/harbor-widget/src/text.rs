//! Shared text types consumed by Widget text primitives.

use crate::layout::Point;
use crate::scene::primitive::TextRunId;
use hashbrown::HashMap;
use std::cell::RefCell;

pub use harbor_text::{AtlasGlyph, AtlasUv, TextMetrics};

// ── Thread-local metrics ────────────────────────────────────────────────────

// Thread-local TextMetrics for widget intrinsic size computation.
thread_local! {
    static CURRENT_METRICS: RefCell<Option<TextMetrics>> = const { RefCell::new(None) };
}

/// Sets the TextMetrics for widget layout on this thread.
/// Widgets like TextLabel read this during `intrinsic_size`.
pub fn set_current_metrics(metrics: TextMetrics) {
    CURRENT_METRICS.with(|m| *m.borrow_mut() = Some(metrics));
}

/// Returns a copy of the current TextMetrics, if set.
pub fn current_metrics() -> Option<TextMetrics> {
    CURRENT_METRICS.with(|m| *m.borrow())
}

// ── Thread-local glyph lookup ──────────────────────────────────────────────

type CurrentGlyphFn = RefCell<Option<Box<dyn Fn(char) -> Option<AtlasGlyph>>>>;

thread_local! {
    static CURRENT_GLYPH_FN: CurrentGlyphFn = const { RefCell::new(None) };
}

/// Sets the glyph lookup function for text run registration on this thread.
pub fn set_current_glyph_fn(f: Box<dyn Fn(char) -> Option<AtlasGlyph>>) {
    CURRENT_GLYPH_FN.with(|g| *g.borrow_mut() = Some(f));
}

/// Invokes the current glyph function, if set.
pub fn current_glyph(ch: char) -> Option<AtlasGlyph> {
    CURRENT_GLYPH_FN.with(|g| g.borrow().as_ref().map(|f| f(ch)).unwrap_or(None))
}

// ── GlyphLookup ─────────────────────────────────────────────────────────────

/// Function type for looking up an AtlasGlyph for a character.
pub type GlyphFn<'a> = dyn Fn(char) -> Option<AtlasGlyph> + 'a;

// ── GlyphLayout ────────────────────────────────────────────────────────────

/// Per-glyph layout data produced by the TextRunCache.
/// Does not include color — color is applied by the renderer.
#[derive(Clone, Debug)]
pub struct GlyphLayout {
    /// Offset from the text run origin (dp).
    pub origin: Point,
    /// Glyph width in logical pixels.
    pub width: f32,
    /// Glyph height in logical pixels.
    pub height: f32,
    /// Atlas UV rectangle.
    pub uv_left: f32,
    pub uv_top: f32,
    pub uv_right: f32,
    pub uv_bottom: f32,
}

// ── TextRunData ─────────────────────────────────────────────────────────────

/// Cached glyph layout data for a single text run.
#[derive(Clone, Debug)]
pub struct TextRunData {
    pub glyphs: Vec<GlyphLayout>,
}

// ── TextRunCache ────────────────────────────────────────────────────────────

/// Caches positioned glyph data keyed by [`TextRunId`].
///
/// Populated during the paint pass when widgets register text runs,
/// and consumed during encode by the [`TextRenderer`] to produce
/// GPU glyph instances.
pub struct TextRunCache {
    runs: HashMap<TextRunId, TextRunData>,
    next_id: TextRunId,
    /// Deduplication: maps (text_hash, metrics_hash) → existing TextRunId.
    dedup: HashMap<(u64, u64), TextRunId>,
}

impl Default for TextRunCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRunCache {
    pub fn new() -> Self {
        TextRunCache {
            runs: HashMap::new(),
            next_id: 1,
            dedup: HashMap::new(),
        }
    }

    /// Registers a text string and returns a `TextRunId`.
    ///
    /// Computes monospace glyph positions by advancing `cell_width` per
    /// character. Characters with no glyph (closure returns `None`) are
    /// skipped — the pen advances but no glyph quad is emitted.
    ///
    /// If the same (text, metrics) pair was previously registered,
    /// returns the existing `TextRunId` instead of recomputing.
    pub fn register(
        &mut self,
        text: &str,
        metrics: &TextMetrics,
        glyph_fn: &GlyphFn<'_>,
    ) -> TextRunId {
        let key = Self::dedup_key(text, metrics);
        if let Some(&existing_id) = self.dedup.get(&key) {
            return existing_id;
        }

        let id = self.next_id;
        self.next_id += 1;
        self.register_with_id(id, text, metrics, glyph_fn);
        self.dedup.insert(key, id);
        id
    }

    /// Registers a text string under a specific `TextRunId`.
    /// Used when the id is assigned externally (e.g., by the widget paint pass).
    pub fn register_with_id(
        &mut self,
        id: TextRunId,
        text: &str,
        metrics: &TextMetrics,
        glyph_fn: &GlyphFn<'_>,
    ) {
        let mut glyphs = Vec::with_capacity(text.len());
        let mut pen_x: f32 = 0.0;
        let cell_w = metrics.cell_width;
        let baseline_y = metrics.ascent;

        for ch in text.chars() {
            if let Some(g) = glyph_fn(ch)
                && g.width > 0
                && g.height > 0
            {
                let origin = Point::new(
                    pen_x + g.bearing_x as f32,
                    baseline_y - g.bearing_y as f32 - g.height as f32,
                );
                glyphs.push(GlyphLayout {
                    origin,
                    width: g.width as f32,
                    height: g.height as f32,
                    uv_left: g.uv.left,
                    uv_top: g.uv.top,
                    uv_right: g.uv.right,
                    uv_bottom: g.uv.bottom,
                });
            }
            pen_x += cell_w;
        }

        self.runs.insert(id, TextRunData { glyphs });
    }

    /// Returns the cached glyph data for a run.
    pub fn get(&self, id: TextRunId) -> Option<&TextRunData> {
        self.runs.get(&id)
    }

    /// Clears all cached runs.
    pub fn clear(&mut self) {
        self.runs.clear();
        self.dedup.clear();
    }

    /// Returns the number of cached runs.
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Computes a dedup key from text content and metrics.
    fn dedup_key(text: &str, metrics: &TextMetrics) -> (u64, u64) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        let text_hash = h.finish();

        let mut h = std::collections::hash_map::DefaultHasher::new();
        metrics.cell_width.to_bits().hash(&mut h);
        metrics.line_height.to_bits().hash(&mut h);
        metrics.ascent.to_bits().hash(&mut h);
        let metrics_hash = h.finish();

        (text_hash, metrics_hash)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metrics() -> TextMetrics {
        TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 0.0,
            underline_thickness: 1.5,
            strikethrough_position: 0.0,
            strikethrough_thickness: 1.5,
        }
    }

    fn test_glyph_fn(_ch: char) -> Option<AtlasGlyph> {
        Some(AtlasGlyph {
            key: harbor_text::GlyphKey::new(
                harbor_text::FaceId::PRIMARY,
                harbor_text::GlyphId::new(0),
                harbor_text::FontSize::new(1.0).expect("valid test font size"),
                harbor_text::FontStyle::REGULAR,
            ),
            uv: AtlasUv {
                left: 0.0,
                top: 0.0,
                right: 0.5,
                bottom: 0.5,
            },
            width: 8,
            height: 16,
            bearing_x: 0,
            bearing_y: 0,
            atlas_x: 0,
            atlas_y: 0,
        })
    }

    fn empty_glyph_fn(_ch: char) -> Option<AtlasGlyph> {
        None
    }

    #[test]
    fn empty_string_returns_zero_glyphs() {
        let mut cache = TextRunCache::new();
        let id = cache.register("", &test_metrics(), &test_glyph_fn);
        let run = cache.get(id).unwrap();
        assert!(run.glyphs.is_empty());
    }

    #[test]
    fn single_char_returns_one_glyph() {
        let mut cache = TextRunCache::new();
        let id = cache.register("A", &test_metrics(), &test_glyph_fn);
        let run = cache.get(id).unwrap();
        assert_eq!(run.glyphs.len(), 1);
    }

    #[test]
    fn multi_char_advances_pen() {
        let mut cache = TextRunCache::new();
        let id = cache.register("ABC", &test_metrics(), &test_glyph_fn);
        let run = cache.get(id).unwrap();
        assert_eq!(run.glyphs.len(), 3);
        // Second glyph should be at x = cell_width
        assert!((run.glyphs[1].origin.x - 10.0).abs() < 0.01);
        // Third glyph at 2 * cell_width
        assert!((run.glyphs[2].origin.x - 20.0).abs() < 0.01);
    }

    #[test]
    fn missing_glyph_skips_but_advances() {
        let mut cache = TextRunCache::new();
        let id = cache.register("A", &test_metrics(), &empty_glyph_fn);
        let run = cache.get(id).unwrap();
        assert!(run.glyphs.is_empty());
    }

    #[test]
    fn different_runs_have_different_ids() {
        let mut cache = TextRunCache::new();
        let id1 = cache.register("A", &test_metrics(), &test_glyph_fn);
        let id2 = cache.register("B", &test_metrics(), &test_glyph_fn);
        assert_ne!(id1, id2);
    }

    #[test]
    fn get_stale_id_returns_none() {
        let cache = TextRunCache::new();
        assert!(cache.get(999).is_none());
    }

    #[test]
    fn clear_removes_all_runs() {
        let mut cache = TextRunCache::new();
        cache.register("A", &test_metrics(), &test_glyph_fn);
        assert!(!cache.is_empty());
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn text_run_cache_new_is_empty() {
        let cache = TextRunCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn zero_width_glyph_is_skipped() {
        let fn_zero_width = |_ch: char| -> Option<AtlasGlyph> {
            Some(AtlasGlyph {
                key: harbor_text::GlyphKey::new(
                    harbor_text::FaceId::PRIMARY,
                    harbor_text::GlyphId::new(0),
                    harbor_text::FontSize::new(1.0).expect("valid test font size"),
                    harbor_text::FontStyle::REGULAR,
                ),
                uv: AtlasUv {
                    left: 0.0,
                    top: 0.0,
                    right: 1.0,
                    bottom: 1.0,
                },
                width: 0,
                height: 16,
                bearing_x: 1,
                bearing_y: 2,
                atlas_x: 0,
                atlas_y: 0,
            })
        };
        let mut cache = TextRunCache::new();
        let id = cache.register("X", &test_metrics(), &fn_zero_width);
        let run = cache.get(id).unwrap();
        // width=0 should be skipped despite height>0
        assert!(run.glyphs.is_empty());
    }
}
