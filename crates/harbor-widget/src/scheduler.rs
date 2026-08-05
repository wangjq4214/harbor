//! Platform-neutral frame scheduling state machine.
//!
//! Turns runtime work, redraw observation, frame completion, activity, and
//! deadlines into redraw and control-flow effects without winit or App types.

use crate::effects::{ControlFlowEffect, RuntimeEffects};
use std::time::Instant;

/// Per-window redraw coalescing and idle wait-policy state.
#[derive(Debug)]
pub(crate) struct FrameScheduler {
    redraw_pending: bool,
    drawable: bool,
    runtime_deadline: Option<Instant>,
    active: bool,
    wake_bits: u32,
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self {
            redraw_pending: false,
            drawable: true,
            runtime_deadline: None,
            active: false,
            wake_bits: 0,
        }
    }
}

impl FrameScheduler {
    /// Diagnostic bit for runtime-originated invalidation (events, dirty Fibers).
    pub(crate) const RUNTIME_WAKE: u32 = 1 << 0;
    /// Diagnostic bit for source-agnostic host external invalidation.
    pub(crate) const EXTERNAL_WAKE: u32 = 1 << 1;
    /// Diagnostic bit for a due animation/runtime deadline wake.
    pub(crate) const DEADLINE_WAKE: u32 = 1 << 2;
    /// Diagnostic bit for active-animation continuation after a frame.
    pub(crate) const CONTINUATION_WAKE: u32 = 1 << 3;
    /// Diagnostic bit for host presentation/surface retry frames.
    pub(crate) const HOST_RETRY_WAKE: u32 = 1 << 4;

    /// Folds one runtime effect batch into scheduler state and returns the
    /// host-facing edge (at most one outstanding redraw request).
    pub(crate) fn schedule(
        &mut self,
        mut effects: RuntimeEffects,
        wake_bit: u32,
    ) -> RuntimeEffects {
        self.fold_control_flow(effects.control_flow);

        if effects.request_redraw {
            self.wake_bits |= wake_bit;
            let edge = self.drawable && !self.redraw_pending;
            effects.request_redraw = edge;
            if self.drawable {
                self.redraw_pending = true;
            }
        }
        effects
    }

    /// Observes frame start: clears the outstanding redraw edge, folds the
    /// runtime update's control-flow state, and suppresses a redundant redraw
    /// because the current frame consumes that work.
    pub(crate) fn frame_started(&mut self, mut effects: RuntimeEffects) -> RuntimeEffects {
        self.redraw_pending = false;
        self.wake_bits = 0;
        self.fold_control_flow(effects.control_flow);
        effects.request_redraw = false;
        effects
    }

    /// Observes successful presentation. Active animation may request one
    /// continuation frame and keep `Poll`; idle completion returns `Wait`.
    pub(crate) fn frame_completed(&mut self, _now: Instant) -> RuntimeEffects {
        if self.active {
            let request_redraw = self.drawable && !self.redraw_pending;
            if request_redraw {
                self.wake_bits |= Self::CONTINUATION_WAKE;
                self.redraw_pending = true;
            }
            return RuntimeEffects {
                request_redraw,
                control_flow: Some(ControlFlowEffect::Poll),
                ..RuntimeEffects::default()
            };
        }

        RuntimeEffects {
            control_flow: Some(ControlFlowEffect::Wait),
            ..RuntimeEffects::default()
        }
    }

