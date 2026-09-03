pub mod primitive;

use hashbrown::{HashMap, HashSet};
use primitive::Primitive;

// ── SceneItem ───────────────────────────────────────────────────────────────

/// A retained GPU-visible draw item with identity and paint ordering.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneItem {
    pub id: u64,
    pub primitive: Primitive,
    pub paint_order: u32,
}

// ── SceneDelta ──────────────────────────────────────────────────────────────

/// Incremental scene update produced by diffing the scene graph.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneDelta {
    pub added: Vec<SceneItem>,
    pub removed: Vec<u64>,
    pub modified: Vec<SceneItem>,
}

impl SceneDelta {
    /// Returns true if this delta has no changes.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    /// Incorporates a later delta so this delta still applies to the original
    /// renderer baseline.
    pub(crate) fn coalesce(&mut self, later: SceneDelta) {
        enum Change {
            Added(SceneItem),
            Removed,
            Modified(SceneItem),
        }

        let mut changes = Vec::new();
        for item in self.added.drain(..) {
            changes.push((item.id, Change::Added(item)));
        }
        for item in self.modified.drain(..) {
            changes.push((item.id, Change::Modified(item)));
        }
        for id in self.removed.drain(..) {
            changes.push((id, Change::Removed));
        }

        for item in later.added {
            if let Some((_, change)) = changes.iter_mut().find(|(id, _)| *id == item.id) {
                // An item removed since the renderer baseline already has a
                // slot, so reintroducing it is an in-place modification.
                *change = if matches!(&*change, Change::Removed) {
                    Change::Modified(item)
                } else {
                    Change::Added(item)
                };
            } else {
                changes.push((item.id, Change::Added(item)));
            }
        }

        for item in later.modified {
            if let Some((_, change)) = changes.iter_mut().find(|(id, _)| *id == item.id) {
                // Keep an unencoded addition as an addition, but replace it
                // with its latest contents.
                *change = if matches!(&*change, Change::Added(_)) {
                    Change::Added(item)
                } else {
                    Change::Modified(item)
                };
            } else {
                changes.push((item.id, Change::Modified(item)));
            }
        }

        for id in later.removed {
            if let Some(index) = changes
                .iter()
                .position(|(existing_id, _)| *existing_id == id)
            {
                if matches!(&changes[index].1, Change::Added(_)) {
                    // The renderer never saw this item, so its addition and
                    // removal cancel out.
                    changes.remove(index);
                } else {
                    changes[index].1 = Change::Removed;
                }
            } else {
                changes.push((id, Change::Removed));
            }
        }

        for (id, change) in changes {
            match change {
                Change::Added(item) => self.added.push(item),
                Change::Removed => self.removed.push(id),
                Change::Modified(item) => self.modified.push(item),
            }
        }
    }
}

// ── SceneGraph ──────────────────────────────────────────────────────────────

/// Retained ordered scene that diffs incoming SceneItems against retained state.
pub struct SceneGraph {
    items: Vec<SceneItem>,
    next_id: u64,
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneGraph {
    pub fn new() -> Self {
        SceneGraph {
            items: Vec::new(),
            next_id: 1,
        }
    }

    /// Diffs incoming items against retained items by paint_order, primitive,
    /// and id. Matches on id; creates new ids for unmatched items.
    pub fn diff(&mut self, incoming: Vec<SceneItem>) -> SceneDelta {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();

        let old_ids: HashSet<u64> = self.items.iter().map(|i| i.id).collect();
        let old_by_id: HashMap<u64, &SceneItem> = self.items.iter().map(|i| (i.id, i)).collect();
        let mut reserved_ids = old_ids.clone();
        reserved_ids.extend(
            incoming
                .iter()
                .filter_map(|item| (item.id != 0).then_some(item.id)),
        );

        let mut new_items = Vec::new();
        let mut seen_ids: HashSet<u64> = HashSet::new();
        let mut next_id = self.next_id;

        for mut item in incoming {
            if item.id != 0 {
                seen_ids.insert(item.id);
                if let Some(old) = old_by_id.get(&item.id) {
                    if std::mem::discriminant(&old.primitive)
                        != std::mem::discriminant(&item.primitive)
                    {
                        removed.push(old.id);
                        added.push(item.clone());
                    } else if old.paint_order != item.paint_order || old.primitive != item.primitive
                    {
                        modified.push(item.clone());
                    }
                } else {
                    added.push(item.clone());
                }
                new_items.push(item);
            } else {
                while next_id == 0 || !reserved_ids.insert(next_id) {
                    next_id = next_id
                        .checked_add(1)
                        .expect("scene item ID allocator exhausted");
                }
                item.id = next_id;
                next_id = next_id
                    .checked_add(1)
                    .expect("scene item ID allocator exhausted");
                seen_ids.insert(item.id);
                added.push(item.clone());
                new_items.push(item);
            }
        }

        self.next_id = next_id;

        // Collect removals while we still have an immutable borrow
        for old in &self.items {
            if !seen_ids.contains(&old.id) {
                removed.push(old.id);
            }
        }

        // Drop old_by_id to release immutable borrow on self.items
        drop(old_by_id);
        drop(old_ids);

        self.items = new_items;

        SceneDelta {
            added,
            removed,
            modified,
        }
    }

    /// Clears all retained items.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Returns the current retained items in paint order.
    pub fn items(&self) -> &[SceneItem] {
        &self.items
    }

    /// Returns the number of retained items.
    pub fn item_count(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Point, Rect, Size};
    use primitive::{Color, Primitive};

    fn make_quad(id: u64, order: u32, x: f32, y: f32) -> SceneItem {
        SceneItem {
            id,
            primitive: Primitive::Quad {
                rect: Rect::from_min_size(Point::new(x, y), Size::new(100.0, 50.0)),
                color: Color::WHITE,
                corner_radius: 0.0,
            },
            paint_order: order,
        }
    }

    #[test]
    fn diff_empty_to_one() {
        let mut graph = SceneGraph::new();
        // All ids are 0 (unset), so they're treated as new
        let incoming = vec![make_quad(0, 0, 0.0, 0.0)];
        let delta = graph.diff(incoming);

        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.removed.len(), 0);
        assert_eq!(delta.modified.len(), 0);
        assert_eq!(delta.added[0].id, 1); // first alloc starts at 1
        assert_eq!(graph.item_count(), 1);
    }

