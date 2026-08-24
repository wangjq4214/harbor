//! Platform-neutral frame scheduling state machine.
//!
//! Turns runtime work, redraw observation, frame completion, activity, and
//! deadlines into redraw and control-flow effects without winit or App types.

use crate::effects::{ControlFlowEffect, RuntimeEffects};
use std::time::{Duration, Instant};

/// Per-window redraw coalescing and idle wait-policy state.
#[derive(Debug)]
pub(crate) struct FrameScheduler {
    redraw_pending: bool,
    drawable: bool,
    runtime_deadline: Option<Instant>,
    recovery_deadline: Option<Instant>,
    active: bool,
    wake_bits: u32,
    has_deferred_externals: bool,
    force_present_pending: bool,
    last_frame_was_commit: bool,
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self {
            redraw_pending: false,
            drawable: true,
            runtime_deadline: None,
            recovery_deadline: None,
            active: false,
            wake_bits: 0,
            has_deferred_externals: false,
            force_present_pending: false,
            last_frame_was_commit: false,
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
    /// Diagnostic bit for a due 100 ms deferred-external recovery commit.
    pub(crate) const RECOVERY_WAKE: u32 = 1 << 5;

    pub(crate) const RECOVERY_INTERVAL: Duration = Duration::from_millis(100);

    /// Folds one runtime effect batch into scheduler state and returns the
    /// host-facing edge (at most one outstanding redraw request).
    pub(crate) fn schedule(
        &mut self,
        mut effects: RuntimeEffects,
        wake_bit: u32,
    ) -> RuntimeEffects {
        self.has_deferred_externals = effects.has_deferred_externals;
        if !self.has_deferred_externals {
            self.recovery_deadline = None;
        }
        effects.ordinary_present_eligible = true;
        self.fold_control_flow(effects.control_flow);

        let ordinary = effects.request_redraw;
        if ordinary || effects.force_present {
            self.force_present_pending |= effects.force_present;
            self.wake_bits |= wake_bit;
            let edge = self.drawable && !self.redraw_pending;
            effects.request_redraw = edge;
            if self.drawable {
                self.redraw_pending = true;
            }
        }
        effects.has_deferred_externals = self.has_deferred_externals;
        effects
    }

    /// Folds a batch that did not collect external schedule demand.
    ///
    /// Default constructors must not clear stored deferred-external presence.
    /// `schedule` still replaces that flag so a later `Runtime::update` collect
    /// can restore or cancel it.
    pub(crate) fn schedule_retaining_ineligibility(
        &mut self,
        mut effects: RuntimeEffects,
        wake_bit: u32,
    ) -> RuntimeEffects {
        effects.has_deferred_externals |= self.has_deferred_externals;
        effects.ordinary_present_eligible = true;
        self.schedule(effects, wake_bit)
    }

    /// Observes frame start: clears the outstanding redraw edge, folds the
    /// runtime update's control-flow state, and suppresses a redundant redraw
    /// because the current frame consumes that work.
    pub(crate) fn frame_started(&mut self, mut effects: RuntimeEffects) -> RuntimeEffects {
        let consumed_redraw = self.redraw_pending;
        self.redraw_pending = false;
        self.wake_bits = 0;
        self.has_deferred_externals = effects.has_deferred_externals;
        if !self.has_deferred_externals {
            self.recovery_deadline = None;
        }
        effects.ordinary_present_eligible = true;
        if consumed_redraw {
            effects.force_present |= self.force_present_pending;
            self.force_present_pending = false;
            self.last_frame_was_commit = effects.force_present;
        } else {
            self.last_frame_was_commit = false;
        }
        self.fold_control_flow(effects.control_flow);
        effects.request_redraw = false;
        effects.has_deferred_externals = self.has_deferred_externals;
        effects
    }

    /// Observes successful presentation. Active animation may request one
    /// continuation frame and keep `Poll`; idle completion returns `Wait`.
    pub(crate) fn frame_completed(&mut self, now: Instant) -> RuntimeEffects {
        let committed = self.last_frame_was_commit;
        self.last_frame_was_commit = false;
        if !self.has_deferred_externals {
            self.recovery_deadline = None;
        } else if committed {
            self.recovery_deadline = Some(now + Self::RECOVERY_INTERVAL);
        }

        if self.active {
            let request_redraw = self.drawable && !self.redraw_pending;
            if request_redraw {
                self.wake_bits |= Self::CONTINUATION_WAKE;
                self.redraw_pending = true;
            }
            return RuntimeEffects {
                request_redraw,
                control_flow: Some(ControlFlowEffect::Poll),
                ordinary_present_eligible: true,
                has_deferred_externals: self.has_deferred_externals,
                ..RuntimeEffects::default()
            };
        }

        RuntimeEffects {
            control_flow: Some(ControlFlowEffect::Wait),
            ordinary_present_eligible: true,
            has_deferred_externals: self.has_deferred_externals,
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
            if let Some(deadline) = self.recovery_deadline
                && now >= deadline
            {
                self.recovery_deadline = None;
            }
            return RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Wait),
                ordinary_present_eligible: true,
                has_deferred_externals: self.has_deferred_externals,
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

        self.apply_recovery(now, &mut request_redraw);

        let control_flow = if self.active {
            ControlFlowEffect::Poll
        } else {
            match effective_deadline(
                effective_deadline(self.runtime_deadline, self.recovery_deadline),
                host_deadline,
            ) {
                Some(deadline) if deadline > now => ControlFlowEffect::WaitUntil(deadline),
                _ => ControlFlowEffect::Wait,
            }
        };

        RuntimeEffects {
            request_redraw,
            control_flow: Some(control_flow),
            ordinary_present_eligible: true,
            force_present: self.force_present_pending,
            has_deferred_externals: self.has_deferred_externals,
            ..RuntimeEffects::default()
        }
    }

    /// Updates whether the native window can acquire a drawable surface.
    /// Non-drawable windows drop any outstanding redraw edge; restoring a
    /// drawable surface emits one recovery edge.
    pub(crate) fn set_drawable(&mut self, drawable: bool) -> RuntimeEffects {
        let restored = drawable && !self.drawable;
        self.drawable = drawable;
        if !drawable {
            self.redraw_pending = false;
        }
        if restored {
            if self.has_deferred_externals {
                self.force_present_pending = true;
            }
            self.request_frame()
        } else {
            RuntimeEffects::default()
        }
    }

    /// Requests a generic host retry frame (surface recovery / suboptimal).
    pub(crate) fn request_frame(&mut self) -> RuntimeEffects {
        let mut effects = RuntimeEffects::request_redraw();
        effects.ordinary_present_eligible = true;
        effects.force_present = self.force_present_pending;
        effects.has_deferred_externals = self.has_deferred_externals;
        self.schedule(effects, Self::HOST_RETRY_WAKE)
    }

    /// Consumes a due `runtime_deadline` before a newer `WaitUntil` is folded.
    ///
    /// External schedule providers report the *next* phase boundary at `now`.
    /// If the adapter folds that future deadline before evaluating the prior
    /// due edge, blink wakes never request a redraw. Call this at the start of
    /// an idle turn, before `Runtime::update` replaces the stored deadline.
    ///
    /// Returns `true` when the host should arm a redraw edge (drawable). Does
    /// not set `redraw_pending` itself — callers fold a `request_redraw` through
    /// [`Self::schedule`] so the host still receives the outstanding edge.
    pub(crate) fn consume_due_deadline(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.runtime_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }

        self.runtime_deadline = None;
        self.wake_bits |= Self::DEADLINE_WAKE;
        self.drawable
    }

