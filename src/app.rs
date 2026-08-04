//! Application shell: winit lifecycle, window bootstrap, frame render.

mod confirmation;

use std::{
    cell::Cell,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::{CursorIcon, Window, WindowId},
};

use crate::event::AppEvent;
use confirmation::ConfirmationWindow;
use harbor_pty::PtyEndpoints;
use harbor_terminal::{GpuContext, Terminal, TextMetrics, load_system_fonts};
use harbor_widget::effects::{
    ClipboardEffect, ControlFlowEffect, CursorEffect, CursorShape, ExternalInvalidation, ImeEffect,
    RuntimeEffects,
};
use harbor_widget::input::event::{KeyboardEvent, PointerPhase, UiEvent};
use harbor_widget::layout::Point;
use harbor_widget::winit::{FrameOutcome, WinitAdapter, WinitFrameTarget};

// ── Thread-local GPU context scope for widget external draw pass ──────────────

thread_local! {
    static CURRENT_GPU: Cell<Option<*const GpuContext>> = const { Cell::new(None) };
}

/// Executes a closure with `gpu` set as the thread-local active GPU context.
/// Resets to `None` on return or unwind.
pub(crate) fn with_current_gpu<R>(gpu: &GpuContext, f: impl FnOnce() -> R) -> R {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            CURRENT_GPU.with(|c| c.set(None));
        }
    }
    CURRENT_GPU.with(|c| c.set(Some(gpu as *const GpuContext)));
    let _guard = Guard;
    f()
}

/// Accesses the active GPU context if within a `with_current_gpu` scope.
pub(crate) fn current_gpu<R>(f: impl FnOnce(&GpuContext) -> R) -> Option<R> {
    CURRENT_GPU.with(|c| {
        let ptr = c.get()?;
        let gpu = unsafe { &*ptr };
        Some(f(gpu))
    })
}

fn routes_terminal_input(
    gate_active: bool,
    event_draw_id: harbor_terminal::ExternalDrawId,
    terminal_draw_id: harbor_terminal::ExternalDrawId,
    event: &UiEvent,
) -> bool {
    if event_draw_id != terminal_draw_id {
        return false;
    }
    if !gate_active {
        return true;
    }
    is_terminal_wheel(event)
}

fn route_terminal_inputs(
    gate_active: bool,
    terminal_draw_id: harbor_terminal::ExternalDrawId,
    events: impl IntoIterator<Item = (harbor_terminal::ExternalDrawId, UiEvent)>,
    mut handle: impl FnMut(UiEvent),
) -> bool {
    let mut routed = false;
    for (event_draw_id, event) in events {
        if routes_terminal_input(gate_active, event_draw_id, terminal_draw_id, &event) {
            routed = true;
            handle(event);
        }
    }
    routed
}

fn is_terminal_key_press(event: &UiEvent) -> bool {
    matches!(event, UiEvent::Keyboard(KeyboardEvent::KeyDown { .. }))
}

fn is_terminal_wheel(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::Pointer(pointer)
            if matches!(
                pointer.phase,
                PointerPhase::WheelLine { .. } | PointerPhase::WheelPixel { .. }
            )
    )
}

fn wakes_redraw_for_routed_input(event: &UiEvent) -> bool {
    // KeyDown always wakes (PTY write / scrollback nav). Wheel wakes only when
    // the viewport actually moves — checked at the delivery site.
    is_terminal_key_press(event)
}

/// Post-delivery redraw wake: KeyDown always; wheel only when `view_offset` moved.
fn needs_redraw_wake_after_delivery(
    key_wakes: bool,
    offset_before: Option<usize>,
    offset_after: usize,
) -> bool {
    key_wakes || matches!(offset_before, Some(before) if before != offset_after)
}

/// Maps host wake events to source-agnostic runtime invalidation.
fn external_invalidation_for_app_event(event: AppEvent) -> Option<ExternalInvalidation> {
    match event {
        AppEvent::TerminalOutputReady => Some(ExternalInvalidation::new()),
    }
}

fn control_flow_for_effect(effect: ControlFlowEffect) -> ControlFlow {
    match effect {
        ControlFlowEffect::Wait => ControlFlow::Wait,
        ControlFlowEffect::WaitUntil(deadline) => ControlFlow::WaitUntil(deadline),
        ControlFlowEffect::Poll => ControlFlow::Poll,
    }
}

