use super::*;
use std::time::Duration;

fn redraw_effects() -> RuntimeEffects {
    RuntimeEffects::request_redraw()
}

#[test]
fn coalesces_wakes_until_frame_starts() {
    let mut scheduler = FrameScheduler::default();

    let first = scheduler.schedule(redraw_effects());
    let second = scheduler.schedule(redraw_effects());
    assert!(first.request_redraw);
    assert!(!second.request_redraw);
    assert!(scheduler.redraw_pending());

    let started = scheduler.frame_started(RuntimeEffects {
        request_redraw: true,
        ..RuntimeEffects::default()
    });
    assert!(!started.request_redraw);
    assert!(!scheduler.redraw_pending());

    let again = scheduler.schedule(redraw_effects());
    assert!(again.request_redraw);
}

#[test]
fn independent_schedulers_do_not_suppress_each_other() {
    let mut main = FrameScheduler::default();
    let mut confirmation = FrameScheduler::default();

    assert!(main.schedule(redraw_effects()).request_redraw);
    assert!(confirmation.schedule(redraw_effects()).request_redraw);

    let _ = main.frame_started(RuntimeEffects::default());
    assert!(!main.redraw_pending());
    assert!(confirmation.redraw_pending());
}

#[test]
fn each_scheduler_can_rewake_after_only_its_own_frame_start() {
    let mut main = FrameScheduler::default();
    let mut confirmation = FrameScheduler::default();
    assert!(main.schedule(redraw_effects()).request_redraw);
    assert!(confirmation.schedule(redraw_effects()).request_redraw);

    let _ = main.frame_started(RuntimeEffects::default());
    assert!(main.schedule(redraw_effects()).request_redraw);

    assert!(!confirmation.schedule(redraw_effects()).request_redraw);
    let _ = confirmation.frame_started(RuntimeEffects::default());
    assert!(confirmation.schedule(redraw_effects()).request_redraw);
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
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(deadline)),
        ..RuntimeEffects::default()
    });
    let waiting = scheduler.about_to_wait(Instant::now(), None);
    assert_eq!(
        waiting.control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );

    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Poll),
        ..RuntimeEffects::default()
    });
    assert!(scheduler.active());
    let active = scheduler.about_to_wait(Instant::now(), None);
    assert_eq!(active.control_flow, Some(ControlFlowEffect::Poll));
    assert!(active.request_redraw);

    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Wait),
        ..RuntimeEffects::default()
    });
    let settled = scheduler.about_to_wait(Instant::now(), None);
    assert_eq!(settled.control_flow, Some(ControlFlowEffect::Wait));
}

#[test]
fn runtime_control_flow_persists_until_replaced() {
    let mut scheduler = FrameScheduler::default();
    let deadline = Instant::now() + Duration::from_secs(1);

    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(deadline)),
        ..RuntimeEffects::default()
    });
    assert_eq!(
        scheduler.about_to_wait(Instant::now(), None).control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );

    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Poll),
        ..RuntimeEffects::default()
    });
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
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Poll),
        ..RuntimeEffects::default()
    });
    assert!(scheduler.active());

    // Act — fold a WaitUntil from external schedule (as Runtime idle turns do).
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(deadline)),
        ..RuntimeEffects::default()
    });

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
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(now - Duration::from_millis(1))),
        ..RuntimeEffects::default()
    });

    // Act — consume due edge, then fold the next blink phase through schedule
    // so the host still receives the redraw edge.
    assert!(scheduler.consume_due_deadline(now));
    let next = now + Duration::from_millis(530);
    let armed = scheduler.schedule(RuntimeEffects {
        request_redraw: true,
        control_flow: Some(ControlFlowEffect::WaitUntil(next)),
        ..RuntimeEffects::default()
    });
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
    assert!(scheduler.schedule(redraw_effects()).request_redraw);

    let _ = scheduler.set_drawable(false);
    assert!(!scheduler.redraw_pending());
    assert!(!scheduler.schedule(redraw_effects()).request_redraw);
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
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Poll),
        ..RuntimeEffects::default()
    });

    let active = scheduler.frame_completed(Instant::now());
    assert!(active.request_redraw);
    assert_eq!(active.control_flow, Some(ControlFlowEffect::Poll));

    let _ = scheduler.frame_started(RuntimeEffects::default());
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Wait),
        ..RuntimeEffects::default()
    });
    let idle = scheduler.frame_completed(Instant::now());
    assert!(!idle.request_redraw);
    assert_eq!(idle.control_flow, Some(ControlFlowEffect::Wait));
}