    fn apply_recovery(&mut self, now: Instant, request_redraw: &mut bool) {
        if !self.has_deferred_externals {
            self.recovery_deadline = None;
            return;
        }

        if let Some(deadline) = self.recovery_deadline
            && now >= deadline
        {
            self.recovery_deadline = None;
            self.force_present_pending = true;
            if !self.redraw_pending {
                self.wake_bits |= Self::RECOVERY_WAKE;
                self.redraw_pending = true;
                *request_redraw = true;
            }
            return;
        }

        if self.recovery_deadline.is_none() {
            self.recovery_deadline = Some(now + Self::RECOVERY_INTERVAL);
        }
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

    #[cfg(test)]
    fn recovery_deadline(&self) -> Option<Instant> {
        self.recovery_deadline
    }

    fn fold_control_flow(&mut self, control_flow: Option<ControlFlowEffect>) {
        match control_flow {
            Some(ControlFlowEffect::Poll) => {
                self.active = true;
                self.runtime_deadline = None;
            }
            Some(ControlFlowEffect::WaitUntil(deadline)) => {
                // Record the deadline without clearing `active`. Poll dominates
                // WaitUntil (same preference as ControlFlowEffect::arbitrate), so
                // an idle external blink deadline cannot demote an active animation.
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
    fn should_keep_poll_active_when_wait_until_arrives_during_animation() {
        // Arrange — animation Poll is active, then an external blink deadline arrives.
        let mut scheduler = FrameScheduler::default();
        let deadline = Instant::now() + Duration::from_secs(2);
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::Poll),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        assert!(scheduler.active());

        // Act — fold a WaitUntil from external schedule (as Runtime idle turns do).
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(deadline)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        // Assert — Poll remains dominant; idle turns stay on Poll rather than demoting.
        assert!(scheduler.active());
        let idle = scheduler.about_to_wait(Instant::now(), None);
        assert_eq!(idle.control_flow, Some(ControlFlowEffect::Poll));
    }

    #[test]
    fn should_consume_due_deadline_before_next_phase_replaces_it() {
        // Arrange
        let now = Instant::now();
        let mut scheduler = FrameScheduler::default();
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(now - Duration::from_millis(1))),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        // Act — consume due edge, then fold the next blink phase through schedule
        // so the host still receives the redraw edge.
        assert!(scheduler.consume_due_deadline(now));
        let next = now + Duration::from_millis(530);
        let armed = scheduler.schedule(
            RuntimeEffects {
                request_redraw: true,
                control_flow: Some(ControlFlowEffect::WaitUntil(next)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let idle = scheduler.about_to_wait(now, None);

        // Assert
        assert!(armed.request_redraw);
        assert!(scheduler.redraw_pending());
        assert!(!idle.request_redraw); // outstanding edge already armed
        assert_eq!(idle.control_flow, Some(ControlFlowEffect::WaitUntil(next)));
    }

    #[test]
    fn suspended_scheduler_drops_pending_redraw_and_rewakes_on_restore() {
        let mut scheduler = FrameScheduler::default();
        assert!(
            scheduler
                .schedule(redraw_effects(), FrameScheduler::RUNTIME_WAKE)
                .request_redraw
        );

        let _ = scheduler.set_drawable(false);
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

        let recovery = scheduler.set_drawable(true);
        assert!(recovery.request_redraw);
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
            ..RuntimeEffects::default()
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
    fn should_request_recovery_frame_after_due_deadline_passes_while_not_drawable() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let past = Instant::now() - Duration::from_millis(1);
        let _ = scheduler.set_drawable(false);
        let _ = scheduler.schedule(
            RuntimeEffects {
                control_flow: Some(ControlFlowEffect::WaitUntil(past)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        // Act — no redraw may be requested while minimized, but restoration
        // emits one recovery edge even though the expired deadline was consumed.
        let suspended = scheduler.about_to_wait(Instant::now(), None);
        let recovery = scheduler.set_drawable(true);
        let restored = scheduler.about_to_wait(Instant::now(), None);

        // Assert
        assert!(!suspended.request_redraw);
        assert_eq!(suspended.control_flow, Some(ControlFlowEffect::Wait));
        assert!(recovery.request_redraw);
        assert!(!restored.request_redraw);
        assert_eq!(restored.control_flow, Some(ControlFlowEffect::Wait));
    }

    #[test]
    fn should_keep_poll_when_externals_are_deferred() {
        let mut scheduler = FrameScheduler::default();

        let effects = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                control_flow: Some(ControlFlowEffect::Poll),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let idle = scheduler.about_to_wait(Instant::now(), None);

        assert_eq!(effects.control_flow, Some(ControlFlowEffect::Poll));
        assert_eq!(idle.control_flow, Some(ControlFlowEffect::Poll));
        assert!(idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(idle.has_deferred_externals);
    }

    #[test]
    fn should_arm_widget_redraw_when_externals_are_deferred() {
        let mut scheduler = FrameScheduler::default();

        let effects = scheduler.schedule(
            RuntimeEffects {
                request_redraw: true,
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        assert!(effects.request_redraw);
        assert!(effects.ordinary_present_eligible);
        assert!(scheduler.redraw_pending());
    }

    #[test]
    fn should_arm_redraw_when_deferred_present_is_forced() {
        let mut scheduler = FrameScheduler::default();

        let first = scheduler.schedule(
            RuntimeEffects {
                request_redraw: true,
                has_deferred_externals: true,
                force_present: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let second = scheduler.schedule(
            RuntimeEffects {
                request_redraw: true,
                has_deferred_externals: true,
                force_present: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        assert!(first.request_redraw);
        assert!(!second.request_redraw);
        assert!(scheduler.redraw_pending());
    }

    #[test]
    fn should_arm_recovery_wait_until_when_externals_are_deferred() {
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let recovery = now + FrameScheduler::RECOVERY_INTERVAL;

        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let idle = scheduler.about_to_wait(now, None);

        assert!(!idle.request_redraw);
        assert!(!idle.force_present);
        assert_eq!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(recovery))
        );
        assert_eq!(scheduler.recovery_deadline(), Some(recovery));
    }

    #[test]
    fn should_force_commit_when_recovery_is_due() {
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let due = now + FrameScheduler::RECOVERY_INTERVAL;

        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let _ = scheduler.about_to_wait(now, None);
        let fired = scheduler.about_to_wait(due, None);
        let started = scheduler.frame_started(RuntimeEffects {
            has_deferred_externals: true,
            ..RuntimeEffects::default()
        });
        let completed = scheduler.frame_completed(due);

        assert!(fired.request_redraw);
        assert!(fired.force_present);
        assert!(started.force_present);
        assert!(!completed.request_redraw);
        assert_eq!(
            scheduler.recovery_deadline(),
            Some(due + FrameScheduler::RECOVERY_INTERVAL)
        );
    }

    #[test]
    fn should_clear_recovery_when_externals_are_no_longer_deferred() {
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();

        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let _ = scheduler.about_to_wait(now, None);
        let cancelled = scheduler.schedule(RuntimeEffects::default(), FrameScheduler::RUNTIME_WAKE);
        let idle = scheduler.about_to_wait(now, None);

        assert!(!cancelled.has_deferred_externals);
        assert!(scheduler.recovery_deadline().is_none());
        assert_eq!(idle.control_flow, Some(ControlFlowEffect::Wait));
        assert!(!idle.force_present);
    }

    #[test]
    fn should_coalesce_due_recovery_onto_pending_widget_redraw() {
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let due = now + FrameScheduler::RECOVERY_INTERVAL;

        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let _ = scheduler.about_to_wait(now, None);
        let widget = scheduler.schedule(
            RuntimeEffects {
                request_redraw: true,
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let idle = scheduler.about_to_wait(due, None);

        assert!(widget.request_redraw);
        assert!(!idle.request_redraw);
        assert!(idle.force_present);
        assert!(scheduler.redraw_pending());
    }

    #[test]
    fn should_keep_recovery_instant_across_idle_repeats() {
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();

        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let first = scheduler.about_to_wait(now, None);
        let second = scheduler.about_to_wait(now + Duration::from_millis(10), None);

        assert_eq!(first.control_flow, second.control_flow);
        assert_eq!(
            scheduler.recovery_deadline(),
            Some(now + FrameScheduler::RECOVERY_INTERVAL)
        );
        assert!(!second.force_present);
    }

    #[test]
    fn should_not_spin_when_recovery_is_due_while_not_drawable() {
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let due = now + FrameScheduler::RECOVERY_INTERVAL;

        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let _ = scheduler.about_to_wait(now, None);
        let _ = scheduler.set_drawable(false);
        let hidden = scheduler.about_to_wait(due, None);
        let restored = scheduler.set_drawable(true);

        assert!(!hidden.request_redraw);
        assert_eq!(hidden.control_flow, Some(ControlFlowEffect::Wait));
        assert!(scheduler.recovery_deadline().is_none());
        assert!(restored.request_redraw);
        assert!(restored.force_present);
    }

    #[test]
    fn should_retain_force_present_until_redraw_is_consumed() {
        let mut scheduler = FrameScheduler::default();
        let _ = scheduler.set_drawable(false);
        let deferred_force = RuntimeEffects {
            has_deferred_externals: true,
            force_present: true,
            request_redraw: true,
            ..RuntimeEffects::default()
        };

        let suspended = scheduler.schedule(deferred_force, FrameScheduler::RUNTIME_WAKE);
        let restored = scheduler.set_drawable(true);
        let started = scheduler.frame_started(RuntimeEffects {
            has_deferred_externals: true,
            ..RuntimeEffects::default()
        });
        let after_consumption = scheduler.frame_started(RuntimeEffects::default());

        assert!(!suspended.request_redraw);
        assert!(restored.request_redraw);
        assert!(restored.force_present);
        assert!(started.force_present);
        assert!(!after_consumption.force_present);
    }

    #[test]
    fn should_not_arm_redraw_when_forced_present_is_not_drawable() {
        let mut scheduler = FrameScheduler::default();
        let _ = scheduler.set_drawable(false);

        let effects = scheduler.schedule(
            RuntimeEffects {
                request_redraw: true,
                has_deferred_externals: true,
                force_present: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        assert!(!effects.request_redraw);
        assert!(!scheduler.redraw_pending());
    }

    #[test]
    fn should_arm_redraw_when_force_present_has_no_ordinary_request() {
        let mut scheduler = FrameScheduler::default();

        let effects = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                force_present: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );

        assert!(effects.request_redraw);
        assert!(effects.force_present);
        assert!(effects.ordinary_present_eligible);
        assert!(scheduler.redraw_pending());
    }

    #[test]
    fn should_keep_deferred_externals_when_retaining_ineligibility_without_collect() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let _ = scheduler.about_to_wait(now, None);

        // Act — a widget-only batch must not clear stored deferred presence
        let retained = scheduler.schedule_retaining_ineligibility(
            RuntimeEffects::request_redraw(),
            FrameScheduler::RUNTIME_WAKE,
        );

        // Assert
        assert!(retained.request_redraw);
        assert!(retained.has_deferred_externals);
        assert!(retained.ordinary_present_eligible);
        assert_eq!(
            scheduler.recovery_deadline(),
            Some(now + FrameScheduler::RECOVERY_INTERVAL)
        );
    }

    #[test]
    fn should_wait_until_recovery_when_runtime_deadline_is_later() {
        // Arrange
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        let blink = now + Duration::from_millis(400);
        let recovery = now + FrameScheduler::RECOVERY_INTERVAL;

        // Act
        let _ = scheduler.schedule(
            RuntimeEffects {
                has_deferred_externals: true,
                control_flow: Some(ControlFlowEffect::WaitUntil(blink)),
                ..RuntimeEffects::default()
            },
            FrameScheduler::RUNTIME_WAKE,
        );
        let idle = scheduler.about_to_wait(now, None);

        // Assert
        assert_eq!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(recovery))
        );
        assert!(!idle.force_present);
    }
}
