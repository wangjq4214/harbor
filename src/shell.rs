//! Application shell: winit lifecycle, window bootstrap, frame render.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
#[cfg(target_os = "windows")]
use winit::platform::windows::{WindowAttributesExtWindows, WindowExtWindows};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Theme, Window, WindowId},
};

use crate::backdrop::{
    BackdropStatus, WindowBackdropBackend, os_build, select_backend, wasdk_available,
};
use crate::chrome::harbor_window_icon;
#[cfg(target_os = "windows")]
use crate::chrome::{paint_gdi_background, suppress_caption_title_and_icon};
use crate::dialog::{
    ConfirmationWindow, DialogOutcome, DialogOverlay, is_paste_shortcut, write_confirmation_outcome,
};
use crate::effects::{apply_control_flow, apply_effects, apply_window_effects};
use crate::event::{AppEvent, external_invalidation_for_app_event};
use crate::telemetry::{
    FrameState, HIDDEN_STARTUP_RETRY_DELAY, TracingFrameLifecycleSink,
};
use crate::terminal_view::{
    TerminalWidgetBridge, build_main_terminal_root, with_current_gpu,
};
use harbor_pty::{PtyEndpoints, ShellCommand};
use harbor_terminal::{
    GpuContext, PasteDisposition, Terminal, TerminalAppearance, TextMetrics,
    alpha_mode_supports_transparency, load_system_fonts,
};
use harbor_widget::effects::{ControlFlowEffect, RuntimeEffects};
use harbor_widget::winit::{FrameOutcome, WinitAdapter, WinitFrameTarget};

/// Runtime resources that exist while the window is alive.
pub(crate) struct ShellRuntime {
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

/// Winit coordinator over concrete lifecycle state groups.
pub(crate) struct Shell {
    runtime: ShellRuntime,
    frame: FrameState,
    event_proxy: EventLoopProxy<AppEvent>,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ShellError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create pty endpoints")]
    Pty(#[source] anyhow::Error),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
}

// ── ApplicationHandler (winit lifecycle) ──────────────────────────────────
impl ApplicationHandler<AppEvent> for Shell {
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
        apply_effects(window, &effects, event_loop);
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
            apply_window_effects(window, &main_effects);
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

        apply_control_flow(event_loop, combined_flow);
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
                DialogOutcome::Cancelled | DialogOutcome::Confirmed(_) => {
                    if let DialogOutcome::Confirmed(_) = &result
                        && let Some(terminal) = self.runtime.terminal.as_ref()
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

        let Some(window) = self.runtime.window.as_ref() else {
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
            apply_effects(window, &outcome.effects, event_loop);
        }

        if let WindowEvent::RedrawRequested = event {
            tracing::trace!("redraw requested");
            self.render_frame(event_loop);
        }
    }
}

// ── Shell (own methods) ───────────────────────────────────────────────────
impl Shell {
    /// Creates the application shell with no initial window, GPU, or terminal.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            runtime: ShellRuntime {
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
                dialog: DialogOverlay::new(),
            },
            frame: FrameState::new(std::rc::Rc::new(TracingFrameLifecycleSink)),
            event_proxy,
        }
    }

    /// Creates the main window, GPU context, font atlas, and terminal engine.
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), ShellError> {
        if self.runtime.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let loaded = harbor_config::load();
        for diagnostic in &loaded.diagnostics {
            match diagnostic.level {
                harbor_config::DiagnosticLevel::Warning => {
                    tracing::warn!(message = %diagnostic.message, "settings warning");
                }
                harbor_config::DiagnosticLevel::Error => {
                    tracing::error!(message = %diagnostic.message, "settings fallback");
                }
            }
        }
        let settings = loaded.settings;
        let appearance = TerminalAppearance::from_palette(settings.colors);
        let backdrop = select_backend(os_build(), wasdk_available());
        let mut window_attrs = Window::default_attributes()
            .with_title("Harbor")
            .with_inner_size(LogicalSize::new(1200.0, 600.0))
            .with_window_icon(harbor_window_icon())
            .with_theme(Some(Theme::Dark))
            .with_visible(false);
        #[cfg(target_os = "windows")]
        {
            window_attrs = window_attrs.with_taskbar_icon(harbor_window_icon());
        }
        window_attrs = backdrop.configure_attributes(window_attrs);

        let window = Arc::new(event_loop.create_window(window_attrs)?);
        // Winit 0.30 emits composition commits only after IME is explicitly enabled.
        window.set_ime_allowed(true);

        #[cfg(target_os = "windows")]
        {
            suppress_caption_title_and_icon(&window);
            // Apply these after caption theming, which may reset the HWND icon slots.
            window.set_window_icon(harbor_window_icon());
            window.set_taskbar_icon(harbor_window_icon());
        }
        let backdrop_style = harbor_config::WindowBackdropStyle::default();
        let BackdropStatus {
            tier,
            backdrop_available: backdrop_applied,
        } = backdrop.apply(&window, &backdrop_style);
        self.runtime.backdrop = Some(backdrop);

        let gpu =
            pollster::block_on(GpuContext::new(window.clone())).map_err(ShellError::Renderer)?;
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
        let fonts = load_system_fonts(&settings.font).map_err(ShellError::Renderer)?;
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());

        let size = Terminal::terminal_size_for(&gpu, &metrics);
        let shell_command = ShellCommand::new(settings.shell.program, settings.shell.args);
        let (pty_read, pty_write, pty_control) = PtyEndpoints::spawn_shell(
            harbor_pty::TerminalSize {
                rows: size.rows,
                cols: size.cols,
            },
            &shell_command,
        )
        .map_err(ShellError::Pty)?
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
        // CustomPaint ExternalDrawFn can share ownership with ShellRuntime.
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
            apply_effects(&window, &effects, event_loop);
        }
        let _ = self.render_frame(event_loop);
        Ok(())
    }

    fn request_main_frame(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(adapter), Some(window)) = (
            self.runtime.winit_adapter.as_mut(),
            self.runtime.window.as_ref(),
        ) else {
            return;
        };
        let effects = adapter.request_frame();
        apply_effects(window, &effects, event_loop);
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
        apply_effects(window, &effects, event_loop);

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

        if matches!(&outcome, FrameOutcome::Skipped(_)) && self.runtime.show_pending {
            self.runtime.startup_retry_deadline = Some(Instant::now() + HIDDEN_STARTUP_RETRY_DELAY);
        }
        presented
    }

    /// Initializes the widget runtime with a terminal bridge root.
    fn init_widget_runtime(&mut self) -> RuntimeEffects {
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

        runtime.drain_external_input();
        initial_effects.merge(runtime.take_pending_effects());

        self.runtime.widget_runtime = Some(runtime);
        initial_effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-only coverage for the feature-gated Host contract.
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
}
