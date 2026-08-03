//! Cross-thread wake events and frame scheduling for the winit event loop.
//!
//! Kept separate from `app` so host I/O (`pty`) does not depend on the shell.

use std::time::Instant;

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// The terminal reader queued output for UI-thread parsing.
    TerminalOutputReady,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum FrameActivity {
    #[default]
    Idle,
    Deadline,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameControlFlow {
    Wait,
    WaitUntil(Instant),
    Poll,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RedrawReason {
    TerminalOutput,
    Input,
    Resize,
    SurfaceRecovery,
    SurfaceSuboptimal,
    Active,
}

#[derive(Debug, Default)]
pub(crate) struct FrameScheduler {
    activity: FrameActivity,
    deadline: Option<Instant>,
    redraw_pending: bool,
    /// Latest runtime-requested control flow, retained across callbacks.
    control_flow_override: Option<FrameControlFlow>,
    /// True while the native window cannot acquire a drawable surface.
    suspended: bool,
}

impl FrameScheduler {
    pub(crate) fn wake(&mut self, reason: RedrawReason) -> bool {
        if reason == RedrawReason::Active {
            self.activity = FrameActivity::Active;
        }
        if self.suspended {
            return false;
        }
        let was_pending = self.redraw_pending;
        self.redraw_pending = true;
        !was_pending
    }

    pub(crate) fn redraw_requested(&mut self) {
        self.redraw_pending = false;
    }

    pub(crate) fn set_active(&mut self, active: bool) {
        if active {
            self.activity = FrameActivity::Active;
        } else if self.deadline.is_some() {
            self.activity = FrameActivity::Deadline;
        } else {
            self.activity = FrameActivity::Idle;
        }
    }

    pub(crate) fn set_deadline(&mut self, deadline: Option<Instant>) {
        self.deadline = deadline;
        if self.activity != FrameActivity::Active {
            self.activity = if deadline.is_some() {
                FrameActivity::Deadline
            } else {
                FrameActivity::Idle
            };
        }
    }

    pub(crate) fn should_request_continuous_redraw(&self) -> bool {
        !self.suspended && self.activity == FrameActivity::Active && !self.redraw_pending
    }

    pub(crate) fn set_drawable(&mut self, drawable: bool) {
        self.suspended = !drawable;
        if !drawable {
            self.redraw_pending = false;
        }
    }

    pub(crate) fn set_control_flow(&mut self, control_flow: FrameControlFlow) {
        self.control_flow_override = Some(control_flow);
    }

    pub(crate) fn control_flow(&self) -> FrameControlFlow {
        if self.suspended {
            return FrameControlFlow::Wait;
        }
        if let Some(control_flow) = self.control_flow_override {
            return control_flow;
        }
        match self.activity {
            FrameActivity::Active => FrameControlFlow::Poll,
            FrameActivity::Deadline => FrameControlFlow::WaitUntil(
                self.deadline
                    .expect("deadline activity requires a deadline"),
            ),
            FrameActivity::Idle => FrameControlFlow::Wait,
        }
    }

    #[cfg(test)]
    fn activity(&self) -> FrameActivity {
        self.activity
    }

    #[cfg(test)]
    fn redraw_pending(&self) -> bool {
        self.redraw_pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn coalesces_wakes_until_redraw_is_observed() {
        let mut scheduler = FrameScheduler::default();

        assert!(scheduler.wake(RedrawReason::TerminalOutput));
        assert!(!scheduler.wake(RedrawReason::TerminalOutput));
        assert!(scheduler.redraw_pending());

        scheduler.redraw_requested();
        assert!(!scheduler.redraw_pending());
        assert!(scheduler.wake(RedrawReason::Resize));
    }

    #[test]
    fn independent_window_schedulers_do_not_suppress_each_other() {
        let mut main = FrameScheduler::default();
        let mut confirmation = FrameScheduler::default();

        assert!(main.wake(RedrawReason::Input));
        assert!(confirmation.wake(RedrawReason::Resize));
        assert!(main.redraw_pending());
        assert!(confirmation.redraw_pending());

        main.redraw_requested();
        assert!(!main.redraw_pending());
        assert!(confirmation.redraw_pending());
    }

    #[test]
    fn each_window_can_rewake_after_only_its_own_redraw_is_observed() {
        // Arrange: both native windows have independently pending redraws.
        let mut main = FrameScheduler::default();
        let mut confirmation = FrameScheduler::default();
        assert!(main.wake(RedrawReason::Input));
        assert!(confirmation.wake(RedrawReason::Resize));

        // Act: present the main window, then request another main frame.
        main.redraw_requested();
        assert!(main.wake(RedrawReason::Input));

        // Assert: the confirmation request stayed coalesced and can be
        // reissued only after confirmation itself presents.
        assert!(!confirmation.wake(RedrawReason::Resize));
        confirmation.redraw_requested();
        assert!(confirmation.wake(RedrawReason::Resize));
        assert!(main.redraw_pending());
    }

    #[test]
    fn should_rewake_confirmation_after_surface_recovery_redraw_is_observed() {
        // Arrange
        let mut confirmation = FrameScheduler::default();

        // Act
        let first_wake = confirmation.wake(RedrawReason::SurfaceRecovery);
        let coalesced_wake = confirmation.wake(RedrawReason::SurfaceRecovery);
        confirmation.redraw_requested();
        let next_wake = confirmation.wake(RedrawReason::SurfaceRecovery);

        // Assert
        assert!(first_wake);
        assert!(!coalesced_wake);
        assert!(next_wake);
        assert!(confirmation.redraw_pending());
    }

    #[test]
    fn should_bound_confirmation_recovery_and_suboptimal_wakes_until_presented() {
        // Arrange
        let mut confirmation = FrameScheduler::default();

        // Act
        let recovery_wake = confirmation.wake(RedrawReason::SurfaceRecovery);
        let suboptimal_wake = confirmation.wake(RedrawReason::SurfaceSuboptimal);
        confirmation.redraw_requested();
        let retry_wake = confirmation.wake(RedrawReason::SurfaceSuboptimal);

        // Assert
        assert!(recovery_wake);
        assert!(!suboptimal_wake);
        assert!(retry_wake);
    }

    #[test]
    fn selects_idle_deadline_and_active_control_flows() {
        let mut scheduler = FrameScheduler::default();
        assert_eq!(scheduler.control_flow(), FrameControlFlow::Wait);

        let deadline = Instant::now() + Duration::from_millis(10);
        scheduler.set_deadline(Some(deadline));
        assert_eq!(scheduler.activity(), FrameActivity::Deadline);
        assert_eq!(
            scheduler.control_flow(),
            FrameControlFlow::WaitUntil(deadline)
        );

        scheduler.set_active(true);
        assert_eq!(scheduler.control_flow(), FrameControlFlow::Poll);

        scheduler.set_active(false);
        scheduler.set_deadline(None);
        assert_eq!(scheduler.activity(), FrameActivity::Idle);
        assert_eq!(scheduler.control_flow(), FrameControlFlow::Wait);
    }

    #[test]
    fn runtime_control_flow_override_persists_across_activity_changes() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let deadline = Instant::now() + Duration::from_secs(1);

        // Act
        scheduler.set_control_flow(FrameControlFlow::WaitUntil(deadline));
        scheduler.set_active(true);
        scheduler.set_deadline(Some(deadline + Duration::from_secs(1)));

        // Assert
        assert_eq!(
            scheduler.control_flow(),
            FrameControlFlow::WaitUntil(deadline)
        );

        // Act: a later runtime request replaces the retained override.
        scheduler.set_control_flow(FrameControlFlow::Poll);
        scheduler.set_active(false);
        scheduler.set_deadline(None);

        // Assert
        assert_eq!(scheduler.control_flow(), FrameControlFlow::Poll);
    }

    #[test]
    fn suspended_scheduler_drops_pending_redraw_and_rewakes_on_restore() {
        let mut scheduler = FrameScheduler::default();
        assert!(scheduler.wake(RedrawReason::Input));

        scheduler.set_drawable(false);
        assert!(!scheduler.redraw_pending());
        assert!(!scheduler.wake(RedrawReason::Resize));
        assert_eq!(scheduler.control_flow(), FrameControlFlow::Wait);

        scheduler.set_drawable(true);
        assert!(scheduler.wake(RedrawReason::Resize));
        assert!(scheduler.redraw_pending());
    }

    #[test]
    fn active_redraw_loop_stops_when_activity_ends() {
        let mut scheduler = FrameScheduler::default();
        scheduler.set_active(true);
        assert!(scheduler.should_request_continuous_redraw());

        scheduler.wake(RedrawReason::Active);
        assert!(!scheduler.should_request_continuous_redraw());
        scheduler.redraw_requested();
        assert!(scheduler.should_request_continuous_redraw());

        scheduler.set_active(false);
        assert!(!scheduler.should_request_continuous_redraw());
        assert_eq!(scheduler.control_flow(), FrameControlFlow::Wait);
    }
}
