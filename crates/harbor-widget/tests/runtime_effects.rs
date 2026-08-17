use harbor_widget::effects::{
    ClipboardEffect, ControlFlowEffect, CursorEffect, CursorShape, ExternalInvalidation, ImeEffect,
    RuntimeEffects,
};
use harbor_widget::layout::Point;
use harbor_widget::runtime::Runtime;
use harbor_widget::widgets::sized_box::SizedBox;
use std::time::{Duration, Instant};

#[test]
fn effects_default_is_inert() {
    let effects = RuntimeEffects::default();
    assert!(!effects.request_redraw);
    assert!(effects.control_flow.is_none());
    assert!(effects.cursor.is_none());
    assert!(effects.ime.is_none());
    assert!(effects.clipboard.is_none());
    assert!(effects.ordinary_present_eligible);
    assert!(!effects.force_present);
    assert!(effects.is_noop());
}

#[test]
fn effects_merge_coalesces_redraw_and_keeps_latest_commands() {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut earlier = RuntimeEffects {
        request_redraw: false,
        control_flow: Some(ControlFlowEffect::wait()),
        cursor: Some(CursorEffect::set_cursor(CursorShape::Default)),
        ime: Some(ImeEffect::set_allowed(false)),
        clipboard: Some(ClipboardEffect::read()),
        ..RuntimeEffects::default()
    };
    let later = RuntimeEffects {
        request_redraw: true,
        control_flow: Some(ControlFlowEffect::wait_until(deadline)),
        cursor: Some(CursorEffect::set_cursor(CursorShape::Pointer)),
        ime: Some(ImeEffect::set_position(Point::new(4.0, 8.0))),
        clipboard: Some(ClipboardEffect::write("copied")),
        ..RuntimeEffects::default()
    };

    earlier.merge(later);

    assert!(earlier.request_redraw);
    assert_eq!(
        earlier.control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );
    assert_eq!(
        earlier.cursor,
        Some(CursorEffect::Set(CursorShape::Pointer))
    );
    assert_eq!(
        earlier.ime,
        Some(ImeEffect {
            allowed: Some(false),
            position: Some(Point::new(4.0, 8.0)),
        })
    );
    assert_eq!(
        earlier.clipboard,
        Some(ClipboardEffect::Write("copied".into()))
    );
}

#[test]
fn ime_allowance_and_position_survive_one_merged_batch() {
    let mut effects = RuntimeEffects {
        ime: Some(ImeEffect::set_allowed(true)),
        ..RuntimeEffects::default()
    };
    effects.merge(RuntimeEffects {
        ime: Some(ImeEffect::set_position(Point::new(12.0, 18.0))),
        ..RuntimeEffects::default()
    });

    assert_eq!(
        effects.ime,
        Some(ImeEffect {
            allowed: Some(true),
            position: Some(Point::new(12.0, 18.0)),
        })
    );
}

#[test]
fn ime_merge_replaces_latest_allowance_without_losing_position() {
    let mut effects = RuntimeEffects {
        ime: Some(ImeEffect {
            allowed: Some(true),
            position: Some(Point::new(4.0, 8.0)),
        }),
        ..RuntimeEffects::default()
    };

    effects.merge(RuntimeEffects {
        ime: Some(ImeEffect::set_allowed(false)),
        ..RuntimeEffects::default()
    });

    assert_eq!(
        effects.ime,
        Some(ImeEffect {
            allowed: Some(false),
            position: Some(Point::new(4.0, 8.0)),
        })
    );
}

#[test]
fn effects_merge_retains_commands_missing_from_later_batch() {
    let mut earlier = RuntimeEffects {
        cursor: Some(CursorEffect::reset()),
        clipboard: Some(ClipboardEffect::write("keep")),
        ..RuntimeEffects::default()
    };
    earlier.merge(RuntimeEffects::request_redraw());

    assert!(earlier.request_redraw);
    assert_eq!(earlier.cursor, Some(CursorEffect::Reset));
    assert_eq!(
        earlier.clipboard,
        Some(ClipboardEffect::Write("keep".into()))
    );
}

#[test]
fn should_merge_effects_without_mutating_inputs_and_support_poll() {
    let earlier = RuntimeEffects {
        control_flow: Some(ControlFlowEffect::poll()),
        ..RuntimeEffects::default()
    };
    let later = RuntimeEffects::request_redraw();

    let merged = earlier.clone().merged(&later);

    assert!(merged.request_redraw);
    assert_eq!(merged.control_flow, Some(ControlFlowEffect::Poll));
    assert_eq!(earlier.control_flow, Some(ControlFlowEffect::Poll));
    assert_eq!(later, RuntimeEffects::request_redraw());
}

#[test]
fn runtime_external_invalidation_is_noop_without_root() {
    let mut runtime = Runtime::new();
    let effects = runtime.invalidate_external(ExternalInvalidation::new());
    assert!(effects.is_noop());
}

#[test]
fn runtime_external_invalidation_requests_redraw_with_root() {
    let mut runtime = Runtime::new();
    runtime.set_root(SizedBox::new(harbor_widget::layout::Size::new(20.0, 20.0)));
    runtime.update(Instant::now());

    let effects = runtime.invalidate_external(ExternalInvalidation::new());
    assert!(effects.request_redraw);
    assert!(runtime.update(Instant::now()).request_redraw);
}

