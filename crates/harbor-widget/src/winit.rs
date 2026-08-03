//! Optional winit integration contracts.
//!
//! This module owns the per-window state needed to adapt winit events into the
//! platform-independent widget runtime. Host policy (close requests, window
//! routing, and scheduling) remains outside this feature-gated module.

use crate::effects::RuntimeEffects;
use crate::input::event::{
    FocusEvent, Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, PointerEvent,
    PointerPhase, UiEvent,
};
use crate::layout::Point;
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use std::time::Instant;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};
use winit::window::Window;

/// The host-visible result of offering one winit event to a window adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct WinitEventOutcome {
    /// Whether the event was a supported state update or was dispatched.
    pub handled: bool,
    /// Effects produced by dispatching the adapted event, if any.
    pub effects: RuntimeEffects,
}

impl WinitEventOutcome {
    pub fn handled(effects: RuntimeEffects) -> Self {
        Self {
            handled: true,
            effects,
        }
    }

    pub fn unhandled() -> Self {
        Self {
            handled: false,
            effects: RuntimeEffects::default(),
        }
    }

    pub const fn is_handled(&self) -> bool {
        self.handled
    }
}

/// Per-window state for the winit integration.
const TOUCH_POINTER_ID_BASE: u64 = 1 << 63;

struct TouchContact {
    device_id: winit::event::DeviceId,
    source_id: u64,
    pointer_id: u64,
    position: Point,
}

/// Per-window state for the winit integration.
pub struct WinitAdapter {
    modifiers: ModifiersState,
    /// The latest mouse cursor position in physical pixels.
    mouse_position: Point,
    scale_factor: f32,
    ime_enabled: bool,
    ime_preedit: Option<String>,
    /// Active touch contacts and their namespaced public pointer ids.
    touch_contacts: Vec<TouchContact>,
    /// Suppresses mouse releases that belong to captures canceled on focus loss,
    /// tracked independently for each supported mouse button.
    mouse_release_quarantine: [bool; 3],
    /// Supported mouse buttons currently held by the native window.
    active_mouse_buttons: [bool; 3],
    /// Suppresses touch releases for contacts that were active on focus loss.
    quarantined_touches: Vec<(winit::event::DeviceId, u64)>,
    next_touch_pointer_id: u64,
}

impl Default for WinitAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WinitAdapter {
    pub const fn new() -> Self {
        Self {
            modifiers: ModifiersState::empty(),
            mouse_position: Point::ZERO,
            scale_factor: 1.0,
            ime_enabled: false,
            ime_preedit: None,
            touch_contacts: Vec::new(),
            mouse_release_quarantine: [false; 3],
            active_mouse_buttons: [false; 3],
            quarantined_touches: Vec::new(),
            next_touch_pointer_id: 1,
        }
    }

