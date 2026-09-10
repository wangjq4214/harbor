//! Application shell: winit lifecycle, window bootstrap, frame render.

use std::{
    sync::{Arc, atomic::AtomicBool},
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
use crate::dialog::{PasteController, PasteEventOutcome, is_paste_shortcut};
use crate::effects::{apply_control_flow, apply_effects, apply_window_effects};
use crate::event::{AppEvent, external_invalidation_for_app_event};
use crate::tab_coordinator::{TabCoordinator, TerminalTabFactory, logical_window_width};
use crate::tab_manager::TabManager;
use crate::tab_view::{TabUiController, TabWorkspace};
use crate::telemetry::{FrameState, HIDDEN_STARTUP_RETRY_DELAY};
use harbor_pty::ShellCommand;
use harbor_terminal::{
    GpuContext, Terminal, TerminalAppearance, TextMetrics, alpha_mode_supports_transparency,
    load_system_fonts,
};
use harbor_widget::effects::{ControlFlowEffect, RuntimeEffects};
use harbor_widget::winit::{FrameOutcome, WinitAdapter, WinitFrameTarget};

/// Active session resources that exist while the window and renderer are alive.
pub(crate) struct ActiveSession {
    window: Arc<Window>,
    gpu: Arc<GpuContext>,
    tabs: TabCoordinator,
    paste: PasteController,
    widget_runtime: harbor_widget::runtime::Runtime,
    winit_adapter: WinitAdapter,
    _backdrop: Box<dyn WindowBackdropBackend>,
    backdrop_available: bool,
    show_pending: bool,
    startup_retry_deadline: Option<Instant>,
}

/// Winit coordinator managing the application lifecycle and active session state.
pub(crate) struct Shell {
    session: Option<ActiveSession>,
    frame: FrameState,
    event_proxy: EventLoopProxy<AppEvent>,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ShellError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
    #[error("failed to create terminal tab")]
    Tab(#[source] anyhow::Error),
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
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.handle_user_event(event_loop, event);
    }

    /// Called when the event loop is about to block.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let host_deadline = self.frame.next_steady_state_deadline();
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.about_to_wait(event_loop, host_deadline);
    }

    /// Dispatches window-level events: resize, redraw, close, and terminal input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.handle_window_event(event_loop, window_id, event, &mut self.frame);
    }
}

// ── ActiveSession (active window lifecycle) ───────────────────────────────
impl ActiveSession {
    fn sync_terminal_allocation(&mut self, event_loop: &ActiveEventLoop) {
        let viewport = self.winit_adapter.viewport();
        if self
            .tabs
            .apply_pending_allocation(viewport.scale_factor, viewport.physical_size)
        {
            let effects = self.winit_adapter.request_frame();
            apply_effects(&self.window, &effects, event_loop);
        }
    }

