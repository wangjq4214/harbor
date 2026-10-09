//! Application of window-local runtime effects for the owned winit host.

use crate::effects::{
    ClipboardEffect, ClipboardEffectError, ControlFlowEffect, CursorEffect, CursorShape, ImeEffect,
    RuntimeEffects,
};
use crate::layout::Point;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::{CursorIcon, Window};

trait WindowEffectSink {
    fn set_cursor(&mut self, cursor: CursorIcon);
    fn set_ime_allowed(&mut self, allowed: bool);
    fn set_ime_cursor_area(&mut self, position: LogicalPosition<f64>, size: LogicalSize<f64>);
    fn apply_clipboard(&mut self, effect: ClipboardEffect) -> Result<(), ClipboardEffectError>;
    fn request_redraw(&mut self);
}

struct NativeWindowEffectSink<'a> {
    window: &'a Window,
}

impl WindowEffectSink for NativeWindowEffectSink<'_> {
    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.window.set_cursor(cursor);
    }

    fn set_ime_allowed(&mut self, allowed: bool) {
        self.window.set_ime_allowed(allowed);
    }

    fn set_ime_cursor_area(&mut self, position: LogicalPosition<f64>, size: LogicalSize<f64>) {
        self.window.set_ime_cursor_area(position, size);
    }

    fn apply_clipboard(&mut self, effect: ClipboardEffect) -> Result<(), ClipboardEffectError> {
        apply_native_clipboard_effect(effect)
    }

    fn request_redraw(&mut self) {
        self.window.request_redraw();
    }
}

pub(super) fn apply_window_effects(
    window: &Window,
    effects: RuntimeEffects,
) -> Option<ControlFlowEffect> {
    apply_window_effects_checked(window, effects).0
}

pub(super) fn apply_window_effects_checked(
    window: &Window,
    effects: RuntimeEffects,
) -> (
    Option<ControlFlowEffect>,
    Option<Result<(), ClipboardEffectError>>,
) {
    apply_effects_to_sink_checked(&mut NativeWindowEffectSink { window }, effects)
}

#[cfg(test)]
fn apply_effects_to_sink(
    sink: &mut impl WindowEffectSink,
    effects: RuntimeEffects,
) -> Option<ControlFlowEffect> {
    apply_effects_to_sink_checked(sink, effects).0
}

fn apply_effects_to_sink_checked(
    sink: &mut impl WindowEffectSink,
    effects: RuntimeEffects,
) -> (
    Option<ControlFlowEffect>,
    Option<Result<(), ClipboardEffectError>>,
) {
    if let Some(cursor) = effects.cursor {
        sink.set_cursor(cursor_icon(cursor));
    }
    if let Some(ImeEffect { allowed, position }) = effects.ime {
        if let Some(allowed) = allowed {
            sink.set_ime_allowed(allowed);
        }
        if let Some(position) = position {
            let (position, size) = ime_cursor_area(position);
            sink.set_ime_cursor_area(position, size);
        }
    }
    let clipboard_result = effects
        .clipboard
        .map(|clipboard| sink.apply_clipboard(clipboard));
    if effects.request_redraw {
        sink.request_redraw();
    }
    (effects.control_flow, clipboard_result)
}

fn cursor_icon(effect: CursorEffect) -> CursorIcon {
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

fn ime_cursor_area(position: Point) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    (
        LogicalPosition::new(position.x as f64, position.y as f64),
        LogicalSize::new(1.0, 1.0),
    )
}

fn apply_native_clipboard_effect(effect: ClipboardEffect) -> Result<(), ClipboardEffectError> {
    apply_clipboard_effect_with(effect, |contents| {
        arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(contents))
    })
}

