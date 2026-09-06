//! Winit host effect translation and application: cursor, IME, control flow, clipboard.

use harbor_widget::effects::{
    ClipboardEffect, ControlFlowEffect, CursorEffect, CursorShape, ImeEffect, RuntimeEffects,
};
use harbor_widget::layout::Point;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{CursorIcon, Window};

pub(crate) fn control_flow_for_effect(effect: ControlFlowEffect) -> ControlFlow {
    match effect {
        ControlFlowEffect::Wait => ControlFlow::Wait,
        ControlFlowEffect::WaitUntil(deadline) => ControlFlow::WaitUntil(deadline),
        ControlFlowEffect::Poll => ControlFlow::Poll,
    }
}

pub(crate) fn cursor_icon_for_effect(effect: CursorEffect) -> CursorIcon {
    match effect {
        CursorEffect::Reset | CursorEffect::Set(CursorShape::Default) => CursorIcon::Default,
        CursorEffect::Set(CursorShape::Pointer) => CursorIcon::Pointer,
        CursorEffect::Set(CursorShape::Text) => CursorIcon::Text,
        CursorEffect::Set(CursorShape::Crosshair) => CursorIcon::Crosshair,
        CursorEffect::Set(CursorShape::Grab) => CursorIcon::Grab,
        CursorEffect::Set(CursorShape::Grabbing) => CursorIcon::Grabbing,
        CursorEffect::Set(CursorShape::NotAllowed) => CursorIcon::NotAllowed,
        CursorEffect::Set(CursorShape::ResizeHorizontal) => CursorIcon::EwResize,
        CursorEffect::Set(CursorShape::ResizeVertical) => CursorIcon::NsResize,
    }
}

pub(crate) fn ime_cursor_area(position: Point) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    (
        LogicalPosition::new(position.x as f64, position.y as f64),
        LogicalSize::new(1.0, 1.0),
    )
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ClipboardHostAction {
    Deferred(ClipboardEffect),
}

/// Returns the operation metadata safe to include in clipboard logs.
pub(crate) fn clipboard_log_metadata(effect: &ClipboardEffect) -> (&'static str, usize) {
    match effect {
        ClipboardEffect::Read => ("read", 0),
        ClipboardEffect::Write(contents) => ("write", contents.len()),
    }
}

/// Logs a clipboard effect that remains deferred until a host result channel
/// exists. In particular, a read is never performed and then discarded by this
/// application shell.
pub(crate) fn log_deferred_clipboard_effect(effect: &ClipboardEffect) {
    let (operation, byte_len) = clipboard_log_metadata(effect);
    tracing::warn!(
        operation,
        byte_len,
        "clipboard effect deferred: host result channel is not implemented"
    );
}

/// Keeps an owned clipboard effect at the platform-neutral boundary until a
/// host result channel exists.
pub(crate) fn apply_clipboard_effect(effect: ClipboardEffect) -> ClipboardHostAction {
    log_deferred_clipboard_effect(&effect);
    ClipboardHostAction::Deferred(effect)
}

/// Applies adapter-authorized window effects without calculating wait policy.
pub(crate) fn apply_window_effects(window: &Window, effects: &RuntimeEffects) {
    if let Some(cursor) = effects.cursor {
        window.set_cursor(cursor_icon_for_effect(cursor));
    }
    if let Some(ImeEffect { allowed, position }) = effects.ime {
        if let Some(allowed) = allowed {
            window.set_ime_allowed(allowed);
        }
        if let Some(position) = position {
            let (position, size) = ime_cursor_area(position);
            window.set_ime_cursor_area(position, size);
        }
    }
    if let Some(clipboard) = effects.clipboard.clone() {
        match clipboard {
            ClipboardEffect::Write(contents) => {
                if let Err(error) = arboard::Clipboard::new()
                    .and_then(|mut clipboard| clipboard.set_text(contents))
                {
                    tracing::warn!(error = %error, "failed to write clipboard effect");
                }
            }
            ClipboardEffect::Read => {
                let _ = apply_clipboard_effect(ClipboardEffect::Read);
            }
        }
    }
    if effects.request_redraw {
        tracing::trace!("requesting redraw");
        window.request_redraw();
    }
}

pub(crate) fn apply_control_flow(event_loop: &ActiveEventLoop, effect: ControlFlowEffect) {
    event_loop.set_control_flow(control_flow_for_effect(effect));
}

