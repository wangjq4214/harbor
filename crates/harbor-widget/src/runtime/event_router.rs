//! Pointer/keyboard/focus event routing over the fiber tree.

use crate::fiber::{FiberArena, FiberId};
use crate::input::event::{FocusEvent, PointerPhase, UiEvent};
use crate::input::event_ctx::EventCtx;
use crate::input::state::InputState;
use crate::layout::{Point, Rect};

/// Owns input state and routes UI events through capture → target → bubble.
///
/// Lifecycle: created with the Runtime and destroyed with it. Does not own the
/// fiber arena — callers pass arena + root for tree walks.
pub(crate) struct EventRouter {
    input: InputState,
    pending_clipboard: Option<String>,
}

impl EventRouter {
    pub(crate) fn new() -> Self {
        Self {
            input: InputState::new(),
            pending_clipboard: None,
        }
    }

    pub(crate) fn input(&self) -> &InputState {
        &self.input
    }

    pub(crate) fn take_clipboard(&mut self) -> Option<String> {
        self.pending_clipboard.take()
    }

    /// Clears focus/capture entries pointing at dead fibers after a rebuild.
    pub(crate) fn clear_dead_targets(&mut self, arena: &FiberArena) {
        self.input.clear_focus_if_dead(arena);
        self.input.clear_capture_if_dead(arena);
    }

    /// Core event routing: hit test → capture → target → bubble → apply.
    /// Returns true if a repaint is needed.
    pub(crate) fn route_event(
        &mut self,
        arena: &FiberArena,
        root_id: Option<FiberId>,
        event: &UiEvent,
    ) -> bool {
        let Some(root_id) = root_id else {
            return false;
        };

        let target: Option<FiberId> = match event {
            UiEvent::Pointer(pe) => {
                if pe.phase == PointerPhase::Cancel {
                    if let Some(captor) = self.input.captor(pe.pointer_id) {
                        self.input.apply(
                            vec![crate::input::event_ctx::EventCommand::ReleasePointer(
                                pe.pointer_id,
                            )],
                            arena,
                        );
                        if arena.contains(captor) {
                            return self.route_to_single(arena, captor, event);
                        }
                    }
                    return false;
                }
                if let Some(captor) = self.input.captor(pe.pointer_id) {
                    if arena.contains(captor) {
                        Some(captor)
                    } else {
                        self.input
                            .apply(std::mem::take(&mut EventCtx::new().take_commands()), arena);
                        None
                    }
                } else {
                    Self::hit_test_walk(arena, root_id, pe.position)
                }
            }
            UiEvent::Keyboard(_) | UiEvent::Focus(_) => self.input.focused,
        };

        let path = if target.is_none() && matches!(event, UiEvent::Keyboard(_) | UiEvent::Focus(_))
        {
            vec![root_id]
        } else {
            Self::build_ancestor_path(arena, target)
        };

        let mut ctx = EventCtx::new();

        for &ancestor_id in path.iter().take(path.len().saturating_sub(1)) {
            if Self::is_modal_block(arena, ancestor_id, target) {
                return self.finish_event(arena, ctx);
            }
            Self::invoke_handler(arena, ancestor_id, event, &mut ctx);
            if ctx.is_propagation_stopped() {
                return self.finish_event(arena, ctx);
            }
        }

        if let Some(tid) = target {
            Self::invoke_handler(arena, tid, event, &mut ctx);
            if ctx.is_propagation_stopped() {
                return self.finish_event(arena, ctx);
            }
        }

        for &ancestor_id in path.iter().take(path.len().saturating_sub(1)).rev() {
            Self::invoke_handler(arena, ancestor_id, event, &mut ctx);
            if ctx.is_propagation_stopped() {
                return self.finish_event(arena, ctx);
            }
        }

        self.finish_event(arena, ctx)
    }

    pub(crate) fn route_to_single(
        &mut self,
        arena: &FiberArena,
        fiber_id: FiberId,
        event: &UiEvent,
    ) -> bool {
        let mut ctx = EventCtx::new();
        Self::invoke_handler(arena, fiber_id, event, &mut ctx);
        self.finish_event(arena, ctx)
    }

    pub(crate) fn finish_event(&mut self, arena: &FiberArena, mut ctx: EventCtx) -> bool {
        let clipboard_write = ctx.take_clipboard_write();
        let previous_focus = self.input.focused;
        let needs_paint = self.input.apply(ctx.take_commands(), arena);
        if clipboard_write.is_some() {
            self.pending_clipboard = clipboard_write;
        }
        let next_focus = self.input.focused;
        let focus_needs_paint = if previous_focus != next_focus {
            self.notify_focus_transition(arena, previous_focus, next_focus)
        } else {
            false
        };
        needs_paint || ctx.needs_paint() || focus_needs_paint
    }

