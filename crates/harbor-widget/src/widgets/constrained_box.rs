use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Size};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Single-child bounds enforced inside the parent's authoritative interval.
///
/// A fixed rail uses `min_width(200.0).max_width(200.0)`; `max_width` alone
/// is only a cap, not a preferred width. Tiny parents can reduce a fixed rail.
/// Bounds must be nonnegative and ordered, minima finite, and maxima finite or
/// positive infinity. Invalid bounds produce `LayoutError::InvalidConstraints`.
#[derive(Clone)]
pub struct ConstrainedBox {
    bounds: BoxConstraints,
    child: Option<View>,
}

impl Default for ConstrainedBox {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstrainedBox {
    pub fn new() -> Self {
        Self {
            bounds: BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)),
            child: None,
        }
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.bounds.min.width = width;
        self
    }

    pub fn max_width(mut self, width: f32) -> Self {
        self.bounds.max.width = width;
        self
    }

    pub fn min_height(mut self, height: f32) -> Self {
        self.bounds.min.height = height;
        self
    }

    pub fn max_height(mut self, height: f32) -> Self {
        self.bounds.max.height = height;
        self
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for ConstrainedBox {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for ConstrainedBox {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("ConstrainedBox", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for ConstrainedBox {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        self.bounds
            .enforce(constraints)
            .map(|bounds| bounds.min)
            .unwrap_or(Size::ZERO)
    }

    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _metrics: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        let bounds = self.bounds.enforce(constraints)?;
        let mut size = bounds.min;
        let mut placements = Vec::with_capacity(children.len());
        for index in 0..children.len() {
            let child = children.measure(index, bounds)?;
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
            placements.push((index, Point::ZERO));
        }
        Ok(ParentLayout {
            size: bounds.constrain(size),
            placements,
            diagnostics: Vec::new(),
        })
    }
}
