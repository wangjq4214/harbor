use super::{FiberArena, FiberId};
use crate::scene::SceneItem;
use crate::text::TextMetrics;
use crate::view::PaintPhase;

// ── Paint ────────────────────────────────────────────────────────────────────

/// Walks the fiber tree as before-child primitives, descendants, then
/// after-child primitives. Each local phase owns an independent stable scene
/// identity list so descendant changes cannot perturb after-child identities.
pub(crate) fn paint_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    base_order: u32,
    next_scene_id: &mut u64,
    metrics: &TextMetrics,
) -> Vec<SceneItem> {
    let mut items = Vec::new();
    let mut order = base_order;

    let (before_primitives, children, after_primitives) = {
        let fiber = match arena.get(id) {
            Some(fiber) => fiber,
            None => return items,
        };
        match (&fiber.view, fiber.layout_rect) {
            (Some(view), Some(rect)) => (
                view.paint_primitives_for_phase(PaintPhase::BeforeChildren, rect, metrics),
                fiber.children.clone(),
                view.paint_primitives_for_phase(PaintPhase::AfterChildren, rect, metrics),
            ),
            _ => (Vec::new(), fiber.children.clone(), Vec::new()),
        }
    };

    let before_ids =
        retain_scene_item_ids(arena, id, false, before_primitives.len(), next_scene_id);
    append_scene_items(&mut items, before_ids, before_primitives, &mut order);

    for child_id in children {
        let child_items = paint_fiber(arena, child_id, order, next_scene_id, metrics);
        order += child_items.len() as u32;
        items.extend(child_items);
    }

    let after_ids = retain_scene_item_ids(arena, id, true, after_primitives.len(), next_scene_id);
    append_scene_items(&mut items, after_ids, after_primitives, &mut order);

    items
}

fn retain_scene_item_ids(
    arena: &mut FiberArena,
    id: FiberId,
    after_children: bool,
    primitive_count: usize,
    next_scene_id: &mut u64,
) -> Vec<u64> {
    let fiber = arena.get_mut(id).expect("fiber was present during paint");
    let scene_item_ids = if after_children {
        &mut fiber.after_scene_item_ids
    } else {
        &mut fiber.scene_item_ids
    };

    while scene_item_ids.len() < primitive_count {
        scene_item_ids.push(*next_scene_id);
        *next_scene_id = next_scene_id
            .checked_add(1)
            .expect("scene item ID allocator exhausted");
    }
    scene_item_ids.truncate(primitive_count);
    scene_item_ids.clone()
}

fn append_scene_items(
    items: &mut Vec<SceneItem>,
    scene_item_ids: Vec<u64>,
    primitives: Vec<crate::scene::primitive::Primitive>,
    order: &mut u32,
) {
    for (id, primitive) in scene_item_ids.into_iter().zip(primitives) {
        items.push(SceneItem {
            id,
            primitive,
            clips: Vec::new(),
            paint_order: *order,
        });
        *order += 1;
    }
}
