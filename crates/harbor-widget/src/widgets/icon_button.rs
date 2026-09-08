use crate::input::event_ctx::EventCtx;
use crate::view::{BuildCx, Component, View};
use crate::widgets::interactive_region::InteractiveRegion;
use std::sync::Arc;

type ClickCallback = Arc<dyn Fn(&mut EventCtx) + Send + Sync>;
/// Compact text-glyph control with the same semantics as [`crate::widgets::Button`].
#[derive(Clone)]
pub struct IconButton {
    glyph: String,
    label: String,
    on_click: Option<ClickCallback>,
    disabled: bool,
    selected: bool,
}

impl IconButton {
    pub fn new(glyph: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            glyph: glyph.into(),
            label: label.into(),
            on_click: None,
            disabled: false,
            selected: false,
        }
    }

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

    /// Text label retained for accessibility integrations and test inspection.
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl Component for IconButton {
    fn build(&self, cx: &mut BuildCx) -> View {
        let glyph = if self.glyph.is_empty() {
            self.label.clone()
        } else {
            self.glyph.clone()
        };
        let mut region = InteractiveRegion::new()
            .compact()
            .disabled(self.disabled)
            .selected(self.selected)
            .label(glyph);
        if let Some(callback) = &self.on_click {
            let callback = callback.clone();
            region = region.on_activate(move |ctx| callback(ctx));
        }
        region.build(cx)
    }
}

impl crate::WithChildren for IconButton {
    fn with_children(
        self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.ensure_leaf("IconButton")?;
        Ok(self)
    }
}
