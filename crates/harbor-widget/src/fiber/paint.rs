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
    paint_fiber_with_clips(arena, id, base_order, next_scene_id, metrics, &[])
}

fn paint_fiber_with_clips(
    arena: &mut FiberArena,
    id: FiberId,
    base_order: u32,
    next_scene_id: &mut u64,
    metrics: &TextMetrics,
    inherited_clips: &[crate::scene::clip::RoundedClip],
) -> Vec<SceneItem> {
    let mut items = Vec::new();
    let mut order = base_order;

    let (before_primitives, children, after_primitives, descendant_clip) = {
        let fiber = match arena.get(id) {
            Some(fiber) => fiber,
            None => return items,
        };
        match (&fiber.view, fiber.layout_rect) {
            (Some(view), Some(rect)) => (
                view.paint_primitives_with_slots_for_phase(
                    PaintPhase::BeforeChildren,
                    rect,
                    metrics,
                ),
                fiber.children.clone(),
                view.paint_primitives_with_slots_for_phase(
                    PaintPhase::AfterChildren,
                    rect,
                    metrics,
                ),
                view.descendant_clip(rect),
            ),
            _ => (Vec::new(), fiber.children.clone(), Vec::new(), None),
        }
    };

    let before_ids = retain_scene_item_ids(arena, id, false, &before_primitives, next_scene_id);
    append_scene_items(
        &mut items,
        before_ids,
        before_primitives,
        inherited_clips,
        &mut order,
    );

    let mut child_clips = inherited_clips.to_vec();
    if let Some(clip) = descendant_clip {
        child_clips.push(clip);
    }
    for child_id in children {
        let child_items =
            paint_fiber_with_clips(arena, child_id, order, next_scene_id, metrics, &child_clips);
        order += child_items.len() as u32;
        items.extend(child_items);
    }

    let after_ids = retain_scene_item_ids(arena, id, true, &after_primitives, next_scene_id);
    append_scene_items(
        &mut items,
        after_ids,
        after_primitives,
        inherited_clips,
        &mut order,
    );

    items
}

fn retain_scene_item_ids(
    arena: &mut FiberArena,
    id: FiberId,
    after_children: bool,
    primitives: &[(u32, crate::scene::primitive::Primitive)],
    next_scene_id: &mut u64,
) -> Vec<u64> {
    let fiber = arena.get_mut(id).expect("fiber was present during paint");
    let slots = if after_children {
        &mut fiber.after_scene_item_slots
    } else {
        &mut fiber.scene_item_slots
    };

    primitives
        .iter()
        .map(|(slot, _)| {
            if let Some((_, id)) = slots
                .iter()
                .find(|(existing_slot, _)| existing_slot == slot)
            {
                *id
            } else {
                let id = *next_scene_id;
                *next_scene_id = next_scene_id
                    .checked_add(1)
                    .expect("scene item ID allocator exhausted");
                slots.push((*slot, id));
                id
            }
        })
        .collect::<Vec<_>>()
}

fn append_scene_items(
    items: &mut Vec<SceneItem>,
    item_ids: Vec<u64>,
    primitives: Vec<(u32, crate::scene::primitive::Primitive)>,
    clips: &[crate::scene::clip::RoundedClip],
    order: &mut u32,
) {
    for (id, (_, primitive)) in item_ids.into_iter().zip(primitives) {
        items.push(SceneItem {
            id,
            primitive,
            clips: clips.to_vec(),
            paint_order: *order,
        });
        *order += 1;
    }
}