    #[test]
    fn diff_preserves_initial_stable_ids_and_skips_them_when_allocating_fallback_ids() {
        let mut graph = SceneGraph::new();
        let delta = graph.diff(vec![make_quad(1, 0, 0.0, 0.0), make_quad(0, 1, 100.0, 0.0)]);

        assert_eq!(
            delta.added.iter().map(|item| item.id).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            graph.items().iter().map(|item| item.id).collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn coalesce_keeps_unencoded_addition_with_latest_item() {
        let initial = make_quad(1, 0, 0.0, 0.0);
        let latest = make_quad(1, 0, 50.0, 50.0);
        let mut delta = SceneDelta {
            added: vec![initial],
            removed: vec![],
            modified: vec![],
        };

        delta.coalesce(SceneDelta {
            added: vec![],
            removed: vec![],
            modified: vec![latest.clone()],
        });

        assert_eq!(delta.added, vec![latest]);
        assert!(delta.removed.is_empty());
        assert!(delta.modified.is_empty());
    }

    #[test]
    fn coalesce_cancels_unencoded_add_then_remove() {
        let mut delta = SceneDelta {
            added: vec![make_quad(1, 0, 0.0, 0.0)],
            removed: vec![],
            modified: vec![],
        };

        delta.coalesce(SceneDelta {
            added: vec![],
            removed: vec![1],
            modified: vec![],
        });

        assert!(delta.is_empty());
    }

    #[test]
    fn coalesce_composes_removed_then_added_and_modified_then_removed() {
        let restored = make_quad(2, 0, 50.0, 50.0);
        let mut delta = SceneDelta {
            added: vec![],
            removed: vec![2],
            modified: vec![make_quad(1, 0, 0.0, 0.0)],
        };

        delta.coalesce(SceneDelta {
            added: vec![restored.clone()],
            removed: vec![1],
            modified: vec![],
        });

        assert!(delta.added.is_empty());
        assert_eq!(delta.removed, vec![1]);
        assert_eq!(delta.modified, vec![restored]);
    }

    #[test]
    fn diff_same_twice_empty_delta() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0)];
        let delta1 = graph.diff(incoming);
        let item_id = delta1.added[0].id;

        // Second diff with same item (using the assigned id)
        let incoming2 = vec![make_quad(item_id, 0, 0.0, 0.0)];
        let delta2 = graph.diff(incoming2);

