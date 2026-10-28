use super::{FiberArena, FiberId};
use crate::layout::child_layout::valid_diagnostic;
use crate::layout::{
    BoxConstraints, ChildMeasurer, LayoutDiagnostic, LayoutError, ParentData, ParentLayout, Point,
    Rect, Size,
};
use crate::text::TextMetrics;
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

/// A complete candidate subtree. Cached alternatives never mutate the arena.
struct MeasuredFiber {
    id: FiberId,
    provisional: bool,
    size: Size,
    children: Vec<(Point, Rc<MeasuredFiber>)>,
    diagnostics: Vec<LayoutDiagnostic>,
}

#[derive(Clone, Copy)]
struct Failure {
    id: FiberId,
    error: LayoutError,
}

/// Shared by all nested measurements, including corrective passes. The first
/// error is sticky so a custom parent cannot commit after swallowing a failure.
struct LayoutPass<'a> {
    arena: &'a FiberArena,
    metrics: &'a TextMetrics,
    remaining: usize,
    active: HashSet<FiberId>,
    failure: Cell<Option<Failure>>,
}

impl LayoutPass<'_> {
    fn fail<T>(&self, id: FiberId, error: LayoutError) -> Result<T, LayoutError> {
        let failure = self.failure.get().unwrap_or(Failure { id, error });
        self.failure.set(Some(failure));
        Err(failure.error)
    }

    fn measure(
        &mut self,
        id: FiberId,
        constraints: BoxConstraints,
        provisional: bool,
    ) -> Result<Rc<MeasuredFiber>, LayoutError> {
        if let Some(failure) = self.failure.get() {
            return Err(failure.error);
        }
        if let Err(error) = constraints.validate() {
            return self.fail(id, error);
        }
        if self.active.contains(&id) {
            return self.fail(id, LayoutError::RecursiveMeasurement);
        }
        if self.remaining == 0 {
            return self.fail(id, LayoutError::WorkBudgetExceeded);
        }
        self.remaining -= 1;
        self.active.insert(id);
        let result = self.measure_active(id, constraints, provisional);
        self.active.remove(&id);
        match result {
            Ok(layout) => Ok(layout),
            Err(error) => self.fail(id, error),
        }
    }

    fn measure_active(
        &mut self,
        id: FiberId,
        constraints: BoxConstraints,
        provisional: bool,
    ) -> Result<Rc<MeasuredFiber>, LayoutError> {
        let fiber = self.arena.get(id).ok_or(LayoutError::StaleChild)?;
        let view = fiber.view.clone();
        // Portal edges have no ordinary layout slot (not even a flex gap).
        let child_ids = fiber
            .children
            .iter()
            .copied()
            .filter(|child| {
                !self
                    .arena
                    .get(*child)
                    .and_then(|f| f.view.as_ref())
                    .is_some_and(|view| view.is_portal())
            })
            .collect::<Vec<_>>();
        let is_root = fiber.parent.is_none();
        let metrics = self.metrics;
        let mut children = FiberChildMeasurer {
            parent: id,
            provisional,
            slots: (0..child_ids.len()).map(|_| ChildSlot::default()).collect(),
            child_ids,
            pass: self,
        };

        // Parent data is observed at exactly one edge. Never inspect descendants
        // or rebuild a Component to discover metadata.
        let accepts_flex = view
            .as_ref()
            .is_some_and(|view| view.accepts_flex_children());
        let is_flex_wrapper = view
            .as_ref()
            .is_some_and(|view| view.parent_data().flex.is_some());
        let mut diagnostics = Vec::new();
        if is_root && is_flex_wrapper {
            diagnostics.push(LayoutDiagnostic::InvalidFlexParent);
        }
        for index in 0..children.len() {
            if children.parent_data(index)?.flex.is_some() {
                let diagnostic = if is_flex_wrapper {
                    Some(LayoutDiagnostic::ConflictingFlexParentData)
                } else if !accepts_flex {
                    Some(LayoutDiagnostic::InvalidFlexParent)
                } else {
                    None
                };
                if let Some(diagnostic) = diagnostic
                    && !diagnostics.contains(&diagnostic)
                {
                    diagnostics.push(diagnostic);
                }
            }
        }

        let result = match view {
            Some(view) => view.layout(constraints, &mut children, metrics),
            None => {
                for index in 0..children.len() {
                    children.measure(index, constraints)?;
                }
                Ok(ParentLayout {
                    size: constraints.constrain(Size::ZERO),
                    placements: (0..children.len())
                        .map(|index| (index, Point::ZERO))
                        .collect(),
                    diagnostics: Vec::new(),
                })
            }
        };
        if let Some(failure) = children.pass.failure.get() {
            return Err(failure.error);
        }
        let result = result?;
        if !result.size.is_valid()
            || result.size.width < constraints.min.width
            || result.size.height < constraints.min.height
            || result.size.width > constraints.max.width
            || result.size.height > constraints.max.height
            || !result.diagnostics.iter().all(valid_diagnostic)
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let mut origins = vec![None; children.len()];
        for (index, origin) in result.placements {
            let placement = origins
                .get_mut(index)
                .ok_or(LayoutError::InvalidChildIndex)?;
            if placement.is_some() {
                return Err(LayoutError::DuplicatePlacement);
            }
            if !origin.is_valid() {
                return Err(LayoutError::InvalidGeometry);
            }
            *placement = Some(origin);
        }
        let mut measured_children = Vec::with_capacity(children.len());
        for (origin, slot) in origins.into_iter().zip(children.slots) {
            let origin = origin.ok_or(LayoutError::MissingPlacement)?;
            let layout = slot.selected.ok_or(LayoutError::UnmeasuredChild)?;
            measured_children.push((origin, layout));
        }
        diagnostics.extend(result.diagnostics);
        Ok(Rc::new(MeasuredFiber {
            id,
            provisional,
            size: result.size,
            children: measured_children,
            diagnostics,
        }))
    }
}

