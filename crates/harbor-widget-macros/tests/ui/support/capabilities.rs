use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::{column::Column, padding::Padding};
use harbor_widget::{ChildConstructionError, Children, MultiChild, SingleChild, WithChildren};

pub struct ExternalSingle(pub Padding);
impl WithChildren for ExternalSingle {
    type Capability = SingleChild;
    fn with_children(self, children: Children) -> Result<Self, ChildConstructionError> {
        Ok(Self(self.0.with_children(children)?))
    }
}
impl Component for ExternalSingle {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.0.build(cx)
    }
}
pub struct ExternalMulti(pub Column);
impl WithChildren for ExternalMulti {
    type Capability = MultiChild;
    fn with_children(self, children: Children) -> Result<Self, ChildConstructionError> {
        Ok(Self(self.0.with_children(children)?))
    }
}
impl Component for ExternalMulti {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.0.build(cx)
    }
}
