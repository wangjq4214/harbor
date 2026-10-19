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
    let portals = super::portal_roots(arena, id);
    let mut items = if portals.first() == Some(&id) {
        Vec::new()
    } else {
        paint_fiber_with_clips(arena, id, base_order, next_scene_id, metrics, &[])
    };
    for portal in portals {
        let root_clip = arena
            .get(portal)
            .and_then(|f| f.layout_rect())
            .and_then(|rect| {
                crate::scene::clip::RoundedClip::new(
                    rect,
                    crate::BorderRadius::default(),
                    crate::ClipBehavior::HardEdge,
                )
                .ok()
            });
        let order = base_order + items.len() as u32;
        items.extend(paint_fiber_with_clips(
            arena,
            portal,
            order,
            next_scene_id,
            metrics,
            root_clip.as_slice(),
        ));
    }
    items
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

    append_scene_items(
        &mut items,
        &mut arena
            .get_mut(id)
            .expect("fiber was present during paint")
            .scene_item_slots,
        before_primitives,
        inherited_clips,
        &mut order,
        next_scene_id,
    );

    let mut child_clips = inherited_clips.to_vec();
    if let Some(clip) = descendant_clip {
        child_clips.push(clip);
    }
    for child_id in children {
        if arena
            .get(child_id)
            .and_then(|f| f.view.as_ref())
            .is_some_and(|v| v.is_portal())
        {
            continue;
        }
        let child_items =
            paint_fiber_with_clips(arena, child_id, order, next_scene_id, metrics, &child_clips);
        order += child_items.len() as u32;
        items.extend(child_items);
    }

    append_scene_items(
        &mut items,
        &mut arena
            .get_mut(id)
            .expect("fiber was present during paint")
            .after_scene_item_slots,
        after_primitives,
        inherited_clips,
        &mut order,
        next_scene_id,
    );

    items
}

fn append_scene_items(
    items: &mut Vec<SceneItem>,
    slots: &mut Vec<(u32, u64)>,
    primitives: Vec<(u32, crate::scene::primitive::Primitive)>,
    clips: &[crate::scene::clip::RoundedClip],
    order: &mut u32,
    next_scene_id: &mut u64,
) {
    for (slot, primitive) in primitives {
        let id = if let Some((_, id)) = slots
            .iter()
            .find(|(existing_slot, _)| *existing_slot == slot)
        {
            *id
        } else {
            let id = *next_scene_id;
            *next_scene_id = next_scene_id
                .checked_add(1)
                .expect("scene item ID allocator exhausted");
            slots.push((slot, id));
            id
        };
        items.push(SceneItem {
            id,
            primitive,
            clips: clips.to_vec(),
            paint_order: *order,
        });
        *order += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Point, Rect, Size};
    use crate::scene::primitive::{Color, Primitive};

    #[test]
    fn omitted_paint_slots_keep_their_ids_when_restored() {
        let primitive = Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(10.0, 10.0)),
            color: Color::RED,
            corner_radius: 0.0,
        };
        let mut slots = Vec::new();
        let mut next_id = 1;
        for (keys, expected_ids) in [
            (vec![2, 7], vec![1, 2]),
            (vec![7], vec![2]),
            (vec![7, 2], vec![2, 1]),
        ] {
            let mut items = Vec::new();
            let mut order = 10;
            append_scene_items(
                &mut items,
                &mut slots,
                keys.into_iter()
                    .map(|key| (key, primitive.clone()))
                    .collect(),
                &[],
                &mut order,
                &mut next_id,
            );
            assert_eq!(
                items.iter().map(|item| item.id).collect::<Vec<_>>(),
                expected_ids
            );
            assert_eq!(
                items
                    .iter()
                    .map(|item| item.paint_order)
                    .collect::<Vec<_>>(),
                (10..order).collect::<Vec<_>>()
            );
            assert!(items.iter().all(|item| item.clips.is_empty()));
            assert_eq!(next_id, 3);
        }
    }
}
