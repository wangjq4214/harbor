//! Application shell: winit lifecycle, window bootstrap, frame render.

mod confirmation;
pub(crate) mod input;
pub(crate) mod translate;
mod ui;

use std::{collections::HashMap, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    app::input::InputEncoder,
    event::{AppEvent, FrameControlFlow, FrameScheduler, RedrawReason},
    terminal_worker::{TerminalWorkerClient, empty_snapshot},
};
use confirmation::ConfirmationWindow;
use harbor_render::{
    EventResult, GpuContext, SurfaceDisposition, SurfaceStatus, TextMetrics, UiRequest,
    load_system_fonts, surface_disposition,
};
use harbor_types::{
    RevisionedUpdateReceiver, TerminalSize, TerminalSnapshot, UpdateDamage, WorkerStatus,
};
use harbor_widget::input::event::{KeyboardEvent as WidgetKbEvent, UiEvent};

use translate::{widget_key_to_winit, widget_to_winit_mods, winit_to_uievent};
use ui::UiRoot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollbackNavigation {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

fn scrollback_navigation(
    logical_key: &Key,
    modifiers: ModifiersState,
    is_alt_screen: bool,
) -> Option<ScrollbackNavigation> {
    if is_alt_screen
        || modifiers.shift_key()
        || modifiers.control_key()
        || modifiers.alt_key()
        || modifiers.super_key()
    {
        return None;
    }

    match logical_key {
        Key::Named(NamedKey::PageUp) => Some(ScrollbackNavigation::PageUp),
        Key::Named(NamedKey::PageDown) => Some(ScrollbackNavigation::PageDown),
        Key::Named(NamedKey::Home) => Some(ScrollbackNavigation::Top),
        Key::Named(NamedKey::End) => Some(ScrollbackNavigation::Bottom),
        _ => None,
    }
}

/// Outcome of a dialog-overlay event dispatch.
struct DialogResult {
    outcome: DialogOutcome,
    /// True when ScaleFactorChanged was handled, requiring a main-window redraw.
    needs_redraw: Option<RedrawReason>,
}

enum DialogOutcome {
    None,
    Cancelled,
    Confirmed(String),
}

/// Owns the optional paste-confirmation dialog and mediates its lifecycle
/// so `window_event` does not inline the take-match-put-back pattern.
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

    /// Dispatches a window event to the active confirmation dialog, returning
    /// the outcome and signalling when a main-window side-effect is needed.
    fn handle_event(
        &mut self,
        event: &WindowEvent,
        scale: f32,
        gpu: Option<&GpuContext>,
        ui: Option<&UiRoot>,
    ) -> DialogResult {
        let Some(mut confirmation) = self.window.take() else {
            return DialogResult {
                outcome: DialogOutcome::None,
                needs_redraw: None,
            };
        };
        let mut needs_redraw = None;
        let outcome = match confirmation.handle_event(event, scale) {
            confirmation::ConfirmationResult::Cancelled => DialogOutcome::Cancelled,
            confirmation::ConfirmationResult::Confirmed => {
                let raw_text = confirmation.raw_text().to_owned();
                DialogOutcome::Confirmed(raw_text)
            }
            confirmation::ConfirmationResult::None => {
                if matches!(event, WindowEvent::RedrawRequested)
                    && let (Some(gpu), Some(ui)) = (gpu, ui)
                {
                    confirmation.render(gpu.device(), gpu.queue(), &|ch| {
                        ui.text_glyph(ch).copied()
                    });
                }
                if let WindowEvent::ScaleFactorChanged { scale_factor, .. } = event
                    && let Some(gpu) = gpu
                {
                    confirmation.scale_factor_changed(gpu.device(), *scale_factor);
                    needs_redraw = Some(RedrawReason::Resize);
                }
                if let WindowEvent::Resized(size) = event
                    && let Some(gpu) = gpu
                {
                    confirmation.resize(gpu.device(), size.width, size.height);
                }
                self.window = Some(confirmation);
                DialogOutcome::None
            }
        };
        DialogResult {
            outcome,
            needs_redraw,
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
    ui: Option<UiRoot>,
    /// Widget framework runtime (Phase 1: layout + quad rendering).
    widget_runtime: Option<harbor_widget::runtime::Runtime>,
    dialog: DialogOverlay,
}

/// Terminal-worker session state and its published projection.
struct TerminalSession {
    latest_snapshot: Option<TerminalSnapshot>,
    updates: RevisionedUpdateReceiver,
    worker: Option<TerminalWorkerClient>,
    worker_status: WorkerStatus,
    pending_resize: Option<TerminalSize>,
    pending_snapshot_commands: HashMap<u64, Instant>,
}

/// State governing damage, scheduling, and surface recovery.
struct FrameState {
    pending_damage: Option<UpdateDamage>,
    render_dirty: bool,
    scheduler: FrameScheduler,
    surface_recovery_attempted: bool,
}

/// Winit coordinator over concrete lifecycle state groups.
pub(crate) struct App {
    runtime: AppRuntime,
    session: TerminalSession,
    frame: FrameState,
    event_proxy: EventLoopProxy<AppEvent>,
    modifiers: ModifiersState,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to start terminal worker")]
    Worker(#[source] anyhow::Error),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
}

// ── ApplicationHandler (winit lifecycle) ──────────────────────────────────
impl ApplicationHandler<AppEvent> for App {
    /// Called on start or wake from suspend.  Bootstraps the window, GPU,
    /// component tree, terminal, and PTY on first call.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    /// Handles terminal-worker update wakes without touching the worker model.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        let AppEvent::WorkerUpdateReady = event;
        if self.consume_worker_updates() {
            self.request_redraw(RedrawReason::WorkerUpdate);
        }
    }

    /// Called when the event loop is about to block. Applies pending resize,
    /// then drives component deadlines (cursor blink, scrollbar auto-hide).
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.consume_worker_updates() {
            self.request_redraw(RedrawReason::WorkerUpdate);
        }
        let (Some(ui), Some(snapshot), Some(worker), Some(window)) = (
            self.runtime.ui.as_mut(),
            self.session.latest_snapshot.as_ref(),
            self.session.worker.as_ref(),
            self.runtime.window.as_ref(),
        ) else {
            self.frame.scheduler.set_deadline(None);
            self.set_control_flow(event_loop);
            return;
        };

        if let Some(new_size) = self.session.pending_resize.take()
            && let Some(request_id) = worker.request_resize(new_size)
        {
            self.session
                .pending_snapshot_commands
                .insert(request_id, Instant::now());
        }

        if matches!(
            self.session.worker_status,
            WorkerStatus::Failed { .. } | WorkerStatus::Stopped
        ) {
            self.session.pending_snapshot_commands.clear();
        }

        if !self.session.pending_snapshot_commands.is_empty() {
            self.frame.scheduler.set_deadline(None);
            if self.frame.scheduler.control_flow() == FrameControlFlow::Poll {
                event_loop.set_control_flow(ControlFlow::Wait);
            } else {
                self.set_control_flow(event_loop);
            }
            return;
        }

        let wait = ui.compact_deadline(snapshot);
        for request in wait.requests {
            match request {
                UiRequest::Scroll(amount) => {
                    let _ = worker.request_scroll_viewport(amount);
                }
                UiRequest::Redraw => {
                    Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Active)
                }
                UiRequest::SetSelectionDragActive(active) => {
                    let _ = worker.send(harbor_types::TerminalCommand::SetSelectionDragActive(
                        active,
                    ));
                }
                _ => {}
            }
        }
        self.frame.scheduler.set_deadline(wait.deadline);
        if self.frame.scheduler.should_request_continuous_redraw() {
            self.request_redraw(RedrawReason::Active);
        }
        self.set_control_flow(event_loop);
    }

    /// Dispatches window-level events: resize, redraw, close, keyboard input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.consume_worker_updates() {
            self.request_redraw(RedrawReason::WorkerUpdate);
        }

        let dialog_window_id = self.runtime.dialog.window_id();
        if dialog_window_id == Some(window_id) {
            let scale = self
                .runtime
                .window
                .as_ref()
                .map(|window| window.scale_factor() as f32)
                .unwrap_or(1.0);
            let result = self.runtime.dialog.handle_event(
                &event,
                scale,
                self.runtime.gpu.as_ref(),
                self.runtime.ui.as_ref(),
            );
            if let Some(reason) = result.needs_redraw {
                self.frame.render_dirty = true;
                self.request_redraw(reason);
            }
            match result.outcome {
                DialogOutcome::Cancelled => {
                    self.request_redraw(RedrawReason::Input);
                    return;
                }
                DialogOutcome::Confirmed(raw_text) => {
                    if let Some(worker) = self.session.worker.as_ref() {
                        let _ = worker.send(harbor_types::TerminalCommand::PasteText(raw_text));
                    }
                    self.request_redraw(RedrawReason::Input);
                    return;
                }
                DialogOutcome::None => {}
            }
        }
        let gate_active = self.runtime.dialog.is_active();

        let (Some(gpu), Some(ui), Some(snapshot), Some(worker), Some(window)) = (
            self.runtime.gpu.as_mut(),
            self.runtime.ui.as_mut(),
            self.session.latest_snapshot.as_ref(),
            self.session.worker.as_ref(),
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

        if matches!(&event, WindowEvent::Focused(false)) {
            self.frame.scheduler.set_active(false);
        }

        let mut handled = ui.handle_event(&event, snapshot, self.modifiers);
        for request in handled.requests.drain(..) {
            match request {
                UiRequest::Copy(bounds) => {
                    if let Some(request_id) = worker.request_copy(bounds) {
                        ui.set_copy_pending(request_id);
                    }
                }
                UiRequest::Paste(text) => {
                    let _ = worker.send(harbor_types::TerminalCommand::PasteText(text));
                }
                UiRequest::Scroll(amount) => {
                    let _ = worker.request_scroll_viewport(amount);
                }
                UiRequest::ScrollToTop => {
                    let _ = worker.request_scroll_to_top();
                }
                UiRequest::ScrollToBottom => {
                    let _ = worker.request_scroll_to_bottom();
                }
                UiRequest::SetSelectionDragActive(active) => {
                    let _ = worker.send(harbor_types::TerminalCommand::SetSelectionDragActive(
                        active,
                    ));
                }
                UiRequest::Input(input) => {
                    let _ = worker.send(harbor_types::TerminalCommand::Input(input));
                }
                UiRequest::Redraw => {
                    Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input)
                }
            }
        }
        let handled = handled.event;

        match &event {
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => self
                .frame
                .scheduler
                .set_active(*state == ElementState::Pressed && handled == EventResult::Handled),
            _ => {}
        }

        if let EventResult::ConfirmPaste(raw_text) = &handled {
            if !self.runtime.dialog.is_active() {
                let metrics = *ui.text_metrics();
                harbor_widget::text::set_current_metrics(metrics);

                // Compute preview wrapping and ensure glyphs for all preview chars.
                let max_chars = ((crate::app::confirmation::DIALOG_WIDTH
                    - crate::app::confirmation::DIALOG_HORIZONTAL_PADDING)
                    as f32
                    / metrics.cell_width)
                    .floor() as usize;
                let max_chars = max_chars.max(1);
                let wrapped_lines =
                    crate::app::confirmation::wrap_preview_text(raw_text, max_chars);
                let all_preview_text: String = wrapped_lines.join("");
                ui.ensure_glyphs(&all_preview_text, gpu);

                self.runtime.dialog.open(ConfirmationWindow::new(
                    raw_text.clone(),
                    wrapped_lines,
                    event_loop,
                    gpu,
                    metrics,
                    ui.text_bind_group_layout(),
                    ui.text_bind_group(),
                    Some(window),
                ));
            }
            self.frame.render_dirty = true;
            Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
            return;
        }

        let is_copy = self.modifiers.control_key()
            && matches!(&event, WindowEvent::KeyboardInput { event: kbd, .. }
                if kbd.state == ElementState::Pressed
                && matches!(&kbd.logical_key, Key::Character(ch) if ch == "c" || ch == "C")
            );

        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
            && kbd.text.is_some()
            && !(handled == EventResult::Handled && is_copy)
        {
            let _ = worker.request_scroll_to_bottom();
        }

        if handled == EventResult::Handled {
            self.frame.render_dirty = true;
            Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
            return;
        }

        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
            && let Some(navigation) =
                scrollback_navigation(&kbd.logical_key, self.modifiers, snapshot.is_alt)
        {
            let page_rows = snapshot.rows;
            let request_id = match navigation {
                ScrollbackNavigation::PageUp => {
                    worker.request_scroll_viewport(-(page_rows as isize))
                }
                ScrollbackNavigation::PageDown => {
                    worker.request_scroll_viewport(page_rows as isize)
                }
                ScrollbackNavigation::Top => worker.request_scroll_to_top(),
                ScrollbackNavigation::Bottom => worker.request_scroll_to_bottom(),
            };
            if let Some(request_id) = request_id {
                self.session
                    .pending_snapshot_commands
                    .insert(request_id, Instant::now());
            }
            Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
            return;
        }

        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
        {
            self.frame.render_dirty = true;
            Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
        }

        let mut widget_handled_key = false;
        if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
            let scale = window.scale_factor() as f32;
            let is_keyboard = matches!(&event, WindowEvent::KeyboardInput { .. });
            if (!gate_active || !is_keyboard)
                && let Some(ui_event) = winit_to_uievent(&event, scale, self.modifiers)
            {
                let frame_request = widget_runtime.dispatch(ui_event, Instant::now());
                if frame_request.needs_redraw {
                    Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
                }
                if !gate_active {
                    for (_id, external_event) in widget_runtime.drain_external_input() {
                        if let UiEvent::Keyboard(WidgetKbEvent::KeyDown { key, modifiers }) =
                            external_event
                        {
                            let (logical_key, text) = widget_key_to_winit(&key);
                            if let Some(request) = InputEncoder::request(
                                &logical_key,
                                text.as_deref(),
                                widget_to_winit_mods(modifiers),
                                false,
                            ) {
                                let _ = worker.send(harbor_types::TerminalCommand::Input(request));
                            }
                            widget_handled_key = true;
                        }
                    }
                }
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
                worker.shutdown();
                event_loop.exit();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                if size.width == 0 || size.height == 0 {
                    return;
                }
                self.frame.surface_recovery_attempted = false;
                gpu.resize(size.width, size.height);
                self.session.pending_resize = Some(ui.terminal_size(gpu));
                self.frame.render_dirty = true;
                if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
                    let scale = window.scale_factor() as f32;
                    let viewport =
                        harbor_widget::renderer::Viewport::new(size.width, size.height, scale);
                    widget_runtime.set_viewport(viewport);
                    widget_runtime.update(Instant::now());
                }
                Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Resize);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                tracing::trace!(?scale_factor, "main window scale factor changed");
                let sf = scale_factor as f32;
                let (physical_w, physical_h) = gpu.surface_size();
                gpu.reconfigure();
                if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
                    let viewport =
                        harbor_widget::renderer::Viewport::new(physical_w, physical_h, sf);
                    widget_runtime.set_viewport(viewport);
                    widget_runtime.update(Instant::now());
                }
                self.frame.render_dirty = true;
                Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Resize);
            }
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                self.frame.scheduler.redraw_requested();
                self.render_frame();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if snapshot.is_alt {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y * 3.0) as isize,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as isize,
                };
                if lines != 0 {
                    let request_id = worker.request_scroll_viewport(-lines);
                    if let Some(request_id) = request_id {
                        self.session
                            .pending_snapshot_commands
                            .insert(request_id, Instant::now());
                    }
                    Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
                }
            }
            WindowEvent::KeyboardInput {
                device_id: _,
                event,
                is_synthetic: _,
            } if event.state == ElementState::Pressed && !gate_active && !widget_handled_key => {
                let is_numpad = event.location == winit::keyboard::KeyLocation::Numpad;
                let Some(request) = InputEncoder::request(
                    &event.logical_key,
                    event.text.as_deref(),
                    self.modifiers,
                    is_numpad,
                ) else {
                    return;
                };
                let _ = worker.send(harbor_types::TerminalCommand::Input(request));
            }
            _ => {}
        }
    }
}

