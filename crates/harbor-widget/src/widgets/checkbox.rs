use super::button::IconText;
use super::interactive_region::InteractiveRegion;
use super::text_label::TextLabel;
use crate::IntoChildView;
use crate::icon::LucideIcon;
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::theme::{AppearanceError, ControlStyle};
use crate::view::{AnyView, BuildCx, Component, View};
use crate::widgets::FocusHandle;

/// A caller-controlled boolean choice with vector checkbox artwork.
///
/// Activation (pointer release, Space or Enter) emits the inverse of the supplied
/// checked value. It never changes that value itself: rebuild with the caller's
/// new value to change the checkmark. Uses the inherited `theme.button` style.
#[derive(Clone)]
pub struct Checkbox {
    label: String,
    checked: bool,
    region: InteractiveRegion,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            region: InteractiveRegion::new(),
        }
    }

    /// Runs on the UI thread and may capture local caller-owned state.
    pub fn on_change(mut self, callback: impl Fn(bool) + 'static) -> Self {
        let next = !self.checked;
        self.region = self.region.on_activate(move |_| callback(next));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.region = self.region.disabled(disabled);
        self
    }

    pub fn style(mut self, style: ControlStyle) -> Result<Self, AppearanceError> {
        self.region = self.region.style(style)?;
        Ok(self)
    }

    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.region = self.region.focus_handle(handle);
        self
    }
}

impl Component for Checkbox {
    fn build(&self, cx: &mut BuildCx) -> View {
        let region = self.region.clone().selected(self.checked);
        let style = region.resolved_style(cx);
        let label = self.label.clone();
        let gap = cx.theme().spacing;
        region
            .child_with(move |_cx, state| {
                let color = style.resolve_state(&state).foreground;
                View::new(
                    IconText { gap },
                    vec![
                        View::new(
                            Checkmark {
                                checked: state.selected,
                                size: style.icon_size,
                                color,
                            },
                            vec![],
                            None,
                        ),
                        TextLabel::new(label.clone())
                            .wrap()
                            .foreground(color)
                            .expect("validated control foreground")
                            .into_child_view(),
                    ],
                    None,
                )
            })
            .build(cx)
    }
}

/// Content only: the region owns the hit target, interaction and focus ring.
struct Checkmark {
    checked: bool,
    size: f32,
    color: Color,
}

impl AnyView for Checkmark {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.constrain(Size::new(self.size, self.size))
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        let side = self.size.min(rect.size().width).min(rect.size().height);
        if side <= 0.0 || self.color.a <= 0.0 {
            return Vec::new();
        }
        let rect = Rect::from_min_size(
            Point::new(
                rect.min.x + (rect.size().width - side) / 2.0,
                rect.min.y + (rect.size().height - side) / 2.0,
            ),
            Size::new(side, side),
        );
        let mut primitives = vec![Primitive::Border {
            rect,
            width: 1.5_f32.min(side / 2.0),
            color: self.color,
            corner_radius: side * 0.15,
        }];
        if self.checked {
            primitives.push(Primitive::Icon {
                icon: LucideIcon::Check,
                rect,
                color: self.color,
            });
        }
        primitives
    }
}
