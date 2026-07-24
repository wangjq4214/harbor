use crate::fiber::FiberId;

// ── EventHandled ────────────────────────────────────────────────────────────

/// Whether a widget consumed an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventHandled {
    Ignored,
    Handled,
}

// ── EventCommand ────────────────────────────────────────────────────────────

/// Commands that event handlers can emit to be applied after the event walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EventCommand {
    RequestFocus(FiberId),
    CapturePointer { pointer_id: u64, captor: FiberId },
    ReleasePointer(u64),
    NavigateFocus { scope: FiberId, forward: bool },
    InvalidatePaint,
    StopPropagation,
}

// ── EventCtx ────────────────────────────────────────────────────────────────

/// Per-event context for handler methods. Commands are accumulated during
/// the event walk and applied atomically by `InputState::apply` afterward.
pub struct EventCtx {
    commands: Vec<EventCommand>,
    propagation_stopped: bool,
    needs_paint: bool,
    current_fiber: Option<FiberId>,
}

impl EventCtx {
    pub fn new() -> Self {
        EventCtx {
            commands: Vec::new(),
            propagation_stopped: false,
            needs_paint: false,
            current_fiber: None,
        }
    }

    /// Set by Runtime before calling handle_event, so capture_pointer knows
    /// which fiber is the captor.
    pub(crate) fn set_current_fiber(&mut self, id: FiberId) {
        self.current_fiber = Some(id);
    }

    /// Request that focus be moved to the given fiber after the event walk.
    pub fn request_focus(&mut self, id: FiberId) {
        self.commands.push(EventCommand::RequestFocus(id));
    }

    /// Capture all subsequent pointer events for the given pointer_id,
    /// routing them to the current fiber regardless of hit test results.
    pub fn capture_pointer(&mut self, pointer_id: u64) {
        if let Some(fid) = self.current_fiber {
            self.commands.push(EventCommand::CapturePointer {
                pointer_id,
                captor: fid,
            });
        }
    }

    /// Release a previously captured pointer.
    pub fn release_pointer(&mut self, pointer_id: u64) {
        self.commands.push(EventCommand::ReleasePointer(pointer_id));
    }

    /// Mark the current widget as needing a repaint.
    pub fn invalidate_paint(&mut self) {
        self.needs_paint = true;
        self.commands.push(EventCommand::InvalidatePaint);
    }

    /// Stop event propagation. No further handlers in the current phase
    /// or subsequent phases will receive this event.
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
        self.commands.push(EventCommand::StopPropagation);
    }

    /// Request focus navigation within a FocusScope.
    /// Called by FocusScope when it intercepts Tab/Shift+Tab.
    pub(crate) fn navigate_focus(&mut self, forward: bool) {
        if let Some(fid) = self.current_fiber {
            self.commands.push(EventCommand::NavigateFocus {
                scope: fid,
                forward,
            });
        }
    }

    /// Drain accumulated commands for post-walk application.
    pub(crate) fn take_commands(&mut self) -> Vec<EventCommand> {
        std::mem::take(&mut self.commands)
    }

    /// Whether stop_propagation was called during this walk.
    pub(crate) fn is_propagation_stopped(&self) -> bool {
        self.propagation_stopped
    }

    /// Whether any handler requested a paint invalidation.
    pub(crate) fn needs_paint(&self) -> bool {
        self.needs_paint
    }
}

impl Default for EventCtx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_fiber_id() -> FiberId {
        use crate::fiber::{Fiber, FiberArena};
        let mut arena = FiberArena::new();
        arena.insert(Fiber::new(None, std::any::TypeId::of::<()>(), None))
    }

    #[test]
    fn empty_ctx_no_commands() {
        let mut ctx = EventCtx::new();
        assert!(ctx.take_commands().is_empty());
        assert!(!ctx.is_propagation_stopped());
        assert!(!ctx.needs_paint());
    }

    #[test]
    fn request_focus_records_command() {
        let mut ctx = EventCtx::new();
        let fid = dummy_fiber_id();
        ctx.request_focus(fid);
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], EventCommand::RequestFocus(fid));
    }

    #[test]
    fn capture_and_release_pointer() {
        let mut ctx = EventCtx::new();
        let fid = dummy_fiber_id();
        ctx.set_current_fiber(fid);
        ctx.capture_pointer(1);
        ctx.release_pointer(1);
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 2);
        assert_eq!(
            cmds[0],
            EventCommand::CapturePointer {
                pointer_id: 1,
                captor: fid
            }
        );
        assert_eq!(cmds[1], EventCommand::ReleasePointer(1));
    }

    #[test]
    fn capture_pointer_without_current_fiber_is_noop() {
        let mut ctx = EventCtx::new();
        ctx.capture_pointer(1);
        let cmds = ctx.take_commands();
        assert!(cmds.is_empty());
    }

    #[test]
    fn stop_propagation_sets_flag() {
        let mut ctx = EventCtx::new();
        assert!(!ctx.is_propagation_stopped());
        ctx.stop_propagation();
        assert!(ctx.is_propagation_stopped());
        // Command also recorded
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], EventCommand::StopPropagation);
    }

    #[test]
    fn invalidate_paint_sets_flag() {
        let mut ctx = EventCtx::new();
        assert!(!ctx.needs_paint());
        ctx.invalidate_paint();
        assert!(ctx.needs_paint());
    }

    #[test]
    fn take_commands_drains() {
        let mut ctx = EventCtx::new();
        ctx.invalidate_paint();
        ctx.stop_propagation();
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 2);
        // Second take is empty
        assert!(ctx.take_commands().is_empty());
    }

    #[test]
    fn multiple_commands_in_order() {
        let mut ctx = EventCtx::new();
        let fid = dummy_fiber_id();
        ctx.set_current_fiber(fid);
        ctx.request_focus(fid);
        ctx.capture_pointer(0);
        ctx.invalidate_paint();
        ctx.stop_propagation();
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 4);
    }
}
