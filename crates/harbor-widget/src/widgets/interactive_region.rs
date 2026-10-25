use super::control_surface::ControlSurface;
use crate::IntoChildView;
use crate::effects::CursorShape;
use crate::input::event::{
    FocusEvent, Key, KeyboardEvent, PointerBoundaryKind, PointerButton, PointerPhase, UiEvent,
};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Rect};
use crate::scene::primitive::Primitive;
use crate::signal::Signal;
use crate::theme::ControlStyle;
use crate::view::{AnyView, BuildCx, Component, FocusMetadata, View};
use crate::widgets::focus::FocusHandle;
use std::rc::Rc;

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
/// ([`InteractiveRegion::child_with`]), letting consumers recognize the slot's
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
    centered: bool,
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
            centered: false,
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
    pub fn child_with(
        mut self,
        builder: impl Fn(&mut BuildCx, InteractionState) -> View + 'static,
    ) -> Self {
        self.child = None;
        self.content = Some(Rc::new(builder));
        self
    }

    /// Overrides control appearance without replacing the inherited theme.
    pub fn style(mut self, style: ControlStyle) -> Result<Self, crate::theme::AppearanceError> {
        style.validate()?;
        self.style = Some(style);
        Ok(self)
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

    pub(super) fn resolved_style(&self, cx: &BuildCx) -> ControlStyle {
        self.style
            .clone()
            .unwrap_or_else(|| cx.theme().button.clone())
    }

    /// Center built-in content without changing application region composition.
    pub(super) fn centered(mut self) -> Self {
        self.centered = true;
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
        let style = self.resolved_style(cx);
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
                surface: ControlSurface {
                    style,
                    centered: self.centered,
                },
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
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("InteractiveRegion", &mut self.child)?;
        self.content = None;
        Ok(self)
    }
}

