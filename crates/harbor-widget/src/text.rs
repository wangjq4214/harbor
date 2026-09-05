//! Shared text types consumed by Widget text primitives.

use crate::layout::Point;
use crate::scene::primitive::TextRunId;
use hashbrown::{HashMap, HashSet};

pub use harbor_text::{AtlasGlyph, AtlasUv, TextMetrics};

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

/// Cached glyph layout data for a single scene text item.
#[derive(Clone, Debug)]
pub struct TextRunData {
    pub glyphs: Vec<GlyphLayout>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct CachedTextRun {
    text: String,
    metrics: TextMetrics,
    data: TextRunData,
}

// ── TextRunCache ────────────────────────────────────────────────────────────

/// Caches positioned glyph data by stable scene item ID.
///
/// The Runtime prepares this cache from its retained scene immediately before
/// encoding. A run is rebuilt only when its text or metrics change, and stale
/// scene IDs are removed as part of the same preparation pass.
pub struct TextRunCache {
    runs: HashMap<TextRunId, CachedTextRun>,
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
        }
    }

    /// Inserts or updates the run for a stable scene item ID.
    ///
    /// Returns `true` when glyph layout was rebuilt and `false` when the
    pub fn upsert(
        &mut self,
        id: TextRunId,
        text: &str,
        metrics: &TextMetrics,
        glyph_fn: &GlyphFn<'_>,
    ) -> bool {
        let new_data = Self::layout_run(text, metrics, glyph_fn);
        if let Some(run) = self.runs.get_mut(&id) {
            if run.text == text
                && text_metrics_equal(&run.metrics, metrics)
                && glyph_layouts_equal(&run.data, &new_data)
            {
                return false;
            }
            run.text = text.to_owned();
            run.metrics = *metrics;
            run.data = new_data;
            return true;
        }

        self.runs.insert(
            id,
            CachedTextRun {
                text: text.to_owned(),
                metrics: *metrics,
                data: new_data,
            },
        );
        true
    }

    /// Removes every run whose stable scene item ID is no longer live.
    pub fn retain_live_ids(&mut self, live_ids: impl IntoIterator<Item = TextRunId>) {
        let live_ids: HashSet<TextRunId> = live_ids.into_iter().collect();
        self.runs.retain(|id, _| live_ids.contains(id));
    }

    /// Removes one run by its stable scene item ID.
    pub fn remove(&mut self, id: TextRunId) -> Option<TextRunData> {
        self.runs.remove(&id).map(|run| run.data)
    }

    /// Returns the cached glyph data for a stable scene item ID.
    pub fn get(&self, id: TextRunId) -> Option<&TextRunData> {
        self.runs.get(&id).map(|run| &run.data)
    }

    /// Clears all cached runs.
    pub fn clear(&mut self) {
        self.runs.clear();
    }

    /// Returns the number of cached runs.
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    fn layout_run(text: &str, metrics: &TextMetrics, glyph_fn: &GlyphFn<'_>) -> TextRunData {
        let mut glyphs = Vec::with_capacity(text.len());
        let mut pen_x = 0.0;

        for ch in text.chars() {
            if let Some(g) = glyph_fn(ch)
                && g.width > 0
                && g.height > 0
            {
                glyphs.push(GlyphLayout {
                    origin: Point::new(
                        pen_x + g.bearing_x as f32,
                        metrics.ascent - g.bearing_y as f32 - g.height as f32,
                    ),
                    width: g.width as f32,
                    height: g.height as f32,
                    uv_left: g.uv.left,
                    uv_top: g.uv.top,
                    uv_right: g.uv.right,
                    uv_bottom: g.uv.bottom,
                });
            }
            pen_x += metrics.cell_width;
        }

        TextRunData { glyphs }
    }
}

pub(crate) fn text_metrics_equal(left: &TextMetrics, right: &TextMetrics) -> bool {
    left.cell_width.to_bits() == right.cell_width.to_bits()
        && left.line_height.to_bits() == right.line_height.to_bits()
        && left.ascent.to_bits() == right.ascent.to_bits()
        && left.underline_position.to_bits() == right.underline_position.to_bits()
        && left.underline_thickness.to_bits() == right.underline_thickness.to_bits()
        && left.strikethrough_position.to_bits() == right.strikethrough_position.to_bits()
        && left.strikethrough_thickness.to_bits() == right.strikethrough_thickness.to_bits()
}

