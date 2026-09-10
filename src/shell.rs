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
use crate::tab_manager::{TabActionOutcome, TabId, TabManager, TerminalTabResources};
use crate::tab_view::{TabCommand, TabFocusPolicy, TabUiController, TabWorkspace};
use crate::telemetry::{FrameState, HIDDEN_STARTUP_RETRY_DELAY};
use crate::terminal_view::{TerminalWidgetBridge, terminal_size_from_allocation};
use harbor_pty::{PtyEndpoints, ShellCommand};
use harbor_terminal::{
    GpuContext, PasteDisposition, Terminal, TerminalAppearance, TextMetrics,
    alpha_mode_supports_transparency, load_system_fonts,
};
use harbor_widget::effects::{ControlFlowEffect, RuntimeEffects};
use harbor_widget::winit::{FrameOutcome, WinitAdapter, WinitFrameTarget};

/// Outcome of a paste confirmation event dispatch.
#[derive(Debug)]
pub(crate) enum PasteEventOutcome {
    Handled { request_redraw: bool },
    Fatal(harbor_widget::winit::FrameError),
}

/// Controller encapsulating modal paste confirmation and clipboard interaction.
pub(crate) struct PasteController {
    dialog: DialogOverlay,
    input_gate: Arc<AtomicBool>,
}

impl PasteController {
    pub(crate) fn new(input_gate: Arc<AtomicBool>) -> Self {
        Self {
            dialog: DialogOverlay::new(),
            input_gate,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.dialog.is_active()
    }

    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.dialog.window_id()
    }

    pub(crate) fn about_to_wait(&mut self, now: Instant) -> Option<ControlFlowEffect> {
        self.dialog.about_to_wait(now)
    }

    #[allow(dead_code)]
    pub(crate) fn is_paste_shortcut(
        event: &WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) -> bool {
        is_paste_shortcut(event, modifiers)
    }

    pub(crate) fn handle_dialog_event(
        &mut self,
        event: &WindowEvent,
        event_loop: &ActiveEventLoop,
        gpu: &GpuContext,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
    ) -> PasteEventOutcome {
        let result = if matches!(event, WindowEvent::RedrawRequested)
            && let Some(active_terminal) = active_terminal
            && let Ok(terminal) = active_terminal.lock()
        {
            let glyph_fn = |ch| terminal.text_glyph(ch).copied();
            self.dialog
                .handle_event(event, event_loop, Some(gpu), Some(&glyph_fn))
        } else {
            self.dialog.handle_event(event, event_loop, Some(gpu), None)
        };

        match &result {
            DialogOutcome::Cancelled | DialogOutcome::Confirmed(_) => {
                if let DialogOutcome::Confirmed(_) = &result
                    && let Some(active_terminal) = active_terminal
                    && let Ok(mut terminal) = active_terminal.lock()
                {
                    let input_modes = terminal.drain_and_snapshot().input_modes;
                    if let Err(error) = write_confirmation_outcome(&result, input_modes, |bytes| {
                        terminal.write_pty(bytes)
                    }) {
                        tracing::warn!(error = %format_args!("{error:#}"), "failed to write confirmed paste");
                    }
                }
                self.input_gate.store(false, Ordering::Release);
                PasteEventOutcome::Handled {
                    request_redraw: true,
                }
            }
            DialogOutcome::Fatal(error) => PasteEventOutcome::Fatal(error.clone()),
            DialogOutcome::None => PasteEventOutcome::Handled {
                request_redraw: false,
            },
        }
    }

