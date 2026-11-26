//! Fiber tree storage, reconciliation, layout, and paint.

mod arena;
mod layout;
mod paint;
mod reconcile;

pub use arena::{DirtyFlags, Fiber, FiberArena, FiberId};
pub use reconcile::{ReconcileDiagnostic, ReconcileSiblingList};

pub(crate) use layout::{LayoutOutcome, layout_fiber};
pub(crate) use paint::paint_fiber;
pub(crate) use reconcile::{reconcile_children_with_externals, unmount_fiber};

/// Root overlays in logical declaration order. Lifetime remains in the Fiber tree.
pub(crate) fn portal_roots(arena: &FiberArena, root: FiberId) -> Vec<FiberId> {
    let mut portals = Vec::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if let Some(fiber) = arena.get(id) {
            if fiber.view.as_ref().is_some_and(|view| view.is_portal()) {
                portals.push(id);
            }
            pending.extend(fiber.children.iter().rev().copied());
        }
    }
    portals
}

#[cfg(test)]
mod tests;