pub(crate) fn glyph_layouts_equal(left: &TextRunData, right: &TextRunData) -> bool {
    if left.glyphs.len() != right.glyphs.len() {
        return false;
    }
    left.glyphs.iter().zip(right.glyphs.iter()).all(|(a, b)| {
        a.origin == b.origin
            && a.width.to_bits() == b.width.to_bits()
            && a.height.to_bits() == b.height.to_bits()
            && a.uv_left.to_bits() == b.uv_left.to_bits()
            && a.uv_top.to_bits() == b.uv_top.to_bits()
            && a.uv_right.to_bits() == b.uv_right.to_bits()
            && a.uv_bottom.to_bits() == b.uv_bottom.to_bits()
    })
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
        cache.upsert(1, "", &test_metrics(), &test_glyph_fn);
        let run = cache.get(1).unwrap();
        assert!(run.glyphs.is_empty());
    }

    #[test]
    fn single_char_returns_one_glyph() {
        let mut cache = TextRunCache::new();
        cache.upsert(1, "A", &test_metrics(), &test_glyph_fn);
        let run = cache.get(1).unwrap();
        assert_eq!(run.glyphs.len(), 1);
    }

    #[test]
    fn multi_char_advances_pen() {
        let mut cache = TextRunCache::new();
        cache.upsert(1, "ABC", &test_metrics(), &test_glyph_fn);
        let run = cache.get(1).unwrap();
        assert_eq!(run.glyphs.len(), 3);
        assert!((run.glyphs[1].origin.x - 10.0).abs() < 0.01);
        assert!((run.glyphs[2].origin.x - 20.0).abs() < 0.01);
    }

    #[test]
    fn missing_glyph_skips_but_advances() {
        let mut cache = TextRunCache::new();
        cache.upsert(1, "A", &test_metrics(), &empty_glyph_fn);
        let run = cache.get(1).unwrap();
        assert!(run.glyphs.is_empty());
    }

    #[test]
    fn separate_scene_ids_cache_separate_runs() {
        let mut cache = TextRunCache::new();
        cache.upsert(1, "A", &test_metrics(), &test_glyph_fn);
        cache.upsert(2, "B", &test_metrics(), &test_glyph_fn);
        assert_eq!(cache.len(), 2);
        assert!(cache.get(1).is_some());
        assert!(cache.get(2).is_some());
    }

    #[test]
    fn upsert_only_rebuilds_changed_scene_runs() {
        let mut cache = TextRunCache::new();
        let metrics = test_metrics();

        assert!(cache.upsert(42, "A", &metrics, &test_glyph_fn));
        assert!(!cache.upsert(42, "A", &metrics, &test_glyph_fn));
        assert_eq!(cache.len(), 1);

        assert!(cache.upsert(42, "AB", &metrics, &test_glyph_fn));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(42).unwrap().glyphs.len(), 2);
    }

    #[test]
    fn retain_live_ids_releases_removed_scene_runs() {
        let mut cache = TextRunCache::new();
        let metrics = test_metrics();
        cache.upsert(1, "A", &metrics, &test_glyph_fn);
        cache.upsert(2, "B", &metrics, &test_glyph_fn);

        cache.retain_live_ids([2]);

        assert!(cache.get(1).is_none());
        assert!(cache.get(2).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn get_stale_id_returns_none() {
        let cache = TextRunCache::new();
        assert!(cache.get(999).is_none());
    }

    #[test]
    fn clear_removes_all_runs() {
        let mut cache = TextRunCache::new();
        cache.upsert(1, "A", &test_metrics(), &test_glyph_fn);
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
        cache.upsert(1, "X", &test_metrics(), &fn_zero_width);
        let run = cache.get(1).unwrap();
        assert!(run.glyphs.is_empty());
    }
}
