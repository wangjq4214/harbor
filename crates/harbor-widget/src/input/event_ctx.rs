use crate::fiber::FiberId;
use crate::input::event::UiEvent;
use crate::scene::primitive::ExternalDrawId;

// ── EventHandled ────────────────────────────────────────────────────────────

/// Whether to continue the current event walk. Independent hover/focus
/// notifications and later input events have their own delivery/ownership rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventHandled {
    /// Continue propagation, retaining any side effects already requested.
    Ignored,
    /// Stop remaining handlers/phases after this handler; still commit its work.
    /// Consumption alone does not request a repaint.
    Handled,
}

// ── EventCommand ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EventPhase {
    Capture,
    Target,
    Bubble,
    #[default]
    Direct,
}

/// Commands that event handlers can emit to be applied after the event walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EventCommand {
    RequestFocus(FiberId),
    RequestFocusWithVisibility { id: FiberId, focus_visible: bool },
    CapturePointer { pointer_id: u64, captor: FiberId },
    ReleasePointer(u64),
    NavigateFocus { scope: FiberId, forward: bool },
    InvalidatePaint,
}

// ── EventCtx ────────────────────────────────────────────────────────────────

/// Deferred side-effect requests for the current handler walk. Work is submitted
/// after routing, even when a handler consumes the event. Propagation is controlled
/// solely by the handler's [`EventHandled`] return, not by mutating this context.
///
/// The former explicit stopping method is not part of the public API:
/// ```compile_fail
/// use harbor_widget::input::event_ctx::EventCtx;
/// EventCtx::new().stop_propagation();
/// ```
pub struct EventCtx {
    commands: Vec<EventCommand>,
    needs_paint: bool,
    clipboard_write: Option<String>,
    editor_clipboard: Option<crate::effects::EditorClipboardRequest>,
    clipboard_request_id: u64,
    current_fiber: Option<FiberId>,
    phase: EventPhase,
    focus_navigation_enabled: bool,
    external_input: Vec<(ExternalDrawId, UiEvent)>,
    consumed_key: Option<(crate::input::event::Key, Option<FiberId>)>,
}

impl EventCtx {
    pub fn new() -> Self {
        // Allocated by the stable routing runtime, not by a reloadable editor.
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let clipboard_request_id = NEXT.fetch_add(1, Ordering::Relaxed);
        EventCtx {
            commands: Vec::new(),
            needs_paint: false,
            clipboard_write: None,
            editor_clipboard: None,
            clipboard_request_id,
            current_fiber: None,
            phase: EventPhase::Direct,
            focus_navigation_enabled: true,
            external_input: Vec::new(),
            consumed_key: None,
        }
    }

    /// Set by Runtime before calling handle_event, so capture_pointer knows
    /// which fiber is the captor.
    pub(crate) fn set_current_fiber(&mut self, id: FiberId) {
        self.current_fiber = Some(id);
    }

    pub(crate) fn set_phase(&mut self, phase: EventPhase) {
        self.phase = phase;
    }

    pub(crate) fn is_capture_phase(&self) -> bool {
        self.phase == EventPhase::Capture
    }

    /// Request that focus be moved to the given fiber after the event walk.
    pub fn request_focus(&mut self, id: FiberId) {
        self.commands.push(EventCommand::RequestFocus(id));
    }

    /// Request focus without showing a keyboard focus ring, used by pointer activation.
    pub(crate) fn request_focus_from_pointer(&mut self) {
        if let Some(id) = self.current_fiber {
            self.commands
                .push(EventCommand::RequestFocusWithVisibility {
                    id,
                    focus_visible: false,
                });
        }
    }

    /// Returns the fiber whose handler is currently running.
    pub(crate) fn current_fiber(&self) -> Option<FiberId> {
        self.current_fiber
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

    /// Request that the host write text to the system clipboard after routing.
    pub fn write_clipboard(&mut self, text: impl Into<String>) {
        self.clipboard_write = Some(text.into());
    }

    pub(crate) fn take_clipboard_write(&mut self) -> Option<String> {
        self.clipboard_write.take()
    }

    /// Request a result-bearing editor clipboard operation after routing.
    pub(crate) fn editor_clipboard(&mut self, write: Option<String>) -> u64 {
        let id = self.clipboard_request_id;
        self.editor_clipboard = Some(crate::effects::EditorClipboardRequest { id, write });
        id
    }

    pub(crate) fn take_editor_clipboard(
        &mut self,
    ) -> Option<crate::effects::EditorClipboardRequest> {
        self.editor_clipboard.take()
    }

    /// Retain an editor gesture across focus/unmount until release (or a new
    /// native press). Repeats may stay local to the current editor, never a new target.
    pub(crate) fn consume_key_press(
        &mut self,
        key: crate::input::event::Key,
        repeat_locally: bool,
    ) {
        self.consumed_key = Some((
            key,
            if repeat_locally {
                self.current_fiber
            } else {
                None
            },
        ));
    }

    pub(crate) fn take_consumed_key(
        &mut self,
    ) -> Option<(crate::input::event::Key, Option<FiberId>)> {
        self.consumed_key.take()
    }
    /// Mark the current widget as needing a repaint.
    pub fn invalidate_paint(&mut self) {
        self.needs_paint = true;
        self.commands.push(EventCommand::InvalidatePaint);
    }

    pub(crate) fn suppress_focus_navigation(&mut self) {
        self.focus_navigation_enabled = false;
    }

    /// Request focus navigation within a FocusScope. Returns whether a command
    /// was queued; a suppressed Tab must remain available to the terminal.
    pub(crate) fn navigate_focus(&mut self, forward: bool) -> bool {
        if self.focus_navigation_enabled
            && let Some(fid) = self.current_fiber
        {
            self.commands.push(EventCommand::NavigateFocus {
                scope: fid,
                forward,
            });
            true
        } else {
            false
        }
    }

    /// Drain accumulated commands for post-walk application.
    pub(crate) fn take_commands(&mut self) -> Vec<EventCommand> {
        std::mem::take(&mut self.commands)
    }

    /// Whether any handler requested a paint invalidation.
    pub fn needs_paint(&self) -> bool {
        self.needs_paint
    }

    /// Queues an input event targeting an external draw handler for deferred delivery.
    pub fn queue_external_input(&mut self, id: ExternalDrawId, event: UiEvent) {
        self.external_input.push((id, event));
    }

    pub(crate) fn take_external_input(&mut self) -> Vec<(ExternalDrawId, UiEvent)> {
        std::mem::take(&mut self.external_input)
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
        ctx.request_focus(dummy_fiber_id());
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
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 3);
    }
}
