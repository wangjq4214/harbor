//! Optional winit integration contracts.
//!
//! This module owns the per-window state needed to adapt winit events into the
//! platform-independent widget runtime, including per-window frame scheduling.
//! Host policy (close requests and window routing) remains outside this module.

mod effects;
mod event;
mod gpu;
mod gpu_fault;
#[cfg(feature = "hmr")]
mod hmr;
mod host;
mod presenter;
mod scheduler;
mod surface;

use crate::effects::RuntimeEffects;
use crate::input::event::{
    FocusEvent, KeyboardEvent, PointerBoundaryEvent, PointerBoundaryKind, PointerButton,
    PointerEvent, PointerPhase, UiEvent,
};
use crate::layout::Point;
use crate::runtime::Runtime;
use crate::winit::event::{
    ime_suppresses_keyboard, keyboard_to_uievent, modifiers_to_widget, mouse_button,
};
#[cfg(test)]
use crate::winit::surface::SurfaceState;
#[cfg(test)]
use std::time::Instant;
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::keyboard::{Key, KeyLocation, ModifiersState};
use winit::window::Window;

#[cfg(target_os = "windows")]
pub use gpu::RENDER_TARGET_IS_TOPMOST;
pub(crate) use gpu::WindowSurface;
pub use gpu::{SharedGpu, select_compositing_alpha_mode};
pub use gpu_fault::{GpuFault, GpuFaultKind, GpuWake, GpuWakeEvent};
#[cfg(feature = "hmr")]
pub use hmr::{WidgetHmrConfig, WidgetHmrWork};
pub use host::{
    HostEventOutcome, HostFrameOutcome, HostGpuSource, HostIdleOutcome, HostInitContext,
    HostStartupError, HostStartupStage, NoopWindowPlatformHooks, WindowPlatformHooks,
    WindowSurfaceInfo, WinitWindowHost, WinitWindowHostBuilder,
};
pub(crate) use presenter::WinitFrameTarget;
pub use presenter::{FrameError, FrameOutcome, WindowPresenter};

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
}

/// Provenance supplied by the native keyboard event, not inferred from modifiers.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeKeyboardMetadata {
    pub repeat: bool,
    /// Windows generates lifecycle releases/presses when focus changes.
    /// They are not new user input and must not release or replay consumed chords.
    pub is_synthetic: bool,
}

/// Per-window state for the winit integration.
pub struct WinitAdapter {
    modifiers: ModifiersState,
    /// The latest mouse cursor position in physical pixels.
    mouse_position: Point,
    scale_factor: f32,
    ime_composing: bool,
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
    pub fn new() -> Self {
        Self::with_scale(1.0)
    }

    /// Creates an input adapter with an initial pointer scale factor.
    pub fn with_scale(scale: f32) -> Self {
        Self {
            modifiers: ModifiersState::empty(),
            mouse_position: Point::ZERO,
            scale_factor: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
            ime_composing: false,
            touch_contacts: Vec::new(),
            mouse_release_quarantine: [false; 3],
            active_mouse_buttons: [false; 3],
            quarantined_touches: Vec::new(),
            next_touch_pointer_id: 1,
        }
    }

    /// Creates an adapter initialized with a surface scale factor.
    pub fn with_surface(_width: u32, _height: u32, scale: f32) -> Self {
        Self::with_scale(scale)
    }