    /// Sets the current window scale factor used for physical pointer input.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        if scale_factor.is_finite() && scale_factor > 0.0 {
            self.scale_factor = scale_factor;
        }
    }

    /// Returns the latest modifier state received for this window.
    pub const fn modifiers(&self) -> ModifiersState {
        self.modifiers
    }

    /// Handles one window event and returns whether it was supported plus any
    /// effects produced by the widget runtime.
    pub fn handle_event(
        &mut self,
        runtime: &mut Runtime,
        event: &WindowEvent,
    ) -> WinitEventOutcome {
        if self.quarantine_pointer_event(event) {
            return WinitEventOutcome::handled(RuntimeEffects::default());
        }
        if let WindowEvent::KeyboardInput { event, .. } = event {
            return self.handle_keyboard_input(
                runtime,
                &event.logical_key,
                event.state,
                event.location,
            );
        }

        let state_only = matches!(
            event,
            WindowEvent::ModifiersChanged(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::Ime(_)
        );
        let Some(ui_event) = self.convert_event(event) else {
            return if state_only {
                WinitEventOutcome::handled(RuntimeEffects::default())
            } else {
                WinitEventOutcome::unhandled()
            };
        };

        let mut effects = runtime.dispatch(ui_event, Instant::now());
        if matches!(event, WindowEvent::Focused(false)) {
            effects.merge(runtime.cancel_pointer_captures(self.logical_pointer_position()));
            self.modifiers = ModifiersState::empty();
            // Focus loss ends the current composition and clears IME
            // enablement so a later focus session cannot inherit suppression.
            self.ime_enabled = false;
            self.ime_preedit = None;
            self.touch_contacts.clear();
        }
        WinitEventOutcome::handled(effects)
    }

    fn quarantine_pointer_event(&mut self, event: &WindowEvent) -> bool {
        if matches!(event, WindowEvent::Focused(false)) {
            for index in 0..self.active_mouse_buttons.len() {
                self.mouse_release_quarantine[index] |= self.active_mouse_buttons[index];
                self.active_mouse_buttons[index] = false;
            }
            let active_touches: Vec<_> = self
                .touch_contacts
                .iter()
                .map(|contact| (contact.device_id, contact.source_id))
                .collect();
            for identity in active_touches {
                if !self.quarantined_touches.contains(&identity) {
                    self.quarantined_touches.push(identity);
                }
            }
            return false;
        }

        match event {
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button,
                ..
            } => {
                let Some(index) = mouse_button_index(*button) else {
                    return false;
                };
                if self.mouse_release_quarantine[index] {
                    self.mouse_release_quarantine[index] = false;
                    return true;
                }
                self.active_mouse_buttons[index] = false;
                false
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                let Some(index) = mouse_button_index(*button) else {
                    return false;
                };
                self.mouse_release_quarantine[index] = false;
                self.active_mouse_buttons[index] = true;
                false
            }
            WindowEvent::Touch(winit::event::Touch {
                phase: TouchPhase::Ended | TouchPhase::Cancelled,
                device_id,
                id,
                ..
            }) => self
                .quarantined_touches
                .iter()
                .any(|identity| identity == &(*device_id, *id)),
            WindowEvent::Touch(winit::event::Touch {
                phase: TouchPhase::Started,
                device_id,
                id,
                ..
            }) => {
                if let Some(index) = self
                    .quarantined_touches
                    .iter()
                    .position(|identity| identity == &(*device_id, *id))
                {
                    self.quarantined_touches.remove(index);
                }
                false
            }
            _ => false,
        }
    }

    /// Handles a key event using the adapter's current modifier and IME state.
    ///
    /// This is also useful to hosts that already normalized a key while
    /// preserving the same handled/no-effect outcome as `handle_event`.
    pub fn handle_keyboard_input(
        &mut self,
        runtime: &mut Runtime,
        logical_key: &Key,
        state: ElementState,
        location: KeyLocation,
    ) -> WinitEventOutcome {
        let suppressed = ime_suppresses_keyboard(
            logical_key,
            state,
            location,
            self.modifiers,
            self.ime_enabled,
        );
        let Some(ui_event) = keyboard_to_uievent(
            logical_key,
            state,
            location,
            self.modifiers,
            self.ime_enabled,
        ) else {
            return if suppressed {
                WinitEventOutcome::handled(RuntimeEffects::default())
            } else {
                WinitEventOutcome::unhandled()
            };
        };
        WinitEventOutcome::handled(runtime.dispatch(ui_event, Instant::now()))
    }

    fn convert_event(&mut self, event: &WindowEvent) -> Option<UiEvent> {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                None
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.set_scale_factor(*scale_factor as f32);
                None
            }
            WindowEvent::Ime(ime) => self.convert_ime(ime),
            WindowEvent::KeyboardInput { event, .. } => self.convert_keyboard(event),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_position = Point::new(position.x as f32, position.y as f32);
                Some(UiEvent::Pointer(PointerEvent::new(
                    self.logical_pointer_position(),
                    PointerPhase::Move,
                    PointerButton::Left,
                    0,
                )))
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = mouse_button(*button)?;
                let phase = match state {
                    ElementState::Pressed => PointerPhase::Down,
                    ElementState::Released => PointerPhase::Up,
                };
                Some(UiEvent::Pointer(PointerEvent::new(
                    self.logical_pointer_position(),
                    phase,
                    button,
                    0,
                )))
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let phase = match delta {
                    MouseScrollDelta::LineDelta(x, y) => PointerPhase::WheelLine { dx: *x, dy: *y },
                    MouseScrollDelta::PixelDelta(position) => PointerPhase::WheelPixel {
                        dx: position.x as f32,
                        dy: position.y as f32,
                    },
                };
                Some(UiEvent::Pointer(PointerEvent::new(
                    self.logical_pointer_position(),
                    phase,
                    PointerButton::Left,
                    0,
                )))
            }
            WindowEvent::Touch(touch) => {
                let phase = match touch.phase {
                    TouchPhase::Started => PointerPhase::Down,
                    TouchPhase::Moved => PointerPhase::Move,
                    TouchPhase::Ended => PointerPhase::Up,
                    TouchPhase::Cancelled => PointerPhase::Cancel,
                };
                let pointer_id = self.touch_pointer_id(touch.device_id, touch.id);
                let position = self.update_touch_position(
                    touch.device_id,
                    touch.id,
                    Point::new(touch.location.x as f32, touch.location.y as f32),
                );
                if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    self.touch_contacts.retain(|contact| {
                        !(contact.device_id == touch.device_id && contact.source_id == touch.id)
                    });
                }
                Some(UiEvent::Pointer(PointerEvent::new(
                    position,
                    phase,
                    PointerButton::Left,
                    pointer_id,
                )))
            }
            WindowEvent::Focused(focused) => Some(UiEvent::Focus(if *focused {
                FocusEvent::Gained
            } else {
                FocusEvent::Lost
            })),
            _ => None,
        }
    }

    fn convert_ime(&mut self, event: &Ime) -> Option<UiEvent> {
        match event {
            Ime::Enabled => {
                self.ime_enabled = true;
                self.ime_preedit = None;
                None
            }
            Ime::Disabled => {
                self.ime_enabled = false;
                self.ime_preedit = None;
                None
            }
            Ime::Preedit(text, _) => {
                self.ime_preedit = Some(text.clone());
                None
            }
            Ime::Commit(text) => {
                self.ime_preedit = None;
                (!text.is_empty()).then(|| UiEvent::Keyboard(KeyboardEvent::Ime(text.clone())))
            }
        }
    }

    fn convert_keyboard(&self, event: &winit::event::KeyEvent) -> Option<UiEvent> {
        keyboard_to_uievent(
            &event.logical_key,
            event.state,
            event.location,
            self.modifiers,
            self.ime_enabled,
        )
    }

    fn touch_pointer_id(&mut self, device_id: winit::event::DeviceId, source_id: u64) -> u64 {
        if let Some(contact) = self
            .touch_contacts
            .iter()
            .find(|contact| contact.device_id == device_id && contact.source_id == source_id)
        {
            return contact.pointer_id;
        }

        let pointer_id = TOUCH_POINTER_ID_BASE | self.next_touch_pointer_id;
        self.next_touch_pointer_id = self.next_touch_pointer_id.wrapping_add(1);
        if self.next_touch_pointer_id == 0 {
            self.next_touch_pointer_id = 1;
        }
        self.touch_contacts.push(TouchContact {
            device_id,
            source_id,
            pointer_id,
            position: Point::ZERO,
        });
        pointer_id
    }

    fn update_touch_position(
        &mut self,
        device_id: winit::event::DeviceId,
        source_id: u64,
        position: Point,
    ) -> Point {
        let contact = self
            .touch_contacts
            .iter_mut()
            .find(|contact| contact.device_id == device_id && contact.source_id == source_id)
            .expect("touch contact allocated before position lookup");
        contact.position = position;
        Point::new(
            position.x / self.scale_factor,
            position.y / self.scale_factor,
        )
    }

    fn logical_pointer_position(&self) -> Point {
        Point::new(
            self.mouse_position.x / self.scale_factor,
            self.mouse_position.y / self.scale_factor,
        )
    }
}

