use crate::IntoChildView;
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
use crate::widgets::focus::FocusHandle;
use std::rc::Rc;
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

type ActivateCallback = Rc<dyn Fn(&mut EventCtx)>;
type ContentBuilder = Rc<dyn Fn(&mut BuildCx, InteractionState) -> View>;

/// Reconciliation key carried by every content-slot fiber
/// ([`InteractiveRegion::content`]), letting consumers recognize the slot's
/// wrapper fiber without depending on its internal type.
pub const CONTENT_SLOT_KEY: &str = "harbor-content-slot";

/// A state-driven content slot.
///
/// The slot is a Component so its content builds inside the slot's own child
/// Fiber with its own hook sequence and subscriptions assigned by
/// reconciliation, instead of consuming the owning region's hook serials
/// (ADR 0079 D4). It re-reads the retained interaction state at build time,
/// so continuing state changes keep driving content without extra wiring.
#[derive(Clone)]
struct ContentSlot {
    builder: ContentBuilder,
    state: Signal<InteractionRuntimeState>,
}

impl Component for ContentSlot {
    /// Stable key so traversal consumers (focused traversal, tests, docs) can
    /// recognize the slot fiber without decoding widget types beneath it.
    fn key(&self) -> Option<crate::view::Key> {
        Some(crate::view::Key::new(CONTENT_SLOT_KEY))
    }

    fn build(&self, cx: &mut BuildCx) -> View {
        let visual = self.state.read().visual;
        (self.builder)(cx, visual)
    }
}

/// Reusable focusable mouse/keyboard activation state machine.
#[derive(Clone)]
pub struct InteractiveRegion {
    disabled: bool,
    selected: bool,
    focusable: bool,
    cursor: Option<CursorShape>,
    label: Option<String>,
    on_activate: Option<ActivateCallback>,
    child: Option<View>,
    content: Option<ContentBuilder>,
    style: Option<ControlStyle>,
    focus_handle: Option<FocusHandle>,
}

impl InteractiveRegion {
    pub fn new() -> Self {
        Self {
            disabled: false,
            selected: false,
            focusable: true,
            cursor: Some(CursorShape::Default),
            label: None,
            on_activate: None,
            child: None,
            content: None,
            style: None,
            focus_handle: None,
        }
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.content = None;
        self.child = Some(child.into_child_view());
        self
    }

    /// Builds visual content from this region's retained interaction state.
    /// Like `child`, this replaces any previously supplied content.
    pub fn content(
        mut self,
        builder: impl Fn(&mut BuildCx, InteractionState) -> View + 'static,
    ) -> Self {
        self.child = None;
        self.content = Some(Rc::new(builder));
        self
    }

    /// Overrides control appearance without replacing the inherited theme.
    pub fn style(mut self, style: ControlStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Assigns a stable focus identity while retaining nested control tab stops.
    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.focus_handle = Some(handle);
        self
    }

    /// Opt out of focus and tab traversal while retaining pointer activation.
    /// Useful for suggestions whose editor must remain the keyboard/IME owner.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
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