fn cursor_icon_for_effect(effect: CursorEffect) -> CursorIcon {
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

#[derive(Debug, PartialEq, Eq)]
enum ClipboardHostAction {
    Deferred(ClipboardEffect),
}

/// Returns the operation metadata safe to include in clipboard logs.
fn clipboard_log_metadata(effect: &ClipboardEffect) -> (&'static str, usize) {
    match effect {
        ClipboardEffect::Read => ("read", 0),
        ClipboardEffect::Write(contents) => ("write", contents.len()),
    }
}

/// Logs a clipboard effect that remains deferred until a host result channel
/// exists. In particular, a read is never performed and then discarded by this
/// application shell.
fn log_deferred_clipboard_effect(effect: &ClipboardEffect) {
    let (operation, byte_len) = clipboard_log_metadata(effect);
    tracing::warn!(
        operation,
        byte_len,
        "clipboard effect deferred: host result channel is not implemented"
    );
}

/// Keeps an owned clipboard effect at the platform-neutral boundary until a
/// host result channel exists.
fn apply_clipboard_effect(effect: ClipboardEffect) -> ClipboardHostAction {
    log_deferred_clipboard_effect(&effect);
    ClipboardHostAction::Deferred(effect)
}

/// Outcome of a dialog-overlay event dispatch.
struct DialogResult {
    outcome: DialogOutcome,
}

enum DialogOutcome {
    None,
    Cancelled,
    Confirmed(String),
}

/// Owns the optional paste-confirmation dialog and mediates its lifecycle.
struct DialogOverlay {
    window: Option<ConfirmationWindow>,
}

impl DialogOverlay {
    fn is_active(&self) -> bool {
        self.window.is_some()
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.window_id())
    }

    fn about_to_wait(&mut self, now: Instant) -> Option<ControlFlowEffect> {
        self.window.as_mut().map(|window| window.about_to_wait(now))
    }

    /// Dispatches a window event to the active confirmation dialog.
    fn handle_event(
        &mut self,
        event: &WindowEvent,
        event_loop: &ActiveEventLoop,
        gpu: Option<&GpuContext>,
        terminal: Option<&Arc<Mutex<Terminal>>>,
    ) -> DialogResult {
        let Some(mut confirmation) = self.window.take() else {
            return DialogResult {
                outcome: DialogOutcome::None,
            };
        };
        let outcome = match confirmation.handle_event(event, event_loop) {
            confirmation::ConfirmationResult::Cancelled => DialogOutcome::Cancelled,
            confirmation::ConfirmationResult::Confirmed => {
                let raw_text = confirmation.raw_text().to_owned();
                DialogOutcome::Confirmed(raw_text)
            }
            confirmation::ConfirmationResult::None => {
                if matches!(event, WindowEvent::RedrawRequested)
                    && let (Some(gpu), Some(terminal)) = (gpu, terminal)
                {
                    let term = terminal.lock().unwrap();
                    confirmation.render(event_loop, gpu.device(), gpu.queue(), &|ch| {
                        term.text_glyph(ch).copied()
                    });
                }
                if let WindowEvent::ScaleFactorChanged { scale_factor, .. } = event
                    && let Some(gpu) = gpu
                {
                    confirmation.scale_factor_changed(event_loop, gpu.device(), *scale_factor);
                }
                if let WindowEvent::Resized(size) = event
                    && let Some(gpu) = gpu
                {
                    confirmation.resize(event_loop, gpu.device(), size.width, size.height);
                }
                self.window = Some(confirmation);
                DialogOutcome::None
            }
        };
        DialogResult { outcome }
    }

    /// Installs a new confirmation dialog, replacing any existing one.
    #[allow(dead_code)]
    fn open(&mut self, confirmation: ConfirmationWindow) {
        self.window = Some(confirmation);
    }
}

/// Runtime resources that exist while the window is alive.
struct AppRuntime {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    terminal: Option<Arc<Mutex<Terminal>>>,
    /// Widget framework runtime.
    widget_runtime: Option<harbor_widget::runtime::Runtime>,
    /// Main-window input adapter, sharing the runtime's window lifecycle.
    winit_adapter: Option<WinitAdapter>,
    dialog: DialogOverlay,
}

/// Host frame lifecycle telemetry and surface-recovery bookkeeping.
struct FrameState {
    surface_recovery_attempted: bool,
    /// Set after the first successful surface present.
    first_present_at: Option<Instant>,
    /// Once-only gate for the steady-state dwell marker.
    steady_state_emitted: bool,
}

/// Documented 5s dwell after first present for the `steady_state` lifecycle marker.
const FONT_STEADY_STATE_DWELL: Duration = Duration::from_secs(5);
/// Keep in sync with `harbor-text` `LIFECYCLE_TARGET` (`font.rs` / `dwrite.rs`).
const FONT_LIFECYCLE_TARGET: &str = "harbor.font.lifecycle";

impl FrameState {
    /// Records the first successful present and emits `first_present` once.
    fn mark_first_present(&mut self) {
        self.mark_first_present_at(Instant::now());
    }

    fn mark_first_present_at(&mut self, at: Instant) {
        if self.first_present_at.is_some() {
            return;
        }
        self.first_present_at = Some(at);
        tracing::info!(
            target: FONT_LIFECYCLE_TARGET,
            phase = "first_present",
            "font lifecycle"
        );
    }

    /// Emits `steady_state` once after the documented dwell past first present.
    fn maybe_emit_steady_state_at(&mut self, now: Instant) {
        if self.steady_state_emitted {
            return;
        }
        let Some(presented_at) = self.first_present_at else {
            return;
        };
        let dwell = now.saturating_duration_since(presented_at);
        if dwell < FONT_STEADY_STATE_DWELL {
            return;
        }
        self.steady_state_emitted = true;
        tracing::info!(
            target: FONT_LIFECYCLE_TARGET,
            phase = "steady_state",
            dwell_ms = dwell.as_millis() as u64,
            "font lifecycle"
        );
    }

    /// Returns the future font-lifecycle telemetry deadline, or emits the
    /// marker when due. Does not choose a winit control-flow mode.
    fn next_steady_state_deadline(&mut self) -> Option<Instant> {
        self.next_steady_state_deadline_at(Instant::now())
    }

    fn next_steady_state_deadline_at(&mut self, now: Instant) -> Option<Instant> {
        if self.steady_state_emitted {
            return None;
        }
        let presented_at = self.first_present_at?;
        let deadline = presented_at + FONT_STEADY_STATE_DWELL;
        if now >= deadline {
            self.maybe_emit_steady_state_at(now);
            return None;
        }
        Some(deadline)
    }
}

