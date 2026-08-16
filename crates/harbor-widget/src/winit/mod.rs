//! Optional winit integration contracts.
//!
//! This module owns the per-window state needed to adapt winit events into the
//! platform-independent widget runtime, including per-window frame scheduling.
//! Host policy (close requests and window routing) remains outside this module.

mod event;
mod presenter;
mod surface;

use crate::effects::{ExternalInvalidation, RuntimeEffects};
use crate::input::event::{
    FocusEvent, KeyboardEvent, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use crate::layout::Point;
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use crate::scheduler::FrameScheduler;
use crate::winit::event::{
    ime_suppresses_keyboard, keyboard_to_uievent, modifiers_to_widget, mouse_button,
    mouse_button_index, physical_size_for_scale_change,
};
use crate::winit::surface::SurfaceState;
use std::time::Instant;
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::keyboard::{Key, KeyLocation, ModifiersState};
use winit::window::Window;

pub use presenter::{FrameError, FrameOutcome, WinitFrameTarget};

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
    /// Per-window redraw coalescing and idle wait policy.
    scheduler: FrameScheduler,
    /// Drawable viewport and surface configuration policy.
    surface_state: SurfaceState,
}

impl Default for WinitAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WinitAdapter {
    pub fn new() -> Self {
        Self::with_surface(0, 0, 1.0)
    }

    /// Creates an adapter with initial physical size and scale factor.
    pub fn with_surface(width: u32, height: u32, scale: f32) -> Self {
        Self {
            modifiers: ModifiersState::empty(),
            mouse_position: Point::ZERO,
            scale_factor: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
            ime_enabled: false,
            ime_preedit: None,
            touch_contacts: Vec::new(),
            mouse_release_quarantine: [false; 3],
            active_mouse_buttons: [false; 3],
            quarantined_touches: Vec::new(),
            next_touch_pointer_id: 1,
            scheduler: FrameScheduler::default(),
            surface_state: SurfaceState::new(width, height, scale),
        }
    }

    /// Creates an adapter initialized from the window's current size and scale.
    pub fn from_window(window: &Window) -> Self {
        let size = window.inner_size();
        Self::with_surface(size.width, size.height, window.scale_factor() as f32)
    }

    /// Returns the adapter's current viewport descriptor.
    pub fn viewport(&self) -> &Viewport {
        self.surface_state.viewport()
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

    /// Updates whether this window can acquire a drawable surface.
    /// Restoring drawability returns one recovery-frame edge.
    pub fn set_drawable(&mut self, drawable: bool) -> RuntimeEffects {
        self.scheduler.set_drawable(drawable)
    }

    /// Folds a raw runtime effect batch through the per-window scheduler.
    pub fn fold_effects(&mut self, effects: RuntimeEffects) -> RuntimeEffects {
        self.scheduler
            .schedule(effects, FrameScheduler::RUNTIME_WAKE)
    }

    /// Forwards source-agnostic host work through runtime external invalidation
    /// and the per-window scheduler.
    pub fn invalidate_external(
        &mut self,
        runtime: &mut Runtime,
        work: ExternalInvalidation,
    ) -> RuntimeEffects {
        self.surface_state.reset_recovery_budget();
        let core_effects = runtime.invalidate_external(work);
        self.scheduler
            .schedule(core_effects, FrameScheduler::EXTERNAL_WAKE)
    }

    /// Observes `RedrawRequested`: runs a runtime update and consumes the
    /// outstanding redraw edge with the current frame.
    pub fn redraw_requested(&mut self, runtime: &mut Runtime, now: Instant) -> RuntimeEffects {
        let core_effects = runtime.update(now);
        self.scheduler.frame_started(core_effects)
    }

    /// Observes successful presentation and may request an active continuation.
    pub fn frame_completed(&mut self, now: Instant) -> RuntimeEffects {
        self.scheduler.frame_completed(now)
    }

    /// Runs an idle turn: folds dirty Fiber work, then calculates wait policy.
    pub fn about_to_wait(
        &mut self,
        runtime: &mut Runtime,
        now: Instant,
        host_deadline: Option<Instant>,
    ) -> RuntimeEffects {
        // Consume a due deadline before update replaces it with the next phase
        // boundary reported by external schedule providers (cursor blink).
        let due_redraw = self.scheduler.consume_due_deadline(now);

        let mut dirty_effects = runtime.update(now);
        if due_redraw {
            dirty_effects.request_redraw = true;
        }
        let mut scheduled = self
            .scheduler
            .schedule(dirty_effects, FrameScheduler::RUNTIME_WAKE);
        // Control-flow from the dirty turn was folded into scheduler state.
        // The idle calculation owns the host-facing wait mode (including host
        // deadlines and due-deadline normalization), so drop the raw CF here
        // before merging redraw/cursor/IME/clipboard side effects.
        scheduled.control_flow = None;
        self.scheduler
            .about_to_wait(now, host_deadline)
            .merged(&scheduled)
    }

    /// Requests a host retry frame (for example after routed terminal input).
    pub fn request_frame(&mut self) -> RuntimeEffects {
        self.scheduler.request_frame()
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
        if let Some(outcome) = self.handle_lifecycle_event(runtime, event, physical_size) {
            return outcome;
        }

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
            WindowEvent::ModifiersChanged(_) | WindowEvent::Ime(_)
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
            effects.merge(RuntimeEffects {
                control_flow: Some(crate::effects::ControlFlowEffect::Wait),
                ..RuntimeEffects::default()
            });
        }
        WinitEventOutcome::handled(self.fold_effects(effects))
    }

