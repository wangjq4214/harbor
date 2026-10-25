use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Presents an optional child in the root overlay without a normal-layout slot.
///
/// The child retains its declaring component's lifetime and logical ancestry, but
/// is laid out against the root bounds and painted above ordinary content. Use
/// ordinary layout widgets inside the Portal to position content, or Stack for
/// multiple overlaid children. Empty space is not hit-testable; Portal itself
/// neither requests focus nor establishes a modal scope. Nested portals paint in
/// declaration order, after their outer portal.
#[derive(Clone, Default)]
pub struct Portal {
    child: Option<View>,
}

impl Portal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the optional child slot.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for Portal {
    fn build(&self, _: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for Portal {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("Portal", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for Portal {
    fn hit_test(&self, _: Point, _: crate::layout::Rect) -> bool {
        false
    }
    fn is_portal(&self) -> bool {
        true
    }

    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        for index in 0..children.len() {
            children.measure(index, BoxConstraints::loose(constraints.max))?;
        }
        Ok(ParentLayout {
            size: constraints.fill_bounded(constraints.min),
            placements: (0..children.len()).map(|i| (i, Point::ZERO)).collect(),
            diagnostics: Vec::new(),
        })
    }
}
