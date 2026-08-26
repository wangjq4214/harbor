use super::{FiberArena, FiberId};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::text::TextMetrics;

// ── Layout ───────────────────────────────────────────────────────────────────

/// A measured Fiber subtree with child origins relative to its own rect.
struct MeasuredFiber {
    id: FiberId,
    size: Size,
    children: Vec<(Point, MeasuredFiber)>,
}

/// Measures a Fiber subtree once without assigning layout rects.
fn measure_fiber(
    arena: &FiberArena,
    id: FiberId,
    constraints: BoxConstraints,
    metrics: &TextMetrics,
) -> Option<MeasuredFiber> {
    let fiber = arena.get(id)?;
    let view = fiber.view.clone();
    let children = fiber.children.clone();
    let Some(view) = view else {
        return Some(MeasuredFiber {
            id,
            size: constraints.constrain(Size::ZERO),
            children: vec![],
        });
    };

    let child_constraints = view.child_constraints(constraints);
    let measured_children = children
        .into_iter()
        .map(|child_id| measure_fiber(arena, child_id, child_constraints, metrics))
        .collect::<Vec<_>>();
    let child_sizes = measured_children
        .iter()
        .map(|layout| layout.as_ref().map_or(Size::ZERO, |layout| layout.size))
        .collect::<Vec<_>>();
    let (size, child_origins) = view.layout_children(constraints, &child_sizes, metrics);
    let children = measured_children
        .into_iter()
        .zip(child_origins)
        .filter_map(|(layout, child_origin)| layout.map(|layout| (child_origin, layout)))
        .collect();

    Some(MeasuredFiber { id, size, children })
}

/// Assigns rects from a completed measurement without remeasuring descendants.
fn apply_layout(arena: &mut FiberArena, layout: &MeasuredFiber, origin: Point) {
    if let Some(fiber) = arena.get_mut(layout.id) {
        fiber.layout_rect = Some(Rect::from_min_size(origin, layout.size));
    }

    for (child_origin, child_layout) in &layout.children {
        apply_layout(
            arena,
            child_layout,
            Point::new(origin.x + child_origin.x, origin.y + child_origin.y),
        );
    }
}

/// Measures the full subtree, then assigns every resulting layout rect.
pub(crate) fn layout_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    constraints: BoxConstraints,
    origin: Point,
    metrics: &TextMetrics,
) {
    let Some(measured) = measure_fiber(arena, id, constraints, metrics) else {
        return;
    };
    apply_layout(arena, &measured, origin);
}
