use crate::layout::{
    Alignment, BoxConstraints, ChildMeasurer, FlexFit, LayoutDiagnostic, LayoutError, ParentLayout,
    Point, Rect, Size,
};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// The direction in which a flex container allocates its children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub(crate) fn main(self, size: Size) -> f32 {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }

    pub(crate) fn cross(self, size: Size) -> f32 {
        match self {
            Self::Horizontal => size.height,
            Self::Vertical => size.width,
        }
    }

    pub(crate) fn size(self, main: f32, cross: f32) -> Size {
        match self {
            Self::Horizontal => Size::new(main, cross),
            Self::Vertical => Size::new(cross, main),
        }
    }

    fn point(self, main: f32, cross: f32) -> Point {
        let size = self.size(main, cross);
        Point::new(size.width, size.height)
    }
}

/// Distribution of unused main-axis space. Explicit gaps remain a minimum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainAxisAlignment {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// An axis-neutral flex container with parent-directed child measurement.
///
/// Without flexible children this shrink-wraps, subject to parent bounds. With
/// flexible children it fills a finite main-axis maximum, even under loose
/// constraints. Unbounded flex is content-driven and emits `UnboundedFlex`.
/// Unbounded cross-axis stretch first discovers natural sizes provisionally,
/// then finalizes every child with the derived finite cross extent. Descendant
/// stretch corrections wait for this final traversal rather than cascading.
/// Overflow is diagnosed, not automatically clipped. Main-axis diagnostic
/// excess at or below `f32::EPSILON * main_extent` is treated as rounding noise;
/// allocations and placement geometry are never adjusted by this tolerance.
#[derive(Clone)]
pub struct Flex {
    pub axis: Axis,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: Alignment,
    pub background: Option<Color>,
    gap: f32,
    children: Vec<View>,
}

impl Flex {
    pub fn new(axis: Axis) -> Self {
        Self {
            axis,
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: Alignment::Start,
            background: None,
            gap: 0.0,
            children: Vec::new(),
        }
    }

    /// Sets a nonnegative finite gap in logical pixels. Invalid values cause a
    /// layout error rather than entering committed geometry.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }

    pub fn cross_axis_alignment(mut self, alignment: Alignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.children.push(child.into_child_view());
        self
    }

    fn engine(&self) -> FlexLayout {
        FlexLayout {
            axis: self.axis,
            gap: self.gap,
            main_axis_alignment: self.main_axis_alignment,
            cross_axis_alignment: self.cross_axis_alignment,
        }
    }
}

impl Component for Flex {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl crate::WithChildren for Flex {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        self.children.extend(children.into_views());
        Ok(self)
    }
}

impl AnyView for Flex {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        finite_fill(constraints)
    }

    fn accepts_flex_children(&self) -> bool {
        true
    }

    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _metrics: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        self.engine().layout(constraints, children)
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        self.engine().layout_children(constraints, child_sizes)
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let mut box_widget = super::sized_box::SizedBox::new(rect.size());
        box_widget.color = self.background;
        box_widget.paint_primitives(rect, metrics)
    }
}

/// The shared policy used directly by Flex, Row and Column (no extra Fiber).
pub(crate) struct FlexLayout {
    pub axis: Axis,
    pub gap: f32,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: Alignment,
}

