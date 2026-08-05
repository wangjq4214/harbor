use super::{FiberArena, FiberId};
use crate::scene::SceneItem;
use crate::text::TextMetrics;

// ── Paint ────────────────────────────────────────────────────────────────────

/// Walks the fiber tree top-down, accumulates Primitives from each fiber
/// via `AnyView::paint_primitives`, assigns incrementing paint_order, and
/// retains one scene identity per local primitive slot.
pub(crate) fn paint_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    base_order: u32,
    next_scene_id: &mut u64,
    metrics: &TextMetrics,
) -> Vec<SceneItem> {
    let mut items = Vec::new();
    let mut order = base_order;

    let (primitives, children) = {
        let fiber = match arena.get(id) {
            Some(f) => f,
            None => return items,
        };
        let primitives = match (&fiber.view, fiber.layout_rect) {
            (Some(view), Some(rect)) => view.paint_primitives(rect, metrics),
            _ => Vec::new(),
        };
        (primitives, fiber.children.clone())
    };

    let scene_item_ids = {
        let fiber = arena.get_mut(id).expect("fiber was present during paint");
        while fiber.scene_item_ids.len() < primitives.len() {
            fiber.scene_item_ids.push(*next_scene_id);
            *next_scene_id = next_scene_id
                .checked_add(1)
                .expect("scene item ID allocator exhausted");
        }
        fiber.scene_item_ids.truncate(primitives.len());
        fiber.scene_item_ids.clone()
    };

    for (id, primitive) in scene_item_ids.into_iter().zip(primitives) {
        items.push(SceneItem {
            id,
            primitive,
            paint_order: order,
        });
        order += 1;
    }

    for child_id in children {
        let child_items = paint_fiber(arena, child_id, order, next_scene_id, metrics);
        order += child_items.len() as u32;
        items.extend(child_items);
    }

    items
}