/// Applies window effects and updates control flow when requested.
pub(crate) fn apply_effects(
    window: &Window,
    effects: &RuntimeEffects,
    event_loop: &ActiveEventLoop,
) {
    apply_window_effects(window, effects);
    if let Some(control_flow) = effects.control_flow {
        apply_control_flow(event_loop, control_flow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    struct FieldRecorder<'a> {
        fields: &'a mut std::collections::HashMap<String, String>,
    }

    impl tracing::field::Visit for FieldRecorder<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    #[derive(Clone, Default)]
    struct ClipboardCapture {
        events: Arc<Mutex<Vec<std::collections::HashMap<String, String>>>>,
    }

    impl<S> tracing_subscriber::layer::Layer<S> for ClipboardCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if !event.metadata().target().contains("effects")
                && !event.metadata().target().contains("app")
            {
                return;
            }
            let mut fields = std::collections::HashMap::new();
            event.record(&mut FieldRecorder {
                fields: &mut fields,
            });
            self.events
                .lock()
                .expect("clipboard capture lock")
                .push(fields);
        }
    }

    fn with_clipboard_capture<R>(f: impl FnOnce() -> R) -> (R, ClipboardCapture) {
        use tracing_subscriber::layer::SubscriberExt;
        let capture = ClipboardCapture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let result = tracing::subscriber::with_default(subscriber, f);
        (result, capture)
    }

    #[test]
    fn control_flow_arbitration_prefers_poll_then_earliest_deadline() {
        // Arrange
        let now = Instant::now();
        let early = now + Duration::from_secs(1);
        let late = now + Duration::from_secs(2);

        // Act and assert each arbitration combination independently.
        assert_eq!(
            ControlFlowEffect::Wait.arbitrate(ControlFlowEffect::Wait),
            ControlFlowEffect::Wait
        );
        assert_eq!(
            ControlFlowEffect::WaitUntil(late).arbitrate(ControlFlowEffect::WaitUntil(early)),
            ControlFlowEffect::WaitUntil(early)
        );
        assert_eq!(
            ControlFlowEffect::WaitUntil(early).arbitrate(ControlFlowEffect::Wait),
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
    }

    #[test]
    fn clipboard_log_metadata_excludes_write_payload() {
        let secret = "private clipboard contents";
        assert_eq!(
            clipboard_log_metadata(&ClipboardEffect::write(secret)),
            ("write", secret.len())
        );
        assert_eq!(clipboard_log_metadata(&ClipboardEffect::Read), ("read", 0));
    }

    #[test]
    fn clipboard_effects_are_explicitly_deferred_without_a_discarding_read() {
        assert_eq!(
            apply_clipboard_effect(ClipboardEffect::Read),
            ClipboardHostAction::Deferred(ClipboardEffect::Read)
        );
        assert_eq!(
            apply_clipboard_effect(ClipboardEffect::write("copied")),
            ClipboardHostAction::Deferred(ClipboardEffect::write("copied"))
        );
    }

    #[test]
    fn clipboard_warning_logs_metadata_without_write_payload() {
        // Arrange: use a value that would make accidental payload logging obvious.
        let secret = "clipboard-secret-not-for-logs";

        // Act: defer the effect under a scoped tracing subscriber.
        let (action, capture) =
            with_clipboard_capture(|| apply_clipboard_effect(ClipboardEffect::write(secret)));

        // Assert: the host action remains deferred and the warning exposes only
        // operation metadata, never the clipboard contents.
        assert_eq!(
            action,
            ClipboardHostAction::Deferred(ClipboardEffect::write(secret))
        );
        let events = capture.events.lock().expect("clipboard events lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].get("operation"), Some(&"write".to_string()));
        assert_eq!(events[0].get("byte_len"), Some(&secret.len().to_string()));
        assert!(!format!("{events:?}").contains(secret));
    }

    #[test]
    fn runtime_effect_host_mapping_is_pure_and_complete_without_native_handles() {
        let deadline = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::Wait),
            ControlFlow::Wait
        );
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::WaitUntil(deadline)),
            ControlFlow::WaitUntil(deadline)
        );
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::Poll),
            ControlFlow::Poll
        );

        let cursor_mappings = [
            (CursorEffect::Reset, CursorIcon::Default),
            (CursorEffect::Set(CursorShape::Default), CursorIcon::Default),
            (CursorEffect::Set(CursorShape::Pointer), CursorIcon::Pointer),
            (CursorEffect::Set(CursorShape::Text), CursorIcon::Text),
            (
                CursorEffect::Set(CursorShape::Crosshair),
                CursorIcon::Crosshair,
            ),
            (CursorEffect::Set(CursorShape::Grab), CursorIcon::Grab),
            (
                CursorEffect::Set(CursorShape::Grabbing),
                CursorIcon::Grabbing,
            ),
            (
                CursorEffect::Set(CursorShape::NotAllowed),
                CursorIcon::NotAllowed,
            ),
            (
                CursorEffect::Set(CursorShape::ResizeHorizontal),
                CursorIcon::EwResize,
            ),
            (
                CursorEffect::Set(CursorShape::ResizeVertical),
                CursorIcon::NsResize,
            ),
        ];
        for (effect, expected) in cursor_mappings {
            assert_eq!(cursor_icon_for_effect(effect), expected);
        }
        assert_eq!(
            ime_cursor_area(Point::new(12.5, 8.0)),
            (LogicalPosition::new(12.5, 8.0), LogicalSize::new(1.0, 1.0))
        );

        let effects = RuntimeEffects {
            request_redraw: true,
            control_flow: Some(ControlFlowEffect::Poll),
            cursor: Some(CursorEffect::Set(CursorShape::Text)),
            ime: Some(ImeEffect::set_allowed(true)),
            clipboard: Some(ClipboardEffect::write("copied")),
            ..RuntimeEffects::default()
        };
        assert!(effects.request_redraw);
        assert_eq!(effects.control_flow, Some(ControlFlowEffect::Poll));
        assert_eq!(effects.cursor, Some(CursorEffect::Set(CursorShape::Text)));
        assert_eq!(effects.ime, Some(ImeEffect::set_allowed(true)));
        assert_eq!(effects.clipboard, Some(ClipboardEffect::write("copied")));
    }
}