impl FlexLayout {
    pub(crate) fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
    ) -> Result<ParentLayout, LayoutError> {
        constraints.validate()?;
        let count = children.len();
        let gaps = self.gaps(count)?;
        let mut data = Vec::with_capacity(count);
        for index in 0..count {
            data.push(children.parent_data(index)?.flex);
        }
        let flex_count = data.iter().filter(|entry| entry.is_some()).count();
        let main_max = self.axis.main(constraints.max);
        let cross_max = self.axis.cross(constraints.max);
        let stretch = self.cross_axis_alignment == Alignment::Stretch;
        let provisional_stretch = stretch && !cross_max.is_finite();
        let cross_min = if stretch && cross_max.is_finite() {
            cross_max
        } else {
            0.0
        };
        let inflexible_max = if main_max.is_finite() {
            finite((f64::from(main_max) - gaps).max(0.0))?
        } else {
            f32::INFINITY
        };
        let natural = BoxConstraints {
            min: self.axis.size(0.0, cross_min),
            max: self.axis.size(inflexible_max, cross_max),
        };
        let mut requests = vec![natural; count];
        let mut sizes = vec![Size::ZERO; count];
        let mut inflexible_extent = 0.0_f64;
        for (index, metadata) in data.iter().enumerate() {
            if metadata.is_none() {
                sizes[index] = if provisional_stretch {
                    children.measure_provisional(index, natural)?
                } else {
                    children.measure(index, natural)?
                };
                validate_size(sizes[index])?;
                inflexible_extent += f64::from(self.axis.main(sizes[index]));
            }
        }

        let mut diagnostics = Vec::new();
        if flex_count > 0 && !main_max.is_finite() {
            diagnostics.push(LayoutDiagnostic::UnboundedFlex);
        }
        // u128 cannot overflow for a Vec indexed by usize containing u32 factors.
        let total_factor: u128 = data
            .iter()
            .flatten()
            .map(|metadata| u128::from(metadata.factor.get()))
            .sum();
        let remaining = (f64::from(main_max) - gaps - inflexible_extent).max(0.0);
        let mut residue = remaining;
        let mut flexible_left = flex_count;
        for (index, metadata) in data.iter().enumerate() {
            let Some(metadata) = metadata else {
                continue;
            };
            flexible_left -= 1;
            let request = if main_max.is_finite() {
                let share = if flexible_left == 0 {
                    finite(residue)?
                } else {
                    finite(
                        (remaining * f64::from(metadata.factor.get()) / total_factor as f64)
                            .min(residue),
                    )?
                };
                // Subtract the actual f32 share, not the unrounded ideal share.
                residue = (residue - f64::from(share)).max(0.0);
                let minimum = if metadata.fit == FlexFit::Tight {
                    share
                } else {
                    0.0
                };
                BoxConstraints {
                    min: self.axis.size(minimum, cross_min),
                    max: self.axis.size(share, cross_max),
                }
            } else {
                natural
            };
            requests[index] = request;
            sizes[index] = if provisional_stretch {
                children.measure_provisional(index, request)?
            } else {
                children.measure(index, request)?
            };
            validate_size(sizes[index])?;
        }

        if provisional_stretch && !children.is_provisional() && count > 0 {
            let cross = self.cross_extent(constraints, &sizes);
            for index in 0..count {
                // Even a child already reporting this cross extent may contain
                // pending provisional descendants. Finalize every child.
                let request = BoxConstraints {
                    min: self.axis.size(self.axis.main(requests[index].min), cross),
                    max: self.axis.size(self.axis.main(requests[index].max), cross),
                };
                sizes[index] = children.measure(index, request)?;
                validate_size(sizes[index])?;
            }
        }
        // Correction deliberately does not redistribute shares or converge.
        self.finish(constraints, &sizes, flex_count > 0, diagnostics)
    }

    /// Compatibility for direct legacy tests; real measurement uses `layout`.
    pub(crate) fn layout_children(
        &self,
        constraints: BoxConstraints,
        sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        match self.finish(constraints, sizes, false, Vec::new()) {
            Ok(layout) => (
                layout.size,
                layout
                    .placements
                    .into_iter()
                    .map(|(_, point)| point)
                    .collect(),
            ),
            Err(_) => (Size::ZERO, vec![Point::ZERO; sizes.len()]),
        }
    }

    fn gaps(&self, count: usize) -> Result<f64, LayoutError> {
        if !self.gap.is_finite() || self.gap < 0.0 {
            return Err(LayoutError::InvalidConstraints);
        }
        let gaps = f64::from(self.gap) * count.saturating_sub(1) as f64;
        finite(gaps)?;
        Ok(gaps)
    }

    fn cross_extent(&self, constraints: BoxConstraints, sizes: &[Size]) -> f32 {
        sizes
            .iter()
            .map(|size| self.axis.cross(*size))
            .fold(0.0, f32::max)
            .clamp(
                self.axis.cross(constraints.min),
                self.axis.cross(constraints.max),
            )
    }

    fn finish(
        &self,
        constraints: BoxConstraints,
        sizes: &[Size],
        has_flex: bool,
        mut diagnostics: Vec<LayoutDiagnostic>,
    ) -> Result<ParentLayout, LayoutError> {
        constraints.validate()?;
        let gaps = self.gaps(sizes.len())?;
        let mut occupied = gaps;
        let mut natural_cross = 0.0_f32;
        for size in sizes {
            validate_size(*size)?;
            occupied += f64::from(self.axis.main(*size));
            natural_cross = natural_cross.max(self.axis.cross(*size));
        }
        finite(occupied)?;
        let maximum = self.axis.main(constraints.max);
        let main = if has_flex && maximum.is_finite() {
            maximum
        } else {
            finite(occupied)?.clamp(self.axis.main(constraints.min), maximum)
        };
        let cross = self.cross_extent(constraints, sizes);
        let size = self.axis.size(main, cross);
        // Widened sums of individually rounded f32 shares can exceed M even
        // when their committed f32 right edge is exactly M. Ignore at most one
        // scale-relative f32 rounding unit for diagnostics, never for geometry.
        let excess = (occupied - f64::from(main)).max(0.0);
        let tolerance = f64::from(f32::EPSILON) * f64::from(main);
        let overflow_main = if excess <= tolerance {
            0.0
        } else {
            finite(excess)?
        };
        let overflow_cross = (natural_cross - cross).max(0.0);
        if overflow_main > 0.0 || overflow_cross > 0.0 {
            diagnostics.push(LayoutDiagnostic::Overflow {
                main: overflow_main,
                cross: overflow_cross,
            });
        }
        let free = (f64::from(main) - occupied).max(0.0);
        let count = sizes.len() as f64;
        let (leading, between) = match self.main_axis_alignment {
            MainAxisAlignment::Center => (free * 0.5, 0.0),
            MainAxisAlignment::End => (free, 0.0),
            MainAxisAlignment::SpaceBetween if count > 1.0 => (0.0, free / (count - 1.0)),
            MainAxisAlignment::SpaceAround if count > 0.0 => (free / count * 0.5, free / count),
            MainAxisAlignment::SpaceEvenly if count > 0.0 => {
                let spacing = free / (count + 1.0);
                (spacing, spacing)
            }
            _ => (0.0, 0.0),
        };
        let mut cursor = leading;
        let mut placements = Vec::with_capacity(sizes.len());
        for (index, child) in sizes.iter().enumerate() {
            let cross_offset = self
                .cross_axis_alignment
                .position(self.axis.cross(*child), cross);
            let main_offset = finite(cursor)?;
            finite(f64::from(main_offset) + f64::from(self.axis.main(*child)))?;
            finite(f64::from(cross_offset) + f64::from(self.axis.cross(*child)))?;
            placements.push((index, self.axis.point(main_offset, cross_offset)));
            cursor += f64::from(self.axis.main(*child)) + f64::from(self.gap) + between;
        }
        Ok(ParentLayout {
            size,
            placements,
            diagnostics,
        })
    }
}

