//! Application shell: winit lifecycle, window bootstrap, frame render.

use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::backdrop::{
    MainWindowPlatformHooks, MainWindowPlatformState, os_build, select_backend, wasdk_available,
};
use crate::dialog::{PasteController, PasteEventOutcome, is_paste_shortcut};
use crate::effects::apply_control_flow;
use crate::event::{AppEvent, external_invalidation_for_app_event};
use crate::tab_coordinator::{TabCoordinator, TerminalTabFactory, logical_window_width};
use crate::telemetry::{FrameState, HIDDEN_STARTUP_RETRY_DELAY};
use harbor_app::tab_manager::TabManager;
use harbor_app::tab_view::TabUiController;
use harbor_app::tab_view::ui::MainWindowRootInputs;
#[cfg(not(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions)))]
use harbor_app::tab_view::ui::main_window_root;
use harbor_pty::ShellCommand;
use harbor_terminal::{Terminal, TerminalAppearance, TextMetrics, load_system_fonts};
use harbor_widget::effects::ControlFlowEffect;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::winit::{
    HostFrameOutcome, HostInitContext, HostStartupError, WinitWindowHost, WinitWindowHostBuilder,
};

/// Active session resources that exist while the window and renderer are alive.
pub(crate) struct ActiveSession {
    // Business state drops before the host-owned Runtime/surface/platform/window/GPU stack.
    paste: PasteController,
    tabs: TabCoordinator,
    #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
    root_inputs: MainWindowRootInputs,
    show_pending: bool,
    startup_retry_deadline: Option<Instant>,
    main_host: WinitWindowHost,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MainFramePolicy {
    presented: bool,
    show_window: bool,
    schedule_hidden_retry: bool,
    fatal: bool,
}

fn main_frame_policy(outcome: &HostFrameOutcome, show_pending: bool) -> MainFramePolicy {
    let presented = outcome.is_presented();
    MainFramePolicy {
        presented,
        show_window: presented && show_pending,
        schedule_hidden_retry: matches!(outcome, HostFrameOutcome::Skipped { .. }) && show_pending,
        fatal: outcome.is_fatal(),
    }
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
    #[error("failed to create native window host")]
    Host(#[from] HostStartupError),
    #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
    #[error("failed to load reloadable application UI: {0}")]
    HotReload(String),
}

/// Type-erases static and dynamic application roots behind one Runtime-owned component.
struct ApplicationRoot(Box<dyn Component>);

impl Component for ApplicationRoot {
    fn build(&self, cx: &mut BuildCx) -> View {
        self.0.build(cx)
    }
}

fn build_application_root(inputs: MainWindowRootInputs) -> Result<ApplicationRoot, ShellError> {
    #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
    {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::hot_reload::build_root(
                inputs.controller,
                inputs.backdrop_available,
                inputs.backdrop_fallback,
            )
        }))
        .map(ApplicationRoot)
        .map_err(|panic| ShellError::HotReload(panic_message(panic)))
    }

    #[cfg(not(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions)))]
    {
        Ok(ApplicationRoot(Box::new(main_window_root(inputs))))
    }
}