    pub(crate) fn paste_from_clipboard(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: &Window,
        gpu: &GpuContext,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        runtime: &mut harbor_widget::runtime::Runtime,
        adapter: &mut WinitAdapter,
    ) -> RuntimeEffects {
        let raw_text =
            match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                Ok(text) => text,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to read clipboard text");
                    return RuntimeEffects::default();
                }
            };

        let confirmation = {
            let Some(active_terminal) = active_terminal else {
                tracing::warn!("no active terminal for clipboard paste");
                return RuntimeEffects::default();
            };
            let Ok(mut terminal) = active_terminal.lock() else {
                tracing::warn!("terminal lock unavailable for clipboard paste");
                return RuntimeEffects::default();
            };
            let input_modes = terminal.drain_and_snapshot().input_modes;

            match PasteDisposition::decide(input_modes, &raw_text) {
                PasteDisposition::SendDirect => {
                    if let Err(error) =
                        terminal.write_pty(input_modes.paste(raw_text.as_bytes()).as_ref())
                    {
                        tracing::warn!(error = %format_args!("{error:#}"), "failed to write clipboard paste");
                    }
                    return RuntimeEffects::default();
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
                        return RuntimeEffects::default();
                    };
                    ConfirmationWindow::new(
                        raw_text,
                        event_loop,
                        gpu,
                        metrics,
                        text_bind_group_layout,
                        text_bind_group,
                        Some(window),
                    )
                }
            }
        };

        self.dialog.open(confirmation);
        self.input_gate.store(true, Ordering::Release);
        adapter.quarantine_active_pointers();
        let effects = runtime.cancel_pointer_captures(harbor_widget::layout::Point::ZERO);
        adapter.fold_effects(effects)
    }
}

/// Coordinator managing Host-owned terminal tab models and their declarative UI projection.
pub(crate) struct TabCoordinator {
    tabs: TabManager,
    tab_ui: TabUiController,
    shell_command: ShellCommand,
    font_settings: harbor_config::FontSettings,
    metrics: TextMetrics,
    appearance: TerminalAppearance,
    gpu: Arc<GpuContext>,
    backdrop_available: bool,
    event_proxy: EventLoopProxy<AppEvent>,
    input_gate: Arc<AtomicBool>,
}

