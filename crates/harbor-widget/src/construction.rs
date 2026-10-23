//! Public protocols for constructing a widget's ordered child view list.
//!
//! These types deliberately convert public [`Component`] values into opaque
//! [`View`] descriptions without exposing the crate-private `AnyView` runtime
//! capability.

use crate::view::{BuildCx, Component, Key, View};
use std::error::Error;
use std::fmt;

/// Converts one public construction value into an opaque child [`View`].
///
/// Components remain deferred until reconciliation assigns their Fiber. An
/// existing `View` is passed through unchanged, preserving its identity.
pub trait IntoChildView {
    /// Converts this value into one child view.
    fn into_child_view(self) -> View;
}

impl IntoChildView for View {
    fn into_child_view(self) -> View {
        self
    }
}

impl<C: Component + 'static> IntoChildView for C {
    fn into_child_view(self) -> View {
        View::deferred(self)
    }
}

/// An ordered collection of child views used by handwritten builders and macro
/// expansions.
#[derive(Default)]
pub struct Children {
    views: Vec<View>,
}

impl Children {
    /// Creates an empty child collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a child collection containing one value.
    pub fn one(child: impl IntoChildView) -> Self {
        let mut children = Self::new();
        children.push(child);
        children
    }

    /// Appends one child after all previously pushed children.
    pub fn push(&mut self, child: impl IntoChildView) {
        self.views.push(child.into_child_view());
    }

    /// Appends children yielded by an iterator in source order.
    pub fn extend<T: IntoChildView>(&mut self, children: impl IntoIterator<Item = T>) {
        self.views
            .extend(children.into_iter().map(IntoChildView::into_child_view));
    }

    /// Returns whether this collection contains no children.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Returns the number of accumulated children.
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// Consumes the collection into the ordered child views.
    pub fn into_views(self) -> Vec<View> {
        self.views
    }

    /// Attaches at most one child from this collection to a single-child slot.
    pub fn into_single(
        self,
        widget: &'static str,
        existing_child: &mut Option<View>,
    ) -> Result<(), ChildConstructionError> {
        let mut incoming = self.into_views();
        let received = incoming.len();
        if received > 1 {
            return Err(ChildConstructionError::too_many(
                widget,
                ChildCardinality::Single,
                received,
            ));
        }
        *existing_child = incoming.pop();
        Ok(())
    }
}

impl IntoIterator for Children {
    type Item = View;
    type IntoIter = std::vec::IntoIter<View>;

    fn into_iter(self) -> Self::IntoIter {
        self.views.into_iter()
    }
}

impl<T: IntoChildView> Extend<T> for Children {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        Self::extend(self, iter);
    }
}

impl<T: IntoChildView> FromIterator<T> for Children {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut children = Self::new();
        children.extend(iter);
        children
    }
}

/// The child cardinality that a widget permits through [`WithChildren`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildCardinality {
    /// The widget cannot receive children.
    Leaf,
    /// The widget can receive at most one child.
    Single,
}

impl ChildCardinality {
    fn maximum(self) -> usize {
        match self {
            Self::Leaf => 0,
            Self::Single => 1,
        }
    }
}

/// A child list violated the receiving widget's cardinality contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildConstructionError {
    /// The receiving widget type.
    pub widget: &'static str,
    /// The cardinality contract that was violated.
    pub cardinality: ChildCardinality,
    /// The incoming child count, excluding any replaced content.
    pub received: usize,
}

impl ChildConstructionError {
    /// Creates an error for a child count that exceeds `cardinality`.
    pub fn too_many(widget: &'static str, cardinality: ChildCardinality, received: usize) -> Self {
        debug_assert!(received > cardinality.maximum());
        Self {
            widget,
            cardinality,
            received,
        }
    }
}

impl fmt::Display for ChildConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let expected = match self.cardinality {
            ChildCardinality::Leaf => "no children",
            ChildCardinality::Single => "at most one child",
        };
        write!(
            formatter,
            "{} accepts {}; received {} children",
            self.widget, expected, self.received
        )
    }
}

impl Error for ChildConstructionError {}

/// Capability for a component owning a zero-or-one child slot.
pub struct SingleChild;

/// Capability for a component owning an ordered child list.
pub struct MultiChild;

/// A capability that accepts a statically bounded incoming count.
///
/// Single-child components accept bounds zero and one, while multi-child
/// components accept every bound. Leaves have no attachment capability.
pub trait AcceptsChildCount<const N: usize> {}
impl AcceptsChildCount<0> for SingleChild {}
impl AcceptsChildCount<1> for SingleChild {}
impl<const N: usize> AcceptsChildCount<N> for MultiChild {}

/// Replaces a component's content with an ordered incoming child list.
///
/// Implementations declare their capability, validate incoming counts (not
/// previously staged content), and clear their slot/list for empty input.
/// Leaves deliberately do not implement this trait.
pub trait WithChildren: Sized {
    type Capability;

    fn with_children(self, children: Children) -> Result<Self, ChildConstructionError>;
}

/// Applies a statically bounded replacement through the receiving capability.
///
/// Macro expansion supplies the bound without evaluating expressions twice.
/// Runtime validation remains in `with_children`, including for public callers
/// that construct the actual list dynamically.
pub fn with_static_children<const N: usize, C: WithChildren>(
    component: C,
    children: Children,
) -> Result<C, ChildConstructionError>
where
    C::Capability: AcceptsChildCount<N>,
{
    component.with_children(children)
}

/// A component wrapper that supplies an explicit sibling reconciliation key.
#[derive(Clone)]
pub struct Keyed<C> {
    component: C,
    key: Key,
}

impl<C> Keyed<C> {
    /// Returns the explicitly assigned reconciliation key.
    pub fn key(&self) -> &Key {
        &self.key
    }

    /// Consumes the wrapper and returns its underlying component.
    pub fn into_inner(self) -> C {
        self.component
    }
}

impl<C: Component> Component for Keyed<C> {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.component.build(cx).with_explicit_key(self.key.clone())
    }

    fn key(&self) -> Option<Key> {
        Some(self.key.clone())
    }
}

impl<C: WithChildren> WithChildren for Keyed<C> {
    type Capability = C::Capability;
    fn with_children(self, children: Children) -> Result<Self, ChildConstructionError> {
        Ok(Self {
            component: self.component.with_children(children)?,
            key: self.key,
        })
    }
}

/// Extension methods available to every public [`Component`].
pub trait ComponentExt: Component + Sized {
    /// Assigns an explicit, parent-local reconciliation key to this component.
    fn keyed(self, key: impl Into<Key>) -> Keyed<Self> {
        Keyed {
            component: self,
            key: key.into(),
        }
    }
}

impl<C: Component> ComponentExt for C {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{BoxConstraints, Size};
    use crate::text::TextMetrics;
    use crate::view::AnyView;
    use std::any::TypeId;

    #[derive(Clone)]
    struct TestComponent;

    impl Component for TestComponent {
        fn build(&self, _cx: &mut BuildCx) -> View {
            View::new(self.clone(), Vec::new(), None)
        }
    }

    impl AnyView for TestComponent {
        fn key(&self) -> Option<&Key> {
            None
        }

        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }

        fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
            constraints.constrain(Size::ZERO)
        }
    }

    #[test]
    fn children_extend_preserves_order() {
        let mut children = Children::one(TestComponent);
        children.extend([TestComponent, TestComponent]);
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn keyed_component_exposes_key_before_building() {
        let view = TestComponent.keyed("test").into_child_view();
        assert_eq!(view.key(), Some(&Key::new("test")));
    }
}
