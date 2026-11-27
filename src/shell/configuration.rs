//! Configuration loading, watched/manual reload transactions, and acceptance notices.

use harbor_app::command::AppCommand;
use harbor_app::workspace::WorkspaceUiController;
use harbor_widget::effects::ControlFlowEffect;
use harbor_widget::winit::WinitWindowHost;
use winit::event_loop::EventLoopProxy;

use crate::effects::merge_wait;
use crate::event::AppEvent;
use crate::tab_coordinator::{ConfigurationAcquisition, TabCoordinator};

/// Separate diagnostics from the subset that needs explicit user attention.
pub(crate) struct ConfigurationAcceptance {
    pub summary: String,
    pub notice: Option<String>,
}

/// Typed application-effect failure, distinct from a rejected document. A missing
/// file is not resource recovery and must not erase compensation/error feedback.
#[derive(Debug)]
pub(crate) struct ConfigurationOperationFailure;

impl std::fmt::Display for ConfigurationOperationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("configuration application failed")
    }
}

/// Strict live I/O seam shared by manual and watched reload. Absence is a
/// non-acceptance outcome; the application transaction is not called at all.
#[cfg(test)]
pub(crate) fn reload_configuration_from_path(
    path: Option<&std::path::Path>,
    accepted: &mut harbor_config::Settings,
    effective_clipboard: &harbor_config::ClipboardSettings,
    apply: impl FnOnce(
        &harbor_config::Settings,
        harbor_app::command::ResolvedKeybindings,
    ) -> anyhow::Result<Vec<String>>,
) -> anyhow::Result<Option<ConfigurationAcceptance>> {
    let path = path.ok_or_else(|| anyhow::anyhow!("could not resolve configuration path"))?;
    let loaded = match harbor_config::load_candidate_from_path(path) {
        Ok(loaded) => loaded,
        Err(error) if error.io_kind == Some(std::io::ErrorKind::NotFound) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    apply_loaded_configuration(path, accepted, effective_clipboard, loaded, apply).map(Some)
}

fn apply_loaded_configuration(
    path: &std::path::Path,
    accepted: &mut harbor_config::Settings,
    effective_clipboard: &harbor_config::ClipboardSettings,
    loaded: harbor_config::SettingsLoad,
    apply: impl FnOnce(
        &harbor_config::Settings,
        harbor_app::command::ResolvedKeybindings,
    ) -> anyhow::Result<Vec<String>>,
) -> anyhow::Result<ConfigurationAcceptance> {
    let mut result = accept_configuration(
        accepted,
        effective_clipboard,
        loaded.settings,
        |candidate, bindings| {
            apply(candidate, bindings).map_err(|error| error.context(ConfigurationOperationFailure))
        },
    )?;
    let mut notices = result.notice.into_iter().collect::<Vec<_>>();
    for diagnostic in loaded.diagnostics {
        tracing::warn!(path = %path.display(), message = %diagnostic.message, "configuration warning");
        notices.push(diagnostic.message);
    }
    result.notice = (!notices.is_empty()).then(|| notices.join("; "));
    Ok(result)
}

/// Validates and publishes one requested generation on the application thread.
/// `apply` owns the real session/factory transaction and infallible binding move.
/// Keeping this independent of a Window makes acceptance ordering testable.
pub(crate) fn accept_configuration(
    accepted: &mut harbor_config::Settings,
    effective_clipboard: &harbor_config::ClipboardSettings,
    candidate: harbor_config::Settings,
    apply: impl FnOnce(
        &harbor_config::Settings,
        harbor_app::command::ResolvedKeybindings,
    ) -> anyhow::Result<Vec<String>>,
) -> anyhow::Result<ConfigurationAcceptance> {
    let bindings = harbor_app::command::validate_keybindings(&candidate.keybindings)
        .map_err(|errors| anyhow::anyhow!("{}", errors.join("; ")))?;
    let font_changed = candidate.font != accepted.font;
    let warnings = apply(&candidate, bindings)?;
    let mut live = Vec::new();
    if candidate.colors != accepted.colors {
        live.push("colors");
    }
    if candidate.keybindings != accepted.keybindings {
        live.push("keybindings");
    }
    if font_changed {
        live.push("terminal fonts");
    }
    if candidate.history != accepted.history {
        live.push("history");
    }
    let mut parts = Vec::new();
    if !live.is_empty() {
        parts.push(format!("applied live: {}", live.join(", ")));
    }
    let mut launch = Vec::new();
    if candidate.shell.program != accepted.shell.program
        || candidate.shell.args != accepted.shell.args
    {
        launch.push("shell program/arguments");
    }
    if candidate.shell.environment != accepted.shell.environment {
        launch.push("environment");
    }
    if candidate.shell.directory != accepted.shell.directory {
        launch.push("initial directory");
    }
    if candidate.profiles != accepted.profiles {
        launch.push("named profiles");
    }
    if !launch.is_empty() {
        parts.push(format!("new sessions only: {}", launch.join(", ")));
    }
    let mut actionable = warnings.clone();
    if candidate.clipboard != *effective_clipboard {
        actionable.push(
            "restart required: clipboard.osc52_write (unchanged for existing and new sessions)"
                .to_owned(),
        );
        parts.push(
            "restart required: clipboard.osc52_write (unchanged for existing and new sessions)"
                .to_owned(),
        );
    }
    if parts.is_empty() {
        parts.push("no effective settings changed".to_owned());
    }
    parts.extend(warnings);
    *accepted = candidate;
    Ok(ConfigurationAcceptance {
        summary: parts.join("; "),
        notice: (!actionable.is_empty()).then(|| actionable.join("; ")),
    })
}

/// A parsed but unaccepted candidate with non-borrowing reader acquisitions.
struct PendingReload {
    loaded: harbor_config::SettingsLoad,
    acquisition: Option<ConfigurationAcquisition>,
    automatic: bool,
    path: std::path::PathBuf,
    report_palette: bool,
    quick_save: Option<(u64, Vec<u8>)>,
}

/// Owns live configuration watching, reload barriers, accepted settings, and notice feedback.
pub(super) struct ConfigurationCoordinator {
    path: Option<std::path::PathBuf>,
    watch: Option<crate::configuration_watch::ConfigurationWatch>,
    feedback: crate::configuration_watch::ReloadFeedback,
    accepted_settings: harbor_config::Settings,
    effective_clipboard: harbor_config::ClipboardSettings,
    pending: Option<PendingReload>,
    reload_after_split: bool,
}

impl ConfigurationCoordinator {
    pub(super) fn new(
        path: Option<std::path::PathBuf>,
        accepted_settings: harbor_config::Settings,
    ) -> Self {
        let effective_clipboard = accepted_settings.clipboard.clone();
        Self {
            path,
            watch: None,
            feedback: Default::default(),
            accepted_settings,
            effective_clipboard,
            pending: None,
            reload_after_split: false,
        }
    }

    pub(super) fn start_watch(
        &mut self,
        event_proxy: EventLoopProxy<AppEvent>,
        workspace_ui: &WorkspaceUiController,
    ) {
        match crate::configuration_watch::ConfigurationWatch::start(self.path.clone(), move || {
            event_proxy.send_event(AppEvent::ConfigurationWatch).is_ok()
        }) {
            Ok(watch) => self.watch = Some(watch),
            // Do not include arbitrary OS messages or any configuration contents.
            Err(error) => self.report_watch_failure(
                workspace_ui,
                &format!("setup: {:?}", error.kind()),
                false,
            ),
        }
    }

    pub(super) fn path(&self) -> Option<&std::path::PathBuf> {
        self.path.as_ref()
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn set_report_palette(&mut self) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        pending.report_palette = true;
        true
    }

    pub(super) fn take_reload_after_split(&mut self) -> bool {
        std::mem::take(&mut self.reload_after_split)
    }

    pub(super) fn supersede_pending(
        &mut self,
        workspace_ui: &WorkspaceUiController,
        reason: &'static str,
    ) {
        if let Some(previous) = self.pending.take()
            && previous.report_palette
        {
            workspace_ui.report_palette_result(AppCommand::ReloadConfiguration, Err(reason));
        }
    }

    pub(super) fn report_watch_failure(
        &mut self,
        workspace_ui: &WorkspaceUiController,
        reason: &str,
        retrying: bool,
    ) {
        // A watcher fault is a different visible failure. Once I/O recovers,
        // resurface any still-invalid candidate rather than leave a stale
        // "watching unavailable" message behind duplicate suppression.
        self.feedback.clear();
        let path = self
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "configuration path unavailable".to_owned());
        let recovery = if retrying {
            "Observation will retry automatically."
        } else {
            "Restart Harbor to retry observation."
        };
        let message = format!(
            "Configuration watching unavailable ({path}): {reason}. {recovery} Previously accepted configuration remains effective. Manual reload (app.reload-configuration) remains available."
        );
        tracing::error!(%message, "configuration watcher failure");
        workspace_ui.publish_configuration_notice(
            harbor_app::notifications::ConfigurationNoticeSource::Observation,
            Some(message),
        );
    }

    pub(super) fn handle_watch_event(
        &mut self,
        tabs: &mut TabCoordinator,
        workspace_ui: &WorkspaceUiController,
        main_host: &mut WinitWindowHost,
        split_pending: bool,
    ) -> Option<ControlFlowEffect> {
        let mut wait = None;
        match self.watch.as_ref().and_then(|watch| watch.take_pending()) {
            Some(crate::configuration_watch::WatchEvent::Reload) => {
                workspace_ui.publish_configuration_notice(
                    harbor_app::notifications::ConfigurationNoticeSource::Observation,
                    None,
                );
                let (_, reload_wait) =
                    self.reload(tabs, workspace_ui, main_host, true, split_pending);
                merge_wait(&mut wait, reload_wait);
            }
            Some(crate::configuration_watch::WatchEvent::Unavailable(kind)) => {
                self.report_watch_failure(
                    workspace_ui,
                    &format!("observation I/O: {kind:?}"),
                    true,
                );
            }
            None => {}
        }
        merge_wait(&mut wait, main_host.request_frame().wait);
        wait
    }

    pub(super) fn begin_quick_save_publish(
        &mut self,
        tabs: &mut TabCoordinator,
        workspace_ui: &WorkspaceUiController,
        main_host: &mut WinitWindowHost,
        loaded: harbor_config::SettingsLoad,
        path: std::path::PathBuf,
        quick_save: (u64, Vec<u8>),
    ) -> Result<Option<ControlFlowEffect>, String> {
        let acquisition = tabs.acquire_configuration(&loaded.settings).map_err(|_| {
            "Preset saved on disk, but configuration preparation failed; retry Save to publish"
                .to_owned()
        })?;
        self.pending = Some(PendingReload {
            loaded,
            acquisition,
            automatic: false,
            path,
            report_palette: false,
            quick_save: Some(quick_save),
        });
        let (_, wait) = self.advance(tabs, workspace_ui, main_host);
        Ok(wait)
    }

    pub(super) fn reload(
        &mut self,
        tabs: &mut TabCoordinator,
        workspace_ui: &WorkspaceUiController,
        main_host: &mut WinitWindowHost,
        automatic: bool,
        split_pending: bool,
    ) -> (Option<bool>, Option<ControlFlowEffect>) {
        if split_pending {
            self.reload_after_split = true;
            return (None, None);
        }
        // Publication for an explicit save retains its acquisitions and exact disk generation.
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.quick_save.is_some())
        {
            return (None, None);
        }
        // A new document supersedes an unaccepted candidate, not an accepted generation.
        self.supersede_pending(workspace_ui, "Rejected: reload superseded");
        let path = self.path.clone();
        let path_label = path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "configuration path unavailable".to_owned());
        let staged = (|| -> anyhow::Result<Option<PendingReload>> {
            let path =
                path.ok_or_else(|| anyhow::anyhow!("could not resolve configuration path"))?;
            let loaded = match harbor_config::load_candidate_from_path(&path) {
                Ok(loaded) => loaded,
                Err(error) if error.io_kind == Some(std::io::ErrorKind::NotFound) => {
                    return Ok(None);
                }
                Err(error) => return Err(error.into()),
            };
            harbor_app::command::validate_keybindings(&loaded.settings.keybindings)
                .map_err(|errors| anyhow::anyhow!("{}", errors.join("; ")))?;
            let acquisition = tabs
                .acquire_configuration(&loaded.settings)
                .map_err(|error| error.context(ConfigurationOperationFailure))?;
            Ok(Some(PendingReload {
                loaded,
                acquisition,
                automatic,
                path,
                report_palette: false,
                quick_save: None,
            }))
        })();
        match staged {
            Ok(Some(pending)) => {
                self.pending = Some(pending);
                self.advance(tabs, workspace_ui, main_host)
            }
            Ok(None) => (
                self.report_reload_result(workspace_ui, Ok(None), automatic, &path_label),
                None,
            ),
            Err(error) => (
                self.report_reload_result(workspace_ui, Err(error), automatic, &path_label),
                None,
            ),
        }
    }

    pub(super) fn advance(
        &mut self,
        tabs: &mut TabCoordinator,
        workspace_ui: &WorkspaceUiController,
        main_host: &mut WinitWindowHost,
    ) -> (Option<bool>, Option<ControlFlowEffect>) {
        let Some(pending) = self.pending.take() else {
            return (None, None);
        };
        let ready = pending
            .acquisition
            .as_ref()
            .map_or(Ok(true), |acquisition| {
                tabs.configuration_ready(acquisition)
            });
        if matches!(ready, Ok(false)) {
            self.pending = Some(pending);
            return (None, None);
        }
        let PendingReload {
            loaded,
            acquisition,
            automatic,
            path,
            report_palette,
            quick_save,
        } = pending;
        let mut wait = None;
        let save_guard = quick_save
            .as_ref()
            .map(|(_, expected)| crate::profile_save::pin_generation(&path, expected));
        if let Some(Err(error)) = save_guard.as_ref() {
            drop(acquisition);
            let generation = quick_save.as_ref().unwrap().0;
            if let Some(focus) = workspace_ui.finish_quick_save(
                generation,
                Err(format!(
                    "Preset was written, but not published: {error}. Inspect/reload the file"
                )),
            ) {
                merge_wait(&mut wait, main_host.request_focus(&focus).wait);
                merge_wait(&mut wait, main_host.request_frame().wait);
            }
            return (Some(false), wait);
        }
        let path_label = path.display().to_string();
        let viewport = main_host.viewport().clone();
        let result = ready
            .map_err(|error| error.context(ConfigurationOperationFailure))
            .and_then(|_| {
                apply_loaded_configuration(
                    &path,
                    &mut self.accepted_settings,
                    &self.effective_clipboard,
                    loaded,
                    |candidate, bindings| {
                        let warnings = tabs.apply_configuration(
                            candidate,
                            viewport.scale_factor,
                            viewport.physical_size,
                        )?;
                        workspace_ui.publish_keybindings(bindings);
                        Ok(warnings)
                    },
                )
                .map(Some)
            });
        // Release every acquisition even on preparation/PTY/compensation failure.
        drop(acquisition);
        drop(save_guard);
        let completion = self.report_reload_result(workspace_ui, result, automatic, &path_label);
        if let Some((generation, _)) = quick_save {
            let outcome = if completion == Some(true) {
                Ok(())
            } else {
                Err("Preset saved on disk, but configuration publication failed; retry Save to publish or cancel and reload".into())
            };
            if let Some(focus) = workspace_ui.finish_quick_save(generation, outcome) {
                merge_wait(&mut wait, main_host.request_focus(&focus).wait);
                merge_wait(&mut wait, main_host.request_frame().wait);
            }
        }
        if report_palette {
            workspace_ui.report_palette_result(
                AppCommand::ReloadConfiguration,
                if completion == Some(true) {
                    Ok(())
                } else {
                    Err("Failed: see configuration reload error")
                },
            );
        }
        (completion, wait)
    }

    fn report_reload_result(
        &mut self,
        workspace_ui: &WorkspaceUiController,
        result: anyhow::Result<Option<ConfigurationAcceptance>>,
        automatic: bool,
        path_label: &str,
    ) -> Option<bool> {
        use harbor_app::notifications::ConfigurationNoticeSource as Source;
        match result {
            Ok(acceptance) => {
                self.feedback.clear();
                let applied = acceptance.is_some();
                if applied {
                    workspace_ui.publish_configuration_notice(Source::Resource, None);
                }
                workspace_ui.publish_configuration_notice(Source::Startup, None);
                let notice = acceptance.and_then(|accepted| {
                    tracing::info!(path = %path_label, summary = %accepted.summary, "configuration reload accepted");
                    accepted.notice.map(|notice| format!("Configuration ({path_label}): {notice}"))
                });
                workspace_ui.publish_configuration_notice(Source::Reload, notice);
                applied.then_some(true)
            }
            Err(error) => {
                workspace_ui.publish_configuration_notice(Source::Startup, None);
                let source = if error.is::<ConfigurationOperationFailure>() {
                    Source::Resource
                } else {
                    Source::Reload
                };
                if source == Source::Resource {
                    workspace_ui.publish_configuration_notice(Source::Reload, None);
                }
                let message = format!(
                    "Configuration reload failed ({path_label}): {error:#}. Previously accepted configuration remains effective; any recovery failure is identified above."
                );
                if self.feedback.failure(&message, automatic) {
                    tracing::error!(%message, "configuration reload rejected");
                    // An explicit retry still reports its result; only repeated
                    // automatic events must stay dismissed in the same episode.
                    if !automatic {
                        workspace_ui.publish_configuration_notice(source, None);
                    }
                    workspace_ui.publish_configuration_notice(source, Some(message));
                }
                Some(false)
            }
        }
    }
}