    /// Runs on the UI thread; callbacks may retain local reactive state.
    pub fn on_activate(mut self, callback: impl Fn(&mut EventCtx) + 'static) -> Self {
        self.on_activate = Some(Rc::new(callback));
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
        let style = self
            .style
            .clone()
            .unwrap_or_else(|| cx.theme().button.clone());
        let children = match &self.content {
            Some(builder) => vec![
                ContentSlot {
                    builder: builder.clone(),
                    state: runtime.clone(),
                }
                .into_child_view(),
            ],
            None => self.child.iter().cloned().collect(),
        };
        View::new(
            InteractiveRegionView {
                runtime,
                style,
                label: self.label.clone(),
                cursor: self.cursor,
                on_activate: self.on_activate.clone(),
                focus_handle: self.focus_handle,
                focusable: self.focusable,
            },
            children,
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
        if self.child.is_some() {
            self.content = None;
        }
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
    focus_handle: Option<FocusHandle>,
    focusable: bool,
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
        let colors = self.style.resolve_state(&state);
        let mut primitives = vec![Primitive::Quad {
            rect,
            color: colors.background,
            corner_radius: self.style.radius,
        }];
        // A zero-width border means no outline, not the renderer's fill mode.
        if self.style.border_width > 0.0 && colors.border.a > 0.0 {
            primitives.push(Primitive::Border {
                rect,
                width: self.style.border_width,
                color: colors.border,
                corner_radius: self.style.radius,
            });
        }
        primitives
    }

    fn paint_primitives_for_phase(
        &self,
        phase: crate::view::PaintPhase,
        rect: Rect,
        metrics: &crate::text::TextMetrics,
    ) -> Vec<Primitive> {
        if phase == crate::view::PaintPhase::AfterChildren {
            let state = self.runtime.read().visual;
            let colors = self.style.resolve_state(&state);
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
        // Descendant controls get first refusal. In particular, a nested close
        // button must not also press or activate its enclosing selectable row.
        if ctx.is_capture_phase() {
            return EventHandled::Ignored;
        }
        let handled = match event {
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
                    if self.focusable {
                        ctx.request_focus_from_pointer();
                    }
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
        };
        if handled == EventHandled::Handled {
            ctx.stop_propagation();
        }
        handled
    }

    fn focus_metadata(&self) -> Option<FocusMetadata> {
        self.focusable.then_some(FocusMetadata {
            enabled: !self.runtime.read().visual.disabled,
            participates_in_tab_traversal: true,
            order: 0,
            handle: self.focus_handle.map(|handle| handle.0),
        })
    }

    fn permits_pointer_capture(&self) -> bool {
        !self.runtime.read().visual.disabled
    }

    fn pointer_cursor(&self) -> Option<CursorShape> {
        if self.runtime.read().visual.disabled {
            Some(CursorShape::Default)
        } else {
            self.cursor
        }
    }

    fn accepts_pointer_boundaries(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod content_slot_tests {
    //! Characterization for the state-driven content slot owning its Fiber
    //! boundary (spec 0034 R4): slot hooks never consume the region's serial
    //! space, slot state survives region passes, and teardown releases the
    //! slot's external registrations with the slot fiber.

    use super::*;
    use crate::construction::IntoChildView;
    use crate::layout::Size;
    use crate::runtime::Runtime;
    use crate::scene::primitive::{ExternalDrawId, ExternalScheduleDemand};
    use crate::widgets::column::Column;
    use crate::widgets::sized_box::SizedBox;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Instant;

    #[derive(Clone)]
    struct SlotHost {
        show_slot: Rc<RefCell<Option<Signal<bool>>>>,
        content_state: Rc<RefCell<Option<Signal<&'static str>>>>,
        slot_installed: bool,
    }

    impl SlotHost {
        fn new(slot_installed: bool) -> Self {
            Self {
                show_slot: Rc::new(RefCell::new(None)),
                content_state: Rc::new(RefCell::new(None)),
                slot_installed,
            }
        }
    }

    impl Component for SlotHost {
        fn build(&self, cx: &mut BuildCx) -> View {
            let show_slot = cx.use_state(|| self.slot_installed);
            *self.show_slot.borrow_mut() = Some(show_slot.clone());
            let region = if show_slot.read().clone() {
                let content_state = self.content_state.clone();
                InteractiveRegion::new().content(move |cx, _state| {
                    let value = cx.use_state(|| "slot-initial");
                    *content_state.borrow_mut() = Some(value.clone());
                    let _ = value.read().clone();
                    cx.register_external_schedule(
                        ExternalDrawId::new(7_034),
                        Arc::new(|_id, _now| ExternalScheduleDemand::empty()),
                    );
                    SizedBox::new(Size::new(4.0, 4.0)).build(cx)
                })
            } else {
                InteractiveRegion::new()
            };

            crate::view! {
                Column::new() => { region; }
            }
        }
    }

    fn region_fiber_id(runtime: &Runtime) -> crate::fiber::FiberId {
        // root (host + collapsed outer composition) -> region fiber -> slot fiber
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .children()[0]
    }

    fn slot_fiber_id(runtime: &Runtime) -> Option<crate::fiber::FiberId> {
        runtime
            .arena()
            .get(region_fiber_id(runtime))
            .unwrap()
            .children()
            .first()
            .copied()
    }

    #[test]
    fn slot_hooks_live_on_their_own_fiber() {
        let host = SlotHost::new(true);
        let mut runtime = Runtime::new();
        runtime.set_root(host.clone());
        runtime.update(Instant::now());

        // The slot's external schedule registration installs with the parent
        // rebuild that materializes the slot.
        assert!(
            runtime
                .external_schedule_ids()
                .contains(&ExternalDrawId::new(7_034))
        );
        let region_id = region_fiber_id(&runtime);
        let slot_id = slot_fiber_id(&runtime).expect("slot content became a child fiber");

        // The region's hook sequence holds exactly its own interaction state;
        // the content hook belongs to the slot fiber instead.
        assert_eq!(runtime.arena().get(region_id).unwrap().hooks.len(), 1);
        assert_eq!(runtime.arena().get(slot_id).unwrap().hooks.len(), 1);

        // Slot state mutated between passes survives the next region pass.
        let content_signal = host.content_state.borrow().clone().unwrap();
        content_signal.set("revised");
        runtime.update(Instant::now());
        let content_signal = host.content_state.borrow().clone().unwrap();
        assert_eq!(content_signal.read().clone(), "revised");
    }

    #[test]
    fn removed_slot_fiber_releases_hooks_and_registrations() {
        let host = SlotHost::new(true);
        let mut runtime = Runtime::new();
        runtime.set_root(host.clone());
        runtime.update(Instant::now());
        assert!(slot_fiber_id(&runtime).is_some());
        assert!(
            runtime
                .external_schedule_ids()
                .contains(&ExternalDrawId::new(7_034))
        );

        // Dismount the slot and confirm its fiber disappeared with its hooks.
        host.show_slot.borrow().as_ref().unwrap().set(false);
        runtime.update(Instant::now());
        assert!(slot_fiber_id(&runtime).is_none());
        assert!(
            !runtime
                .external_schedule_ids()
                .contains(&ExternalDrawId::new(7_034)),
            "the slot's external registration leaves with its fiber"
        );

        let region_id = region_fiber_id(&runtime);
        let region = runtime.arena().get(region_id).unwrap();
        assert!(
            region.children.is_empty(),
            "the region keeps only its own fiber state after the slot is removed",
        );
    }
}
