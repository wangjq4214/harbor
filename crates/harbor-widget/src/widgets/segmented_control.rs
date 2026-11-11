use super::{Button, Expanded, Row};
use crate::ComponentExt;
use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Size};
use crate::theme::{AppearanceError, ControlStyle};
use crate::view::{AnyView, BuildCx, Component, View};
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
    gap: f32,
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
            gap: 0.0,
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

    /// Sets the finite, nonnegative gap between segment surfaces; defaults to zero.
    pub fn gap(mut self, gap: f32) -> Result<Self, crate::layout::LayoutConfigError> {
        crate::layout::configuration::validate_scalar("SegmentedControl.gap", gap)?;
        self.gap = gap;
        Ok(self)
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
            .gap(self.gap)
            .expect("validated segment gap")
            .children(self.choices.iter().map(|(value, label)| {
                let callback = self.on_change.clone();
                let intent = value.clone();
                let label = label.clone();
                let content_style = style.clone();
                Expanded::new()
                    .child(
                        Button::new()
                            .child_with(move |cx, state| {
                                let foreground = content_style.resolve_state(&state).foreground;
                                let label = super::text_label::TextLabel::new(label.clone())
                                    .wrap()
                                    .foreground(foreground)
                                    .expect("validated control foreground")
                                    .build(cx);
                                let mut children = vec![label];
                                if state.selected {
                                    // Keep selection recognizable in every interaction state.
                                    children.push(
                                        super::Icon::new(crate::LucideIcon::Check)
                                            .size(content_style.icon_size)
                                            .expect("validated icon size")
                                            .foreground(foreground)
                                            .expect("validated control foreground")
                                            .build(cx),
                                    );
                                }
                                View::new(
                                    SegmentContent {
                                        icon_size: content_style.icon_size,
                                        gap: cx.theme().spacing,
                                    },
                                    children,
                                    None,
                                )
                            })
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

/// Centers the label on the segment, not on an icon-plus-label group.
/// Symmetric gutters stay reserved even when the selection marker is absent.
struct SegmentContent {
    icon_size: f32,
    gap: f32,
}

impl AnyView for SegmentContent {
    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _metrics: &crate::text::TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        // On very narrow segments, retain most of the width for wrapping text.
        let gutter = (self.icon_size + self.gap).min(constraints.max.width / 5.0);
        let marker_size = self.icon_size.min(gutter).min(constraints.max.height);
        let label = children.measure(
            0,
            BoxConstraints::loose(Size::new(
                (constraints.max.width - 2.0 * gutter).max(0.0),
                constraints.max.height,
            )),
        )?;
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            label.width + 2.0 * gutter
        };
        let size = constraints.constrain(Size::new(width, label.height.max(marker_size)));
        let mut placements = vec![(
            0,
            Point::new(
                (size.width - label.width) / 2.0,
                (size.height - label.height) / 2.0,
            ),
        )];
        if children.len() > 1 {
            let marker = children.measure(
                1,
                BoxConstraints::tight(Size::new(marker_size, marker_size)),
            )?;
            placements.push((1, Point::new(0.0, (size.height - marker.height) / 2.0)));
        }
        Ok(ParentLayout {
            size,
            placements,
            diagnostics: Vec::new(),
        })
    }
}