    /// Calculates idle-turn redraw and wait policy, optionally considering a
    /// host-supplied deadline that must not overwrite the runtime deadline.
    pub(crate) fn about_to_wait(
        &mut self,
        now: Instant,
        host_deadline: Option<Instant>,
    ) -> RuntimeEffects {
        if !self.drawable {
            if let Some(deadline) = self.runtime_deadline
                && now >= deadline
            {
                self.runtime_deadline = None;
                self.wake_bits |= Self::DEADLINE_WAKE;
            }
            return RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Wait),
                ..RuntimeEffects::default()
            };
        }

        let mut request_redraw = false;
        if self.active && !self.redraw_pending {
            self.wake_bits |= Self::CONTINUATION_WAKE;
            self.redraw_pending = true;
            request_redraw = true;
        }

        if let Some(deadline) = self.runtime_deadline
            && now >= deadline
        {
            self.runtime_deadline = None;
            if !self.redraw_pending {
                self.wake_bits |= Self::DEADLINE_WAKE;
                self.redraw_pending = true;
                request_redraw = true;
            }
        }

        let control_flow = if self.active {
            ControlFlowEffect::Poll
        } else {
            match effective_deadline(self.runtime_deadline, host_deadline) {
                Some(deadline) if deadline > now => ControlFlowEffect::WaitUntil(deadline),
                _ => ControlFlowEffect::Wait,
            }
        };

        RuntimeEffects {
            request_redraw,
            control_flow: Some(control_flow),
            ..RuntimeEffects::default()
        }
    }

    /// Updates whether the native window can acquire a drawable surface.
    /// Non-drawable windows drop any outstanding redraw edge.
    pub(crate) fn set_drawable(&mut self, drawable: bool) {
        self.drawable = drawable;
        if !drawable {
            self.redraw_pending = false;
        }
    }

    /// Requests a generic host retry frame (surface recovery / suboptimal).
    pub(crate) fn request_frame(&mut self) -> RuntimeEffects {
        self.schedule(RuntimeEffects::request_redraw(), Self::HOST_RETRY_WAKE)
    }

    #[cfg(test)]
    fn wake_bits(&self) -> u32 {
        self.wake_bits
    }

    #[cfg(test)]
    fn redraw_pending(&self) -> bool {
        self.redraw_pending
    }

    #[cfg(test)]
    fn active(&self) -> bool {
        self.active
    }

    fn fold_control_flow(&mut self, control_flow: Option<ControlFlowEffect>) {
        match control_flow {
            Some(ControlFlowEffect::Poll) => {
                self.active = true;
                self.runtime_deadline = None;
            }
            Some(ControlFlowEffect::WaitUntil(deadline)) => {
                self.active = false;
                self.runtime_deadline = Some(deadline);
            }
            Some(ControlFlowEffect::Wait) => {
                self.active = false;
                self.runtime_deadline = None;
            }
            None => {}
        }
    }
}

