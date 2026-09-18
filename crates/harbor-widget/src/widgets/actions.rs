use crate::layout::{BoxConstraints, Point, Size};
use crate::view::{ActionProviderView, AnyView, BuildCx, Component, View};
use std::any::Any;
use std::sync::Arc;

/// Result of synchronously invoking a typed application action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionOutcome {
    /// The action was accepted and the triggering keyboard event must stop.
    Consumed,
    /// The action declined the current context and routing must continue.
    PassThrough,
}

/// Typed action handler selected by the nearest compatible ancestor provider.
#[derive(Clone)]
pub struct Actions<A: Clone + 'static> {
    handler: Arc<dyn Fn(A) -> ActionOutcome>,
    child: Option<View>,
}

impl<A: Clone + 'static> Actions<A> {
    /// Creates an action provider without a child for declarative `view!` composition.
    pub fn handler(handler: impl Fn(A) -> ActionOutcome + 'static) -> Self {
        Self {
            handler: Arc::new(handler),
            child: None,
        }
    }

    pub fn new(
        child: impl crate::IntoChildView,
        handler: impl Fn(A) -> ActionOutcome + 'static,
    ) -> Self {
        Self {
            handler: Arc::new(handler),
            child: Some(child.into_child_view()),
        }
    }
}

impl<A: Clone + 'static> Component for Actions<A> {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl<A: Clone + 'static> crate::WithChildren for Actions<A> {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("Actions", &mut self.child)?;
        Ok(self)
    }
}

impl<A: Clone + 'static> AnyView for Actions<A> {
    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &crate::text::TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(child_sizes.first().copied().unwrap_or(Size::ZERO));
        (size, vec![Point::ZERO; child_sizes.len()])
    }
    fn as_action_provider(&self) -> Option<&dyn ActionProviderView> {
        Some(self)
    }
}

impl<A: Clone + 'static> ActionProviderView for Actions<A> {
    fn invoke_action(&self, action: &dyn Any) -> Option<ActionOutcome> {
        let action = action.downcast_ref::<A>()?;
        Some((self.handler)(action.clone()))
    }
}
