use crate::fiber::{FiberArena, FiberId};
use crate::input::event_ctx::EventCommand;
use std::collections::HashMap;

// ── InputState ──────────────────────────────────────────────────────────────

/// Per-Runtime input state: focus, hover, and pointer captures.
///
/// Commands accumulated by `EventCtx` during the event walk are applied
/// atomically via `apply()` after the walk completes.
pub struct InputState {
    pub(crate) focused: Option<FiberId>,
    pub(crate) hovered: Option<FiberId>,
    pub(crate) focus_visible: bool,
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
            focus_visible: false,
            pointer_captures: HashMap::new(),
        }
    }

    /// Fiber currently receiving keyboard / focus events, if any.
    pub fn focused(&self) -> Option<FiberId> {
        self.focused
    }

    /// Whether the current focus target should render a keyboard focus affordance.
    pub(crate) fn focus_visible(&self) -> bool {
        self.focus_visible
    }

    /// Fiber currently under the pointer, if tracked.
    pub fn hovered(&self) -> Option<FiberId> {
        self.hovered
    }

    /// Returns the fiber that has captured the given pointer, if any.
    pub fn captor(&self, pointer_id: u64) -> Option<FiberId> {
        self.pointer_captures.get(&pointer_id).copied()
    }

    /// Returns all pointer ids currently captured by widgets.
    pub(crate) fn captured_pointer_ids(&self) -> Vec<u64> {
        self.pointer_captures.keys().copied().collect()
    }

    pub(crate) fn capture_pointer(&mut self, pointer_id: u64, captor: FiberId) {
        self.pointer_captures.insert(pointer_id, captor);
    }

    pub(crate) fn release_pointer(&mut self, pointer_id: u64) {
        self.pointer_captures.remove(&pointer_id);
    }

    /// Applies accumulated EventCtx commands after the event walk.
    /// Returns true if a paint invalidation was requested.
    pub(crate) fn apply(&mut self, commands: Vec<EventCommand>, _arena: &FiberArena) -> bool {
        let mut needs_paint = false;
        for cmd in commands {
            match cmd {
                EventCommand::RequestFocus(id) => {
                    if self.focused != Some(id) {
                        self.focused = Some(id);
                        self.focus_visible = true;
                    }
                }
                EventCommand::RequestFocusWithVisibility { id, focus_visible } => {
                    self.focused = Some(id);
                    self.focus_visible = focus_visible;
                }
                EventCommand::CapturePointer { pointer_id, captor } => {
                    self.capture_pointer(pointer_id, captor);
                }
                EventCommand::ReleasePointer(pointer_id) => {
                    self.release_pointer(pointer_id);
                }
                EventCommand::NavigateFocus { .. } => {
                    // Focus navigation over the fiber hierarchy is handled by EventRouter.
                }
                EventCommand::InvalidatePaint => {
                    needs_paint = true;
                }
            }
        }
        needs_paint
    }

    /// Clears the focused fiber if it is no longer present in the arena.
    #[cfg(test)]
    pub(crate) fn clear_focus_if_dead(&mut self, arena: &FiberArena) {
        if let Some(fid) = self.focused
            && !arena.contains(fid)
        {
            self.focused = None;
            self.focus_visible = false;
        }
    }

    /// Clears focus when the current live target no longer satisfies its contract.
    pub(crate) fn clear_focus_if(&mut self, should_clear: impl FnOnce(FiberId) -> bool) -> bool {
        if self.focused.is_some_and(should_clear) {
            self.focused = None;
            self.focus_visible = false;
            true
        } else {
            false
        }
    }

    /// Removes pointer captures for fibers that are no longer in the arena.
    #[cfg(test)]
    pub(crate) fn clear_capture_if_dead(&mut self, arena: &FiberArena) {
        self.pointer_captures.retain(|_, fid| arena.contains(*fid));
    }

    /// Removes captures whose still-live targets have become ineligible.
    pub(crate) fn clear_capture_if(&mut self, mut retains: impl FnMut(FiberId) -> bool) {
        self.pointer_captures.retain(|_, fid| retains(*fid));
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