/// Winit coordinator over concrete lifecycle state groups.
pub(crate) struct App {
    runtime: AppRuntime,
    frame: FrameState,
    event_proxy: EventLoopProxy<AppEvent>,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create pty endpoints")]
    Pty(#[source] anyhow::Error),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
}

// ── ApplicationHandler (winit lifecycle) ──────────────────────────────────
impl ApplicationHandler<AppEvent> for App {
    /// Called on start or wake from suspend. Bootstraps the window, GPU,
    /// terminal engine, and PTY on first call.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    /// Handles redraw wakes posted by the terminal reader thread.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        let Some(invalidation) = external_invalidation_for_app_event(event) else {
            return;
        };
        let (Some(adapter), Some(runtime), Some(window)) = (
            self.runtime.winit_adapter.as_mut(),
            self.runtime.widget_runtime.as_mut(),
            self.runtime.window.as_ref(),
        ) else {
            return;
        };
        let effects = adapter.invalidate_external(runtime, invalidation);
        Self::apply_window_effects(window, &effects);
        if let Some(control_flow) = effects.control_flow {
            Self::apply_control_flow(event_loop, control_flow);
        }
    }

    /// Called when the event loop is about to block.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let host_deadline = self.frame.next_steady_state_deadline();

        let mut combined_flow = ControlFlowEffect::Wait;
        if let (Some(adapter), Some(runtime), Some(window)) = (
            self.runtime.winit_adapter.as_mut(),
            self.runtime.widget_runtime.as_mut(),
            self.runtime.window.as_ref(),
        ) {
            let main_effects = adapter.about_to_wait(runtime, now, host_deadline);
            Self::apply_window_effects(window, &main_effects);
            if let Some(flow) = main_effects.control_flow {
                combined_flow = flow;
            }
        }

        if let Some(confirmation_flow) = self.runtime.dialog.about_to_wait(now) {
            combined_flow = combined_flow.arbitrate(confirmation_flow);
        }

        Self::apply_control_flow(event_loop, combined_flow);
    }

    /// Dispatches window-level events: resize, redraw, close, and terminal input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let dialog_window_id = self.runtime.dialog.window_id();
        if dialog_window_id == Some(window_id) {
            let result = self.runtime.dialog.handle_event(
                &event,
                event_loop,
                self.runtime.gpu.as_ref(),
                self.runtime.terminal.as_ref(),
            );
            match result.outcome {
                DialogOutcome::Cancelled => {
                    self.request_main_frame(event_loop);
                    return;
                }
                DialogOutcome::Confirmed(raw_text) => {
                    if let Some(terminal) = self.runtime.terminal.as_ref()
                        && let Ok(mut terminal) = terminal.lock()
                    {
                        let bytes = terminal
                            .drain_and_snapshot()
                            .input_modes
                            .paste(raw_text.as_bytes());
                        if let Err(error) = terminal.write_pty(&bytes) {
                            tracing::warn!(error = %format_args!("{error:#}"), "failed to write confirmed paste");
                        }
                    }
                    self.request_main_frame(event_loop);
                    return;
                }
                DialogOutcome::None => {}
            }
        }
        let gate_active = self.runtime.dialog.is_active();

        let (Some(gpu), Some(terminal), Some(window)) = (
            self.runtime.gpu.as_mut(),
            self.runtime.terminal.as_ref(),
            self.runtime.window.as_ref(),
        ) else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        if gate_active && matches!(&event, WindowEvent::KeyboardInput { .. }) {
            return;
        }

        if matches!(&event, WindowEvent::CloseRequested) {
            tracing::info!("close requested");
            event_loop.exit();
            return;
        }

        let outcome = match (
            self.runtime.winit_adapter.as_mut(),
            self.runtime.widget_runtime.as_mut(),
        ) {
            (Some(adapter), Some(widget_runtime)) => {
                Some(adapter.handle_event(widget_runtime, &event))
            }
            _ => None,
        };
        if let Some(outcome) = outcome
            && outcome.handled
        {
            Self::apply_window_effects(window, &outcome.effects);
            if let Some(control_flow) = outcome.effects.control_flow {
                Self::apply_control_flow(event_loop, control_flow);
            }
            if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
                let draw_id = terminal.lock().unwrap().draw_id();
                let needs_redraw_wake = std::cell::Cell::new(false);
                let routed = route_terminal_inputs(
                    gate_active,
                    draw_id,
                    widget_runtime.drain_external_input(),
                    |external_event| {
                        let mut terminal = terminal.lock().unwrap();
                        let wheel = is_terminal_wheel(&external_event);
                        let offset_before = wheel.then(|| terminal.screen().view_offset());
                        let key_wakes = wakes_redraw_for_routed_input(&external_event);
                        if let Err(error) = terminal.handle_event(external_event) {
                            tracing::warn!(error = %format_args!("{error:#}"), "failed to write terminal input");
                        }
                        if needs_redraw_wake_after_delivery(
                            key_wakes,
                            offset_before,
                            terminal.screen().view_offset(),
                        ) {
                            needs_redraw_wake.set(true);
                        }
                    },
                );
                // CustomPaint does not necessarily invalidate paint for a
                // terminal keypress or wheel. Request a host retry frame for
                // routed KeyDown and for wheels that actually moved the
                // viewport (alt-screen / zero-line wheels stay silent).
                if routed
                    && needs_redraw_wake.get()
                    && let Some(adapter) = self.runtime.winit_adapter.as_mut()
                {
                    let effects = adapter.request_frame();
                    Self::apply_window_effects(window, &effects);
                    if let Some(control_flow) = effects.control_flow {
                        Self::apply_control_flow(event_loop, control_flow);
                    }
                }
            }
        }

        match event {
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                self.frame.surface_recovery_attempted = false;
                if size.width == 0 || size.height == 0 {
                    if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
                        adapter.set_drawable(false);
                    }
                    return;
                }
                if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
                    adapter.set_drawable(true);
                }
                gpu.resize(size.width, size.height);
                let mut terminal = terminal.lock().unwrap();
                let terminal_size = terminal.terminal_size(gpu);
                terminal.resize_gpu(terminal_size, gpu);
                if let (Some(adapter), Some(widget_runtime)) = (
                    self.runtime.winit_adapter.as_mut(),
                    self.runtime.widget_runtime.as_mut(),
                ) {
                    let scale = window.scale_factor() as f32;
                    let viewport =
                        harbor_widget::renderer::Viewport::new(size.width, size.height, scale);
                    widget_runtime.set_viewport(viewport);
                    let mut effects = adapter.fold_effects(widget_runtime.update(Instant::now()));
                    effects.merge(adapter.request_frame());
                    Self::apply_window_effects(window, &effects);
                    if let Some(control_flow) = effects.control_flow {
                        Self::apply_control_flow(event_loop, control_flow);
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                tracing::trace!(?scale_factor, "main window scale factor changed");
                let scale = scale_factor as f32;
                let size = window.inner_size();
                self.frame.surface_recovery_attempted = false;
                if size.width == 0 || size.height == 0 {
                    if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
                        adapter.set_drawable(false);
                    }
                    return;
                }
                if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
                    adapter.set_drawable(true);
                }
                let (physical_width, physical_height) = (size.width, size.height);
                gpu.resize(physical_width, physical_height);
                if let (Some(adapter), Some(widget_runtime)) = (
                    self.runtime.winit_adapter.as_mut(),
                    self.runtime.widget_runtime.as_mut(),
                ) {
                    let viewport = harbor_widget::renderer::Viewport::new(
                        physical_width,
                        physical_height,
                        scale,
                    );
                    widget_runtime.set_viewport(viewport);
                    let mut effects = adapter.fold_effects(widget_runtime.update(Instant::now()));
                    effects.merge(adapter.request_frame());
                    Self::apply_window_effects(window, &effects);
                    if let Some(control_flow) = effects.control_flow {
                        Self::apply_control_flow(event_loop, control_flow);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                self.render_frame(event_loop);
            }
            _ => {}
        }
    }
}