    /// Creates an adapter initialized from the window's current scale.
    pub fn from_window(window: &Window) -> Self {
        Self::with_scale(window.scale_factor() as f32)
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
    /// effects produced by the widget runtime, folded through the scheduler.
    ///
    /// Prefer [`Self::handle_event_with_size`] when the host can supply the
    /// window's current physical size so scale-factor changes combine with
    /// post-DPI dimensions rather than a stale SurfaceState snapshot.
    pub fn handle_event(
        &mut self,
        runtime: &mut Runtime,
        event: &WindowEvent,
    ) -> WinitEventOutcome {
        self.handle_event_with_size(runtime, event, None)
    }

    /// Like [`Self::handle_event`], but accepts the host's current physical
    /// window size for resize/DPI lifecycle transitions.
    pub fn handle_event_with_size(
        &mut self,
        runtime: &mut Runtime,
        event: &WindowEvent,
        physical_size: Option<(u32, u32)>,
    ) -> WinitEventOutcome {
        let _ = physical_size;
        if matches!(event, WindowEvent::CloseRequested) {
            self.quarantine_active_pointers();
            let effects = runtime.cancel_active_input_ownership(self.logical_pointer_position());
            return WinitEventOutcome {
                handled: false,
                effects,
            };
        }

        if self.quarantine_pointer_event(event) {
            return WinitEventOutcome::handled(RuntimeEffects::default());
        }
        if let WindowEvent::KeyboardInput {
            event,
            is_synthetic,
            ..
        } = event
        {
            return self.handle_keyboard_input_with_metadata(
                runtime,
                &event.logical_key,
                event.state,
                event.location,
                NativeKeyboardMetadata {
                    repeat: event.repeat,
                    is_synthetic: *is_synthetic,
                },
            );
        }

        let state_only = matches!(
            event,
            WindowEvent::ModifiersChanged(_) | WindowEvent::Ime(_)
        );
        let Some(mut ui_event) = self.convert_event(event) else {
            return if state_only {
                WinitEventOutcome::handled(RuntimeEffects::default())
            } else {
                WinitEventOutcome::unhandled()
            };
        };

        if matches!(event, WindowEvent::Focused(true)) && runtime.input().focus_visible() {
            ui_event = UiEvent::Focus(FocusEvent::GainedVisible);
        }

        let mut effects = runtime.dispatch(ui_event);
        if matches!(event, WindowEvent::CursorLeft { .. }) {
            effects.merge(runtime.cancel_pointer_captures(self.logical_pointer_position()));
        }
        if matches!(event, WindowEvent::Focused(false)) {
            effects.merge(runtime.cancel_active_input_ownership(self.logical_pointer_position()));
            self.modifiers = ModifiersState::empty();
            // Focus loss ends the current composition so a later focus
            // session cannot inherit IME suppression.
            self.ime_composing = false;
            self.touch_contacts.clear();
            effects.merge(RuntimeEffects {
                control_flow: Some(crate::effects::ControlFlowEffect::Wait),
                ..RuntimeEffects::default()
            });
        }
        WinitEventOutcome::handled(effects)
    }
    /// Quarantines releases for every pointer source currently held by the native window.
    /// Hosts use this before replacing an input-owning subtree.
    pub fn quarantine_active_pointers(&mut self) {
        for index in 0..self.active_mouse_buttons.len() {
            self.mouse_release_quarantine[index] |= self.active_mouse_buttons[index];
            self.active_mouse_buttons[index] = false;
        }
        for contact in &self.touch_contacts {
            let identity = (contact.device_id, contact.source_id);
            if !self.quarantined_touches.contains(&identity) {
                self.quarantined_touches.push(identity);
            }
        }
    }

    /// Records a pointer event rejected by a host modal gate without dispatching it.
    /// Matching releases remain quarantined after the gate opens.
    pub fn quarantine_blocked_pointer_event(&mut self, event: &WindowEvent) {
        self.quarantine_active_pointers();
        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                let Some((index, _)) = mouse_button(*button) else {
                    return;
                };
                self.active_mouse_buttons[index] = false;
                self.mouse_release_quarantine[index] = *state == ElementState::Pressed;
            }
            WindowEvent::Touch(winit::event::Touch {
                phase,
                device_id,
                id,
                ..
            }) => {
                let identity = (*device_id, *id);
                match phase {
                    TouchPhase::Started => {
                        if !self.quarantined_touches.contains(&identity) {
                            self.quarantined_touches.push(identity);
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.quarantined_touches.retain(|entry| entry != &identity);
                        self.touch_contacts
                            .retain(|contact| (contact.device_id, contact.source_id) != identity);
                    }
                    TouchPhase::Moved => {}
                }
            }
            _ => {}
        }
    }