#[test]
fn due_deadline_requests_one_frame_and_never_returns_past_wait_until() {
    let mut scheduler = FrameScheduler::default();
    let past = Instant::now() - Duration::from_millis(1);
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(past)),
        ..RuntimeEffects::default()
    });

    let now = Instant::now();
    let effects = scheduler.about_to_wait(now, None);
    assert!(effects.request_redraw);
    assert_eq!(effects.control_flow, Some(ControlFlowEffect::Wait));
}

#[test]
fn host_deadline_does_not_overwrite_runtime_deadline() {
    let mut scheduler = FrameScheduler::default();
    let runtime = Instant::now() + Duration::from_secs(2);
    let host = Instant::now() + Duration::from_secs(1);
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(runtime)),
        ..RuntimeEffects::default()
    });

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
    assert!(scheduler.schedule(redraw_effects()).request_redraw);

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
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(future)),
        ..RuntimeEffects::default()
    });

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
    let _ = scheduler.schedule(RuntimeEffects {
        request_redraw: true,
        control_flow: Some(ControlFlowEffect::Poll),
        ..RuntimeEffects::default()
    });

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
            .schedule(RuntimeEffects {
                request_redraw: true,
                control_flow: Some(ControlFlowEffect::WaitUntil(past)),
                ..RuntimeEffects::default()
            },)
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
    let _ = scheduler.schedule(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::WaitUntil(past)),
        ..RuntimeEffects::default()
    });

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

    let effects = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        control_flow: Some(ControlFlowEffect::Poll),
        ..RuntimeEffects::default()
    });
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

    let effects = scheduler.schedule(RuntimeEffects {
        request_redraw: true,
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });

    assert!(effects.request_redraw);
    assert!(effects.ordinary_present_eligible);
    assert!(scheduler.redraw_pending());
}

#[test]
fn should_arm_redraw_when_deferred_present_is_forced() {
    let mut scheduler = FrameScheduler::default();

    let first = scheduler.schedule(RuntimeEffects {
        request_redraw: true,
        has_deferred_externals: true,
        force_present: true,
        ..RuntimeEffects::default()
    });
    let second = scheduler.schedule(RuntimeEffects {
        request_redraw: true,
        has_deferred_externals: true,
        force_present: true,
        ..RuntimeEffects::default()
    });

    assert!(first.request_redraw);
    assert!(!second.request_redraw);
    assert!(scheduler.redraw_pending());
}

#[test]
fn should_arm_recovery_wait_until_when_externals_are_deferred() {
    let mut scheduler = FrameScheduler::default();
    let now = Instant::now();
    let recovery = now + FrameScheduler::RECOVERY_INTERVAL;

    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
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

    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
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

    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
    let _ = scheduler.about_to_wait(now, None);
    let cancelled = scheduler.schedule(RuntimeEffects::default());
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

    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
    let _ = scheduler.about_to_wait(now, None);
    let widget = scheduler.schedule(RuntimeEffects {
        request_redraw: true,
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
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

    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
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

    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
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

    let suspended = scheduler.schedule(deferred_force);
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

    let effects = scheduler.schedule(RuntimeEffects {
        request_redraw: true,
        has_deferred_externals: true,
        force_present: true,
        ..RuntimeEffects::default()
    });

    assert!(!effects.request_redraw);
    assert!(!scheduler.redraw_pending());
}

#[test]
fn should_arm_redraw_when_force_present_has_no_ordinary_request() {
    let mut scheduler = FrameScheduler::default();

    let effects = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        force_present: true,
        ..RuntimeEffects::default()
    });

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
    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
    let _ = scheduler.about_to_wait(now, None);

    // Act — a widget-only batch must not clear stored deferred presence
    let retained = scheduler.schedule_retaining_ineligibility(RuntimeEffects::request_redraw());

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
    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        control_flow: Some(ControlFlowEffect::WaitUntil(blink)),
        ..RuntimeEffects::default()
    });
    let idle = scheduler.about_to_wait(now, None);

    // Assert
    assert_eq!(
        idle.control_flow,
        Some(ControlFlowEffect::WaitUntil(recovery))
    );
    assert!(!idle.force_present);
}

#[test]
fn should_not_keep_force_present_when_deferred_externals_clear_while_not_drawable() {
    let mut scheduler = FrameScheduler::default();
    let now = Instant::now();

    let _ = scheduler.schedule(RuntimeEffects {
        has_deferred_externals: true,
        ..RuntimeEffects::default()
    });
    let _ = scheduler.about_to_wait(now, None);
    let _ = scheduler.set_drawable(false);

    let cancelled = scheduler.schedule(RuntimeEffects::default());
    let idle = scheduler.about_to_wait(now, None);
    let restored = scheduler.set_drawable(true);

    assert!(!cancelled.has_deferred_externals);
    assert!(scheduler.recovery_deadline().is_none());
    assert_eq!(idle.control_flow, Some(ControlFlowEffect::Wait));
    assert!(!idle.force_present);
    assert!(!restored.force_present);
}

