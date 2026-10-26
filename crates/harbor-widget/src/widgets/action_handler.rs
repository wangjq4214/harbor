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
pub struct ActionHandler<A: Clone + 'static> {
    handler: Arc<dyn Fn(A) -> ActionOutcome>,
    child: Option<View>,
}

impl<A: Clone + 'static> ActionHandler<A> {
    /// Creates an action provider with the required handler and no child.
    pub fn new(handler: impl Fn(A) -> ActionOutcome + 'static) -> Self {
        Self {
            handler: Arc::new(handler),
            child: None,
        }
    }

    /// Replaces the single child slot.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl<A: Clone + 'static> Component for ActionHandler<A> {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl<A: Clone + 'static> crate::WithChildren for ActionHandler<A> {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("ActionHandler", &mut self.child)?;
        Ok(self)
    }
}

impl<A: Clone + 'static> AnyView for ActionHandler<A> {
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

impl<A: Clone + 'static> ActionProviderView for ActionHandler<A> {
    fn invoke_action(&self, action: &dyn Any) -> Option<ActionOutcome> {
        let action = action.downcast_ref::<A>()?;
        Some((self.handler)(action.clone()))
    }
}
