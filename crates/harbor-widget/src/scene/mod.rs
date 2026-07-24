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

        let mut new_items = Vec::new();
        let mut seen_ids: HashSet<u64> = HashSet::new();
        let mut next_id = self.next_id;

        for mut item in incoming {
            if item.id != 0 && old_ids.contains(&item.id) {
                let old = old_by_id[&item.id];
                seen_ids.insert(item.id);
                if old.paint_order != item.paint_order || old.primitive != item.primitive {
                    modified.push(item.clone());
                }
                new_items.push(item);
            } else {
                let new_id = next_id;
                next_id += 1;
                item.id = new_id;
                seen_ids.insert(new_id);
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

    /// Returns the current pending delta (produced by last diff), if any.
    /// After calling diff(), callers can inspect the result.
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
}
