use super::{DirtyFlags, Fiber, FiberArena, FiberId};
use crate::view::View;

// ── Reconciliation ───────────────────────────────────────────────────────────

/// Recursively unmounts a fiber and its entire subtree.
///
/// Unsubscribes all hooks and removes all fibers from the arena.
pub(crate) fn unmount_fiber(arena: &mut FiberArena, id: FiberId) {
    // Clone children before borrowing mutably below
    let children = arena
        .get(id)
        .map(|f| f.children.clone())
        .unwrap_or_default();
    for child_id in children {
        unmount_fiber(arena, child_id);
    }
    if let Some(fiber) = arena.remove(id) {
        for hook in &fiber.hooks {
            hook.unsubscribe_all(id);
        }
    }
}

/// Creates a new Fiber from a View and recursively reconciles its children.
/// Pass `None` for `parent_id` for root-level fibers.
pub(crate) fn create_fiber_from_view(
    arena: &mut FiberArena,
    parent_id: Option<FiberId>,
    view: View,
) -> FiberId {
    let key = view.key().cloned();
    let widget_type = view.widget_type();
    let (inner, children, _key) = view.decompose();

    let mut fiber = Fiber::new(key, widget_type, Some(inner));
    fiber.parent = parent_id;
    fiber.flags.insert(DirtyFlags::BUILD_DIRTY);
    fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
    let id = arena.insert(fiber);

    // Reconcile children of the new fiber
    let new_children = reconcile_children(arena, id, &[], children);
    if let Some(f) = arena.get_mut(id) {
        f.children = new_children;
    }

    id
}

/// Reconciles a parent fiber's children against new Views.
///
/// Matches old and new children by position, widget type, and key.
/// Returns the new list of child FiberIds.
pub(crate) fn reconcile_children(
    arena: &mut FiberArena,
    parent_id: FiberId,
    old_children: &[FiberId],
    new_views: Vec<View>,
) -> Vec<FiberId> {
    let max_len = old_children.len().max(new_views.len());
    let mut new_child_ids = Vec::with_capacity(max_len);
    let mut view_iter = new_views.into_iter();

    for i in 0..max_len {
        let old_id = old_children.get(i).copied();
        let view = view_iter.next();

        match (old_id, view) {
            (Some(old_id), Some(view)) => {
                let can_reuse = match arena.get(old_id) {
                    Some(old_fiber) => {
                        old_fiber.widget_type == view.widget_type()
                            && old_fiber.key.as_ref() == view.key()
                    }
                    None => false,
                };

                if can_reuse {
                    // Reuse the old fiber -- update view and reconcile children
                    let (inner, view_children, _key) = view.decompose();

                    let grand_old = arena
                        .get(old_id)
                        .map(|f| f.children.clone())
                        .unwrap_or_default();

                    if let Some(fiber) = arena.get_mut(old_id) {
                        fiber.view = Some(inner);
                    }

                    let new_grandchildren =
                        reconcile_children(arena, old_id, &grand_old, view_children);

                    if let Some(fiber) = arena.get_mut(old_id) {
                        fiber.children = new_grandchildren;
                    }

                    new_child_ids.push(old_id);
                } else {
                    // Type or key mismatch -- unmount old, create new
                    unmount_fiber(arena, old_id);
                    let new_id = create_fiber_from_view(arena, Some(parent_id), view);
                    new_child_ids.push(new_id);
                }
            }
            (Some(old_id), None) => {
                // Old child no longer exists in the new View tree
                unmount_fiber(arena, old_id);
            }
            (None, Some(view)) => {
                // New child with no matching old fiber
                let new_id = create_fiber_from_view(arena, Some(parent_id), view);
                new_child_ids.push(new_id);
            }
            (None, None) => unreachable!("loop bound is max(old, new) so at least one is Some"),
        }
    }

    new_child_ids
}