    fn handle_user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        let AppEvent::TerminalOutputReady(tab_id) = event;
        let outcome = self.tabs.process_output(tab_id);
        if outcome.unread_changed {
            self.tabs.sync_ui(&self.window);
            let effects = self.widget_runtime.update(Instant::now());
            let effects = self.winit_adapter.fold_effects(effects);
            self.sync_terminal_allocation(event_loop);
            apply_effects(&self.window, &effects, event_loop);
        }
        if !outcome.request_active_invalidation {
            return;
        }
        let Some(invalidation) = external_invalidation_for_app_event(event) else {
            return;
        };
        let effects = self
            .winit_adapter
            .invalidate_external(&mut self.widget_runtime, invalidation);
        apply_effects(&self.window, &effects, event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop, host_deadline: Option<Instant>) {
        let now = Instant::now();
        let main_effects =
            self.winit_adapter
                .about_to_wait(&mut self.widget_runtime, now, host_deadline);
        self.sync_terminal_allocation(event_loop);
        apply_window_effects(&self.window, &main_effects);
        let mut combined_flow = main_effects.control_flow.unwrap_or(ControlFlowEffect::Wait);

        if let Some(confirmation_flow) = self.paste.about_to_wait(now) {
            combined_flow = combined_flow.arbitrate(confirmation_flow);
        }

        if let Some(deadline) = self.startup_retry_deadline {
            if now >= deadline {
                self.startup_retry_deadline = None;
                self.request_main_frame(event_loop);
            } else {
                combined_flow = combined_flow.arbitrate(ControlFlowEffect::WaitUntil(deadline));
            }
        }

        apply_control_flow(event_loop, combined_flow);
    }

    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
        frame: &mut FrameState,
    ) {
        if self.paste.window_id() == Some(window_id) {
            match self.paste.handle_dialog_event(
                &event,
                event_loop,
                &self.gpu,
                self.tabs.active_terminal().as_ref(),
            ) {
                PasteEventOutcome::Handled { request_redraw } => {
                    if request_redraw {
                        self.request_main_frame(event_loop);
                    }
                }
                PasteEventOutcome::Fatal(error) => {
                    tracing::error!(?error, "fatal confirmation-window frame error");
                    event_loop.exit();
                }
            }
            return;
        }

        let gate_active = self.paste.is_active();

        if self.window.id() != window_id {
            return;
        }

        // Reject activations before Runtime can move focus or capture a pointer. Wheel events
        // still reach the terminal bridge, whose paste gate intentionally permits scrolling.
        if gate_active {
            if matches!(&event, WindowEvent::KeyboardInput { .. }) {
                return;
            }
            if matches!(
                &event,
                WindowEvent::MouseInput { .. } | WindowEvent::Touch(_)
            ) {
                self.winit_adapter.quarantine_blocked_pointer_event(&event);
                return;
            }
        }

        if matches!(&event, WindowEvent::CloseRequested) {
            tracing::info!("close requested");
            event_loop.exit();
            return;
        }

        if is_paste_shortcut(&event, self.winit_adapter.modifiers()) {
            let effects = self.paste.paste_from_clipboard(
                event_loop,
                &self.window,
                &self.gpu,
                self.tabs.active_terminal().as_ref(),
                &mut self.widget_runtime,
                &mut self.winit_adapter,
            );
            apply_effects(&self.window, &effects, event_loop);
            return;
        }

        if self.tabs.update_presentation(&self.window) {
            let effects = self.widget_runtime.update(Instant::now());
            let effects = self.winit_adapter.fold_effects(effects);
            apply_effects(&self.window, &effects, event_loop);
        }
        let size = self.window.inner_size();
        let outcome = self.winit_adapter.handle_event_with_size(
            &mut self.widget_runtime,
            &event,
            Some((size.width, size.height)),
        );
        self.sync_terminal_allocation(event_loop);
        if outcome.handled {
            apply_effects(&self.window, &outcome.effects, event_loop);
            self.tabs.drain_tab_commands(
                &self.window,
                &mut self.widget_runtime,
                &mut self.winit_adapter,
                event_loop,
            );
        }

        if let WindowEvent::RedrawRequested = event {
            tracing::trace!("redraw requested");
            self.render_frame(event_loop, frame);
        }
    }

    fn request_main_frame(&mut self, event_loop: &ActiveEventLoop) {
        let effects = self.winit_adapter.request_frame();
        apply_effects(&self.window, &effects, event_loop);
    }

    fn render_frame(&mut self, event_loop: &ActiveEventLoop, frame: &mut FrameState) -> bool {
        let outcome = {
            let mut configure = |width, height| self.gpu.configure_size(width, height);
            let (surface, device, queue) = self.gpu.borrow_frame();
            let target = WinitFrameTarget::new(
                &self.window,
                surface,
                device,
                queue,
                &mut configure,
                self.backdrop_available,
                self.gpu.alpha_mode(),
            );
            self.winit_adapter.render(&mut self.widget_runtime, target)
        };
        self.sync_terminal_allocation(event_loop);

        let effects = outcome.effects().clone();
        apply_effects(&self.window, &effects, event_loop);

        let presented = outcome.is_presented();
        if presented {
            frame.mark_first_present();
            let _ = frame.next_steady_state_deadline();
            if self.show_pending {
                self.window.set_visible(true);
                self.show_pending = false;
                self.startup_retry_deadline = None;
            }
        } else if let FrameOutcome::Fatal(error, _) = &outcome {
            tracing::error!(?error, "fatal main-window frame error");
            event_loop.exit();
        }

        if matches!(&outcome, FrameOutcome::Skipped(_)) && self.show_pending {
            self.startup_retry_deadline = Some(Instant::now() + HIDDEN_STARTUP_RETRY_DELAY);
        }
        presented
    }
}

