//! Application shell: winit lifecycle, window bootstrap, frame render.

#[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
use std::{cell::RefCell, rc::Rc};
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
use crate::dialog::{ConfirmationCoordinator, PasteResolution};
use crate::effects::{apply_control_flow, merge_wait};
use crate::event::{AppEvent, external_invalidation_for_app_event};
use crate::tab_coordinator::{
    TabCoordinator, TerminalTabFactory, format_window_title, logical_window_width,
};
use crate::telemetry::{FrameState, HIDDEN_STARTUP_RETRY_DELAY};
use harbor_app::command::{AppCommand, resolve_keybindings};
use harbor_app::tab_manager::TabManager;
use harbor_app::workspace::MainWindowRootInputs;
use harbor_app::workspace::WorkspaceUiController;
#[cfg(not(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions)))]
use harbor_app::workspace::main_window_root;
use harbor_terminal::{Terminal, TerminalAppearance, TextMetrics, load_system_fonts};
use harbor_widget::effects::ControlFlowEffect;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::winit::{
    HostFrameOutcome, HostInitContext, HostStartupError, WinitWindowHost, WinitWindowHostBuilder,
};

mod configuration;
mod pane_commands;

#[cfg(test)]
pub(crate) use configuration::{
    ConfigurationOperationFailure, accept_configuration, reload_configuration_from_path,
};

