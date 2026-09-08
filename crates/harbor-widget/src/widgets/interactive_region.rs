use crate::effects::CursorShape;
use crate::input::event::{
    FocusEvent, Key, KeyboardEvent, PointerBoundaryKind, PointerButton, PointerPhase, UiEvent,
};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::Primitive;
use crate::signal::Signal;
use crate::theme::ControlStyle;
use crate::view::{AnyView, BuildCx, Component, FocusMetadata, View};
use std::sync::Arc;

/// Orthogonal state exposed by interactive desktop controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InteractionState {
    pub disabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub cancelled: bool,
    pub focused: bool,
    pub focus_visible: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct InteractionRuntimeState {
    visual: InteractionState,
    active_pointer: Option<u64>,
}

type ActivateCallback = Arc<dyn Fn(&mut EventCtx) + Send + Sync>;

/// Reusable focusable mouse/keyboard activation state machine.
#[derive(Clone)]
pub struct InteractiveRegion {
    disabled: bool,
    selected: bool,
    cursor: Option<CursorShape>,
    compact: bool,
    label: Option<String>,
    on_activate: Option<ActivateCallback>,
    child: Option<View>,
}

impl InteractiveRegion {
    pub fn new() -> Self {
        Self {
            disabled: false,
            selected: false,
            cursor: Some(CursorShape::Pointer),
            compact: false,
            label: None,
            on_activate: None,
            child: None,
        }
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
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

    pub fn cursor(mut self, cursor: CursorShape) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn on_activate(mut self, callback: impl Fn(&mut EventCtx) + Send + Sync + 'static) -> Self {
        self.on_activate = Some(Arc::new(callback));
        self
    }

    pub(crate) fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub(crate) fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Default for InteractiveRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for InteractiveRegion {
    fn build(&self, cx: &mut BuildCx) -> View {
        let runtime = cx.use_state(|| InteractionRuntimeState {
            visual: InteractionState {
                disabled: self.disabled,
                selected: self.selected,
                ..InteractionState::default()
            },
            active_pointer: None,
        });
        let previous = runtime.read().clone();
        if previous.visual.disabled != self.disabled || previous.visual.selected != self.selected {
            let mut next = previous;
            next.visual.disabled = self.disabled;
            next.visual.selected = self.selected;
            if self.disabled {
                next.active_pointer = None;
                next.visual.pressed = false;
                next.visual.cancelled = false;
            }
            runtime.set(next);
        }
        let style = if self.compact {
            cx.theme().icon_button.clone()
        } else {
            cx.theme().button.clone()
        };
        View::new(
            InteractiveRegionView {
                runtime,
                style,
                label: self.label.clone(),
                cursor: self.cursor,
                on_activate: self.on_activate.clone(),
            },
            self.child.iter().cloned().collect(),
            None,
        )
    }
}

impl crate::WithChildren for InteractiveRegion {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("InteractiveRegion", &mut self.child)?;
        Ok(self)
    }
}

#[derive(Clone)]
struct InteractiveRegionView {
    runtime: Signal<InteractionRuntimeState>,
    style: ControlStyle,
    label: Option<String>,
    cursor: Option<CursorShape>,
    on_activate: Option<ActivateCallback>,
}

impl InteractiveRegionView {
    fn update(&self, ctx: &mut EventCtx, change: impl FnOnce(&mut InteractionRuntimeState)) {
        let current = self.runtime.read().clone();
        let mut next = current.clone();
        change(&mut next);
        if next != current {
            self.runtime.set(next);
            ctx.invalidate_paint();
        }
    }

    fn activate(&self, ctx: &mut EventCtx) {
        if !self.runtime.read().visual.disabled
            && let Some(callback) = &self.on_activate
        {
            callback(ctx);
        }
    }
}

impl AnyView for InteractiveRegionView {
    fn intrinsic_size(
        &self,
        constraints: BoxConstraints,
        metrics: &crate::text::TextMetrics,
    ) -> Size {
        let width = self
            .label
            .as_ref()
            .map(|label| {
                label.chars().count() as f32 * metrics.cell_width
                    + self.style.horizontal_padding * 2.0
            })
            .unwrap_or(0.0);
        let height = if self.label.is_some() {
            metrics.line_height.max(self.style.min_height)
        } else {
            self.style.min_height
        };
        constraints.constrain(Size::new(width, height))
    }

    fn child_constraints(&self, constraints: BoxConstraints) -> BoxConstraints {
        constraints.deflate(Size::new(self.style.horizontal_padding * 2.0, 0.0))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        metrics: &crate::text::TextMetrics,
    ) -> (Size, Vec<Point>) {
        let child = child_sizes.first().copied().unwrap_or(Size::ZERO);
        let label_size = self.label.as_ref().map_or(Size::ZERO, |label| {
            Size::new(
                label.chars().count() as f32 * metrics.cell_width,
                metrics.line_height,
            )
        });
        let content = Size::new(
            child.width.max(label_size.width),
            child.height.max(label_size.height),
        );
        let size = constraints.constrain(Size::new(
            content.width + self.style.horizontal_padding * 2.0,
            content.height.max(self.style.min_height),
        ));
        let origin = Point::new(
            self.style.horizontal_padding.min(size.width),
            ((size.height - child.height) / 2.0).max(0.0),
        );
        (size, vec![origin; child_sizes.len()])
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &crate::text::TextMetrics) -> Vec<Primitive> {
        let state = self.runtime.read().visual;
        let colors =
            self.style
                .resolve(state.disabled, state.selected, state.hovered, state.pressed);
        vec![
            Primitive::Quad {
                rect,
                color: colors.background,
                corner_radius: self.style.radius,
            },
            Primitive::Border {
                rect,
                width: self.style.border_width,
                color: colors.border,
                corner_radius: self.style.radius,
            },
        ]
    }

