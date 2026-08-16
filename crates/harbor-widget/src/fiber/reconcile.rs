use super::{DirtyFlags, Fiber, FiberArena, FiberId};
use crate::scene::primitive::{ExternalDrawFn, ExternalDrawId, ExternalScheduleFn};
use crate::view::{BuildCx, View, ViewContents};
use std::sync::Arc;

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
#[cfg(test)]
pub(crate) fn create_fiber_from_view(
    arena: &mut FiberArena,
    parent_id: Option<FiberId>,
    view: View,
) -> FiberId {
    create_fiber_from_view_with_external_draws(
        arena,
        parent_id,
        view,
        &mut Vec::new(),
        &mut Vec::new(),
    )
}

fn create_fiber_from_view_with_external_draws(
    arena: &mut FiberArena,
    parent_id: Option<FiberId>,
    view: View,
    external_draws: &mut Vec<(ExternalDrawId, Arc<ExternalDrawFn<'static>>)>,
    external_schedules: &mut Vec<(ExternalDrawId, Arc<ExternalScheduleFn>)>,
) -> FiberId {
    let key = view.key().cloned();
    let widget_type = view.widget_type();

    let mut fiber = Fiber::new(key, widget_type, None);
    fiber.parent = parent_id;
    fiber.flags.insert(DirtyFlags::BUILD_DIRTY);
    fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
    let id = arena.insert(fiber);

    reconcile_fiber(arena, id, view, external_draws, external_schedules);
    id
}

/// Builds a deferred view in its assigned Fiber, then reconciles the concrete
/// tree that it returns. Its hook vector is restored before its descendants are
/// reconciled, so state stays owned by the same Fiber across updates.
fn reconcile_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    view: View,
    external_draws: &mut Vec<(ExternalDrawId, Arc<ExternalDrawFn<'static>>)>,
    external_schedules: &mut Vec<(ExternalDrawId, Arc<ExternalScheduleFn>)>,
) {
    let (contents, children, _key) = view.into_parts();
    match contents {
        ViewContents::Concrete(inner) => {
            reconcile_concrete_fiber(
                arena,
                id,
                inner,
                children,
                None,
                external_draws,
                external_schedules,
            );
        }
        ViewContents::Deferred { component, .. } => {
            let hooks = std::mem::take(&mut arena.get_mut(id).unwrap().hooks);
            let mut cx = BuildCx {
                current_fiber: Some(id),
                hooks,
                hook_index: 0,
                external_draws: Vec::new(),
                external_schedules: Vec::new(),
            };
            let materialized = component.build(&mut cx);
            external_draws.append(&mut cx.external_draws);
            external_schedules.append(&mut cx.external_schedules);
            let hooks = cx.hooks;
            let (inner, materialized_children, _key) = materialized.decompose();
            reconcile_concrete_fiber(
                arena,
                id,
                inner,
                materialized_children,
                Some(hooks),
                external_draws,
                external_schedules,
            );
        }
    }
}

fn reconcile_concrete_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    inner: Arc<dyn crate::view::AnyView>,
    children: Vec<View>,
    hooks: Option<Vec<Box<dyn crate::signal::Hook>>>,
    external_draws: &mut Vec<(ExternalDrawId, Arc<ExternalDrawFn<'static>>)>,
    external_schedules: &mut Vec<(ExternalDrawId, Arc<ExternalScheduleFn>)>,
) {
    let old_children = arena
        .get(id)
        .map(|fiber| fiber.children.clone())
        .unwrap_or_default();

    if let Some(fiber) = arena.get_mut(id) {
        fiber.view = Some(inner);
        if let Some(hooks) = hooks {
            fiber.hooks = hooks;
        }
    }

    let new_children = reconcile_children_with_external_draws(
        arena,
        id,
        &old_children,
        children,
        external_draws,
        external_schedules,
    );
    if let Some(fiber) = arena.get_mut(id) {
        fiber.children = new_children;
    }
}

/// Reconciles a parent fiber's children against new Views.
///
/// Matches old and new children by position, widget type, and key.
/// Returns the new list of child FiberIds.
#[cfg(test)]
pub(crate) fn reconcile_children(
    arena: &mut FiberArena,
    parent_id: FiberId,
    old_children: &[FiberId],
    new_views: Vec<View>,
) -> Vec<FiberId> {
    reconcile_children_with_external_draws(
        arena,
        parent_id,
        old_children,
        new_views,
        &mut Vec::new(),
        &mut Vec::new(),
    )
}

pub(crate) fn reconcile_children_with_external_draws(
    arena: &mut FiberArena,
    parent_id: FiberId,
    old_children: &[FiberId],
    new_views: Vec<View>,
    external_draws: &mut Vec<(ExternalDrawId, Arc<ExternalDrawFn<'static>>)>,
    external_schedules: &mut Vec<(ExternalDrawId, Arc<ExternalScheduleFn>)>,
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
                    reconcile_fiber(arena, old_id, view, external_draws, external_schedules);
                    new_child_ids.push(old_id);
                } else {
                    // Type or key mismatch -- unmount old, create new
                    unmount_fiber(arena, old_id);
                    let new_id = create_fiber_from_view_with_external_draws(
                        arena,
                        Some(parent_id),
                        view,
                        external_draws,
                        external_schedules,
                    );
                    new_child_ids.push(new_id);
                }
            }
            (Some(old_id), None) => {
                // Old child no longer exists in the new View tree
                unmount_fiber(arena, old_id);
            }
            (None, Some(view)) => {
                // New child with no matching old fiber
                let new_id = create_fiber_from_view_with_external_draws(
                    arena,
                    Some(parent_id),
                    view,
                    external_draws,
                    external_schedules,
                );
                new_child_ids.push(new_id);
            }
            (None, None) => unreachable!("loop bound is max(old, new) so at least one is Some"),
        }
    }

    new_child_ids
}
