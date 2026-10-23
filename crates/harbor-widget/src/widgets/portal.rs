use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Presents arbitrary children in the root overlay without a normal-layout slot.
///
/// Children retain their declaring component's lifetime and logical ancestry, but
/// are laid out against the root bounds and painted above ordinary content. Use
/// ordinary layout widgets inside the Portal to position content. Empty space is
/// not hit-testable; Portal itself neither requests focus nor establishes a modal
/// scope. Nested portals paint in declaration order, after their outer portal.
#[derive(Clone, Default)]
pub struct Portal {
    children: Vec<View>,
}

impl Portal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.children.push(child.into_child_view());
        self
    }
}

impl Component for Portal {
    fn build(&self, _: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl crate::WithChildren for Portal {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        self.children.extend(children.into_views());
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
