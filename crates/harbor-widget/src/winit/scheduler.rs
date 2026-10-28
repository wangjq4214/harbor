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
            has_deferred_externals: false,
            force_present_pending: false,
            last_frame_was_commit: false,
        }
    }
}

impl FrameScheduler {
    pub(crate) const RECOVERY_INTERVAL: Duration = Duration::from_millis(100);

    /// Folds one runtime effect batch into scheduler state and returns the
    /// host-facing edge (at most one outstanding redraw request).
    pub(crate) fn schedule(&mut self, mut effects: RuntimeEffects) -> RuntimeEffects {
        self.has_deferred_externals = effects.has_deferred_externals;
        if !self.has_deferred_externals {
            self.recovery_deadline = None;
        }
        effects.ordinary_present_eligible = true;
        self.fold_control_flow(effects.control_flow);

        let ordinary = effects.request_redraw;
        if ordinary || effects.force_present {
            self.force_present_pending |= effects.force_present;
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
    ) -> RuntimeEffects {
        effects.has_deferred_externals |= self.has_deferred_externals;
        effects.ordinary_present_eligible = true;
        self.schedule(effects)
    }

    /// Observes frame start: clears the outstanding redraw edge, folds the
    /// runtime update's control-flow state, and suppresses a redundant redraw
    /// because the current frame consumes that work.
    pub(crate) fn frame_started(&mut self, mut effects: RuntimeEffects) -> RuntimeEffects {
        let consumed_redraw = self.redraw_pending;
        self.redraw_pending = false;
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
    /// Suspension suppresses render wake demand, not future Host wake demand.
    pub(crate) fn about_to_wait(
        &mut self,
        now: Instant,
        host_deadline: Option<Instant>,
    ) -> RuntimeEffects {
        // Business work belongs to the caller. Due input must neither spin nor
        // mask another future demand in drawable deadline arbitration.
        let host_deadline = host_deadline.filter(|deadline| *deadline > now);
        if !self.drawable {
            if let Some(deadline) = self.runtime_deadline
                && now >= deadline
            {
                self.runtime_deadline = None;
            }
            if let Some(deadline) = self.recovery_deadline
                && now >= deadline
            {
                self.recovery_deadline = None;
            }
            return RuntimeEffects {
                control_flow: Some(
                    host_deadline
                        .map(ControlFlowEffect::WaitUntil)
                        .unwrap_or(ControlFlowEffect::Wait),
                ),
                ordinary_present_eligible: true,
                has_deferred_externals: self.has_deferred_externals,
                ..RuntimeEffects::default()
            };
        }

        let mut request_redraw = false;
        if self.active && !self.redraw_pending {
            self.redraw_pending = true;
            request_redraw = true;
        }

        if let Some(deadline) = self.runtime_deadline
            && now >= deadline
        {
            self.runtime_deadline = None;
            if !self.redraw_pending {
                self.redraw_pending = true;
                request_redraw = true;
            }
        }

        self.apply_recovery(now, &mut request_redraw);

        let control_flow = if self.active {
            ControlFlowEffect::Poll
        } else {
            match [self.runtime_deadline, self.recovery_deadline, host_deadline]
                .into_iter()
                .flatten()
                .min()
            {
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
        self.schedule(effects)
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

#[cfg(test)]
mod tests;