#[derive(Default)]
struct ChildSlot {
    cache: Vec<(BoxConstraints, bool, Rc<MeasuredFiber>)>,
    selected: Option<Rc<MeasuredFiber>>,
}

struct FiberChildMeasurer<'pass, 'arena> {
    parent: FiberId,
    provisional: bool,
    child_ids: Vec<FiberId>,
    slots: Vec<ChildSlot>,
    pass: &'pass mut LayoutPass<'arena>,
}

impl ChildMeasurer for FiberChildMeasurer<'_, '_> {
    fn len(&self) -> usize {
        self.child_ids.len()
    }

    fn parent_data(&self, index: usize) -> Result<ParentData, LayoutError> {
        if let Some(failure) = self.pass.failure.get() {
            return Err(failure.error);
        }
        let Some(&id) = self.child_ids.get(index) else {
            return self.pass.fail(self.parent, LayoutError::InvalidChildIndex);
        };
        let Some(fiber) = self.pass.arena.get(id) else {
            return self.pass.fail(self.parent, LayoutError::StaleChild);
        };
        Ok(fiber
            .view
            .as_ref()
            .map_or_else(ParentData::default, |view| view.parent_data()))
    }

    fn is_provisional(&self) -> bool {
        self.provisional
    }

    fn measure(&mut self, index: usize, constraints: BoxConstraints) -> Result<Size, LayoutError> {
        self.measure_child(index, constraints, self.provisional)
    }

    fn measure_provisional(
        &mut self,
        index: usize,
        constraints: BoxConstraints,
    ) -> Result<Size, LayoutError> {
        self.measure_child(index, constraints, true)
    }
}

impl FiberChildMeasurer<'_, '_> {
    fn measure_child(
        &mut self,
        index: usize,
        constraints: BoxConstraints,
        provisional: bool,
    ) -> Result<Size, LayoutError> {
        if let Some(failure) = self.pass.failure.get() {
            return Err(failure.error);
        }
        let Some(&id) = self.child_ids.get(index) else {
            return self.pass.fail(self.parent, LayoutError::InvalidChildIndex);
        };
        if let Err(error) = constraints.validate() {
            return self.pass.fail(self.parent, error);
        }
        let slot = &mut self.slots[index];
        if let Some((_, _, cached)) = slot
            .cache
            .iter()
            .find(|(key, phase, _)| *key == constraints && *phase == provisional)
        {
            slot.selected = Some(Rc::clone(cached));
            return Ok(cached.size);
        }
        if slot.cache.len() == 2 {
            return self.pass.fail(self.parent, LayoutError::MeasurementLimit);
        }
        let measured = self.pass.measure(id, constraints, provisional)?;
        let size = measured.size;
        slot.selected = Some(Rc::clone(&measured));
        slot.cache.push((constraints, provisional, measured));
        Ok(size)
    }
}

/// Count unique reachable live nodes without recursing through malformed cycles.
fn reachable_nodes(arena: &FiberArena, root: FiberId) -> Vec<FiberId> {
    let mut seen = HashSet::new();
    let mut pending = vec![root];
    let mut nodes = Vec::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(fiber) = arena.get(id) {
            nodes.push(id);
            pending.extend(fiber.children.iter().rev().copied());
        }
    }
    nodes
}