/// Initializes the persistent tab workspace once; subsequent model changes use its Signal.
fn init_widget_runtime(
    window: &Arc<Window>,
    gpu: &GpuContext,
    tab_ui: TabUiController,
    backdrop_available: bool,
    backdrop_fallback: [f32; 3],
) -> (harbor_widget::runtime::Runtime, RuntimeEffects) {
    let initial_size = window.inner_size();
    let initial_viewport = harbor_widget::renderer::Viewport::new(
        initial_size.width,
        initial_size.height,
        window.scale_factor() as f32,
    );
    let mut runtime = harbor_widget::runtime::Runtime::new();
    runtime.set_root(TabWorkspace::with_fallback(
        tab_ui.clone(),
        backdrop_available,
        backdrop_fallback,
    ));
    runtime.init_renderer(gpu.device(), gpu.format());
    runtime.set_viewport(initial_viewport);
    let mut initial_effects = runtime.update(Instant::now());
    initial_effects.merge(runtime.request_focus(&tab_ui.terminal_focus()));
    initial_effects.merge(runtime.take_pending_effects());
    (runtime, initial_effects)
}

// ── Shell (own methods) ───────────────────────────────────────────────────
impl Shell {
    /// Creates the application shell with no initial window, GPU, or terminal.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            session: None,
            frame: FrameState::new(),
            event_proxy,
        }
    }

    /// Creates the main window, GPU context, font atlas, and terminal engine.
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), ShellError> {
        if self.session.is_some() {
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

        #[allow(clippy::arc_with_non_send_sync)]
        let gpu = Arc::new(
            pollster::block_on(GpuContext::new(window.clone())).map_err(ShellError::Renderer)?,
        );
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
        let font_settings = settings.font.clone();
        let fonts = load_system_fonts(&font_settings).map_err(ShellError::Renderer)?;
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        let size = Terminal::terminal_size_for(&gpu, &metrics);
        let shell_command = ShellCommand::new(settings.shell.program, settings.shell.args);
        let input_gate = Arc::new(AtomicBool::new(false));
        let event_proxy = self.event_proxy.clone();
        let factory = TerminalTabFactory::new(
            Arc::clone(&gpu),
            shell_command,
            font_settings,
            metrics,
            appearance,
            main_window_backdrop_available,
            event_proxy,
            Arc::clone(&input_gate),
        );
        let mut tabs = TabManager::new();
        tabs.create_tab(|tab_id, draw_id| factory.create_resources(tab_id, draw_id, size))
            .map_err(ShellError::Tab)?;

        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
        let mut winit_adapter = WinitAdapter::from_window(&window);
        winit_adapter.set_drawable(initial_size.width != 0 && initial_size.height != 0);
        let active_bridge = tabs
            .active_bridge()
            .expect("initial tab creation establishes an active bridge");
        let tab_ui = TabUiController::new(
            tabs.snapshots(),
            active_bridge,
            logical_window_width(&window),
        );
        let (widget_runtime, initial_effects) = init_widget_runtime(
            &window,
            &gpu,
            tab_ui.clone(),
            main_window_backdrop_available,
            backdrop_style.fallback,
        );
        let tabs = TabCoordinator::new(tabs, tab_ui, factory);
        let paste = PasteController::new(input_gate);
        let mut session = ActiveSession {
            window,
            gpu,
            tabs,
            paste,
            widget_runtime,
            winit_adapter,
            _backdrop: backdrop,
            backdrop_available: main_window_backdrop_available,
            show_pending: true,
            startup_retry_deadline: None,
        };

        session.sync_terminal_allocation(event_loop);
        let mut effects = session.winit_adapter.fold_effects(initial_effects);
        effects.merge(session.winit_adapter.request_frame());
        apply_effects(&session.window, &effects, event_loop);
        let _ = session.render_frame(event_loop, &mut self.frame);
        self.session = Some(session);
        Ok(())
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
