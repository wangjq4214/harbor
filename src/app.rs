//! Application shell: winit lifecycle, window bootstrap, frame render.

mod confirmation;
mod terminal_decoration_preset;
mod window_backdrop;

use std::{
    cell::Cell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState},
    window::{CursorIcon, Theme, Window, WindowId},
};

use crate::event::AppEvent;
use confirmation::ConfirmationWindow;
use harbor_pty::PtyEndpoints;
use harbor_terminal::{
    GpuContext, InputModes, PasteDisposition, Terminal, TerminalAppearance, TextMetrics,
    alpha_mode_supports_transparency, load_system_fonts,
};
use harbor_widget::effects::{
    ClipboardEffect, ControlFlowEffect, CursorEffect, CursorShape, ExternalInvalidation, ImeEffect,
    RuntimeEffects,
};
use harbor_widget::layout::Point;
use harbor_widget::text::GlyphFn;
use harbor_widget::winit::{FrameError, FrameOutcome, WinitAdapter, WinitFrameTarget};
use terminal_decoration_preset::build_main_terminal_root;
use window_backdrop::{
    BackdropStatus, WindowBackdropBackend, os_build, select_backend, wasdk_available,
};

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

/// Encodes the unchanged confirmation text with the modes current at the
/// moment of confirmation, then performs exactly one Host-owned PTY write.
fn write_confirmation_outcome<E>(
    outcome: &DialogOutcome,
    input_modes: InputModes,
    write: impl FnOnce(&[u8]) -> Result<(), E>,
) -> Result<bool, E> {
    let DialogOutcome::Confirmed(raw_text) = outcome else {
        return Ok(false);
    };
    let bytes = input_modes.paste(raw_text.as_bytes());
    write(bytes.as_ref())?;
    Ok(true)
}

fn is_paste_shortcut(event: &WindowEvent, modifiers: ModifiersState) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { event, .. }
            if event.state == ElementState::Pressed
                && modifiers.control_key()
                && !modifiers.alt_key()
                && !modifiers.super_key()
                && matches!(&event.logical_key, Key::Character(character) if character.eq_ignore_ascii_case("v"))
    )
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

enum DialogOutcome {
    None,
    Cancelled,
    Confirmed(String),
    Fatal(FrameError),
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
        glyph_fn: Option<&GlyphFn>,
    ) -> DialogOutcome {
        let Some(mut confirmation) = self.window.take() else {
            return DialogOutcome::None;
        };
        match confirmation.handle_event(event, event_loop) {
            confirmation::ConfirmationResult::Cancelled => DialogOutcome::Cancelled,
            confirmation::ConfirmationResult::Confirmed => {
                let raw_text = confirmation.raw_text().to_owned();
                DialogOutcome::Confirmed(raw_text)
            }
            confirmation::ConfirmationResult::None => {
                if matches!(event, WindowEvent::RedrawRequested)
                    && let (Some(gpu), Some(glyph_fn)) = (gpu, glyph_fn)
                {
                    let frame = confirmation.render(gpu.device(), gpu.queue(), glyph_fn);
                    confirmation.apply_frame_effects(&frame, event_loop);
                    if let Some(error) = frame.fatal_error().cloned() {
                        DialogOutcome::Fatal(error)
                    } else {
                        self.window = Some(confirmation);
                        DialogOutcome::None
                    }
                } else {
                    self.window = Some(confirmation);
                    DialogOutcome::None
                }
            }
        }
    }

    /// Installs a new confirmation dialog, replacing any existing one.
    fn open(&mut self, confirmation: ConfirmationWindow) {
        self.window = Some(confirmation);
    }
}

/// Runtime resources that exist while the window is alive.
struct AppRuntime {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    terminal: Option<Arc<Mutex<Terminal>>>,
    /// Host-owned gate mirrored into the terminal bridge for in-tree input suppression.
    input_gate: Arc<AtomicBool>,
    /// Widget framework runtime.
    widget_runtime: Option<harbor_widget::runtime::Runtime>,
    /// Main-window input adapter, sharing the runtime's window lifecycle.
    winit_adapter: Option<WinitAdapter>,
    /// Selected window backdrop backend, held for the window lifetime.
    backdrop: Option<Box<dyn WindowBackdropBackend>>,
    /// Host fact injected into the Widget presenter for terminal clear policy.
    backdrop_available: bool,
    /// Keeps a newly-created window hidden until its first frame is presented.
    show_pending: bool,
    /// Delays retries after a skipped hidden-startup acquisition.
    startup_retry_deadline: Option<Instant>,
    dialog: DialogOverlay,
}

