use super::{FiberArena, FiberId};
use crate::scene::SceneItem;

// ── Paint ────────────────────────────────────────────────────────────────────

static NEXT_SCENE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_scene_id() -> u64 {
    NEXT_SCENE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Walks the fiber tree top-down, accumulates Primitives from each fiber
/// via `AnyView::paint_primitives`, assigns incrementing paint_order and
/// scene ids, and returns a flat Vec of SceneItems.
pub(crate) fn paint_fiber(arena: &FiberArena, id: FiberId, base_order: u32) -> Vec<SceneItem> {
    let mut items = Vec::new();
    let mut order = base_order;

    let fiber = match arena.get(id) {
        Some(f) => f,
        None => return items,
    };

    let rect = fiber.layout_rect;
    let children = fiber.children.clone();
    let _has_view = fiber.view.is_some();

    // Collect self primitives
    if let Some(ref view) = fiber.view
        && let Some(r) = rect
    {
        for prim in view.paint_primitives(r) {
            items.push(SceneItem {
                id: next_scene_id(),
                primitive: prim,
                paint_order: order,
            });
            order += 1;
        }
    }

    // Collect children (in order)
    for child_id in &children {
        let child_items = paint_fiber(arena, *child_id, order);
        order += child_items.len() as u32;
        items.extend(child_items);
    }

    items
}