fn ime_suppresses_keyboard(
    logical_key: &Key,
    state: ElementState,
    location: KeyLocation,
    modifiers: ModifiersState,
    ime_enabled: bool,
) -> bool {
    state == ElementState::Pressed
        && ime_enabled
        && location != KeyLocation::Numpad
        && !modifiers.control_key()
        && !modifiers.alt_key()
        && !modifiers.super_key()
        && matches!(logical_key, Key::Character(_))
}

fn mouse_button_index(button: MouseButton) -> Option<usize> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Right => Some(1),
        MouseButton::Middle => Some(2),
        _ => None,
    }
}

fn mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Left),
        MouseButton::Right => Some(PointerButton::Right),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

/// Maps a winit named key to the platform-independent widget key.
fn named_to_widget_key(named: &NamedKey) -> Option<WidgetKey> {
    match named {
        NamedKey::Tab => Some(WidgetKey::Tab),
        NamedKey::Enter => Some(WidgetKey::Enter),
        NamedKey::Space => Some(WidgetKey::Space),
        NamedKey::Escape => Some(WidgetKey::Escape),
        NamedKey::Backspace => Some(WidgetKey::Backspace),
        NamedKey::Insert => Some(WidgetKey::Insert),
        NamedKey::Delete => Some(WidgetKey::Delete),
        NamedKey::F1 => Some(WidgetKey::F1),
        NamedKey::F2 => Some(WidgetKey::F2),
        NamedKey::F3 => Some(WidgetKey::F3),
        NamedKey::F4 => Some(WidgetKey::F4),
        NamedKey::F5 => Some(WidgetKey::F5),
        NamedKey::F6 => Some(WidgetKey::F6),
        NamedKey::F7 => Some(WidgetKey::F7),
        NamedKey::F8 => Some(WidgetKey::F8),
        NamedKey::F9 => Some(WidgetKey::F9),
        NamedKey::F10 => Some(WidgetKey::F10),
        NamedKey::F11 => Some(WidgetKey::F11),
        NamedKey::F12 => Some(WidgetKey::F12),
        NamedKey::ArrowUp => Some(WidgetKey::ArrowUp),
        NamedKey::ArrowDown => Some(WidgetKey::ArrowDown),
        NamedKey::ArrowLeft => Some(WidgetKey::ArrowLeft),
        NamedKey::ArrowRight => Some(WidgetKey::ArrowRight),
        NamedKey::Home => Some(WidgetKey::Home),
        NamedKey::End => Some(WidgetKey::End),
        NamedKey::PageUp => Some(WidgetKey::PageUp),
        NamedKey::PageDown => Some(WidgetKey::PageDown),
        _ => None,
    }
}

