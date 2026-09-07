use super::{FiberArena, FiberId};
use crate::layout::child_layout::{valid_diagnostic, valid_point, valid_size};
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
        let child_ids = fiber.children.clone();
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
        if !valid_size(result.size)
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
            if !valid_point(origin) {
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
        if !valid_point(origin) || !valid_point(rect.max) || !valid_size(rect.size()) {
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

/// Bounded parent-directed measurement followed by one atomic geometry commit.
/// Each actual node measurement consumes the shared 8 × reachable-node budget.
/// Failure retains committed geometry/diagnostics; first-layout fallback is zero
/// and non-hit-testable. A stale top-level id remains a no-op.
pub(crate) fn layout_fiber(
    arena: &mut FiberArena,
    id: FiberId,
    constraints: BoxConstraints,
    origin: Point,
    metrics: &TextMetrics,
) {
    if !arena.contains(id) {
        return;
    }
    let nodes = reachable_nodes(arena, id);
    let mut pass = LayoutPass {
        arena,
        metrics,
        remaining: nodes.len().saturating_mul(8),
        active: HashSet::new(),
        failure: Cell::new(None),
    };
    let measured = if valid_point(origin) {
        pass.measure(id, constraints, false)
    } else {
        pass.fail(id, LayoutError::InvalidGeometry)
    };
    let result = measured.map_err(|error| pass.failure.get().unwrap_or(Failure { id, error }));
    let result = result.and_then(|layout| {
        // Keep the owning Rc alive while the commit list borrows its subtrees.
        let commits = prepare_commit(arena, &layout, origin)?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fiber::Fiber;
    use crate::layout::{FlexFit, FlexParentData};
    use crate::runtime::DEFAULT_TEXT_METRICS;
    use crate::view::{AnyView, Key};
    use std::any::TypeId;
    use std::cell::RefCell;
    use std::num::NonZeroU32;
    use std::sync::Arc;

    type LayoutFn =
        dyn Fn(BoxConstraints, &mut dyn ChildMeasurer) -> Result<ParentLayout, LayoutError>;
    type Measurements = Rc<RefCell<Vec<(&'static str, BoxConstraints)>>>;

    struct TestView {
        layout: Box<LayoutFn>,
        data: ParentData,
        accepts_flex: bool,
    }

    impl AnyView for TestView {
        fn key(&self) -> Option<&Key> {
            None
        }

        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }

        fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
            constraints.constrain(Size::ZERO)
        }

        fn parent_data(&self) -> ParentData {
            self.data
        }

        fn accepts_flex_children(&self) -> bool {
            self.accepts_flex
        }

        fn layout(
            &self,
            constraints: BoxConstraints,
            children: &mut dyn ChildMeasurer,
            _: &TextMetrics,
        ) -> Result<ParentLayout, LayoutError> {
            (self.layout)(constraints, children)
        }
    }

    fn view(
        layout: impl Fn(BoxConstraints, &mut dyn ChildMeasurer) -> Result<ParentLayout, LayoutError>
        + 'static,
    ) -> TestView {
        TestView {
            layout: Box::new(layout),
            data: ParentData::default(),
            accepts_flex: false,
        }
    }

    fn ordinary_layout(
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
    ) -> Result<ParentLayout, LayoutError> {
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

    fn insert(arena: &mut FiberArena, view: impl AnyView, children: Vec<FiberId>) -> FiberId {
        let mut fiber = Fiber::new(None, view.widget_type(), Some(Arc::new(view)));
        fiber.children = children.clone();
        let id = arena.insert(fiber);
        for child in children {
            arena.get_mut(child).unwrap().parent = Some(id);
        }
        id
    }

    fn run(arena: &mut FiberArena, root: FiberId) {
        layout_fiber(
            arena,
            root,
            BoxConstraints::loose(Size::new(100.0, 100.0)),
            Point::ZERO,
            &DEFAULT_TEXT_METRICS,
        );
    }

    fn rect(arena: &FiberArena, id: FiberId) -> Rect {
        arena.get(id).unwrap().layout_rect().unwrap()
    }

    fn assert_zero_fallback(arena: &FiberArena, ids: &[FiberId]) {
        for &id in ids {
            let rect = rect(arena, id);
            assert_eq!(rect.size(), Size::ZERO);
            assert!(!rect.contains(Point::ZERO));
            assert!(!rect.contains(rect.min));
        }
    }

    fn recording_view(label: &'static str, measurements: Measurements) -> TestView {
        view(move |constraints, children| {
            measurements.borrow_mut().push((label, constraints));
            let mut result = ordinary_layout(constraints, children)?;
            for (_, origin) in &mut result.placements {
                *origin = Point::new(constraints.min.width / 10.0, 1.0);
            }
            result.diagnostics.push(LayoutDiagnostic::Overflow {
                main: constraints.min.width,
                cross: 0.0,
            });
            Ok(result)
        })
    }

    #[test]
    fn reverse_measurement_and_cached_selection_commit_only_selected_descendants() {
        for select_first_again in [false, true] {
            let mut arena = FiberArena::new();
            let measurements = Measurements::default();
            let grandchild = insert(
                &mut arena,
                recording_view("grandchild", measurements.clone()),
                vec![],
            );
            let left = insert(
                &mut arena,
                recording_view("left", measurements.clone()),
                vec![grandchild],
            );
            let right = insert(
                &mut arena,
                recording_view("right", measurements.clone()),
                vec![],
            );
            let initial = BoxConstraints::tight(Size::new(10.0, 5.0));
            let corrected = BoxConstraints::tight(Size::new(40.0, 7.0));
            let right_constraints = BoxConstraints::tight(Size::new(20.0, 30.0));
            let root = insert(
                &mut arena,
                view(move |_, children| {
                    assert_eq!(children.len(), 2);
                    assert_eq!(children.parent_data(0)?, ParentData::default());
                    assert_eq!(
                        children.measure(1, right_constraints)?,
                        right_constraints.min
                    );
                    children.measure(0, initial)?;
                    children.measure(0, initial)?;
                    assert_eq!(children.measure(0, corrected)?, corrected.min);
                    let final_constraints = if select_first_again {
                        initial
                    } else {
                        corrected
                    };
                    for _ in 0..16 {
                        assert_eq!(
                            children.measure(0, final_constraints)?,
                            final_constraints.min
                        );
                    }
                    Ok(ParentLayout {
                        size: Size::new(100.0, 100.0),
                        placements: vec![(1, Point::new(50.0, 4.0)), (0, Point::new(3.0, 2.0))],
                        diagnostics: vec![],
                    })
                }),
                vec![left, right],
            );

            layout_fiber(
                &mut arena,
                root,
                BoxConstraints::loose(Size::new(100.0, 100.0)),
                Point::new(10.0, 20.0),
                &DEFAULT_TEXT_METRICS,
            );

            assert_eq!(arena.get(root).unwrap().layout_error(), None);
            assert_eq!(
                *measurements.borrow(),
                vec![
                    ("right", right_constraints),
                    ("left", initial),
                    ("grandchild", initial),
                    ("left", corrected),
                    ("grandchild", corrected),
                ]
            );
            let selected = if select_first_again {
                initial
            } else {
                corrected
            };
            assert_eq!(
                rect(&arena, left),
                Rect::from_min_size(Point::new(13.0, 22.0), selected.min)
            );
            assert_eq!(
                rect(&arena, grandchild),
                Rect::from_min_size(
                    Point::new(13.0 + selected.min.width / 10.0, 23.0),
                    selected.min
                )
            );
            assert_eq!(
                rect(&arena, right),
                Rect::from_min_size(Point::new(60.0, 24.0), right_constraints.min)
            );
            assert_eq!(arena.get(root).unwrap().children(), &[left, right]);
            assert_eq!(
                arena.get(grandchild).unwrap().layout_diagnostics(),
                &[LayoutDiagnostic::Overflow {
                    main: selected.min.width,
                    cross: 0.0
                }]
            );
        }
    }

    #[test]
    fn phase_cache_reuses_each_mode_and_legacy_adapters_inherit_provisional_mode() {
        use crate::widgets::padding::Padding;

        let mut arena = FiberArena::new();
        let phases = Rc::new(RefCell::new(Vec::new()));
        let leaf_phases = phases.clone();
        let leaf = insert(
            &mut arena,
            view(move |_, children| {
                let provisional = children.is_provisional();
                leaf_phases.borrow_mut().push(provisional);
                let width = if provisional { 10.0 } else { 20.0 };
                Ok(ParentLayout {
                    size: Size::new(width, 10.0),
                    placements: vec![],
                    diagnostics: vec![LayoutDiagnostic::Overflow {
                        main: width,
                        cross: 0.0,
                    }],
                })
            }),
            vec![],
        );
        let adapter = insert(&mut arena, Padding::all(0.0), vec![leaf]);
        let root = insert(
            &mut arena,
            view(|constraints, children| {
                assert!(!children.is_provisional());
                for provisional in [true, true, false, false, true, false] {
                    let size = if provisional {
                        children.measure_provisional(0, constraints)?
                    } else {
                        children.measure(0, constraints)?
                    };
                    assert_eq!(size, Size::new(if provisional { 10.0 } else { 20.0 }, 10.0));
                }
                Ok(ParentLayout {
                    size: constraints.max,
                    placements: vec![(0, Point::ZERO)],
                    diagnostics: vec![],
                })
            }),
            vec![adapter],
        );
        run(&mut arena, root);
        assert_eq!(*phases.borrow(), [true, false]);
        assert_eq!(arena.get(root).unwrap().layout_error(), None);
        assert_eq!(rect(&arena, leaf).size(), Size::new(20.0, 10.0));
        assert_eq!(rect(&arena, adapter), rect(&arena, leaf));
        assert_eq!(
            arena.get(leaf).unwrap().layout_diagnostics(),
            &[LayoutDiagnostic::Overflow {
                main: 20.0,
                cross: 0.0
            }]
        );
    }

    #[test]
    fn provisional_selection_never_commits_and_preserves_prior_geometry() {
        for prior_layout in [false, true] {
            let mut arena = FiberArena::new();
            let leave_provisional = Rc::new(Cell::new(false));
            let parent_mode = leave_provisional.clone();
            let child = insert(&mut arena, view(ordinary_layout), vec![]);
            let root = insert(
                &mut arena,
                view(move |constraints, children| {
                    children.measure(0, BoxConstraints::tight(Size::new(10.0, 10.0)))?;
                    if parent_mode.get() {
                        children
                            .measure_provisional(0, BoxConstraints::tight(Size::new(20.0, 10.0)))?;
                    }
                    Ok(ParentLayout {
                        size: constraints.max,
                        placements: vec![(0, Point::ZERO)],
                        diagnostics: vec![],
                    })
                }),
                vec![child],
            );
            if prior_layout {
                run(&mut arena, root);
            }
            let previous = if prior_layout {
                Some([rect(&arena, root), rect(&arena, child)])
            } else {
                None
            };
            leave_provisional.set(true);
            run(&mut arena, root);
            assert_eq!(
                arena.get(root).unwrap().layout_error(),
                Some(LayoutError::UnfinalizedMeasurement)
            );
            assert_eq!(
                arena.get(child).unwrap().layout_error(),
                Some(LayoutError::UnfinalizedMeasurement)
            );
            if let Some(previous) = previous {
                assert_eq!([rect(&arena, root), rect(&arena, child)], previous);
            } else {
                assert_zero_fallback(&arena, &[root, child]);
            }
            leave_provisional.set(false);
            run(&mut arena, root);
            assert_eq!(arena.get(root).unwrap().layout_error(), None);
            assert_eq!(arena.get(child).unwrap().layout_error(), None);
        }
    }

    #[test]
    fn cached_provisional_descendant_cannot_hide_under_final_parent_measurements() {
        let mut arena = FiberArena::new();
        let phases = Rc::new(RefCell::new(Vec::new()));
        let leaf_phases = phases.clone();
        let leaf = insert(
            &mut arena,
            view(move |constraints, children| {
                leaf_phases.borrow_mut().push(children.is_provisional());
                ordinary_layout(constraints, children)
            }),
            vec![],
        );
        let middle = insert(
            &mut arena,
            view(|constraints, children| {
                assert!(!children.is_provisional());
                children.measure_provisional(0, constraints)?;
                children.measure(0, constraints)?;
                // Reselecting the cached natural subtree must not mark it finalized.
                children.measure_provisional(0, constraints)?;
                Ok(ParentLayout {
                    size: constraints.max,
                    placements: vec![(0, Point::ZERO)],
                    diagnostics: vec![],
                })
            }),
            vec![leaf],
        );
        let root = insert(&mut arena, view(ordinary_layout), vec![middle]);
        run(&mut arena, root);
        assert_eq!(*phases.borrow(), [true, false]);
        assert_eq!(
            arena.get(root).unwrap().layout_error(),
            Some(LayoutError::UnfinalizedMeasurement)
        );
        assert_eq!(
            arena.get(leaf).unwrap().layout_error(),
            Some(LayoutError::UnfinalizedMeasurement)
        );
        assert_zero_fallback(&arena, &[root, middle, leaf]);
    }

    #[test]
    fn phase_switch_consumes_the_second_cache_entry_and_third_request_is_sticky() {
        let mut arena = FiberArena::new();
        let calls = Rc::new(Cell::new(0));
        let child_calls = calls.clone();
        let child = insert(
            &mut arena,
            view(move |constraints, children| {
                child_calls.set(child_calls.get() + 1);
                ordinary_layout(constraints, children)
            }),
            vec![],
        );
        let root = insert(
            &mut arena,
            view(|constraints, children| {
                children.measure_provisional(0, constraints)?;
                children.measure(0, constraints)?;
                assert_eq!(
                    children.measure(0, BoxConstraints::tight(Size::new(20.0, 20.0))),
                    Err(LayoutError::MeasurementLimit)
                );
                assert_eq!(
                    children.measure_provisional(0, constraints),
                    Err(LayoutError::MeasurementLimit)
                );
                Ok(ParentLayout {
                    size: constraints.max,
                    placements: vec![(0, Point::ZERO)],
                    diagnostics: vec![],
                })
            }),
            vec![child],
        );
        run(&mut arena, root);
        assert_eq!(calls.get(), 2);
        assert_eq!(
            arena.get(root).unwrap().layout_error(),
            Some(LayoutError::MeasurementLimit)
        );
        assert_zero_fallback(&arena, &[root, child]);
    }

    #[test]
    fn swallowed_provisional_measurement_error_cannot_reuse_a_successful_final_cache() {
        let mut arena = FiberArena::new();
        let child = insert(
            &mut arena,
            view(|constraints, children| {
                if children.is_provisional() {
                    return Err(LayoutError::InvalidGeometry);
                }
                ordinary_layout(constraints, children)
            }),
            vec![],
        );
        let root = insert(
            &mut arena,
            view(|constraints, children| {
                children.measure(0, constraints)?;
                assert_eq!(
                    children.measure_provisional(0, constraints),
                    Err(LayoutError::InvalidGeometry)
                );
                assert_eq!(
                    children.measure(0, constraints),
                    Err(LayoutError::InvalidGeometry)
                );
                Ok(ParentLayout {
                    size: constraints.max,
                    placements: vec![(0, Point::ZERO)],
                    diagnostics: vec![],
                })
            }),
            vec![child],
        );
        run(&mut arena, root);
        assert_eq!(
            arena.get(root).unwrap().layout_error(),
            Some(LayoutError::InvalidGeometry)
        );
        assert_eq!(
            arena.get(child).unwrap().layout_error(),
            Some(LayoutError::InvalidGeometry)
        );
        assert_zero_fallback(&arena, &[root, child]);
    }

    #[test]
    fn rejects_missing_duplicate_unmeasured_and_out_of_range_placements() {
        for (placements, measured, expected) in [
            (vec![], true, LayoutError::MissingPlacement),
            (
                vec![(0, Point::ZERO), (0, Point::ZERO)],
                true,
                LayoutError::DuplicatePlacement,
            ),
            (vec![(0, Point::ZERO)], false, LayoutError::UnmeasuredChild),
            (vec![(1, Point::ZERO)], true, LayoutError::InvalidChildIndex),
        ] {
            let mut arena = FiberArena::new();
            let child = insert(&mut arena, view(ordinary_layout), vec![]);
            let root = insert(
                &mut arena,
                view(move |constraints, children| {
                    if measured {
                        children.measure(0, constraints)?;
                    }
                    Ok(ParentLayout {
                        size: Size::new(20.0, 10.0),
                        placements: placements.clone(),
                        diagnostics: vec![],
                    })
                }),
                vec![child],
            );
            run(&mut arena, root);
            assert_eq!(arena.get(root).unwrap().layout_error(), Some(expected));
            assert_zero_fallback(&arena, &[root, child]);
        }
    }

    #[test]
    fn third_distinct_measurement_is_sticky_even_when_parent_ignores_error() {
        let mut arena = FiberArena::new();
        let measurements = Measurements::default();
        let child = insert(
            &mut arena,
            recording_view("child", measurements.clone()),
            vec![],
        );
        let root = insert(
            &mut arena,
            view(|_, children| {
                for width in [10.0, 20.0] {
                    children.measure(0, BoxConstraints::tight(Size::new(width, 10.0)))?;
                }
                assert_eq!(
                    children.measure(0, BoxConstraints::tight(Size::new(30.0, 10.0))),
                    Err(LayoutError::MeasurementLimit)
                );
                assert_eq!(
                    children.measure(0, BoxConstraints::tight(Size::new(10.0, 10.0))),
                    Err(LayoutError::MeasurementLimit)
                );
                Ok(ParentLayout {
                    size: Size::new(40.0, 10.0),
                    placements: vec![(0, Point::ZERO)],
                    diagnostics: vec![],
                })
            }),
            vec![child],
        );
        run(&mut arena, root);
        assert_eq!(
            arena.get(root).unwrap().layout_error(),
            Some(LayoutError::MeasurementLimit)
        );
        assert_eq!(measurements.borrow().len(), 2);
        assert_zero_fallback(&arena, &[root, child]);
    }

    #[test]
    fn invalid_index_is_sticky_for_measurement_and_parent_data_queries() {
        for metadata_query in [false, true] {
            let mut arena = FiberArena::new();
            let child = insert(&mut arena, view(ordinary_layout), vec![]);
            let root = insert(
                &mut arena,
                view(move |constraints, children| {
                    children.measure(0, constraints)?;
                    if metadata_query {
                        assert_eq!(children.parent_data(1), Err(LayoutError::InvalidChildIndex));
                    } else {
                        assert_eq!(
                            children.measure(1, constraints),
                            Err(LayoutError::InvalidChildIndex)
                        );
                    }
                    Ok(ParentLayout {
                        size: Size::ZERO,
                        placements: vec![(0, Point::ZERO)],
                        diagnostics: vec![],
                    })
                }),
                vec![child],
            );
            run(&mut arena, root);
            assert_eq!(
                arena.get(root).unwrap().layout_error(),
                Some(LayoutError::InvalidChildIndex)
            );
            assert_zero_fallback(&arena, &[root, child]);
        }
    }

    #[test]
    fn invalid_constraints_are_rejected_at_root_and_child_boundary() {
        let invalid = [
            BoxConstraints::loose(Size::new(f32::NAN, 10.0)),
            BoxConstraints::loose(Size::new(f32::NEG_INFINITY, 10.0)),
            BoxConstraints::loose(Size::new(-1.0, 10.0)),
            BoxConstraints::tight(Size::new(f32::INFINITY, 10.0)),
            BoxConstraints {
                min: Size::new(-1.0, 0.0),
                max: Size::new(10.0, 10.0),
            },
            BoxConstraints {
                min: Size::new(11.0, 0.0),
                max: Size::new(10.0, 10.0),
            },
            BoxConstraints {
                min: Size::new(0.0, f32::NAN),
                max: Size::new(10.0, 10.0),
            },
        ];
        for constraints in invalid {
            for at_root in [true, false] {
                let mut arena = FiberArena::new();
                let child = insert(&mut arena, view(ordinary_layout), vec![]);
                let root = insert(
                    &mut arena,
                    view(move |_, children| {
                        let _ = children.measure(0, constraints);
                        Ok(ParentLayout {
                            size: Size::ZERO,
                            placements: vec![(0, Point::ZERO)],
                            diagnostics: vec![],
                        })
                    }),
                    vec![child],
                );
                layout_fiber(
                    &mut arena,
                    root,
                    if at_root {
                        constraints
                    } else {
                        BoxConstraints::loose(Size::new(100.0, 100.0))
                    },
                    Point::ZERO,
                    &DEFAULT_TEXT_METRICS,
                );
                assert_eq!(
                    arena.get(root).unwrap().layout_error(),
                    Some(LayoutError::InvalidConstraints)
                );
                assert_zero_fallback(&arena, &[root, child]);
            }
        }
    }

    #[test]
    fn unbounded_maxima_are_valid_but_infinite_and_out_of_constraint_sizes_are_not() {
        for (size, expected) in [
            (Size::new(20.0, 10.0), None),
            (
                Size::new(f32::INFINITY, 10.0),
                Some(LayoutError::InvalidGeometry),
            ),
            (
                Size::new(f32::NAN, 10.0),
                Some(LayoutError::InvalidGeometry),
            ),
            (Size::new(-1.0, 10.0), Some(LayoutError::InvalidGeometry)),
            (Size::new(20.0, 101.0), Some(LayoutError::InvalidGeometry)),
            (Size::new(20.0, 0.0), Some(LayoutError::InvalidGeometry)),
        ] {
            let mut arena = FiberArena::new();
            let root = insert(
                &mut arena,
                view(move |_, _| {
                    Ok(ParentLayout {
                        size,
                        placements: vec![],
                        diagnostics: vec![],
                    })
                }),
                vec![],
            );
            layout_fiber(
                &mut arena,
                root,
                BoxConstraints {
                    min: Size::new(0.0, 1.0),
                    max: Size::new(f32::INFINITY, 100.0),
                },
                Point::ZERO,
                &DEFAULT_TEXT_METRICS,
            );
            assert_eq!(arena.get(root).unwrap().layout_error(), expected);
            assert!(valid_size(rect(&arena, root).size()));
        }
    }

    #[test]
    fn nonfinite_origins_and_invalid_overflow_extents_never_commit() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for bad_origin in [false, true] {
                let mut arena = FiberArena::new();
                let child = insert(&mut arena, view(ordinary_layout), vec![]);
                let root = insert(
                    &mut arena,
                    view(move |constraints, children| {
                        let mut result = ordinary_layout(constraints, children)?;
                        if bad_origin {
                            result.placements[0].1.x = bad;
                        } else {
                            result.diagnostics.push(LayoutDiagnostic::Overflow {
                                main: 0.0,
                                cross: bad,
                            });
                        }
                        Ok(result)
                    }),
                    vec![child],
                );
                run(&mut arena, root);
                assert_eq!(
                    arena.get(root).unwrap().layout_error(),
                    Some(LayoutError::InvalidGeometry)
                );
                assert_zero_fallback(&arena, &[root, child]);
            }
        }
        let mut arena = FiberArena::new();
        let root = insert(
            &mut arena,
            view(|constraints, children| {
                let mut result = ordinary_layout(constraints, children)?;
                result.diagnostics.push(LayoutDiagnostic::Overflow {
                    main: -1.0,
                    cross: 0.0,
                });
                Ok(result)
            }),
            vec![],
        );
        run(&mut arena, root);
        assert_eq!(
            arena.get(root).unwrap().layout_error(),
            Some(LayoutError::InvalidGeometry)
        );
    }

    #[test]
    fn descendant_measurement_failure_preserves_all_geometry_and_diagnostics_then_recovers() {
        let mut arena = FiberArena::new();
        let invalid = Rc::new(Cell::new(false));
        let invalid_child = invalid.clone();
        let left = insert(&mut arena, view(ordinary_layout), vec![]);
        let right = insert(
            &mut arena,
            view(move |constraints, children| {
                let mut result = ordinary_layout(constraints, children)?;
                if invalid_child.get() {
                    result.size.width = f32::NAN;
                }
                Ok(result)
            }),
            vec![],
        );
        let mode = Rc::new(Cell::new(0));
        let parent_mode = mode.clone();
        let root = insert(
            &mut arena,
            view(move |constraints, children| {
                // A previously measured sibling changes, but may not partially commit.
                children.measure(
                    0,
                    BoxConstraints::tight(Size::new(10.0 + parent_mode.get() as f32, 10.0)),
                )?;
                // Swallow the descendant's error deliberately.
                let _ = children.measure(1, BoxConstraints::tight(Size::new(10.0, 10.0)));
                Ok(ParentLayout {
                    size: constraints.max,
                    placements: vec![(0, Point::ZERO), (1, Point::new(10.0, 0.0))],
                    diagnostics: if parent_mode.get() == 0 {
                        vec![LayoutDiagnostic::Overflow {
                            main: 1.0,
                            cross: 2.0,
                        }]
                    } else {
                        vec![]
                    },
                })
            }),
            vec![left, right],
        );
        run(&mut arena, root);
        let old_rects = [root, left, right].map(|id| rect(&arena, id));
        let old_diagnostics = arena.get(root).unwrap().layout_diagnostics().to_vec();

        invalid.set(true);
        mode.set(1);
        run(&mut arena, root);

        assert_eq!([root, left, right].map(|id| rect(&arena, id)), old_rects);
        assert_eq!(
            arena.get(root).unwrap().layout_diagnostics(),
            old_diagnostics
        );
        assert_eq!(
            arena.get(root).unwrap().layout_error(),
            Some(LayoutError::InvalidGeometry)
        );
        assert_eq!(
            arena.get(right).unwrap().layout_error(),
            Some(LayoutError::InvalidGeometry)
        );
        invalid.set(false);
        run(&mut arena, root);
        for id in [root, left, right] {
            assert_eq!(arena.get(id).unwrap().layout_error(), None);
        }
        assert!(arena.get(root).unwrap().layout_diagnostics().is_empty());
        assert_eq!(rect(&arena, left).size().width, 11.0);
    }

    #[test]
    fn absolute_edge_and_origin_overflow_abort_before_any_rect_is_applied() {
        for origin_overflow in [false, true] {
            let mut arena = FiberArena::new();
            let large = Rc::new(Cell::new(false));
            let large_parent = large.clone();
            let child = insert(&mut arena, view(ordinary_layout), vec![]);
            let root = insert(
                &mut arena,
                view(move |_, children| {
                    children.measure(
                        0,
                        BoxConstraints::tight(Size::new(
                            if large_parent.get() { f32::MAX } else { 10.0 },
                            1.0,
                        )),
                    )?;
                    Ok(ParentLayout {
                        size: Size::new(10.0, 10.0),
                        placements: vec![(
                            0,
                            Point::new(if large_parent.get() { f32::MAX } else { 0.0 }, 0.0),
                        )],
                        diagnostics: vec![],
                    })
                }),
                vec![child],
            );
            run(&mut arena, root);
            let before = [rect(&arena, root), rect(&arena, child)];
            large.set(true);
            layout_fiber(
                &mut arena,
                root,
                BoxConstraints::loose(Size::new(100.0, 100.0)),
                if origin_overflow {
                    Point::new(f32::MAX, 0.0)
                } else {
                    Point::ZERO
                },
                &DEFAULT_TEXT_METRICS,
            );
            assert_eq!(
                arena.get(root).unwrap().layout_error(),
                Some(LayoutError::InvalidGeometry)
            );
            assert_eq!([rect(&arena, root), rect(&arena, child)], before);
        }
    }

    #[test]
    fn stale_children_and_recursive_or_aliased_fibers_fail_but_stale_root_is_noop() {
        for expected in [
            LayoutError::StaleChild,
            LayoutError::RecursiveMeasurement,
            LayoutError::DuplicateChild,
        ] {
            let mut arena = FiberArena::new();
            let child = insert(&mut arena, view(ordinary_layout), vec![]);
            let root = insert(&mut arena, view(ordinary_layout), vec![child]);
            match expected {
                LayoutError::StaleChild => {
                    arena.remove(child);
                }
                LayoutError::RecursiveMeasurement => {
                    arena.get_mut(child).unwrap().children.push(root)
                }
                LayoutError::DuplicateChild => arena.get_mut(root).unwrap().children.push(child),
                _ => unreachable!(),
            }
            run(&mut arena, root);
            assert_eq!(arena.get(root).unwrap().layout_error(), Some(expected));
            assert_zero_fallback(&arena, &[root]);
            arena.remove(root);
            run(&mut arena, root);
        }
    }

    fn remeasuring_tree(
        arena: &mut FiberArena,
        depth: usize,
        branches: usize,
        calls: Rc<Cell<usize>>,
    ) -> FiberId {
        let children = if depth == 0 {
            vec![]
        } else {
            (0..branches)
                .map(|_| remeasuring_tree(arena, depth - 1, branches, calls.clone()))
                .collect()
        };
        insert(
            arena,
            view(move |constraints, children| {
                calls.set(calls.get() + 1);
                for index in 0..children.len() {
                    children.measure(
                        index,
                        BoxConstraints::tight(Size::new(constraints.max.width / 2.0, 10.0)),
                    )?;
                    children.measure(
                        index,
                        BoxConstraints::tight(Size::new(constraints.max.width / 4.0, 10.0)),
                    )?;
                }
                Ok(ParentLayout {
                    size: constraints.constrain(Size::ZERO),
                    placements: (0..children.len())
                        .map(|index| (index, Point::ZERO))
                        .collect(),
                    diagnostics: vec![],
                })
            }),
            children,
        )
    }

    #[test]
    fn shared_budget_bounds_nested_chain_and_branching_remeasurement() {
        for (depth, branches) in [(8, 1), (4, 2)] {
            let mut arena = FiberArena::new();
            let calls = Rc::new(Cell::new(0));
            let root = remeasuring_tree(&mut arena, depth, branches, calls.clone());
            let nodes = reachable_nodes(&arena, root);
            run(&mut arena, root);
            assert_eq!(
                arena.get(root).unwrap().layout_error(),
                Some(LayoutError::WorkBudgetExceeded)
            );
            assert_eq!(calls.get(), nodes.len() * 8);
            assert_zero_fallback(&arena, &nodes);
        }
    }

    #[test]
    fn legacy_adapter_passes_child_constraints_once_and_rejects_origin_count_mismatch() {
        struct Legacy {
            origins: usize,
            calls: Rc<Cell<usize>>,
        }
        impl AnyView for Legacy {
            fn key(&self) -> Option<&Key> {
                None
            }
            fn widget_type(&self) -> TypeId {
                TypeId::of::<Self>()
            }
            fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
                constraints.constrain(Size::ZERO)
            }
            fn child_constraints(&self, constraints: BoxConstraints) -> BoxConstraints {
                constraints.deflate(Size::new(30.0, 30.0))
            }
            fn layout_children(
                &self,
                constraints: BoxConstraints,
                sizes: &[Size],
                _: &TextMetrics,
            ) -> (Size, Vec<Point>) {
                self.calls.set(self.calls.get() + 1);
                assert_eq!(sizes, &[Size::new(40.0, 20.0)]);
                (
                    constraints.constrain(Size::ZERO),
                    vec![Point::new(15.0, 15.0); self.origins],
                )
            }
        }
        for (origins, expected) in [
            (0, Some(LayoutError::MissingPlacement)),
            (1, None),
            (2, Some(LayoutError::InvalidChildIndex)),
        ] {
            let mut arena = FiberArena::new();
            let measurements = Measurements::default();
            let calls = Rc::new(Cell::new(0));
            let child = insert(
                &mut arena,
                recording_view("child", measurements.clone()),
                vec![],
            );
            let root = insert(
                &mut arena,
                Legacy {
                    origins,
                    calls: calls.clone(),
                },
                vec![child],
            );
            layout_fiber(
                &mut arena,
                root,
                BoxConstraints::tight(Size::new(70.0, 50.0)),
                Point::ZERO,
                &DEFAULT_TEXT_METRICS,
            );
            assert_eq!(calls.get(), 1);
            assert_eq!(
                *measurements.borrow(),
                vec![("child", BoxConstraints::tight(Size::new(40.0, 20.0)))]
            );
            assert_eq!(arena.get(root).unwrap().layout_error(), expected);
            if expected.is_none() {
                assert_eq!(
                    rect(&arena, child),
                    Rect::from_min_size(Point::new(15.0, 15.0), Size::new(40.0, 20.0))
                );
            } else {
                assert_zero_fallback(&arena, &[root, child]);
            }
        }
    }

    #[test]
    fn nonfinite_root_origin_uses_zero_fallback_without_invoking_parent() {
        for origin in [
            Point::new(f32::NAN, 0.0),
            Point::new(0.0, f32::INFINITY),
            Point::new(f32::NEG_INFINITY, 0.0),
        ] {
            let mut arena = FiberArena::new();
            let measurements = Measurements::default();
            let root = insert(
                &mut arena,
                recording_view("root", measurements.clone()),
                vec![],
            );
            layout_fiber(
                &mut arena,
                root,
                BoxConstraints::loose(Size::new(10.0, 10.0)),
                origin,
                &DEFAULT_TEXT_METRICS,
            );
            assert_eq!(
                arena.get(root).unwrap().layout_error(),
                Some(LayoutError::InvalidGeometry)
            );
            assert!(measurements.borrow().is_empty());
            assert_zero_fallback(&arena, &[root]);
        }
    }

    #[test]
    fn ordinary_wide_tree_fits_shared_budget() {
        let mut arena = FiberArena::new();
        let measurements = Measurements::default();
        let children = (0..256)
            .map(|_| {
                insert(
                    &mut arena,
                    recording_view("leaf", measurements.clone()),
                    vec![],
                )
            })
            .collect();
        let root = insert(
            &mut arena,
            recording_view("root", measurements.clone()),
            children,
        );
        run(&mut arena, root);
        assert_eq!(arena.get(root).unwrap().layout_error(), None);
        assert_eq!(measurements.borrow().len(), 257);
    }

    #[test]
    fn built_in_unbounded_cross_stretch_chains_stay_within_shared_budget() {
        use crate::layout::Alignment;
        use crate::widgets::column::Column;
        use crate::widgets::row::Row;
        use crate::widgets::sized_box::SizedBox;

        for horizontal in [false, true] {
            // Includes correction at every level, not only constant-cross chains.
            for (depth, growing_cross) in [
                (1, false),
                (8, false),
                (32, false),
                (8, true),
                (16, true),
                (64, true),
            ] {
                let mut arena = FiberArena::new();
                let axis_size = |main, cross| {
                    if horizontal {
                        Size::new(main, cross)
                    } else {
                        Size::new(cross, main)
                    }
                };
                let leaf = insert(&mut arena, SizedBox::new(axis_size(3.0, 7.0)), vec![]);
                let mut root = leaf;
                for level in 0..depth {
                    let cross = 20.0 + if growing_cross { level as f32 } else { 0.0 };
                    let sibling = insert(&mut arena, SizedBox::new(axis_size(2.0, cross)), vec![]);
                    root = if horizontal {
                        insert(
                            &mut arena,
                            Row::new().cross_axis_alignment(Alignment::Stretch),
                            vec![root, sibling],
                        )
                    } else {
                        insert(
                            &mut arena,
                            Column::new().cross_axis_alignment(Alignment::Stretch),
                            vec![root, sibling],
                        )
                    };
                }
                layout_fiber(
                    &mut arena,
                    root,
                    BoxConstraints::loose(axis_size(1000.0, f32::INFINITY)),
                    Point::ZERO,
                    &DEFAULT_TEXT_METRICS,
                );
                assert_eq!(
                    arena.get(root).unwrap().layout_error(),
                    None,
                    "horizontal={horizontal}, depth={depth}, growing_cross={growing_cross}"
                );
                let cross = 20.0
                    + if growing_cross {
                        (depth - 1) as f32
                    } else {
                        0.0
                    };
                assert_eq!(
                    rect(&arena, root).size(),
                    axis_size(3.0 + depth as f32 * 2.0, cross)
                );
                assert_eq!(rect(&arena, leaf).size(), axis_size(3.0, cross));
                for id in reachable_nodes(&arena, root) {
                    assert!(valid_size(rect(&arena, id).size()));
                    assert!(valid_point(rect(&arena, id).max));
                    assert_eq!(arena.get(id).unwrap().layout_error(), None);
                }
            }
        }
    }

    fn measured_work(arena: &FiberArena, root: FiberId, constraints: BoxConstraints) -> usize {
        let budget = reachable_nodes(arena, root).len() * 8;
        let mut pass = LayoutPass {
            arena,
            metrics: &DEFAULT_TEXT_METRICS,
            remaining: budget,
            active: HashSet::new(),
            failure: Cell::new(None),
        };
        let measured = pass.measure(root, constraints, false).unwrap();
        prepare_commit(arena, &measured, Point::ZERO)
            .unwrap_or_else(|failure| panic!("uncommittable measurement: {:?}", failure.error));
        budget - pass.remaining
    }

    #[test]
    fn deep_staggered_builtin_stretch_chains_use_one_natural_and_one_final_traversal() {
        use crate::layout::Alignment;
        use crate::widgets::column::Column;
        use crate::widgets::row::Row;
        use crate::widgets::sized_box::SizedBox;

        for horizontal in [false, true] {
            for depth in [15, 16, 64] {
                let mut arena = FiberArena::new();
                let axis_size = |main, cross| {
                    if horizontal {
                        Size::new(main, cross)
                    } else {
                        Size::new(cross, main)
                    }
                };
                let leaf = insert(&mut arena, SizedBox::new(axis_size(1.0, 0.0)), vec![]);
                let mut root = leaf;
                for level in 1..=depth {
                    let sibling = insert(
                        &mut arena,
                        SizedBox::new(axis_size(1.0, level as f32)),
                        vec![],
                    );
                    root = if horizontal {
                        insert(
                            &mut arena,
                            Row::new().cross_axis_alignment(Alignment::Stretch),
                            vec![root, sibling],
                        )
                    } else {
                        insert(
                            &mut arena,
                            Column::new().cross_axis_alignment(Alignment::Stretch),
                            vec![root, sibling],
                        )
                    };
                }
                let constraints = BoxConstraints::loose(axis_size(100.0, f32::INFINITY));
                let nodes = reachable_nodes(&arena, root);
                let visits = measured_work(&arena, root, constraints);
                assert_eq!(visits, 2 * nodes.len() - 1);
                assert!(visits <= 8 * nodes.len());
                eprintln!(
                    "stretch chain horizontal={horizontal} depth={depth}: nodes={}, visits={visits}, budget={}",
                    nodes.len(),
                    8 * nodes.len()
                );

                layout_fiber(
                    &mut arena,
                    root,
                    constraints,
                    Point::ZERO,
                    &DEFAULT_TEXT_METRICS,
                );
                assert_eq!(arena.get(root).unwrap().layout_error(), None);
                assert_eq!(
                    rect(&arena, root).size(),
                    axis_size((depth + 1) as f32, depth as f32)
                );
                assert_eq!(rect(&arena, leaf).size(), axis_size(1.0, depth as f32));
                for id in nodes {
                    let rect = rect(&arena, id);
                    assert!(valid_size(rect.size()));
                    assert!(valid_point(rect.min) && valid_point(rect.max));
                    assert_eq!(
                        if horizontal {
                            rect.size().height
                        } else {
                            rect.size().width
                        },
                        depth as f32
                    );
                    assert_eq!(arena.get(id).unwrap().layout_error(), None);
                }
            }
        }
    }

    #[test]
    fn alternating_axis_stretch_chains_finalize_both_axes_within_budget() {
        use crate::layout::Alignment;
        use crate::widgets::column::Column;
        use crate::widgets::row::Row;
        use crate::widgets::sized_box::SizedBox;

        for depth in [16, 64] {
            let mut arena = FiberArena::new();
            let mut root = insert(&mut arena, SizedBox::new(Size::new(1.0, 1.0)), vec![]);
            for level in 1..=depth {
                let sibling = insert(
                    &mut arena,
                    SizedBox::new(Size::new(level as f32, level as f32)),
                    vec![],
                );
                root = if level % 2 == 0 {
                    insert(
                        &mut arena,
                        Row::new().cross_axis_alignment(Alignment::Stretch),
                        vec![root, sibling],
                    )
                } else {
                    insert(
                        &mut arena,
                        Column::new().cross_axis_alignment(Alignment::Stretch),
                        vec![root, sibling],
                    )
                };
            }
            let constraints = BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY));
            let nodes = reachable_nodes(&arena, root);
            let visits = measured_work(&arena, root, constraints);
            assert!(visits <= 8 * nodes.len());
            eprintln!(
                "alternating stretch depth={depth}: nodes={}, visits={visits}, budget={}",
                nodes.len(),
                8 * nodes.len()
            );
            layout_fiber(
                &mut arena,
                root,
                constraints,
                Point::ZERO,
                &DEFAULT_TEXT_METRICS,
            );
            for id in nodes {
                assert_eq!(arena.get(id).unwrap().layout_error(), None);
                let rect = rect(&arena, id);
                assert!(valid_size(rect.size()));
                assert!(valid_point(rect.min) && valid_point(rect.max));
            }
        }
    }

    #[test]
    fn real_flexible_and_expanded_roots_fall_back_to_ordinary_layout_with_diagnostic() {
        use crate::widgets::flexible::{Expanded, Flexible};
        use crate::widgets::sized_box::SizedBox;

        for tight in [false, true] {
            let mut arena = FiberArena::new();
            let child = insert(&mut arena, SizedBox::new(Size::new(20.0, 10.0)), vec![]);
            let root = if tight {
                insert(&mut arena, Expanded::new(), vec![child])
            } else {
                insert(&mut arena, Flexible::new(), vec![child])
            };
            run(&mut arena, root);
            assert_eq!(arena.get(root).unwrap().layout_error(), None);
            assert_eq!(
                arena.get(root).unwrap().layout_diagnostics(),
                &[LayoutDiagnostic::InvalidFlexParent]
            );
            assert_eq!(rect(&arena, root).size(), Size::new(20.0, 10.0));
            assert_eq!(rect(&arena, root), rect(&arena, child));
        }
    }

    fn flex_data() -> ParentData {
        ParentData {
            flex: Some(FlexParentData {
                factor: NonZeroU32::new(1).unwrap(),
                fit: FlexFit::Tight,
            }),
        }
    }

    #[test]
    fn immediate_parent_diagnostics_do_not_tunnel_and_clear_after_successful_change() {
        let mut arena = FiberArena::new();
        let mut wrapper_view = view(ordinary_layout);
        wrapper_view.data = flex_data();
        let wrapper = insert(&mut arena, wrapper_view, vec![]);
        let ordinary_parent = insert(&mut arena, view(ordinary_layout), vec![wrapper]);
        let mut accepting_view = view(ordinary_layout);
        accepting_view.accepts_flex = true;
        let root = insert(&mut arena, accepting_view, vec![ordinary_parent]);
        run(&mut arena, root);
        assert!(arena.get(root).unwrap().layout_diagnostics().is_empty());
        assert_eq!(
            arena.get(ordinary_parent).unwrap().layout_diagnostics(),
            &[LayoutDiagnostic::InvalidFlexParent]
        );
        assert!(arena.get(wrapper).unwrap().layout_diagnostics().is_empty());

        let mut changed_parent = view(ordinary_layout);
        changed_parent.accepts_flex = true;
        // Fiber owns Arc<dyn AnyView>; these test callbacks are intentionally local.
        #[allow(clippy::arc_with_non_send_sync)]
        let changed_parent = Arc::new(changed_parent);
        arena.get_mut(ordinary_parent).unwrap().view = Some(changed_parent);
        run(&mut arena, root);
        assert!(
            arena
                .get(ordinary_parent)
                .unwrap()
                .layout_diagnostics()
                .is_empty()
        );
    }

    #[test]
    fn root_flex_metadata_and_conflicting_immediate_wrappers_are_diagnosed() {
        let mut arena = FiberArena::new();
        let mut inner = view(ordinary_layout);
        inner.data = flex_data();
        let child = insert(&mut arena, inner, vec![]);
        let mut outer = view(ordinary_layout);
        outer.data = flex_data();
        let root = insert(&mut arena, outer, vec![child]);
        run(&mut arena, root);
        assert_eq!(arena.get(root).unwrap().layout_error(), None);
        assert_eq!(
            arena.get(root).unwrap().layout_diagnostics(),
            &[
                LayoutDiagnostic::InvalidFlexParent,
                LayoutDiagnostic::ConflictingFlexParentData,
            ]
        );
    }
}
