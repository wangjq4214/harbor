mod event_router;
mod frame_encoder;
#[cfg(test)]
mod portal_tests;
#[cfg(test)]
mod text_input_blink_tests;

use crate::construction::IntoChildView;
use crate::effects::{
    ClipboardEffect, ControlFlowEffect, CursorEffect, ExternalInvalidation, ImeEffect,
    RuntimeEffects,
};
use crate::fiber::{
    DirtyFlags, Fiber, FiberArena, FiberId, LayoutOutcome, layout_fiber, paint_fiber,
    reconcile_children_with_externals, unmount_fiber,
};
use crate::input::event::{PointerPhase, UiEvent};
#[cfg(test)]
use crate::input::event_ctx::EventCtx;
use crate::input::state::InputState;
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::renderer::Viewport;
use crate::runtime::event_router::EventRouter;
use crate::runtime::frame_encoder::{EncodeScene, FrameEncoder};
use crate::scene::primitive::{
    ExternalDrawFn, ExternalDrawGpu, ExternalDrawId, ExternalImeFn, ExternalScheduleFn,
};
use crate::scene::{SceneDelta, SceneGraph};
use crate::signal::{RuntimeId, RuntimeScope, mark_dirty_for, remove_runtime, take_dirty};
#[cfg(test)]
use crate::text::TextRunCache;
use crate::text::{TextMetrics, text_metrics_equal};
use crate::theme::Theme;
use crate::view::{BuildCx, ExternalRegistrations, View, materialize_for_fiber};
use hashbrown::HashMap;
use std::sync::Arc;
use std::time::Instant;

// ── Runtime ─────────────────────────────────────────────────────────────────