fn modifiers_to_widget(modifiers: ModifiersState) -> Modifiers {
    Modifiers {
        shift: modifiers.shift_key(),
        ctrl: modifiers.control_key(),
        alt: modifiers.alt_key(),
        meta: modifiers.super_key(),
    }
}

fn keyboard_to_uievent(
    logical_key: &Key,
    state: ElementState,
    location: KeyLocation,
    modifiers: ModifiersState,
    ime_enabled: bool,
) -> Option<UiEvent> {
    let is_numpad = location == KeyLocation::Numpad;
    if state == ElementState::Pressed
        && ime_enabled
        && !is_numpad
        && !modifiers.control_key()
        && !modifiers.alt_key()
        && !modifiers.super_key()
        && matches!(logical_key, Key::Character(_))
    {
        return None;
    }

    let key = match logical_key {
        Key::Named(NamedKey::Enter) if is_numpad => WidgetKey::NumpadEnter,
        Key::Named(named) => named_to_widget_key(named)?,
        Key::Character(text) if is_numpad => WidgetKey::NumpadCharacter(text.chars().next()?),
        Key::Character(text) => WidgetKey::Character(text.chars().next()?),
        _ => return None,
    };
    let modifiers = modifiers_to_widget(modifiers);
    let event = match state {
        ElementState::Pressed => KeyboardEvent::KeyDown { key, modifiers },
        ElementState::Released => KeyboardEvent::KeyUp { key, modifiers },
    };
    Some(UiEvent::Keyboard(event))
}

impl WinitAdapter {
    /// Attempts one integration frame.
    ///
    /// Frame acquisition, encoding, submission, and presentation are deferred
    /// to the presentation slice. A deferred result makes that absence of
    /// behavior explicit rather than pretending a frame was presented.
    pub fn render<'frame, 'surface>(
        &mut self,
        _runtime: &mut Runtime,
        _target: WinitFrameTarget<'frame, 'surface>,
    ) -> FrameOutcome {
        FrameOutcome::Deferred
    }
}

/// Borrowed host resources valid for one frame.
///
/// The separate `surface` lifetime describes the lifetime carried by the wgpu
/// surface itself, while `frame` describes the borrow of that surface and the
/// other host resources. This value is consumed by a frame call and cannot be
/// retained by either the runtime or adapter.
pub struct WinitFrameTarget<'frame, 'surface> {
    window: &'frame Window,
    surface: &'frame wgpu::Surface<'surface>,
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    config: &'frame mut wgpu::SurfaceConfiguration,
    viewport: Viewport,
    clear_color: wgpu::Color,
}

impl<'frame, 'surface> WinitFrameTarget<'frame, 'surface> {
    pub fn new(
        window: &'frame Window,
        surface: &'frame wgpu::Surface<'surface>,
        device: &'frame wgpu::Device,
        queue: &'frame wgpu::Queue,
        config: &'frame mut wgpu::SurfaceConfiguration,
        viewport: Viewport,
        clear_color: wgpu::Color,
    ) -> Self {
        Self {
            window,
            surface,
            device,
            queue,
            config,
            viewport,
            clear_color,
        }
    }

    pub fn window(&self) -> &'frame Window {
        self.window
    }

    pub fn surface(&self) -> &'frame wgpu::Surface<'surface> {
        self.surface
    }

    pub fn device(&self) -> &'frame wgpu::Device {
        self.device
    }

    pub fn queue(&self) -> &'frame wgpu::Queue {
        self.queue
    }

    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        self.config
    }

    pub fn config_mut(&mut self) -> &mut wgpu::SurfaceConfiguration {
        self.config
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn clear_color(&self) -> wgpu::Color {
        self.clear_color
    }
}

/// The host-visible result of one attempted frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameOutcome {
    Presented,
    Skipped,
    Deferred,
    RecoveryRequired,
    Fatal(FrameError),
}

impl FrameOutcome {
    pub const fn presented() -> Self {
        Self::Presented
    }

    pub const fn skipped() -> Self {
        Self::Skipped
    }

    pub const fn deferred() -> Self {
        Self::Deferred
    }

    pub const fn recovery_required() -> Self {
        Self::RecoveryRequired
    }

    pub const fn fatal(error: FrameError) -> Self {
        Self::Fatal(error)
    }

    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    pub const fn is_presented(&self) -> bool {
        matches!(self, Self::Presented)
    }

    pub const fn is_recovery_required(&self) -> bool {
        matches!(self, Self::RecoveryRequired)
    }
}

/// A fatal presentation failure for the host to diagnose and handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    OutOfMemory,
    DeviceLost,
    Validation(String),
    Presentation(String),
    Other(String),
}

impl FrameError {
    pub const fn out_of_memory() -> Self {
        Self::OutOfMemory
    }

