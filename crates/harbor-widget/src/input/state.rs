use crate::fiber::{FiberArena, FiberId};
use crate::input::event_ctx::EventCommand;
use std::collections::HashMap;

// ── InputState ──────────────────────────────────────────────────────────────

/// Per-Runtime input state: focus, hover, and pointer captures.
///
/// Commands accumulated by `EventCtx` during the event walk are applied
/// atomically via `apply()` after the walk completes.
pub struct InputState {
    pub focused: Option<FiberId>,
    pub hovered: Option<FiberId>,
    pointer_captures: HashMap<u64, FiberId>,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            focused: None,
            hovered: None,
            pointer_captures: HashMap::new(),
        }
    }

    /// Returns the fiber that has captured the given pointer, if any.
    pub fn captor(&self, pointer_id: u64) -> Option<FiberId> {
        self.pointer_captures.get(&pointer_id).copied()
    }

    /// Applies accumulated EventCtx commands after the event walk.
    /// Returns true if a paint invalidation was requested.
    pub(crate) fn apply(&mut self, commands: Vec<EventCommand>, arena: &FiberArena) -> bool {
        let mut needs_paint = false;
        for cmd in commands {
            match cmd {
                EventCommand::RequestFocus(id) => {
                    self.focused = Some(id);
                }
                EventCommand::CapturePointer { pointer_id, captor } => {
                    self.pointer_captures.insert(pointer_id, captor);
                }
                EventCommand::ReleasePointer(pointer_id) => {
                    self.pointer_captures.remove(&pointer_id);
                }
                EventCommand::NavigateFocus { scope, forward } => {
                    let next = Self::find_next_focusable(arena, scope, self.focused, forward);
                    self.focused = next;
                }
                EventCommand::InvalidatePaint => {
                    needs_paint = true;
                }
                EventCommand::StopPropagation => {
                    // Handled by EventCtx during the walk; no state change needed.
                }
            }
        }
        needs_paint
    }

    /// Clears the focused fiber if it is no longer present in the arena.
    pub(crate) fn clear_focus_if_dead(&mut self, arena: &FiberArena) {
        if let Some(fid) = self.focused
            && !arena.contains(fid)
        {
            self.focused = None;
        }
    }

    /// Removes pointer captures for fibers that are no longer in the arena.
    pub(crate) fn clear_capture_if_dead(&mut self, arena: &FiberArena) {
        self.pointer_captures.retain(|_, fid| arena.contains(*fid));
    }

    /// DFS walk to collect all focusable fibers in a subtree, then return
    /// the next (or previous) focusable relative to `current`.
    fn find_next_focusable(
        arena: &FiberArena,
        scope: FiberId,
        current: Option<FiberId>,
        forward: bool,
    ) -> Option<FiberId> {
        let mut focusable = Vec::new();
        Self::collect_focusable(arena, scope, &mut focusable);

        if focusable.is_empty() {
            return None;
        }

        let current_idx = current.and_then(|cid| focusable.iter().position(|&id| id == cid));

        let next_idx = match current_idx {
            Some(idx) => {
                if forward {
                    (idx + 1) % focusable.len()
                } else {
                    (idx + focusable.len() - 1) % focusable.len()
                }
            }
            None => {
                if forward {
                    0
                } else {
                    focusable.len() - 1
                }
            }
        };

        Some(focusable[next_idx])
    }

    /// DFS traversal collecting focusable fibers within a subtree.
    fn collect_focusable(arena: &FiberArena, fiber_id: FiberId, out: &mut Vec<FiberId>) {
        let fiber = match arena.get(fiber_id) {
            Some(f) => f,
            None => return,
        };

        // Check self (exclude FocusScope containers themselves)
        if let Some(ref view) = fiber.view
            && view.is_focusable()
        {
            out.push(fiber_id);
        }

        // Recurse children in DFS order.
        // Clone necessary to release the immutable borrow before recursing.
        let children = fiber.children.clone();
        for &child_id in &children {
            Self::collect_focusable(arena, child_id, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fiber::{Fiber, FiberArena};

    fn make_fiber(arena: &mut FiberArena) -> FiberId {
        arena.insert(Fiber::new(None, std::any::TypeId::of::<()>(), None))
    }

    #[test]
    fn new_state_empty() {
        let state = InputState::new();
        assert!(state.focused.is_none());
        assert!(state.hovered.is_none());
        assert!(state.captor(0).is_none());
    }

    #[test]
    fn apply_request_focus() {
        let mut state = InputState::new();
        let mut arena = FiberArena::new();
        let fid = make_fiber(&mut arena);

        let needs_paint = state.apply(vec![EventCommand::RequestFocus(fid)], &arena);
        assert!(!needs_paint);
        assert_eq!(state.focused, Some(fid));
    }

    #[test]
    fn apply_capture_pointer() {
        let mut state = InputState::new();
        let mut arena = FiberArena::new();
        let fid = make_fiber(&mut arena);

        let needs_paint = state.apply(
            vec![EventCommand::CapturePointer {
                pointer_id: 1,
                captor: fid,
            }],
            &arena,
        );
        assert!(!needs_paint);
        assert_eq!(state.captor(1), Some(fid));
    }

    #[test]
    fn apply_release_pointer() {
        let mut state = InputState::new();
        let mut arena = FiberArena::new();
        let fid = make_fiber(&mut arena);

        state.apply(
            vec![EventCommand::CapturePointer {
                pointer_id: 1,
                captor: fid,
            }],
            &arena,
        );
        assert_eq!(state.captor(1), Some(fid));

        let needs_paint = state.apply(vec![EventCommand::ReleasePointer(1)], &arena);
        assert!(!needs_paint);
        assert!(state.captor(1).is_none());
    }

    #[test]
    fn apply_invalidate_paint() {
        let mut state = InputState::new();
        let arena = FiberArena::new();
        let needs_paint = state.apply(vec![EventCommand::InvalidatePaint], &arena);
        assert!(needs_paint);
    }

    #[test]
    fn clear_focus_if_dead() {
        let mut state = InputState::new();
        let mut arena = FiberArena::new();
        let fid = make_fiber(&mut arena);

        state.focused = Some(fid);
        state.clear_focus_if_dead(&arena);
        assert_eq!(state.focused, Some(fid)); // still alive

        arena.remove(fid);
        state.clear_focus_if_dead(&arena);
        assert!(state.focused.is_none());
    }

    #[test]
    fn clear_capture_if_dead() {
        let mut state = InputState::new();
        let mut arena = FiberArena::new();
        let fid = make_fiber(&mut arena);

        state.apply(
            vec![EventCommand::CapturePointer {
                pointer_id: 1,
                captor: fid,
            }],
            &arena,
        );
        assert_eq!(state.captor(1), Some(fid));

        arena.remove(fid);
        state.clear_capture_if_dead(&arena);
        assert!(state.captor(1).is_none());
    }

    #[test]
    fn apply_multiple_commands() {
        let mut state = InputState::new();
        let mut arena = FiberArena::new();
        let fid = make_fiber(&mut arena);

        let needs_paint = state.apply(
            vec![
                EventCommand::RequestFocus(fid),
                EventCommand::InvalidatePaint,
                EventCommand::StopPropagation,
            ],
            &arena,
        );
        assert!(needs_paint);
        assert_eq!(state.focused, Some(fid));
    }

    #[test]
    fn capture_pointer_overwrites() {
        let mut state = InputState::new();
        let mut arena = FiberArena::new();
        let fid = make_fiber(&mut arena);
        let fid2 = make_fiber(&mut arena);

        state.apply(
            vec![EventCommand::CapturePointer {
                pointer_id: 1,
                captor: fid,
            }],
            &arena,
        );
        assert_eq!(state.captor(1), Some(fid));

        state.apply(
            vec![EventCommand::CapturePointer {
                pointer_id: 1,
                captor: fid2,
            }],
            &arena,
        );
        assert_eq!(state.captor(1), Some(fid2)); // overwrites
    }
}