struct Commit<'a> {
    layout: &'a MeasuredFiber,
    rect: Rect,
}

/// Validate absolute coordinates and edges for the *whole* selected candidate
/// before writing any retained rect, including overflow from adding finite values.
fn prepare_commit<'a>(
    arena: &FiberArena,
    layout: &'a MeasuredFiber,
    origin: Point,
) -> Result<Vec<Commit<'a>>, Failure> {
    let mut pending = vec![(layout, origin)];
    let mut seen = HashSet::new();
    let mut commits = Vec::new();
    while let Some((layout, origin)) = pending.pop() {
        let error = if !arena.contains(layout.id) {
            Some(LayoutError::StaleChild)
        } else if !seen.insert(layout.id) {
            Some(LayoutError::DuplicateChild)
        } else if layout.provisional {
            Some(LayoutError::UnfinalizedMeasurement)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(Failure {
                id: layout.id,
                error,
            });
        }
        let rect = Rect::from_min_size(origin, layout.size);
        if !origin.is_valid() || !rect.max.is_valid() || !rect.size().is_valid() {
            return Err(Failure {
                id: layout.id,
                error: LayoutError::InvalidGeometry,
            });
        }
        commits.push(Commit { layout, rect });
        for (relative, child) in layout.children.iter().rev() {
            pending.push((
                child,
                Point::new(origin.x + relative.x, origin.y + relative.y),
            ));
        }
    }
    Ok(commits)
}

/// Whether the selected candidate completed its atomic geometry commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayoutOutcome {
    pub committed: bool,
}

/// Bounded parent-directed measurement followed by one atomic geometry commit.
/// Each actual node measurement consumes the shared 8 × reachable-node budget.
/// Failure retains committed geometry/diagnostics; first-layout fallback is zero
/// and non-hit-testable. A stale top-level id remains a failed/no-op outcome.
pub(crate) fn layout_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    constraints: BoxConstraints,
    origin: Point,
    metrics: &TextMetrics,
) -> LayoutOutcome {
    if !arena.contains(id) {
        return LayoutOutcome { committed: false };
    }
    let nodes = reachable_nodes(arena, id);
    let mut pass = LayoutPass {
        arena,
        metrics,
        remaining: nodes.len().saturating_mul(8),
        active: HashSet::new(),
        failure: Cell::new(None),
    };
    let measured = if origin.is_valid() {
        pass.measure(id, constraints, false)
    } else {
        pass.fail(id, LayoutError::InvalidGeometry)
    };
    let result = measured.map_err(|error| pass.failure.get().unwrap_or(Failure { id, error }));
    let result = result.and_then(|layout| {
        // Measure every escaped subtree against the same root extent, never the
        // declaring parent's constraints. Validate all geometry before any write.
        let overlay_constraints = BoxConstraints::tight(constraints.fill_bounded(layout.size));
        let mut layouts = vec![layout];
        for portal in super::portal_roots(arena, id)
            .into_iter()
            .filter(|p| *p != id)
        {
            let overlay = pass
                .measure(portal, overlay_constraints, false)
                .map_err(|error| pass.failure.get().unwrap_or(Failure { id: portal, error }))?;
            layouts.push(overlay);
        }
        Ok(layouts)
    });
    drop(pass);
    let result = result.and_then(|layouts| {
        let mut commits = Vec::new();
        for layout in &layouts {
            commits.extend(prepare_commit(arena, layout, origin)?);
        }
        for commit in commits {
            let fiber = arena
                .get_mut(commit.layout.id)
                .expect("validated live fiber");
            fiber.layout_rect = Some(commit.rect);
            fiber
                .layout_diagnostics
                .clone_from(&commit.layout.diagnostics);
            fiber.layout_error = None;
        }
        Ok(())
    });
    let committed = result.is_ok();
    if let Err(failure) = result {
        for node in nodes {
            let fiber = arena.get_mut(node).expect("reachable live fiber");
            fiber
                .layout_rect
                .get_or_insert(Rect::from_min_size(Point::ZERO, Size::ZERO));
        }
        arena.get_mut(id).expect("live root").layout_error = Some(failure.error);
        if let Some(fiber) = arena.get_mut(failure.id) {
            fiber.layout_error = Some(failure.error);
        }
    }
    LayoutOutcome { committed }
}

#[cfg(test)]
mod tests;
