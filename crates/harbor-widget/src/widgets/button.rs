use crate::input::event_ctx::EventCtx;
use crate::view::{BuildCx, Component, View};
use crate::widgets::focus::FocusHandle;
use crate::widgets::interactive_region::InteractiveRegion;
use std::rc::Rc;

type ClickCallback = Rc<dyn Fn(&mut EventCtx)>;
/// Compatibility button built on the reusable desktop interaction state machine.
#[derive(Clone)]
pub struct Button {
    label: String,
    on_click: Option<ClickCallback>,
    disabled: bool,
    focus_handle: Option<FocusHandle>,
    selected: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_click: None,
            disabled: false,
            focus_handle: None,
            selected: false,
        }
    }

    /// Runs on the UI thread with local captures and existing activation timing.
    pub fn on_click(mut self, handler: impl Fn(&mut EventCtx) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Assigns the existing interactive region a stable focus identity.
    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.focus_handle = Some(handle);
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
        if let Some(handle) = self.focus_handle {
            region = region.focus_handle(handle);
        }
        if let Some(callback) = &self.on_click {
            let callback = callback.clone();
            region = region.on_activate(move |ctx| callback(ctx));
        }
        region.build(cx)
    }
}
