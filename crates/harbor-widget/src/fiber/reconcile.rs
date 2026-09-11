use super::{DirtyFlags, Fiber, FiberArena, FiberId};
use crate::theme::Theme;
use crate::view::{BuildCx, ExternalRegistrations, Key, View, ViewContents};
use hashbrown::{HashMap, HashSet};
use std::sync::Arc;

// ── Reconciliation ───────────────────────────────────────────────────────────

/// Identifies which sibling list contained a duplicate reconciliation key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileSiblingList {
    /// The children currently retained by the parent Fiber.
    Previous,
    /// The incoming children being reconciled.
    Incoming,
}

/// A deterministic diagnostic emitted when sibling keys are ambiguous.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconcileDiagnostic {
    /// A key occurs more than once in one sibling list.
    DuplicateSiblingKey {
        /// The parent whose immediate children were reconciled.
        parent: FiberId,
        /// Which sibling list contained the duplicate.
        list: ReconcileSiblingList,
        /// The duplicated key.
        key: Key,
        /// All source-order occurrences of `key` in `list`.
        occurrences: Vec<usize>,
    },
}

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
        for subscription in &fiber.subscriptions {
            subscription.unsubscribe_all(id);
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
    create_fiber_from_view_with_externals(
        arena,
        parent_id,
        view,
        &mut ExternalRegistrations::default(),
        Arc::new(Theme::default()),
    )
}

fn create_fiber_from_view_with_externals(
    arena: &mut FiberArena,
    parent_id: Option<FiberId>,
    view: View,
    externals: &mut ExternalRegistrations,
    inherited_theme: Arc<Theme>,
) -> FiberId {
    let key = view.key().cloned();
    let widget_type = view.widget_type();

    let mut fiber = Fiber::new(key, widget_type, None);
    fiber.parent = parent_id;
    fiber.theme = inherited_theme;
    fiber.flags.insert(DirtyFlags::BUILD_DIRTY);
    fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
    let id = arena.insert(fiber);

    reconcile_fiber(arena, id, view, externals);
    id
}

/// Builds a deferred view in its assigned Fiber, then reconciles the concrete
/// tree that it returns. Its hook vector is restored before its descendants are
/// reconciled, so state stays owned by the same Fiber across updates.
fn reconcile_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    view: View,
    externals: &mut ExternalRegistrations,
) {
    let (contents, children, _key) = view.into_parts();
    match contents {
        ViewContents::Concrete(inner) => {
            reconcile_concrete_fiber(arena, id, inner, children, None, externals);
        }
        ViewContents::Deferred { component, .. } => {
            let theme = arena.get(id).unwrap().theme.clone();
            let hooks = std::mem::take(&mut arena.get_mut(id).unwrap().hooks);
            let subscriptions = std::mem::take(&mut arena.get_mut(id).unwrap().subscriptions);
            for subscription in subscriptions {
                subscription.unsubscribe_all(id);
            }
            let mut cx = BuildCx {
                current_fiber: Some(id),
                hooks,
                subscriptions: Vec::new(),
                hook_index: 0,
                externals: ExternalRegistrations::default(),
                theme,
            };
            let materialized = component.build(&mut cx);
            externals.append(&mut cx.externals);
            let hooks = cx.hooks;
            let subscriptions = cx.subscriptions;
            let (inner, materialized_children, _key) = materialized.decompose();
            reconcile_concrete_fiber(
                arena,
                id,
                inner,
                materialized_children,
                Some(hooks),
                externals,
            );
            arena.get_mut(id).unwrap().subscriptions = subscriptions;
        }
    }
}

fn reconcile_concrete_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    inner: Arc<dyn crate::view::AnyView>,
    children: Vec<View>,
    hooks: Option<Vec<Box<dyn crate::signal::Hook>>>,
    externals: &mut ExternalRegistrations,
) {
    let old_children = arena
        .get(id)
        .map(|fiber| fiber.children.clone())
        .unwrap_or_default();

    let inherited_theme = arena
        .get(id)
        .map(|fiber| fiber.theme.clone())
        .unwrap_or_else(|| Arc::new(Theme::default()));
    let effective_theme = inner.theme_override().unwrap_or(inherited_theme);
    if let Some(fiber) = arena.get_mut(id) {
        fiber.view = Some(inner);
        fiber.theme = effective_theme;
        if let Some(hooks) = hooks {
            fiber.hooks = hooks;
        }
    }

    let new_children =
        reconcile_children_with_externals(arena, id, &old_children, children, externals);
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
    reconcile_children_with_externals(
        arena,
        parent_id,
        old_children,
        new_views,
        &mut ExternalRegistrations::default(),
    )
}