#[test]
fn should_coalesce_repeated_external_invalidations_into_one_update() {
    let mut runtime = Runtime::new();
    runtime.set_root(SizedBox::new(harbor_widget::layout::Size::new(20.0, 20.0)));
    assert!(runtime.update(Instant::now()).request_redraw);

    let first = runtime.invalidate_external(ExternalInvalidation::new());
    let second = runtime.invalidate_external(ExternalInvalidation::new());

    assert!(first.request_redraw);
    assert!(second.request_redraw);
    assert!(runtime.update(Instant::now()).request_redraw);
    assert!(!runtime.update(Instant::now()).request_redraw);
}

#[test]
fn control_flow_arbitrate_is_commutative_and_prefers_poll_then_earliest_deadline() {
    let now = Instant::now();
    let early = now + Duration::from_secs(1);
    let late = now + Duration::from_secs(2);

    assert_eq!(
        ControlFlowEffect::Wait.arbitrate(ControlFlowEffect::Wait),
        ControlFlowEffect::Wait
    );
    assert_eq!(
        ControlFlowEffect::WaitUntil(late).arbitrate(ControlFlowEffect::WaitUntil(early)),
        ControlFlowEffect::WaitUntil(early)
    );
    assert_eq!(
        ControlFlowEffect::WaitUntil(early).arbitrate(ControlFlowEffect::WaitUntil(late)),
        ControlFlowEffect::WaitUntil(early)
    );
    assert_eq!(
        ControlFlowEffect::WaitUntil(early).arbitrate(ControlFlowEffect::WaitUntil(early)),
        ControlFlowEffect::WaitUntil(early)
    );
    assert_eq!(
        ControlFlowEffect::WaitUntil(early).arbitrate(ControlFlowEffect::Wait),
        ControlFlowEffect::WaitUntil(early)
    );
    assert_eq!(
        ControlFlowEffect::Wait.arbitrate(ControlFlowEffect::WaitUntil(early)),
        ControlFlowEffect::WaitUntil(early)
    );
    assert_eq!(
        ControlFlowEffect::Poll.arbitrate(ControlFlowEffect::WaitUntil(late)),
        ControlFlowEffect::Poll
    );
    assert_eq!(
        ControlFlowEffect::WaitUntil(late).arbitrate(ControlFlowEffect::Poll),
        ControlFlowEffect::Poll
    );
    assert_eq!(
        ControlFlowEffect::Poll.arbitrate(ControlFlowEffect::Wait),
        ControlFlowEffect::Poll
    );
    assert_eq!(
        ControlFlowEffect::Wait.arbitrate(ControlFlowEffect::Poll),
        ControlFlowEffect::Poll
    );
    assert_eq!(
        ControlFlowEffect::Poll.arbitrate(ControlFlowEffect::Poll),
        ControlFlowEffect::Poll
    );

    // Sequential merge still keeps the later turn; arbitration is independent.
    let mut earlier = RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Poll),
        ..RuntimeEffects::default()
    };
    earlier.merge(RuntimeEffects {
        control_flow: Some(ControlFlowEffect::Wait),
        ..RuntimeEffects::default()
    });
    assert_eq!(earlier.control_flow, Some(ControlFlowEffect::Wait));
}

#[test]
fn should_and_eligibility_and_or_force_when_effects_merge() {
    let mut deferred = RuntimeEffects {
        ordinary_present_eligible: false,
        ..RuntimeEffects::default()
    };
    deferred.merge(RuntimeEffects::request_redraw());

    assert!(deferred.request_redraw);
    assert!(!deferred.ordinary_present_eligible);
    assert!(!deferred.force_present);
    assert!(!deferred.is_noop());

    deferred.merge(RuntimeEffects::force_present());

    assert!(deferred.request_redraw);
    assert!(!deferred.ordinary_present_eligible);
    assert!(deferred.force_present);
}

#[test]
fn should_not_treat_deferred_only_effects_as_noop() {
    let deferred = RuntimeEffects {
        ordinary_present_eligible: false,
        ..RuntimeEffects::default()
    };

    assert!(!deferred.is_noop());
    assert!(!deferred.request_redraw);
}

#[test]
fn should_request_redraw_when_force_present_is_constructed() {
    let effects = RuntimeEffects::force_present();

    assert!(effects.request_redraw);
    assert!(effects.force_present);
    assert!(effects.ordinary_present_eligible);
}

#[test]
fn should_keep_eligible_present_when_request_redraw_is_constructed() {
    // Arrange / Act
    let effects = RuntimeEffects::request_redraw();

    // Assert
    assert!(effects.request_redraw);
    assert!(effects.ordinary_present_eligible);
    assert!(!effects.force_present);
}

#[test]
fn should_clear_eligibility_when_later_batch_is_deferred() {
    // Arrange
    let mut eligible = RuntimeEffects::request_redraw();

    // Act
    eligible.merge(RuntimeEffects {
        ordinary_present_eligible: false,
        ..RuntimeEffects::default()
    });

    // Assert
    assert!(eligible.request_redraw);
    assert!(!eligible.ordinary_present_eligible);
    assert!(!eligible.force_present);
}

#[test]
fn should_not_treat_force_present_as_noop() {
    // Arrange
    let effects = RuntimeEffects {
        force_present: true,
        ..RuntimeEffects::default()
    };

    // Act / Assert
    assert!(!effects.is_noop());
    assert!(!effects.request_redraw);
}
