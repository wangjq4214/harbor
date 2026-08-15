//! Terminal-owned boundary vocabulary for render geometry and input events.
//!
//! These types intentionally mirror current `UiEvent` semantics without depending
//! on widget types. Conversions from widget events live outside this crate.

use std::time::Instant;

/// Host-neutral scheduling snapshot: immediate redraw need plus optional blink deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameDemand {
    /// True when the host should request a frame before waiting on `deadline`.
    pub redraw_now: bool,
    /// Earliest next cursor-blink phase boundary, when blinking is active.
    pub deadline: Option<Instant>,
}

impl FrameDemand {
    /// Empty demand used when no Cursor/renderer is available.
    pub const fn empty() -> Self {
        Self {
            redraw_now: false,
            deadline: None,
        }
    }
}

/// A terminal render allocation expressed entirely in physical surface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderTarget {
    /// Physical origin of the allocation within the full surface (pixels).
    pub allocation_origin: (f32, f32),
    /// Physical size of the render allocation (pixels).
    pub allocation_size: (u32, u32),
    /// Full surface dimensions used for NDC normalization.
    pub surface_size: (u32, u32),
}

impl RenderTarget {
    /// Constructs a render target, preserving all supplied values exactly.
    pub fn new(
        allocation_origin: (f32, f32),
        allocation_size: (u32, u32),
        surface_size: (u32, u32),
    ) -> Self {
        Self {
            allocation_origin,
            allocation_size,
            surface_size,
        }
    }
}

/// The complete platform-independent input message accepted by the terminal engine.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalEvent {
    Keyboard(TerminalKeyboardEvent),
    Pointer(TerminalPointerEvent),
    Focus(TerminalFocusEvent),
}

/// A terminal-relevant keyboard or IME state transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalKeyboardEvent {
    KeyDown {
        key: TerminalKey,
        modifiers: TerminalModifiers,
    },
    KeyUp {
        key: TerminalKey,
        modifiers: TerminalModifiers,
    },
    Ime(String),
}

/// The finite key vocabulary currently understood by terminal encoding and scrollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKey {
    Tab,
    Enter,
    Space,
    Escape,
    Backspace,
    Insert,
    Delete,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    /// Character emitted from the numeric keypad.
    NumpadCharacter(char),
    /// Enter emitted from the numeric keypad.
    NumpadEnter,
    Character(char),
}

/// The modifier state accompanying a terminal key transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// A routed pointer sample delivered to the terminal allocation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalPointerEvent {
    pub position: (f32, f32),
    pub phase: TerminalPointerPhase,
    pub button: TerminalPointerButton,
    pub pointer_id: u64,
}

impl TerminalPointerEvent {
    pub fn new(
        position: (f32, f32),
        phase: TerminalPointerPhase,
        button: TerminalPointerButton,
        pointer_id: u64,
    ) -> Self {
        Self {
            position,
            phase,
            button,
            pointer_id,
        }
    }
}

/// The lifecycle or wheel-unit meaning of a pointer sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TerminalPointerPhase {
    Down,
    Move,
    Up,
    Cancel,
    WheelLine { dx: f32, dy: f32 },
    WheelPixel { dx: f32, dy: f32 },
}

/// The pointer button identity preserved at the terminal boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPointerButton {
    Left,
    Right,
    Middle,
}

/// A terminal focus transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalFocusEvent {
    Gained,
    Lost,
}
