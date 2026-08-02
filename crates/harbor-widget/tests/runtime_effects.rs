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
    };
    let later = RuntimeEffects {
        request_redraw: true,
        control_flow: Some(ControlFlowEffect::wait_until(deadline)),
        cursor: Some(CursorEffect::set_cursor(CursorShape::Pointer)),
        ime: Some(ImeEffect::set_position(Point::new(4.0, 8.0))),
        clipboard: Some(ClipboardEffect::write("copied")),
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
