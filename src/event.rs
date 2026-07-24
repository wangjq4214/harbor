//! Cross-thread wake events and frame scheduling for the winit event loop.
//!
//! Kept separate from `app` so host I/O (`pty`) does not depend on the shell.

use std::time::Instant;

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// The terminal worker published a new snapshot or status.
    WorkerUpdateReady,
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
    WorkerUpdate,
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
}

impl FrameScheduler {
    pub(crate) fn wake(&mut self, reason: RedrawReason) -> bool {
        if reason == RedrawReason::Active {
            self.activity = FrameActivity::Active;
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
        self.activity == FrameActivity::Active && !self.redraw_pending
    }

    pub(crate) fn control_flow(&self) -> FrameControlFlow {
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

        assert!(scheduler.wake(RedrawReason::WorkerUpdate));
        assert!(!scheduler.wake(RedrawReason::WorkerUpdate));
        assert!(scheduler.redraw_pending());

        scheduler.redraw_requested();
        assert!(!scheduler.redraw_pending());
        assert!(scheduler.wake(RedrawReason::Resize));
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