    pub const fn device_lost() -> Self {
        Self::DeviceLost
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn presentation(message: impl Into<String>) -> Self {
        Self::Presentation(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{KeyboardEvent, PointerPhase};
    use crate::widgets::button::Button;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use winit::dpi::PhysicalPosition;

    #[test]
    fn outcomes_distinguish_supported_noop_and_unsupported_events() {
        let handled = WinitEventOutcome::handled(RuntimeEffects::default());
        assert!(handled.handled);
        assert!(handled.is_handled());
        assert!(handled.effects.is_noop());

        let unhandled = WinitEventOutcome::unhandled();
        assert!(!unhandled.handled);
        assert!(!unhandled.is_handled());
        assert!(unhandled.effects.is_noop());
    }

    #[test]
    fn state_only_events_are_handled_without_dispatch() {
        let mut adapter = WinitAdapter::new();
        let mut runtime = Runtime::new();
        let outcome = adapter.handle_event(
            &mut runtime,
            &WindowEvent::Ime(Ime::Preedit("draft".into(), Some((0, 5)))),
        );
        assert!(outcome.handled);
        assert!(outcome.effects.is_noop());
        assert!(adapter.ime_preedit.is_some());
    }

    #[test]
    fn ime_commit_is_the_only_composition_text_event() {
        let mut adapter = WinitAdapter::new();
        let mut runtime = Runtime::new();
        adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
        assert!(
            keyboard_to_uievent(
                &Key::Character("a".into()),
                ElementState::Pressed,
                KeyLocation::Standard,
                ModifiersState::empty(),
                true,
            )
            .is_none()
        );

        let outcome =
            adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Commit("語".into())));
        assert!(outcome.handled);
        assert!(adapter.ime_preedit.is_none());
    }

    #[test]
    fn ime_keeps_shortcuts_navigation_and_numpad_keys_active() {
        let modifiers = ModifiersState::empty();
        assert!(
            keyboard_to_uievent(
                &Key::Named(NamedKey::ArrowUp),
                ElementState::Pressed,
                KeyLocation::Standard,
                modifiers,
                true,
            )
            .is_some()
        );
        assert!(
            keyboard_to_uievent(
                &Key::Character("1".into()),
                ElementState::Pressed,
                KeyLocation::Numpad,
                modifiers,
                true,
            )
            .is_some()
        );
        assert!(
            keyboard_to_uievent(
                &Key::Character("c".into()),
                ElementState::Pressed,
                KeyLocation::Standard,
                ModifiersState::CONTROL,
                true,
            )
            .is_some()
        );
    }

    #[test]
    fn keyboard_conversion_preserves_press_release_named_and_numpad_keys() {
        let down = keyboard_to_uievent(
            &Key::Named(NamedKey::F5),
            ElementState::Pressed,
            KeyLocation::Standard,
            ModifiersState::SHIFT,
            false,
        );
        assert_eq!(
            down,
            Some(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: WidgetKey::F5,
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            }))
        );