    fn handle_lifecycle_event(
        &mut self,
        runtime: &mut Runtime,
        event: &WindowEvent,
        physical_size: Option<(u32, u32)>,
    ) -> Option<WinitEventOutcome> {
        match event {
            WindowEvent::Resized(size) => Some(self.handle_surface_transition(
                runtime,
                size.width,
                size.height,
                self.scale_factor,
            )),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.set_scale_factor(*scale_factor as f32);
                // Combine the new scale with the host's current physical size
                // when available. Falling back to SurfaceState is only for
                // headless tests that omit a window size hint.
                let (width, height) = physical_size_for_scale_change(
                    physical_size,
                    self.surface_state.viewport().physical_size,
                );
                Some(self.handle_surface_transition(runtime, width, height, self.scale_factor))
            }
            _ => None,
        }
    }

    fn handle_surface_transition(
        &mut self,
        runtime: &mut Runtime,
        width: u32,
        height: u32,
        scale: f32,
    ) -> WinitEventOutcome {
        let changed = self.surface_state.update(width, height, scale);
        runtime.set_viewport(self.surface_state.viewport().clone());
        let mut effects = self.set_drawable(self.surface_state.can_acquire());
        self.surface_state.reset_recovery_budget();

        if changed && self.surface_state.can_acquire() {
            effects.merge(self.fold_effects(runtime.update(Instant::now())));
            effects.merge(self.request_frame());
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
        WinitEventOutcome::handled(self.fold_effects(runtime.dispatch(ui_event, Instant::now())))
    }

    fn convert_event(&mut self, event: &WindowEvent) -> Option<UiEvent> {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                None
            }
            WindowEvent::Ime(ime) => self.convert_ime(ime),
            WindowEvent::KeyboardInput { event, .. } => self.convert_keyboard(event),
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
                let button = mouse_button(*button)?;
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
                        dx: position.x as f32,
                        dy: position.y as f32,
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

#[cfg(test)]
mod tests {
    use super::event::modifiers_to_widget;
    use super::presenter::{
        FrameAcquisition, FrameAcquisitionKind, PresentableKind, classify_presentable,
        classify_surface_texture, execute_presented_frame,
    };
    use super::*;
    use crate::input::event::{Key as WidgetKey, KeyboardEvent, Modifiers, PointerPhase};
    use crate::widgets::button::Button;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use winit::dpi::PhysicalPosition;
    use winit::event::MouseButton;
    use winit::keyboard::NamedKey;

    #[test]
    fn presented_frame_operations_are_ordered_and_stop_on_acquire_or_encode_error() {
        use std::cell::RefCell;

        let order = RefCell::new(Vec::new());
        let result = execute_presented_frame(
            || {
                order.borrow_mut().push("acquire");
                Ok::<_, &'static str>("texture")
            },
            |_| {
                order.borrow_mut().push("view");
                "view"
            },
            |_| {
                order.borrow_mut().push("encode");
                Ok::<_, &'static str>("command")
            },
            |_| order.borrow_mut().push("submit"),
            || order.borrow_mut().push("pre_present_notify"),
            |_| order.borrow_mut().push("present"),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            *order.borrow(),
            vec![
                "acquire",
                "view",
                "encode",
                "submit",
                "pre_present_notify",
                "present"
            ]
        );

        let order = RefCell::new(Vec::new());
        let result = execute_presented_frame(
            || {
                order.borrow_mut().push("acquire");
                Err::<&str, _>("acquire")
            },
            |_| {
                order.borrow_mut().push("view");
                "view"
            },
            |_| {
                order.borrow_mut().push("encode");
                Ok::<_, &'static str>("command")
            },
            |_| order.borrow_mut().push("submit"),
            || order.borrow_mut().push("pre_present_notify"),
            |_| order.borrow_mut().push("present"),
        );
        assert_eq!(result, Err("acquire"));
        assert_eq!(*order.borrow(), vec!["acquire"]);

        let order = RefCell::new(Vec::new());
        let result = execute_presented_frame(
            || {
                order.borrow_mut().push("acquire");
                Ok::<_, &'static str>("texture")
            },
            |_| {
                order.borrow_mut().push("view");
                "view"
            },
            |_| {
                order.borrow_mut().push("encode");
                Err::<&str, _>("encode")
            },
            |_| order.borrow_mut().push("submit"),
            || order.borrow_mut().push("pre_present_notify"),
            |_| order.borrow_mut().push("present"),
        );
        assert_eq!(result, Err("encode"));
        assert_eq!(*order.borrow(), vec!["acquire", "view", "encode"]);
    }

    #[test]
    fn surface_texture_statuses_share_production_classification() {
        // wgpu keeps SurfaceTexture construction private, so Success and
        // Suboptimal exercise the shared payload classification seam directly.
        assert_eq!(
            classify_presentable("success", PresentableKind::Presented).kind(),
            FrameAcquisitionKind::Presented
        );
        assert_eq!(
            classify_presentable("suboptimal", PresentableKind::Suboptimal).kind(),
            FrameAcquisitionKind::Suboptimal
        );
        assert_eq!(
            classify_surface_texture(wgpu::CurrentSurfaceTexture::Lost).kind(),
            FrameAcquisitionKind::RecoveryRequired
        );
        assert_eq!(
            classify_surface_texture(wgpu::CurrentSurfaceTexture::Outdated).kind(),
            FrameAcquisitionKind::RecoveryRequired
        );
        assert_eq!(
            classify_surface_texture(wgpu::CurrentSurfaceTexture::Timeout).kind(),
            FrameAcquisitionKind::Skipped
        );
        assert_eq!(
            classify_surface_texture(wgpu::CurrentSurfaceTexture::Occluded).kind(),
            FrameAcquisitionKind::Skipped
        );
        assert_eq!(
            classify_surface_texture(wgpu::CurrentSurfaceTexture::Validation).kind(),
            FrameAcquisitionKind::Skipped
        );
    }

    #[test]
    fn surface_state_tracks_drawable_transitions_and_recovery_budget() {
        let mut state = SurfaceState::new(800, 600, 1.0);
        assert!(state.can_acquire());
        assert!(state.update(800, 600, 2.0));
        assert!(state.configuration_dirty());
        assert!(!state.recovery_attempted());
        assert!(state.allow_recovery_retry());
        assert!(!state.allow_recovery_retry());
        state.reset_after_success();
        assert!(state.allow_recovery_retry());

        state.update(0, 600, 2.0);
        assert!(!state.can_acquire());
        state.reset_recovery_budget();
        assert!(state.allow_recovery_retry());
    }

    #[test]
    fn should_return_false_when_surface_state_update_receives_same_viewport() {
        // Arrange
        let mut state = SurfaceState::new(800, 600, 1.0);
        state.mark_configured();

        // Act
        let changed = state.update(800, 600, 1.0);

        // Assert
        assert!(!changed);
        assert!(!state.configuration_dirty());
    }

    #[test]
    fn should_clear_configuration_dirty_when_surface_state_is_marked_configured() {
        // Arrange
        let mut state = SurfaceState::new(800, 600, 1.0);
        assert!(state.configuration_dirty());

        // Act
        state.mark_configured();

        // Assert
        assert!(!state.configuration_dirty());
    }

    #[test]
    fn should_start_without_configuration_dirty_for_zero_sized_surface() {
        // Arrange / Act
        let state = SurfaceState::new(0, 0, 1.0);

        // Assert
        assert!(!state.configuration_dirty());
        assert!(!state.can_acquire());
    }

    #[test]
    fn should_reset_recovery_budget_when_surface_state_viewport_changes() {
        // Arrange
        let mut state = SurfaceState::new(800, 600, 1.0);
        assert!(state.allow_recovery_retry());
        assert!(!state.allow_recovery_retry());

        // Act
        assert!(state.update(1024, 768, 1.0));

        // Assert
        assert!(state.allow_recovery_retry());
    }

    #[test]
    fn should_update_adapter_and_runtime_viewport_when_window_is_resized() {
        // Arrange
        use crate::runtime::Runtime;
        use winit::dpi::PhysicalSize;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();
        let resized = WindowEvent::Resized(PhysicalSize::new(1024, 768));

        // Act
        let outcome = adapter.handle_event(&mut runtime, &resized);

        // Assert
        assert!(outcome.is_handled());
        assert_eq!(adapter.viewport().physical_size, (1024, 768));
        assert_eq!(
            runtime.current_viewport().map(|vp| vp.physical_size),
            Some((1024, 768))
        );
        assert!(outcome.effects.request_redraw);
    }

    #[test]
    fn should_prefer_host_physical_size_on_scale_change() {
        // Arrange / Act / Assert: DPI events must not keep the pre-DPI physical size.
        assert_eq!(
            physical_size_for_scale_change(Some((1600, 1200)), (800, 600)),
            (1600, 1200)
        );
        assert_eq!(physical_size_for_scale_change(None, (800, 600)), (800, 600));
    }

    #[test]
    fn should_combine_host_physical_size_when_scale_factor_changes() {
        // Arrange: DPI change often arrives before Resized; the host supplies
        // window.inner_size() so logical layout does not use a stale physical.
        // (WindowEvent::ScaleFactorChanged cannot be constructed outside winit,
        // so this exercises the same transition the lifecycle handler applies.)
        use crate::runtime::Runtime;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();
        adapter.set_scale_factor(2.0);

        // Act
        let outcome = adapter.handle_surface_transition(&mut runtime, 1600, 1200, 2.0);

        // Assert
        assert!(outcome.is_handled());
        assert_eq!(adapter.viewport().physical_size, (1600, 1200));
        assert!((adapter.viewport().scale_factor - 2.0).abs() < f32::EPSILON);
        assert_eq!(
            runtime.current_viewport().map(|vp| vp.physical_size),
            Some((1600, 1200))
        );
        assert!(outcome.effects.request_redraw);
    }

    #[test]
    fn should_not_request_redraw_when_resize_reports_same_viewport() {
        // Arrange
        use crate::runtime::Runtime;
        use winit::dpi::PhysicalSize;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::Resized(PhysicalSize::new(800, 600)),
        );

        // Act
        let outcome = adapter.handle_event(
            &mut runtime,
            &WindowEvent::Resized(PhysicalSize::new(800, 600)),
        );

        // Assert
        assert!(outcome.is_handled());
        assert!(!outcome.effects.request_redraw);
    }

    #[test]
    fn should_mark_surface_non_drawable_when_resized_to_zero_extent() {
        // Arrange
        use crate::runtime::Runtime;
        use winit::dpi::PhysicalSize;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();

        // Act
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::Resized(PhysicalSize::new(0, 600)),
        );

        // Assert
        assert!(!adapter.viewport().is_drawable());
        assert_eq!(
            runtime.current_viewport().map(|vp| vp.physical_size),
            Some((0, 600))
        );
    }

    #[test]
    fn should_not_request_redraw_when_resized_to_zero_extent() {
        // Arrange
        use crate::runtime::Runtime;
        use winit::dpi::PhysicalSize;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();

        // Act
        let outcome = adapter.handle_event(
            &mut runtime,
            &WindowEvent::Resized(PhysicalSize::new(0, 600)),
        );

        // Assert: zero-size suspends acquisition without scheduling a frame.
        assert!(outcome.is_handled());
        assert!(!outcome.effects.request_redraw);
    }

    #[test]
    fn should_request_redraw_when_restored_from_zero_extent() {
        // Arrange
        use crate::runtime::Runtime;
        use winit::dpi::PhysicalSize;

        let mut adapter = WinitAdapter::with_surface(0, 0, 1.0);
        let mut runtime = Runtime::new();
        adapter.handle_event(&mut runtime, &WindowEvent::Resized(PhysicalSize::new(0, 0)));

        // Act
        let outcome = adapter.handle_event(
            &mut runtime,
            &WindowEvent::Resized(PhysicalSize::new(800, 600)),
        );

        // Assert
        assert!(outcome.is_handled());
        assert!(adapter.viewport().is_drawable());
        assert!(outcome.effects.request_redraw);
    }

    #[test]
    fn should_request_one_recovery_frame_when_acquisition_is_lost() {
        // Arrange
        use crate::runtime::Runtime;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();
        let frame_start = RuntimeEffects::default();

        // Act
        let first = adapter.finish_acquisition(
            frame_start.clone(),
            FrameAcquisition::<&str>::RecoveryRequired,
            |_| Ok(()),
        );
        // Host consumes the recovery redraw before the next acquisition attempt.
        let _ = adapter.redraw_requested(&mut runtime, Instant::now());
        let second = adapter.finish_acquisition(
            frame_start,
            FrameAcquisition::<&str>::RecoveryRequired,
            |_| Ok(()),
        );

        // Assert: one internal retry, then wait for an external wake.
        assert!(first.is_recovery_required());
        assert!(first.effects().request_redraw);
        assert!(second.is_recovery_required());
        assert!(!second.effects().request_redraw);
    }

    #[test]
    fn should_reset_recovery_budget_when_external_invalidation_arrives() {
        // Arrange
        use crate::effects::ExternalInvalidation;
        use crate::runtime::Runtime;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();
        let exhausted = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::<&str>::RecoveryRequired,
            |_| Ok(()),
        );
        assert!(exhausted.effects().request_redraw);
        let _ = adapter.redraw_requested(&mut runtime, Instant::now());
        let blocked = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::<&str>::RecoveryRequired,
            |_| Ok(()),
        );
        assert!(!blocked.effects().request_redraw);

        // Act: a fresh external wake restores the one-retry budget.
        let _ = adapter.invalidate_external(&mut runtime, ExternalInvalidation::new());
        let _ = adapter.redraw_requested(&mut runtime, Instant::now());
        let retry = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::<&str>::RecoveryRequired,
            |_| Ok(()),
        );

        // Assert
        assert!(retry.is_recovery_required());
        assert!(retry.effects().request_redraw);
    }

    #[test]
    fn should_not_request_recovery_frame_when_acquisition_is_skipped() {
        // Arrange
        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);

        // Act
        let outcome = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::<&str>::Skipped,
            |_| Ok(()),
        );

        // Assert: timeout/occluded/validation skip without completion or retry.
        assert!(!outcome.is_presented());
        assert!(!outcome.is_recovery_required());
        assert!(!outcome.effects().request_redraw);
    }

    #[test]
    fn should_request_recovery_frame_when_presentation_is_suboptimal() {
        // Arrange
        use crate::runtime::Runtime;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();

        // Act
        let first = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::Suboptimal("ok"),
            |_| Ok(()),
        );
        let _ = adapter.redraw_requested(&mut runtime, Instant::now());
        let second = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::Suboptimal("ok"),
            |_| Ok(()),
        );

        // Assert
        assert!(first.is_presented());
        assert!(first.is_suboptimal());
        assert!(first.effects().request_redraw);
        assert!(second.is_presented());
        assert!(second.is_suboptimal());
        assert!(!second.effects().request_redraw);
    }

    #[test]
    fn should_restore_recovery_budget_after_successful_presentation() {
        // Arrange
        use crate::runtime::Runtime;

        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let mut runtime = Runtime::new();
        let exhausted = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::<&str>::RecoveryRequired,
            |_| Ok(()),
        );
        assert!(exhausted.effects().request_redraw);
        let _ = adapter.redraw_requested(&mut runtime, Instant::now());

        // Act: a successful present resets the one-retry budget.
        let presented = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::Presented("ok"),
            |_| Ok(()),
        );
        let retry = adapter.finish_acquisition(
            RuntimeEffects::default(),
            FrameAcquisition::<&str>::RecoveryRequired,
            |_| Ok(()),
        );

        // Assert
        assert!(presented.is_presented());
        assert!(retry.is_recovery_required());
        assert!(retry.effects().request_redraw);
    }

    #[test]
    fn acquisition_policy_returns_frame_start_effects_and_completes_only_presented_frames() {
        use crate::effects::{ControlFlowEffect, CursorEffect, CursorShape};
        use std::cell::Cell;

        let frame_start = RuntimeEffects {
            request_redraw: true,
            control_flow: Some(ControlFlowEffect::wait_until(Instant::now())),
            cursor: Some(CursorEffect::set_cursor(CursorShape::Pointer)),
            ..RuntimeEffects::default()
        };

        // A skipped acquisition returns exactly the frame-start batch and does
        // not invoke presentation or consume the active continuation.
        {
            let mut adapter = WinitAdapter::new();
            adapter.fold_effects(RuntimeEffects {
                control_flow: Some(ControlFlowEffect::poll()),
                ..RuntimeEffects::default()
            });
            let present_count = Cell::new(0);
            let outcome = adapter.finish_acquisition(
                frame_start.clone(),
                FrameAcquisition::<&str>::Skipped,
                |_| {
                    present_count.set(present_count.get() + 1);
                    Ok(())
                },
            );

            assert_eq!(outcome.effects(), &frame_start);
            assert!(!outcome.is_presented());
            assert_eq!(present_count.get(), 0);

            let continuation = adapter.frame_completed(Instant::now());
            assert!(continuation.request_redraw);
            assert_eq!(continuation.control_flow, Some(ControlFlowEffect::Poll));
        }

        // RecoveryRequired grants one retry edge; that pending redraw means the
        // scheduler has already consumed the continuation opportunity.
        {
            let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
            adapter.fold_effects(RuntimeEffects {
                control_flow: Some(ControlFlowEffect::poll()),
                ..RuntimeEffects::default()
            });
            let present_count = Cell::new(0);
            let outcome = adapter.finish_acquisition(
                RuntimeEffects::default(),
                FrameAcquisition::<&str>::RecoveryRequired,
                |_| {
                    present_count.set(present_count.get() + 1);
                    Ok(())
                },
            );

            assert!(outcome.is_recovery_required());
            assert!(outcome.effects().request_redraw);
            assert_eq!(present_count.get(), 0);
            assert!(!adapter.frame_completed(Instant::now()).request_redraw);
        }

        for (acquisition, expected_suboptimal) in [
            (FrameAcquisition::Presented("success"), false),
            (FrameAcquisition::Suboptimal("suboptimal"), true),
        ] {
            let mut adapter = WinitAdapter::new();
            adapter.fold_effects(RuntimeEffects {
                control_flow: Some(ControlFlowEffect::poll()),
                ..RuntimeEffects::default()
            });
            let present_count = Cell::new(0);
            let outcome = adapter.finish_acquisition(frame_start.clone(), acquisition, |label| {
                assert_eq!(
                    label,
                    if expected_suboptimal {
                        "suboptimal"
                    } else {
                        "success"
                    }
                );
                present_count.set(present_count.get() + 1);
                Ok(())
            });

            assert!(outcome.is_presented());
            assert_eq!(outcome.is_suboptimal(), expected_suboptimal);
            assert_eq!(present_count.get(), 1);
            assert!(outcome.effects().request_redraw);
            assert_eq!(
                outcome.effects().control_flow,
                Some(ControlFlowEffect::Poll)
            );
            assert_eq!(outcome.effects().cursor, frame_start.cursor);
        }

        // An execution failure is not a completed presentation, so it keeps
        // only frame-start effects and leaves completion available to the host.
        let mut adapter = WinitAdapter::new();
        adapter.fold_effects(RuntimeEffects {
            control_flow: Some(ControlFlowEffect::poll()),
            ..RuntimeEffects::default()
        });
        let failed = adapter.finish_acquisition(
            frame_start.clone(),
            FrameAcquisition::Presented("failed"),
            |_| Err(FrameError::other("encode")),
        );
        assert!(failed.is_fatal());
        assert_eq!(failed.effects(), &frame_start);
        assert!(adapter.frame_completed(Instant::now()).request_redraw);
    }

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

        let hover = adapter.handle_event(
            &mut runtime,
            &WindowEvent::CursorMoved {
                device_id: winit::event::DeviceId::dummy(),
                position: PhysicalPosition::new(4.0, 4.0),
            },
        );
        assert!(hover.effects.request_redraw);
        adapter.handle_event(
            &mut runtime,
            &WindowEvent::MouseInput {
                device_id: winit::event::DeviceId::dummy(),
                state: ElementState::Pressed,
                button: MouseButton::Left,
            },
        );
        let focus_loss = adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
        // Hover already owns the outstanding redraw edge; later cancel work coalesces.
        assert!(!focus_loss.effects.request_redraw);
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
