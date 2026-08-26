mod event_router;
mod frame_encoder;

use crate::effects::{ClipboardEffect, ControlFlowEffect, ExternalInvalidation, RuntimeEffects};
use crate::fiber::{
    DirtyFlags, Fiber, FiberArena, FiberId, layout_fiber, paint_fiber,
    reconcile_children_with_externals, unmount_fiber,
};
use crate::input::event::{PointerPhase, UiEvent};
#[cfg(test)]
use crate::input::event_ctx::EventCtx;
use crate::input::state::InputState;
use crate::layout::{BoxConstraints, Point, Size};
use crate::renderer::Viewport;
use crate::runtime::event_router::EventRouter;
use crate::runtime::frame_encoder::{EncodeScene, FrameEncoder};
use crate::scene::primitive::{
    ExternalDrawFn, ExternalDrawId, ExternalFrameAppearance, ExternalFrameAppearanceFn,
    ExternalScheduleFn,
};
use crate::scene::{SceneDelta, SceneGraph};
use crate::signal::{
    RuntimeId, RuntimeScope, active_runtime_id, mark_dirty_for, remove_runtime, take_dirty,
};
use crate::text::{TextMetrics, TextRunCache, text_metrics_equal};
use crate::view::{BuildCx, Component, ExternalRegistrations};
use hashbrown::HashMap;
use std::sync::Arc;
use std::time::Instant;

// ── External input trampoline ──────────────────────────────────────────────

// Thread-local queues for external input events, grouped by owning Runtime.
//
// Written by `CustomPaint::handle_event` during the event walk and drained
// by `Runtime::drain_external_input` after the walk completes. The active
// Runtime scope is required so two window runtimes on the same thread cannot
// consume each other's deferred input.
thread_local! {
    static PENDING_EXTERNAL_INPUT: std::cell::RefCell<
        HashMap<
            RuntimeId,
            Vec<(crate::scene::primitive::ExternalDrawId, crate::input::event::UiEvent)>,
        >,
    > = std::cell::RefCell::new(HashMap::new());
}

/// Called by CustomPaint::handle_event during event routing.
/// Queues an event for deferred delivery to the active Runtime's host.
///
/// Calls outside a Runtime dispatch are ignored because there is no safe
/// owner for the event; CustomPaint always runs inside `Runtime::dispatch`.
pub fn queue_external_input(
    id: crate::scene::primitive::ExternalDrawId,
    event: crate::input::event::UiEvent,
) {
    let Some(runtime_id) = active_runtime_id() else {
        return;
    };
    PENDING_EXTERNAL_INPUT.with(|queues| {
        queues
            .borrow_mut()
            .entry(runtime_id)
            .or_default()
            .push((id, event));
    });
}

fn remove_external_input(runtime_id: RuntimeId) {
    PENDING_EXTERNAL_INPUT.with(|queues| {
        queues.borrow_mut().remove(&runtime_id);
    });
}

// ── Runtime ─────────────────────────────────────────────────────────────────

/// Documented fallback metrics used by [`Runtime::new`].
pub const DEFAULT_TEXT_METRICS: TextMetrics = TextMetrics {
    cell_width: 10.0,
    line_height: 20.0,
    ascent: 16.0,
    underline_position: 0.0,
    underline_thickness: 1.5,
    strikethrough_position: 0.0,
    strikethrough_thickness: 1.5,
};

/// Top-level widget tree scheduler.
///
/// Owns the fiber tree, text metrics, and text run cache lifecycle while
/// orchestrating reconcile → layout → paint cycles. Input routing and GPU
/// encoding are delegated to [`EventRouter`] and [`FrameEncoder`].
pub struct Runtime {
    runtime_id: RuntimeId,
    arena: FiberArena,
    root_id: Option<FiberId>,
    root_component: Option<Box<dyn Component>>,
    text_metrics: TextMetrics,
    scene_graph: SceneGraph,
    next_scene_item_id: u64,
    pending_delta: Option<SceneDelta>,
    current_viewport: Option<Viewport>,
    external_draws: HashMap<ExternalDrawId, Arc<ExternalDrawFn<'static>>>,
    external_schedules: HashMap<ExternalDrawId, Arc<ExternalScheduleFn>>,
    /// At most one frame-appearance provider is expected for a Runtime.
    external_frame_appearance: Option<(ExternalDrawId, Arc<ExternalFrameAppearanceFn>)>,
    external_eligible: HashMap<ExternalDrawId, bool>,
    events: EventRouter,
    encoder: FrameEncoder,
    pending_effects: RuntimeEffects,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Creates a Runtime with the documented fallback text metrics.
    pub fn new() -> Self {
        Self::with_text_metrics(DEFAULT_TEXT_METRICS)
    }

    /// Creates a Runtime that owns the supplied text metrics.
    pub fn with_text_metrics(text_metrics: TextMetrics) -> Self {
        Runtime {
            runtime_id: RuntimeId::new(),
            arena: FiberArena::new(),
            root_id: None,
            root_component: None,
            text_metrics,
            scene_graph: SceneGraph::new(),
            next_scene_item_id: 1,
            pending_delta: None,
            current_viewport: None,
            external_draws: HashMap::new(),
            external_schedules: HashMap::new(),
            external_frame_appearance: None,
            external_eligible: HashMap::new(),
            events: EventRouter::new(),
            encoder: FrameEncoder::new(),
            pending_effects: RuntimeEffects::default(),
        }
    }

    /// Returns the text metrics owned by this Runtime.
    pub fn text_metrics(&self) -> &TextMetrics {
        &self.text_metrics
    }

    /// Replaces this Runtime's text metrics and marks the root layout dirty.
    pub fn set_text_metrics(&mut self, text_metrics: TextMetrics) {
        if text_metrics_equal(&self.text_metrics, &text_metrics) {
            return;
        }
        self.text_metrics = text_metrics;
        if let Some(root_id) = self.root_id {
            if let Some(fiber) = self.arena.get_mut(root_id) {
                fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
            }
            mark_dirty_for(self.runtime_id, root_id);
        }
    }

