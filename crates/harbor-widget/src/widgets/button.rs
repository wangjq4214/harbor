use crate::input::event_ctx::EventCtx;
use crate::view::{BuildCx, Component, View};
use crate::widgets::interactive_region::InteractiveRegion;
use std::sync::Arc;

type ClickCallback = Arc<dyn Fn(&mut EventCtx) + Send + Sync>;
/// Compatibility button built on the reusable desktop interaction state machine.
#[derive(Clone)]
pub struct Button {
    label: String,
    on_click: Option<ClickCallback>,
    disabled: bool,
    selected: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_click: None,
            disabled: false,
            selected: false,
        }
    }

    /// Preserves the established callback signature and dispatch timing.
    pub fn on_click(mut self, handler: impl Fn(&mut EventCtx) + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(handler));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Component for Button {
    fn build(&self, cx: &mut BuildCx) -> View {
        let mut region = InteractiveRegion::new()
            .disabled(self.disabled)
            .selected(self.selected)
            .label(self.label.clone());
        if let Some(callback) = &self.on_click {
            let callback = callback.clone();
            region = region.on_activate(move |ctx| callback(ctx));
        }
        region.build(cx)
    }
}

impl crate::WithChildren for Button {
    fn with_children(
        self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.ensure_leaf("Button")?;
        Ok(self)
    }
}
