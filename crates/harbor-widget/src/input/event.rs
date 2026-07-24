use crate::layout::Point;

// ── UiEvent ─────────────────────────────────────────────────────────────────

/// All input events dispatched through the widget tree.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    Pointer(PointerEvent),
    Keyboard(KeyboardEvent),
    Focus(FocusEvent),
}

// ── PointerEvent ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct PointerEvent {
    pub position: Point,
    pub phase: PointerPhase,
    pub button: PointerButton,
    pub pointer_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
    Wheel { dx: f32, dy: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}

// ── KeyboardEvent ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum KeyboardEvent {
    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
    Ime(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    Enter,
    Space,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Character(char),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

// ── FocusEvent ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusEvent {
    Gained,
    Lost,
}

// ── UiEvent helpers ─────────────────────────────────────────────────────────

impl UiEvent {
    /// Returns the pointer_id if this is a Pointer event.
    pub fn pointer_id(&self) -> Option<u64> {
        match self {
            UiEvent::Pointer(pe) => Some(pe.pointer_id),
            _ => None,
        }
    }

    /// Returns true if this is a pointer event with the given phase.
    pub fn is_pointer_phase(&self, phase: PointerPhase) -> bool {
        matches!(self, UiEvent::Pointer(pe) if pe.phase == phase)
    }
}

impl PointerEvent {
    pub fn new(
        position: Point,
        phase: PointerPhase,
        button: PointerButton,
        pointer_id: u64,
    ) -> Self {
        PointerEvent {
            position,
            phase,
            button,
            pointer_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_event_construction() {
        let evt = PointerEvent::new(
            Point::new(100.0, 200.0),
            PointerPhase::Down,
            PointerButton::Left,
            1,
        );
        assert_eq!(evt.position, Point::new(100.0, 200.0));
        assert_eq!(evt.phase, PointerPhase::Down);
        assert_eq!(evt.button, PointerButton::Left);
        assert_eq!(evt.pointer_id, 1);
    }

    #[test]
    fn ui_event_pointer_id() {
        let pe = PointerEvent::new(Point::ZERO, PointerPhase::Move, PointerButton::Left, 42);
        let event = UiEvent::Pointer(pe);
        assert_eq!(event.pointer_id(), Some(42));

        let kb = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        });
        assert_eq!(kb.pointer_id(), None);
    }

    #[test]
    fn is_pointer_phase_match() {
        let event = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Up,
            PointerButton::Left,
            1,
        ));
        assert!(event.is_pointer_phase(PointerPhase::Up));
        assert!(!event.is_pointer_phase(PointerPhase::Down));
    }

    #[test]
    fn modifiers_default() {
        let m = Modifiers::default();
        assert!(!m.shift);
        assert!(!m.ctrl);
        assert!(!m.alt);
        assert!(!m.meta);
    }

    #[test]
    fn key_variants() {
        assert_eq!(Key::Tab, Key::Tab);
        assert_ne!(Key::Enter, Key::Space);
    }

    #[test]
    fn pointer_phase_wheel_carries_delta() {
        let phase = PointerPhase::Wheel { dx: 0.0, dy: 10.0 };
        assert_eq!(phase, PointerPhase::Wheel { dx: 0.0, dy: 10.0 });
    }
}
