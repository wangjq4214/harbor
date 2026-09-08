use std::num::NonZeroU32;

use crate::layout::{BoxConstraints, FlexFit, FlexParentData, ParentData, Point, Size};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Transparent flexible child metadata for an immediate Flex, Row or Column.
///
/// Defaults to factor one and loose fit. Metadata does not tunnel through other
/// containers. Outside a flex parent this is ordinary single-child layout with
/// an `InvalidFlexParent` diagnostic. Child Components remain deferred.
#[derive(Clone)]
pub struct Flexible {
    factor: NonZeroU32,
    fit: FlexFit,
    child: Option<View>,
}

impl Default for Flexible {
    fn default() -> Self {
        Self::new()
    }
}

impl Flexible {
    pub fn new() -> Self {
        Self {
            factor: NonZeroU32::MIN,
            fit: FlexFit::Loose,
            child: None,
        }
    }

    /// A positive allocation weight; zero is unrepresentable.
    pub fn factor(mut self, factor: NonZeroU32) -> Self {
        self.factor = factor;
        self
    }

    pub fn fit(mut self, fit: FlexFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for Flexible {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for Flexible {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("Flexible", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for Flexible {
    fn parent_data(&self) -> ParentData {
        ParentData {
            flex: Some(FlexParentData {
                factor: self.factor,
                fit: self.fit,
            }),
        }
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = sizes.iter().fold(Size::ZERO, |size, child| {
            Size::new(size.width.max(child.width), size.height.max(child.height))
        });
        (constraints.constrain(size), vec![Point::ZERO; sizes.len()])
    }
}

/// A flexible child that must consume its entire main-axis share.
#[derive(Clone)]
pub struct Expanded {
    inner: Flexible,
}

impl Default for Expanded {
    fn default() -> Self {
        Self::new()
    }
}

impl Expanded {
    pub fn new() -> Self {
        Self {
            inner: Flexible::new().fit(FlexFit::Tight),
        }
    }

    pub fn factor(mut self, factor: NonZeroU32) -> Self {
        self.inner = self.inner.factor(factor);
        self
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.inner = self.inner.child(child);
        self
    }
}

impl Component for Expanded {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(
            self.clone(),
            self.inner.child.iter().cloned().collect(),
            None,
        )
    }
}

impl crate::WithChildren for Expanded {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        self.inner = crate::WithChildren::with_children(self.inner, children)?;
        Ok(self)
    }
}

impl AnyView for Expanded {
    fn parent_data(&self) -> ParentData {
        self.inner.parent_data()
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        sizes: &[Size],
        metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        self.inner.layout_children(constraints, sizes, metrics)
    }
}

/// Empty tight flexible space. Under an unbounded main axis it contributes zero.
#[derive(Clone)]
pub struct Spacer {
    inner: Expanded,
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new()
    }
}

impl Spacer {
    pub fn new() -> Self {
        Self {
            inner: Expanded::new().child(super::sized_box::SizedBox::new(Size::ZERO)),
        }
    }

    pub fn factor(mut self, factor: NonZeroU32) -> Self {
        self.inner = self.inner.factor(factor);
        self
    }
}

impl Component for Spacer {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.inner.build(cx)
    }
}

impl crate::WithChildren for Spacer {
    fn with_children(
        self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.ensure_leaf("Spacer")?;
        Ok(self)
    }
}