/// Documented 5s dwell after first present for the `steady_state` lifecycle marker.
const FONT_STEADY_STATE_DWELL: Duration = Duration::from_secs(5);
/// Avoids an immediate redraw loop while a hidden startup surface is skipped.
const HIDDEN_STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameLifecycleEvent {
    FirstPresent,
    SteadyState { dwell_ms: u64 },
}

trait FrameLifecycleSink {
    fn emit(&self, event: FrameLifecycleEvent);
}

struct TracingFrameLifecycleSink;

impl FrameLifecycleSink for TracingFrameLifecycleSink {
    fn emit(&self, event: FrameLifecycleEvent) {
        match event {
            FrameLifecycleEvent::FirstPresent => tracing::info!(
                target: "harbor.font.lifecycle",
                phase = "first_present",
                "font lifecycle"
            ),
            FrameLifecycleEvent::SteadyState { dwell_ms } => tracing::info!(
                target: "harbor.font.lifecycle",
                phase = "steady_state",
                dwell_ms,
                "font lifecycle"
            ),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct RecordingFrameLifecycleSink {
    events: std::cell::RefCell<Vec<FrameLifecycleEvent>>,
}

#[cfg(test)]
impl RecordingFrameLifecycleSink {
    fn events(&self) -> Vec<FrameLifecycleEvent> {
        self.events.borrow().clone()
    }
}

#[cfg(test)]
impl FrameLifecycleSink for RecordingFrameLifecycleSink {
    fn emit(&self, event: FrameLifecycleEvent) {
        self.events.borrow_mut().push(event);
    }
}

/// Host frame lifecycle telemetry.
struct FrameState {
    /// Set after the first successful surface present.
    first_present_at: Option<Instant>,
    /// Once-only gate for the steady-state dwell marker.
    steady_state_emitted: bool,
    lifecycle: std::rc::Rc<dyn FrameLifecycleSink>,
}

impl FrameState {
    fn new(lifecycle: std::rc::Rc<dyn FrameLifecycleSink>) -> Self {
        Self {
            first_present_at: None,
            steady_state_emitted: false,
            lifecycle,
        }
    }

    /// Records the first successful present and emits `first_present` once.
    fn mark_first_present(&mut self) {
        self.mark_first_present_at(Instant::now());
    }

    fn mark_first_present_at(&mut self, at: Instant) {
        if self.first_present_at.is_some() {
            return;
        }
        self.first_present_at = Some(at);
        self.lifecycle.emit(FrameLifecycleEvent::FirstPresent);
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
        self.lifecycle.emit(FrameLifecycleEvent::SteadyState {
            dwell_ms: dwell.as_millis() as u64,
        });
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

        if let Some(deadline) = self.runtime.startup_retry_deadline {
            if now >= deadline {
                self.runtime.startup_retry_deadline = None;
                self.request_main_frame(event_loop);
            } else {
                combined_flow = combined_flow.arbitrate(ControlFlowEffect::WaitUntil(deadline));
            }
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
            let result = {
                let term_guard = self.runtime.terminal.as_ref().map(|t| t.lock().unwrap());
                let dialog = &mut self.runtime.dialog;
                let gpu = self.runtime.gpu.as_ref();
                match term_guard.as_ref() {
                    Some(terminal) => {
                        let glyph_fn = |ch| terminal.text_glyph(ch).copied();
                        dialog.handle_event(&event, event_loop, gpu, Some(&glyph_fn))
                    }
                    None => dialog.handle_event(&event, event_loop, gpu, None),
                }
            };
            match &result {
                DialogOutcome::Cancelled => {
                    self.runtime.input_gate.store(false, Ordering::Release);
                    self.request_main_frame(event_loop);
                    return;
                }
                DialogOutcome::Confirmed(_) => {
                    if let Some(terminal) = self.runtime.terminal.as_ref()
                        && let Ok(mut terminal) = terminal.lock()
                    {
                        let input_modes = terminal.drain_and_snapshot().input_modes;
                        if let Err(error) =
                            write_confirmation_outcome(&result, input_modes, |bytes| {
                                terminal.write_pty(bytes)
                            })
                        {
                            tracing::warn!(error = %format_args!("{error:#}"), "failed to write confirmed paste");
                        }
                    }
                    self.runtime.input_gate.store(false, Ordering::Release);
                    self.request_main_frame(event_loop);
                    return;
                }
                DialogOutcome::Fatal(error) => {
                    tracing::error!(?error, "fatal confirmation-window frame error");
                    event_loop.exit();
                    return;
                }
                DialogOutcome::None => {}
            }
        }
        let gate_active = self.runtime.dialog.is_active();
        self.runtime
            .input_gate
            .store(gate_active, Ordering::Release);

        let (Some(_gpu), Some(_terminal), Some(window)) = (
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

        if self
            .runtime
            .winit_adapter
            .as_ref()
            .is_some_and(|adapter| is_paste_shortcut(&event, adapter.modifiers()))
        {
            self.paste_from_clipboard(event_loop);
            return;
        }

        let outcome = match (
            self.runtime.winit_adapter.as_mut(),
            self.runtime.widget_runtime.as_mut(),
        ) {
            (Some(adapter), Some(widget_runtime)) => {
                let size = window.inner_size();
                Some(adapter.handle_event_with_size(
                    widget_runtime,
                    &event,
                    Some((size.width, size.height)),
                ))
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
        }

        if let WindowEvent::RedrawRequested = event {
            tracing::trace!("redraw requested");
            self.render_frame(event_loop);
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
                input_gate: Arc::new(AtomicBool::new(false)),
                widget_runtime: None,
                winit_adapter: None,
                backdrop: None,
                backdrop_available: false,
                show_pending: false,
                startup_retry_deadline: None,
                dialog: DialogOverlay { window: None },
            },
            frame: FrameState::new(std::rc::Rc::new(TracingFrameLifecycleSink)),
            event_proxy,
        }
    }

    /// Creates the main window, GPU context, font atlas, and terminal engine.
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.runtime.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let appearance = TerminalAppearance::default();
        let backdrop = select_backend(os_build(), wasdk_available());
        let mut window_attrs = Window::default_attributes()
            .with_title("Harbor")
            .with_theme(Some(Theme::Dark))
            .with_visible(false);
        window_attrs = backdrop.configure_attributes(window_attrs);

        let window = Arc::new(event_loop.create_window(window_attrs)?);
        // Winit 0.30 emits composition commits only after IME is explicitly enabled.
        window.set_ime_allowed(true);

        #[cfg(target_os = "windows")]
        suppress_caption_title_and_icon(&window);
        let backdrop_style = harbor_config::WindowBackdropStyle::default();
        let BackdropStatus {
            tier,
            backdrop_available: backdrop_applied,
        } = backdrop.apply(&window, &backdrop_style);
        self.runtime.backdrop = Some(backdrop);

        let gpu =
            pollster::block_on(GpuContext::new(window.clone())).map_err(AppError::Renderer)?;
        let main_window_backdrop_available =
            backdrop_applied && alpha_mode_supports_transparency(gpu.alpha_mode());
        #[cfg(target_os = "windows")]
        if !main_window_backdrop_available {
            paint_gdi_background(&window, backdrop_style.fallback);
        }
        let initial_size = window.inner_size();

        tracing::info!(
            backdrop_available = main_window_backdrop_available,
            tier = ?tier,
            alpha_mode = ?gpu.alpha_mode(),
            "main window backdrop selected"
        );

        // Create DirectWrite objects on the UI/render owning thread (no font-loader thread).
        let fonts = load_system_fonts().map_err(AppError::Renderer)?;
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());

        let size = Terminal::terminal_size_for(&gpu, &metrics);
        let (pty_read, pty_write, pty_control) = PtyEndpoints::spawn_shell(size)
            .map_err(AppError::Pty)?
            .into_parts();
        let event_proxy = self.event_proxy.clone();
        let mut terminal = Terminal::new_with_appearance(
            size,
            pty_read,
            pty_write,
            pty_control,
            &gpu,
            fonts,
            metrics,
            appearance,
            move || {
                event_proxy
                    .send_event(AppEvent::TerminalOutputReady)
                    .is_ok()
            },
        );
        terminal.set_backdrop_available(main_window_backdrop_available);
        // Terminal is UI-thread-only (not Send/Sync); Arc is required so the
        // CustomPaint ExternalDrawFn can share ownership with AppRuntime.
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(terminal));

        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
        self.runtime.gpu = Some(gpu);
        self.runtime.terminal = Some(terminal);
        self.runtime.backdrop_available = main_window_backdrop_available;
        let mut winit_adapter = WinitAdapter::from_window(&window);
        winit_adapter.set_drawable(initial_size.width != 0 && initial_size.height != 0);
        self.runtime.winit_adapter = Some(winit_adapter);
        self.runtime.window = Some(window.clone());
        self.runtime.show_pending = true;
        let initial_effects = self.init_widget_runtime();
        if let Some(adapter) = self.runtime.winit_adapter.as_mut() {
            let mut effects = adapter.fold_effects(initial_effects);
            effects.merge(adapter.request_frame());
            Self::apply_window_effects(&window, &effects);
            if let Some(control_flow) = effects.control_flow {
                Self::apply_control_flow(event_loop, control_flow);
            }
        }
        let _ = self.render_frame(event_loop);
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
        if let Some(clipboard) = effects.clipboard.clone() {
            match clipboard {
                ClipboardEffect::Write(contents) => {
                    if let Err(error) = arboard::Clipboard::new()
                        .and_then(|mut clipboard| clipboard.set_text(contents))
                    {
                        tracing::warn!(error = %error, "failed to write clipboard effect");
                    }
                }
                ClipboardEffect::Read => {
                    let _ = apply_clipboard_effect(ClipboardEffect::Read);
                }
            }
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

    /// Reads the clipboard and either writes a safe direct paste or opens the
    /// native confirmation window for a multiline paste requiring confirmation.
    fn paste_from_clipboard(&mut self, event_loop: &ActiveEventLoop) {
        let raw_text =
            match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                Ok(text) => text,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to read clipboard text");
                    return;
                }
            };

        let confirmation = {
            let (Some(gpu), Some(main_window), Some(terminal)) = (
                self.runtime.gpu.as_ref(),
                self.runtime.window.as_deref(),
                self.runtime.terminal.as_ref(),
            ) else {
                return;
            };
            let Ok(mut terminal) = terminal.lock() else {
                tracing::warn!("terminal lock unavailable for clipboard paste");
                return;
            };
            let input_modes = terminal.drain_and_snapshot().input_modes;

            match PasteDisposition::decide(input_modes, &raw_text) {
                PasteDisposition::SendDirect => {
                    if let Err(error) =
                        terminal.write_pty(input_modes.paste(raw_text.as_bytes()).as_ref())
                    {
                        tracing::warn!(error = %format_args!("{error:#}"), "failed to write clipboard paste");
                    }
                    return;
                }
                PasteDisposition::Confirm { raw_text } => {
                    terminal.ensure_glyphs(&raw_text, gpu);
                    let (Some(metrics), Some(text_bind_group_layout), Some(text_bind_group)) = (
                        terminal.text_metrics().copied(),
                        terminal.text_bind_group_layout(),
                        terminal.text_bind_group(),
                    ) else {
                        tracing::warn!(
                            "terminal text resources unavailable for paste confirmation"
                        );
                        return;
                    };
                    ConfirmationWindow::new(
                        raw_text,
                        event_loop,
                        gpu,
                        metrics,
                        text_bind_group_layout,
                        text_bind_group,
                        Some(main_window),
                    )
                }
            }
        };

        self.runtime.dialog.open(confirmation);
        self.runtime.input_gate.store(true, Ordering::Release);
    }

    /// Runs one borrowed main-window frame through the winit integration.
    fn render_frame(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(window) = self.runtime.window.as_ref() else {
            return false;
        };

        let Some(outcome) = (|| {
            let gpu = self.runtime.gpu.as_ref()?;
            let backdrop_available = self.runtime.backdrop_available;
            let adapter = self.runtime.winit_adapter.as_mut()?;
            let widget_runtime = self.runtime.widget_runtime.as_mut()?;
            // Install the thread-local GPU pointer before borrowing frame
            // resources so CustomPaint can resolve GpuContext during encode.
            // Configuration updates go through GpuContext::configure_size so
            // encode never aliases a mutable SurfaceConfiguration borrow.
            Some(with_current_gpu(gpu, || {
                let mut configure = |width, height| gpu.configure_size(width, height);
                let (surface, device, queue) = gpu.borrow_frame();
                let target = WinitFrameTarget::new(
                    window,
                    surface,
                    device,
                    queue,
                    &mut configure,
                    backdrop_available,
                    gpu.alpha_mode(),
                );
                adapter.render(widget_runtime, target)
            }))
        })() else {
            return false;
        };

        let effects = outcome.effects().clone();
        Self::apply_window_effects(window, &effects);
        if let Some(control_flow) = effects.control_flow {
            Self::apply_control_flow(event_loop, control_flow);
        }

        let presented = outcome.is_presented();
        if presented {
            self.frame.mark_first_present();
            let _ = self.frame.next_steady_state_deadline();
            if self.runtime.show_pending {
                window.set_visible(true);
                self.runtime.show_pending = false;
                self.runtime.startup_retry_deadline = None;
            }
        } else if let FrameOutcome::Fatal(error, _) = &outcome {
            tracing::error!(?error, "fatal main-window frame error");
            event_loop.exit();
        }

        // A hidden startup window must keep requesting a frame after a
        // transient timeout/occlusion; otherwise it can never reach the first
        // successful presentation that makes it visible.
        if matches!(&outcome, FrameOutcome::Skipped(_)) && self.runtime.show_pending {
            self.runtime.startup_retry_deadline = Some(Instant::now() + HIDDEN_STARTUP_RETRY_DELAY);
        }
        presented
    }

    /// Initializes the widget runtime with a terminal bridge root.
    ///
    /// The returned effects are produced during the bootstrap focus transition
    /// and must be applied after the native window is available.
    fn init_widget_runtime(&mut self) -> RuntimeEffects {
        use crate::terminal_widget_bridge::TerminalWidgetBridge;

        let terminal_arc = self.runtime.terminal.as_ref().unwrap().clone();
        let bridge = TerminalWidgetBridge::new(terminal_arc, Arc::clone(&self.runtime.input_gate));

        let gpu = self.runtime.gpu.as_ref().unwrap();
        let window = self.runtime.window.as_ref().unwrap();
        let initial_size = window.inner_size();
        let initial_viewport = harbor_widget::renderer::Viewport::new(
            initial_size.width,
            initial_size.height,
            window.scale_factor() as f32,
        );
        let mut runtime = harbor_widget::runtime::Runtime::new();
        runtime.set_root(build_main_terminal_root(
            self.runtime.backdrop_available,
            bridge,
        ));
        runtime.init_renderer(gpu.device(), gpu.format());
        runtime.set_viewport(initial_viewport);
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

/// uxtheme `WTA_NONCLIENT` attribute type for non-client theme options.
#[cfg(target_os = "windows")]
const WTA_NONCLIENT: u32 = 1;
/// Do not draw caption text in the title bar.
#[cfg(target_os = "windows")]
const WTNCA_NODRAWCAPTION: u32 = 0x1;
/// Do not draw the window icon in the title bar.
#[cfg(target_os = "windows")]
const WTNCA_NODRAWICON: u32 = 0x2;
/// Replaces the small title-bar icon for an HWND.
#[cfg(target_os = "windows")]
const WM_SETICON: u32 = 0x0080;
/// The small icon, which Windows uses in the title bar.
#[cfg(target_os = "windows")]
const ICON_SMALL: usize = 0;

/// Returns a process-lifetime transparent icon for the native caption.
///
/// Passing a null icon to `WM_SETICON` is insufficient because Windows can
/// fall back to the window-class icon. A transparent window-level icon blocks
/// that fallback without changing the big icon used by Alt-Tab and the taskbar.
#[cfg(target_os = "windows")]
fn transparent_caption_icon() -> Option<isize> {
    use std::sync::OnceLock;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn CreateIcon(
            instance: isize,
            width: i32,
            height: i32,
            planes: u8,
            bits_per_pixel: u8,
            and_bits: *const u8,
            xor_bits: *const u8,
        ) -> isize;
    }

    static ICON: OnceLock<Option<isize>> = OnceLock::new();
    *ICON.get_or_init(|| {
        // Monochrome icon scanlines are WORD-aligned. An all-one AND mask and
        // all-zero XOR mask leave the destination pixel fully unchanged.
        let and_bits = [0xff_u8; 2];
        let xor_bits = [0_u8; 2];
        let icon = unsafe { CreateIcon(0, 1, 1, 1, 1, and_bits.as_ptr(), xor_bits.as_ptr()) };
        (icon != 0).then_some(icon)
    })
}

/// Pure packing of undrawn-caption theme flags for `WTA_OPTIONS`.
///
/// Returns `(dwFlags, dwMask)` both set to `NODRAWCAPTION | NODRAWICON`.
#[cfg(target_os = "windows")]
fn caption_nodraw_flags() -> (u32, u32) {
    let flags = WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON;
    (flags, flags)
}

/// Suppresses caption text and the visible caption icon on `window`.
///
/// System min/max/close buttons stay DWM-drawn. Missing HWND or a uxtheme API
/// failure is logged and ignored so startup still proceeds.
#[cfg(target_os = "windows")]
fn suppress_caption_title_and_icon(window: &Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct WtaOptions {
        dw_flags: u32,
        dw_mask: u32,
    }

    #[link(name = "uxtheme")]
    unsafe extern "system" {
        fn SetWindowThemeAttribute(
            hwnd: isize,
            e_attribute: u32,
            pv_attribute: *const WtaOptions,
            cb_attribute: u32,
        ) -> i32;
        fn SendMessageW(hwnd: isize, message: u32, w_param: usize, l_param: isize) -> isize;
    }

    let Ok(handle) = window.window_handle() else {
        tracing::warn!("caption theme skipped: window handle unavailable");
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        tracing::warn!("caption theme skipped: non-Win32 window handle");
        return;
    };

    let (flags, mask) = caption_nodraw_flags();
    let options = WtaOptions {
        dw_flags: flags,
        dw_mask: mask,
    };
    let hwnd = h.hwnd.get();
    let hr = unsafe {
        SetWindowThemeAttribute(
            hwnd,
            WTA_NONCLIENT,
            &options,
            std::mem::size_of::<WtaOptions>() as u32,
        )
    };
    if hr < 0 {
        tracing::warn!(hr, "SetWindowThemeAttribute failed for caption nodraw");
    }

    // WTA_NONCLIENT is advisory under DWM. Use a transparent per-window small
    // icon rather than null: null permits fallback to the window-class icon.
    // Leave the big icon alone so Alt-Tab and taskbar identity are unchanged.
    if let Some(icon) = transparent_caption_icon() {
        unsafe {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL, icon);
        }
    } else {
        tracing::warn!("failed to create transparent caption icon");
    }
}

/// Paints the opaque backdrop fallback into the window using GDI, before the
/// wgpu surface is ready.
#[cfg(target_os = "windows")]
fn paint_gdi_background(window: &Window, fallback: [f32; 3]) {
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
    let to_byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    let color: u32 =
        to_byte(fallback[0]) | (to_byte(fallback[1]) << 8) | (to_byte(fallback[2]) << 16);
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

    #[cfg(target_os = "windows")]
    #[test]
    fn should_pack_nodraw_caption_and_icon_flags() {
        // Arrange / Act
        let (flags, mask) = caption_nodraw_flags();

        // Assert — both caption and icon nodraw bits are set for WTA_OPTIONS
        assert_eq!(flags, WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON);
        assert_eq!(flags & WTNCA_NODRAWCAPTION, WTNCA_NODRAWCAPTION);
        assert_eq!(flags & WTNCA_NODRAWICON, WTNCA_NODRAWICON);
        assert_eq!(mask, flags);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn should_return_exact_win32_nodraw_bitmask() {
        // Arrange — documented WTNCA values: NODRAWCAPTION=0x1, NODRAWICON=0x2
        // Act
        let (flags, mask) = caption_nodraw_flags();

        // Assert — ABI-stable packing with no extra bits
        assert_eq!(flags, 0x3);
        assert_eq!(mask, 0x3);
        assert_eq!(flags & !0x3, 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn should_return_identical_packing_on_repeated_calls() {
        // Arrange / Act
        let first = caption_nodraw_flags();
        let second = caption_nodraw_flags();

        // Assert
        assert_eq!(first, second);
    }

    // Compile-only coverage for the feature-gated Host contract. The fixture is
    // intentionally never called: its parameters are borrowed by the caller,
    // so no Window, Surface, Device, or Queue is created by the test suite.
    #[allow(dead_code)]
    fn winit_runtime_contract_fixture<'frame, 'surface>(
        window: &'frame Window,
        surface: &'frame wgpu::Surface<'surface>,
        device: &'frame wgpu::Device,
        queue: &'frame wgpu::Queue,
        configure: &'frame mut dyn FnMut(u32, u32),
    ) {
        use harbor_widget::{
            runtime::Runtime,
            winit::{FrameOutcome, WinitAdapter, WinitEventOutcome, WinitFrameTarget},
        };

        type HandleEventWithSize = fn(
            &mut WinitAdapter,
            &mut Runtime,
            &WindowEvent,
            Option<(u32, u32)>,
        ) -> WinitEventOutcome;
        let _: fn(&mut WinitAdapter, &mut Runtime, &WindowEvent) -> WinitEventOutcome =
            WinitAdapter::handle_event;
        let _: HandleEventWithSize = WinitAdapter::handle_event_with_size;
        let mut runtime = Runtime::new();
        let mut adapter = WinitAdapter::new();
        let target = WinitFrameTarget::new(
            window,
            surface,
            device,
            queue,
            configure,
            false,
            wgpu::CompositeAlphaMode::Opaque,
        );
        let outcome: FrameOutcome = adapter.render(&mut runtime, target);
        let _ = outcome;
    }

    fn empty_frame() -> FrameState {
        FrameState::new(std::rc::Rc::new(TracingFrameLifecycleSink))
    }

    #[derive(Clone)]
    struct RecordingFrameLifecycleSinkHandle(std::rc::Rc<RecordingFrameLifecycleSink>);

    impl FrameLifecycleSink for RecordingFrameLifecycleSinkHandle {
        fn emit(&self, event: FrameLifecycleEvent) {
            self.0.emit(event);
        }
    }

    fn recording_frame() -> (FrameState, std::rc::Rc<RecordingFrameLifecycleSink>) {
        let sink = std::rc::Rc::new(RecordingFrameLifecycleSink::default());
        let handle = std::rc::Rc::new(RecordingFrameLifecycleSinkHandle(sink.clone()));
        (FrameState::new(handle), sink)
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
        let (mut frame, sink) = recording_frame();
        let at = Instant::now();

        frame.mark_first_present_at(at);
        frame.mark_first_present_at(at + Duration::from_millis(1));

        assert_eq!(sink.events(), vec![FrameLifecycleEvent::FirstPresent]);
    }

    #[test]
    fn should_not_emit_steady_state_when_first_present_missing() {
        let (mut frame, sink) = recording_frame();

        frame.maybe_emit_steady_state_at(Instant::now());

        assert!(sink.events().is_empty());
    }

    #[test]
    fn should_not_emit_steady_state_when_dwell_incomplete() {
        let (mut frame, sink) = recording_frame();
        let presented_at = Instant::now();
        let before_dwell = presented_at + FONT_STEADY_STATE_DWELL - Duration::from_millis(1);

        frame.mark_first_present_at(presented_at);
        frame.maybe_emit_steady_state_at(before_dwell);

        assert_eq!(sink.events(), vec![FrameLifecycleEvent::FirstPresent]);
    }

    #[test]
    fn should_emit_steady_state_once_when_dwell_elapsed() {
        let (mut frame, sink) = recording_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);

        frame.mark_first_present_at(presented_at);
        frame.maybe_emit_steady_state_at(after_dwell);
        frame.maybe_emit_steady_state_at(after_dwell + Duration::from_secs(1));

        assert_eq!(
            sink.events(),
            vec![
                FrameLifecycleEvent::FirstPresent,
                FrameLifecycleEvent::SteadyState { dwell_ms: 5050 },
            ]
        );
    }

    #[test]
    fn should_return_future_deadline_when_first_present_marked() {
        let (mut frame, _) = recording_frame();
        let presented_at = Instant::now();
        let expected_deadline = presented_at + FONT_STEADY_STATE_DWELL;

        frame.mark_first_present_at(presented_at);
        let deadline = frame.next_steady_state_deadline_at(presented_at);

        assert_eq!(deadline, Some(expected_deadline));
    }

    #[test]
    fn should_return_no_deadline_when_first_present_missing() {
        let (mut frame, _) = recording_frame();

        let deadline = frame.next_steady_state_deadline_at(Instant::now());

        assert_eq!(deadline, None);
    }

    #[test]
    fn should_emit_steady_state_without_deadline_when_dwell_already_elapsed() {
        let (mut frame, sink) = recording_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);

        frame.mark_first_present_at(presented_at);
        let deadline = frame.next_steady_state_deadline_at(after_dwell);

        assert_eq!(
            sink.events(),
            vec![
                FrameLifecycleEvent::FirstPresent,
                FrameLifecycleEvent::SteadyState { dwell_ms: 5050 },
            ]
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
    fn terminal_output_event_maps_only_to_generic_external_invalidation() {
        assert_eq!(
            external_invalidation_for_app_event(AppEvent::TerminalOutputReady),
            Some(ExternalInvalidation::new())
        );
    }

    #[test]
    fn confirmed_paste_writes_unchanged_raw_text_with_current_modes() {
        let outcome = DialogOutcome::Confirmed("first\r\nsecond\t\x1b[A".to_owned());
        let mut writes = Vec::new();

        let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |bytes| {
            writes.push(bytes.to_vec());
            Ok::<_, ()>(())
        })
        .expect("capture writer succeeds");

        assert!(wrote);
        assert_eq!(writes, vec![b"first\r\nsecond\t\x1b[A".to_vec()]);
    }

    #[test]
    fn confirmed_paste_uses_bracketed_mode_current_at_confirmation_time() {
        let outcome = DialogOutcome::Confirmed("first\nsecond".to_owned());
        let mut writes = Vec::new();
        let modes = InputModes {
            bracketed_paste: true,
            ..InputModes::default()
        };

        let wrote = write_confirmation_outcome(&outcome, modes, |bytes| {
            writes.push(bytes.to_vec());
            Ok::<_, ()>(())
        })
        .expect("capture writer succeeds");

        assert!(wrote);
        assert_eq!(writes, vec![b"\x1b[200~first\nsecond\x1b[201~".to_vec()]);
    }

    #[test]
    fn cancelled_or_closed_confirmation_never_calls_the_writer() {
        for outcome in [DialogOutcome::Cancelled, DialogOutcome::None] {
            let wrote = write_confirmation_outcome(
                &outcome,
                InputModes::default(),
                |_| -> Result<(), ()> {
                    panic!("cancelled and native-close outcomes must not write to the PTY")
                },
            )
            .expect("no-write outcome cannot fail");
            assert!(!wrote);
        }
    }

    #[test]
    fn should_not_write_to_pty_when_confirmation_frame_is_fatal() {
        // Arrange
        let outcome = DialogOutcome::Fatal(FrameError::out_of_memory());
        let write_attempts = std::cell::Cell::new(0);

        // Act
        let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
            write_attempts.set(write_attempts.get() + 1);
            Ok::<_, ()>(())
        })
        .expect("fatal outcome cannot invoke the writer");

        // Assert
        assert!(!wrote);
        assert_eq!(write_attempts.get(), 0);
    }

    #[test]
    fn should_return_the_writer_error_after_one_confirmed_paste_attempt() {
        // Arrange
        let outcome = DialogOutcome::Confirmed("paste".to_owned());
        let write_attempts = std::cell::Cell::new(0);

        // Act
        let result = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
            write_attempts.set(write_attempts.get() + 1);
            Err::<(), _>("PTY disconnected")
        });

        // Assert
        assert_eq!(result, Err("PTY disconnected"));
        assert_eq!(write_attempts.get(), 1);
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
            ..RuntimeEffects::default()
        };
        assert!(effects.request_redraw);
        assert_eq!(effects.control_flow, Some(ControlFlowEffect::Poll));
        assert_eq!(effects.cursor, Some(CursorEffect::Set(CursorShape::Text)));
        assert_eq!(effects.ime, Some(ImeEffect::set_allowed(true)));
        assert_eq!(effects.clipboard, Some(ClipboardEffect::write("copied")));
    }
}