fn finite(value: f64) -> Result<f32, LayoutError> {
    if value.is_finite() && value >= 0.0 && value <= f64::from(f32::MAX) {
        Ok(value as f32)
    } else {
        Err(LayoutError::InvalidGeometry)
    }
}

fn validate_size(size: Size) -> Result<(), LayoutError> {
    finite(f64::from(size.width))?;
    finite(f64::from(size.height))?;
    Ok(())
}

pub(crate) fn finite_fill(constraints: BoxConstraints) -> Size {
    constraints.fill_bounded(Size::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{FlexParentData, ParentData};
    use std::num::NonZeroU32;

    type ChildSpec = (f32, f32, Option<(u32, FlexFit)>);

    #[derive(Clone)]
    struct CorrectionLeaf {
        axis: Axis,
        measurements: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl Component for CorrectionLeaf {
        fn build(&self, _cx: &mut BuildCx) -> View {
            View::new(self.clone(), Vec::new(), None)
        }
    }

    impl AnyView for CorrectionLeaf {
        fn key(&self) -> Option<&Key> {
            None
        }
        fn widget_type(&self) -> std::any::TypeId {
            std::any::TypeId::of::<Self>()
        }
        fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
            self.measurements.set(self.measurements.get() + 1);
            let main = if self.axis.cross(constraints.min) >= 20.0 {
                80.0
            } else {
                50.0
            };
            constraints.constrain(self.axis.size(main, 10.0))
        }
    }

    fn layout_tree(
        component: impl Component + 'static,
        constraints: BoxConstraints,
    ) -> (crate::fiber::FiberArena, crate::fiber::FiberId) {
        let mut arena = crate::fiber::FiberArena::new();
        // Use the production deferred reconciler, then detach its temporary
        // staging parent so layout observes the actual unbounded root.
        let staging = arena.insert(crate::fiber::Fiber::new(
            None,
            std::any::TypeId::of::<()>(),
            None,
        ));
        let root = crate::fiber::reconcile_children_with_externals(
            &mut arena,
            staging,
            &[],
            vec![View::deferred(component)],
            &mut crate::view::ExternalRegistrations::default(),
        )[0];
        arena.remove(staging);
        arena.get_mut(root).unwrap().parent = None;
        crate::fiber::layout_fiber(
            &mut arena,
            root,
            constraints,
            Point::ZERO,
            &crate::runtime::DEFAULT_TEXT_METRICS,
        );
        (arena, root)
    }

    fn assert_committed_tree(arena: &crate::fiber::FiberArena, root: crate::fiber::FiberId) {
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            let fiber = arena.get(id).unwrap();
            assert_eq!(fiber.layout_error(), None, "fiber {id:?}");
            let rect = fiber.layout_rect().unwrap();
            assert!(rect.min.x.is_finite() && rect.min.y.is_finite());
            assert!(rect.max.x.is_finite() && rect.max.y.is_finite());
            assert!(rect.size().width >= 0.0 && rect.size().height >= 0.0);
            pending.extend_from_slice(fiber.children());
        }
    }

    #[test]
    fn final_traversal_retains_correction_sensitive_descendants_through_alternating_axes() {
        use crate::widgets::sized_box::SizedBox;
        use crate::{Expanded, Flexible};
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let other_axis = match axis {
                Axis::Horizontal => Axis::Vertical,
                Axis::Vertical => Axis::Horizontal,
            };
            for maximum in [100.0, f32::INFINITY] {
                for flexible in [false, true] {
                    let measurements = std::rc::Rc::new(std::cell::Cell::new(0));
                    let opposite =
                        Flex::new(other_axis).child(Expanded::new().child(CorrectionLeaf {
                            axis,
                            measurements: measurements.clone(),
                        }));
                    let root = Flex::new(axis).cross_axis_alignment(Alignment::Stretch);
                    let root = if flexible {
                        root.child(Flexible::new().child(opposite))
                    } else {
                        root.child(opposite)
                    };
                    let root = root.child(SizedBox::new(axis.size(20.0, 20.0)));
                    let (arena, root) = layout_tree(
                        root,
                        BoxConstraints::loose(axis.size(maximum, f32::INFINITY)),
                    );
                    assert_committed_tree(&arena, root);
                    let root_fiber = arena.get(root).unwrap();
                    assert_eq!(
                        root_fiber.layout_rect().unwrap().size(),
                        axis.size(100.0, 20.0)
                    );
                    let opposite_id = root_fiber.children()[0];
                    assert_eq!(
                        arena
                            .get(opposite_id)
                            .unwrap()
                            .layout_rect()
                            .unwrap()
                            .size(),
                        axis.size(80.0, 20.0)
                    );
                    let sibling = arena.get(root_fiber.children()[1]).unwrap();
                    assert_eq!(sibling.layout_rect().unwrap().min, axis.point(80.0, 0.0));
                    assert_eq!(
                        measurements.get(),
                        2,
                        "natural discovery plus final geometry"
                    );
                    let mut pending = vec![opposite_id];
                    while let Some(id) = pending.pop() {
                        let fiber = arena.get(id).unwrap();
                        assert!(
                            !fiber
                                .layout_diagnostics()
                                .contains(&LayoutDiagnostic::UnboundedFlex),
                            "provisional diagnostic leaked"
                        );
                        if fiber.widget_type() == std::any::TypeId::of::<CorrectionLeaf>() {
                            assert_eq!(fiber.layout_rect().unwrap().size(), axis.size(80.0, 20.0));
                        }
                        pending.extend_from_slice(fiber.children());
                    }
                }
            }
        }
    }

    #[test]
    fn alternating_stretch_axes_finalize_deep_trees_under_unbounded_root_constraints() {
        use crate::Expanded;
        use crate::widgets::sized_box::SizedBox;
        for root_axis in [Axis::Horizontal, Axis::Vertical] {
            for maximum in [1000.0, f32::INFINITY] {
                for depth in [8, 32] {
                    let mut tree = Flex::new(root_axis).child(SizedBox::new(Size::new(1.0, 1.0)));
                    for level in 0..depth {
                        let axis = if level % 2 == 0 {
                            root_axis
                        } else {
                            match root_axis {
                                Axis::Horizontal => Axis::Vertical,
                                Axis::Vertical => Axis::Horizontal,
                            }
                        };
                        tree = Flex::new(axis)
                            .cross_axis_alignment(Alignment::Stretch)
                            .child(Expanded::new().child(tree))
                            .child(SizedBox::new(axis.size(1.0, level as f32 + 2.0)));
                    }
                    let axis = tree.axis;
                    let (arena, root) = layout_tree(
                        tree,
                        BoxConstraints::loose(axis.size(maximum, f32::INFINITY)),
                    );
                    assert_committed_tree(&arena, root);
                    let size = arena.get(root).unwrap().layout_rect().unwrap().size();
                    assert!(axis.main(size) > 0.0 && axis.cross(size) > 0.0);
                }
            }
        }
    }

    struct Children {
        axis: Axis,
        sizes: Vec<Size>,
        data: Vec<ParentData>,
        requests: Vec<Vec<BoxConstraints>>,
        order: Vec<usize>,
        grow_on_stretch: bool,
        provisional: bool,
        provisional_calls: Vec<usize>,
    }

    impl Children {
        fn new(axis: Axis, children: &[ChildSpec]) -> Self {
            Self {
                axis,
                sizes: children
                    .iter()
                    .map(|&(main, cross, _)| axis.size(main, cross))
                    .collect(),
                data: children
                    .iter()
                    .map(|&(_, _, flex)| ParentData {
                        flex: flex.map(|(factor, fit)| FlexParentData {
                            factor: NonZeroU32::new(factor).unwrap(),
                            fit,
                        }),
                    })
                    .collect(),
                requests: vec![Vec::new(); children.len()],
                order: Vec::new(),
                grow_on_stretch: false,
                provisional: false,
                provisional_calls: Vec::new(),
            }
        }
    }

    impl ChildMeasurer for Children {
        fn is_provisional(&self) -> bool {
            self.provisional
        }

        fn measure_provisional(
            &mut self,
            index: usize,
            constraints: BoxConstraints,
        ) -> Result<Size, LayoutError> {
            self.provisional_calls.push(index);
            self.measure(index, constraints)
        }

        fn len(&self) -> usize {
            self.sizes.len()
        }
        fn parent_data(&self, index: usize) -> Result<ParentData, LayoutError> {
            Ok(self.data[index])
        }
        fn measure(
            &mut self,
            index: usize,
            constraints: BoxConstraints,
        ) -> Result<Size, LayoutError> {
            constraints.validate()?;
            self.order.push(index);
            self.requests[index].push(constraints);
            assert!(self.requests[index].len() <= 2, "more than one correction");
            let mut size = self.sizes[index];
            if self.grow_on_stretch && self.axis.cross(constraints.min) > self.axis.cross(size) {
                size = self
                    .axis
                    .size(self.axis.main(size) + 30.0, self.axis.cross(size));
            }
            Ok(constraints.constrain(size))
        }
    }

    fn engine(axis: Axis) -> FlexLayout {
        FlexLayout {
            axis,
            gap: 0.0,
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: Alignment::Start,
        }
    }

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.0001,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn measures_inflexible_first_and_uses_final_residue_in_both_axes() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut children = Children::new(
                axis,
                &[
                    (0.0, 10.0, Some((1, FlexFit::Tight))),
                    (10.0, 10.0, None),
                    (0.0, 10.0, Some((2, FlexFit::Tight))),
                    (0.0, 10.0, Some((3, FlexFit::Tight))),
                ],
            );
            let mut engine = engine(axis);
            engine.gap = 1.0;
            let result = engine
                .layout(
                    BoxConstraints::loose(axis.size(100.25, 20.0)),
                    &mut children,
                )
                .unwrap();
            assert_eq!(children.order, [1, 0, 2, 3]);
            assert_eq!(axis.main(result.size), 100.25);
            let shares: Vec<_> = [0, 2, 3]
                .map(|i| axis.main(children.requests[i][0].max))
                .to_vec();
            close(shares[0], 87.25 / 6.0);
            close(shares[1], 87.25 / 3.0);
            close(shares.iter().sum(), 87.25);
            for index in [0, 2, 3] {
                assert_eq!(
                    axis.main(children.requests[index][0].min),
                    axis.main(children.requests[index][0].max)
                );
            }
        }
    }

    #[test]
    fn maximum_integer_factors_do_not_overflow_and_loose_fit_is_not_redistributed() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut children = Children::new(
                axis,
                &[
                    (10.0, 5.0, Some((u32::MAX, FlexFit::Loose))),
                    (0.0, 5.0, Some((u32::MAX, FlexFit::Tight))),
                ],
            );
            let mut engine = engine(axis);
            engine.main_axis_alignment = MainAxisAlignment::End;
            let result = engine
                .layout(BoxConstraints::loose(axis.size(100.0, 20.0)), &mut children)
                .unwrap();
            assert_eq!(children.order, [0, 1]);
            assert_eq!(axis.main(children.requests[0][0].min), 0.0);
            assert_eq!(axis.main(children.requests[1][0].min), 50.0);
            assert_eq!(
                result.placements,
                [(0, axis.point(40.0, 0.0)), (1, axis.point(50.0, 0.0))]
            );
        }
    }

    #[test]
    fn unbounded_flex_is_finite_content_driven_and_spacer_is_zero() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut children = Children::new(
                axis,
                &[
                    (12.0, 5.0, Some((1, FlexFit::Tight))),
                    (17.0, 7.0, Some((1, FlexFit::Loose))),
                    (0.0, 0.0, Some((1, FlexFit::Tight))),
                ],
            );
            let result = engine(axis)
                .layout(
                    BoxConstraints::loose(axis.size(f32::INFINITY, f32::INFINITY)),
                    &mut children,
                )
                .unwrap();
            assert_eq!(result.size, axis.size(29.0, 7.0));
            assert_eq!(result.diagnostics, [LayoutDiagnostic::UnboundedFlex]);
            for requests in children.requests {
                assert_eq!(axis.main(requests[0].min), 0.0);
                assert_eq!(axis.main(requests[0].max), f32::INFINITY);
            }
        }
    }

    #[test]
    fn unbounded_stretch_corrects_once_and_uses_final_main_extent_and_overflow() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut children = Children::new(axis, &[(50.0, 10.0, None), (70.0, 20.0, None)]);
            children.grow_on_stretch = true;
            let mut engine = engine(axis);
            engine.cross_axis_alignment = Alignment::Stretch;
            let result = engine
                .layout(
                    BoxConstraints::loose(axis.size(100.0, f32::INFINITY)),
                    &mut children,
                )
                .unwrap();
            assert_eq!(children.order, [0, 1, 0, 1]);
            assert_eq!(children.provisional_calls, [0, 1]);
            assert_eq!(children.requests[1][1].min, axis.size(0.0, 20.0));
            assert_eq!(children.requests[0][1].min, axis.size(0.0, 20.0));
            assert_eq!(children.requests[0][1].max, axis.size(100.0, 20.0));
            assert_eq!(result.size, axis.size(100.0, 20.0));
            assert_eq!(result.placements[1].1, axis.point(80.0, 0.0));
            assert_eq!(
                result.diagnostics,
                [LayoutDiagnostic::Overflow {
                    main: 50.0,
                    cross: 0.0
                }]
            );
        }
    }

    #[test]
    fn provisional_unbounded_stretch_measures_every_fit_naturally_without_correction() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut children = Children::new(
                axis,
                &[
                    (10.0, 5.0, Some((1, FlexFit::Loose))),
                    (20.0, 10.0, None),
                    (0.0, 20.0, Some((1, FlexFit::Tight))),
                ],
            );
            children.provisional = true;
            let mut engine = engine(axis);
            engine.cross_axis_alignment = Alignment::Stretch;
            let result = engine
                .layout(
                    BoxConstraints::loose(axis.size(100.0, f32::INFINITY)),
                    &mut children,
                )
                .unwrap();
            assert_eq!(children.order, [1, 0, 2]);
            assert_eq!(children.provisional_calls, children.order);
            assert_eq!(result.size, axis.size(100.0, 20.0));
            assert!(children.requests.iter().all(|requests| requests.len() == 1));
        }
    }

    #[test]
    fn proportional_share_rounding_does_not_emit_phantom_overflow() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut children = Children::new(
                axis,
                &[
                    (0.0, 5.0, Some((1, FlexFit::Tight))),
                    (0.0, 5.0, Some((2, FlexFit::Tight))),
                ],
            );
            let engine = engine(axis);
            let result = engine
                .layout(BoxConstraints::loose(axis.size(100.0, 20.0)), &mut children)
                .unwrap();
            assert!(result.diagnostics.is_empty());
            let position = result.placements[1].1;
            assert_eq!(
                axis.main(Size::new(position.x, position.y))
                    + axis.main(children.requests[1][0].min),
                100.0
            );
            let overflow = engine
                .finish(
                    BoxConstraints::loose(axis.size(100.0, 20.0)),
                    &[axis.size(50.0, 5.0), axis.size(50.01, 5.0)],
                    false,
                    vec![],
                )
                .unwrap();
            assert!(
                matches!(overflow.diagnostics.as_slice(), [LayoutDiagnostic::Overflow { main, .. }] if *main > 0.009)
            );
        }
    }

    #[test]
    fn finite_stretch_is_supplied_on_the_first_measurement() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut children = Children::new(axis, &[(10.0, 1.0, None), (10.0, 3.0, None)]);
            let mut engine = engine(axis);
            engine.cross_axis_alignment = Alignment::Stretch;
            let result = engine
                .layout(BoxConstraints::loose(axis.size(100.0, 20.0)), &mut children)
                .unwrap();
            assert_eq!(result.size, axis.size(20.0, 20.0));
            assert_eq!(children.order, [0, 1]);
            for requests in children.requests {
                assert_eq!(axis.cross(requests[0].min), 20.0);
                assert_eq!(axis.cross(requests[0].max), 20.0);
            }
        }
    }

    #[test]
    fn extreme_finite_shares_including_subnormal_residue_remain_finite() {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            for maximum in [0.0, f32::from_bits(1), f32::MIN_POSITIVE, 0.125, f32::MAX] {
                let mut children =
                    Children::new(axis, &[(0.0, 0.0, Some((u32::MAX, FlexFit::Tight)))]);
                let result = engine(axis)
                    .layout(
                        BoxConstraints::loose(axis.size(maximum, 1.0)),
                        &mut children,
                    )
                    .unwrap();
                assert_eq!(result.size, axis.size(maximum, 0.0));
                assert_eq!(result.placements, [(0, Point::ZERO)]);
                assert_eq!(axis.main(children.requests[0][0].min), maximum);
            }
        }
    }

    #[test]
    fn negative_nan_and_infinite_gaps_and_arithmetic_overflow_are_errors() {
        for gap in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut engine = engine(Axis::Horizontal);
            engine.gap = gap;
            let mut children = Children::new(Axis::Horizontal, &[]);
            assert!(matches!(
                engine.layout(BoxConstraints::loose(Size::new(10.0, 10.0)), &mut children),
                Err(LayoutError::InvalidConstraints)
            ));
        }
        let mut engine = engine(Axis::Horizontal);
        engine.gap = f32::MAX;
        let mut children = Children::new(Axis::Horizontal, &[(0.0, 0.0, None); 3]);
        assert!(matches!(
            engine.layout(BoxConstraints::loose(Size::new(10.0, 10.0)), &mut children),
            Err(LayoutError::InvalidGeometry)
        ));
        let engine = self::engine(Axis::Horizontal);
        assert!(matches!(
            engine.finish(
                BoxConstraints::loose(Size::new(f32::INFINITY, 10.0)),
                &[Size::new(f32::MAX, 1.0); 2],
                false,
                vec![]
            ),
            Err(LayoutError::InvalidGeometry)
        ));
    }

    #[test]
    fn overflowing_children_align_from_start_and_report_both_axes() {
        let mut engine = engine(Axis::Vertical);
        engine.main_axis_alignment = MainAxisAlignment::End;
        engine.cross_axis_alignment = Alignment::Center;
        let result = engine
            .finish(
                BoxConstraints::tight(Size::new(10.0, 20.0)),
                &[Size::new(15.0, 30.0)],
                false,
                vec![],
            )
            .unwrap();
        assert_eq!(result.placements, [(0, Point::ZERO)]);
        assert_eq!(
            result.diagnostics,
            [LayoutDiagnostic::Overflow {
                main: 10.0,
                cross: 5.0
            }]
        );
    }
}
