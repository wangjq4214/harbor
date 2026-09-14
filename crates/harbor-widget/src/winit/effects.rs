//! Application of window-local runtime effects for the owned winit host.

use crate::effects::{
    ClipboardEffect, ControlFlowEffect, CursorEffect, CursorShape, ImeEffect, RuntimeEffects,
};
use crate::layout::Point;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::{CursorIcon, Window};

trait WindowEffectSink {
    fn set_cursor(&mut self, cursor: CursorIcon);
    fn set_ime_allowed(&mut self, allowed: bool);
    fn set_ime_cursor_area(&mut self, position: LogicalPosition<f64>, size: LogicalSize<f64>);
    fn apply_clipboard(&mut self, effect: ClipboardEffect);
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

    fn apply_clipboard(&mut self, effect: ClipboardEffect) {
        apply_native_clipboard_effect(effect);
    }

    fn request_redraw(&mut self) {
        self.window.request_redraw();
    }
}

pub(super) fn apply_window_effects(
    window: &Window,
    effects: RuntimeEffects,
) -> Option<ControlFlowEffect> {
    apply_effects_to_sink(&mut NativeWindowEffectSink { window }, effects)
}

fn apply_effects_to_sink(
    sink: &mut impl WindowEffectSink,
    effects: RuntimeEffects,
) -> Option<ControlFlowEffect> {
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
    if let Some(clipboard) = effects.clipboard {
        sink.apply_clipboard(clipboard);
    }
    if effects.request_redraw {
        sink.request_redraw();
    }
    effects.control_flow
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

fn apply_native_clipboard_effect(effect: ClipboardEffect) {
    match effect {
        ClipboardEffect::Write(contents) => {
            let byte_len = contents.len();
            if let Err(error) =
                arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(contents))
            {
                tracing::warn!(%error, byte_len, "failed to write clipboard effect");
            }
        }
        ClipboardEffect::Read => {
            tracing::warn!(
                operation = "read",
                "clipboard effect deferred: host result channel is not implemented"
            );
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

        fn apply_clipboard(&mut self, effect: ClipboardEffect) {
            self.clipboard.push(effect);
        }

        fn request_redraw(&mut self) {
            self.redraws += 1;
        }
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