// ── App (own methods) ─────────────────────────────────────────────────────
impl App {
    /// Creates the application shell with no initial window, GPU, or worker.
    /// These are lazily initialised on the first `resumed` call.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            runtime: AppRuntime {
                window: None,
                gpu: None,
                ui: None,
                widget_runtime: None,
                dialog: DialogOverlay { window: None },
            },
            session: TerminalSession {
                latest_snapshot: None,
                updates: RevisionedUpdateReceiver::default(),
                worker: None,
                worker_status: WorkerStatus::Ready,
                pending_resize: None,
                pending_snapshot_commands: HashMap::new(),
            },
            frame: FrameState {
                pending_damage: None,
                render_dirty: false,
                scheduler: FrameScheduler::default(),
                surface_recovery_attempted: false,
            },
            event_proxy,
            modifiers: ModifiersState::default(),
        }
    }

    /// Creates the main window, GPU context, font atlas, and component tree.
    /// Keeps existing state on repeated resumes (e.g. after suspend/resume).
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.runtime.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);

        #[cfg(target_os = "windows")]
        paint_gdi_background(&window);

        let font_handle = std::thread::Builder::new()
            .name("font-loader".into())
            .spawn(|| {
                #[cfg(target_os = "windows")]
                {
                    use windows::Win32::System::Threading::{
                        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
                    };
                    unsafe {
                        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                    }
                }
                load_system_fonts()
            })
            .expect("failed to spawn font-loader thread");

        let gpu =
            pollster::block_on(GpuContext::new(window.clone())).map_err(AppError::Renderer)?;
        gpu.clear_surface(bg_wgpu(harbor_config::BACKGROUND));

        let fonts = font_handle
            .join()
            .map_err(|_| AppError::Renderer(anyhow::anyhow!("font loader thread panicked")))?
            .map_err(AppError::Renderer)?;
        let metrics = TextMetrics::new(&fonts);

        let bootstrap = empty_snapshot(1, 1);
        let ui = UiRoot::new(&gpu, &bootstrap, fonts, metrics).map_err(AppError::Renderer)?;
        let size = ui.terminal_size(&gpu);
        let worker = TerminalWorkerClient::start(size, self.event_proxy.clone())
            .map_err(AppError::Worker)?;
        let initial = worker.take_update().ok_or_else(|| {
            AppError::Worker(anyhow::anyhow!("worker did not publish initial snapshot"))
        })?;

        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
        self.runtime.gpu = Some(gpu);
        self.runtime.ui = Some(ui);
        self.init_widget_runtime();
        self.session
            .updates
            .accept(initial.clone())
            .expect("initial worker revision must be accepted");
        self.frame.pending_damage = Some(UpdateDamage::FullUpload);
        self.session.latest_snapshot = Some(initial.snapshot);
        self.session.worker_status = worker.status();
        self.session.worker = Some(worker);
        self.runtime.window = Some(window.clone());
        self.request_redraw(RedrawReason::Input);
        Ok(())
    }

    fn request_redraw(&mut self, reason: RedrawReason) {
        if let Some(window) = self.runtime.window.as_ref() {
            Self::wake_redraw(&mut self.frame.scheduler, window, reason);
        }
    }

    fn wake_redraw(scheduler: &mut FrameScheduler, window: &Window, reason: RedrawReason) {
        if scheduler.wake(reason) {
            tracing::trace!(?reason, "requesting redraw");
            window.request_redraw();
        }
    }

    fn set_control_flow(&self, event_loop: &ActiveEventLoop) {
        match self.frame.scheduler.control_flow() {
            FrameControlFlow::Wait => event_loop.set_control_flow(ControlFlow::Wait),
            FrameControlFlow::WaitUntil(deadline) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
            FrameControlFlow::Poll => event_loop.set_control_flow(ControlFlow::Poll),
        }
    }

    fn damage_after_coalescing(damage: UpdateDamage, accepted_updates: usize) -> UpdateDamage {
        if accepted_updates > 1 {
            UpdateDamage::FullUpload
        } else {
            damage
        }
    }

    fn consume_worker_updates(&mut self) -> bool {
        let mut changed = false;
        let mut latest_update = None;
        let mut accepted_updates = 0usize;
        loop {
            let update = self
                .session
                .worker
                .as_ref()
                .and_then(TerminalWorkerClient::take_update);
            let Some(update) = update else {
                break;
            };
            let Some(update) = self.session.updates.accept(update) else {
                continue;
            };
            accepted_updates = accepted_updates.saturating_add(1);
            latest_update = Some(update);
        }
        if let Some(update) = latest_update {
            let damage = Self::damage_after_coalescing(update.damage, accepted_updates);
            if let Some(existing) = self.frame.pending_damage.as_mut() {
                *existing = UpdateDamage::FullUpload;
            } else {
                self.frame.pending_damage = Some(damage);
            }
            self.session.latest_snapshot = Some(update.snapshot);
            self.frame.render_dirty = true;
            changed = true;
        }
        loop {
            let request_id = self
                .session
                .worker
                .as_ref()
                .and_then(TerminalWorkerClient::take_acknowledgement);
            let Some(request_id) = request_id else {
                break;
            };
            self.session.pending_snapshot_commands.remove(&request_id);
        }
        loop {
            let result = self
                .session
                .worker
                .as_ref()
                .and_then(TerminalWorkerClient::take_copy_result);
            let Some(result) = result else {
                break;
            };
            if let Some(ui) = self.runtime.ui.as_mut()
                && ui.apply_copy_result(result)
            {
                changed = true;
            }
        }
        if let Some(worker) = self.session.worker.as_ref() {
            let status = worker.status();
            if status != self.session.worker_status {
                match &status {
                    WorkerStatus::Failed { .. } => {
                        tracing::error!(status = ?status, "terminal worker failed");
                        self.session.pending_snapshot_commands.clear();
                    }
                    WorkerStatus::Stopped => {
                        tracing::info!(status = ?status, "terminal worker stopped");
                        self.session.pending_snapshot_commands.clear();
                    }
                    WorkerStatus::Ready | WorkerStatus::Processing | WorkerStatus::Idle => {}
                }
                self.session.worker_status = status;
                changed = true;
            }
        }
        changed
    }

    fn render_frame(&mut self) {
        let (Some(gpu), Some(ui), Some(snapshot)) = (
            self.runtime.gpu.as_mut(),
            self.runtime.ui.as_mut(),
            self.session.latest_snapshot.as_ref(),
        ) else {
            return;
        };

        let frame = gpu.get_current_texture();
        let status = match &frame {
            wgpu::CurrentSurfaceTexture::Success(_) => SurfaceStatus::Success,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) => SurfaceStatus::Suboptimal,
            wgpu::CurrentSurfaceTexture::Lost => SurfaceStatus::Lost,
            wgpu::CurrentSurfaceTexture::Outdated => SurfaceStatus::Outdated,
            wgpu::CurrentSurfaceTexture::Timeout => SurfaceStatus::Timeout,
            wgpu::CurrentSurfaceTexture::Occluded => SurfaceStatus::Occluded,
            wgpu::CurrentSurfaceTexture::Validation => SurfaceStatus::Validation,
        };
        let disposition = surface_disposition(status);
        let (output, reconfigure_after_present) = match (frame, disposition) {
            (wgpu::CurrentSurfaceTexture::Success(output), SurfaceDisposition::Present) => {
                (output, false)
            }
            (
                wgpu::CurrentSurfaceTexture::Suboptimal(output),
                SurfaceDisposition::PresentAndReconfigure,
            ) => {
                tracing::warn!("surface texture suboptimal; presenting then reconfiguring");
                (output, true)
            }
            (_, SurfaceDisposition::ReconfigureAndRedraw) => {
                tracing::warn!(?status, "surface requires reconfiguration");
                if !self.frame.surface_recovery_attempted {
                    self.frame.surface_recovery_attempted = true;
                    gpu.reconfigure();
                    self.request_redraw(RedrawReason::SurfaceRecovery);
                } else {
                    tracing::warn!(?status, "surface recovery deferred until external wake");
                }
                return;
            }
            (_, SurfaceDisposition::Skip) => {
                tracing::debug!(?status, "surface frame skipped");
                return;
            }
            _ => unreachable!("surface disposition must match texture status"),
        };

        // Surface texture acquired successfully — now prepare GPU buffers
        let render_dirty = std::mem::take(&mut self.frame.render_dirty);
        if let Some(damage) = self.frame.pending_damage.take() {
            ui.prepare_update_damage(gpu, snapshot, &damage);
        } else if render_dirty {
            ui.prepare(gpu, snapshot);
        }

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(bg_wgpu(harbor_config::BACKGROUND)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let terminal_ui: &UiRoot = ui;
            if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
                let scale = self.runtime.window.as_ref().unwrap().scale_factor() as f32;
                let (physical_w, physical_h) = gpu.surface_size();
                let viewport =
                    harbor_widget::renderer::Viewport::new(physical_w, physical_h, scale);
                let draw_terminal =
                    |_id: harbor_widget::scene::primitive::ExternalDrawId,
                     _rect: harbor_widget::layout::Rect,
                     pass: &mut wgpu::RenderPass<'_>| {
                        terminal_ui.draw(pass);
                    };

                widget_runtime.encode(
                    gpu.queue(),
                    &mut render_pass,
                    viewport,
                    Some(&draw_terminal),
                );
            } else {
                terminal_ui.draw(&mut render_pass);
            }
        }

        let command_buffer = encoder.finish();
        gpu.queue().submit(Some(command_buffer));
        gpu.present(output);
        tracing::trace!(?status, "surface frame presented");
        if reconfigure_after_present && !self.frame.surface_recovery_attempted {
            self.frame.surface_recovery_attempted = true;
            gpu.reconfigure();
            self.request_redraw(RedrawReason::SurfaceSuboptimal);
        } else if status == SurfaceStatus::Success {
            self.frame.surface_recovery_attempted = false;
        }
    }

    /// Initializes the widget runtime with a terminal CustomPaint root.
    fn init_widget_runtime(&mut self) {
        use harbor_widget::widgets::custom_paint::CustomPaint;

        const TERMINAL_DRAW_ID: harbor_widget::scene::primitive::ExternalDrawId = 1;

        let gpu = self.runtime.gpu.as_ref().unwrap();
        let mut runtime = harbor_widget::runtime::Runtime::new();
        runtime.set_root(CustomPaint::new(TERMINAL_DRAW_ID));
        runtime.init_renderer(gpu.device(), gpu.format());
        runtime.update(Instant::now());

        self.runtime.widget_runtime = Some(runtime);
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
/// wgpu surface is ready. Prevents the OS from showing a white window during
/// the GPU initialisation period.
///
/// The linear-light BACKGROUND values (0.36, 0.20, 0.08) are converted to
/// sRGB bytes (162, 124, 80) for GDI. COLORREF format is 0x00BBGGRR.
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
    // BACKGROUND linear (0.36, 0.20, 0.08) → sRGB (162, 124, 80).
    // COLORREF byte order is 0x00BBGGRR.
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
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    fn key(name: NamedKey) -> Key {
        Key::Named(name)
    }

    #[test]
    fn bare_navigation_keys_are_owned_in_normal_screen() {
        assert_eq!(
            scrollback_navigation(&key(NamedKey::PageUp), ModifiersState::default(), false),
            Some(ScrollbackNavigation::PageUp)
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::PageDown), ModifiersState::default(), false),
            Some(ScrollbackNavigation::PageDown)
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::Home), ModifiersState::default(), false),
            Some(ScrollbackNavigation::Top)
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::End), ModifiersState::default(), false),
            Some(ScrollbackNavigation::Bottom)
        );
    }

    #[test]
    fn modified_or_alt_screen_navigation_is_not_owned() {
        assert_eq!(
            scrollback_navigation(&key(NamedKey::PageUp), ModifiersState::SHIFT, false),
            None
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::Home), ModifiersState::CONTROL, false),
            None
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::End), ModifiersState::default(), true),
            None
        );
    }
    #[test]
    fn coalesced_updates_require_full_upload_but_single_update_keeps_damage() {
        let ranges = vec![harbor_terminal::DirtyRange {
            row: 0,
            start_col: 1,
            end_col: 2,
        }];
        assert_eq!(
            App::damage_after_coalescing(UpdateDamage::Ranges(ranges.clone()), 1),
            UpdateDamage::Ranges(ranges)
        );
        assert_eq!(
            App::damage_after_coalescing(UpdateDamage::Ranges(Vec::new()), 2),
            UpdateDamage::FullUpload
        );
    }
}