    pub(crate) fn transition_focus(&mut self, arena: &FiberArena, next: Option<FiberId>) -> bool {
        let previous = self.input.focused;
        if previous == next {
            return false;
        }
        self.input.focused = next;
        self.notify_focus_transition(arena, previous, next)
    }

    pub(crate) fn notify_focus_transition(
        &mut self,
        arena: &FiberArena,
        previous: Option<FiberId>,
        next: Option<FiberId>,
    ) -> bool {
        let mut needs_paint = previous != next;
        if let Some(fiber_id) = previous {
            needs_paint |= self.notify_focus(arena, fiber_id, FocusEvent::Lost);
        }
        if let Some(fiber_id) = next {
            needs_paint |= self.notify_focus(arena, fiber_id, FocusEvent::Gained);
        }
        needs_paint
    }

    fn notify_focus(&mut self, arena: &FiberArena, fiber_id: FiberId, event: FocusEvent) -> bool {
        let mut ctx = EventCtx::new();
        Self::invoke_handler(arena, fiber_id, &UiEvent::Focus(event), &mut ctx);
        let needs_paint = self.input.apply(ctx.take_commands(), arena);
        needs_paint || ctx.needs_paint()
    }

    pub(crate) fn find_first_focusable(arena: &FiberArena, fiber_id: FiberId) -> Option<FiberId> {
        let fiber = arena.get(fiber_id)?;
        if let Some(ref view) = fiber.view
            && view.is_focusable()
        {
            return Some(fiber_id);
        }
        for &child_id in &fiber.children {
            if let Some(fid) = Self::find_first_focusable(arena, child_id) {
                return Some(fid);
            }
        }
        None
    }

    pub(crate) fn tree_has_modal(arena: &FiberArena, fiber_id: FiberId) -> bool {
        let fiber = match arena.get(fiber_id) {
            Some(f) => f,
            None => return false,
        };
        if fiber.view.as_ref().is_some_and(|v| v.is_modal_scope()) {
            return true;
        }
        for &child_id in &fiber.children {
            if Self::tree_has_modal(arena, child_id) {
                return true;
            }
        }
        false
    }

    pub(crate) fn hit_test_walk(
        arena: &FiberArena,
        fiber_id: FiberId,
        point: Point,
    ) -> Option<FiberId> {
        let fiber = arena.get(fiber_id)?;
        let rect = fiber.layout_rect?;

        if !rect.contains(point) {
            return None;
        }

        let children = fiber.children.clone();
        for &child_id in children.iter().rev() {
            if let Some(hit) = Self::hit_test_walk(arena, child_id, point) {
                return Some(hit);
            }
        }

        let local_point = Point::new(point.x - rect.min.x, point.y - rect.min.y);
        let local_rect = Rect::from_min_size(Point::ZERO, rect.size());
        if let Some(ref view) = fiber.view
            && view.hit_test(local_point, local_rect)
        {
            return Some(fiber_id);
        }

        None
    }

    pub(crate) fn build_ancestor_path(arena: &FiberArena, target: Option<FiberId>) -> Vec<FiberId> {
        let mut path = Vec::new();
        let mut current = target;
        while let Some(id) = current {
            path.push(id);
            current = arena.get(id).and_then(|f| f.parent);
        }
        path.reverse();
        path
    }

    pub(crate) fn is_modal_block(
        arena: &FiberArena,
        ancestor: FiberId,
        target: Option<FiberId>,
    ) -> bool {
        let fiber = match arena.get(ancestor) {
            Some(f) => f,
            None => return false,
        };

        let is_modal = fiber.view.as_ref().is_some_and(|v| v.is_modal_scope());

        if !is_modal {
            return false;
        }

        !Self::is_descendant_of(arena, target, ancestor)
    }

    pub(crate) fn is_descendant_of(
        arena: &FiberArena,
        descendant: Option<FiberId>,
        ancestor: FiberId,
    ) -> bool {
        let mut current = descendant;
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = arena.get(id).and_then(|f| f.parent);
        }
        false
    }

    fn invoke_handler(arena: &FiberArena, fiber_id: FiberId, event: &UiEvent, ctx: &mut EventCtx) {
        let rect = arena.get(fiber_id).and_then(|f| f.layout_rect);
        let view = arena.get(fiber_id).and_then(|f| f.view.clone());

        if let (Some(view), Some(rect)) = (view, rect) {
            ctx.set_current_fiber(fiber_id);
            view.handle_event(event, ctx, rect);
        }
    }
}
