use crate::icon::LucideIcon;
use crate::input::event_ctx::EventCtx;
use crate::scene::primitive::Color;
use crate::theme::ControlStyle;
use crate::view::{BuildCx, Component, View};
use crate::widgets::focus::FocusHandle;
use crate::widgets::icon::Icon;
use crate::widgets::interactive_region::{InteractionState, InteractiveRegion};
use std::rc::Rc;

type ClickCallback = Rc<dyn Fn(&mut EventCtx)>;
type ColorCallback = Rc<dyn Fn(&InteractionState) -> Color>;

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
    focus_handle: Option<FocusHandle>,
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
            focus_handle: None,
            selected: false,
            style: None,
            icon_size: None,
            color: None,
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

    /// Overrides the inherited `theme.icon_button` control style.
    pub fn style(mut self, style: ControlStyle) -> Result<Self, crate::theme::AppearanceError> {
        style.validate()?;
        self.style = Some(style);
        Ok(self)
    }

    /// Overrides the resolved style's `icon_size` in logical pixels.
    /// Zero is legal; negative or non-finite sizes are rejected.
    pub fn icon_size(mut self, size: f32) -> Result<Self, crate::LayoutConfigError> {
        crate::layout::configuration::validate_scalar("IconButton.icon_size", size)?;
        self.icon_size = Some(size);
        Ok(self)
    }

    /// Replaces the state-resolved style foreground as the icon tint.
    ///
    /// The callback receives this control's retained interaction state, so callers can
    /// express appearance rules a [`ControlStyle`] cannot, such as an enclosing row's
    /// visibility or keyboard focus. Each consumed result must have finite RGBA
    /// components; an invalid result panics with the field name, without fallback.
    pub fn foreground_with(mut self, color: impl Fn(&InteractionState) -> Color + 'static) -> Self {
        self.color = Some(Rc::new(color));
        self
    }

    /// Text label retained for accessibility integrations and test inspection.
    ///
    /// The label is never painted; the control renders its icon only.
    pub fn get_label(&self) -> &str {
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
            .expect("validated icon-button style")
            .disabled(self.disabled)
            .selected(self.selected);
        if let Some(handle) = self.focus_handle {
            region = region.focus_handle(handle);
        }
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
                crate::theme::validate_color("IconButton.foreground_with", tint)
                    .unwrap_or_else(|error| panic!("{error}"));
                Icon::new(icon)
                    .size(size)
                    .expect("validated icon size")
                    .foreground(tint)
                    .expect("validated icon foreground")
                    .build(cx)
            })
            .build(cx)
    }
}