        let up = keyboard_to_uievent(
            &Key::Named(NamedKey::Enter),
            ElementState::Released,
            KeyLocation::Numpad,
            ModifiersState::empty(),
            false,
        );
        assert_eq!(
            up,
            Some(UiEvent::Keyboard(KeyboardEvent::KeyUp {
                key: WidgetKey::NumpadEnter,
                modifiers: Modifiers::default(),
            }))
        );
    }

    #[test]
    fn supported_named_keys_and_modifier_bits_are_preserved() {
        let supported = [
            (NamedKey::Tab, WidgetKey::Tab),
            (NamedKey::Enter, WidgetKey::Enter),
            (NamedKey::Space, WidgetKey::Space),
            (NamedKey::Escape, WidgetKey::Escape),
            (NamedKey::Backspace, WidgetKey::Backspace),
            (NamedKey::Insert, WidgetKey::Insert),
            (NamedKey::Delete, WidgetKey::Delete),
            (NamedKey::F1, WidgetKey::F1),
            (NamedKey::F2, WidgetKey::F2),
            (NamedKey::F3, WidgetKey::F3),
            (NamedKey::F4, WidgetKey::F4),
            (NamedKey::F5, WidgetKey::F5),
            (NamedKey::F6, WidgetKey::F6),
            (NamedKey::F7, WidgetKey::F7),
            (NamedKey::F8, WidgetKey::F8),
            (NamedKey::F9, WidgetKey::F9),
            (NamedKey::F10, WidgetKey::F10),
            (NamedKey::F11, WidgetKey::F11),
            (NamedKey::F12, WidgetKey::F12),
            (NamedKey::ArrowUp, WidgetKey::ArrowUp),
            (NamedKey::ArrowDown, WidgetKey::ArrowDown),
            (NamedKey::ArrowLeft, WidgetKey::ArrowLeft),
            (NamedKey::ArrowRight, WidgetKey::ArrowRight),
            (NamedKey::Home, WidgetKey::Home),
            (NamedKey::End, WidgetKey::End),
            (NamedKey::PageUp, WidgetKey::PageUp),
            (NamedKey::PageDown, WidgetKey::PageDown),
        ];
        let modifier_bits = [
            ModifiersState::empty(),
            ModifiersState::SHIFT,
            ModifiersState::CONTROL,
            ModifiersState::ALT,
            ModifiersState::SUPER,
            ModifiersState::SHIFT
                | ModifiersState::CONTROL
                | ModifiersState::ALT
                | ModifiersState::SUPER,
        ];
        for (named, expected_key) in supported {
            for modifiers in modifier_bits {
                let event = keyboard_to_uievent(
                    &Key::Named(named),
                    ElementState::Pressed,
                    KeyLocation::Standard,
                    modifiers,
                    false,
                );
                assert_eq!(
                    event,
                    Some(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                        key: expected_key,
                        modifiers: modifiers_to_widget(modifiers),
                    }))
                );
            }
        }
    }

    #[test]
    fn character_and_numpad_matrix_preserves_modifier_combinations() {
        let modifier_bits = [
            ModifiersState::empty(),
            ModifiersState::SHIFT,
            ModifiersState::CONTROL,
            ModifiersState::ALT,
            ModifiersState::SUPER,
            ModifiersState::SHIFT
                | ModifiersState::CONTROL
                | ModifiersState::ALT
                | ModifiersState::SUPER,
        ];
        for modifiers in modifier_bits {
            for (text, expected) in [
                ("a", WidgetKey::Character('a')),
                ("é", WidgetKey::Character('é')),
            ] {
                assert_eq!(
                    keyboard_to_uievent(
                        &Key::Character(text.into()),
                        ElementState::Pressed,
                        KeyLocation::Standard,
                        modifiers,
                        false,
                    ),
                    Some(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                        key: expected,
                        modifiers: modifiers_to_widget(modifiers),
                    }))
                );
                assert_eq!(
                    keyboard_to_uievent(
                        &Key::Character(text.into()),
                        ElementState::Pressed,
                        KeyLocation::Numpad,
                        modifiers,
                        false,
                    ),
                    Some(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                        key: match expected {
                            WidgetKey::Character(ch) => WidgetKey::NumpadCharacter(ch),
                            _ => unreachable!(),
                        },
                        modifiers: modifiers_to_widget(modifiers),
                    }))
                );
            }
            assert_eq!(
                keyboard_to_uievent(
                    &Key::Named(NamedKey::Enter),
                    ElementState::Released,
                    KeyLocation::Numpad,
                    modifiers,
                    false,
                ),
                Some(UiEvent::Keyboard(KeyboardEvent::KeyUp {
                    key: WidgetKey::NumpadEnter,
                    modifiers: modifiers_to_widget(modifiers),
                }))
            );
        }
    }

    #[test]
    fn mouse_button_mapping_covers_supported_and_unsupported_variants() {
        assert_eq!(mouse_button(MouseButton::Left), Some(PointerButton::Left));
        assert_eq!(mouse_button(MouseButton::Right), Some(PointerButton::Right));
        assert_eq!(
            mouse_button(MouseButton::Middle),
            Some(PointerButton::Middle)
        );
        assert_eq!(mouse_button(MouseButton::Other(8)), None);
    }

    #[test]
    fn pointer_positions_are_physical_until_dispatch_and_scale_changes_apply() {
        let mut adapter = WinitAdapter::new();
        adapter.set_scale_factor(2.0);
        let event = WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: PhysicalPosition::new(80.0, 40.0),
        };
        let converted = adapter.convert_event(&event).unwrap();
        assert_eq!(
            converted,
            UiEvent::Pointer(PointerEvent::new(
                Point::new(40.0, 20.0),
                PointerPhase::Move,
                PointerButton::Left,
                0,
            ))
        );

        let click = WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        };
        assert_eq!(
            adapter.convert_event(&click).unwrap(),
            UiEvent::Pointer(PointerEvent::new(
                Point::new(40.0, 20.0),
                PointerPhase::Down,
                PointerButton::Left,
                0,
            ))
        );
    }

    #[test]
    fn wheel_conversion_preserves_line_and_pixel_payloads() {
        let mut adapter = WinitAdapter::new();
        adapter.mouse_position = Point::new(12.0, 18.0);

        let line = adapter
            .convert_event(&WindowEvent::MouseWheel {
                device_id: winit::event::DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(1.5, -2.0),
                phase: winit::event::TouchPhase::Moved,
            })
            .unwrap();
        assert_eq!(
            line,
            UiEvent::Pointer(PointerEvent::new(
                Point::new(12.0, 18.0),
                PointerPhase::WheelLine { dx: 1.5, dy: -2.0 },
                PointerButton::Left,
                0,
            ))
        );

        let pixel = adapter
            .convert_event(&WindowEvent::MouseWheel {
                device_id: winit::event::DeviceId::dummy(),
                delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(3.0, 4.0)),
                phase: winit::event::TouchPhase::Moved,
            })
            .unwrap();
        assert_eq!(
            pixel,
            UiEvent::Pointer(PointerEvent::new(
                Point::new(12.0, 18.0),
                PointerPhase::WheelPixel { dx: 3.0, dy: 4.0 },
                PointerButton::Left,
                0,
            ))
        );
    }

    #[test]
    fn touch_conversion_preserves_all_phases_ids_and_scale() {
        let mut adapter = WinitAdapter::new();
        adapter.set_scale_factor(2.0);
        for (source, expected) in [
            (TouchPhase::Started, PointerPhase::Down),
            (TouchPhase::Moved, PointerPhase::Move),
            (TouchPhase::Ended, PointerPhase::Up),
        ] {
            let event = WindowEvent::Touch(winit::event::Touch {
                device_id: winit::event::DeviceId::dummy(),
                phase: source,
                location: PhysicalPosition::new(20.0, 10.0),
                force: None,
                id: 9,
            });
            assert_eq!(
                adapter.convert_event(&event),
                Some(UiEvent::Pointer(PointerEvent::new(
                    Point::new(10.0, 5.0),
                    expected,
                    PointerButton::Left,
                    TOUCH_POINTER_ID_BASE | 1,
                )))
            );
        }
        // A terminal phase releases the contact identity; a later malformed
        // event for the same source is treated as a new namespaced contact.
        let cancelled = WindowEvent::Touch(winit::event::Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase: TouchPhase::Cancelled,
            location: PhysicalPosition::new(20.0, 10.0),
            force: None,
            id: 9,
        });
        assert_eq!(
            adapter.convert_event(&cancelled),
            Some(UiEvent::Pointer(PointerEvent::new(
                Point::new(10.0, 5.0),
                PointerPhase::Cancel,
                PointerButton::Left,
                TOUCH_POINTER_ID_BASE | 2,
            )))
        );
    }

    #[test]
    fn touch_contacts_are_namespaced_and_do_not_contaminate_mouse_position() {
        let mut adapter = WinitAdapter::new();
        adapter.set_scale_factor(2.0);

        let mouse_move = WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: PhysicalPosition::new(80.0, 40.0),
        };
        let touch_zero = WindowEvent::Touch(winit::event::Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase: TouchPhase::Started,
            location: PhysicalPosition::new(20.0, 10.0),
            force: None,
            id: 0,
        });
        let touch_one = WindowEvent::Touch(winit::event::Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase: TouchPhase::Started,
            location: PhysicalPosition::new(40.0, 20.0),
            force: None,
            id: 1,
        });
        let mouse_down = WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        };

        let mouse = adapter.convert_event(&mouse_move).unwrap();
        let first_touch = adapter.convert_event(&touch_zero).unwrap();
        let second_touch = adapter.convert_event(&touch_one).unwrap();
        let mouse_after_touch = adapter.convert_event(&mouse_down).unwrap();

        let mouse_id = mouse.pointer_id().unwrap();
        let first_id = first_touch.pointer_id().unwrap();
        let second_id = second_touch.pointer_id().unwrap();
        assert_eq!(mouse_id, 0);
        assert_eq!(first_id, TOUCH_POINTER_ID_BASE | 1);
        assert_eq!(second_id, TOUCH_POINTER_ID_BASE | 2);
        assert_ne!(first_id, second_id);
        assert_ne!(mouse_id, first_id);
        assert_eq!(
            mouse_after_touch,
            UiEvent::Pointer(PointerEvent::new(
                Point::new(40.0, 20.0),
                PointerPhase::Down,
                PointerButton::Left,
                0,
            ))
        );

        // A later move for one touch contact changes only that contact's
        // position; the physical mouse position remains the click position.
        let touch_move = WindowEvent::Touch(winit::event::Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase: TouchPhase::Moved,
            location: PhysicalPosition::new(60.0, 30.0),
            force: None,
            id: 0,
        });
        adapter.convert_event(&touch_move);
        assert_eq!(
            adapter.convert_event(&mouse_down).unwrap().pointer_id(),
            Some(0)
        );
        assert_eq!(
            adapter.convert_event(&mouse_down).unwrap(),
            UiEvent::Pointer(PointerEvent::new(
                Point::new(40.0, 20.0),
                PointerPhase::Down,
                PointerButton::Left,
                0,
            ))
        );
    }

    #[test]
    fn scale_changes_after_cursor_move_are_applied_to_later_mouse_input() {
        let mut adapter = WinitAdapter::new();
        let cursor = WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: PhysicalPosition::new(80.0, 40.0),
        };
        adapter.convert_event(&cursor);
        adapter.set_scale_factor(2.0);

        let wheel = WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(1.0, 2.0),
            phase: TouchPhase::Moved,
        };
        assert_eq!(
            adapter.convert_event(&wheel).unwrap(),
            UiEvent::Pointer(PointerEvent::new(
                Point::new(40.0, 20.0),
                PointerPhase::WheelLine { dx: 1.0, dy: 2.0 },
                PointerButton::Left,
                0,
            ))
        );
    }

    #[test]
    fn adapter_dispatches_focus_events_and_forwards_redraw_effects() {
        let mut runtime = Runtime::new();
        runtime.set_root(crate::widgets::button::Button::new("OK"));
        runtime.update(Instant::now());
        assert!(runtime.focus_first_focusable());

        let mut adapter = WinitAdapter::new();
        let outcome = adapter.handle_event(&mut runtime, &WindowEvent::Focused(true));
        assert!(outcome.handled);
        assert!(outcome.effects.request_redraw);
    }

    #[test]
    fn unsupported_events_are_unhandled() {
        let mut adapter = WinitAdapter::new();
        let mut runtime = Runtime::new();
        let outcome = adapter.handle_event(&mut runtime, &WindowEvent::RedrawRequested);
        assert!(!outcome.handled);
        assert!(outcome.effects.is_noop());
    }

    #[test]
    fn focus_loss_clears_window_transient_state() {
        let mut adapter = WinitAdapter::new();
        adapter.modifiers = ModifiersState::SHIFT;
        adapter.ime_enabled = true;
        adapter.ime_preedit = Some("draft".into());
        let mut runtime = Runtime::new();
        let outcome = adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
        assert!(outcome.handled);
        assert_eq!(adapter.modifiers, ModifiersState::empty());
        assert!(!adapter.ime_enabled);
        assert!(adapter.ime_preedit.is_none());

        adapter.handle_event(&mut runtime, &WindowEvent::Focused(true));
        assert!(
            keyboard_to_uievent(
                &Key::Character("x".into()),
                ElementState::Pressed,
                KeyLocation::Standard,
                adapter.modifiers,
                adapter.ime_enabled,
            )
            .is_some()
        );
    }

    #[test]
    fn adapters_keep_pointer_and_ime_state_isolated() {
        let mut first = WinitAdapter::new();
        let second = WinitAdapter::new();
        first.set_scale_factor(2.0);
        first.mouse_position = Point::new(10.0, 20.0);
        first.ime_enabled = true;
        assert_eq!(second.scale_factor, 1.0);
        assert_eq!(second.mouse_position, Point::ZERO);
        assert!(!second.ime_enabled);
    }

    #[test]
    fn adapter_dispatches_pointer_input_directly_to_button() {
        let clicked = Arc::new(AtomicBool::new(false));
        let clicked_clone = Arc::clone(&clicked);
        let mut runtime = Runtime::new();
        runtime.set_root(Button::new("OK").on_click(move |_| {
            clicked_clone.store(true, Ordering::SeqCst);
        }));
        runtime.update(Instant::now());
        let mut adapter = WinitAdapter::new();

        adapter.handle_event(
            &mut runtime,
            &WindowEvent::CursorMoved {
                device_id: winit::event::DeviceId::dummy(),
                position: PhysicalPosition::new(4.0, 4.0),
            },
        );
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::MouseInput {
                device_id: winit::event::DeviceId::dummy(),
                state: ElementState::Pressed,
                button: MouseButton::Left,
            },
        );
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::MouseInput {
                device_id: winit::event::DeviceId::dummy(),
                state: ElementState::Released,
                button: MouseButton::Left,
            },
        );

        assert!(clicked.load(Ordering::SeqCst));
    }

    #[test]
    fn focus_loss_cancels_button_capture_before_later_mouse_up() {
        let clicked = Arc::new(AtomicBool::new(false));
        let clicked_clone = Arc::clone(&clicked);
        let mut runtime = Runtime::new();
        runtime.set_root(Button::new("OK").on_click(move |_| {
            clicked_clone.store(true, Ordering::SeqCst);
        }));
        runtime.update(Instant::now());
        let mut adapter = WinitAdapter::new();

        adapter.handle_event(
            &mut runtime,
            &WindowEvent::CursorMoved {
                device_id: winit::event::DeviceId::dummy(),
                position: PhysicalPosition::new(4.0, 4.0),
            },
        );
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::MouseInput {
                device_id: winit::event::DeviceId::dummy(),
                state: ElementState::Pressed,
                button: MouseButton::Left,
            },
        );
        let focus_loss = adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
        assert!(focus_loss.effects.request_redraw);
        assert!(runtime.input().captor(0).is_none());

        adapter.handle_event(
            &mut runtime,
            &WindowEvent::CursorMoved {
                device_id: winit::event::DeviceId::dummy(),
                position: PhysicalPosition::new(500.0, 500.0),
            },
        );
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::MouseInput {
                device_id: winit::event::DeviceId::dummy(),
                state: ElementState::Released,
                button: MouseButton::Left,
            },
        );
        assert!(!clicked.load(Ordering::SeqCst));
    }
}
