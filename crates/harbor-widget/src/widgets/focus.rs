use crate::layout::{BoxConstraints, Point, Size};
use crate::view::{AnyView, BuildCx, Component, FocusMetadata, View};
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable public identity used to restore focus without exposing a FiberId.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FocusHandle(pub(crate) u64);

impl FocusHandle {
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for FocusHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// A focusable single-child wrapper with explicit tab order metadata.
#[derive(Clone)]
pub struct Focus {
    enabled: bool,
    order: i32,
    handle: Option<FocusHandle>,
    child: Option<View>,
}

impl Focus {
    pub fn new(child: impl crate::IntoChildView) -> Self {
        Self {
            enabled: true,
            order: 0,
            handle: None,
            child: Some(child.into_child_view()),
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    pub fn handle(mut self, handle: FocusHandle) -> Self {
        self.handle = Some(handle);
        self
    }
}

impl Component for Focus {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for Focus {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("Focus", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for Focus {
    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &crate::text::TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(child_sizes.first().copied().unwrap_or(Size::ZERO));
        (size, vec![Point::ZERO; child_sizes.len()])
    }

    fn focus_metadata(&self) -> Option<FocusMetadata> {
        Some(FocusMetadata {
            enabled: self.enabled,
            order: self.order,
            handle: self.handle.map(|handle| handle.0),
        })
    }

    fn delegates_focused_events(&self) -> bool {
        true
    }
}
