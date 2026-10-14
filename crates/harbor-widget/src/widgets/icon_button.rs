use crate::icon::LucideIcon;
use crate::input::event_ctx::EventCtx;
use crate::scene::primitive::Color;
use crate::theme::ControlStyle;
use crate::view::{BuildCx, Component, View};
use crate::widgets::icon::Icon;
use crate::widgets::interactive_region::{InteractionState, InteractiveRegion};
use std::sync::Arc;

type ClickCallback = Arc<dyn Fn(&mut EventCtx) + Send + Sync>;
type ColorCallback = Arc<dyn Fn(&InteractionState) -> Color + Send + Sync>;

/// Compact icon control with the same interaction semantics as [`crate::widgets::Button`].
///
/// The control binds focus, activation, disabled, and selected behavior through an
/// [`InteractiveRegion`] and renders a display-only [`Icon`] as its only child, so the
/// artwork never depends on terminal text metrics or an installed icon font. The
/// accessible label is retained for integrations and test inspection; it is never
/// painted as text.
#[derive(Clone)]
pub struct IconButton {
    icon: LucideIcon,
    label: String,
    on_click: Option<ClickCallback>,
    disabled: bool,
    selected: bool,
    style: Option<ControlStyle>,
    icon_size: Option<f32>,
    color: Option<ColorCallback>,
}

impl IconButton {
    pub fn new(icon: LucideIcon, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
            on_click: None,
            disabled: false,
            selected: false,
            style: None,
            icon_size: None,
            color: None,
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

    /// Overrides the inherited `theme.icon_button` control style.
    pub fn style(mut self, style: ControlStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Overrides the resolved style's `icon_size` in logical pixels.
    ///
    /// Invalid (negative or non-finite) values leave the previous size unchanged.
    pub fn icon_size(mut self, size: f32) -> Self {
        if size.is_finite() && size >= 0.0 {
            self.icon_size = Some(size);
        }
        self
    }

    /// Replaces the state-resolved style foreground as the icon tint.
    ///
    /// The callback receives this control's retained interaction state, so callers can
    /// express appearance rules a [`ControlStyle`] cannot, such as an enclosing row's
    /// visibility or keyboard focus.
    pub fn color_with(
        mut self,
        color: impl Fn(&InteractionState) -> Color + Send + Sync + 'static,
    ) -> Self {
        self.color = Some(Arc::new(color));
        self
    }

    /// Text label retained for accessibility integrations and test inspection.
    ///
    /// The label is never painted; the control renders its icon only.
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl Component for IconButton {
    fn build(&self, cx: &mut BuildCx) -> View {
        let style = self
            .style
            .clone()
            .unwrap_or_else(|| cx.theme().icon_button.clone());
        let mut region = InteractiveRegion::new()
            .style(style.clone())
            .disabled(self.disabled)
            .selected(self.selected);
        if let Some(callback) = &self.on_click {
            let callback = callback.clone();
            region = region.on_activate(move |ctx| callback(ctx));
        }
        let icon = self.icon;
        let size = self.icon_size.unwrap_or(style.icon_size);
        let color = self.color.clone();
        region
            .content(move |cx, state| {
                let tint = color.as_ref().map_or_else(
                    || style.resolve_state(&state).foreground,
                    |color| color(&state),
                );
                Icon::new(icon).size(size).color(tint).build(cx)
            })
            .build(cx)
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