#[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "reloadable UI panicked".to_owned()
    }
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
    fn merge_wait(current: &mut Option<ControlFlowEffect>, next: Option<ControlFlowEffect>) {
        *current = match (*current, next) {
            (Some(left), Some(right)) => Some(left.arbitrate(right)),
            (Some(wait), None) | (None, Some(wait)) => Some(wait),
            (None, None) => None,
        };
    }

    fn sync_terminal_allocation(&mut self) -> Option<ControlFlowEffect> {
        let viewport = self.main_host.viewport().clone();
        if self
            .tabs
            .apply_pending_allocation(viewport.scale_factor, viewport.physical_size)
        {
            self.main_host.request_frame().wait
        } else {
            None
        }
    }

    fn handle_user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        let mut wait = None;
        match event {
            AppEvent::TerminalOutputReady(tab_id) => {
                let outcome = self.tabs.process_output(tab_id);
                if outcome.unread_changed {
                    self.tabs.sync_ui(self.main_host.window());
                    Self::merge_wait(
                        &mut wait,
                        self.main_host
                            .invalidate_external(
                                harbor_widget::effects::ExternalInvalidation::new(),
                            )
                            .wait,
                    );
                    let allocation_wait = self.sync_terminal_allocation();
                    Self::merge_wait(&mut wait, allocation_wait);
                }
                if outcome.request_active_invalidation
                    && let Some(invalidation) =
                        external_invalidation_for_app_event(&AppEvent::TerminalOutputReady(tab_id))
                {
                    Self::merge_wait(
                        &mut wait,
                        self.main_host.invalidate_external(invalidation).wait,
                    );
                }
            }
            #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
            AppEvent::WidgetReloadAboutToStart(blocker) => {
                tracing::info!("unmounting application UI for hot reload");
                Self::merge_wait(&mut wait, self.main_host.unmount_root_for_reload().wait);
                // Releasing the loader only after Host-owned Runtime teardown prevents stale callbacks.
                drop(blocker);
            }
            #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
            AppEvent::WidgetReloaded => {
                Self::merge_wait(&mut wait, self.install_reloaded_root());
            }
        }
        if let Some(wait) = wait {
            apply_control_flow(event_loop, wait);
        }
    }

    #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
    fn install_reloaded_root(&mut self) -> Option<ControlFlowEffect> {
        let root = match build_application_root(self.root_inputs.clone()) {
            Ok(root) => root,
            Err(error) => {
                tracing::error!(error = %error, "failed to install reloaded application UI");
                return None;
            }
        };
        let installed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.main_host.replace_root_for_reload(root)
        }));
        let mut wait = match installed {
            Ok(outcome) => outcome.wait,
            Err(panic) => {
                let error = ShellError::HotReload(panic_message(panic));
                tracing::error!(error = %error, "reloaded application UI build failed");
                return self.main_host.unmount_root_for_reload().wait;
            }
        };
        let focus = self.tabs.terminal_focus();
        Self::merge_wait(&mut wait, self.main_host.request_focus(&focus).wait);
        let allocation_wait = self.sync_terminal_allocation();
        Self::merge_wait(&mut wait, allocation_wait);
        tracing::info!("installed reloaded application UI");
        wait
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop, host_deadline: Option<Instant>) {
        let now = Instant::now();
        let mut combined_flow = self
            .main_host
            .about_to_wait(now, host_deadline)
            .wait
            .unwrap_or(ControlFlowEffect::Wait);
        if let Some(wait) = self.sync_terminal_allocation() {
            combined_flow = combined_flow.arbitrate(wait);
        }

        if let Some(confirmation_flow) = self.paste.about_to_wait(now) {
            combined_flow = combined_flow.arbitrate(confirmation_flow);
        }

        if let Some(deadline) = self.startup_retry_deadline {
            if now >= deadline {
                self.startup_retry_deadline = None;
                if let Some(wait) = self.main_host.request_frame().wait {
                    combined_flow = combined_flow.arbitrate(wait);
                }
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
            match self
                .paste
                .handle_dialog_event(&event, self.tabs.active_terminal().as_ref())
            {
                PasteEventOutcome::Handled {
                    request_redraw,
                    wait,
                } => {
                    let mut wait = wait;
                    if request_redraw {
                        Self::merge_wait(&mut wait, self.main_host.request_frame().wait);
                    }
                    if let Some(wait) = wait {
                        apply_control_flow(event_loop, wait);
                    }
                }
                PasteEventOutcome::Fatal { error, wait } => {
                    if let Some(wait) = wait {
                        apply_control_flow(event_loop, wait);
                    }
                    tracing::error!(?error, "fatal confirmation-window frame error");
                    event_loop.exit();
                }
            }
            return;
        }

        let gate_active = self.paste.is_active();
        if self.main_host.window_id() != window_id {
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
                self.main_host.quarantine_blocked_pointer_event(&event);
                return;
            }
        }

        if matches!(&event, WindowEvent::CloseRequested) {
            tracing::info!("close requested");
            event_loop.exit();
            return;
        }

        if is_paste_shortcut(&event, self.main_host.modifiers()) {
            let active_terminal = self.tabs.active_terminal();
            let outcome = self.paste.paste_from_clipboard(
                event_loop,
                &mut self.main_host,
                active_terminal.as_ref(),
            );
            if let Some(wait) = outcome.wait {
                apply_control_flow(event_loop, wait);
            }
            return;
        }

        let mut wait = None;
        if self.tabs.update_presentation(self.main_host.window()) {
            Self::merge_wait(
                &mut wait,
                self.main_host
                    .invalidate_external(harbor_widget::effects::ExternalInvalidation::new())
                    .wait,
            );
        }

        let outcome = self.main_host.handle_window_event(&event);
        Self::merge_wait(&mut wait, outcome.wait);
        if !outcome.external_input.is_empty() {
            // Main terminal bridges consume input in-tree; queued events indicate a broken root contract.
            tracing::error!(
                count = outcome.external_input.len(),
                "unexpected deferred main-window external input"
            );
        }
        let allocation_wait = self.sync_terminal_allocation();
        Self::merge_wait(&mut wait, allocation_wait);
        if outcome.handled {
            let tab_outcome = self.tabs.drain_tab_commands(&mut self.main_host);
            Self::merge_wait(&mut wait, tab_outcome.wait);
            if tab_outcome.close_window {
                event_loop.exit();
                return;
            }
        }

        if let Some(frame_outcome) = outcome.frame {
            tracing::trace!("redraw requested");
            self.handle_frame_outcome(event_loop, frame, &frame_outcome);
        }
        if let Some(wait) = wait {
            apply_control_flow(event_loop, wait);
        }
    }

    fn handle_frame_outcome(
        &mut self,
        event_loop: &ActiveEventLoop,
        frame: &mut FrameState,
        outcome: &HostFrameOutcome,
    ) -> bool {
        let policy = main_frame_policy(outcome, self.show_pending);
        if policy.presented {
            frame.mark_first_present();
            let _ = frame.next_steady_state_deadline();
        }
        if policy.show_window {
            self.main_host.window().set_visible(true);
            self.show_pending = false;
            self.startup_retry_deadline = None;
        }
        if policy.fatal
            && let HostFrameOutcome::Fatal { error, .. } = outcome
        {
            tracing::error!(?error, "fatal main-window frame error");
            event_loop.exit();
        }
        if policy.schedule_hidden_retry {
            self.startup_retry_deadline = Some(Instant::now() + HIDDEN_STARTUP_RETRY_DELAY);
        }
        policy.presented
    }
}