#[derive(Clone)]
struct InteractiveRegionView {
    runtime: Signal<InteractionRuntimeState>,
    surface: ControlSurface,
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
    fn child_constraints(&self, constraints: BoxConstraints) -> BoxConstraints {
        self.surface.child_constraints(constraints)
    }

    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn crate::layout::ChildMeasurer,
        _metrics: &crate::text::TextMetrics,
    ) -> Result<crate::layout::ParentLayout, crate::layout::LayoutError> {
        let child_constraints = self.surface.child_constraints(constraints);
        let child_sizes = (0..children.len())
            .map(|index| children.measure(index, child_constraints))
            .collect::<Result<Vec<_>, _>>()?;
        let (size, origins) = self.surface.layout(constraints, &child_sizes)?;
        Ok(crate::layout::ParentLayout {
            size,
            placements: origins.into_iter().enumerate().collect(),
            diagnostics: Vec::new(),
        })
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &crate::text::TextMetrics) -> Vec<Primitive> {
        self.surface.paint(
            crate::view::PaintPhase::BeforeChildren,
            rect,
            self.runtime.read().visual,
        )
    }

    fn paint_primitives_for_phase(
        &self,
        phase: crate::view::PaintPhase,
        rect: Rect,
        _metrics: &crate::text::TextMetrics,
    ) -> Vec<Primitive> {
        self.surface.paint(phase, rect, self.runtime.read().visual)
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        // Descendant controls get first refusal. In particular, a nested close
        // button must not also press or activate its enclosing selectable row.
        if ctx.is_capture_phase() {
            return EventHandled::Ignored;
        }
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
        }
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
                InteractiveRegion::new().child_with(move |cx, _state| {
                    let value = cx.use_state(|| "slot-initial");
                    *content_state.borrow_mut() = Some(value.clone());
                    let _ = value.read().clone();
                    cx.register_external_schedule(
                        ExternalDrawId::new(7_034),
                        Arc::new(|_id, _now| ExternalScheduleDemand::empty()),
                    );
                    SizedBox::new(Size::new(4.0, 4.0)).unwrap().build(cx)
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

    #[derive(Clone)]
    struct ReplacementHost {
        mode: Signal<u8>,
        source: Signal<u32>,
        slot_state: Rc<RefCell<Option<Signal<u32>>>>,
        observations: Rc<RefCell<Vec<(u32, crate::scene::primitive::Color)>>>,
    }

    impl Component for ReplacementHost {
        fn build(&self, cx: &mut BuildCx) -> View {
            cx.track(&self.mode);
            let source = self.source.clone();
            let slot_state = self.slot_state.clone();
            let observations = self.observations.clone();
            let builder = move |cx: &mut BuildCx, _state| {
                cx.track(&source);
                let retained = cx.use_state(|| 10u32);
                *slot_state.borrow_mut() = Some(retained.clone());
                let value = *source.read() + *retained.read();
                observations
                    .borrow_mut()
                    .push((value, cx.theme().button.normal.foreground));
                cx.register_external_schedule(
                    ExternalDrawId::new(7_035),
                    Arc::new(|_, _| ExternalScheduleDemand::empty()),
                );
                SizedBox::new(Size::new(4.0, 4.0)).unwrap().build(cx)
            };
            let region = InteractiveRegion::new().child_with(builder.clone());
            let mode = *self.mode.read();
            let description = match mode {
                0 => crate::view! { region; },
                1 => crate::view! { region => { SizedBox::new(Size::new(8.0, 8.0)); } },
                2 => crate::view! { region => {} },
                4 => crate::view! { region.child(SizedBox::new(Size::ZERO).unwrap()) => {} },
                // content must replace an explicit child, in either setting order.
                _ => {
                    crate::view! { region.child(SizedBox::new(Size::ZERO).unwrap()).child_with(builder); }
                }
            };
            crate::view! { Column::new() => { description; } }
        }
    }

    #[test]
    fn block_replacement_and_clearing_teardown_content_and_reselection_restores_own_fiber() {
        use crate::scene::primitive::Color;
        use crate::widgets::ThemeProvider;
        for replacement_mode in [1, 2, 4] {
            let host = ReplacementHost {
                mode: Signal::new(0),
                source: Signal::new(1),
                slot_state: Rc::new(RefCell::new(None)),
                observations: Rc::new(RefCell::new(Vec::new())),
            };
            let mut theme = crate::Theme::default();
            theme.button.normal.foreground = Color::GREEN;
            let mut runtime = Runtime::new();
            runtime.set_root(
                ThemeProvider::new(theme)
                    .expect("valid theme")
                    .child(host.clone()),
            );
            runtime.update(Instant::now());
            let provider_id = runtime.root_id().unwrap();
            let host_id = runtime.arena().get(provider_id).unwrap().children()[0];
            let region_id = runtime.arena().get(host_id).unwrap().children()[0];
            let old_slot = runtime.arena().get(region_id).unwrap().children()[0];
            assert_eq!(runtime.arena().get(region_id).unwrap().hooks.len(), 1);
            assert_eq!(runtime.arena().get(old_slot).unwrap().hooks.len(), 1);
            assert!(
                runtime
                    .external_schedule_ids()
                    .contains(&ExternalDrawId::new(7_035))
            );
            assert_eq!(*host.observations.borrow(), vec![(11, Color::GREEN)]);
            let old_state = host.slot_state.borrow().clone().unwrap();
            old_state.set(20);
            runtime.update(Instant::now());
            assert_eq!(host.observations.borrow().last(), Some(&(21, Color::GREEN)));

            host.mode.set(replacement_mode);
            runtime.update(Instant::now());
            assert!(!runtime.arena().contains(old_slot));
            assert!(
                !runtime
                    .external_schedule_ids()
                    .contains(&ExternalDrawId::new(7_035))
            );
            assert_eq!(
                runtime.arena().get(region_id).unwrap().children().len(),
                usize::from(replacement_mode == 1)
            );
            let observed = host.observations.borrow().len();
            old_state.set(99);
            host.source.set(5);
            runtime.update(Instant::now());
            assert_eq!(
                host.observations.borrow().len(),
                observed,
                "removed content must not rebuild"
            );

            host.mode.set(3);
            runtime.update(Instant::now());
            let new_slot = runtime.arena().get(region_id).unwrap().children()[0];
            assert_ne!(new_slot, old_slot);
            assert_eq!(runtime.arena().get(region_id).unwrap().hooks.len(), 1);
            assert_eq!(runtime.arena().get(new_slot).unwrap().hooks.len(), 1);
            assert_eq!(host.observations.borrow().last(), Some(&(15, Color::GREEN)));
            assert!(
                runtime
                    .external_schedule_ids()
                    .contains(&ExternalDrawId::new(7_035))
            );
        }
    }
}