/// Active session resources that exist while the window and renderer are alive.
pub(crate) struct ActiveSession {
    // Business state drops before the host-owned Runtime/surface/platform/window/GPU stack.
    confirmations: ConfirmationCoordinator,
    foreground_monitor: crate::clipboard::foreground_monitor::ForegroundMonitor,
    config: configuration::ConfigurationCoordinator,
    quick_save_transaction: Option<crate::profile_save::SaveTransaction>,
    tabs: TabCoordinator,
    pending_split: Option<pane_commands::PendingSplit>,
    workspace_ui: WorkspaceUiController,
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
            crate::hot_reload::build_root(inputs)
        }))
        .map(ApplicationRoot)
        .map_err(|_| ShellError::HotReload("reloadable UI root factory panicked".to_owned()))
    }

    #[cfg(not(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions)))]
    {
        Ok(ApplicationRoot(Box::new(main_window_root(inputs))))
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

/// Application-owned device-fatal policy shared by frame, wake and idle consumers.
/// The exit hook keeps policy testable without constructing an EventLoop or PTY.
fn apply_gpu_fatal_policy(
    fault: &harbor_widget::winit::GpuFault,
    confirmations: &mut ConfirmationCoordinator,
    exit: impl FnOnce(),
) {
    confirmations.cancel_for_shutdown();
    tracing::error!(%fault, "fatal shared GPU error");
    exit();
}

// ── ActiveSession (active window lifecycle) ───────────────────────────────
impl ActiveSession {
    fn cancel_clipboard_confirmation(&mut self, wait: &mut Option<ControlFlowEffect>) -> bool {
        if !self.confirmations.cancel_clipboard(&self.workspace_ui) {
            return false;
        }
        if !self.workspace_ui.popup_is_open() {
            merge_wait(
                wait,
                self.main_host
                    .request_focus(&self.workspace_ui.terminal_focus())
                    .wait,
            );
        }
        merge_wait(wait, self.main_host.request_frame().wait);
        true
    }

    fn exit_gpu_fault(
        &mut self,
        event_loop: &ActiveEventLoop,
        fault: harbor_widget::winit::GpuFault,
    ) {
        self.workspace_ui.close_active_confirmation();
        // Cancel before exit: later queued input must never commit a pending approval.
        apply_gpu_fatal_policy(&fault, &mut self.confirmations, || event_loop.exit());
    }

    fn stop_for_gpu_fault(&mut self, event_loop: &ActiveEventLoop) -> bool {
        if let Some(fault) = self.main_host.gpu_fault() {
            self.exit_gpu_fault(event_loop, fault);
            return true;
        }
        event_loop.exiting()
    }

    fn clipboard_eligible(
        &self,
        source: harbor_app::tab_manager::SessionId,
        check_epoch: bool,
    ) -> bool {
        if self.pending_split.is_some() {
            return false;
        }
        if check_epoch
            && !crate::clipboard::foreground_monitor::uninterrupted(
                self.foreground_monitor.available(),
                self.confirmations.clipboard_epoch(),
                self.foreground_monitor.epoch(),
            )
        {
            return false;
        }
        crate::clipboard::Eligibility {
            active_live: self.tabs.active_live(source),
            minimized: self.main_host.window().is_minimized() == Some(true),
            main_foreground: crate::clipboard::foreground(self.main_host.window()),
        }
        .permits()
    }

    #[must_use]
    fn reconcile_clipboard(&mut self) -> Option<ControlFlowEffect> {
        let mut wait = None;
        let paste_displaced = self
            .confirmations
            .paste_generation()
            .is_some_and(|generation| !self.workspace_ui.is_confirmation_current(generation));
        let paste_target_lost = self.confirmations.paste_source().is_some_and(|source| {
            self.tabs.active_binding() != Some(source) || !self.tabs.active_live(source.session)
        });
        if paste_displaced || paste_target_lost {
            if let Some(completion) = self.confirmations.retire_paste_target(&self.workspace_ui) {
                self.workspace_ui
                    .report_palette_completion(AppCommand::Paste, completion);
            }
            if !self.workspace_ui.popup_is_open() {
                merge_wait(
                    &mut wait,
                    self.main_host
                        .request_focus(&self.workspace_ui.terminal_focus())
                        .wait,
                );
            }
            merge_wait(&mut wait, self.main_host.request_frame().wait);
        }
        let clipboard_displaced = self
            .confirmations
            .clipboard_generation()
            .is_some_and(|generation| !self.workspace_ui.is_confirmation_current(generation));
        let clipboard_ineligible = self
            .confirmations
            .clipboard_source()
            .is_some_and(|source| !self.clipboard_eligible(source, true));
        if clipboard_displaced || clipboard_ineligible {
            self.cancel_clipboard_confirmation(&mut wait);
        }
        wait
    }

    fn handle_clipboard_request(
        &mut self,
        request: harbor_app::tab_manager::SessionClipboardWrite,
    ) -> Option<ControlFlowEffect> {
        use crate::clipboard::Admission;
        let eligible = self.clipboard_eligible(request.source, false);
        match self.confirmations.clipboard_admission(eligible) {
            Admission::Denied => {
                tracing::debug!("OSC 52 write denied");
                None
            }
            Admission::Write => {
                // Revalidate immediately at the existing native effect boundary.
                if self.clipboard_eligible(request.source, false) {
                    self.apply_clipboard_write(request, false);
                }
                None
            }
            Admission::Confirm => {
                let foreground_available = self.foreground_monitor.available();
                let current_epoch = self.foreground_monitor.epoch();
                let focus = self.confirmations.open_clipboard_confirmation(
                    &self.workspace_ui,
                    foreground_available,
                    current_epoch,
                    request,
                )?;
                self.quick_save_transaction = None;
                let mut wait = self.main_host.cancel_active_input_ownership().wait;
                merge_wait(&mut wait, self.main_host.request_focus(&focus).wait);
                merge_wait(&mut wait, self.main_host.request_frame().wait);
                wait
            }
        }
    }

    fn apply_clipboard_write(
        &mut self,
        request: harbor_app::tab_manager::SessionClipboardWrite,
        check_epoch: bool,
    ) {
        let eligible = self.main_host.gpu_fault().is_none()
            && self.clipboard_eligible(request.source, check_epoch);
        if crate::clipboard::execute(request, eligible, |text| {
            self.main_host.write_clipboard_checked(text).map(|_| ())
        })
        .is_err()
        {
            tracing::warn!("OSC 52 clipboard effect failed");
        }
    }

    fn sync_terminal_allocation(&mut self) {
        let viewport = self.main_host.viewport().clone();
        self.tabs
            .apply_pending_allocation(viewport.scale_factor, viewport.physical_size);
    }

    fn service_and_verify_active_source(
        &mut self,
        event_loop: &ActiveEventLoop,
        wait: &mut Option<ControlFlowEffect>,
    ) -> bool {
        let source = self.tabs.active_binding();
        if let Some(source) = source {
            merge_wait(
                wait,
                self.process_terminal_output(event_loop, source.session),
            );
        }
        source.is_some_and(|source| {
            self.tabs.active_binding() == Some(source) && self.tabs.active_live(source.session)
        })
    }

    fn begin_quick_save(&mut self) -> Result<harbor_widget::FocusHandle, String> {
        let draft = self.tabs.capture_quick_save().map_err(str::to_owned)?;
        let path = self
            .config
            .path()
            .cloned()
            .ok_or("Selected configuration path unavailable")?;
        let transaction = crate::profile_save::SaveTransaction::begin(path)?;
        self.quick_save_transaction = Some(transaction);
        Ok(self.workspace_ui.open_quick_save(draft))
    }

    fn commit_quick_save(
        &mut self,
        request: harbor_app::command::AppCommandRequest,
        wait: &mut Option<ControlFlowEffect>,
    ) -> Option<harbor_widget::FocusHandle> {
        let submission = self.workspace_ui.take_quick_save_submission(request)?;
        // A previous unaccepted reload must never publish over the save generation.
        self.config
            .supersede_pending(&self.workspace_ui, "Rejected: reload superseded by save");
        let result = (|| -> Result<_, String> {
            let transaction = self
                .quick_save_transaction
                .as_mut()
                .ok_or("Save source expired; reopen save")?;
            let loaded = transaction.commit(&submission.profile)?;
            let bytes = transaction
                .committed_bytes()
                .ok_or("Save durability was not established")?;
            Ok((loaded, transaction.path().to_owned(), bytes))
        })();
        match result {
            Ok((loaded, path, bytes)) => match self.config.begin_quick_save_publish(
                &mut self.tabs,
                &self.workspace_ui,
                &mut self.main_host,
                loaded,
                path,
                (submission.generation, bytes),
            ) {
                Ok(reload_wait) => {
                    merge_wait(wait, reload_wait);
                    None
                }
                Err(error) => self
                    .workspace_ui
                    .finish_quick_save(submission.generation, Err(error)),
            },
            Err(error) => self
                .workspace_ui
                .finish_quick_save(submission.generation, Err(error)),
        }
    }

    fn drain_app_commands(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> crate::tab_coordinator::TabDrainOutcome {
        let mut result = crate::tab_coordinator::TabDrainOutcome::default();
        use crate::tab_coordinator::AppActionAdmission;
        let mut actions = self.workspace_ui.drain_actions().into_iter();
        while let Some(admission) = self
            .tabs
            .next_app_action(&mut actions, self.confirmations.is_active())
        {
            let (request, from_palette, profile) = match admission {
                AppActionAdmission::Rejected {
                    request,
                    reason,
                    restore_terminal_focus,
                } => {
                    self.workspace_ui.report_palette_result(
                        request.command,
                        Err(&format!("Rejected: {reason}")),
                    );
                    if restore_terminal_focus {
                        merge_wait(
                            &mut result.wait,
                            self.main_host
                                .request_focus(&self.workspace_ui.terminal_focus())
                                .wait,
                        );
                    }
                    merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                    continue;
                }
                AppActionAdmission::Execute {
                    request,
                    clear_palette_focus,
                    profile,
                } => {
                    if clear_palette_focus {
                        merge_wait(&mut result.wait, self.main_host.clear_focus().wait);
                    }
                    (request, clear_palette_focus, profile)
                }
            };
            if self.pending_split.is_some()
                && !matches!(
                    request.command,
                    AppCommand::Tab(_) | AppCommand::CancelSplit(_)
                )
            {
                if from_palette {
                    self.workspace_ui.report_palette_result(
                        request.command,
                        Err("Rejected: split startup is pending"),
                    );
                }
                continue;
            }
            match request.command {
                AppCommand::Pane(command) => {
                    use harbor_app::command::PaneCommand;
                    if matches!(
                        command,
                        PaneCommand::SplitHorizontal | PaneCommand::SplitVertical
                    ) {
                        // Servicing queued metadata may also reduce exact shell exit.
                        // Never turn that reduction into a split of the survivor.
                        let completion = if self
                            .service_and_verify_active_source(event_loop, &mut result.wait)
                        {
                            self.begin_pane_split(command, from_palette)
                        } else {
                            Err("Rejected: original split source retired")
                        };
                        if let Err(reason) = completion {
                            self.workspace_ui
                                .report_palette_result(request.command, Err(reason));
                        }
                        merge_wait(&mut result.wait, self.advance_pane_split());
                    } else {
                        let outcome = self.tabs.execute_pane_command(&mut self.main_host, command);
                        merge_wait(&mut result.wait, outcome.wait);
                        merge_wait(&mut result.wait, self.reconcile_clipboard());
                        if from_palette {
                            self.workspace_ui
                                .report_palette_result(request.command, Ok(()));
                        }
                        if outcome.close_window {
                            result.close_window = true;
                            break;
                        }
                    }
                }
                AppCommand::CancelSplit(target) => {
                    if let Some(pending) = self.pending_split.as_mut()
                        && pending.operation.target().0 == target
                    {
                        pending.operation.cancel();
                        merge_wait(&mut result.wait, self.advance_pane_split());
                    }
                }
                AppCommand::OpenCommandPalette => {
                    let focus = self.workspace_ui.open_palette();
                    merge_wait(&mut result.wait, self.main_host.request_focus(&focus).wait);
                }
                AppCommand::SaveCurrentSession => {
                    let capture =
                        if self.service_and_verify_active_source(event_loop, &mut result.wait) {
                            self.begin_quick_save()
                        } else {
                            Err("Rejected: original capture source retired".into())
                        };
                    match capture {
                        Ok(focus) => {
                            merge_wait(&mut result.wait, self.main_host.request_focus(&focus).wait)
                        }
                        Err(error) => self
                            .workspace_ui
                            .report_palette_result(request.command, Err(&error)),
                    }
                    merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                }
                AppCommand::AdvanceQuickSave | AppCommand::EditQuickSave => {
                    if let Some(focus) = self.workspace_ui.navigate_quick_save(request) {
                        // Only an admitted current-generation edit may refresh disk conflict state.
                        if request.command == AppCommand::EditQuickSave
                            && let Some(transaction) = self.quick_save_transaction.as_mut()
                            && let Err(error) = transaction.refresh()
                        {
                            self.workspace_ui
                                .report_palette_result(request.command, Err(&error));
                        }
                        merge_wait(&mut result.wait, self.main_host.request_focus(&focus).wait);
                    }
                    merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                }
                AppCommand::CommitQuickSave => {
                    if let Some(focus) = self.commit_quick_save(request, &mut result.wait) {
                        merge_wait(&mut result.wait, self.main_host.request_focus(&focus).wait);
                    }
                    merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                }
                AppCommand::CancelQuickSave => {
                    if self.workspace_ui.cancel_quick_save(request) {
                        self.quick_save_transaction = None;
                        merge_wait(
                            &mut result.wait,
                            self.main_host
                                .request_focus(&self.workspace_ui.terminal_focus())
                                .wait,
                        );
                        merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                    }
                }
                AppCommand::ConfirmConfirmation | AppCommand::CancelConfirmation => {
                    let harbor_app::command::CommandSource::Popup { generation } = request.source
                    else {
                        continue;
                    };
                    if let Some(fault) = self.main_host.gpu_fault() {
                        self.exit_gpu_fault(event_loop, fault);
                        result.close_window = true;
                        break;
                    }
                    let confirmed = matches!(request.command, AppCommand::ConfirmConfirmation);
                    if self.confirmations.paste_generation() == Some(generation) {
                        match self.confirmations.resolve_paste(
                            generation,
                            confirmed,
                            &self.workspace_ui,
                            self.tabs.active_terminal().as_ref(),
                            self.tabs.active_binding(),
                        ) {
                            PasteResolution::Ignored => {}
                            PasteResolution::Resolved { palette_completion } => {
                                if let Some(completion) = palette_completion {
                                    self.workspace_ui
                                        .report_palette_completion(AppCommand::Paste, completion);
                                }
                                merge_wait(
                                    &mut result.wait,
                                    self.main_host
                                        .request_focus(&self.workspace_ui.terminal_focus())
                                        .wait,
                                );
                                merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                            }
                        }
                    } else if self.confirmations.clipboard_generation() == Some(generation) {
                        let eligible = self
                            .confirmations
                            .clipboard_source()
                            .is_some_and(|source| self.clipboard_eligible(source, true));
                        if let Some(write_request) = self.confirmations.resolve_clipboard(
                            generation,
                            confirmed,
                            eligible,
                            &self.workspace_ui,
                        ) {
                            self.apply_clipboard_write(write_request, true);
                        } else if !confirmed || !eligible {
                            tracing::debug!("OSC 52 confirmation denied or invalidated");
                        }
                        merge_wait(
                            &mut result.wait,
                            self.main_host
                                .request_focus(&self.workspace_ui.terminal_focus())
                                .wait,
                        );
                        merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                    }
                }
                AppCommand::OpenProfileChooser => {
                    let focus = self.workspace_ui.open_profile_chooser();
                    merge_wait(&mut result.wait, self.main_host.request_focus(&focus).wait);
                }
                AppCommand::BackToCommands(generation) => {
                    if let Some(focus) = self.workspace_ui.back_to_commands(generation) {
                        merge_wait(&mut result.wait, self.main_host.request_focus(&focus).wait);
                    }
                }
                AppCommand::LaunchProfile => {
                    // Only admitted palette requests carry this generation-scoped intent.
                    let intent = profile.expect("admitted profile launch");
                    let outcome = self
                        .tabs
                        .execute_profile_launch(&mut self.main_host, intent);
                    self.workspace_ui.report_palette_result(
                        request.command,
                        if outcome.failed {
                            Err("Failed: see session creation error")
                        } else {
                            Ok(())
                        },
                    );
                    if outcome.failed {
                        merge_wait(
                            &mut result.wait,
                            self.main_host
                                .request_focus(&self.workspace_ui.terminal_focus())
                                .wait,
                        );
                    }
                    merge_wait(&mut result.wait, outcome.wait);
                    merge_wait(&mut result.wait, self.reconcile_clipboard());
                }
                AppCommand::CloseCommandPalette(generation) => {
                    if self.workspace_ui.close_palette(generation) {
                        merge_wait(
                            &mut result.wait,
                            self.main_host
                                .request_focus(&self.workspace_ui.terminal_focus())
                                .wait,
                        );
                    }
                }
                AppCommand::SearchOpen
                | AppCommand::SearchClose
                | AppCommand::SearchChanged
                | AppCommand::SearchNext
                | AppCommand::SearchPrevious => {
                    let outcome = self.tabs.dispatch_search_command(request.command);
                    let focus = outcome.focus_for_source(request.source, &self.workspace_ui);
                    if from_palette {
                        self.workspace_ui
                            .report_palette_completion(request.command, outcome.completion);
                    }
                    merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                    if let Some(focus) = focus {
                        merge_wait(&mut result.wait, self.main_host.request_focus(&focus).wait);
                    }
                }
                AppCommand::DismissNotification(revision) => {
                    self.workspace_ui.dismiss_notification(revision);
                    merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                }
                AppCommand::ReloadConfiguration => {
                    let (completion, reload_wait) = self.config.reload(
                        &mut self.tabs,
                        &self.workspace_ui,
                        &mut self.main_host,
                        false,
                        self.pending_split.is_some(),
                    );
                    merge_wait(&mut result.wait, reload_wait);
                    if from_palette {
                        if !self.config.set_report_palette() {
                            self.workspace_ui.report_palette_result(
                                request.command,
                                match completion {
                                    Some(true) => Ok(()),
                                    Some(false) => Err("Failed: see configuration reload error"),
                                    None => {
                                        Err("Rejected: configuration file is absent or unavailable")
                                    }
                                },
                            );
                        }
                        merge_wait(
                            &mut result.wait,
                            self.main_host
                                .request_focus(&self.workspace_ui.terminal_focus())
                                .wait,
                        );
                    }
                    merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                }
                AppCommand::Tab(command) => {
                    let outcome =
                        self.tabs
                            .execute_tab_command(&mut self.main_host, command, request.focus);

                    if from_palette {
                        self.workspace_ui.report_palette_result(
                            request.command,
                            if outcome.failed {
                                Err("Failed: see session creation error")
                            } else {
                                Ok(())
                            },
                        );
                        if outcome.failed {
                            merge_wait(
                                &mut result.wait,
                                self.main_host
                                    .request_focus(&self.workspace_ui.terminal_focus())
                                    .wait,
                            );
                        }
                    }
                    merge_wait(&mut result.wait, outcome.wait);
                    merge_wait(&mut result.wait, self.reconcile_clipboard());
                    if outcome.close_window {
                        result.close_window = true;
                        break;
                    }
                }
                AppCommand::Copy
                | AppCommand::CopyOrInterrupt
                | AppCommand::PageUp
                | AppCommand::PageDown
                | AppCommand::ScrollToTop
                | AppCommand::ScrollToBottom => {
                    use harbor_app::command::CommandCompletion;
                    let completion = self
                        .tabs
                        .dispatch_terminal_command(request.command)
                        .and_then(|outcome| {
                            let completion = if let Some(text) = outcome.clipboard_text {
                                self.main_host
                                    .write_clipboard_checked(text)
                                    .map(|effect| {
                                        merge_wait(&mut result.wait, effect.wait);
                                        CommandCompletion::Applied
                                    })
                                    .map_err(|error| {
                                        tracing::warn!(?error, "failed to copy terminal selection");
                                        "Failed: clipboard write unavailable"
                                    })
                            } else {
                                Ok(if outcome.redraw {
                                    CommandCompletion::Applied
                                } else {
                                    CommandCompletion::Unchanged
                                })
                            };
                            if outcome.release_pointer.is_some() {
                                merge_wait(
                                    &mut result.wait,
                                    self.main_host.cancel_active_input_ownership().wait,
                                );
                            }
                            if outcome.redraw {
                                merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                            }
                            completion
                        });
                    if from_palette {
                        self.workspace_ui
                            .report_palette_completion(request.command, completion);
                        merge_wait(
                            &mut result.wait,
                            self.main_host
                                .request_focus(&self.workspace_ui.terminal_focus())
                                .wait,
                        );
                        merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                    }
                }
                AppCommand::Paste => {
                    use harbor_app::command::CommandCompletion;
                    let active_terminal = self.tabs.active_terminal();
                    let completion = self
                        .confirmations
                        .paste_from_clipboard(
                            &self.workspace_ui,
                            &mut self.main_host,
                            active_terminal.as_ref(),
                            self.tabs.active_binding(),
                            from_palette,
                        )
                        .map(|(wait, completion)| {
                            merge_wait(&mut result.wait, wait);
                            completion
                        });
                    if let Err(reason) = completion {
                        tracing::warn!(%reason, "clipboard paste rejected or failed");
                    }
                    if from_palette {
                        self.workspace_ui
                            .report_palette_completion(request.command, completion);
                        if !matches!(completion, Ok(CommandCompletion::PendingConfirmation))
                            && !self.workspace_ui.popup_is_open()
                        {
                            merge_wait(
                                &mut result.wait,
                                self.main_host
                                    .request_focus(&self.workspace_ui.terminal_focus())
                                    .wait,
                            );
                        }
                        merge_wait(&mut result.wait, self.main_host.request_frame().wait);
                    }
                }
            }
            if event_loop.exiting() {
                result.close_window = true;
                break;
            }
        }
        result
    }

    fn handle_user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        if self.stop_for_gpu_fault(event_loop) {
            return;
        }
        let mut wait = self.reconcile_clipboard();
        match event {
            AppEvent::GpuWake => {} // Persistent Host observation above; no redraw needed.
            AppEvent::TerminalOutputReady(tab_id) => {
                merge_wait(&mut wait, self.process_terminal_output(event_loop, tab_id));
            }
            AppEvent::ConfigurationWatch => {
                let watch_wait = self.config.handle_watch_event(
                    &mut self.tabs,
                    &self.workspace_ui,
                    &mut self.main_host,
                    self.pending_split.is_some(),
                );
                merge_wait(&mut wait, watch_wait);
            }
            AppEvent::OpenHyperlink(source, uri) => {
                if self.tabs.active_live(source)
                    && let Err(error) = crate::hyperlink::open(&uri)
                {
                    tracing::warn!(
                        uri,
                        error = %format_args!("{error:#}"),
                        "failed to open terminal hyperlink"
                    );
                }
            }
            #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
            AppEvent::WidgetHostWork(work) => {
                merge_wait(&mut wait, self.main_host.handle_hmr_work(work).wait);
                let focus = self.workspace_ui.terminal_focus();
                merge_wait(&mut wait, self.main_host.request_focus(&focus).wait);
                self.sync_terminal_allocation();
            }
        }
        if let Some(wait) = wait {
            apply_control_flow(event_loop, wait);
        }
    }

    fn process_terminal_output(
        &mut self,
        event_loop: &ActiveEventLoop,
        tab_id: harbor_app::tab_manager::SessionId,
    ) -> Option<ControlFlowEffect> {
        let mut wait = None;
        let outcome = self.tabs.process_output(tab_id);
        if let Some(request) = outcome.clipboard_write {
            merge_wait(&mut wait, self.handle_clipboard_request(request));
        }
        if outcome.unread_changed || outcome.title_changed {
            if outcome.active_title_changed {
                self.tabs.sync_ui(self.main_host.window());
            } else {
                self.tabs.sync_tab_ui(self.main_host.window());
            }
            merge_wait(
                &mut wait,
                self.main_host
                    .invalidate_external(harbor_widget::effects::ExternalInvalidation::new())
                    .wait,
            );
            self.sync_terminal_allocation();
        }
        if outcome.request_active_invalidation
            && let Some(invalidation) =
                external_invalidation_for_app_event(&AppEvent::TerminalOutputReady(tab_id))
        {
            merge_wait(
                &mut wait,
                self.main_host.invalidate_external(invalidation).wait,
            );
        }
        let closed = self.tabs.close_exited_session(tab_id);
        if closed != Default::default() {
            let outcome = self.tabs.apply_tab_outcome(
                &mut self.main_host,
                closed,
                if closed.active_bridge_changed {
                    harbor_app::tab_view::TabFocusPolicy::Terminal
                } else {
                    harbor_app::tab_view::TabFocusPolicy::PreserveRail
                },
            );
            merge_wait(&mut wait, outcome.wait);
            merge_wait(&mut wait, self.reconcile_clipboard());
            if outcome.close_window {
                event_loop.exit();
            }
        }
        wait
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop, host_deadline: Option<Instant>) {
        if self.stop_for_gpu_fault(event_loop) {
            return;
        }
        let mut resize_wait = self.reconcile_clipboard();
        let now = Instant::now();
        for id in self.tabs.pending_resize_sessions() {
            merge_wait(
                &mut resize_wait,
                self.process_terminal_output(event_loop, id),
            );
        }
        if event_loop.exiting() {
            return;
        }
        merge_wait(&mut resize_wait, self.advance_pane_split());
        let reload_pending = self.config.has_pending();
        if self.pending_split.is_none() {
            let (_, reload_wait) =
                self.config
                    .advance(&mut self.tabs, &self.workspace_ui, &mut self.main_host);
            merge_wait(&mut resize_wait, reload_wait);
        }
        if reload_pending && !self.config.has_pending() {
            merge_wait(&mut resize_wait, self.main_host.request_frame().wait);
        }
        let idle = self.main_host.about_to_wait(now, host_deadline);
        if let Some(fault) = idle.fatal {
            self.exit_gpu_fault(event_loop, fault);
            return;
        }
        let mut combined_flow = idle.wait.unwrap_or(ControlFlowEffect::Wait);
        if let Some(wait) = resize_wait {
            combined_flow = combined_flow.arbitrate(wait);
        }
        self.sync_terminal_allocation();
        // Output, scheduling and allocation above only dispatch/submit managed work.
        // Admit one shared batch here, never a separate batch per callback or tab.
        let resized = self.tabs.advance_resizes();
        if resized.active_changed
            && let Some(wait) = self.main_host.request_frame().wait
        {
            combined_flow = combined_flow.arbitrate(wait);
        }
        if resized.ready_remaining {
            combined_flow = combined_flow.arbitrate(ControlFlowEffect::Poll);
        }
        // Allocation can start a new acquisition this turn. Observe its deadline
        // after submission, even if no reader wake/text/draw follows it.
        if let Some(deadline) = self.tabs.resize_deadline() {
            combined_flow = combined_flow.arbitrate(ControlFlowEffect::WaitUntil(deadline));
        }

        if let Some(deadline) = self
            .pending_split
            .as_ref()
            .and_then(|pending| pending.operation.next_poll(now))
        {
            combined_flow = combined_flow.arbitrate(ControlFlowEffect::WaitUntil(deadline));
        }
        let (search_pending, search_changed) = self.tabs.advance_search();
        if search_pending {
            combined_flow = combined_flow.arbitrate(ControlFlowEffect::Poll);
        }
        if search_changed && let Some(wait) = self.main_host.request_frame().wait {
            combined_flow = combined_flow.arbitrate(wait);
        }

        let (notices_expired, notice_deadline) = self.workspace_ui.advance_notifications(now);
        if notices_expired && let Some(wait) = self.main_host.request_frame().wait {
            combined_flow = combined_flow.arbitrate(wait);
        }
        if let Some(deadline) = notice_deadline {
            combined_flow = combined_flow.arbitrate(ControlFlowEffect::WaitUntil(deadline));
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
        if self.stop_for_gpu_fault(event_loop) || self.main_host.window_id() != window_id {
            return;
        }
        let mut wait = self.reconcile_clipboard();

        if matches!(&event, WindowEvent::CloseRequested) {
            tracing::info!("close requested");
            event_loop.exit();
            return;
        }

        if self.tabs.update_presentation(self.main_host.window()) {
            merge_wait(
                &mut wait,
                self.main_host
                    .invalidate_external(harbor_widget::effects::ExternalInvalidation::new())
                    .wait,
            );
        }

        let outcome = self.main_host.handle_window_event(&event);
        if let Some(fault) = outcome.fatal {
            self.exit_gpu_fault(event_loop, fault);
            return;
        }
        if self.stop_for_gpu_fault(event_loop) {
            return;
        }
        if matches!(&event, WindowEvent::Focused(false))
            && (self.workspace_ui.cancel_palette()
                || self.workspace_ui.cancel_quick_save_on_focus_loss())
        {
            // Native focus loss expires a queued selection, not just preedit/clipboard delivery.
            merge_wait(
                &mut wait,
                self.main_host
                    .request_focus(&self.workspace_ui.terminal_focus())
                    .wait,
            );
        }
        if matches!(
            &event,
            WindowEvent::Focused(false) | WindowEvent::Occluded(true)
        ) {
            self.cancel_clipboard_confirmation(&mut wait);
        }
        merge_wait(&mut wait, outcome.wait);
        if !outcome.external_input.is_empty() {
            // Main terminal bridges consume input in-tree; queued events indicate a broken root contract.
            tracing::error!(
                count = outcome.external_input.len(),
                "unexpected deferred main-window external input"
            );
        }
        self.sync_terminal_allocation();
        let command_outcome = self.drain_app_commands(event_loop);
        merge_wait(&mut wait, command_outcome.wait);
        if command_outcome.close_window {
            event_loop.exit();
            return;
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
    ) {
        if let HostFrameOutcome::Fatal {
            error: harbor_widget::winit::FrameError::SharedGpu(fault),
            ..
        } = outcome
        {
            self.exit_gpu_fault(event_loop, fault.clone());
            return;
        }
        let policy = main_frame_policy(outcome, self.show_pending);
        if policy.presented {
            frame.mark_first_present();
            let _ = frame.next_steady_state_deadline();
        }
        if policy.show_window {
            let window = self.main_host.window();
            window.set_visible(true);
            window.focus_window();
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
    }
}

// ── Shell (own methods) ───────────────────────────────────────────────────
impl Shell {
    /// Creates the application shell with no initial window, GPU, or terminal.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Result<Self, ShellError> {
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
        let (configuration_path, loaded) = harbor_config::load_with_path();
        let startup_path_label = configuration_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "configuration path unavailable".to_owned());
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
        let mut startup_notices = loaded
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>();
        let keybinding_resolution = resolve_keybindings(&loaded.settings.keybindings);
        for diagnostic in &keybinding_resolution.diagnostics {
            tracing::error!(message = %diagnostic, "keybinding fallback");
        }
        startup_notices.extend(keybinding_resolution.diagnostics.iter().cloned());
        let keybindings = keybinding_resolution.keybindings;
        let settings = loaded.settings;
        let clipboard_policy = settings.clipboard.osc52_write;
        let appearance = TerminalAppearance::from_palette(settings.colors);
        let backdrop_style = harbor_config::WindowBackdropStyle::default();
        let backdrop_fallback = backdrop_style.fallback;
        let platform = MainWindowPlatformHooks::new(
            select_backend(os_build(), wasdk_available()),
            backdrop_style,
        );
        let font_settings = settings.font.clone();
        let history = settings.history;
        let shell_command = crate::tab_coordinator::configured_shell(&settings.shell);
        let profiles = settings.profiles.clone();
        let event_proxy = self.event_proxy.clone();
        #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
        let hmr_root_inputs = Rc::new(RefCell::new(None::<MainWindowRootInputs>));
        #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
        let hmr = {
            let wake_proxy = event_proxy.clone();
            let factory_inputs = Rc::clone(&hmr_root_inputs);
            crate::hot_reload::config(
                move |work| {
                    wake_proxy
                        .send_event(AppEvent::WidgetHostWork(work))
                        .is_ok()
                },
                move || {
                    let inputs = factory_inputs
                        .borrow()
                        .as_ref()
                        .expect("initial root inputs installed before HMR activation")
                        .clone();
                    crate::hot_reload::build_root(inputs)
                },
            )
            .map_err(ShellError::HotReload)?
        };
        #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
        let bootstrap_hmr_inputs = Rc::clone(&hmr_root_inputs);

        let builder = WinitWindowHostBuilder::new(
            Window::default_attributes(),
            move |context: HostInitContext<'_>, _platform_state: &MainWindowPlatformState| {
                // Create DirectWrite objects on the UI/render owning thread (no font-loader thread).
                let fonts = load_system_fonts(&font_settings)?;
                let font_warnings = fonts
                    .selection_warning()
                    .map(str::to_owned)
                    .into_iter()
                    .collect::<Vec<_>>();
                startup_notices.extend(font_warnings.iter().cloned());
                let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
                let surface = context.surface();
                let size = Terminal::terminal_size_for(surface.physical_size, &metrics);
                let input_gate = Arc::new(AtomicBool::new(false));
                let factory = TerminalTabFactory::new(
                    Arc::clone(context.shared_gpu()),
                    surface.format,
                    shell_command,
                    profiles,
                    font_settings,
                    history,
                    font_warnings,
                    metrics,
                    appearance,
                    context.backdrop_available(),
                    event_proxy,
                    Arc::clone(&input_gate),
                    crate::clipboard::delivery(clipboard_policy),
                );
                let mut tabs = TabManager::new();
                tabs.create_tab(|tab_id, draw_id| {
                    factory.create_resources(tab_id, draw_id, size, surface.physical_size)
                })?;
                if let Some(title) = tabs.active_title() {
                    context.window().set_title(&format_window_title(title));
                }

                tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
                let active_bridge = tabs
                    .active_bridge()
                    .expect("initial tab creation establishes an active bridge");
                let workspace_ui = WorkspaceUiController::new(
                    tabs.snapshots(),
                    active_bridge,
                    logical_window_width(context.window()),
                );
                if !startup_notices.is_empty() {
                    workspace_ui.publish_configuration_notice(
                        harbor_app::notifications::ConfigurationNoticeSource::Startup,
                        Some(format!(
                            "Configuration startup ({startup_path_label}): {}",
                            startup_notices.join("; ")
                        )),
                    );
                }
                let root_inputs = MainWindowRootInputs::new(
                    workspace_ui.clone(),
                    context.backdrop_available(),
                    backdrop_fallback,
                    keybindings,
                );
                #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
                {
                    *bootstrap_hmr_inputs.borrow_mut() = Some(root_inputs.clone());
                }
                let root = build_application_root(root_inputs)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let tabs = TabCoordinator::new(tabs, workspace_ui.clone(), factory);
                Ok((root, (tabs, workspace_ui, input_gate)))
            },
        )
        .with_gpu_wake(harbor_widget::winit::GpuWake::new(self.event_proxy.clone()))
        .with_platform_hooks(platform);
        #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
        let builder = builder.with_hmr(hmr);

        let (main_host, (tabs, workspace_ui, input_gate)) =
            pollster::block_on(builder.build_with_output(event_loop))?;
        let mut session = ActiveSession {
            confirmations: ConfirmationCoordinator::new(input_gate, clipboard_policy),
            foreground_monitor: crate::clipboard::foreground_monitor::ForegroundMonitor::new(),
            config: configuration::ConfigurationCoordinator::new(configuration_path, settings),
            quick_save_transaction: None,
            tabs,
            pending_split: None,
            workspace_ui,
            show_pending: true,
            startup_retry_deadline: None,
            main_host,
        };

        session
            .config
            .start_watch(self.event_proxy.clone(), &session.workspace_ui);

        let mut wait = None;
        // The HWND may already be active without a Focused(true) event.
        let focus = session.workspace_ui.terminal_focus();
        merge_wait(&mut wait, session.main_host.request_focus(&focus).wait);
        session.sync_terminal_allocation();
        let frame_outcome = session.main_host.present_now();
        merge_wait(&mut wait, frame_outcome.wait());
        session.handle_frame_outcome(event_loop, &mut self.frame, &frame_outcome);
        if let Some(wait) = wait {
            apply_control_flow(event_loop, wait);
        }
        self.session = Some(session);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