fn apply_clipboard_effect_with<E>(
    effect: ClipboardEffect,
    write: impl FnOnce(String) -> Result<(), E>,
) -> Result<(), ClipboardEffectError> {
    match effect {
        ClipboardEffect::Write(contents) => {
            let byte_len = contents.len();
            write(contents).map_err(|_| {
                // Backend errors may contain the payload: never log or retain them.
                tracing::warn!(byte_len, "failed to write clipboard effect");
                ClipboardEffectError::WriteFailed
            })
        }
        ClipboardEffect::Read => {
            tracing::warn!(
                operation = "read",
                "clipboard effect deferred: host result channel is not implemented"
            );
            Err(ClipboardEffectError::ReadUnsupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ImeEffect;
    use std::time::Instant;

    #[derive(Default)]
    struct RecordingSink {
        cursors: Vec<CursorIcon>,
        ime_allowed: Vec<bool>,
        ime_areas: Vec<(LogicalPosition<f64>, LogicalSize<f64>)>,
        clipboard: Vec<ClipboardEffect>,
        redraws: usize,
        fail_clipboard: bool,
    }

    impl WindowEffectSink for RecordingSink {
        fn set_cursor(&mut self, cursor: CursorIcon) {
            self.cursors.push(cursor);
        }

        fn set_ime_allowed(&mut self, allowed: bool) {
            self.ime_allowed.push(allowed);
        }

        fn set_ime_cursor_area(&mut self, position: LogicalPosition<f64>, size: LogicalSize<f64>) {
            self.ime_areas.push((position, size));
        }

        fn apply_clipboard(&mut self, effect: ClipboardEffect) -> Result<(), ClipboardEffectError> {
            self.clipboard.push(effect);
            if self.fail_clipboard {
                Err(ClipboardEffectError::WriteFailed)
            } else {
                Ok(())
            }
        }

        fn request_redraw(&mut self) {
            self.redraws += 1;
        }
    }

    #[test]
    fn checked_batch_preserves_success_failure_and_empty_writes() {
        for text in ["fixture text\n\t", ""] {
            for fail_clipboard in [false, true] {
                let mut sink = RecordingSink {
                    fail_clipboard,
                    ..RecordingSink::default()
                };
                let (wait, result) = apply_effects_to_sink_checked(
                    &mut sink,
                    RuntimeEffects {
                        clipboard: Some(ClipboardEffect::write(text)),
                        ..RuntimeEffects::default()
                    },
                );
                assert_eq!(wait, None);
                assert_eq!(
                    result,
                    Some(if fail_clipboard {
                        Err(ClipboardEffectError::WriteFailed)
                    } else {
                        Ok(())
                    })
                );
                assert_eq!(sink.clipboard, vec![ClipboardEffect::write(text)]);
            }
        }
        let mut sink = RecordingSink::default();
        assert_eq!(
            apply_effects_to_sink_checked(&mut sink, RuntimeEffects::default()).1,
            None
        );
    }

    #[test]
    fn native_effect_adapter_preserves_backend_result_without_payload_error() {
        for text in ["fixture text", ""] {
            let result = apply_clipboard_effect_with(ClipboardEffect::write(text), |actual| {
                assert_eq!(actual, text);
                Ok::<_, String>(())
            });
            assert_eq!(result, Ok(()));
            let result = apply_clipboard_effect_with(ClipboardEffect::write(text), |actual| {
                assert_eq!(actual, text);
                Err(format!("backend error containing {actual}"))
            });
            assert_eq!(result, Err(ClipboardEffectError::WriteFailed));
            assert_eq!(result.unwrap_err().to_string(), "clipboard write failed");
        }
        assert_eq!(
            apply_clipboard_effect_with(ClipboardEffect::Read, |_| -> Result<(), ()> {
                panic!("unsupported reads must not invoke the write backend")
            }),
            Err(ClipboardEffectError::ReadUnsupported)
        );
    }

    #[test]
    fn cursor_mapping_covers_every_runtime_shape() {
        let cases = [
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

        for (effect, expected) in cases {
            assert_eq!(cursor_icon(effect), expected);
        }
    }

    #[test]
    fn ime_area_uses_logical_coordinates() {
        assert_eq!(
            ime_cursor_area(Point::new(12.5, 8.0)),
            (LogicalPosition::new(12.5, 8.0), LogicalSize::new(1.0, 1.0))
        );
    }

    #[test]
    fn effect_batch_applies_each_window_operation_once_and_returns_only_wait() {
        let deadline = Instant::now();
        let mut sink = RecordingSink::default();
        let effects = RuntimeEffects {
            request_redraw: true,
            control_flow: Some(ControlFlowEffect::WaitUntil(deadline)),
            cursor: Some(CursorEffect::Set(CursorShape::Text)),
            ime: Some(ImeEffect {
                allowed: Some(true),
                position: Some(Point::new(12.5, 8.0)),
            }),
            clipboard: Some(ClipboardEffect::write("secret payload")),
            ..RuntimeEffects::default()
        };

        let wait = apply_effects_to_sink(&mut sink, effects);

        assert_eq!(wait, Some(ControlFlowEffect::WaitUntil(deadline)));
        assert_eq!(sink.cursors, vec![CursorIcon::Text]);
        assert_eq!(sink.ime_allowed, vec![true]);
        assert_eq!(
            sink.ime_areas,
            vec![(LogicalPosition::new(12.5, 8.0), LogicalSize::new(1.0, 1.0))]
        );
        assert_eq!(
            sink.clipboard,
            vec![ClipboardEffect::write("secret payload")]
        );
        assert_eq!(sink.redraws, 1);
    }

    #[test]
    fn noop_effect_batch_does_not_touch_window_sink() {
        let mut sink = RecordingSink::default();
        assert_eq!(
            apply_effects_to_sink(&mut sink, RuntimeEffects::default()),
            None
        );
        assert!(sink.cursors.is_empty());
        assert!(sink.ime_allowed.is_empty());
        assert!(sink.ime_areas.is_empty());
        assert!(sink.clipboard.is_empty());
        assert_eq!(sink.redraws, 0);
    }
}