// ── App (own methods) ─────────────────────────────────────────────────────
impl App {
    /// Creates the application shell with no initial window, GPU, or terminal.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            runtime: AppRuntime {
                window: None,
                gpu: None,
                terminal: None,
                widget_runtime: None,
                winit_adapter: None,
                dialog: DialogOverlay { window: None },
            },
            frame: FrameState {
                surface_recovery_attempted: false,
                first_present_at: None,
                steady_state_emitted: false,
            },
            event_proxy,
        }
    }

    /// Creates the main window, GPU context, font atlas, and terminal engine.
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.runtime.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);
        // Winit 0.30 emits composition commits only after IME is explicitly enabled.
        window.set_ime_allowed(true);

        #[cfg(target_os = "windows")]
        paint_gdi_background(&window);

        let gpu =
            pollster::block_on(GpuContext::new(window.clone())).map_err(AppError::Renderer)?;
        let initial_size = window.inner_size();
        if initial_size.width != 0 && initial_size.height != 0 {
            gpu.clear_surface(bg_wgpu(harbor_config::BACKGROUND));
        }

        // Create DirectWrite objects on the UI/render owning thread (no font-loader thread).
        let fonts = load_system_fonts().map_err(AppError::Renderer)?;
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());

        let size = Terminal::terminal_size_for(&gpu, &metrics);
        let (pty_read, pty_write, pty_control) = PtyEndpoints::spawn_shell(size)
            .map_err(AppError::Pty)?
            .into_parts();
        let event_proxy = self.event_proxy.clone();
        let terminal = Terminal::new(
            size,
            pty_read,
            pty_write,
            pty_control,
            &gpu,
            fonts,
            metrics,
            move || {
                event_proxy
                    .send_event(AppEvent::TerminalOutputReady)
                    .is_ok()
            },
        );
        let terminal = Arc::new(Mutex::new(terminal));

        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
        self.runtime.gpu = Some(gpu);
        self.runtime.terminal = Some(terminal);
        let mut winit_adapter = WinitAdapter::new();
        winit_adapter.set_scale_factor(window.scale_factor() as f32);
        let initial_size = window.inner_size();
        winit_adapter.set_drawable(initial_size.width != 0 && initial_size.height != 0);
        self.runtime.winit_adapter = Some(winit_adapter);
        let initial_effects = self.init_widget_runtime();
        self.runtime.window = Some(window.clone());
        if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
            let mut effects = adapter.fold_effects(initial_effects);
            effects.merge(adapter.request_frame());
            Self::apply_window_effects(&window, &effects);
            if let Some(control_flow) = effects.control_flow {
                Self::apply_control_flow(event_loop, control_flow);
            }
        }
        Ok(())
    }

    /// Applies adapter-authorized window effects without calculating wait policy.
    pub(crate) fn apply_window_effects(window: &Window, effects: &RuntimeEffects) {
        if let Some(cursor) = effects.cursor {
            window.set_cursor(cursor_icon_for_effect(cursor));
        }
        if let Some(ImeEffect { allowed, position }) = effects.ime {
            if let Some(allowed) = allowed {
                window.set_ime_allowed(allowed);
            }
            if let Some(position) = position {
                let (position, size) = ime_cursor_area(position);
                window.set_ime_cursor_area(position, size);
            }
        }
        if let Some(clipboard) = effects.clipboard.as_ref() {
            log_deferred_clipboard_effect(clipboard);
        }
        if effects.request_redraw {
            tracing::trace!("requesting redraw");
            window.request_redraw();
        }
    }

    fn apply_control_flow(event_loop: &ActiveEventLoop, effect: ControlFlowEffect) {
        event_loop.set_control_flow(control_flow_for_effect(effect));
    }

    fn request_main_frame(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(adapter), Some(window)) = (
            self.runtime.winit_adapter.as_mut(),
            self.runtime.window.as_ref(),
        ) else {
            return;
        };
        let effects = adapter.request_frame();
        Self::apply_window_effects(window, &effects);
        if let Some(control_flow) = effects.control_flow {
            Self::apply_control_flow(event_loop, control_flow);
        }
    }

    /// Runs one borrowed main-window frame through the winit integration.
    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.runtime.window.as_ref() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
                adapter.set_drawable(false);
            }
            return;
        }
        if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
            adapter.set_drawable(true);
        }

        if let Some(terminal) = self.runtime.terminal.as_ref()
            && let Ok(mut terminal) = terminal.lock()
        {
            terminal.drain_pty();
        }

        let Some(outcome) = (|| {
            let gpu = self.runtime.gpu.as_ref()?;
            let adapter = self.runtime.winit_adapter.as_mut()?;
            let widget_runtime = self.runtime.widget_runtime.as_mut()?;
            let scale = window.scale_factor() as f32;
            let (physical_width, physical_height) = gpu.surface_size();
            let target = WinitFrameTarget::new(
                window,
                gpu.surface(),
                gpu.device(),
                gpu.queue(),
                gpu.surface_config(),
                harbor_widget::renderer::Viewport::new(physical_width, physical_height, scale),
                bg_wgpu(harbor_config::BACKGROUND),
            );
            Some(with_current_gpu(gpu, || {
                adapter.render(widget_runtime, target)
            }))
        })() else {
            return;
        };

        let effects = outcome.effects().clone();
        Self::apply_window_effects(window, &effects);
        if let Some(control_flow) = effects.control_flow {
            Self::apply_control_flow(event_loop, control_flow);
        }

        match outcome {
            FrameOutcome::Presented(_) => {
                self.frame.mark_first_present();
                let _ = self.frame.next_steady_state_deadline();
                self.frame.surface_recovery_attempted = false;
            }
            FrameOutcome::PresentedSuboptimal(_) => {
                tracing::warn!("surface texture suboptimal; presenting then reconfiguring");
                self.frame.mark_first_present();
                let _ = self.frame.next_steady_state_deadline();
                if !self.frame.surface_recovery_attempted {
                    self.frame.surface_recovery_attempted = true;
                    if let Some(gpu) = self.runtime.gpu.as_mut() {
                        gpu.reconfigure();
                    }
                    self.request_main_frame(event_loop);
                }
            }
            FrameOutcome::RecoveryRequired(_) => {
                tracing::warn!("surface requires reconfiguration");
                if !self.frame.surface_recovery_attempted {
                    self.frame.surface_recovery_attempted = true;
                    if let Some(gpu) = self.runtime.gpu.as_mut() {
                        gpu.reconfigure();
                    }
                    self.request_main_frame(event_loop);
                } else {
                    tracing::warn!("surface recovery deferred until external wake");
                }
            }
            FrameOutcome::Skipped(_) => {
                tracing::debug!("surface frame skipped");
            }
            FrameOutcome::Fatal(error, _) => {
                tracing::error!(?error, "fatal main-window frame error");
                event_loop.exit();
            }
        }
    }

    /// Initializes the widget runtime with a terminal CustomPaint root.
    ///
    /// The returned effects are produced during the bootstrap focus transition
    /// and must be applied after the native window is available.
    fn init_widget_runtime(&mut self) -> RuntimeEffects {
        use harbor_widget::widgets::custom_paint::CustomPaint;

        let terminal_arc = self.runtime.terminal.as_ref().unwrap().clone();
        let draw_id = terminal_arc.lock().unwrap().draw_id();

        let handler: Arc<harbor_widget::scene::primitive::ExternalDrawFn<'static>> =
            Arc::new(move |id, rect, pass| {
                current_gpu(|gpu| {
                    if let Ok(mut term) = terminal_arc.lock() {
                        term.render(id, rect, pass, gpu);
                    }
                });
            });

        let custom_paint = CustomPaint::new(draw_id).handler(handler);
        let gpu = self.runtime.gpu.as_ref().unwrap();
        let mut runtime = harbor_widget::runtime::Runtime::new();
        runtime.set_root(custom_paint);
        runtime.init_renderer(gpu.device(), gpu.format());
        let mut initial_effects = runtime.update(Instant::now());
        runtime.focus_first_focusable();

        // Bootstrap focus is a local widget-state transition, not a native
        // focus event. Consume its deferred CustomPaint notification now so
        // the first native window event cannot replay it.
        runtime.drain_external_input();
        initial_effects.merge(runtime.take_pending_effects());

        self.runtime.widget_runtime = Some(runtime);
        initial_effects
    }
}

