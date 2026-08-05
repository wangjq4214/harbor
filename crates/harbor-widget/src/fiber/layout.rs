use super::{FiberArena, FiberId};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::text::TextMetrics;

// ── Layout ───────────────────────────────────────────────────────────────────

/// Two-pass layout walk:
///   1. Collect child intrinsic sizes bottom-up.
///   2. Call `layout_children` on the parent to compute own size and child origins.
///   3. Recurse into children with computed positions.
pub(crate) fn layout_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    constraints: BoxConstraints,
    origin: Point,
    metrics: &TextMetrics,
) {
    // Phase 1: collect child intrinsic sizes (bottom-up)
    let child_specs: Vec<(FiberId, Size)> = {
        let fiber = match arena.get(id) {
            Some(f) => f,
            None => return,
        };
        let children = fiber.children.clone();
        let mut specs = Vec::with_capacity(children.len());
        for &cid in &children {
            let child_size = if let Some(child) = arena.get(cid) {
                child
                    .view
                    .as_ref()
                    .map(|v| v.intrinsic_size(constraints, metrics))
                    .unwrap_or(Size::ZERO)
            } else {
                Size::ZERO
            };
            specs.push((cid, child_size));
        }
        specs
    };

    // Phase 2: compute own size and child origins
    let (own_size, child_origins) = {
        let fiber = match arena.get(id) {
            Some(f) => f,
            None => return,
        };
        let sizes: Vec<Size> = child_specs.iter().map(|(_, s)| *s).collect();

        fiber
            .view
            .as_ref()
            .map(|v| v.layout_children(constraints, &sizes, metrics))
            .unwrap_or((constraints.constrain(Size::ZERO), vec![]))
    };

    // Store own rect
    if let Some(fiber) = arena.get_mut(id) {
        fiber.layout_rect = Some(Rect::from_min_size(origin, own_size));
    }

    // Phase 3: recurse into children with computed positions
    for ((cid, _child_size), child_pos) in child_specs.iter().zip(child_origins.iter()) {
        let child_origin = Point::new(origin.x + child_pos.x, origin.y + child_pos.y);
        let child_constraints = BoxConstraints::loose(own_size);
        layout_fiber(arena, *cid, child_constraints, child_origin, metrics);
    }
}
