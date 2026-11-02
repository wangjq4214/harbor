use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::Point;

pub fn pointer_event(
    position: Point,
    phase: PointerPhase,
    button: PointerButton,
    pointer_id: u64,
) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(position, phase, button, pointer_id))
}

pub fn key_down(key: Key) -> UiEvent {
    UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
    })
}