fn effective_deadline(
    runtime_deadline: Option<Instant>,
    host_deadline: Option<Instant>,
) -> Option<Instant> {
    match (runtime_deadline, host_deadline) {
        (Some(runtime), Some(host)) => Some(runtime.min(host)),
        (Some(runtime), None) => Some(runtime),
        (None, Some(host)) => Some(host),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn redraw_effects() -> RuntimeEffects {
        RuntimeEffects::request_redraw()
    }

    #[test]
    fn coalesces_wakes_until_frame_starts() {
        let mut scheduler = FrameScheduler::default();

        let first = scheduler.schedule(redraw_effects(), FrameScheduler::EXTERNAL_WAKE);
        let second = scheduler.schedule(redraw_effects(), FrameScheduler::EXTERNAL_WAKE);
        assert!(first.request_redraw);
        assert!(!second.request_redraw);
        assert!(scheduler.redraw_pending());
        assert_eq!(
            scheduler.wake_bits() & FrameScheduler::EXTERNAL_WAKE,
            FrameScheduler::EXTERNAL_WAKE
        );

        let started = scheduler.frame_started(RuntimeEffects {
            request_redraw: true,
            ..RuntimeEffects::default()
        });
        assert!(!started.request_redraw);
        assert!(!scheduler.redraw_pending());
        assert_eq!(scheduler.wake_bits(), 0);

        let again = scheduler.schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE);
        assert!(again.request_redraw);
    }

    #[test]
    fn independent_schedulers_do_not_suppress_each_other() {
        let mut main = FrameScheduler::default();
        let mut confirmation = FrameScheduler::default();

        assert!(
            main.schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );
        assert!(
            confirmation
                .schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );

        let _ = main.frame_started(RuntimeEffects::default());
        assert!(!main.redraw_pending());
        assert!(confirmation.redraw_pending());
    }

    #[test]
    fn each_scheduler_can_rewake_after_only_its_own_frame_start() {
        let mut main = FrameScheduler::default();
        let mut confirmation = FrameScheduler::default();
        assert!(
            main.schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );
        assert!(
            confirmation
                .schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );

        let _ = main.frame_started(RuntimeEffects::default());
        assert!(
            main.schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );

        assert!(
            !confirmation
                .schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );
        let _ = confirmation.frame_started(RuntimeEffects::default());
        assert!(
            confirmation
                .schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );
        assert!(main.redraw_pending());
    }

    #[test]
    fn host_retry_rewakes_after_frame_start() {
        let mut scheduler = FrameScheduler::default();
        let first = scheduler.request_frame();
        let coalesced = scheduler.request_frame();
        let _ = scheduler.frame_started(RuntimeEffects::default());
        let next = scheduler.request_frame();

        assert!(first.request_redraw);
        assert!(!coalesced.request_redraw);
        assert!(next.request_redraw);
    }

    #[test]
    fn selects_idle_deadline_and_active_control_flows() {
        let mut scheduler = FrameScheduler::default();
        let idle = scheduler.about_to_wait(Instant::now(), None);
        assert_eq!(idle.control_flow, Some(ControlFlowEffect::Wait));

        let deadline = Instant::now() + Duration::from_millis(10);
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(deadline)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let waiting = scheduler.about_to_wait(Instant::now(), None);
        assert_eq!(
            waiting.control_flow,
            Some(ControlFlowEffect::WaitUntil(deadline))
        );

        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Poll),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        assert!(scheduler.active());
        let active = scheduler.about_to_wait(Instant::now(), None);
        assert_eq!(active.control_flow, Some(ControlFlowEffect::Poll));
        assert!(active.request_redraw);

        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Wait),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let settled = scheduler.about_to_wait(Instant::now(), None);
        assert_eq!(settled.control_flow, Some(ControlFlowEffect::Wait));
    }

    #[test]
    fn runtime_control_flow_persists_until_replaced() {
        let mut scheduler = FrameScheduler::default();
        let deadline = Instant::now() + Duration::from_secs(1);

        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(deadline)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        assert_eq!(
            scheduler.about_to_wait(Instant::now(), None).control_flow,
            Some(ControlFlowEffect::WaitUntil(deadline))
        );

        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Poll),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        assert_eq!(
            scheduler.about_to_wait(Instant::now(), None).control_flow,
            Some(ControlFlowEffect::Poll)
        );
    }

    #[test]
    fn suspended_scheduler_drops_pending_redraw_and_rewakes_on_restore() {
        let mut scheduler = FrameScheduler::default();
        assert!(
            scheduler
                .schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );

        scheduler.set_drawable(false);
        assert!(!scheduler.redraw_pending());
        assert!(
            !scheduler
                .schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );
        assert_eq!(
            scheduler.about_to_wait(Instant::now(), None).control_flow,
            Some(ControlFlowEffect::Wait)
        );

        scheduler.set_drawable(true);
        assert!(
            scheduler
                .schedule(redraw_effects(), FrameScheduler::HOST_RETRY_WAKE)
                .request_redraw
        );
        assert!(scheduler.redraw_pending());
    }

    #[test]
    fn active_completion_requests_continuation_idle_completion_waits() {
        let mut scheduler = FrameScheduler::default();
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Poll),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        let active = scheduler.frame_completed(Instant::now());
        assert!(active.request_redraw);
        assert_eq!(active.control_flow, Some(ControlFlowEffect::Poll));

        let _ = scheduler.frame_started(RuntimeEffects::default());
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Wait),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let idle = scheduler.frame_completed(Instant::now());
        assert!(!idle.request_redraw);
        assert_eq!(idle.control_flow, Some(ControlFlowEffect::Wait));
    }

    #[test]
    fn due_deadline_requests_one_frame_and_never_returns_past_wait_until() {
        let mut scheduler = FrameScheduler::default();
        let past = Instant::now() - Duration::from_millis(1);
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(past)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        let now = Instant::now();
        let effects = scheduler.about_to_wait(now, None);
        assert!(effects.request_redraw);
        assert_eq!(effects.control_flow, Some(ControlFlowEffect::Wait));
        assert_eq!(
            scheduler.wake_bits() & FrameScheduler::DEADLINE_WAKE,
            FrameScheduler::DEADLINE_WAKE
        );
    }

    #[test]
    fn host_deadline_does_not_overwrite_runtime_deadline() {
        let mut scheduler = FrameScheduler::default();
        let runtime = Instant::now() + Duration::from_secs(2);
        let host = Instant::now() + Duration::from_secs(1);
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(runtime)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        let effects = scheduler.about_to_wait(Instant::now(), Some(host));
        assert_eq!(
            effects.control_flow,
            Some(ControlFlowEffect::WaitUntil(host))
        );

        // Runtime deadline remains for a later idle turn after host telemetry emits.
        let later = scheduler.about_to_wait(Instant::now(), None);
        assert_eq!(
            later.control_flow,
            Some(ControlFlowEffect::WaitUntil(runtime))
        );
    }

    #[test]
    fn frame_start_consumes_dirty_redraw_without_second_edge() {
        let mut scheduler = FrameScheduler::default();
        assert!(
            scheduler
                .schedule(redraw_effects(), FrameScheduler::EXTERNAL_WAKE)
                .request_redraw
        );

        let started = scheduler.frame_started(RuntimeEffects {
            request_redraw: true,
            control_flow: Some(ControlFlowEffect::Wait),
            cursor: None,
            ime: None,
            clipboard: None,
        });
        assert!(!started.request_redraw);
        assert_eq!(started.control_flow, Some(ControlFlowEffect::Wait));
        assert!(!scheduler.redraw_pending());
    }

    #[test]
    fn should_return_host_wait_until_when_runtime_has_no_deadline() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let host = now + Duration::from_secs(1);

        // Act
        let effects = scheduler.about_to_wait(now, Some(host));

        // Assert
        assert!(!effects.request_redraw);
        assert_eq!(
            effects.control_flow,
            Some(ControlFlowEffect::WaitUntil(host))
        );
    }

    #[test]
    fn should_return_wait_when_host_deadline_is_already_due() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let past_host = now - Duration::from_millis(1);

        // Act
        let effects = scheduler.about_to_wait(now, Some(past_host));

        // Assert — never returns a past WaitUntil.
        assert!(!effects.request_redraw);
        assert_eq!(effects.control_flow, Some(ControlFlowEffect::Wait));
    }

    #[test]
    fn should_wait_without_redraw_when_completing_under_future_wait_until() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let future = Instant::now() + Duration::from_secs(5);
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(future)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        // Act
        let completed = scheduler.frame_completed(Instant::now());

        // Assert — completion observes activity, not the stored deadline.
        assert!(!completed.request_redraw);
        assert_eq!(completed.control_flow, Some(ControlFlowEffect::Wait));
    }

    #[test]
    fn should_not_request_second_continuation_when_redraw_already_pending() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let _ = scheduler.schedule(
            RuntimeEffects {
                request_redraw: true,
                control_flow: Some(ControlFlowEffect::Poll),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        // Act
        let completed = scheduler.frame_completed(Instant::now());

        // Assert
        assert!(!completed.request_redraw);
        assert_eq!(completed.control_flow, Some(ControlFlowEffect::Poll));
    }

    #[test]
    fn should_consume_due_deadline_when_redraw_already_pending() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let past = Instant::now() - Duration::from_millis(1);
        assert!(
            scheduler
                .schedule(
                    RuntimeEffects {
                        request_redraw: true,
                        control_flow: Some(ControlFlowEffect::WaitUntil(past)),
                        ..RuntimeEffects::default()
                    },
                    FrameScheduler::RUNTIME_WAKE,
                )
                .request_redraw
        );

        // Act — the outstanding edge covers the due deadline, then the frame starts.
        let covered = scheduler.about_to_wait(Instant::now(), None);
        let _ = scheduler.frame_started(RuntimeEffects::default());
        let after_frame_start = scheduler.about_to_wait(Instant::now(), None);

        // Assert — the deadline was consumed rather than scheduling a second frame.
        assert!(!covered.request_redraw);
        assert_eq!(covered.control_flow, Some(ControlFlowEffect::Wait));
        assert!(!after_frame_start.request_redraw);
        assert_eq!(
            after_frame_start.control_flow,
            Some(ControlFlowEffect::Wait)
        );
    }

    #[test]
    fn should_consume_due_deadline_while_window_is_not_drawable() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let past = Instant::now() - Duration::from_millis(1);
        scheduler.set_drawable(false);
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(past)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        // Act — no redraw may be requested while minimized.
        let suspended = scheduler.about_to_wait(Instant::now(), None);
        scheduler.set_drawable(true);
        let restored = scheduler.about_to_wait(Instant::now(), None);

        // Assert — the expired deadline was discarded, so restoration is idle.
        assert!(!suspended.request_redraw);
        assert_eq!(suspended.control_flow, Some(ControlFlowEffect::Wait));
        assert!(!restored.request_redraw);
        assert_eq!(restored.control_flow, Some(ControlFlowEffect::Wait));
    }
}