// ── Shell (own methods) ───────────────────────────────────────────────────
impl Shell {
    /// Creates the application shell with no initial window, GPU, or terminal.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Result<Self, ShellError> {
        #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
        crate::hot_reload::spawn_observer(event_proxy.clone()).map_err(ShellError::HotReload)?;
        Ok(Self {
            session: None,
            frame: FrameState::new(),
            event_proxy,
        })
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
        let backdrop_style = harbor_config::WindowBackdropStyle::default();
        let backdrop_fallback = backdrop_style.fallback;
        let platform = MainWindowPlatformHooks::new(
            select_backend(os_build(), wasdk_available()),
            backdrop_style,
        );
        let font_settings = settings.font.clone();
        let shell_command = ShellCommand::new(settings.shell.program, settings.shell.args);
        let event_proxy = self.event_proxy.clone();

        let builder = WinitWindowHostBuilder::new(
            Window::default_attributes(),
            move |context: HostInitContext<'_>, _platform_state: &MainWindowPlatformState| {
                // Create DirectWrite objects on the UI/render owning thread (no font-loader thread).
                let fonts = load_system_fonts(&font_settings)?;
                let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
                let surface = context.surface();
                let size = Terminal::terminal_size_for(surface.physical_size, &metrics);
                let input_gate = Arc::new(AtomicBool::new(false));
                let factory = TerminalTabFactory::new(
                    Arc::clone(context.shared_gpu()),
                    surface.format,
                    shell_command,
                    font_settings,
                    metrics,
                    appearance,
                    context.backdrop_available(),
                    event_proxy,
                    Arc::clone(&input_gate),
                );
                let mut tabs = TabManager::new();
                tabs.create_tab(|tab_id, draw_id| {
                    factory.create_resources(tab_id, draw_id, size, surface.physical_size)
                })?;

                tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
                let active_bridge = tabs
                    .active_bridge()
                    .expect("initial tab creation establishes an active bridge");
                let tab_ui = TabUiController::new(
                    tabs.snapshots(),
                    active_bridge,
                    logical_window_width(context.window()),
                );
                let root_inputs = MainWindowRootInputs::new(
                    tab_ui.clone(),
                    context.backdrop_available(),
                    backdrop_fallback,
                );
                let root = build_application_root(root_inputs.clone())
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let tabs = TabCoordinator::new(tabs, tab_ui, factory);
                let paste = PasteController::new(input_gate);
                Ok((root, (tabs, paste, root_inputs)))
            },
        )
        .with_platform_hooks(platform);

        let (main_host, (tabs, paste, root_inputs)) =
            pollster::block_on(builder.build_with_output(event_loop))?;
        #[cfg(not(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions)))]
        let _ = root_inputs;
        let mut session = ActiveSession {
            paste,
            tabs,
            #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
            root_inputs,
            show_pending: true,
            startup_retry_deadline: None,
            main_host,
        };

        let mut wait = None;
        let focus = session.tabs.terminal_focus();
        ActiveSession::merge_wait(&mut wait, session.main_host.request_focus(&focus).wait);
        let allocation_wait = session.sync_terminal_allocation();
        ActiveSession::merge_wait(&mut wait, allocation_wait);
        let frame_outcome = session.main_host.present_now();
        ActiveSession::merge_wait(&mut wait, frame_outcome.wait());
        session.handle_frame_outcome(event_loop, &mut self.frame, &frame_outcome);
        if let Some(wait) = wait {
            apply_control_flow(event_loop, wait);
        }
        self.session = Some(session);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presented_frame_shows_only_a_pending_window() {
        let outcome = HostFrameOutcome::Presented { wait: None };
        assert_eq!(
            main_frame_policy(&outcome, true),
            MainFramePolicy {
                presented: true,
                show_window: true,
                schedule_hidden_retry: false,
                fatal: false,
            }
        );
        assert!(!main_frame_policy(&outcome, false).show_window);
    }

    #[test]
    fn skipped_hidden_frame_retries_without_showing() {
        let outcome = HostFrameOutcome::Skipped { wait: None };
        let policy = main_frame_policy(&outcome, true);
        assert!(!policy.presented);
        assert!(!policy.show_window);
        assert!(policy.schedule_hidden_retry);
        assert!(!policy.fatal);
    }

    #[test]
    fn recovery_waits_for_host_and_fatal_is_delegated_to_shell() {
        let recovery = HostFrameOutcome::RecoveryScheduled { wait: None };
        assert_eq!(
            main_frame_policy(&recovery, true),
            MainFramePolicy::default()
        );

        let fatal = HostFrameOutcome::Fatal {
            error: harbor_widget::winit::FrameError::out_of_memory(),
            wait: None,
        };
        assert!(main_frame_policy(&fatal, true).fatal);
    }
}
