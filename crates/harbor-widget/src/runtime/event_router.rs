//! Pointer, focus, shortcut, and keyboard event routing over the fiber tree.

use crate::decoration::ClipBehavior;
use crate::effects::{CursorEffect, CursorShape};
use crate::fiber::{FiberArena, FiberId};
use crate::input::event::{
    FocusEvent, Key, KeyboardEvent, PointerBoundaryKind, PointerPhase, UiEvent,
};
use crate::input::event_ctx::EventCtx;
use crate::input::state::InputState;
use crate::layout::{Point, Rect};
use crate::widgets::shortcuts::KeyChord;
use std::any::Any;

/// Owns input state and routes UI events through capture → target → bubble.
pub(crate) struct EventRouter {
    input: InputState,
    pending_clipboard: Option<String>,
    pending_cursor: Option<CursorEffect>,
    hover_path: Vec<FiberId>,
    last_cursor: Option<CursorShape>,
    pending_focus_handle: Option<u64>,
    last_focus_scope: Option<FiberId>,
}

impl EventRouter {
    pub(crate) fn new() -> Self {
        Self {
            input: InputState::new(),
            pending_clipboard: None,
            pending_cursor: None,
            hover_path: Vec::new(),
            last_cursor: None,
            pending_focus_handle: None,
            last_focus_scope: None,
        }
    }

    pub(crate) fn input(&self) -> &InputState {
        &self.input
    }

    pub(crate) fn take_clipboard(&mut self) -> Option<String> {
        self.pending_clipboard.take()
    }

    pub(crate) fn take_cursor(&mut self) -> Option<CursorEffect> {
        self.pending_cursor.take()
    }

    /// Clears stale routing identities after reconciliation and resolves a pending
    /// public focus-handle request when its target has appeared.
    pub(crate) fn clear_dead_targets(&mut self, arena: &FiberArena, root: Option<FiberId>) {
        let focused_became_invalid = self.input.focused.is_some_and(|focused| {
            arena
                .get(focused)
                .and_then(|fiber| fiber.view.as_ref())
                .and_then(|view| view.focus_metadata())
                .map(|metadata| !metadata.enabled)
                .unwrap_or(true)
        });
        let focus_visible = self.input.focus_visible();
        self.input.clear_focus_if_dead(arena);
        self.input.clear_focus_if(|focused| {
            arena
                .get(focused)
                .and_then(|fiber| fiber.view.as_ref())
                .and_then(|view| view.focus_metadata())
                .is_none_or(|metadata| !metadata.enabled)
        });
        self.input.clear_capture_if_dead(arena);
        self.hover_path.retain(|id| arena.contains(*id));
        self.input.clear_capture_if(|id| {
            arena
                .get(id)
                .and_then(|fiber| fiber.view.as_ref())
                .is_some_and(|view| view.permits_pointer_capture())
        });
        self.input.hovered = self.hover_path.last().copied();
        let cursor = self.hover_path.iter().rev().find_map(|fiber| {
            arena
                .get(*fiber)
                .and_then(|fiber| fiber.view.as_ref())
                .and_then(|view| view.pointer_cursor())
        });
        self.set_cursor(cursor);
        if focused_became_invalid && self.pending_focus_handle.is_none() {
            let fallback = self
                .last_focus_scope
                .filter(|scope| arena.contains(*scope))
                .and_then(|scope| Self::find_next_focusable(arena, scope, None, true));
            self.transition_focus_with_visibility(arena, fallback, focus_visible);
        }
        if let (Some(handle), Some(root)) = (self.pending_focus_handle, root)
            && let Some(target) = Self::find_by_focus_handle(arena, root, handle)
        {
            self.pending_focus_handle = None;
            self.transition_focus_with_visibility(arena, Some(target), true);
        }
    }

    pub(crate) fn request_focus_handle(
        &mut self,
        arena: &FiberArena,
        root: Option<FiberId>,
        handle: u64,
    ) -> bool {
        let Some(root) = root else {
            self.pending_focus_handle = Some(handle);
            return false;
        };
        if let Some(target) = Self::find_by_focus_handle(arena, root, handle) {
            self.pending_focus_handle = None;
            self.transition_focus_with_visibility(arena, Some(target), true)
        } else {
            self.pending_focus_handle = Some(handle);
            false
        }
    }

