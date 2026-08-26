//! Terminal-owned boundary vocabulary for render geometry and input events.
//!
//! These types intentionally mirror current `UiEvent` semantics without depending
//! on widget types. Conversions from widget events live outside this crate.

use std::time::Instant;

/// Terminal-owned visual policy for the default cell background.
///
/// The terminal keeps the tint and fallback semantics independent from any
/// window, surface, or GPU type. Hosts only report whether a compositor
/// backdrop is actually available for the current window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalAppearance {
    rgba: [f32; 4],
}

impl TerminalAppearance {
    /// Creates an appearance from a straight-alpha RGBA tint.
    pub const fn new(rgba: [f32; 4]) -> Self {
        Self { rgba }
    }

    /// Returns the configured tint used by a host compositor API.
    pub const fn rgba(self) -> [f32; 4] {
        self.rgba
    }

    /// Selects the default clear color for the current host environment.
    ///
    /// With Acrylic available the configured tint remains translucent. When
    /// it is unavailable, the same RGB is made opaque for a readable fallback.
    pub const fn clear_rgba(self, backdrop_available: bool) -> [f32; 4] {
        if backdrop_available {
            self.rgba
        } else {
            [self.rgba[0], self.rgba[1], self.rgba[2], 1.0]
        }
    }
}

impl Default for TerminalAppearance {
    fn default() -> Self {
        Self::new(harbor_config::BACKGROUND)
    }
}

/// Host-neutral scheduling snapshot: immediate redraw need plus optional blink deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameDemand {
    /// True when the host should request a frame before waiting on `deadline`.
    pub redraw_now: bool,
    /// Earliest next cursor-blink phase boundary, when blinking is active.
    pub deadline: Option<Instant>,
    /// False when an ordinary present should be deferred. Default empty demand is eligible.
    pub ordinary_present_eligible: bool,
}

impl FrameDemand {
    /// Empty demand used when no Cursor/renderer is available.
    pub const fn empty() -> Self {
        Self {
            redraw_now: false,
            deadline: None,
            ordinary_present_eligible: true,
        }
    }
}

impl Default for FrameDemand {
    fn default() -> Self {
        Self::empty()
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
    /// Scale used to convert widget logical pointer coordinates to pixels.
    pub scale_factor: f32,
}

impl RenderTarget {
    /// Constructs a render target, preserving all supplied values exactly.
    pub fn new(
        allocation_origin: (f32, f32),
        allocation_size: (u32, u32),
        surface_size: (u32, u32),
    ) -> Self {
        Self::new_with_scale(allocation_origin, allocation_size, surface_size, 1.0)
    }

    pub fn new_with_scale(
        allocation_origin: (f32, f32),
        allocation_size: (u32, u32),
        surface_size: (u32, u32),
        scale_factor: f32,
    ) -> Self {
        Self {
            allocation_origin,
            allocation_size,
            surface_size,
            scale_factor: scale_factor.max(0.001),
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

/// Host-neutral effects produced while handling one terminal event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalEventOutcome {
    pub redraw: bool,
    pub capture_pointer: Option<u64>,
    pub release_pointer: Option<u64>,
    pub clipboard_text: Option<String>,
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
    pub modifiers: TerminalModifiers,
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
            modifiers: TerminalModifiers::default(),
        }
    }

    pub fn with_modifiers(mut self, modifiers: TerminalModifiers) -> Self {
        self.modifiers = modifiers;
        self
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
    None,
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

#[cfg(test)]
mod tests {
    use super::{FrameDemand, TerminalAppearance};
    use std::time::Instant;

    #[test]
    fn should_return_translucent_tint_when_backdrop_is_available() {
        let appearance = TerminalAppearance::new([0.36, 0.20, 0.08, 0.25]);

        assert_eq!(appearance.clear_rgba(true), [0.36, 0.20, 0.08, 0.25]);
    }

    #[test]
    fn should_return_opaque_rgb_equivalent_when_backdrop_is_unavailable() {
        let appearance = TerminalAppearance::new([0.36, 0.20, 0.08, 0.25]);

        assert_eq!(appearance.clear_rgba(false), [0.36, 0.20, 0.08, 1.0]);
    }

    #[test]
    fn default_appearance_uses_configured_background() {
        assert_eq!(
            TerminalAppearance::default().rgba(),
            harbor_config::BACKGROUND
        );
    }

    #[test]
    fn should_report_eligible_present_when_frame_demand_is_empty() {
        // Arrange / Act
        let empty = FrameDemand::empty();

        // Assert
        assert_eq!(empty, FrameDemand::default());
        assert!(!empty.redraw_now);
        assert!(empty.deadline.is_none());
        assert!(empty.ordinary_present_eligible);
    }

    #[test]
    fn should_preserve_deferred_eligibility_when_frame_demand_is_constructed() {
        // Arrange
        let deadline = Instant::now();

        // Act
        let demand = FrameDemand {
            redraw_now: true,
            deadline: Some(deadline),
            ordinary_present_eligible: false,
        };

        // Assert
        assert!(demand.redraw_now);
        assert_eq!(demand.deadline, Some(deadline));
        assert!(!demand.ordinary_present_eligible);
        assert_ne!(demand, FrameDemand::empty());
    }
}
