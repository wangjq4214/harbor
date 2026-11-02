//! Input shorthand for mounted control tests; dispatch never implies an update.
use harbor_widget::{
    input::event::{
        Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
    },
    layout::Point,
    runtime::Runtime,
};
use std::time::Instant;

pub fn update(runtime: &mut Runtime) {
    runtime.update(Instant::now());
}

pub fn pointer(runtime: &mut Runtime, position: Point, phase: PointerPhase) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        position,
        phase,
        PointerButton::Left,
        0,
    )));
}

pub fn key(runtime: &mut Runtime, key: Key, shift: bool) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key,
        modifiers: Modifiers {
            shift,
            ..Modifiers::default()
        },
    }));
}
