use crate::IntoChildView;
use crate::icon::LucideIcon;
use crate::input::event_ctx::EventCtx;
use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Size};
use crate::theme::ControlStyle;
use crate::view::{AnyView, BuildCx, Component, View};
use crate::widgets::Icon;
use crate::widgets::focus::FocusHandle;
use crate::widgets::interactive_region::{InteractionState, InteractiveRegion};
use crate::widgets::text_label::TextLabel;

/// A single-slot control sharing InteractiveRegion's interaction and surface.
///
/// `text` and `icon` select replaceable built-in content. `child`, `child_with`
/// and macro child blocks replace that choice, never append to it.
#[derive(Clone, Default)]
pub struct Button {
    region: InteractiveRegion,
    builtin: Option<(Option<LucideIcon>, String)>,
}

impl Button {
    /// An empty single-slot button.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self {
            builtin: Some((None, text.into())),
            ..Self::new()
        }
    }

    /// An icon followed by visible text (unlike IconButton's metadata label).
    pub fn icon(icon: LucideIcon, text: impl Into<String>) -> Self {
        Self {
            builtin: Some((Some(icon), text.into())),
            ..Self::new()
        }
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.builtin = None;
        self.region = self.region.child(child);
        self
    }

    /// Replaces the slot with a builder on its own child Fiber and hook sequence.
    pub fn child_with(
        mut self,
        builder: impl Fn(&mut BuildCx, InteractionState) -> View + 'static,
    ) -> Self {
        self.builtin = None;
        self.region = self.region.child_with(builder);
        self
    }

    /// Overrides the inherited `theme.button` style.
    pub fn style(mut self, style: ControlStyle) -> Result<Self, crate::theme::AppearanceError> {
        self.region = self.region.style(style)?;
        Ok(self)
    }

    /// Runs on the UI thread with local captures and existing activation timing.
    pub fn on_click(mut self, handler: impl Fn(&mut EventCtx) + 'static) -> Self {
        self.region = self.region.on_activate(handler);
        self
    }

    /// Assigns the actual interactive region a stable focus identity.
    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.region = self.region.focus_handle(handle);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.region = self.region.disabled(disabled);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.region = self.region.selected(selected);
        self
    }
}

impl Component for Button {
    fn build(&self, cx: &mut BuildCx) -> View {
        let mut region = self.region.clone();
        if let Some((icon, text)) = &self.builtin {
            let icon = *icon;
            let text = text.clone();
            // Resolve the exact same local override/theme role as the surface.
            let style = region.resolved_style(cx);
            let gap = cx.theme().spacing;
            region = region.centered().child_with(move |cx, state| {
                let color = style.resolve_state(&state).foreground;
                let label = TextLabel::new(text.clone())
                    .ellipsis()
                    .foreground(color)
                    .expect("valid color");
                match icon {
                    None => label.build(cx),
                    Some(icon) => View::new(
                        IconText { gap },
                        vec![
                            Icon::new(icon)
                                .size(style.icon_size)
                                .expect("valid icon size")
                                .foreground(color)
                                .expect("valid color")
                                .into_child_view(),
                            label.into_child_view(),
                        ],
                        None,
                    ),
                }
            });
        }
        region.build(cx)
    }
}

/// Only arranges normal Icon/TextLabel children; it neither measures glyphs nor paints.
/// Unlike a bounded flex row, this keeps a short icon/text button content-sized.
struct IconText {
    gap: f32,
}

impl AnyView for IconText {
    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _metrics: &crate::text::TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        let icon = children.measure(0, BoxConstraints::loose(constraints.max))?;
        let remaining = (constraints.max.width - icon.width).max(0.0);
        let gap = self.gap.min(remaining);
        let text = children.measure(
            1,
            BoxConstraints::loose(Size::new(
                (remaining - gap).max(0.0),
                constraints.max.height,
            )),
        )?;
        let size = constraints.constrain(Size::new(
            icon.width + gap + text.width,
            icon.height.max(text.height),
        ));
        Ok(ParentLayout {
            size,
            placements: vec![
                (0, Point::new(0.0, (size.height - icon.height) / 2.0)),
                (
                    1,
                    Point::new(icon.width + gap, (size.height - text.height) / 2.0),
                ),
            ],
            diagnostics: Vec::new(),
        })
    }
}

impl crate::WithChildren for Button {
    type Capability = crate::SingleChild;

    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        let mut child = None;
        children.into_single("Button", &mut child)?;
        // Even an empty incoming block replaces conveniences and builders.
        self.builtin = None;
        self.region = crate::WithChildren::with_children(
            self.region,
            child.map_or_else(crate::Children::new, crate::Children::one),
        )?;
        Ok(self)
    }
}