struct PendingLayoutNotification {
    fiber: FiberId,
    rect: Rect,
    callback: Arc<dyn Fn(Rect) + 'static>,
}

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
/// encoding are delegated to internal `EventRouter` and `FrameEncoder` helpers.
pub struct Runtime {
    runtime_id: RuntimeId,
    arena: FiberArena,
    root_id: Option<FiberId>,
    root_view: Option<View>,
    text_metrics: TextMetrics,
    scene_graph: SceneGraph,
    next_scene_item_id: u64,
    pending_delta: Option<SceneDelta>,
    current_viewport: Option<Viewport>,
    external_draws: HashMap<ExternalDrawId, Arc<ExternalDrawFn<'static>>>,
    external_schedules: HashMap<ExternalDrawId, Arc<ExternalScheduleFn>>,
    external_ime: HashMap<ExternalDrawId, Arc<ExternalImeFn>>,
    external_ime_active: bool,
    /// Tracks per-draw present eligibility for synchronized output.
    external_eligible: HashMap<ExternalDrawId, bool>,
    events: EventRouter,
    encoder: FrameEncoder,
    pending_effects: RuntimeEffects,
    pending_layout_notifications: Vec<PendingLayoutNotification>,
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
            root_view: None,
            text_metrics,
            scene_graph: SceneGraph::new(),
            next_scene_item_id: 1,
            pending_delta: None,
            current_viewport: None,
            external_draws: HashMap::new(),
            external_schedules: HashMap::new(),
            external_ime: HashMap::new(),
            external_ime_active: false,
            external_eligible: HashMap::new(),
            events: EventRouter::new(),
            encoder: FrameEncoder::new(),
            pending_effects: RuntimeEffects::default(),
            pending_layout_notifications: Vec::new(),
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
        self.invalidate_layout();
    }

    /// Drops the mounted tree and resets all state derived from it.
    pub(crate) fn clear_root(&mut self) {
        let _scope = RuntimeScope::enter(self.runtime_id);

        // Detach every old-generation owner before invoking unmount hooks. If a
        // hook panics, unwinding drops this detached closure environment while the
        // Runtime itself already contains only generation-neutral state.
        let old_root = self.root_id.take();
        let old_root_view = self.root_view.take();
        let mut old_arena = std::mem::take(&mut self.arena);
        let old_external_draws = std::mem::take(&mut self.external_draws);
        let old_external_schedules = std::mem::take(&mut self.external_schedules);
        let old_external_ime = std::mem::take(&mut self.external_ime);
        self.external_ime_active = false;
        let old_external_eligible = std::mem::take(&mut self.external_eligible);
        let old_layout_notifications = std::mem::take(&mut self.pending_layout_notifications);
        let mut next_events = EventRouter::new();
        next_events.retire_ime_from(&self.events);
        let old_events = std::mem::replace(&mut self.events, next_events);
        remove_runtime(self.runtime_id);
        self.encoder.release_text_owner(self.runtime_id);

        let removal = self.scene_graph.diff(Vec::new());
        if let Some(pending_delta) = &mut self.pending_delta {
            pending_delta.coalesce(removal);
        } else {
            self.pending_delta = Some(removal);
        }

        self.pending_effects = RuntimeEffects {
            request_redraw: true,
            cursor: Some(CursorEffect::reset()),
            ime: Some(ImeEffect::set_allowed(false)),
            ..RuntimeEffects::default()
        };

        let unmount_panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(root_id) = old_root {
                unmount_fiber(&mut old_arena, root_id);
            }
        }))
        .is_err();
        // Remove any fibers left behind by a panicking unmount hook, then retain
        // the emptied arena so its slot generations continue rejecting stale IDs.
        old_arena.clear();
        self.arena = old_arena;

        let drop_panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            drop((
                old_root_view,
                old_external_draws,
                old_external_schedules,
                old_external_ime,
                old_external_eligible,
                old_layout_notifications,
                old_events,
            ));
        }))
        .is_err();
        if unmount_panicked || drop_panicked {
            tracing::error!("widget root teardown callback panicked after state was detached");
        }
    }

    /// Sets the root view description and performs the initial build + layout.
    ///
    /// Accepts any value converting through `IntoChildView`: a `Component` or an
    /// already-built `View` (as `view!` produces). If a previous root existed,
    /// it is torn down before the replacement is built. The description is not
    /// built here; the placeholder root Fiber, owned by the Runtime, materializes
    /// it during `rebuild_root`.
    pub fn set_root(&mut self, root: impl IntoChildView + 'static) {
        if self.root_id.is_some() || self.root_view.is_some() {
            let pending_focus_handle = self.events.pending_focus_handle();
            self.clear_root();
            self.events
                .restore_pending_focus_handle(pending_focus_handle);
        }
        let _scope = RuntimeScope::enter(self.runtime_id);

        // Create a temporary root fiber
        let root_fiber = Fiber::new(
            None,
            std::any::TypeId::of::<()>(), // placeholder, updated below
            None,
        );
        let root_id = self.arena.insert(root_fiber);
        self.root_id = Some(root_id);

        self.root_view = Some(root.into_child_view());

        // Full rebuild from root with empty old children
        self.rebuild_root(root_id, &[]);

        // The first update presents this tree: arm the redraw edge without
        // rebuilding the identical tree a second time.
        self.pending_effects.merge(RuntimeEffects::request_redraw());
    }

    /// Processes dirty fibers and runs layout.
    ///
    /// Returns the platform-neutral effects produced by this update.
    /// Always folds registered external schedule demands, including clean idle
    /// turns, so hosts can wait until the next deadline without polling.
    pub fn update(&mut self, now: Instant) -> RuntimeEffects {
        let mut effects = std::mem::take(&mut self.pending_effects);
        effects.merge(self.flush_pending_work());
        if let Some(cursor) = self.events.take_cursor() {
            effects.cursor = Some(cursor);
        }
        effects.merge(self.collect_external_schedule(now));
        let animation = self.collect_focused_animation(now);
        let control_flow = match (effects.control_flow, animation.control_flow) {
            (Some(left), Some(right)) => Some(left.arbitrate(right)),
            (left, right) => left.or(right),
        };
        effects.merge(animation);
        effects.control_flow = control_flow;
        effects.merge(self.collect_focused_external_ime());
        effects
    }
    #[cfg(test)]
    pub(crate) fn external_schedule_ids(&self) -> Vec<ExternalDrawId> {
        self.external_schedules.keys().copied().collect()
    }

    /// Layout invalidation is separate from the signal/external build queue:
    /// resizing or replacing metrics must not evaluate Components or registrations.
    fn invalidate_layout(&mut self) {
        if let Some(root_id) = self.root_id
            && let Some(fiber) = self.arena.get_mut(root_id)
        {
            fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
        }
    }

    /// Commits geometry before either painting or routing subsequent input.
    /// Schedule providers are deliberately only polled by `update`, not here.
    fn flush_pending_work(&mut self) -> RuntimeEffects {
        let dirty = take_dirty(self.runtime_id);
        let mut did_tree_work = false;
        if let Some(root_id) = self.root_id {
            if !dirty.is_empty() {
                // Signals only enqueue fibers (without setting flags). Rebuild the
                // full root conservatively: changed content/factors affect siblings.
                let old_children = self
                    .arena
                    .get(root_id)
                    .map(|fiber| fiber.children.clone())
                    .unwrap_or_default();
                self.rebuild_root(root_id, &old_children);
                did_tree_work = true;
            } else if self
                .arena
                .get(root_id)
                .is_some_and(|fiber| fiber.flags.contains(DirtyFlags::LAYOUT_DIRTY))
            {
                self.layout_and_paint(root_id);
                did_tree_work = true;
            }
        }

        // User callbacks run only after paint and dirty settlement. Signal writes
        // performed here remain queued for the next Runtime turn.
        self.drain_layout_notifications();
        RuntimeEffects::from_redraw(did_tree_work)
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

    /// Evaluates the focused editor or external leaf after current layout geometry is committed.
    fn collect_focused_external_ime(&mut self) -> RuntimeEffects {
        let ime = (|| {
            let target = self.events.focused_event_target(&self.arena)?;
            let fiber = self.arena.get(target)?;
            if let Some(ime) = fiber.view.as_ref()?.focused_ime(fiber.layout_rect()?) {
                return Some(ime);
            }
            let draw_id = fiber.view.as_ref()?.external_draw_id()?;
            let provider = self.external_ime.get(&draw_id)?;
            let rect = fiber.layout_rect()?;
            let viewport = self.current_viewport.clone()?;
            let context = crate::scene::primitive::ExternalDrawContext::new(rect, viewport);
            provider(draw_id, &context)
        })();

        let active = ime
            .as_ref()
            .is_some_and(|effect| effect.allowed == Some(true));
        let ime = if ime.is_none() && self.external_ime_active {
            Some(ImeEffect::set_allowed(false))
        } else {
            ime
        };
        self.external_ime_active = active;
        RuntimeEffects {
            ime,
            ..RuntimeEffects::default()
        }
    }

    /// Only the current focused target can keep a local animation deadline alive.
    fn collect_focused_animation(&mut self, now: Instant) -> RuntimeEffects {
        let (repaint, deadline) = self
            .events
            .focused_event_target(&self.arena)
            .and_then(|id| self.arena.get(id))
            .and_then(|fiber| fiber.view.as_ref())
            .map_or((false, None), |view| view.focused_animation(now));
        if repaint {
            self.run_paint_pass();
        }
        RuntimeEffects {
            request_redraw: repaint,
            control_flow: deadline.map(ControlFlowEffect::WaitUntil),
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
        self.events.set_layout_available(false);
        let _scope = RuntimeScope::enter(self.runtime_id);
        let hooks = std::mem::take(&mut self.arena.get_mut(root_id).unwrap().hooks);
        let subscriptions = std::mem::take(&mut self.arena.get_mut(root_id).unwrap().subscriptions);
        for subscription in subscriptions {
            subscription.unsubscribe_all(root_id);
        }
        let mut cx = BuildCx {
            current_fiber: Some(root_id),
            hooks,
            subscriptions: Vec::new(),
            hook_index: 0,
            externals: ExternalRegistrations::default(),
            theme: Arc::new(Theme::default()),
        };

        let view = self
            .root_view
            .as_ref()
            .unwrap_or_else(|| panic!("rebuild_root called without a stored root description"))
            .clone();
        let widget_type = view.widget_type();
        let key = view.key().cloned();
        let (inner, children, _explicit_key) = materialize_for_fiber(view, &mut cx);
        let effective_theme = inner
            .theme_override()
            .unwrap_or_else(|| Arc::new(Theme::default()));

        // Update root fiber
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.hooks = cx.hooks;
            fiber.subscriptions = std::mem::take(&mut cx.subscriptions);
            fiber.key = key;
            fiber.widget_type = widget_type;
            fiber.view = Some(inner);
            fiber.theme = effective_theme;
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

        // Resolve stale input identities before painting so fallback focus and
        // cursor state are reflected in the same frame.
        self.events.clear_dead_targets(&self.arena, self.root_id);

        self.layout_and_paint(root_id);
    }

    fn layout_and_paint(&mut self, root_id: FiberId) -> LayoutOutcome {
        self.events.set_layout_available(false);
        let viewport_size = self
            .current_viewport
            .as_ref()
            .map(|v| v.logical_size)
            .unwrap_or(Size::new(800.0, 600.0));
        let constraints = BoxConstraints::loose(viewport_size);
        // Intrinsic text widths must use the same advances as paint, including DPI.
        let scale = crate::text::normalize_raster_scale(
            self.current_viewport
                .as_ref()
                .map_or(1.0, |v| v.scale_factor),
        );
        let mut pending = vec![root_id];
        while let Some(id) = pending.pop() {
            if let Some(fiber) = self.arena.get(id) {
                pending.extend_from_slice(&fiber.children);
                if let Some(view) = &fiber.view {
                    view.prepare_text_geometry(&self.text_metrics, &mut |ch| {
                        self.encoder
                            .advance_width(ch, scale, self.text_metrics.cell_width)
                    });
                }
            }
        }
        let outcome = layout_fiber(
            &mut self.arena,
            root_id,
            constraints,
            Point::ZERO,
            &self.text_metrics,
        );
        if outcome.committed {
            self.events.set_layout_available(true);
            self.collect_post_layout(root_id);
            self.events.retry_pending_focus_visibility(&self.arena);
        }
        self.run_paint_pass();

        // The layout transaction retains prior/fallback geometry on failure.
        // Consume this attempt even then: only a new invalidation may retry it.
        // A root pass completes work for descendants as well as the root.
        let mut pending = vec![root_id];
        while let Some(id) = pending.pop() {
            if let Some(fiber) = self.arena.get_mut(id) {
                fiber.flags = DirtyFlags::NONE;
                pending.extend_from_slice(&fiber.children);
            }
        }
        outcome
    }

    fn collect_post_layout(&mut self, root_id: FiberId) {
        let mut pending = vec![root_id];
        while let Some(id) = pending.pop() {
            let Some(fiber) = self.arena.get(id) else {
                continue;
            };
            let children = fiber.children.clone();
            let rect = fiber.layout_rect;
            let view = fiber.view.clone();
            pending.extend_from_slice(&children);
            let (Some(rect), Some(view)) = (rect, view) else {
                continue;
            };
            let child_rects = children
                .iter()
                .filter(|child| {
                    !self
                        .arena
                        .get(**child)
                        .and_then(|f| f.view.as_ref())
                        .is_some_and(|v| v.is_portal())
                })
                .filter_map(|child| self.arena.get(*child).and_then(|fiber| fiber.layout_rect))
                .collect::<Vec<_>>();
            view.post_layout(rect, &child_rects);

            if !rect.is_valid() {
                continue;
            }
            let Some(callback) = view.layout_changed_callback() else {
                continue;
            };
            let Some(fiber) = self.arena.get_mut(id) else {
                continue;
            };
            if fiber.last_layout_notification == Some(rect) {
                continue;
            }
            fiber.last_layout_notification = Some(rect);
            self.pending_layout_notifications
                .push(PendingLayoutNotification {
                    fiber: id,
                    rect,
                    callback,
                });
        }
    }

    fn drain_layout_notifications(&mut self) {
        let notifications = std::mem::take(&mut self.pending_layout_notifications);
        for notification in notifications {
            let still_current = self
                .arena
                .get(notification.fiber)
                .is_some_and(|fiber| fiber.last_layout_notification == Some(notification.rect));
            if still_current {
                (notification.callback)(notification.rect);
            }
        }
    }

    /// Replaces Runtime-owned external registrations from one rebuild bag.
    fn install_externals(&mut self, externals: &mut ExternalRegistrations) {
        self.external_draws.clear();
        self.external_draws.extend(externals.draws.drain(..));
        self.external_schedules.clear();
        self.external_schedules
            .extend(externals.schedules.drain(..));
        self.external_ime.clear();
        self.external_ime.extend(externals.ime.drain(..));
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

    /// Initializes encoder-owned Widget text resources using system default UI fonts.
    pub fn init_text_renderer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> anyhow::Result<()> {
        let fonts = harbor_text::load_system_ui_fonts()?;
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        self.set_text_metrics(metrics);
        self.encoder
            .init_text_resources(device, queue, format, fonts);
        Ok(())
    }

    /// Creates another Runtime with independent render buffers and shared text resources.
    pub fn create_child_runtime(&self, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let mut runtime = Self::with_text_metrics(self.text_metrics);
        runtime.init_renderer(device, format);
        runtime
            .encoder
            .inherit_text_resources(&self.encoder, device, format);
        runtime
    }

    #[cfg(test)]
    pub(crate) fn init_text_renderer_with_bind_group(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
        bind_group: &wgpu::BindGroup,
    ) {
        self.encoder
            .init_text_renderer(device, format, bind_group_layout, bind_group);
    }

    /// Applies pending scene changes and encodes ordinary scenes into a pass.
    /// No-op until renderer initialization. Backdrop filters require encode_frame.
    ///
    /// `commit` live-encodes ineligible externals this pass (recovery / force).
    pub fn encode<'a>(
        &'a mut self,
        gpu: ExternalDrawGpu<'a>,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: Viewport,
        commit: bool,
    ) {
        self.encoder.encode(
            gpu,
            pass,
            viewport,
            EncodeScene {
                scene_graph: &self.scene_graph,
                pending_delta: &mut self.pending_delta,
                external_draws: &self.external_draws,
                external_eligible: &self.external_eligible,
                commit,
            },
        );
    }

    /// Encodes a full frame, including local backdrop filters, in paint order.
    /// The target is single-sampled and must match gpu.target_format() and viewport.
    /// The caller submits the encoder after this call; external callbacks run once.
    pub fn encode_frame(
        &mut self,
        gpu: ExternalDrawGpu<'_>,
        encoder: &mut wgpu::CommandEncoder,
        target: crate::renderer::FrameTarget<'_>,
        viewport: Viewport,
        commit: bool,
    ) {
        self.encoder.encode_frame(
            gpu,
            encoder,
            target,
            viewport,
            EncodeScene {
                scene_graph: &self.scene_graph,
                pending_delta: &mut self.pending_delta,
                external_draws: &self.external_draws,
                external_eligible: &self.external_eligible,
                commit,
            },
        );
    }

    /// Resolves whether this Runtime owns at least one external draw handler.
    ///
    /// Presenters use this to select the frame clear policy: runtimes that own
    /// external draws (e.g. the main window) clear transparently, while plain
    /// widget runtimes (e.g. dialogs) keep an opaque fallback clear.
    pub fn has_external_draws(&self) -> bool {
        !self.external_draws.is_empty()
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
        self.invalidate_layout();
        true
    }

    // ── Input ───────────────────────────────────────────────────────────

    /// Dispatches a UI event into the widget tree.
    ///
    /// Routes the event through capture → target → bubble phases,
    /// then applies any commands issued by handlers. Returns only this event's
    /// effects; prior work committed for routing is retained for the next update.
    pub fn dispatch(&mut self, event: UiEvent) -> RuntimeEffects {
        let pending = self.flush_pending_work();
        self.pending_effects.merge(pending);
        let _scope = RuntimeScope::enter(self.runtime_id);
        let needs_redraw = self.events.route_event(&self.arena, self.root_id, &event);
        if needs_redraw && let Some(root_id) = self.root_id {
            mark_dirty_for(self.runtime_id, root_id);
        }
        let mut effects = RuntimeEffects::from_redraw(needs_redraw);
        if let Some(text) = self.events.take_clipboard() {
            effects.clipboard = Some(ClipboardEffect::write(text));
        }
        if let Some(request) = self.events.take_editor_clipboard() {
            effects.clipboard = Some(ClipboardEffect::Editor(request));
        }
        if let Some(cursor) = self.events.take_cursor() {
            effects.cursor = Some(cursor);
        }
        effects.merge(self.collect_focused_external_ime());
        effects
    }

    /// Completes a tagged editor operation only while its mounted focused target is live.
    pub fn complete_clipboard(
        &mut self,
        request: u64,
        result: Result<String, crate::effects::ClipboardEffectError>,
    ) -> RuntimeEffects {
        let pending = self.flush_pending_work();
        self.pending_effects.merge(pending);
        let redraw = self.events.complete_clipboard(&self.arena, request, result);
        if redraw && let Some(root) = self.root_id {
            mark_dirty_for(self.runtime_id, root);
        }
        RuntimeEffects::from_redraw(redraw)
    }

    /// Cancel input actions when a host gates/replaces an input-owning subtree.
    /// Consumed shortcut repeats/releases stay suppressed until release or a new
    /// native physical press, even if a modal gate drops the old release.
    pub fn cancel_active_input_ownership(&mut self, position: Point) -> RuntimeEffects {
        self.events.cancel_shortcut_actions();
        self.cancel_pointer_captures(position)
    }

    #[cfg(feature = "winit")]
    pub(crate) fn begin_key_press(&mut self, key: crate::input::event::Key) {
        self.events.begin_key_press(key);
    }

    /// Cancels every pointer currently captured by a widget.
    ///
    /// Hosts use this at lifecycle boundaries such as window focus loss so a
    /// widget cannot observe a later button-up from a different interaction.
    pub fn cancel_pointer_captures(&mut self, position: Point) -> RuntimeEffects {
        let pointer_ids = self.events.input().captured_pointer_ids();
        let mut effects = RuntimeEffects::default();
        for pointer_id in pointer_ids {
            effects.merge(
                self.dispatch(UiEvent::Pointer(crate::input::event::PointerEvent::new(
                    position,
                    PointerPhase::Cancel,
                    crate::input::event::PointerButton::Left,
                    pointer_id,
                ))),
            );
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

    /// Ensures glyphs and prepares cached text runs through the frame encoder.
    ///
    /// Call after Runtime update/paint and before encoding the frame.
    pub fn prepare_text(&mut self, queue: &wgpu::Queue) {
        let raster_scale = self
            .current_viewport
            .as_ref()
            .map_or(1.0, |viewport| viewport.scale_factor);
        self.encoder.prepare_text(
            &self.scene_graph,
            &self.text_metrics,
            self.runtime_id,
            raster_scale,
            queue,
        );
    }

    #[cfg(test)]
    fn prepare_text_runs(&mut self, glyph_fn: &crate::text::GlyphFn<'_>) {
        self.encoder
            .prepare_text_runs(&self.scene_graph, &self.text_metrics, 0, glyph_fn);
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

    /// Drains queued external input events produced by focusable ExternalSurface
    /// widgets during the last event dispatch.
    pub fn drain_external_input(
        &mut self,
    ) -> Vec<(
        crate::scene::primitive::ExternalDrawId,
        crate::input::event::UiEvent,
    )> {
        self.events.drain_external_input()
    }

    /// Returns a reference to the InputState.
    pub fn input(&self) -> &InputState {
        self.events.input()
    }

    /// Requests focus by stable public handle. An absent target is retained until
    /// reconciliation creates a matching enabled Focus wrapper.
    pub fn request_focus(&mut self, handle: &crate::widgets::FocusHandle) -> RuntimeEffects {
        let _scope = RuntimeScope::enter(self.runtime_id);
        let changed = self
            .events
            .request_focus_handle(&self.arena, self.root_id, handle.0);
        if changed {
            if let Some(root_id) = self.root_id {
                mark_dirty_for(self.runtime_id, root_id);
            }
            RuntimeEffects::request_redraw()
        } else {
            RuntimeEffects::default()
        }
    }

    /// Returns the number of retained cached text runs.
    pub fn cached_text_run_count(&self) -> usize {
        self.encoder.cached_text_run_count()
    }

    #[cfg(test)]
    pub(crate) fn text_run_cache(&mut self) -> &mut TextRunCache {
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
        self.clear_root();
    }
}

#[cfg(test)]
mod tests;