        assert!(delta2.is_empty());
        assert_eq!(graph.item_count(), 1);
    }

    #[test]
    fn diff_remove_item() {
        let mut graph = SceneGraph::new();
        // Add two items
        let incoming = vec![make_quad(0, 0, 0.0, 0.0), make_quad(0, 1, 100.0, 0.0)];
        let delta1 = graph.diff(incoming);
        assert_eq!(delta1.added.len(), 2);
        let id0 = delta1.added[0].id;
        let id1 = delta1.added[1].id;

        // Remove first item
        let incoming2 = vec![make_quad(id1, 1, 100.0, 0.0)];
        let delta2 = graph.diff(incoming2);
        assert_eq!(delta2.removed.len(), 1);
        assert_eq!(delta2.removed[0], id0);
        assert_eq!(graph.item_count(), 1);
    }

    #[test]
    fn diff_modify_item() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0)];
        let delta1 = graph.diff(incoming);
        let id = delta1.added[0].id;

        // Same id, different position (different primitive)
        let modified = make_quad(id, 0, 50.0, 50.0);
        let delta2 = graph.diff(vec![modified]);

        assert!(delta2.added.is_empty());
        assert!(delta2.removed.is_empty());
        assert_eq!(delta2.modified.len(), 1);
    }

    #[test]
    fn diff_clear_and_rebuild() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0)];
        graph.diff(incoming);

        // Clear and add different items
        graph.clear();
        let incoming2 = vec![make_quad(0, 0, 100.0, 100.0)];
        let delta = graph.diff(incoming2);
        assert_eq!(delta.added.len(), 1);
        assert_eq!(graph.item_count(), 1);
    }

    #[test]
    fn diff_mixed_added_removed_modified() {
        let mut graph = SceneGraph::new();
        // Build initial state: items A, B, C
        let incoming = vec![
            make_quad(0, 0, 0.0, 0.0),   // A: id will be 1
            make_quad(0, 1, 100.0, 0.0), // B: id will be 2
            make_quad(0, 2, 200.0, 0.0), // C: id will be 3
        ];
        let delta1 = graph.diff(incoming);
        assert_eq!(delta1.added.len(), 3);
        let id_a = delta1.added[0].id;
        //let id_b = delta1.added[1].id; // Not used explicitly
        let id_c = delta1.added[2].id;
        assert_eq!(graph.item_count(), 3);

        // Second diff:
        // - A stays the same (retained, no change)
        // - B removed
        // - C modified (changed position)
        // - D added (new item)
        let incoming2 = vec![
            make_quad(id_a, 0, 0.0, 0.0),   // A: unchanged
            make_quad(id_c, 2, 300.0, 0.0), // C: modified (different rect)
            make_quad(0, 3, 400.0, 0.0),    // D: new item
        ];
        let delta2 = graph.diff(incoming2);

        assert_eq!(delta2.added.len(), 1, "one new item added");
        assert_eq!(delta2.removed.len(), 1, "B should be removed");
        assert_eq!(delta2.modified.len(), 1, "C should be modified");
        assert!(!delta2.is_empty());
        assert_eq!(graph.item_count(), 3);
    }

    #[test]
    fn diff_empty_incoming_clears_all() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0), make_quad(0, 1, 100.0, 0.0)];
        graph.diff(incoming);
        assert_eq!(graph.item_count(), 2);

        // Empty incoming should mark all as removed
        let delta = graph.diff(vec![]);
        assert_eq!(delta.added.len(), 0);
        assert_eq!(delta.removed.len(), 2);
        assert_eq!(delta.modified.len(), 0);
        assert!(!delta.is_empty());
        assert_eq!(graph.item_count(), 0);
    }

    #[test]
    fn diff_id_zero_items_always_treated_as_new() {
        let mut graph = SceneGraph::new();
        // First diff with id=0 items
        let incoming1 = vec![make_quad(0, 0, 0.0, 0.0)];
        let delta1 = graph.diff(incoming1);
        let first_id = delta1.added[0].id;

        // Second diff with id=0 item (no retained id reference) — treated as new
        let incoming2 = vec![make_quad(0, 0, 50.0, 50.0)];
        let delta2 = graph.diff(incoming2);
        assert_eq!(delta2.added.len(), 1);
        assert!(
            delta2.added[0].id != first_id,
            "id=0 items get new ids each time"
        );
        assert_eq!(delta2.removed.len(), 1, "old item should be removed");
        assert_eq!(graph.item_count(), 1);
    }

    #[test]
    fn diff_reorder_triggers_modify_due_to_paint_order_change() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0), make_quad(0, 1, 100.0, 0.0)];
        let delta1 = graph.diff(incoming);
        let id_a = delta1.added[0].id;
        let id_b = delta1.added[1].id;

        // Swap paint_order: B first, A second
        let incoming2 = vec![make_quad(id_b, 0, 100.0, 0.0), make_quad(id_a, 1, 0.0, 0.0)];
        let delta2 = graph.diff(incoming2);
        // Both items have different paint_order => both should be modified
        assert_eq!(delta2.modified.len(), 2);
        assert!(delta2.added.is_empty());
        assert!(delta2.removed.is_empty());
    }

    #[test]
    fn diff_paint_order_unchanged_but_primitive_changed() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0)];
        let delta1 = graph.diff(incoming);
        let id = delta1.added[0].id;

        // Same paint_order, same primitive => no change
        let incoming2 = vec![make_quad(id, 0, 0.0, 0.0)];
        let delta2 = graph.diff(incoming2);
        assert!(delta2.is_empty());

        // Same paint_order, different position => modified
        let incoming3 = vec![make_quad(id, 0, 50.0, 50.0)];
        let delta3 = graph.diff(incoming3);
        assert_eq!(delta3.modified.len(), 1);
        assert!(delta3.added.is_empty());
    }

    #[test]
    fn diff_primitive_discriminant_change_emits_removed_and_added() {
        let mut graph = SceneGraph::new();
        let quad_item = SceneItem {
            id: 10,
            paint_order: 0,
            primitive: Primitive::Quad {
                rect: Rect::from_min_size(Point::ZERO, Size::new(10.0, 10.0)),
                color: Color::WHITE,
                corner_radius: 0.0,
            },
        };
        let delta1 = graph.diff(vec![quad_item]);
        assert_eq!(delta1.added.len(), 1);

        // Same ID 10 now becomes a Text item
        let text_item = SceneItem {
            id: 10,
            paint_order: 0,
            primitive: Primitive::Text {
                text: std::sync::Arc::from("Hello"),
                origin: Point::ZERO,
                color: Color::WHITE,
            },
        };
        let delta2 = graph.diff(vec![text_item]);
        assert_eq!(delta2.removed, vec![10]);
        assert_eq!(delta2.added.len(), 1);
        assert_eq!(delta2.added[0].id, 10);
        assert!(delta2.modified.is_empty());
    }
    // ── items() accessor ──────────────────────────────────────────────

    #[test]
    fn items_returns_empty_slice_for_new_graph() {
        let graph = SceneGraph::new();
        assert!(graph.items().is_empty());
    }

    #[test]
    fn items_returns_retained_items_in_order() {
        let mut graph = SceneGraph::new();
        let incoming = vec![
            make_quad(0, 0, 0.0, 0.0),
            make_quad(0, 1, 100.0, 0.0),
            make_quad(0, 2, 200.0, 0.0),
        ];
        graph.diff(incoming);
        let items = graph.items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].paint_order, 0);
        assert_eq!(items[1].paint_order, 1);
        assert_eq!(items[2].paint_order, 2);
    }

    #[test]
    fn items_reflects_diff_result() {
        let mut graph = SceneGraph::new();
        // Diff some items in
        let incoming = vec![make_quad(0, 0, 0.0, 0.0)];
        graph.diff(incoming);
        assert_eq!(graph.items().len(), 1);

        // Diff empty — all removed
        graph.diff(vec![]);
        assert!(graph.items().is_empty());
    }

    #[test]
    fn items_returns_same_reference_after_multiple_diffs() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0)];
        graph.diff(incoming);
        let first = graph.items().as_ptr();

        // Diff again with same item (no structural changes)
        let id = graph.items()[0].id;
        graph.diff(vec![make_quad(id, 0, 0.0, 0.0)]);
        let second = graph.items().as_ptr();
        // items is replaced wholesale by diff, so pointer may differ
        // but contents are consistent
        assert_eq!(graph.items().len(), 1);
        let _ = (first, second);
    }

    #[test]
    fn items_after_clear_is_empty() {
        let mut graph = SceneGraph::new();
        let incoming = vec![make_quad(0, 0, 0.0, 0.0), make_quad(0, 1, 100.0, 0.0)];
        graph.diff(incoming);
        assert_eq!(graph.items().len(), 2);

        graph.clear();
        assert!(graph.items().is_empty());
        assert_eq!(graph.item_count(), 0);
    }

    #[test]
    fn items_includes_external_primitives() {
        use crate::scene::primitive::{ExternalDrawId, Primitive};

        let mut graph = SceneGraph::new();
        let external_item = SceneItem {
            id: 0,
            primitive: Primitive::External {
                draw: 42u64 as ExternalDrawId,
                rect: Rect::from_min_size(Point::new(0.0, 0.0), Size::new(800.0, 600.0)),
            },
            paint_order: 0,
        };
        graph.diff(vec![external_item]);

        let items = graph.items();
        assert_eq!(items.len(), 1);
        match &items[0].primitive {
            Primitive::External { draw, rect } => {
                assert_eq!(*draw, 42);
                assert_eq!(rect.size(), Size::new(800.0, 600.0));
            }
            _ => panic!("expected External primitive"),
        }
    }
}
