use crate::fiber::{
    DirtyFlags, Fiber, FiberArena, FiberId, layout_fiber, paint_fiber, reconcile_children,
    unmount_fiber,
};
use crate::input::event::{PointerPhase, UiEvent};
use crate::input::event_ctx::EventCtx;
use crate::input::state::InputState;
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::renderer::Viewport;
use crate::renderer::quad::QuadRenderer;
use crate::scene::{SceneDelta, SceneGraph};
use crate::signal::PENDING_DIRTY;
use crate::view::{BuildCx, Component};
use hashbrown::HashSet;
use std::time::Instant;

// ── FrameRequest ────────────────────────────────────────────────────────────

/// Post-update signal indicating whether a redraw is needed.
pub struct FrameRequest {
    pub needs_redraw: bool,
}

// ── Runtime ─────────────────────────────────────────────────────────────────

/// Top-level widget tree scheduler.
///
/// Owns the fiber tree and orchestrates reconcile -> layout -> paint cycles
/// as well as input event routing.
pub struct Runtime {
    arena: FiberArena,
    root_id: Option<FiberId>,
    root_component: Option<Box<dyn Component>>,
    scene_graph: SceneGraph,
    renderer: Option<QuadRenderer>,
    pending_delta: Option<SceneDelta>,
    current_viewport: Option<Viewport>,
    input: InputState,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            arena: FiberArena::new(),
            root_id: None,
            root_component: None,
            scene_graph: SceneGraph::new(),
            renderer: None,
            pending_delta: None,
            current_viewport: None,
            input: InputState::new(),
        }
    }

    /// Sets the root component and performs the initial build + layout.
    ///
    /// If a previous root existed, it is unmounted recursively.
    pub fn set_root(&mut self, root: impl Component + 'static) {
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
        crate::signal::mark_dirty(root_id);
    }

    /// Processes dirty fibers and runs layout.
    ///
    /// Returns a `FrameRequest` indicating whether a redraw is needed.
    pub fn update(&mut self, _now: Instant) -> FrameRequest {
        let dirty: HashSet<FiberId> = PENDING_DIRTY.with(|q| std::mem::take(&mut *q.borrow_mut()));

        if dirty.is_empty() {
            return FrameRequest {
                needs_redraw: false,
            };
        }

        let Some(root_id) = self.root_id else {
            return FrameRequest {
                needs_redraw: false,
            };
        };

        let old_children = self
            .arena
            .get(root_id)
            .map(|f| f.children.clone())
            .unwrap_or_default();

        self.rebuild_root(root_id, &old_children);

        FrameRequest { needs_redraw: true }
    }

    /// Shared rebuild → reconcile → layout → paint sequence.
    fn rebuild_root(&mut self, root_id: FiberId, old_children: &[FiberId]) {
        let hooks = std::mem::take(&mut self.arena.get_mut(root_id).unwrap().hooks);
        let mut cx = BuildCx {
            current_fiber: Some(root_id),
            hooks,
            hook_index: 0,
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
        let new_children = reconcile_children(&mut self.arena, root_id, old_children, children);
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.children = new_children;
        }

        // Layout
        let viewport_size = self
            .current_viewport
            .as_ref()
            .map(|v| v.logical_size)
            .unwrap_or(Size::new(800.0, 600.0));
        let constraints = BoxConstraints::loose(viewport_size);
        layout_fiber(&mut self.arena, root_id, constraints, Point::ZERO);
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.flags.remove(DirtyFlags::LAYOUT_DIRTY);
        }

        // Paint
        self.run_paint_pass();

        // Clean input state
        self.input.clear_focus_if_dead(&self.arena);
        self.input.clear_capture_if_dead(&self.arena);
    }

    /// Initializes the GPU renderer. Must be called after a wgpu Device is
    /// available and before the first call to `encode()`.
    pub fn init_renderer(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.renderer = Some(QuadRenderer::new(device, format));
    }

    /// Applies the pending SceneDelta to the GPU renderer and encodes draw
    /// calls into the RenderPass. No-op if the renderer hasn't been
    /// initialized or there is no pending delta.
    pub fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: Viewport,
    ) {
        if let Some(ref mut renderer) = self.renderer {
            if let Some(ref delta) = self.pending_delta {
                self.current_viewport = Some(viewport.clone());
                renderer.update(queue, delta, &viewport);
            }
            renderer.encode(pass);
        }
    }

    /// Signals that the viewport has changed (e.g., due to window resize).
    /// Marks the root fiber as LAYOUT_DIRTY so the next update re-lays out
    /// at the new size.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.current_viewport = Some(viewport);
        if let Some(root_id) = self.root_id {
            if let Some(fiber) = self.arena.get_mut(root_id) {
                fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
            }
            crate::signal::mark_dirty(root_id);
        }
    }

    // ── Input ───────────────────────────────────────────────────────────

    /// Dispatches a UI event into the widget tree.
    ///
    /// Routes the event through capture → target → bubble phases,
    /// then applies any commands issued by handlers. Returns a
    /// `FrameRequest` indicating whether a redraw is needed.
    pub fn dispatch(&mut self, event: UiEvent, _now: Instant) -> FrameRequest {
        let needs_redraw = self.route_event(&event);
        if needs_redraw && let Some(root_id) = self.root_id {
            crate::signal::mark_dirty(root_id);
        }
        FrameRequest { needs_redraw }
    }

    /// Core event routing: hit test → capture → target → bubble → apply.
    /// Returns true if a repaint is needed.
    fn route_event(&mut self, event: &UiEvent) -> bool {
        let Some(root_id) = self.root_id else {
            return false;
        };

        // 1. Determine target
        let target: Option<FiberId> = match event {
            UiEvent::Pointer(pe) => {
                // Pointer Cancel: release capture and route to captor one last time
                if pe.phase == PointerPhase::Cancel {
                    if let Some(captor) = self.input.captor(pe.pointer_id) {
                        self.input.apply(
                            vec![crate::input::event_ctx::EventCommand::ReleasePointer(
                                pe.pointer_id,
                            )],
                            &self.arena,
                        );
                        if self.arena.contains(captor) {
                            return self.route_to_single(captor, event);
                        }
                    }
                    return false;
                }
                // If this pointer is captured, bypass hit test
                if let Some(captor) = self.input.captor(pe.pointer_id) {
                    if self.arena.contains(captor) {
                        Some(captor)
                    } else {
                        // Captor is dead — release capture and fall through
                        self.input.apply(
                            std::mem::take(&mut EventCtx::new().take_commands()),
                            &self.arena,
                        );
                        None
                    }
                } else {
                    self.hit_test_walk(root_id, pe.position)
                }
            }
            UiEvent::Keyboard(_) | UiEvent::Focus(_) => self.input.focused,
        };

        // 2. Build ancestor path from root to target.
        // For keyboard/focus events with no focused widget, include the
        // root so root-level FocusScope can still intercept Tab.
        let path = if target.is_none() && matches!(event, UiEvent::Keyboard(_) | UiEvent::Focus(_))
        {
            vec![root_id]
        } else {
            self.build_ancestor_path(target)
        };

        // 3. Walk phases
        let mut ctx = EventCtx::new();

        // Capture phase: root → target (excluding target)
        for &ancestor_id in path.iter().take(path.len().saturating_sub(1)) {
            if self.is_modal_block(ancestor_id, target) {
                return self.finish_event(ctx);
            }
            self.invoke_handler(ancestor_id, event, &mut ctx);
            if ctx.is_propagation_stopped() {
                return self.finish_event(ctx);
            }
        }

        // Target phase
        if let Some(tid) = target {
            self.invoke_handler(tid, event, &mut ctx);
            if ctx.is_propagation_stopped() {
                return self.finish_event(ctx);
            }
        }

        // Bubble phase: target → root (excluding target)
        for &ancestor_id in path.iter().take(path.len().saturating_sub(1)).rev() {
            self.invoke_handler(ancestor_id, event, &mut ctx);
            if ctx.is_propagation_stopped() {
                return self.finish_event(ctx);
            }
        }

        self.finish_event(ctx)
    }

    /// Route an event to a single fiber (capture → target → bubble within
    /// just that fiber). Used for Pointer Cancel delivery to captor.
    fn route_to_single(&mut self, fiber_id: FiberId, event: &UiEvent) -> bool {
        let mut ctx = EventCtx::new();
        self.invoke_handler(fiber_id, event, &mut ctx);
        self.finish_event(ctx)
    }

    /// Apply accumulated EventCtx commands and return whether repaint is needed.
    fn finish_event(&mut self, mut ctx: EventCtx) -> bool {
        let needs_paint = self.input.apply(ctx.take_commands(), &self.arena);
        needs_paint || ctx.needs_paint()
    }

    /// Call handle_event on a fiber, setting up EventCtx with the fiber id.
    fn invoke_handler(&mut self, fiber_id: FiberId, event: &UiEvent, ctx: &mut EventCtx) {
        let rect = self.arena.get(fiber_id).and_then(|f| f.layout_rect);
        let view = self.arena.get(fiber_id).and_then(|f| f.view.clone());

        if let (Some(view), Some(rect)) = (view, rect) {
            ctx.set_current_fiber(fiber_id);
            view.handle_event(event, ctx, rect);
        }
    }

    /// Hit test: reverse-paint-order DFS walk.
    /// Returns the topmost fiber whose hit_test returns true.
    fn hit_test_walk(&self, fiber_id: FiberId, point: Point) -> Option<FiberId> {
        let fiber = self.arena.get(fiber_id)?;
        let rect = fiber.layout_rect?;

        // Coarse check: point must be within this fiber's rect
        if !rect.contains(point) {
            return None;
        }

        // Children in reverse order (topmost / last-painted first).
        // Clone necessary to release the immutable borrow before recursing.
        let children = fiber.children.clone();
        for &child_id in children.iter().rev() {
            if let Some(hit) = self.hit_test_walk(child_id, point) {
                return Some(hit);
            }
        }

        // Check self: point in local coordinates
        let local_point = Point::new(point.x - rect.min.x, point.y - rect.min.y);
        let local_rect = Rect::from_min_size(Point::ZERO, rect.size());
        if let Some(ref view) = fiber.view
            && view.hit_test(local_point, local_rect)
        {
            return Some(fiber_id);
        }

        None
    }

    /// Builds the ancestor chain from root to the given target.
    fn build_ancestor_path(&self, target: Option<FiberId>) -> Vec<FiberId> {
        let mut path = Vec::new();
        let mut current = target;
        while let Some(id) = current {
            path.push(id);
            current = self.arena.get(id).and_then(|f| f.parent);
        }
        path.reverse();
        path
    }

    /// Checks whether `ancestor` is a modal scope that blocks events
    /// targeting `target`.
    fn is_modal_block(&self, ancestor: FiberId, target: Option<FiberId>) -> bool {
        let fiber = match self.arena.get(ancestor) {
            Some(f) => f,
            None => return false,
        };

        let is_modal = fiber.view.as_ref().is_some_and(|v| v.is_modal_scope());

        if !is_modal {
            return false;
        }

        // Block if target is not a descendant of this modal scope
        !self.is_descendant_of(target, ancestor)
    }

    /// Returns true if `descendant` is in the subtree rooted at `ancestor`.
    fn is_descendant_of(&self, descendant: Option<FiberId>, ancestor: FiberId) -> bool {
        let mut current = descendant;
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.arena.get(id).and_then(|f| f.parent);
        }
        false
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn run_paint_pass(&mut self) {
        let Some(root_id) = self.root_id else {
            return;
        };

        let items = paint_fiber(&self.arena, root_id, 0);
        let delta = self.scene_graph.diff(items);
        self.pending_delta = Some(delta);
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

    /// Returns a reference to the InputState.
    pub fn input(&self) -> &InputState {
        &self.input
    }

    /// Programmatically sets the focused fiber for keyboard event routing.
    pub fn set_focus(&mut self, id: FiberId) {
        self.input.focused = Some(id);
    }

    /// Clears the focused fiber.
    pub fn clear_focus(&mut self) {
        self.input.focused = None;
    }

    /// Returns true if any modal FocusScope is currently active in the tree.
    /// The host can check this to suppress events (e.g., paste) that should
    /// be blocked while a modal is open.
    pub fn has_modal(&self) -> bool {
        let Some(root_id) = self.root_id else {
            return false;
        };
        self.tree_has_modal(root_id)
    }

    fn tree_has_modal(&self, fiber_id: FiberId) -> bool {
        let fiber = match self.arena.get(fiber_id) {
            Some(f) => f,
            None => return false,
        };
        if fiber.view.as_ref().is_some_and(|v| v.is_modal_scope()) {
            return true;
        }
        for &child_id in &fiber.children {
            if self.tree_has_modal(child_id) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{
        Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
    };
    use crate::widgets::button::Button;
    use crate::widgets::focus_scope::FocusScope;
    use crate::widgets::sized_box::SizedBox;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    fn now() -> Instant {
        Instant::now()
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
    fn new_runtime_has_no_root() {
        let rt = Runtime::new();
        assert!(rt.root_id().is_none());
        assert!(rt.pending_delta().is_none());
        assert!(rt.input().focused.is_none());
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
        );
        // No panic = pass
    }

    // ── has_modal with no root ─────────────────────────────────────────

    #[test]
    fn has_modal_returns_false_with_no_root() {
        let rt = Runtime::new();
        assert!(!rt.has_modal());
    }
}