fn key_counts(keys: &[Option<Key>]) -> HashMap<Key, usize> {
    let mut counts = HashMap::new();
    for key in keys.iter().flatten() {
        *counts.entry(key.clone()).or_insert(0) += 1;
    }
    counts
}

fn duplicate_key_diagnostics(
    parent: FiberId,
    list: ReconcileSiblingList,
    keys: &[Option<Key>],
    counts: &HashMap<Key, usize>,
) -> Vec<ReconcileDiagnostic> {
    let mut occurrences_by_key: HashMap<Key, Vec<usize>> = HashMap::new();
    for (index, key) in keys.iter().enumerate() {
        if let Some(key) = key {
            occurrences_by_key
                .entry(key.clone())
                .or_default()
                .push(index);
        }
    }

    let mut reported = HashSet::new();
    let mut diagnostics = Vec::new();
    for key in keys.iter().flatten() {
        if counts.get(key).copied().unwrap_or_default() > 1 && reported.insert(key.clone()) {
            diagnostics.push(ReconcileDiagnostic::DuplicateSiblingKey {
                parent,
                list,
                key: key.clone(),
                occurrences: occurrences_by_key
                    .remove(key)
                    .expect("key was collected before duplicate diagnostics"),
            });
        }
    }
    diagnostics
}

/// Reconciles a parent fiber's children using keyed matching plus positional
/// matching for unkeyed siblings.
///
/// A unique key can retain a compatible Fiber across sibling positions. An
/// unkeyed View can only retain the compatible unkeyed Fiber at the same index.
/// Duplicate keys fail closed: no occurrence reuses an existing Fiber, and all
/// unmatched old Fibers are swept only after every new child has been selected.
pub(crate) fn reconcile_children_with_externals(
    arena: &mut FiberArena,
    parent_id: FiberId,
    old_children: &[FiberId],
    new_views: Vec<View>,
    externals: &mut ExternalRegistrations,
) -> Vec<FiberId> {
    let parent_theme = arena
        .get(parent_id)
        .map(|fiber| fiber.theme.clone())
        .unwrap_or_else(|| Arc::new(Theme::default()));
    let old_keys: Vec<Option<Key>> = old_children
        .iter()
        .map(|&id| arena.get(id).and_then(|fiber| fiber.key.clone()))
        .collect();
    let new_keys: Vec<Option<Key>> = new_views.iter().map(|view| view.key().cloned()).collect();

    let old_key_counts = key_counts(&old_keys);
    let new_key_counts = key_counts(&new_keys);
    let old_unique_by_key: HashMap<Key, FiberId> = old_children
        .iter()
        .copied()
        .zip(old_keys.iter())
        .filter_map(|(id, key)| {
            key.as_ref()
                .filter(|key| old_key_counts.get(*key) == Some(&1))
                .map(|key| (key.clone(), id))
        })
        .collect();
    let duplicate_new_keys: HashSet<Key> = new_key_counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(key, _)| key.clone())
        .collect();

    let mut diagnostics = duplicate_key_diagnostics(
        parent_id,
        ReconcileSiblingList::Previous,
        &old_keys,
        &old_key_counts,
    );
    diagnostics.extend(duplicate_key_diagnostics(
        parent_id,
        ReconcileSiblingList::Incoming,
        &new_keys,
        &new_key_counts,
    ));

    let mut consumed = HashSet::new();
    let mut new_child_ids = Vec::with_capacity(new_views.len());
    for (new_index, view) in new_views.into_iter().enumerate() {
        let key = &new_keys[new_index];
        let candidate = match key {
            Some(key) if !duplicate_new_keys.contains(key) => old_unique_by_key.get(key).copied(),
            Some(_) => None,
            None => old_children.get(new_index).copied(),
        };

        let reusable = candidate.filter(|old_id| {
            !consumed.contains(old_id)
                && arena.get(*old_id).is_some_and(|old_fiber| {
                    old_fiber.widget_type == view.widget_type()
                        && old_fiber.key.as_ref() == key.as_ref()
                })
        });

        if let Some(old_id) = reusable {
            consumed.insert(old_id);
            if let Some(fiber) = arena.get_mut(old_id) {
                fiber.theme = parent_theme.clone();
            }
            reconcile_fiber(arena, old_id, view, externals);
            new_child_ids.push(old_id);
        } else {
            new_child_ids.push(create_fiber_from_view_with_externals(
                arena,
                Some(parent_id),
                view,
                externals,
                parent_theme.clone(),
            ));
        }
    }

    for &old_id in old_children {
        if !consumed.contains(&old_id) {
            unmount_fiber(arena, old_id);
        }
    }
    if let Some(parent) = arena.get_mut(parent_id) {
        parent.reconcile_diagnostics = diagnostics;
    }

    new_child_ids
}
