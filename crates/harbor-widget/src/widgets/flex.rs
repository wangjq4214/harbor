use crate::layout::{
    BoxConstraints, ChildMeasurer, CrossAxisAlignment, FlexFit, LayoutConfigError,
    LayoutDiagnostic, LayoutError, ParentLayout, Point, Rect, Size,
};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

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

/// Local flex settings. Components validate raw gap values on consumption.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlexOptions {
    pub gap: f32,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: CrossAxisAlignment,
}

impl Default for FlexOptions {
    fn default() -> Self {
        Self {
            gap: 0.0,
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: CrossAxisAlignment::Start,
        }
    }
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
    pub cross_axis_alignment: CrossAxisAlignment,
    background: Option<Color>,
    pub(crate) gap: f32,
    pub(crate) children: Vec<View>,
}

impl Flex {
    pub fn new(axis: Axis) -> Self {
        Self {
            axis,
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: CrossAxisAlignment::Start,
            background: None,
            gap: 0.0,
            children: Vec::new(),
        }
    }

    pub fn with_options(axis: Axis, options: FlexOptions) -> Result<Self, LayoutConfigError> {
        Self::new(axis)
            .main_axis_alignment(options.main_axis_alignment)
            .cross_axis_alignment(options.cross_axis_alignment)
            .gap(options.gap)
    }

    pub fn get_options(&self) -> FlexOptions {
        FlexOptions {
            gap: self.gap,
            main_axis_alignment: self.main_axis_alignment,
            cross_axis_alignment: self.cross_axis_alignment,
        }
    }

    /// Sets a finite nonnegative gap, rejecting invalid input before layout.
    pub fn gap(mut self, gap: f32) -> Result<Self, LayoutConfigError> {
        crate::layout::configuration::validate_scalar("gap", gap)?;
        self.gap = gap;
        Ok(self)
    }

    pub fn main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }

    pub fn cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }

    pub fn background(mut self, color: Color) -> Result<Self, crate::theme::ColorConfigError> {
        crate::theme::validate_color("Flex.background", color)?;
        self.background = Some(color);
        Ok(self)
    }

    /// Replaces the ordered child list, clearing it for empty input.
    pub fn children<T: crate::IntoChildView>(
        mut self,
        children: impl IntoIterator<Item = T>,
    ) -> Self {
        self.children = children
            .into_iter()
            .map(crate::IntoChildView::into_child_view)
            .collect();
        self
    }

    /// Appends children in iteration order.
    pub fn extend_children<T: crate::IntoChildView>(
        mut self,
        children: impl IntoIterator<Item = T>,
    ) -> Self {
        self.children.extend(
            children
                .into_iter()
                .map(crate::IntoChildView::into_child_view),
        );
        self
    }

    /// Appends one child after all existing children.
    pub fn push_child(mut self, child: impl crate::IntoChildView) -> Self {
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
    type Capability = crate::MultiChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        self.children = children.into_views();
        Ok(self)
    }
}

impl AnyView for Flex {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.fill_bounded(Size::ZERO)
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

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        self.background
            .map(|color| Primitive::Quad {
                rect,
                color,
                corner_radius: 0.0,
            })
            .into_iter()
            .collect()
    }
}

/// The shared policy used directly by Flex, Row and Column (no extra Fiber).
pub(crate) struct FlexLayout {
    pub axis: Axis,
    pub gap: f32,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: CrossAxisAlignment,
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
        let stretch = self.cross_axis_alignment == CrossAxisAlignment::Stretch;
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

#[cfg(test)]
mod tests;