#[test]
fn suspended_host_deadlines_wake_without_rendering_or_spinning() {
    let now = Instant::now();
    let host = now + Duration::from_secs(1);
    let mut scheduler = FrameScheduler::default();
    scheduler.set_drawable(false);

    for (at, deadline, expected) in [
        (now, None, ControlFlowEffect::Wait),
        (now, Some(host), ControlFlowEffect::WaitUntil(host)),
        (host, Some(host), ControlFlowEffect::Wait),
        (host, Some(host), ControlFlowEffect::Wait),
        (
            host + Duration::from_secs(1),
            Some(host),
            ControlFlowEffect::Wait,
        ),
        (
            host + Duration::from_secs(1),
            Some(host + Duration::from_secs(2)),
            ControlFlowEffect::WaitUntil(host + Duration::from_secs(2)),
        ),
        (host + Duration::from_secs(1), None, ControlFlowEffect::Wait),
    ] {
        let effects = scheduler.about_to_wait(at, deadline);
        assert_eq!(effects.control_flow, Some(expected));
        assert!(!effects.request_redraw);
        assert!(!effects.force_present);
        assert!(!scheduler.redraw_pending());
    }
}

#[test]
fn suspended_host_wake_preserves_render_demand_and_coalesces_restoration() {
    let now = Instant::now();
    let runtime = now + Duration::from_secs(2);
    let host = now + Duration::from_secs(1);
    for demand in [
        ControlFlowEffect::Poll,
        ControlFlowEffect::WaitUntil(runtime),
    ] {
        let mut scheduler = FrameScheduler::default();
        scheduler.schedule(RuntimeEffects {
            request_redraw: true,
            control_flow: Some(demand),
            has_deferred_externals: true,
            ..RuntimeEffects::default()
        });
        // Arm a recovery deadline, then suspend with redraw still outstanding.
        scheduler.about_to_wait(now, None);
        scheduler.set_drawable(false);
        for at in [now, now + FrameScheduler::RECOVERY_INTERVAL] {
            let effects = scheduler.about_to_wait(at, Some(host));
            assert_eq!(
                effects.control_flow,
                Some(ControlFlowEffect::WaitUntil(host))
            );
            assert!(!effects.request_redraw);
            assert!(!effects.force_present);
        }
        assert!(scheduler.recovery_deadline().is_none());

        let restored = scheduler.set_drawable(true);
        assert!(restored.request_redraw);
        assert!(restored.force_present);
        assert!(!scheduler.set_drawable(true).request_redraw);
        let idle = scheduler.about_to_wait(host, Some(host));
        assert!(!idle.request_redraw);
        if demand == ControlFlowEffect::Poll {
            assert_eq!(idle.control_flow, Some(ControlFlowEffect::Poll));
        } else {
            assert_eq!(scheduler.runtime_deadline, Some(runtime));
            assert_eq!(
                idle.control_flow,
                Some(ControlFlowEffect::WaitUntil(
                    host + FrameScheduler::RECOVERY_INTERVAL
                ))
            );
        }
        let started = scheduler.frame_started(RuntimeEffects {
            has_deferred_externals: true,
            ..RuntimeEffects::default()
        });
        assert!(started.force_present);
        assert!(!started.request_redraw);
    }
}

#[test]
fn expired_host_deadline_does_not_mask_drawable_runtime_or_recovery_wait() {
    let now = Instant::now();
    let runtime = now + Duration::from_secs(2);
    for deferred in [false, true] {
        let mut scheduler = FrameScheduler::default();
        scheduler.schedule(RuntimeEffects {
            control_flow: Some(ControlFlowEffect::WaitUntil(runtime)),
            has_deferred_externals: deferred,
            ..RuntimeEffects::default()
        });
        for expired in [now - Duration::from_secs(1), now] {
            let effects = scheduler.about_to_wait(now, Some(expired));
            let expected = if deferred {
                now + FrameScheduler::RECOVERY_INTERVAL
            } else {
                runtime
            };
            assert_eq!(
                effects.control_flow,
                Some(ControlFlowEffect::WaitUntil(expected))
            );
            assert!(!effects.request_redraw);
        }
    }
}
