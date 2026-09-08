use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key, View};
use std::sync::Arc;

/// Callback invoked after a distinct, non-empty allocation is committed.
pub type LayoutChangedCallback = Arc<dyn Fn(Rect) + 'static>;

/// Transparent single-child wrapper that observes its committed logical allocation.
///
/// The first valid non-empty allocation is delivered once. Equal allocations are
/// coalesced for the lifetime of the surviving Fiber; zero/non-finite allocations
/// and failed layout attempts are ignored. Callbacks run after paint and dirty-flag
/// settlement, never from measurement or geometry commit.
#[derive(Clone)]
pub struct LayoutObserver {
    callback: LayoutChangedCallback,
    child: Option<View>,
}

impl LayoutObserver {
    /// Creates an observer from a callback.
    pub fn new(callback: impl Fn(Rect) + 'static) -> Self {
        Self {
            callback: Arc::new(callback),
            child: None,
        }
    }

    /// Creates an observer from a shared callback handle.
    pub fn from_callback(callback: LayoutChangedCallback) -> Self {
        Self {
            callback,
            child: None,
        }
    }

    /// Sets the transparently laid-out single child.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for LayoutObserver {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for LayoutObserver {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        let mut incoming = children.into_views();
        let received = usize::from(self.child.is_some()) + incoming.len();
        if received > 1 {
            return Err(crate::ChildConstructionError::too_many(
                "LayoutObserver",
                crate::ChildCardinality::Single,
                received,
            ));
        }
        if let Some(child) = incoming.pop() {
            self.child = Some(child);
        }
        Ok(self)
    }
}

impl AnyView for LayoutObserver {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _metrics: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        constraints.validate()?;
        if children.len() > 1 {
            return Err(LayoutError::InvalidChildIndex);
        }
        let size = if children.len() == 1 {
            children.measure(0, constraints)?
        } else {
            constraints.constrain(Size::ZERO)
        };
        Ok(ParentLayout {
            size,
            placements: if children.len() == 1 {
                vec![(0, Point::ZERO)]
            } else {
                Vec::new()
            },
            diagnostics: Vec::new(),
        })
    }

    fn layout_changed_callback(&self) -> Option<LayoutChangedCallback> {
        Some(Arc::clone(&self.callback))
    }
}
