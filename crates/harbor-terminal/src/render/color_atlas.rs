//! Terminal-owned presentation storage. No model/native handles; scalar atlas is unchanged.
use harbor_text::{
    PresentationGeneration, PresentationTile, SEQUENCE_MAX_UTF16, SEQUENCE_PIXEL_BUDGET,
    SEQUENCE_TILE_SIDE, SequenceOutcome, SequencePresentation, SequenceRequest,
};
use std::collections::{HashMap, HashSet};

pub(super) const COLOR_ATLAS_SIDE: u32 = 1024;
pub(super) const COLOR_CACHE_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub(super) struct Placement {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CachedOutcome {
    Complete(harbor_text::CompleteKind),
    Unsupported(harbor_text::UnsupportedReason),
    Overflow,
}

pub(super) struct CachedPresentation {
    pub tile: Option<PresentationTile>,
    pub placement: Option<Placement>,
    pub attempted: bool,
    pub outcome: CachedOutcome,
}

/// Keeps only the bounded currently requested set. Tiles are owned once here,
/// never retained as native Arc results (which also carry shaping diagnostics).
#[derive(Default)]
pub(super) struct ColorAtlas {
    pub entries: HashMap<SequenceRequest, CachedPresentation>,
    pub pixels: Vec<u8>,
    pub revision: u64,
    generation: Option<PresentationGeneration>,
    x: u32,
    y: u32,
    shelf_height: u32,
    placed: usize,
}

#[derive(Default)]
pub(super) struct AtlasChanges {
    pub full: bool,
    pub added: Vec<SequenceRequest>,
}

impl ColorAtlas {
    pub fn generation_changed(&self, generation: PresentationGeneration) -> bool {
        self.generation != Some(generation)
    }
    pub fn sync(
        &mut self,
        requests: &[SequenceRequest],
        generation: PresentationGeneration,
        mut present: impl FnMut(&SequenceRequest) -> SequencePresentation,
    ) -> AtlasChanges {
        let mut changes = AtlasChanges::default();
        if self.generation != Some(generation) {
            self.entries.clear();
            self.reset();
            self.generation = Some(generation);
            changes.full = true;
        }
        let mut seen = HashSet::new();
        let active: Vec<_> = requests
            .iter()
            .filter(|r| {
                r.source.encode_utf16().count() <= SEQUENCE_MAX_UTF16 && seen.insert((*r).clone())
            })
            .take(COLOR_CACHE_ENTRIES)
            .cloned()
            .collect();
        let before = self.entries.len();
        self.entries.retain(|r, _| active.contains(r));
        if before != self.entries.len() {
            // A previously budget-rejected tile is not a native negative result.
            // Retry it when other active requests release retained pixel storage.
            self.entries
                .retain(|_, e| e.outcome != CachedOutcome::Overflow);
        }
        let retry = before != self.entries.len()
            && self
                .entries
                .values()
                .any(|e| e.tile.is_some() && e.placement.is_none());
        if retry {
            self.reset();
            for e in self.entries.values_mut() {
                e.placement = None;
                e.attempted = false;
            }
            changes.full = true;
        }
        let mut bytes: usize = self
            .entries
            .values()
            .filter_map(|e| e.tile.as_ref())
            .map(|t| t.rgba.len())
            .sum();
        for request in &active {
            if self.entries.contains_key(request) {
                continue;
            }
            let result = present(request);
            let (tile, outcome) = match result.outcome {
                SequenceOutcome::Complete { kind, tile, .. }
                    if result.generation == generation
                        && valid_tile(&tile)
                        && bytes + tile.rgba.len() <= SEQUENCE_PIXEL_BUDGET =>
                {
                    (Some(tile), CachedOutcome::Complete(kind))
                }
                SequenceOutcome::Unsupported(reason) => (None, CachedOutcome::Unsupported(reason)),
                _ => (None, CachedOutcome::Overflow),
            };
            bytes += tile.as_ref().map_or(0, |t| t.rgba.len());
            self.entries.insert(
                request.clone(),
                CachedPresentation {
                    tile,
                    placement: None,
                    attempted: false,
                    outcome,
                },
            );
        }
        let pending: Vec<_> = active
            .iter()
            .filter(|r| {
                let e = &self.entries[*r];
                e.tile.is_some() && !e.attempted
            })
            .cloned()
            .collect();
        for request in pending {
            let bounds = self.entries[&request].tile.as_ref().unwrap().bounds;
            if let Some(p) = self.pack(bounds.width, bounds.height) {
                self.blit(&request, p);
                changes.added.push(request);
            } else {
                // All previously issued UVs are invalid. Rebuild every retained
                // reference in one prepare; anything that still does not fit falls back.
                self.reset();
                for e in self.entries.values_mut() {
                    e.placement = None;
                    e.attempted = true;
                }
                for r in &active {
                    if let Some(t) = self.entries[r].tile.as_ref() {
                        let bounds = t.bounds;
                        if let Some(p) = self.pack(bounds.width, bounds.height) {
                            self.blit(r, p);
                        }
                    }
                }
                changes.full = true;
                changes.added.clear();
                break;
            }
        }
        changes
    }

    fn reset(&mut self) {
        self.pixels.fill(0);
        self.x = 0;
        self.y = 0;
        self.shelf_height = 0;
        self.placed = 0;
        self.revision = self.revision.wrapping_add(1);
    }

    fn pack(&mut self, width: u32, height: u32) -> Option<Placement> {
        let (w, h) = (width + 2, height + 2); // Transparent filter gutter.
        if self.placed >= COLOR_CACHE_ENTRIES || w > COLOR_ATLAS_SIDE || h > COLOR_ATLAS_SIDE {
            return None;
        }
        if self.x + w > COLOR_ATLAS_SIDE {
            self.y += self.shelf_height;
            self.x = 0;
            self.shelf_height = 0;
        }
        if self.y + h > COLOR_ATLAS_SIDE {
            return None;
        }
        let p = Placement {
            x: self.x + 1,
            y: self.y + 1,
        };
        self.x += w;
        self.shelf_height = self.shelf_height.max(h);
        self.placed += 1;
        Some(p)
    }

    fn blit(&mut self, request: &SequenceRequest, p: Placement) {
        if self.pixels.is_empty() {
            self.pixels
                .resize((COLOR_ATLAS_SIDE * COLOR_ATLAS_SIDE * 4) as usize, 0);
        }
        let e = self.entries.get_mut(request).unwrap();
        let tile = e.tile.as_ref().unwrap();
        let width = tile.bounds.width as usize * 4;
        for row in 0..tile.bounds.height as usize {
            let dst = ((p.y as usize + row) * COLOR_ATLAS_SIDE as usize + p.x as usize) * 4;
            self.pixels[dst..dst + width]
                .copy_from_slice(&tile.rgba[row * width..(row + 1) * width]);
        }
        e.placement = Some(p);
        e.attempted = true;
    }
}

fn valid_tile(t: &PresentationTile) -> bool {
    t.bounds.width > 0
        && t.bounds.height > 0
        && t.bounds.width <= SEQUENCE_TILE_SIDE
        && t.bounds.height <= SEQUENCE_TILE_SIDE
        && t.rgba.len() == (t.bounds.width * t.bounds.height * 4) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_text::{CompleteKind, FontSize, TileBounds, UnsupportedReason};
    fn generation() -> PresentationGeneration {
        PresentationGeneration {
            session: 1,
            revision: 0,
        }
    }
    fn request(n: usize) -> SequenceRequest {
        SequenceRequest::new(
            format!("😀{n}"),
            FontSize::new(16.0).unwrap(),
            FontSize::new(96.0).unwrap(),
        )
    }
    fn tile(size: u32) -> SequencePresentation {
        SequencePresentation {
            generation: generation(),
            outcome: SequenceOutcome::Complete {
                kind: CompleteKind::Color,
                tile: PresentationTile {
                    bounds: TileBounds {
                        left: -4,
                        top: -8,
                        width: size,
                        height: size,
                    },
                    image_formats: 0,
                    rgba: vec![128; (size * size * 4) as usize],
                },
                runs: vec![],
            },
        }
    }
    #[test]
    fn lazy_warm_negative_and_generation_cache() {
        let mut a = ColorAtlas::default();
        a.sync(&[], generation(), |_| panic!("ordinary should not shape"));
        assert!(a.pixels.is_empty());
        let negative = || SequencePresentation {
            generation: generation(),
            outcome: SequenceOutcome::Unsupported(UnsupportedReason::MissingGlyph),
        };
        a.sync(&[request(0)], generation(), |_| negative());
        assert!(a.pixels.is_empty());
        assert_eq!(
            a.entries[&request(0)].outcome,
            CachedOutcome::Unsupported(UnsupportedReason::MissingGlyph)
        );
        a.sync(&[request(0)], generation(), |_| panic!("negative hit"));
        let next = PresentationGeneration {
            session: 2,
            revision: 0,
        };
        let mut calls = 0;
        a.sync(&[request(0)], next, |_| {
            calls += 1;
            let mut r = negative();
            r.generation = next;
            r
        });
        assert_eq!(calls, 1);
    }
    #[test]
    fn churn_overflow_revises_uvs_and_bounds_every_storage() {
        let mut a = ColorAtlas::default();
        for start in 0..20 {
            let req: Vec<_> = (start * 200..start * 200 + 200).map(request).collect();
            let before = a.revision;
            let changes = a.sync(&req, generation(), |_| tile(512));
            assert!(changes.full);
            let revision = a.revision;
            let warm = a.sync(&req, generation(), |_| {
                panic!("overflow must not repeat presentation")
            });
            assert!(!warm.full);
            assert_eq!(a.revision, revision);
            assert!(a.revision > before);
            assert!(a.entries.len() <= COLOR_CACHE_ENTRIES);
            assert!(
                a.entries
                    .values()
                    .filter_map(|e| e.tile.as_ref())
                    .map(|t| t.rgba.len())
                    .sum::<usize>()
                    <= SEQUENCE_PIXEL_BUDGET
            );
            assert_eq!(
                a.pixels.len(),
                (COLOR_ATLAS_SIDE * COLOR_ATLAS_SIDE * 4) as usize
            );
            assert!(a.entries.values().filter(|e| e.placement.is_some()).count() <= 1);
        }
    }
    #[test]
    fn complete_hits_skip_present_and_incremental_add_preserves_uvs() {
        let mut a = ColorAtlas::default();
        a.sync(&[request(0)], generation(), |_| tile(16));
        let p = a.entries[&request(0)].placement.unwrap();
        let revision = a.revision;
        let changes = a.sync(&[request(0), request(1)], generation(), |r| {
            assert_eq!(r, &request(1));
            tile(16)
        });
        assert!(!changes.full);
        assert_eq!(a.revision, revision);
        assert_eq!(a.entries[&request(0)].placement.unwrap().x, p.x);
        a.sync(&[request(0), request(1)], generation(), |_| {
            panic!("warm hit")
        });
    }
    #[test]
    fn repack_relocates_old_retained_tiles_and_clears_obsolete_pixels() {
        let mut a = ColorAtlas::default();
        let initial: Vec<_> = (0..COLOR_CACHE_ENTRIES).map(request).collect();
        a.sync(&initial, generation(), |_| tile(16));
        let old = a.entries[&request(127)].placement.unwrap();
        let revision = a.revision;
        let changes = a.sync(&[request(127), request(128)], generation(), |_| tile(16));
        assert!(changes.full);
        assert!(a.revision > revision);
        let new = a.entries[&request(127)].placement.unwrap();
        assert_ne!((old.x, old.y), (new.x, new.y));
        let pixel = |p: Placement| ((p.y * COLOR_ATLAS_SIDE + p.x) * 4) as usize;
        assert_eq!(a.pixels[pixel(new)], 128);
        assert_eq!(a.pixels[pixel(old)], 0);
        assert_eq!(a.entries.len(), 2);
    }
    #[test]
    fn budget_overflow_retries_after_active_tiles_release_space() {
        let mut a = ColorAtlas::default();
        let requests: Vec<_> = (0..9).map(request).collect();
        a.sync(&requests, generation(), |_| tile(512));
        assert_eq!(a.entries[&request(8)].outcome, CachedOutcome::Overflow);
        let mut calls = 0;
        a.sync(&[request(8)], generation(), |_| {
            calls += 1;
            tile(512)
        });
        assert_eq!(calls, 1);
        assert_eq!(
            a.entries[&request(8)].outcome,
            CachedOutcome::Complete(harbor_text::CompleteKind::Color)
        );
        assert!(a.entries[&request(8)].placement.is_some());
    }
    #[test]
    fn intentional_transparent_complete_tile_does_not_become_unsupported_fallback() {
        let mut a = ColorAtlas::default();
        a.sync(&[request(0)], generation(), |_| {
            let mut result = tile(16);
            if let SequenceOutcome::Complete { kind, tile, .. } = &mut result.outcome {
                *kind = harbor_text::CompleteKind::Monochrome;
                tile.rgba.fill(0);
            }
            result
        });
        assert_eq!(
            a.entries[&request(0)].outcome,
            CachedOutcome::Complete(harbor_text::CompleteKind::Monochrome)
        );
        assert!(a.entries[&request(0)].placement.is_some());
    }
}