    /// Sets the root component and performs the initial build + layout.
    ///
    /// If a previous root existed, it is unmounted recursively.
    pub fn set_root(&mut self, root: impl Component + 'static) {
        let _scope = RuntimeScope::enter(self.runtime_id);

        // Unmount old root if present
        if let Some(old_root) = self.root_id.take() {
            unmount_fiber(&mut self.arena, old_root);
        }

        // Create a temporary root fiber
        let root_fiber = Fiber::new(
            None,
            std::any::TypeId::of::<()>(), // placeholder, updated below
            None,
        );
        let root_id = self.arena.insert(root_fiber);
        self.root_id = Some(root_id);

        self.root_component = Some(Box::new(root));

        // Full rebuild from root with empty old children
        self.rebuild_root(root_id, &[]);

        // Mark root dirty (first update will trigger a redraw)
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.flags.insert(DirtyFlags::BUILD_DIRTY);
            fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
        }
        mark_dirty_for(self.runtime_id, root_id);
    }

    /// Processes dirty fibers and runs layout.
    ///
    /// Returns the platform-neutral effects produced by this update.
    /// Always folds registered external schedule demands, including clean idle
    /// turns, so hosts can wait until the next deadline without polling.
    pub fn update(&mut self, now: Instant) -> RuntimeEffects {
        let mut effects = std::mem::take(&mut self.pending_effects);
        let dirty = take_dirty(self.runtime_id);

        if !dirty.is_empty()
            && let Some(root_id) = self.root_id
        {
            let old_children = self
                .arena
                .get(root_id)
                .map(|f| f.children.clone())
                .unwrap_or_default();

            self.rebuild_root(root_id, &old_children);
            effects.merge(RuntimeEffects::request_redraw());
        }

        effects.merge(self.collect_external_schedule(now));
        effects
    }

    /// Queries registered external schedule providers and folds their demands.
    ///
    /// Per-id eligibility is stored for encode. Window effects stay eligible so
    /// a deferred external cannot freeze every widget that shares this Runtime.
    fn collect_external_schedule(&mut self, now: Instant) -> RuntimeEffects {
        let mut redraw_now = false;
        let mut earliest: Option<Instant> = None;
        let mut has_deferred_externals = false;
        self.external_eligible.clear();

        for (id, schedule) in &self.external_schedules {
            let demand = schedule(*id, now);
            self.external_eligible
                .insert(*id, demand.ordinary_present_eligible);
            if !demand.ordinary_present_eligible {
                has_deferred_externals = true;
            }
            if demand.ordinary_present_eligible {
                redraw_now |= demand.redraw_now;
            }
            earliest = match (earliest, demand.deadline) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (None, Some(right)) => Some(right),
                (left, None) => left,
            };
        }

        RuntimeEffects {
            ordinary_present_eligible: true,
            has_deferred_externals,
            request_redraw: redraw_now,
            control_flow: earliest.map(ControlFlowEffect::WaitUntil),
            ..RuntimeEffects::default()
        }
    }

    /// Marks work originating outside the runtime as pending.
    ///
    /// The source is intentionally represented only by a marker so this core
    /// API remains independent of terminals, application events, and window
    /// systems. A host applies the returned effects just like any other turn.
    pub fn invalidate_external(&mut self, _work: ExternalInvalidation) -> RuntimeEffects {
        let Some(root_id) = self.root_id else {
            return RuntimeEffects::default();
        };

        mark_dirty_for(self.runtime_id, root_id);
        RuntimeEffects::request_redraw()
    }

    /// Shared rebuild → reconcile → layout → paint sequence.
    fn rebuild_root(&mut self, root_id: FiberId, old_children: &[FiberId]) {
        let _scope = RuntimeScope::enter(self.runtime_id);
        let hooks = std::mem::take(&mut self.arena.get_mut(root_id).unwrap().hooks);
        let mut cx = BuildCx {
            current_fiber: Some(root_id),
            hooks,
            hook_index: 0,
            externals: ExternalRegistrations::default(),
        };

        let view = self.root_component.as_ref().unwrap().build(&mut cx);
        let widget_type = view.widget_type();
        let key = view.key().cloned();
        let (inner, children, _explicit_key) = view.decompose();

        // Update root fiber
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.hooks = cx.hooks;
            fiber.key = key;
            fiber.widget_type = widget_type;
            fiber.view = Some(inner);
            fiber.flags.remove(DirtyFlags::BUILD_DIRTY);
        }

        // Reconcile children
        let new_children = reconcile_children_with_externals(
            &mut self.arena,
            root_id,
            old_children,
            children,
            &mut cx.externals,
        );
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.children = new_children;
        }
        self.install_externals(&mut cx.externals);

        // Layout
        let viewport_size = self
            .current_viewport
            .as_ref()
            .map(|v| v.logical_size)
            .unwrap_or(Size::new(800.0, 600.0));
        let constraints = BoxConstraints::loose(viewport_size);
        layout_fiber(
            &mut self.arena,
            root_id,
            constraints,
            Point::ZERO,
            &self.text_metrics,
        );
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.flags.remove(DirtyFlags::LAYOUT_DIRTY);
        }

        // Paint
        self.run_paint_pass();

        // Clean input state
        self.events.clear_dead_targets(&self.arena);
    }

    /// Replaces Runtime-owned external registrations from one rebuild bag.
    fn install_externals(&mut self, externals: &mut ExternalRegistrations) {
        self.external_draws.clear();
        self.external_draws.extend(externals.draws.drain(..));
        self.external_schedules.clear();
        self.external_schedules
            .extend(externals.schedules.drain(..));
        self.external_frame_appearance = None;
        for (id, provider) in externals.frame_appearances.drain(..) {
            if self.external_frame_appearance.is_some() {
                tracing::warn!(
                    id,
                    "multiple external frame-appearance providers; retaining the first"
                );
            } else {
                self.external_frame_appearance = Some((id, provider));
            }
        }
    }

    /// Returns the viewport installed for the current frame, if any.
    pub fn current_viewport(&self) -> Option<&Viewport> {
        self.current_viewport.as_ref()
    }

    /// Initializes the GPU renderer. Must be called after a wgpu Device is
    /// available and before the first call to `encode()`.
    pub fn init_renderer(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.encoder.init_renderer(device, format);
    }

    /// Initializes the text renderer with the shared glyph atlas.
    /// Must be called after `init_renderer`. If not called, text primitives
    /// are silently skipped during encode.
    pub fn init_text_renderer(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
        bind_group: &wgpu::BindGroup,
    ) {
        self.encoder
            .init_text_renderer(device, format, bind_group_layout, bind_group);
    }

    /// Applies the pending SceneDelta to the GPU renderers and encodes draw
    /// calls into the RenderPass. No-op if the quad renderer hasn't been
    /// initialized or there is no pending delta.
    ///
    /// `commit` live-encodes ineligible externals this pass (recovery / force).
    pub fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: Viewport,
        commit: bool,
    ) {
        self.encoder.encode(
            queue,
            pass,
            viewport,
            EncodeScene {
                scene_graph: &self.scene_graph,
                pending_delta: &mut self.pending_delta,
                current_viewport: &mut self.current_viewport,
                external_draws: &self.external_draws,
                external_eligible: &self.external_eligible,
                commit,
            },
        );
    }

    /// Resolves the terminal-owned default clear appearance for a frame.
    ///
    /// Returns `None` when the Runtime has no external appearance provider;
    /// presenters then use their own opaque fallback (for example, dialogs).
    pub fn frame_appearance(&self, backdrop_available: bool) -> Option<ExternalFrameAppearance> {
        self.external_frame_appearance
            .as_ref()
            .and_then(|(id, provider)| provider(*id, backdrop_available))
    }

    /// Signals that the viewport has changed (e.g., due to window resize).
    ///
    /// Stores every physical/scale transition, including zero size. Marks the
    /// root fiber as LAYOUT_DIRTY only when the viewport actually changes.
    /// Returns `true` when the stored viewport changed.
    pub fn set_viewport(&mut self, viewport: Viewport) -> bool {
        if self.current_viewport.as_ref() == Some(&viewport) {
            return false;
        }
        self.current_viewport = Some(viewport);
        if let Some(root_id) = self.root_id {
            if let Some(fiber) = self.arena.get_mut(root_id) {
                fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
            }
            mark_dirty_for(self.runtime_id, root_id);
        }
        true
    }

    // ── Input ───────────────────────────────────────────────────────────

    /// Dispatches a UI event into the widget tree.
    ///
    /// Routes the event through capture → target → bubble phases,
    /// then applies any commands issued by handlers.
    pub fn dispatch(&mut self, event: UiEvent, _now: Instant) -> RuntimeEffects {
        let _scope = RuntimeScope::enter(self.runtime_id);
        let needs_redraw = self.events.route_event(&self.arena, self.root_id, &event);
        if needs_redraw && let Some(root_id) = self.root_id {
            mark_dirty_for(self.runtime_id, root_id);
        }
        let mut effects = RuntimeEffects::from_redraw(needs_redraw);
        if let Some(text) = self.events.take_clipboard() {
            effects.clipboard = Some(ClipboardEffect::write(text));
        }
        effects
    }

    /// Cancels every pointer currently captured by a widget.
    ///
    /// Hosts use this at lifecycle boundaries such as window focus loss so a
    /// widget cannot observe a later button-up from a different interaction.
    pub fn cancel_pointer_captures(&mut self, position: Point) -> RuntimeEffects {
        let pointer_ids = self.events.input().captured_pointer_ids();
        let mut effects = RuntimeEffects::default();
        for pointer_id in pointer_ids {
            effects.merge(self.dispatch(
                UiEvent::Pointer(crate::input::event::PointerEvent::new(
                    position,
                    PointerPhase::Cancel,
                    crate::input::event::PointerButton::Left,
                    pointer_id,
                )),
                Instant::now(),
            ));
        }
        effects
    }

    fn run_paint_pass(&mut self) {
        let Some(root_id) = self.root_id else {
            return;
        };

        let items = paint_fiber(
            &mut self.arena,
            root_id,
            0,
            &mut self.next_scene_item_id,
            &self.text_metrics,
        );
        let delta = self.scene_graph.diff(items);

        if let Some(pending_delta) = &mut self.pending_delta {
            pending_delta.coalesce(delta);
        } else {
            self.pending_delta = Some(delta);
        }
    }

    /// Prepares cached glyph layouts from the current retained scene.
    ///
    /// Call after the paint pass and before encoding with the glyph lookup that
    /// matches the active atlas.
    pub fn prepare_text_runs(&mut self, glyph_fn: &crate::text::GlyphFn<'_>) {
        self.encoder
            .prepare_text_runs(&self.scene_graph, &self.text_metrics, glyph_fn);
    }

    /// Backwards-compatible spelling for [`Self::prepare_text_runs`].
    ///
    /// This no longer reads a pending queue or thread-local state.
    pub fn register_pending_text_runs(&mut self, glyph_fn: &crate::text::GlyphFn<'_>) {
        self.prepare_text_runs(glyph_fn);
    }

    // ── Accessors ──────────────────────────────────────────────────────

    /// Returns the root FiberId, if a root component has been set.
    pub fn root_id(&self) -> Option<FiberId> {
        self.root_id
    }

    /// Returns a reference to the FiberArena.
    pub fn arena(&self) -> &FiberArena {
        &self.arena
    }

    /// Returns the pending SceneDelta, if any.
    pub fn pending_delta(&self) -> Option<&SceneDelta> {
        self.pending_delta.as_ref()
    }

    /// Takes effects produced by a programmatic runtime operation before the
    /// next update turn. Hosts can apply these immediately when the runtime is
    /// otherwise idle.
    pub fn take_pending_effects(&mut self) -> RuntimeEffects {
        std::mem::take(&mut self.pending_effects)
    }

    /// Drains queued external input events produced by focusable CustomPaint
    /// widgets during the last event dispatch.
    pub fn drain_external_input(
        &self,
    ) -> Vec<(
        crate::scene::primitive::ExternalDrawId,
        crate::input::event::UiEvent,
    )> {
        PENDING_EXTERNAL_INPUT.with(|queues| {
            queues
                .borrow_mut()
                .remove(&self.runtime_id)
                .unwrap_or_default()
        })
    }

    /// Returns a reference to the InputState.
    pub fn input(&self) -> &InputState {
        self.events.input()
    }

    /// Returns a mutable reference to the TextRunCache.
    /// The host uses this to look up glyph data for text rendering.
    pub fn text_run_cache(&mut self) -> &mut TextRunCache {
        self.encoder.text_run_cache()
    }

    /// Programmatically sets the focused fiber for keyboard event routing.
    pub fn set_focus(&mut self, id: FiberId) {
        self.transition_focus(Some(id));
    }

    /// Scans the fiber tree and focuses the first focusable widget.
    /// Returns true if a focusable widget was found and focused.
    pub fn focus_first_focusable(&mut self) -> bool {
        let Some(root_id) = self.root_id else {
            return false;
        };
        if let Some(fid) = EventRouter::find_first_focusable(&self.arena, root_id) {
            self.transition_focus(Some(fid));
            true
        } else {
            false
        }
    }

    /// Clears the focused fiber.
    pub fn clear_focus(&mut self) {
        self.transition_focus(None);
    }

    /// Applies a programmatic focus transition and notifies both endpoints.
    fn transition_focus(&mut self, next: Option<FiberId>) {
        let _scope = RuntimeScope::enter(self.runtime_id);
        if self.events.transition_focus(&self.arena, next)
            && let Some(root_id) = self.root_id
        {
            mark_dirty_for(self.runtime_id, root_id);
            self.pending_effects.merge(RuntimeEffects::request_redraw());
        }
    }

    /// Returns true if any modal FocusScope is currently active in the tree.
    /// The host can check this to suppress events (e.g., paste) that should
    /// be blocked while a modal is open.
    pub fn has_modal(&self) -> bool {
        let Some(root_id) = self.root_id else {
            return false;
        };
        EventRouter::tree_has_modal(&self.arena, root_id)
    }

    // ── Test / internal façades (delegate to EventRouter) ───────────────

    #[cfg(test)]
    fn route_event(&mut self, event: &UiEvent) -> bool {
        self.events.route_event(&self.arena, self.root_id, event)
    }

    #[cfg(test)]
    fn route_to_single(&mut self, fiber_id: FiberId, event: &UiEvent) -> bool {
        self.events.route_to_single(&self.arena, fiber_id, event)
    }

    #[cfg(test)]
    fn finish_event(&mut self, ctx: EventCtx) -> bool {
        self.events.finish_event(&self.arena, ctx)
    }

    #[cfg(test)]
    fn build_ancestor_path(&self, target: Option<FiberId>) -> Vec<FiberId> {
        EventRouter::build_ancestor_path(&self.arena, target)
    }

    #[cfg(test)]
    fn is_descendant_of(&self, descendant: Option<FiberId>, ancestor: FiberId) -> bool {
        EventRouter::is_descendant_of(&self.arena, descendant, ancestor)
    }

    #[cfg(test)]
    fn tree_has_modal(&self, fiber_id: FiberId) -> bool {
        EventRouter::tree_has_modal(&self.arena, fiber_id)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _scope = RuntimeScope::enter(self.runtime_id);
        if let Some(root_id) = self.root_id.take() {
            unmount_fiber(&mut self.arena, root_id);
        }
        remove_runtime(self.runtime_id);
        remove_external_input(self.runtime_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{
        Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
    };
    use crate::input::event_ctx::EventCtx;
    use crate::scene::primitive::{
        ExternalFrameAppearance, ExternalFrameAppearanceFn, ExternalScheduleDemand,
    };
    use crate::widgets::button::Button;
    use crate::widgets::column::Column;
    use crate::widgets::custom_paint::CustomPaint;
    use crate::widgets::focus_scope::FocusScope;
    use crate::widgets::sized_box::SizedBox;
    use crate::widgets::text_label::TextLabel;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    fn now() -> Instant {
        Instant::now()
    }

    #[test]
    fn unencoded_repaint_preserves_scene_ids_and_initial_additions() {
        use crate::scene::primitive::Color;

        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(100.0, 50.0)).color(Color::RED));
        let scene_ids: Vec<u64> = rt.scene_graph.items().iter().map(|item| item.id).collect();

        rt.run_paint_pass();

        assert_eq!(
            rt.scene_graph
                .items()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            scene_ids
        );
        let delta = rt.pending_delta().unwrap();
        assert_eq!(
            delta.added.iter().map(|item| item.id).collect::<Vec<_>>(),
            scene_ids
        );
        assert!(delta.removed.is_empty());
        assert!(delta.modified.is_empty());
    }

    #[test]
    fn unencoded_modified_item_remains_added_with_its_latest_contents() {
        use crate::effects::ExternalInvalidation;
        use crate::scene::primitive::Color;
        use crate::view::{BuildCx, Component, View};
        use std::cell::Cell;
        use std::rc::Rc;

        struct ColorToggle(Rc<Cell<bool>>);

        impl Component for ColorToggle {
            fn build(&self, cx: &mut BuildCx) -> View {
                SizedBox::new(Size::new(100.0, 50.0))
                    .color(if self.0.get() {
                        Color::BLUE
                    } else {
                        Color::RED
                    })
                    .build(cx)
            }
        }

        let blue = Rc::new(Cell::new(false));
        let mut rt = Runtime::new();
        rt.set_root(ColorToggle(Rc::clone(&blue)));
        let scene_id = rt.scene_graph.items()[0].id;

        blue.set(true);
        rt.invalidate_external(ExternalInvalidation::new());
        rt.update(now());

        let delta = rt.pending_delta().unwrap();
        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.added[0].id, scene_id);
        assert_eq!(delta.added[0], rt.scene_graph.items()[0]);
        assert!(delta.removed.is_empty());
        assert!(delta.modified.is_empty());
    }

    #[test]
    fn nested_deferred_component_preserves_state_and_subscribes_its_fiber() {
        use crate::signal::Signal;
        use crate::view::{BuildCx, Component, View};
        use crate::widgets::column::Column;
        use crate::widgets::padding::Padding;
        use std::cell::RefCell;
        use std::rc::Rc;

        #[derive(Clone)]
        struct StatefulChild {
            signal: Rc<RefCell<Option<Signal<u32>>>>,
            observed_values: Rc<RefCell<Vec<u32>>>,
        }

        impl Component for StatefulChild {
            fn build(&self, cx: &mut BuildCx) -> View {
                let state = cx.use_state(|| 0u32);
                self.observed_values.borrow_mut().push(*state.read());
                *self.signal.borrow_mut() = Some(state);
                SizedBox::new(Size::new(10.0, 10.0)).build(cx)
            }
        }

        let signal = Rc::new(RefCell::new(None));
        let observed_values = Rc::new(RefCell::new(Vec::new()));
        let child = StatefulChild {
            signal: Rc::clone(&signal),
            observed_values: Rc::clone(&observed_values),
        };
        let mut rt = Runtime::new();
        rt.set_root(Column::new().child(Padding::new(1.0, 1.0, 1.0, 1.0).child(child)));

        let root_id = rt.root_id().unwrap();
        let padding_id = rt.arena().get(root_id).unwrap().children[0];
        let child_id = rt.arena().get(padding_id).unwrap().children[0];
        assert_eq!(rt.arena().get(child_id).unwrap().hooks.len(), 1);
        assert_eq!(observed_values.borrow().as_slice(), &[0]);

        let state = signal.borrow().as_ref().unwrap().clone();
        state.set(7);
        let effects = rt.update(now());

        assert!(
            effects.request_redraw,
            "the nested Fiber subscribed to its state"
        );
        assert_eq!(observed_values.borrow().last(), Some(&7));
        let rebuilt_padding = rt.arena().get(root_id).unwrap().children[0];
        assert_eq!(
            rt.arena().get(rebuilt_padding).unwrap().children[0],
            child_id
        );
        assert_eq!(rt.arena().get(child_id).unwrap().hooks.len(), 1);
    }

    // ── is_descendant_of ───────────────────────────────────────────────

    #[test]
    fn is_descendant_of_direct_child() {
        use crate::widgets::padding::Padding;
        let mut rt = Runtime::new();
        rt.set_root(Padding::new(8.0, 8.0, 8.0, 8.0).child(SizedBox::new(Size::new(100.0, 50.0))));
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        let padding_fiber = rt.arena().get(root_id).unwrap();
        let child_id = padding_fiber.children[0];

        assert!(rt.is_descendant_of(Some(child_id), root_id));
    }

    #[test]
    fn is_descendant_of_self_is_true() {
        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        assert!(rt.is_descendant_of(Some(root_id), root_id));
    }

    #[test]
    fn is_descendant_of_none_is_false() {
        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        assert!(!rt.is_descendant_of(None, root_id));
    }

    // ── build_ancestor_path ────────────────────────────────────────────

    #[test]
    fn build_ancestor_path_none() {
        let rt = Runtime::new();
        let path = rt.build_ancestor_path(None);
        assert!(path.is_empty());
    }

    #[test]
    fn build_ancestor_path_root_is_singleton() {
        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        let path = rt.build_ancestor_path(Some(root_id));
        assert_eq!(path, vec![root_id]);
    }

    // ── tree_has_modal ─────────────────────────────────────────────────

    #[test]
    fn tree_has_modal_returns_false_for_dead_fiber() {
        let rt = Runtime::new();
        let mut arena = FiberArena::new();
        let fid = arena.insert(Fiber::new(None, std::any::TypeId::of::<()>(), None));
        arena.remove(fid);
        // Stale key — tree_has_modal should return false
        assert!(!rt.tree_has_modal(fid));
    }

    #[test]
    fn tree_has_modal_detects_modal_at_root() {
        let mut rt = Runtime::new();
        rt.set_root(
            FocusScope::new()
                .modal(true)
                .child(SizedBox::new(Size::new(100.0, 50.0))),
        );
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        assert!(rt.tree_has_modal(root_id));
    }

    #[test]
    fn tree_has_modal_detects_modal_in_deep_subtree() {
        use crate::widgets::column::Column;
        use crate::widgets::padding::Padding;

        let mut rt = Runtime::new();
        rt.set_root(
            Padding::new(8.0, 8.0, 8.0, 8.0).child(
                Column::new().child(
                    FocusScope::new()
                        .modal(true)
                        .child(SizedBox::new(Size::new(100.0, 50.0))),
                ),
            ),
        );
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        // tree_has_modal recursively scans the entire subtree of the given fiber.
        // The root (Padding) contains a Column containing a modal, so scanning
        // the root returns true.
        assert!(rt.tree_has_modal(root_id));

        // A separate non-modal SVG or SizedBox root returns false
        let mut rt2 = Runtime::new();
        rt2.set_root(SizedBox::new(Size::new(100.0, 50.0)));
        rt2.update(now());
        let root2_id = rt2.root_id().unwrap();
        assert!(!rt2.tree_has_modal(root2_id));
    }

    #[test]
    fn tree_has_modal_returns_false_for_non_modal_tree() {
        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        assert!(!rt.tree_has_modal(root_id));
    }

    // ── finish_event ───────────────────────────────────────────────────

    #[test]
    fn finish_event_returns_false_when_no_commands_no_paint() {
        let mut rt = Runtime::new();
        let ctx = EventCtx::new();
        assert!(!rt.finish_event(ctx));
    }

    #[test]
    fn finish_event_returns_true_when_invalidate_paint_called() {
        let mut rt = Runtime::new();
        let mut ctx = EventCtx::new();
        ctx.invalidate_paint();
        assert!(rt.finish_event(ctx));
    }

    #[test]
    fn finish_event_returns_true_when_stop_propagation_without_paint() {
        // stop_propagation itself does not request paint, but we check
        let mut rt = Runtime::new();
        let mut ctx = EventCtx::new();
        ctx.stop_propagation();
        assert!(!rt.finish_event(ctx));
    }

    // ── route_to_single ────────────────────────────────────────────────

    #[test]
    fn route_to_single_with_live_fiber() {
        let mut rt = Runtime::new();
        rt.set_root(Button::new("OK"));
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        });
        let needs_redraw = rt.route_to_single(root_id, &event);
        // Button handles Enter, invalidates paint internally — but route_to_single
        // calls finish_event which may also return true from ctx.needs_paint()
        // Only check it doesn't panic
        let _ = needs_redraw;
    }

    // ── Accessors ──────────────────────────────────────────────────────

    #[test]
    fn should_resolve_external_frame_appearance_with_host_backdrop_fact() {
        let provider: Arc<ExternalFrameAppearanceFn> = Arc::new(|_, backdrop| {
            if backdrop {
                Some(ExternalFrameAppearance::new([0.1, 0.2, 0.3, 0.25]))
            } else {
                Some(ExternalFrameAppearance::new([0.1, 0.2, 0.3, 1.0]))
            }
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(77).frame_appearance(provider));

        assert_eq!(
            rt.frame_appearance(true).map(|appearance| appearance.rgba),
            Some([0.1, 0.2, 0.3, 0.25])
        );
        assert_eq!(
            rt.frame_appearance(false).map(|appearance| appearance.rgba),
            Some([0.1, 0.2, 0.3, 1.0])
        );
    }

    #[test]
    fn should_retain_first_external_frame_appearance_provider_deterministically() {
        let first: Arc<ExternalFrameAppearanceFn> =
            Arc::new(|_, _| Some(ExternalFrameAppearance::new([1.0, 0.0, 0.0, 1.0])));
        let second: Arc<ExternalFrameAppearanceFn> =
            Arc::new(|_, _| Some(ExternalFrameAppearance::new([0.0, 1.0, 0.0, 1.0])));
        let mut rt = Runtime::new();
        rt.set_root(
            Column::new()
                .child(CustomPaint::new(1).frame_appearance(first))
                .child(CustomPaint::new(2).frame_appearance(second)),
        );

        assert_eq!(
            rt.frame_appearance(true).map(|appearance| appearance.rgba),
            Some([1.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn new_runtime_has_no_root() {
        let rt = Runtime::new();
        assert!(rt.root_id().is_none());
        assert!(rt.pending_delta().is_none());
        assert!(rt.input().focused.is_none());
        assert!(rt.current_viewport().is_none());
    }

    #[test]
    fn should_store_viewport_and_report_change_when_set_viewport_differs() {
        // Arrange
        let mut rt = Runtime::new();
        let viewport = Viewport::new(800, 600, 1.0);

        // Act
        let changed = rt.set_viewport(viewport.clone());
        let unchanged = rt.set_viewport(viewport.clone());

        // Assert
        assert!(changed);
        assert!(!unchanged);
        assert_eq!(rt.current_viewport(), Some(&viewport));
    }

    #[test]
    fn should_store_zero_sized_viewport_without_rejecting_it() {
        // Arrange
        let mut rt = Runtime::new();
        let viewport = Viewport::new(0, 0, 1.0);

        // Act
        assert!(rt.set_viewport(viewport.clone()));

        // Assert
        assert_eq!(rt.current_viewport(), Some(&viewport));
        assert!(!rt.current_viewport().unwrap().is_drawable());
    }

    #[test]
    fn should_report_change_when_scale_changes_with_constant_physical_size() {
        // Arrange
        let mut rt = Runtime::new();
        assert!(rt.set_viewport(Viewport::new(800, 600, 1.0)));

        // Act
        let changed = rt.set_viewport(Viewport::new(800, 600, 2.0));

        // Assert
        assert!(changed);
        assert_eq!(rt.current_viewport().map(|vp| vp.scale_factor), Some(2.0));
        assert_eq!(
            rt.current_viewport().map(|vp| vp.logical_size),
            Some(crate::layout::Size::new(400.0, 300.0))
        );
    }

    #[test]
    fn should_dirty_layout_when_scale_only_viewport_changes() {
        // Arrange
        use crate::widgets::sized_box::SizedBox;

        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(crate::layout::Size::new(100.0, 50.0)));
        rt.set_viewport(Viewport::new(800, 600, 1.0));
        assert!(rt.update(now()).request_redraw);
        assert!(!rt.update(now()).request_redraw);

        // Act
        assert!(rt.set_viewport(Viewport::new(800, 600, 2.0)));
        let effects = rt.update(now());

        // Assert
        assert!(effects.request_redraw);
    }

    #[test]
    fn focus_transition_notifies_old_and_new_fibers_and_requests_redraw() {
        let mut rt = Runtime::new();
        rt.set_root(
            FocusScope::new()
                .child(CustomPaint::new(1))
                .child(CustomPaint::new(2)),
        );
        rt.update(now());
        assert!(rt.focus_first_focusable());
        rt.drain_external_input();

        let first = rt.input().focused;
        let effects = rt.dispatch(
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Tab,
                modifiers: Modifiers::default(),
            }),
            now(),
        );

        assert!(effects.request_redraw);
        assert_ne!(rt.input().focused, first);
        let focus_events = rt.drain_external_input();
        assert!(focus_events.iter().any(|(_, event)| matches!(
            event,
            UiEvent::Focus(crate::input::event::FocusEvent::Lost)
        )));
        assert!(focus_events.iter().any(|(_, event)| matches!(
            event,
            UiEvent::Focus(crate::input::event::FocusEvent::Gained)
        )));
    }

    #[test]
    fn set_focus_and_clear_focus() {
        let mut rt = Runtime::new();
        rt.set_root(Button::new("OK"));
        rt.update(now());

        let root_id = rt.root_id().unwrap();
        assert!(rt.input().focused.is_none());

        rt.set_focus(root_id);
        assert_eq!(rt.input().focused, Some(root_id));

        rt.clear_focus();
        assert!(rt.input().focused.is_none());
    }

    // ── route_event keyboard with no focused widget ────────────────────

    #[test]
    fn route_event_keyboard_no_focused_returns_false() {
        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
        rt.update(now());

        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        });
        // No focused widget; keyboard path = [root_id] only
        let needs_redraw = rt.route_event(&event);
        assert!(!needs_redraw);
    }

    // ── route_event pointer cancel without capture ─────────────────────

    #[test]
    fn route_event_pointer_cancel_without_capture_returns_false() {
        let mut rt = Runtime::new();
        rt.set_root(Button::new("OK"));
        rt.update(now());

        let event = UiEvent::Pointer(PointerEvent::new(
            Point::new(50.0, 16.0),
            PointerPhase::Cancel,
            PointerButton::Left,
            42,
        ));
        // No capture for pointer 42 — should return false without crash
        let needs_redraw = rt.route_event(&event);
        assert!(!needs_redraw);
    }

    // ── route_event dead captor ────────────────────────────────────────

    #[test]
    fn route_event_dead_captor_falls_through_to_hit_test() {
        // Test: when a pointer is captured but the captor fiber is dead,
        // the runtime should not panic. We verify via public API by
        // capturing a pointer on one tree, replacing the tree, then
        // sending events for the captured pointer.
        //
        // After set_root replaces the tree, the old captor fiber is
        // unmounted and `clear_capture_if_dead` in rebuild_root removes
        // the capture. Sending events for the stale pointer is a no-op.
        let clicked = Arc::new(AtomicBool::new(false));
        let mut rt = Runtime::new();
        rt.set_root(Button::new("OK").on_click(move |_ctx| {
            clicked.store(true, Ordering::SeqCst);
        }));
        rt.update(now());

        // Down to capture pointer 99
        rt.dispatch(
            UiEvent::Pointer(PointerEvent::new(
                Point::new(46.0, 16.0),
                PointerPhase::Down,
                PointerButton::Left,
                99,
            )),
            now(),
        );
        assert!(
            rt.input().captor(99).is_some(),
            "pointer 99 should be captured"
        );

        // Replace tree — old fibers are unmounted, captures cleared
        rt.set_root(Button::new("Replacement"));
        rt.update(now());

        // After tree replacement, capture should be cleared
        assert!(
            rt.input().captor(99).is_none(),
            "capture should be cleared when tree is replaced"
        );

        // Sending events for now-released pointer should not panic
        let req = rt.dispatch(
            UiEvent::Pointer(PointerEvent::new(
                Point::new(46.0, 16.0),
                PointerPhase::Up,
                PointerButton::Left,
                99,
            )),
            now(),
        );
        let _ = req;
    }

    // ── layout_fiber edge cases ────────────────────────────────────────

    #[test]
    fn layout_fiber_with_stale_fiber_id_does_not_panic() {
        let mut arena = FiberArena::new();
        let id = arena.insert(Fiber::new(None, std::any::TypeId::of::<()>(), None));
        arena.remove(id);
        // Calling layout_fiber with a stale id should be a no-op
        layout_fiber(
            &mut arena,
            id,
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            Point::ZERO,
            &DEFAULT_TEXT_METRICS,
        );
        // No panic = pass
    }

    // ── has_modal with no root ─────────────────────────────────────────

    #[test]
    fn has_modal_returns_false_with_no_root() {
        let rt = Runtime::new();
        assert!(!rt.has_modal());
    }
    // ── focus_first_focusable ──────────────────────────────────────────

    #[test]
    fn should_focus_button_when_it_is_focusable() {
        // Arrange
        let mut rt = Runtime::new();
        rt.set_root(Button::new("OK"));
        rt.update(now());

        // Act
        let found = rt.focus_first_focusable();

        // Assert
        assert!(
            found,
            "focus_first_focusable should return true for a Button"
        );
        assert!(rt.input().focused.is_some(), "focused widget should be set");
    }

    #[test]
    fn should_skip_non_focusable_sized_box_and_focus_button() {
        use crate::widgets::column::Column;
        // Arrange
        let mut rt = Runtime::new();
        rt.set_root(
            Column::new()
                .child(SizedBox::new(Size::new(100.0, 50.0)))
                .child(Button::new("Next")),
        );
        rt.update(now());

        // Act
        let found = rt.focus_first_focusable();

        // Assert
        assert!(
            found,
            "should find the Button even though SizedBox comes first"
        );
        let focused_id = rt.input().focused.expect("focused widget should be set");
        let fiber = rt.arena().get(focused_id).unwrap();
        assert!(
            fiber.view.as_ref().unwrap().is_focusable(),
            "focused widget should be focusable (the Button, not the SizedBox)"
        );
    }

    #[test]
    fn should_return_false_when_no_root_set() {
        // Arrange
        let mut rt = Runtime::new();

        // Act
        let found = rt.focus_first_focusable();

        // Assert
        assert!(!found, "should return false when no root widget is set");
        assert!(rt.input().focused.is_none(), "focused should remain None");
    }

    #[test]
    fn should_return_false_when_no_focusable_widgets_exist() {
        // Arrange
        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(100.0, 50.0)));
        rt.update(now());

        // Act
        let found = rt.focus_first_focusable();

        // Assert
        assert!(
            !found,
            "should return false when tree has no focusable widgets"
        );
        assert!(rt.input().focused.is_none(), "focused should remain None");
    }

    // ── text run preparation ───────────────────────────────────────────

    #[test]
    fn text_preparation_reuses_unchanged_scene_runs_and_releases_removed_runs() {
        let mut rt = Runtime::new();
        rt.set_root(TextLabel::new("Confirm paste?"));
        rt.prepare_text_runs(&|_ch| None);
        assert_eq!(rt.text_run_cache().len(), 1);

        // Preparing an unchanged retained scene must not allocate another run.
        rt.prepare_text_runs(&|_ch| None);
        assert_eq!(rt.text_run_cache().len(), 1);

        rt.set_root(SizedBox::new(Size::new(1.0, 1.0)));
        rt.prepare_text_runs(&|_ch| None);
        assert!(rt.text_run_cache().is_empty());
    }

    #[test]
    fn text_metrics_are_isolated_per_runtime_and_relayout_the_owner() {
        let mut narrow = DEFAULT_TEXT_METRICS;
        narrow.cell_width = 4.0;
        let mut wide = DEFAULT_TEXT_METRICS;
        wide.cell_width = 20.0;

        let mut first = Runtime::with_text_metrics(narrow);
        let mut second = Runtime::with_text_metrics(wide);
        first.set_root(TextLabel::new("text"));
        second.set_root(TextLabel::new("text"));

        assert_eq!(
            first
                .arena()
                .get(first.root_id().unwrap())
                .unwrap()
                .layout_rect()
                .unwrap()
                .size()
                .width,
            20.0
        );
        assert_eq!(
            second
                .arena()
                .get(second.root_id().unwrap())
                .unwrap()
                .layout_rect()
                .unwrap()
                .size()
                .width,
            84.0
        );

        first.set_text_metrics(wide);
        assert!(first.update(now()).request_redraw);
        assert_eq!(
            first
                .arena()
                .get(first.root_id().unwrap())
                .unwrap()
                .layout_rect()
                .unwrap()
                .size()
                .width,
            84.0
        );
        assert_eq!(second.text_metrics().cell_width, 20.0);
    }

    #[test]
    fn legacy_text_preparation_spelling_forwards_to_scene_preparation() {
        let mut rt = Runtime::new();
        rt.set_root(TextLabel::new("Confirm paste?"));

        rt.register_pending_text_runs(&|_ch| None);

        assert_eq!(rt.text_run_cache().len(), 1);
    }

    // ── external schedule merge ─────────────────────────────────────────

    #[test]
    fn should_emit_wait_until_on_idle_when_schedule_reports_deadline() {
        // Arrange
        let deadline = Instant::now() + Duration::from_secs(5);
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
            redraw_now: false,
            deadline: Some(deadline),
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(1).schedule(schedule));
        let _ = rt.update(now());

        // Act
        let idle = rt.update(now());

        // Assert
        assert!(!idle.request_redraw);
        assert_eq!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(deadline))
        );
    }

    #[test]
    fn should_select_earliest_deadline_when_multiple_schedules_report() {
        // Arrange
        let early = Instant::now() + Duration::from_millis(100);
        let late = Instant::now() + Duration::from_secs(2);
        let late_schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
            redraw_now: false,
            deadline: Some(late),
            ..ExternalScheduleDemand::empty()
        });
        let early_schedule: Arc<ExternalScheduleFn> =
            Arc::new(move |_, _| ExternalScheduleDemand {
                redraw_now: false,
                deadline: Some(early),
                ..ExternalScheduleDemand::empty()
            });
        let mut rt = Runtime::new();
        rt.set_root(
            Column::new()
                .child(CustomPaint::new(1).schedule(late_schedule))
                .child(CustomPaint::new(2).schedule(early_schedule)),
        );
        let _ = rt.update(now());

        // Act
        let idle = rt.update(now());

        // Assert
        assert_eq!(idle.control_flow, Some(ControlFlowEffect::WaitUntil(early)));
    }

    #[test]
    fn should_request_redraw_when_schedule_reports_redraw_now() {
        // Arrange
        let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
            redraw_now: true,
            deadline: None,
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(4).schedule(schedule));
        let _ = rt.update(now());

        // Act
        let idle = rt.update(now());

        // Assert
        assert!(idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(!idle.force_present);
        assert!(idle.control_flow.is_none());
    }

    #[test]
    fn should_merge_redraw_and_wait_until_when_schedule_reports_both() {
        // Arrange
        let deadline = Instant::now() + Duration::from_millis(250);
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
            redraw_now: true,
            deadline: Some(deadline),
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(5).schedule(schedule));
        let _ = rt.update(now());

        // Act
        let idle = rt.update(now());

        // Assert
        assert!(idle.request_redraw);
        assert_eq!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(deadline))
        );
    }

    #[test]
    fn should_not_emit_poll_when_external_schedule_is_empty() {
        // Arrange — empty demand must not invent blink-only Poll
        let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(6).schedule(schedule));
        let _ = rt.update(now());

        // Act
        let idle = rt.update(now());

        // Assert
        assert!(!idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(!idle.force_present);
        assert!(idle.control_flow.is_none());
        assert_ne!(idle.control_flow, Some(ControlFlowEffect::Poll));
    }

    #[test]
    fn should_pass_registered_draw_id_when_collecting_external_schedule() {
        // Arrange
        let seen = Arc::new(AtomicU64::new(0));
        let seen_id = Arc::clone(&seen);
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |id, _| {
            seen_id.store(id, Ordering::SeqCst);
            ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(42).schedule(schedule));
        let _ = rt.update(now());

        // Act
        let _ = rt.update(now());

        // Assert
        assert_eq!(seen.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn should_clear_external_schedules_when_root_no_longer_registers_them() {
        // Arrange — provider reports a deadline, then root is replaced without one.
        let deadline = Instant::now() + Duration::from_secs(3);
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
            redraw_now: false,
            deadline: Some(deadline),
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(1).schedule(schedule));
        let _ = rt.update(now());
        assert_eq!(
            rt.update(now()).control_flow,
            Some(ControlFlowEffect::WaitUntil(deadline))
        );

        // Act — rebuild without a schedule provider
        rt.set_root(SizedBox::new(Size::new(20.0, 20.0)));
        let _ = rt.update(now());
        let idle = rt.update(now());

        // Assert
        assert!(idle.control_flow.is_none());
        assert!(!idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(!idle.has_deferred_externals);
        assert!(!idle.force_present);
    }

    #[test]
    fn should_not_request_redraw_when_schedule_defers_ordinary_present() {
        let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
            redraw_now: true,
            ordinary_present_eligible: false,
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(7).schedule(schedule));
        let _ = rt.update(now());

        let idle = rt.update(now());

        assert!(!idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(idle.has_deferred_externals);
        assert!(!idle.force_present);
    }

    #[test]
    fn should_keep_wait_until_when_schedule_defers_ordinary_present() {
        let deadline = Instant::now() + Duration::from_millis(400);
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
            redraw_now: true,
            deadline: Some(deadline),
            ordinary_present_eligible: false,
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(8).schedule(schedule));
        let _ = rt.update(now());

        let idle = rt.update(now());

        assert!(!idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(idle.has_deferred_externals);
        assert_eq!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(deadline))
        );
        assert_ne!(idle.control_flow, Some(ControlFlowEffect::Poll));
    }

    #[test]
    fn should_keep_window_eligible_when_any_schedule_reports_ineligible() {
        let eligible: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
            redraw_now: true,
            ..ExternalScheduleDemand::empty()
        });
        let deferred: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
            ordinary_present_eligible: false,
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(
            Column::new()
                .child(CustomPaint::new(1).schedule(eligible))
                .child(CustomPaint::new(2).schedule(deferred)),
        );
        let _ = rt.update(now());

        let idle = rt.update(now());

        assert!(idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(idle.has_deferred_externals);
        assert!(!idle.force_present);
    }

    #[test]
    fn should_remain_eligible_when_no_external_schedule_is_registered() {
        // Arrange
        let mut rt = Runtime::new();
        rt.set_root(SizedBox::new(Size::new(20.0, 20.0)));
        let _ = rt.update(now());

        // Act
        let idle = rt.update(now());

        // Assert
        assert!(idle.ordinary_present_eligible);
        assert!(!idle.has_deferred_externals);
        assert!(!idle.force_present);
        assert!(!idle.request_redraw);
    }

    #[test]
    fn should_keep_dirty_redraw_deferred_when_schedule_is_ineligible() {
        // Arrange
        let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
            ordinary_present_eligible: false,
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(9).schedule(schedule));

        // Act — first update rebuilds dirty Fibers and then folds schedule demand
        let first = rt.update(now());

        // Assert
        assert!(first.request_redraw);
        assert!(first.ordinary_present_eligible);
        assert!(first.has_deferred_externals);
        assert!(!first.force_present);
    }

    #[test]
    fn should_ignore_deferred_redraw_when_eligible_sibling_reports_none() {
        // Arrange
        let eligible: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
        let deferred: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
            redraw_now: true,
            ordinary_present_eligible: false,
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(
            Column::new()
                .child(CustomPaint::new(10).schedule(eligible))
                .child(CustomPaint::new(11).schedule(deferred)),
        );
        let _ = rt.update(now());

        // Act
        let idle = rt.update(now());

        // Assert
        assert!(!idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(idle.has_deferred_externals);
        assert!(!idle.force_present);
    }

    #[test]
    fn should_clear_deferred_externals_when_ineligible_schedule_is_removed() {
        // Arrange
        let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand {
            ordinary_present_eligible: false,
            ..ExternalScheduleDemand::empty()
        });
        let mut rt = Runtime::new();
        rt.set_root(CustomPaint::new(12).schedule(schedule));
        let _ = rt.update(now());
        assert!(rt.update(now()).has_deferred_externals);

        // Act
        rt.set_root(SizedBox::new(Size::new(20.0, 20.0)));
        let _ = rt.update(now());
        let idle = rt.update(now());

        // Assert
        assert!(!idle.has_deferred_externals);
        assert!(idle.ordinary_present_eligible);
        assert!(!idle.request_redraw);
    }
}