    /// Core event routing: hover bookkeeping → shortcut/focus pre-routing →
    /// capture → target → bubble → command application.
    pub(crate) fn route_event(
        &mut self,
        arena: &FiberArena,
        root_id: Option<FiberId>,
        event: &UiEvent,
    ) -> bool {
        let Some(root_id) = root_id else {
            return false;
        };

        let mut hover_needs_redraw = false;
        match event {
            UiEvent::Pointer(pointer) if pointer.phase == PointerPhase::Move => {
                hover_needs_redraw = self.update_hover_path(
                    arena,
                    root_id,
                    pointer.pointer_id,
                    Some(pointer.position),
                );
            }
            UiEvent::PointerBoundary(boundary) => {
                let position = match boundary.kind {
                    PointerBoundaryKind::Enter => boundary.position,
                    PointerBoundaryKind::Leave => None,
                };
                return self.update_hover_path(arena, root_id, boundary.pointer_id, position);
            }
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Tab,
                modifiers,
            }) if !modifiers.ctrl && !modifiers.alt && !modifiers.meta => {
                return self.navigate_focus(arena, root_id, !modifiers.shift);
            }
            UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }) => {
                if let Some((source, action)) = self.find_shortcut(arena, root_id, *key, *modifiers)
                {
                    let focus_visibility_changed = self.input.focused.is_some()
                        && !self.input.focus_visible()
                        && self.transition_focus_with_visibility(arena, self.input.focused, true);
                    let commands_committed = self.finish_event(arena, EventCtx::new());
                    let action_invoked = self.invoke_action(arena, source, action);
                    return focus_visibility_changed || commands_committed || action_invoked;
                }
            }
            _ => {}
        }

        let target = match event {
            UiEvent::Pointer(pointer) => {
                if pointer.phase == PointerPhase::Cancel {
                    if let Some(captor) = self.input.captor(pointer.pointer_id) {
                        self.input.apply(
                            vec![crate::input::event_ctx::EventCommand::ReleasePointer(
                                pointer.pointer_id,
                            )],
                            arena,
                        );
                        if arena.contains(captor) {
                            return self.route_to_single(arena, captor, event);
                        }
                    }
                    return false;
                }
                self.input
                    .captor(pointer.pointer_id)
                    .filter(|captor| arena.contains(*captor))
                    .or_else(|| Self::hit_test_walk(arena, root_id, pointer.position))
            }
            UiEvent::Keyboard(_) | UiEvent::Focus(_) => self.input.focused,
            UiEvent::PointerBoundary(_) => None,
        };

        let target = if matches!(event, UiEvent::Keyboard(_) | UiEvent::Focus(_)) {
            target.map(|target| Self::resolve_focused_event_target(arena, target))
        } else {
            target
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

        if let Some(target) = target {
            Self::invoke_handler(arena, target, event, &mut ctx);
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
        hover_needs_redraw || self.finish_event(arena, ctx)
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
        if let Some(text) = ctx.take_clipboard_write() {
            self.pending_clipboard = Some(text);
        }
        let previous_focus = self.input.focused;
        let previous_visible = self.input.focus_visible;
        let needs_paint = self.input.apply(ctx.take_commands(), arena);
        let next_focus = self.input.focused;
        let next_visible = self.input.focus_visible;
        if previous_focus != next_focus {
            self.last_focus_scope =
                next_focus.and_then(|target| Self::focus_scope_for(arena, target));
        }
        let focus_needs_paint = if previous_focus != next_focus {
            self.notify_focus_transition(arena, previous_focus, next_focus, next_visible)
        } else if previous_visible != next_visible {
            next_focus
                .map(|fiber| {
                    self.notify_focus(arena, fiber, FocusEvent::VisibilityChanged(next_visible))
                })
                .unwrap_or(false)
        } else {
            false
        };
        needs_paint || ctx.needs_paint() || focus_needs_paint
    }

    pub(crate) fn transition_focus(&mut self, arena: &FiberArena, next: Option<FiberId>) -> bool {
        self.transition_focus_with_visibility(arena, next, false)
    }

    fn transition_focus_with_visibility(
        &mut self,
        arena: &FiberArena,
        next: Option<FiberId>,
        focus_visible: bool,
    ) -> bool {
        let previous = self.input.focused;
        let previous_visible = self.input.focus_visible;
        if previous == next && previous_visible == focus_visible {
            return false;
        }
        self.input.focused = next;
        self.input.focus_visible = focus_visible;
        self.last_focus_scope = next.and_then(|target| Self::focus_scope_for(arena, target));
        if previous != next {
            self.notify_focus_transition(arena, previous, next, focus_visible)
        } else if let Some(fiber) = next {
            self.notify_focus(arena, fiber, FocusEvent::VisibilityChanged(focus_visible))
        } else {
            false
        }
    }

    fn notify_focus_transition(
        &mut self,
        arena: &FiberArena,
        previous: Option<FiberId>,
        next: Option<FiberId>,
        focus_visible: bool,
    ) -> bool {
        let mut needs_paint = previous != next;
        if let Some(fiber) = previous {
            needs_paint |= self.notify_focus(arena, fiber, FocusEvent::Lost);
        }
        if let Some(fiber) = next {
            let gained = if focus_visible {
                FocusEvent::GainedVisible
            } else {
                FocusEvent::Gained
            };
            needs_paint |= self.notify_focus(arena, fiber, gained);
        }
        needs_paint
    }

    fn notify_focus(&mut self, arena: &FiberArena, fiber: FiberId, event: FocusEvent) -> bool {
        let fiber = Self::resolve_focused_event_target(arena, fiber);
        let mut ctx = EventCtx::new();
        if let Some(entry) = arena.get(fiber)
            && let Some(view) = entry.view.clone()
        {
            ctx.set_current_fiber(fiber);
            view.handle_event(
                &UiEvent::Focus(event),
                &mut ctx,
                entry
                    .layout_rect
                    .unwrap_or_else(|| Rect::from_min_size(Point::ZERO, crate::layout::Size::ZERO)),
            );
        }
        self.input.apply(ctx.take_commands(), arena) || ctx.needs_paint()
    }

    fn update_hover_path(
        &mut self,
        arena: &FiberArena,
        root: FiberId,
        pointer_id: u64,
        position: Option<Point>,
    ) -> bool {
        let next_path = position
            .and_then(|point| Self::hit_test_walk(arena, root, point))
            .map(|target| Self::build_ancestor_path(arena, Some(target)))
            .unwrap_or_default();
        let common = self
            .hover_path
            .iter()
            .zip(&next_path)
            .take_while(|(left, right)| left == right)
            .count();

        let leaving: Vec<_> = self.hover_path[common..].iter().rev().copied().collect();
        let entering: Vec<_> = next_path[common..].to_vec();
        let mut needs_redraw = false;
        for fiber in leaving {
            if Self::accepts_pointer_boundaries(arena, fiber) {
                needs_redraw |= self.route_to_single(
                    arena,
                    fiber,
                    &UiEvent::PointerBoundary(crate::input::event::PointerBoundaryEvent {
                        pointer_id,
                        position,
                        kind: PointerBoundaryKind::Leave,
                    }),
                );
            }
        }
        for fiber in entering {
            if Self::accepts_pointer_boundaries(arena, fiber) {
                needs_redraw |= self.route_to_single(
                    arena,
                    fiber,
                    &UiEvent::PointerBoundary(crate::input::event::PointerBoundaryEvent {
                        pointer_id,
                        position,
                        kind: PointerBoundaryKind::Enter,
                    }),
                );
            }
        }
        self.hover_path = next_path;
        self.input.hovered = self.hover_path.last().copied();

        let cursor = self.hover_path.iter().rev().find_map(|fiber| {
            arena
                .get(*fiber)
                .and_then(|fiber| fiber.view.as_ref())
                .and_then(|view| view.pointer_cursor())
        });
        self.set_cursor(cursor);
        needs_redraw
    }

    fn set_cursor(&mut self, cursor: Option<CursorShape>) {
        if cursor != self.last_cursor {
            self.pending_cursor = Some(match cursor {
                Some(shape) => CursorEffect::Set(shape),
                None => CursorEffect::Reset,
            });
            self.last_cursor = cursor;
        }
    }

    fn find_shortcut(
        &self,
        arena: &FiberArena,
        root: FiberId,
        key: Key,
        modifiers: crate::input::event::Modifiers,
    ) -> Option<(FiberId, Box<dyn Any>)> {
        let chord = KeyChord::new(key, modifiers);
        if self.input.focused.is_some() {
            return Self::build_ancestor_path(arena, self.input.focused)
                .into_iter()
                .rev()
                .find_map(|source| {
                    arena
                        .get(source)
                        .and_then(|fiber| fiber.view.as_ref())
                        .and_then(|view| view.shortcut_action(chord))
                        .map(|action| (source, action))
                });
        }

        // With no focus, only the root's unambiguous single-child wrapper chain
        // participates. This supports global Actions -> Shortcuts composition
        // without activating branch-local bindings arbitrarily.
        let mut current = Some(root);
        let mut matched = None;
        while let Some(source) = current {
            let Some(fiber) = arena.get(source) else {
                break;
            };
            if let Some(action) = fiber
                .view
                .as_ref()
                .and_then(|view| view.shortcut_action(chord))
            {
                matched = Some((source, action));
            }
            current = match fiber.children.as_slice() {
                [child] => Some(*child),
                _ => None,
            };
        }
        matched
    }

    fn invoke_action(&self, arena: &FiberArena, source: FiberId, action: Box<dyn Any>) -> bool {
        let mut current = Some(source);
        while let Some(fiber) = current {
            if arena
                .get(fiber)
                .and_then(|fiber| fiber.view.as_ref())
                .is_some_and(|view| view.invoke_action(action.as_ref()))
            {
                return true;
            }
            current = arena.get(fiber).and_then(|fiber| fiber.parent);
        }
        false
    }

    fn focus_scope_for(arena: &FiberArena, target: FiberId) -> Option<FiberId> {
        let path = Self::build_ancestor_path(arena, Some(target));
        path.iter()
            .rev()
            .copied()
            .find(|fiber| {
                arena
                    .get(*fiber)
                    .and_then(|fiber| fiber.view.as_ref())
                    .is_some_and(|view| view.is_focus_scope())
            })
            .or_else(|| path.first().copied())
    }

    fn resolve_focused_event_target(arena: &FiberArena, mut target: FiberId) -> FiberId {
        loop {
            let Some(fiber) = arena.get(target) else {
                return target;
            };
            if !fiber
                .view
                .as_ref()
                .is_some_and(|view| view.delegates_focused_events())
            {
                return target;
            }
            match fiber.children.as_slice() {
                [child] => target = *child,
                _ => return target,
            }
        }
    }

    fn navigate_focus(&mut self, arena: &FiberArena, root: FiberId, forward: bool) -> bool {
        let path = if self.input.focused.is_some() {
            Self::build_ancestor_path(arena, self.input.focused)
        } else {
            vec![root]
        };
        let scope = path
            .iter()
            .rev()
            .copied()
            .find(|id| {
                arena
                    .get(*id)
                    .and_then(|fiber| fiber.view.as_ref())
                    .is_some_and(|view| view.is_focus_scope())
            })
            .unwrap_or(root);
        let next = Self::find_next_focusable(arena, scope, self.input.focused, forward);
        self.transition_focus_with_visibility(arena, next, true)
    }

    fn find_next_focusable(
        arena: &FiberArena,
        scope: FiberId,
        current: Option<FiberId>,
        forward: bool,
    ) -> Option<FiberId> {
        let mut candidates = Vec::new();
        Self::collect_focusable(arena, scope, &mut candidates);
        candidates.sort_by_key(|(_, order, source_order)| (*order, *source_order));
        let candidates: Vec<_> = candidates.into_iter().map(|(id, _, _)| id).collect();
        if candidates.is_empty() {
            return None;
        }
        let index = current.and_then(|current| candidates.iter().position(|id| *id == current));
        let next = match index {
            Some(index) if forward => (index + 1) % candidates.len(),
            Some(index) => (index + candidates.len() - 1) % candidates.len(),
            None if forward => 0,
            None => candidates.len() - 1,
        };
        Some(candidates[next])
    }

    fn collect_focusable(arena: &FiberArena, id: FiberId, out: &mut Vec<(FiberId, i32, usize)>) {
        let Some(fiber) = arena.get(id) else {
            return;
        };
        let view = fiber.view.as_ref();
        if let Some(metadata) = view.and_then(|view| view.focus_metadata()) {
            if metadata.enabled {
                out.push((id, metadata.order, out.len()));
            }
            // A node-level Focus wrapper owns one logical tab stop and merely
            // delegates delivery to its child. Disabled wrappers suppress that
            // logical stop rather than exposing a focusable descendant.
            if view.is_some_and(|view| view.delegates_focused_events()) {
                return;
            }
        }
        for child in &fiber.children {
            Self::collect_focusable(arena, *child, out);
        }
    }

    pub(crate) fn find_first_focusable(arena: &FiberArena, fiber: FiberId) -> Option<FiberId> {
        let view = arena.get(fiber)?.view.as_ref()?;
        if let Some(metadata) = view.focus_metadata() {
            if metadata.enabled {
                return Some(fiber);
            }
            if view.delegates_focused_events() {
                return None;
            }
        }
        for child in &arena.get(fiber)?.children {
            if let Some(found) = Self::find_first_focusable(arena, *child) {
                return Some(found);
            }
        }
        None
    }

    fn find_by_focus_handle(arena: &FiberArena, fiber: FiberId, handle: u64) -> Option<FiberId> {
        let entry = arena.get(fiber)?;
        if let Some(view) = entry.view.as_ref()
            && let Some(metadata) = view.focus_metadata()
            && view.delegates_focused_events()
        {
            return (metadata.enabled && metadata.handle == Some(handle)).then_some(fiber);
        }
        if entry
            .view
            .as_ref()
            .and_then(|view| view.focus_metadata())
            .is_some_and(|metadata| metadata.enabled && metadata.handle == Some(handle))
        {
            return Some(fiber);
        }
        for child in &entry.children {
            if let Some(found) = Self::find_by_focus_handle(arena, *child, handle) {
                return Some(found);
            }
        }
        None
    }

    pub(crate) fn tree_has_modal(arena: &FiberArena, fiber: FiberId) -> bool {
        let Some(fiber) = arena.get(fiber) else {
            return false;
        };
        if fiber
            .view
            .as_ref()
            .is_some_and(|view| view.is_modal_scope())
        {
            return true;
        }
        fiber
            .children
            .iter()
            .any(|child| Self::tree_has_modal(arena, *child))
    }

    pub(crate) fn hit_test_walk(
        arena: &FiberArena,
        fiber: FiberId,
        point: Point,
    ) -> Option<FiberId> {
        let entry = arena.get(fiber)?;
        let rect = entry.layout_rect?;
        if entry
            .view
            .as_ref()
            .and_then(|view| view.descendant_clip(rect))
            .is_some_and(|clip| clip.behavior() != ClipBehavior::None && !clip.contains(point))
        {
            return None;
        }
        for child in entry.children.iter().rev() {
            if let Some(hit) = Self::hit_test_walk(arena, *child, point) {
                return Some(hit);
            }
        }
        if !rect.contains(point) {
            return None;
        }
        let local_point = Point::new(point.x - rect.min.x, point.y - rect.min.y);
        let local_rect = Rect::from_min_size(Point::ZERO, rect.size());
        entry
            .view
            .as_ref()
            .filter(|view| view.hit_test(local_point, local_rect))
            .map(|_| fiber)
    }

    pub(crate) fn build_ancestor_path(arena: &FiberArena, target: Option<FiberId>) -> Vec<FiberId> {
        let mut path = Vec::new();
        let mut current = target;
        while let Some(fiber) = current {
            path.push(fiber);
            current = arena.get(fiber).and_then(|fiber| fiber.parent);
        }
        path.reverse();
        path
    }

    fn is_modal_block(arena: &FiberArena, ancestor: FiberId, target: Option<FiberId>) -> bool {
        let is_modal = arena
            .get(ancestor)
            .and_then(|fiber| fiber.view.as_ref())
            .is_some_and(|view| view.is_modal_scope());
        is_modal && !Self::is_descendant_of(arena, target, ancestor)
    }

    pub(crate) fn is_descendant_of(
        arena: &FiberArena,
        descendant: Option<FiberId>,
        ancestor: FiberId,
    ) -> bool {
        let mut current = descendant;
        while let Some(fiber) = current {
            if fiber == ancestor {
                return true;
            }
            current = arena.get(fiber).and_then(|fiber| fiber.parent);
        }
        false
    }

    fn accepts_pointer_boundaries(arena: &FiberArena, fiber: FiberId) -> bool {
        arena
            .get(fiber)
            .and_then(|fiber| fiber.view.as_ref())
            .is_some_and(|view| view.accepts_pointer_boundaries())
    }
    fn invoke_handler(arena: &FiberArena, fiber: FiberId, event: &UiEvent, ctx: &mut EventCtx) {
        let rect = arena.get(fiber).and_then(|fiber| fiber.layout_rect);
        let view = arena.get(fiber).and_then(|fiber| fiber.view.clone());
        if let (Some(view), Some(rect)) = (view, rect) {
            ctx.set_current_fiber(fiber);
            view.handle_event(event, ctx, rect);
        }
    }
}