    fn paint_primitives_for_phase(
        &self,
        phase: crate::view::PaintPhase,
        rect: Rect,
        metrics: &crate::text::TextMetrics,
    ) -> Vec<Primitive> {
        if phase == crate::view::PaintPhase::AfterChildren {
            let state = self.runtime.read().visual;
            let colors =
                self.style
                    .resolve(state.disabled, state.selected, state.hovered, state.pressed);
            let mut primitives = Vec::new();
            if let Some(label) = &self.label {
                let text_width = label.chars().count() as f32 * metrics.cell_width;
                primitives.push(Primitive::Text {
                    text: Arc::from(label.as_str()),
                    origin: Point::new(
                        rect.min.x + (rect.size().width - text_width).max(0.0) / 2.0,
                        rect.min.y + (rect.size().height - metrics.line_height).max(0.0) / 2.0,
                    ),
                    color: colors.foreground,
                });
            }
            if state.focused && state.focus_visible {
                primitives.push(Primitive::Border {
                    rect,
                    width: self.style.border_width + 1.0,
                    color: self.style.focus_ring,
                    corner_radius: self.style.radius,
                });
            }
            return primitives;
        }
        self.paint_primitives(rect, metrics)
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        match event {
            UiEvent::PointerBoundary(boundary) => {
                self.update(ctx, |state| {
                    state.visual.hovered = boundary.kind == PointerBoundaryKind::Enter;
                });
                EventHandled::Handled
            }
            UiEvent::Pointer(pointer) => match pointer.phase {
                PointerPhase::Down
                    if pointer.button == PointerButton::Left
                        && !self.runtime.read().visual.disabled
                        && self.runtime.read().active_pointer.is_none() =>
                {
                    self.update(ctx, |state| {
                        state.active_pointer = Some(pointer.pointer_id);
                        state.visual.pressed = true;
                        state.visual.cancelled = false;
                    });
                    ctx.request_focus_from_pointer();
                    ctx.capture_pointer(pointer.pointer_id);
                    EventHandled::Handled
                }
                PointerPhase::Move => {
                    let active = self.runtime.read().active_pointer;
                    if active == Some(pointer.pointer_id) {
                        let inside = rect.contains(pointer.position);
                        self.update(ctx, |state| {
                            state.visual.pressed = inside;
                            state.visual.cancelled = !inside;
                        });
                        EventHandled::Handled
                    } else {
                        EventHandled::Ignored
                    }
                }
                PointerPhase::Up if pointer.button == PointerButton::Left => {
                    let active = self.runtime.read().active_pointer;
                    if active != Some(pointer.pointer_id) {
                        return EventHandled::Ignored;
                    }
                    let activate =
                        self.runtime.read().visual.pressed && rect.contains(pointer.position);
                    self.update(ctx, |state| {
                        state.active_pointer = None;
                        state.visual.pressed = false;
                        state.visual.cancelled = false;
                        state.visual.hovered = rect.contains(pointer.position);
                    });
                    ctx.release_pointer(pointer.pointer_id);
                    if activate {
                        self.activate(ctx);
                    }
                    EventHandled::Handled
                }
                PointerPhase::Cancel => {
                    if self.runtime.read().active_pointer != Some(pointer.pointer_id) {
                        return EventHandled::Ignored;
                    }
                    self.update(ctx, |state| {
                        state.active_pointer = None;
                        state.visual.pressed = false;
                        state.visual.cancelled = false;
                    });
                    ctx.release_pointer(pointer.pointer_id);
                    EventHandled::Handled
                }
                _ => EventHandled::Ignored,
            },
            UiEvent::Focus(event) => {
                self.update(ctx, |state| match event {
                    FocusEvent::Gained => {
                        state.visual.focused = true;
                        state.visual.focus_visible = false;
                    }
                    FocusEvent::GainedVisible => {
                        state.visual.focused = true;
                        state.visual.focus_visible = true;
                    }
                    FocusEvent::VisibilityChanged(visible) => {
                        state.visual.focus_visible = *visible;
                    }
                    FocusEvent::Lost => {
                        state.visual.focused = false;
                        state.visual.focus_visible = false;
                    }
                });
                EventHandled::Handled
            }
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Enter | Key::Space,
                ..
            }) => {
                self.activate(ctx);
                EventHandled::Handled
            }
            _ => EventHandled::Ignored,
        }
    }

    fn focus_metadata(&self) -> Option<FocusMetadata> {
        Some(FocusMetadata {
            enabled: !self.runtime.read().visual.disabled,
            order: 0,
            handle: None,
        })
    }

    fn is_focusable(&self) -> bool {
        !self.runtime.read().visual.disabled
    }

    fn permits_pointer_capture(&self) -> bool {
        !self.runtime.read().visual.disabled
    }

    fn pointer_cursor(&self) -> Option<CursorShape> {
        if self.runtime.read().visual.disabled {
            Some(CursorShape::NotAllowed)
        } else {
            self.cursor
        }
    }

    fn accepts_pointer_boundaries(&self) -> bool {
        true
    }
}