impl TabCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        tabs: TabManager,
        tab_ui: TabUiController,
        shell_command: ShellCommand,
        font_settings: harbor_config::FontSettings,
        metrics: TextMetrics,
        appearance: TerminalAppearance,
        gpu: Arc<GpuContext>,
        backdrop_available: bool,
        event_proxy: EventLoopProxy<AppEvent>,
        input_gate: Arc<AtomicBool>,
    ) -> Self {
        Self {
            tabs,
            tab_ui,
            shell_command,
            font_settings,
            metrics,
            appearance,
            gpu,
            backdrop_available,
            event_proxy,
            input_gate,
        }
    }

    pub(crate) fn active_terminal(&self) -> Option<Arc<Mutex<Terminal>>> {
        self.tabs.active_terminal()
    }

    pub(crate) fn sync_ui(&self, window: &Window) {
        self.tab_ui.sync(
            self.tabs.snapshots(),
            self.tabs.active_bridge(),
            logical_window_width(window),
        );
    }

    pub(crate) fn update_presentation(&self, window: &Window) -> bool {
        self.tab_ui
            .update_presentation(logical_window_width(window))
    }

    pub(crate) fn process_output(&mut self, tab_id: TabId) -> crate::tab_manager::TabOutputOutcome {
        self.tabs.process_output(tab_id)
    }

    pub(crate) fn apply_pending_allocation(
        &mut self,
        scale_factor: f32,
        physical_size: (u32, u32),
    ) -> bool {
        let Some(rect) = self.tab_ui.latest_terminal_allocation() else {
            return false;
        };
        let Some(size) =
            terminal_size_from_allocation(rect, scale_factor, physical_size, &self.metrics)
        else {
            return false;
        };
        self.tabs.resize_all_if_changed(size)
    }

    pub(crate) fn create_terminal_tab(&mut self) -> anyhow::Result<TabActionOutcome> {
        let size = self
            .tabs
            .last_broadcast_size()
            .unwrap_or_else(|| Terminal::terminal_size_for(&self.gpu, &self.metrics));
        let fonts = load_system_fonts(&self.font_settings)?;
        let metrics = self.metrics;
        let shell_command = &self.shell_command;
        let gpu = &self.gpu;
        let appearance = self.appearance;
        let backdrop_available = self.backdrop_available;
        let event_proxy = self.event_proxy.clone();
        let input_gate = Arc::clone(&self.input_gate);
        self.tabs.create_tab(|tab_id, draw_id| {
            create_terminal_tab_resources(
                tab_id,
                draw_id,
                size,
                shell_command,
                gpu,
                fonts,
                metrics,
                appearance,
                backdrop_available,
                event_proxy,
                input_gate,
            )
        })
    }

    pub(crate) fn apply_tab_outcome(
        &mut self,
        window: &Window,
        runtime: &mut harbor_widget::runtime::Runtime,
        adapter: &mut WinitAdapter,
        outcome: TabActionOutcome,
        focus: TabFocusPolicy,
    ) -> (RuntimeEffects, bool) {
        if outcome.close_window {
            self.sync_ui(window);
            runtime.set_root(harbor_widget::widgets::sized_box::SizedBox::new(
                harbor_widget::layout::Size::ZERO,
            ));
            let effects = adapter.fold_effects(runtime.update(Instant::now()));
            return (effects, true);
        }
        let model_changed =
            outcome.active_bridge_changed || outcome.unread_changed || outcome.request_redraw;
        let focus_requested = focus != TabFocusPolicy::PreserveRail;
        if !model_changed && !focus_requested {
            return (RuntimeEffects::default(), false);
        }

        let mut effects = RuntimeEffects::default();
        if outcome.active_bridge_changed {
            adapter.quarantine_active_pointers();
            effects.merge(runtime.cancel_pointer_captures(harbor_widget::layout::Point::ZERO));
        }
        if focus_requested {
            runtime.clear_focus();
        }
        if model_changed {
            self.sync_ui(window);
            effects.merge(runtime.update(Instant::now()));
        }
        let viewport = adapter.viewport();
        self.apply_pending_allocation(viewport.scale_factor, viewport.physical_size);
        let focus_effects = match focus {
            TabFocusPolicy::PreserveRail => RuntimeEffects::default(),
            TabFocusPolicy::RailTab(id) => self
                .tab_ui
                .tab_focus(id)
                .map(|handle| runtime.request_focus(&handle))
                .unwrap_or_default(),
            TabFocusPolicy::Terminal => runtime.request_focus(&self.tab_ui.terminal_focus()),
        };
        effects.merge(focus_effects);
        effects.merge(runtime.take_pending_effects());
        let effects = adapter.fold_effects(effects);
        (effects, false)
    }

    pub(crate) fn drain_tab_commands(
        &mut self,
        window: &Window,
        runtime: &mut harbor_widget::runtime::Runtime,
        adapter: &mut WinitAdapter,
        event_loop: &ActiveEventLoop,
    ) {
        for request in self.tab_ui.drain_commands() {
            if self.input_gate.load(Ordering::Acquire) {
                return;
            }
            let focus = match (request.focus, request.command) {
                (TabFocusPolicy::PreserveRail, TabCommand::Close(id)) => self
                    .tabs
                    .neighbor_for_close(id)
                    .map(TabFocusPolicy::RailTab)
                    .unwrap_or(TabFocusPolicy::PreserveRail),
                (focus, _) => focus,
            };
            let outcome = match request.command {
                TabCommand::New => match self.create_terminal_tab() {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        tracing::warn!(error = %format_args!("{error:#}"), "failed to create terminal tab");
                        continue;
                    }
                },
                TabCommand::Close(id) => self.tabs.close(id),
                TabCommand::CloseActive => self.tabs.close_active(),
                TabCommand::Activate(id) => self.tabs.activate(id),
                TabCommand::Next => self.tabs.activate_next(),
                TabCommand::Previous => self.tabs.activate_previous(),
                TabCommand::Numeric(index) => self.tabs.activate_numeric(index),
            };
            let (effects, close_window) =
                self.apply_tab_outcome(window, runtime, adapter, outcome, focus);
            apply_effects(window, &effects, event_loop);
            if close_window {
                event_loop.exit();
                return;
            }
        }
    }
}

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

