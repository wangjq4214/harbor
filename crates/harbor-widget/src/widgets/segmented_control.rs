use super::{Button, Expanded, Row};
use crate::ComponentExt;
use crate::theme::{AppearanceError, ControlStyle};
use crate::view::{BuildCx, Component, View};
use std::rc::Rc;

/// A caller-controlled, single-choice group of adjacent control surfaces.
///
/// Choices are (value, visible label) pairs. Values identify choices and should
/// be unique. An unknown selected value leaves every segment unselected; empty
/// choices produce no tab stops. No selection is inferred or stored locally.
///
/// Each segment is a normal Tab/Shift+Tab stop; Space/Enter or pointer release
/// emits its value. Focus alone does not select. Rebuild with the caller's
/// selected value to update the visuals. Uses `theme.button` unless styled.
#[derive(Clone)]
pub struct SegmentedControl {
    choices: Vec<(String, String)>,
    selected: String,
    on_change: Option<Rc<dyn Fn(String)>>,
    disabled: bool,
    style: Option<ControlStyle>,
}

impl SegmentedControl {
    pub fn new(choices: impl IntoIterator<Item = (String, String)>, selected: String) -> Self {
        // Duplicate values describe the same choice: retain its first label so
        // even malformed caller input cannot paint multiple selected segments.
        let mut unique = Vec::new();
        for (value, label) in choices {
            if !unique.iter().any(|(existing, _)| existing == &value) {
                unique.push((value, label));
            }
        }
        Self {
            choices: unique,
            selected,
            on_change: None,
            disabled: false,
            style: None,
        }
    }

    /// Emits selection intent, without changing the caller's selected value.
    pub fn on_change(mut self, callback: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Rc::new(callback));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn style(mut self, style: ControlStyle) -> Result<Self, AppearanceError> {
        style.validate()?;
        self.style = Some(style);
        Ok(self)
    }
}

impl Component for SegmentedControl {
    fn build(&self, cx: &mut BuildCx) -> View {
        let style = self
            .style
            .clone()
            .unwrap_or_else(|| cx.theme().button.clone());
        Row::new()
            .children(self.choices.iter().map(|(value, label)| {
                let callback = self.on_change.clone();
                let intent = value.clone();
                Expanded::new()
                    .child(
                        if self.disabled && *value == self.selected {
                            // Disabled colors intentionally override interaction colors;
                            // keep the caller's selection visible through a shape as well.
                            Button::icon(crate::LucideIcon::Check, label.clone())
                        } else {
                            Button::text(label.clone())
                        }
                        .selected(*value == self.selected)
                        .disabled(self.disabled)
                        .style(style.clone())
                        .expect("validated control style")
                        .on_click(move |_| {
                            if let Some(callback) = &callback {
                                callback(intent.clone());
                            }
                        }),
                    )
                    .keyed(value.clone())
            }))
            .build(cx)
    }
}