/// Converts `[f32;4]` from `harbor_config` to `wgpu::Color`.
fn bg_wgpu(c: [f32; 4]) -> wgpu::Color {
    wgpu::Color {
        r: c[0] as f64,
        g: c[1] as f64,
        b: c[2] as f64,
        a: c[3] as f64,
    }
}

/// Paints the terminal background color into the window using GDI, before the
/// wgpu surface is ready.
#[cfg(target_os = "windows")]
fn paint_gdi_background(window: &Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    unsafe extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
        fn CreateSolidBrush(color: u32) -> isize;
        fn FillRect(hdc: isize, rect: *const Rect, brush: isize) -> i32;
        fn DeleteObject(obj: isize) -> i32;
    }

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };

    let hwnd = h.hwnd.get();
    let size = window.inner_size();
    let color: u32 = 162 | (124 << 8) | (80 << 16);
    let rect = Rect {
        left: 0,
        top: 0,
        right: size.width as i32,
        bottom: size.height as i32,
    };

    unsafe {
        let hdc = GetDC(hwnd);
        if hdc != 0 {
            let brush = CreateSolidBrush(color);
            FillRect(hdc, &rect, brush);
            ReleaseDC(hwnd, hdc);
            DeleteObject(brush);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-only coverage for the feature-gated Host contract. The fixture is
    // intentionally never called: its parameters are borrowed by the caller,
    // so no Window, Surface, Device, or Queue is created by the test suite.
    #[allow(dead_code)]
    fn winit_runtime_contract_fixture<'frame, 'surface>(
        window: &'frame Window,
        surface: &'frame wgpu::Surface<'surface>,
        device: &'frame wgpu::Device,
        queue: &'frame wgpu::Queue,
        config: &'frame wgpu::SurfaceConfiguration,
    ) {
        use harbor_widget::{
            renderer::Viewport,
            runtime::Runtime,
            winit::{FrameOutcome, WinitAdapter, WinitEventOutcome, WinitFrameTarget},
        };

        let _: fn(&mut WinitAdapter, &mut Runtime, &WindowEvent) -> WinitEventOutcome =
            WinitAdapter::handle_event;
        let mut runtime = Runtime::new();
        let mut adapter = WinitAdapter::new();
        let target = WinitFrameTarget::new(
            window,
            surface,
            device,
            queue,
            config,
            Viewport::new(1, 1, 1.0),
            wgpu::Color::BLACK,
        );
        let outcome: FrameOutcome = adapter.render(&mut runtime, target);
        let _ = outcome;
    }

    fn empty_frame() -> FrameState {
        FrameState {
            surface_recovery_attempted: false,
            first_present_at: None,
            steady_state_emitted: false,
        }
    }

    /// Captures `harbor.font.lifecycle` field maps for behavior assertions.
    #[derive(Clone, Default)]
    struct LifecycleCapture {
        events: Arc<Mutex<Vec<std::collections::HashMap<String, String>>>>,
    }

    impl LifecycleCapture {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn phases(&self) -> Vec<String> {
            self.events
                .lock()
                .expect("lifecycle capture lock")
                .iter()
                .filter_map(|fields| fields.get("phase").cloned())
                .collect()
        }
    }

    struct FieldRecorder<'a> {
        fields: &'a mut std::collections::HashMap<String, String>,
    }

    impl tracing::field::Visit for FieldRecorder<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    impl<S> tracing_subscriber::layer::Layer<S> for LifecycleCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if event.metadata().target() != FONT_LIFECYCLE_TARGET {
                return;
            }
            let mut fields = std::collections::HashMap::new();
            event.record(&mut FieldRecorder {
                fields: &mut fields,
            });
            self.events
                .lock()
                .expect("lifecycle capture lock")
                .push(fields);
        }
    }

    fn with_lifecycle_capture<R>(f: impl FnOnce() -> R) -> (R, LifecycleCapture) {
        use tracing_subscriber::layer::SubscriberExt;
        let capture = LifecycleCapture::new();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let result = tracing::subscriber::with_default(subscriber, f);
        (result, capture)
    }

    #[derive(Clone, Default)]
    struct ClipboardCapture {
        events: Arc<Mutex<Vec<std::collections::HashMap<String, String>>>>,
    }

    impl<S> tracing_subscriber::layer::Layer<S> for ClipboardCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if !event.metadata().target().contains("app") {
                return;
            }
            let mut fields = std::collections::HashMap::new();
            event.record(&mut FieldRecorder {
                fields: &mut fields,
            });
            self.events
                .lock()
                .expect("clipboard capture lock")
                .push(fields);
        }
    }

    fn with_clipboard_capture<R>(f: impl FnOnce() -> R) -> (R, ClipboardCapture) {
        use tracing_subscriber::layer::SubscriberExt;
        let capture = ClipboardCapture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let result = tracing::subscriber::with_default(subscriber, f);
        (result, capture)
    }

    #[test]
    fn should_emit_first_present_once_when_marked() {
        // Arrange
        let mut frame = empty_frame();
        let at = Instant::now();

        // Act
        let (_, capture) = with_lifecycle_capture(|| {
            frame.mark_first_present_at(at);
            frame.mark_first_present_at(at + Duration::from_millis(1));
        });

        // Assert
        assert_eq!(capture.phases(), vec!["first_present".to_string()]);
    }

    #[test]
    fn should_not_emit_steady_state_when_first_present_missing() {
        // Arrange
        let mut frame = empty_frame();

        // Act
        let (_, capture) = with_lifecycle_capture(|| {
            frame.maybe_emit_steady_state_at(Instant::now());
        });

        // Assert
        assert!(capture.phases().is_empty());
    }

    #[test]
    fn should_not_emit_steady_state_when_dwell_incomplete() {
        // Arrange
        let mut frame = empty_frame();
        let presented_at = Instant::now();
        let before_dwell = presented_at + FONT_STEADY_STATE_DWELL - Duration::from_millis(1);

        // Act
        let (_, capture) = with_lifecycle_capture(|| {
            frame.mark_first_present_at(presented_at);
            frame.maybe_emit_steady_state_at(before_dwell);
        });

        // Assert
        assert_eq!(capture.phases(), vec!["first_present".to_string()]);
    }

    #[test]
    fn should_emit_steady_state_once_when_dwell_elapsed() {
        // Arrange
        let mut frame = empty_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);

        // Act
        let (_, capture) = with_lifecycle_capture(|| {
            frame.mark_first_present_at(presented_at);
            frame.maybe_emit_steady_state_at(after_dwell);
            frame.maybe_emit_steady_state_at(after_dwell + Duration::from_secs(1));
        });

        // Assert
        assert_eq!(
            capture.phases(),
            vec!["first_present".to_string(), "steady_state".to_string()]
        );
    }

    #[test]
    fn should_return_future_deadline_when_first_present_marked() {
        // Arrange
        let mut frame = empty_frame();
        let presented_at = Instant::now();
        let expected_deadline = presented_at + FONT_STEADY_STATE_DWELL;

        // Act
        frame.mark_first_present_at(presented_at);
        let deadline = frame.next_steady_state_deadline_at(presented_at);

        // Assert
        assert_eq!(deadline, Some(expected_deadline));
    }

    #[test]
    fn should_return_no_deadline_when_first_present_missing() {
        // Arrange
        let mut frame = empty_frame();

        // Act
        let deadline = frame.next_steady_state_deadline_at(Instant::now());

        // Assert
        assert_eq!(deadline, None);
    }

    #[test]
    fn should_emit_steady_state_without_deadline_when_dwell_already_elapsed() {
        // Arrange
        let mut frame = empty_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);

        // Act
        let (deadline, capture) = with_lifecycle_capture(|| {
            frame.mark_first_present_at(presented_at);
            frame.next_steady_state_deadline_at(after_dwell)
        });

        // Assert
        assert_eq!(
            capture.phases(),
            vec!["first_present".to_string(), "steady_state".to_string()]
        );
        assert_eq!(deadline, None);
    }

    #[test]
    fn should_return_no_deadline_when_steady_state_already_emitted() {
        // Arrange
        let mut frame = empty_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);
        frame.mark_first_present_at(presented_at);
        frame.maybe_emit_steady_state_at(after_dwell);

        // Act
        let deadline = frame.next_steady_state_deadline_at(after_dwell);

        // Assert
        assert_eq!(deadline, None);
    }

    #[test]
    fn external_input_requires_matching_draw_id_and_allows_wheel_when_gated() {
        use harbor_widget::input::event::{
            Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
            UiEvent,
        };
        use harbor_widget::layout::Point;

        let key = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Enter,
            modifiers: Modifiers::default(),
        });
        let wheel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            PointerButton::Left,
            0,
        ));

        assert!(routes_terminal_input(false, 7, 7, &key));
        assert!(!routes_terminal_input(false, 6, 7, &key));
        assert!(!routes_terminal_input(true, 7, 7, &key));
        assert!(routes_terminal_input(true, 7, 7, &wheel));
        assert!(!routes_terminal_input(true, 6, 7, &wheel));
    }

    #[test]
    fn should_reject_non_wheel_pointer_when_gate_active() {
        use harbor_widget::input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent};
        use harbor_widget::layout::Point;

        // Arrange
        let move_event = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ));
        let wheel_pixel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelPixel { dx: 0.0, dy: 40.0 },
            PointerButton::Left,
            0,
        ));

        // Act / Assert
        assert!(!routes_terminal_input(true, 7, 7, &move_event));
        assert!(routes_terminal_input(true, 7, 7, &wheel_pixel));
        assert!(routes_terminal_input(false, 7, 7, &move_event));
    }

    #[test]
    fn routes_keyboard_when_gate_open_and_wheel_when_gated() {
        use harbor_widget::input::event::{
            Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
            UiEvent,
        };
        use harbor_widget::layout::Point;

        let matching = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Enter,
            modifiers: Modifiers::default(),
        });
        let other = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Escape,
            modifiers: Modifiers::default(),
        });
        let wheel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelPixel { dx: 0.0, dy: 40.0 },
            PointerButton::Left,
            0,
        ));
        let mut routed = Vec::new();

        route_terminal_inputs(false, 7, [(6, other), (7, matching.clone())], |event| {
            routed.push(event)
        });
        route_terminal_inputs(true, 7, [(7, matching.clone())], |event| routed.push(event));
        route_terminal_inputs(true, 7, [(7, wheel.clone())], |event| routed.push(event));

        assert_eq!(routed, vec![matching, wheel]);
    }

    #[test]
    fn terminal_output_event_maps_only_to_generic_external_invalidation() {
        assert_eq!(
            external_invalidation_for_app_event(AppEvent::TerminalOutputReady),
            Some(ExternalInvalidation::new())
        );
    }

    #[test]
    fn terminal_keypress_wake_classification_excludes_ime_commits_and_other_events() {
        use harbor_widget::input::event::{
            Key as WidgetKey, Modifiers, PointerButton, PointerEvent, PointerPhase,
        };
        use harbor_widget::layout::Point;

        assert!(is_terminal_key_press(&UiEvent::Keyboard(
            KeyboardEvent::KeyDown {
                key: WidgetKey::Enter,
                modifiers: Modifiers::default(),
            }
        )));
        assert!(!is_terminal_key_press(&UiEvent::Keyboard(
            KeyboardEvent::Ime("text".into())
        )));
        assert!(!is_terminal_key_press(&UiEvent::Keyboard(
            KeyboardEvent::KeyUp {
                key: WidgetKey::Enter,
                modifiers: Modifiers::default(),
            }
        )));

        let wheel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            PointerButton::Left,
            0,
        ));
        assert!(is_terminal_wheel(&wheel));
        assert!(!wakes_redraw_for_routed_input(&wheel));
        assert!(!is_terminal_key_press(&wheel));
    }

    #[test]
    fn should_wake_redraw_for_keydown_only_wheel_needs_viewport_change() {
        use harbor_widget::input::event::{
            Key as WidgetKey, Modifiers, PointerButton, PointerEvent, PointerPhase,
        };
        use harbor_widget::layout::Point;

        // Arrange
        let key_down = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Enter,
            modifiers: Modifiers::default(),
        });
        let key_up = UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: WidgetKey::Enter,
            modifiers: Modifiers::default(),
        });
        let wheel_line = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            PointerButton::Left,
            0,
        ));
        let wheel_pixel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelPixel { dx: 0.0, dy: 40.0 },
            PointerButton::Left,
            0,
        ));
        let move_event = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ));

        // Act / Assert — classification helpers only; wheel redraw depends on
        // view_offset changing at the delivery site.
        assert!(wakes_redraw_for_routed_input(&key_down));
        assert!(!wakes_redraw_for_routed_input(&key_up));
        assert!(!wakes_redraw_for_routed_input(&wheel_line));
        assert!(!wakes_redraw_for_routed_input(&wheel_pixel));
        assert!(is_terminal_wheel(&wheel_line));
        assert!(is_terminal_wheel(&wheel_pixel));
        assert!(!wakes_redraw_for_routed_input(&move_event));
        assert!(!is_terminal_wheel(&move_event));
    }

    #[test]
    fn should_wake_after_delivery_when_keydown_regardless_of_offset() {
        // Arrange / Act / Assert
        assert!(needs_redraw_wake_after_delivery(true, None, 0));
        assert!(needs_redraw_wake_after_delivery(true, Some(3), 3));
        assert!(needs_redraw_wake_after_delivery(true, Some(0), 5));
    }

    #[test]
    fn should_wake_after_delivery_when_wheel_moves_viewport() {
        // Arrange / Act / Assert
        assert!(needs_redraw_wake_after_delivery(false, Some(0), 3));
        assert!(needs_redraw_wake_after_delivery(false, Some(6), 3));
    }

    #[test]
    fn should_not_wake_after_delivery_when_wheel_leaves_viewport_unchanged() {
        // Arrange / Act / Assert — zero-delta, clamp, or alt-screen: offset same
        assert!(!needs_redraw_wake_after_delivery(false, Some(0), 0));
        assert!(!needs_redraw_wake_after_delivery(false, Some(12), 12));
        assert!(!needs_redraw_wake_after_delivery(false, None, 5));
    }

    #[test]
    fn control_flow_arbitration_prefers_poll_then_earliest_deadline() {
        // Arrange
        let now = Instant::now();
        let early = now + Duration::from_secs(1);
        let late = now + Duration::from_secs(2);

        // Act and assert each arbitration combination independently.
        assert_eq!(
            ControlFlowEffect::Wait.arbitrate(ControlFlowEffect::Wait),
            ControlFlowEffect::Wait
        );
        assert_eq!(
            ControlFlowEffect::WaitUntil(late).arbitrate(ControlFlowEffect::WaitUntil(early)),
            ControlFlowEffect::WaitUntil(early)
        );
        assert_eq!(
            ControlFlowEffect::WaitUntil(early).arbitrate(ControlFlowEffect::Wait),
            ControlFlowEffect::WaitUntil(early)
        );
        assert_eq!(
            ControlFlowEffect::Poll.arbitrate(ControlFlowEffect::WaitUntil(late)),
            ControlFlowEffect::Poll
        );
        assert_eq!(
            ControlFlowEffect::WaitUntil(late).arbitrate(ControlFlowEffect::Poll),
            ControlFlowEffect::Poll
        );
    }

    #[test]
    fn clipboard_log_metadata_excludes_write_payload() {
        let secret = "private clipboard contents";
        assert_eq!(
            clipboard_log_metadata(&ClipboardEffect::write(secret)),
            ("write", secret.len())
        );
        assert_eq!(clipboard_log_metadata(&ClipboardEffect::Read), ("read", 0));
    }

    #[test]
    fn clipboard_effects_are_explicitly_deferred_without_a_discarding_read() {
        assert_eq!(
            apply_clipboard_effect(ClipboardEffect::Read),
            ClipboardHostAction::Deferred(ClipboardEffect::Read)
        );
        assert_eq!(
            apply_clipboard_effect(ClipboardEffect::write("copied")),
            ClipboardHostAction::Deferred(ClipboardEffect::write("copied"))
        );
    }

    #[test]
    fn clipboard_warning_logs_metadata_without_write_payload() {
        // Arrange: use a value that would make accidental payload logging obvious.
        let secret = "clipboard-secret-not-for-logs";

        // Act: defer the effect under a scoped tracing subscriber.
        let (action, capture) =
            with_clipboard_capture(|| apply_clipboard_effect(ClipboardEffect::write(secret)));

        // Assert: the host action remains deferred and the warning exposes only
        // operation metadata, never the clipboard contents.
        assert_eq!(
            action,
            ClipboardHostAction::Deferred(ClipboardEffect::write(secret))
        );
        let events = capture.events.lock().expect("clipboard events lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].get("operation"), Some(&"write".to_string()));
        assert_eq!(events[0].get("byte_len"), Some(&secret.len().to_string()));
        assert!(!format!("{events:?}").contains(secret));
    }

    #[test]
    fn runtime_effect_host_mapping_is_pure_and_complete_without_native_handles() {
        let deadline = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::Wait),
            ControlFlow::Wait
        );
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::WaitUntil(deadline)),
            ControlFlow::WaitUntil(deadline)
        );
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::Poll),
            ControlFlow::Poll
        );

        let cursor_mappings = [
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
        for (effect, expected) in cursor_mappings {
            assert_eq!(cursor_icon_for_effect(effect), expected);
        }
        assert_eq!(
            ime_cursor_area(Point::new(12.5, 8.0)),
            (LogicalPosition::new(12.5, 8.0), LogicalSize::new(1.0, 1.0))
        );

        let effects = RuntimeEffects {
            request_redraw: true,
            control_flow: Some(ControlFlowEffect::Poll),
            cursor: Some(CursorEffect::Set(CursorShape::Text)),
            ime: Some(ImeEffect::set_allowed(true)),
            clipboard: Some(ClipboardEffect::write("copied")),
        };
        assert!(effects.request_redraw);
        assert_eq!(effects.control_flow, Some(ControlFlowEffect::Poll));
        assert_eq!(effects.cursor, Some(CursorEffect::Set(CursorShape::Text)));
        assert_eq!(effects.ime, Some(ImeEffect::set_allowed(true)));
        assert_eq!(effects.clipboard, Some(ClipboardEffect::write("copied")));
    }
}