fn logical_window_width(window: &Window) -> f64 {
    logical_width_from_physical(window.inner_size().width, window.scale_factor())
}

fn logical_width_from_physical(width: u32, scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        f64::from(width) / scale
    } else {
        0.0
    }
}
/// Initializes the persistent tab workspace once; subsequent model changes use its Signal.
fn init_widget_runtime(
    window: &Arc<Window>,
    gpu: &GpuContext,
    tab_ui: TabUiController,
    backdrop_available: bool,
) -> (harbor_widget::runtime::Runtime, RuntimeEffects) {
    let initial_size = window.inner_size();
    let initial_viewport = harbor_widget::renderer::Viewport::new(
        initial_size.width,
        initial_size.height,
        window.scale_factor() as f32,
    );
    let mut runtime = harbor_widget::runtime::Runtime::new();
    runtime.set_root(TabWorkspace::new(tab_ui.clone(), backdrop_available));
    runtime.init_renderer(gpu.device(), gpu.format());
    runtime.set_viewport(initial_viewport);
    let mut initial_effects = runtime.update(Instant::now());
    initial_effects.merge(runtime.request_focus(&tab_ui.terminal_focus()));
    initial_effects.merge(runtime.take_pending_effects());
    (runtime, initial_effects)
}

#[allow(clippy::too_many_arguments)]
fn create_terminal_tab_resources(
    tab_id: TabId,
    draw_id: harbor_widget::scene::primitive::ExternalDrawId,
    size: harbor_terminal::TerminalSize,
    shell_command: &ShellCommand,
    gpu: &Arc<GpuContext>,
    fonts: harbor_terminal::FontBook,
    metrics: TextMetrics,
    appearance: TerminalAppearance,
    backdrop_available: bool,
    event_proxy: EventLoopProxy<AppEvent>,
    input_gate: Arc<AtomicBool>,
) -> anyhow::Result<TerminalTabResources> {
    let endpoints = PtyEndpoints::spawn_shell(
        harbor_pty::TerminalSize {
            rows: size.rows,
            cols: size.cols,
        },
        shell_command,
    )?;
    let mut terminal = Terminal::try_new_with_appearance_from_endpoints(
        size,
        endpoints,
        gpu,
        fonts,
        metrics,
        appearance,
        move || {
            event_proxy
                .send_event(AppEvent::TerminalOutputReady(tab_id))
                .is_ok()
        },
    )?;
    terminal.set_backdrop_available(backdrop_available);
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(terminal));
    let bridge =
        TerminalWidgetBridge::with_gpu(draw_id, Arc::clone(&terminal), Arc::clone(gpu), input_gate);
    Ok(TerminalTabResources::new(terminal, bridge))
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
        let mut tabs = TabManager::new();
        tabs.create_tab(|tab_id, draw_id| {
            create_terminal_tab_resources(
                tab_id,
                draw_id,
                size,
                &shell_command,
                &gpu,
                fonts,
                metrics,
                appearance,
                main_window_backdrop_available,
                event_proxy.clone(),
                Arc::clone(&input_gate),
            )
        })
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
        );
        let tabs = TabCoordinator::new(
            tabs,
            tab_ui,
            shell_command,
            font_settings,
            metrics,
            appearance,
            Arc::clone(&gpu),
            main_window_backdrop_available,
            event_proxy,
            Arc::clone(&input_gate),
        );
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

    #[test]
    fn logical_width_conversion_handles_dpi_zero_and_invalid_scale() {
        assert_eq!(logical_width_from_physical(1_350, 1.5), 900.0);
        assert_eq!(logical_width_from_physical(0, 2.0), 0.0);
        assert_eq!(logical_width_from_physical(900, 0.0), 0.0);
        assert_eq!(logical_width_from_physical(900, f64::NAN), 0.0);
        assert_eq!(logical_width_from_physical(900, f64::INFINITY), 0.0);
    }
}