    /// Updates native input bookkeeping for widget input rejected by a host gate.
    /// No event from this path is dispatched into the Runtime.
    #[cfg_attr(not(feature = "hmr"), allow(dead_code))]
    pub(crate) fn observe_blocked_widget_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::MouseInput { .. } | WindowEvent::Touch(_) => {
                self.quarantine_blocked_pointer_event(event);
            }
            WindowEvent::ModifiersChanged(_)
            | WindowEvent::Ime(_)
            | WindowEvent::CursorMoved { .. } => {
                let _ = self.convert_event(event);
            }
            _ => {}
        }
    }

    fn quarantine_pointer_event(&mut self, event: &WindowEvent) -> bool {
        if matches!(
            event,
            WindowEvent::Focused(false) | WindowEvent::CursorLeft { .. }
        ) {
            self.quarantine_active_pointers();
            return false;
        }

        match event {
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button,
                ..
            } => {
                let Some((index, _)) = mouse_button(*button) else {
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
                let Some((index, _)) = mouse_button(*button) else {
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
        self.handle_keyboard_input_with_repeat(runtime, logical_key, state, location, false)
    }

    /// Adapt a native press/repeat distinction without changing the widget event
    /// vocabulary. Fresh presses discard ownership whose release a gate dropped.
    pub fn handle_keyboard_input_with_repeat(
        &mut self,
        runtime: &mut Runtime,
        logical_key: &Key,
        state: ElementState,
        location: KeyLocation,
        repeat: bool,
    ) -> WinitEventOutcome {
        self.handle_keyboard_input_with_metadata(
            runtime,
            logical_key,
            state,
            location,
            NativeKeyboardMetadata {
                repeat,
                is_synthetic: false,
            },
        )
    }

    /// Route the same native provenance used by `WindowEvent::KeyboardInput`.
    /// Ignore focus-synthesized events before touching IME or shortcut ownership.
    pub fn handle_keyboard_input_with_metadata(
        &mut self,
        runtime: &mut Runtime,
        logical_key: &Key,
        state: ElementState,
        location: KeyLocation,
        metadata: NativeKeyboardMetadata,
    ) -> WinitEventOutcome {
        if metadata.is_synthetic {
            return WinitEventOutcome::handled(RuntimeEffects::default());
        }
        let is_composing = self.ime_composing;
        let suppressed =
            ime_suppresses_keyboard(logical_key, state, location, self.modifiers, is_composing);
        let Some(ui_event) =
            keyboard_to_uievent(logical_key, state, location, self.modifiers, is_composing)
        else {
            return if suppressed {
                WinitEventOutcome::handled(RuntimeEffects::default())
            } else {
                WinitEventOutcome::unhandled()
            };
        };
        if !metadata.repeat
            && let UiEvent::Keyboard(KeyboardEvent::KeyDown { key, .. }) = &ui_event
        {
            runtime.begin_key_press(*key);
        }
        let effects = runtime.dispatch(ui_event);
        WinitEventOutcome::handled(effects)
    }

    fn convert_event(&mut self, event: &WindowEvent) -> Option<UiEvent> {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                None
            }
            WindowEvent::Ime(ime) => self.convert_ime(ime),
            WindowEvent::CursorEntered { .. } => {
                Some(UiEvent::PointerBoundary(PointerBoundaryEvent {
                    pointer_id: 0,
                    position: Some(self.logical_pointer_position()),
                    kind: PointerBoundaryKind::Enter,
                }))
            }
            WindowEvent::CursorLeft { .. } => {
                Some(UiEvent::PointerBoundary(PointerBoundaryEvent {
                    pointer_id: 0,
                    position: None,
                    kind: PointerBoundaryKind::Leave,
                }))
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_position = Point::new(position.x as f32, position.y as f32);
                Some(UiEvent::Pointer(
                    PointerEvent::new(
                        self.logical_pointer_position(),
                        PointerPhase::Move,
                        PointerButton::Left,
                        0,
                    )
                    .with_modifiers(modifiers_to_widget(self.modifiers)),
                ))
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (_, button) = mouse_button(*button)?;
                let phase = match state {
                    ElementState::Pressed => PointerPhase::Down,
                    ElementState::Released => PointerPhase::Up,
                };
                Some(UiEvent::Pointer(
                    PointerEvent::new(self.logical_pointer_position(), phase, button, 0)
                        .with_modifiers(modifiers_to_widget(self.modifiers)),
                ))
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let phase = match delta {
                    MouseScrollDelta::LineDelta(x, y) => PointerPhase::WheelLine { dx: *x, dy: *y },
                    MouseScrollDelta::PixelDelta(position) => PointerPhase::WheelPixel {
                        dx: position.x as f32 / self.scale_factor,
                        dy: position.y as f32 / self.scale_factor,
                    },
                };
                Some(UiEvent::Pointer(
                    PointerEvent::new(
                        self.logical_pointer_position(),
                        phase,
                        PointerButton::Left,
                        0,
                    )
                    .with_modifiers(modifiers_to_widget(self.modifiers)),
                ))
            }
            WindowEvent::Touch(touch) => {
                let phase = match touch.phase {
                    TouchPhase::Started => PointerPhase::Down,
                    TouchPhase::Moved => PointerPhase::Move,
                    TouchPhase::Ended => PointerPhase::Up,
                    TouchPhase::Cancelled => PointerPhase::Cancel,
                };
                let pointer_id = self.touch_pointer_id(touch.device_id, touch.id);
                let position = Point::new(
                    touch.location.x as f32 / self.scale_factor,
                    touch.location.y as f32 / self.scale_factor,
                );
                if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    self.touch_contacts.retain(|contact| {
                        !(contact.device_id == touch.device_id && contact.source_id == touch.id)
                    });
                }
                Some(UiEvent::Pointer(
                    PointerEvent::new(position, phase, PointerButton::Left, pointer_id)
                        .with_modifiers(modifiers_to_widget(self.modifiers)),
                ))
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
                self.ime_composing = false;
                None
            }
            Ime::Disabled => {
                self.ime_composing = false;
                Some(UiEvent::ImePreedit(
                    crate::input::event::ImePreedit::default(),
                ))
            }
            Ime::Preedit(text, cursor_range) => {
                self.ime_composing = !text.is_empty();
                Some(UiEvent::ImePreedit(crate::input::event::ImePreedit::new(
                    text.clone(),
                    *cursor_range,
                )))
            }
            Ime::Commit(text) => {
                let was_composing = std::mem::take(&mut self.ime_composing);
                if text.is_empty() {
                    was_composing
                        .then(|| UiEvent::ImePreedit(crate::input::event::ImePreedit::default()))
                } else {
                    Some(UiEvent::Keyboard(KeyboardEvent::Ime(text.clone())))
                }
            }
        }
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
        });
        pointer_id
    }

    fn logical_pointer_position(&self) -> Point {
        Point::new(
            self.mouse_position.x / self.scale_factor,
            self.mouse_position.y / self.scale_factor,
        )
    }
}

#[cfg(test)]
mod tests;
