//! Tab orchestration between Host-owned terminal tab models and declarative UI projection.

use anyhow::Context as _;
use std::sync::{Arc, Mutex};

use harbor_pty::PtyEndpoints;
use harbor_pty::ShellCommand;
use harbor_terminal::{
    Terminal, TerminalAppearance, TerminalGpuAccess, TerminalRenderPipeline, TerminalSize,
    TextMetrics, load_system_fonts,
};
use harbor_widget::{
    effects::ControlFlowEffect,
    scene::primitive::ExternalDrawId,
    winit::{SharedGpu, WinitWindowHost},
};
use winit::{event_loop::EventLoopProxy, window::Window};

use crate::event::AppEvent;
use harbor_app::{
    tab_manager::{TabActionOutcome, TabId, TabManager, TerminalTabResources},
    tab_view::{TabCommand, TabFocusPolicy},
    terminal_view::{TerminalWidgetBridge, terminal_size_from_allocation},
    workspace::WorkspaceUiController,
};
/// Shared startup and accepted-generation conversion; never merges prior overrides.
pub(crate) fn configured_shell(settings: &harbor_config::ShellSettings) -> ShellCommand {
    ShellCommand {
        program: settings.program.clone(),
        args: settings.args.clone(),
        environment: settings.environment.clone(),
        directory: settings.directory.clone(),
    }
}
pub(crate) fn format_window_title(title: &str) -> String {
    format!("{title} — Harbor")
}

/// Application-visible result of draining one FIFO tab action batch.
#[derive(Debug, Default)]
pub(crate) struct TabDrainOutcome {
    pub(crate) wait: Option<ControlFlowEffect>,
    pub(crate) close_window: bool,
    pub(crate) failed: bool,
}

impl TabDrainOutcome {
    fn merge_wait(&mut self, wait: Option<ControlFlowEffect>) {
        self.wait = match (self.wait, wait) {
            (Some(left), Some(right)) => Some(left.arbitrate(right)),
            (Some(wait), None) | (None, Some(wait)) => Some(wait),
            (None, None) => None,
        };
    }
}

/// One step in the Host's drained FIFO. This is admission policy, not an effect bus:
/// the Shell still executes commands and owns all native focus/window/clipboard effects.
pub(crate) enum AppActionAdmission {
    Execute {
        request: harbor_app::command::AppCommandRequest,
        clear_palette_focus: bool,
    },
    Rejected {
        request: harbor_app::command::AppCommandRequest,
        reason: &'static str,
        restore_terminal_focus: bool,
    },
}

/// Pull after each execution so a gate raised by Paste applies to the remainder.
/// Palette admission precedes the gate: even a blocked request retires its own editor.
fn next_app_action(
    actions: &mut impl Iterator<Item = harbor_app::command::AppCommandRequest>,
    ui: &WorkspaceUiController,
    tabs: &TabManager,
    input_blocked: bool,
) -> Option<AppActionAdmission> {
    use harbor_app::command::CommandSource;
    for request in actions.by_ref() {
        let palette = matches!(request.source, CommandSource::Palette { .. });
        if palette && let Err(reason) = ui.take_palette_request(request, tabs, input_blocked) {
            return Some(AppActionAdmission::Rejected {
                request,
                reason,
                restore_terminal_focus: !ui.palette_is_open() && !input_blocked,
            });
        }
        if !input_blocked {
            return Some(AppActionAdmission::Execute {
                request,
                clear_palette_focus: palette,
            });
        }
    }
    None
}

/// Window-independent factory policy and the real application-thread transaction.
/// Platform/window ownership stays in TerminalTabFactory; no publication occurs until
/// all model/render preparations and group PTY effects have succeeded.
struct TerminalConfiguration {
    shell_command: ShellCommand,
    font_settings: harbor_config::FontSettings,
    // Effective selection warnings survive no-change reloads until the font
    // request changes successfully; silence must not imply the request took effect.
    font_warnings: Vec<String>,
    metrics: TextMetrics,
    appearance: TerminalAppearance,
    // A rejected group may leave a reconciled model at a different PTY size (ADR 0061).
    // Settings equality is not evidence that geometry no longer needs recovery.
    geometry_recovery_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReloadStage {
    Font,
    Metrics,
    Palette,
    Model,
    Gpu,
    Pty(TerminalSize),
}

impl TerminalConfiguration {
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        tabs: &mut TabManager,
        workspace_ui: &WorkspaceUiController,
        gpu: TerminalGpuAccess<'_>,
        settings: &harbor_config::Settings,
        scale: f32,
        surface: (u32, u32),
        mut preflight: impl FnMut(usize, ReloadStage) -> anyhow::Result<()>,
    ) -> anyhow::Result<Vec<String>> {
        let font_changed = settings.font != self.font_settings;
        let colors_changed = settings.colors != self.appearance.palette();
        if !font_changed && !self.geometry_recovery_pending {
            if colors_changed {
                let targets = tabs.configuration_targets();
                anyhow::ensure!(!targets.is_empty(), "no applicable terminal sessions");
                // Acquire every model before publishing a palette. No renderer, allocation,
                // reader barrier or PTY operation is involved in a color-only change.
                let mut terminals = targets
                    .iter()
                    .map(|(id, terminal, _)| {
                        terminal
                            .lock()
                            .map_err(|_| anyhow::anyhow!("session {id}: terminal lock unavailable"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let mut palettes = Vec::new();
                for (index, terminal) in terminals.iter_mut().enumerate() {
                    preflight(index, ReloadStage::Palette).with_context(|| {
                        format!("session {}: palette preparation", targets[index].0)
                    })?;
                    let size = TerminalSize {
                        rows: terminal.screen().rows(),
                        cols: terminal.screen().cols(),
                    };
                    // Same-size preparation owns exclusive model access without a resize/barrier.
                    palettes.push(terminal.prepare_configuration_resize(size)?);
                }
                for palette in palettes {
                    palette.commit(settings.colors, None, scale);
                }
                self.appearance = TerminalAppearance::from_palette(settings.colors);
            }
            self.shell_command = configured_shell(&settings.shell);
            // Bindings and launch/restart-only changes do not depend on session resources.
            return Ok(self.font_warnings.clone());
        }
        let targets = tabs.configuration_targets();
        anyhow::ensure!(!targets.is_empty(), "no applicable terminal sessions");
        let mut warnings = if font_changed {
            Vec::new()
        } else {
            self.font_warnings.clone()
        };
        let mut fonts = Vec::new();
        let mut metrics = self.metrics;
        if font_changed {
            for (index, (id, _, _)) in targets.iter().enumerate() {
                preflight(index, ReloadStage::Font)
                    .with_context(|| format!("session {id}: terminal font acquisition"))?;
                let font = load_system_fonts(&settings.font)
                    .with_context(|| format!("session {id}: terminal font acquisition"))?;
                if let Some(warning) = font.selection_warning()
                    && !warnings.iter().any(|item| item == warning)
                {
                    warnings.push(warning.to_owned());
                }
                preflight(index, ReloadStage::Metrics)
                    .with_context(|| format!("session {id}: primary font metrics"))?;
                let measured = TextMetrics::from_font_metrics(font.font_metrics());
                if index == 0 {
                    metrics = measured;
                }
                anyhow::ensure!(
                    measured == metrics,
                    "session {id}: inconsistent primary font metrics"
                );
                fonts.push(font);
            }
        }
        let size = if font_changed {
            let rect = workspace_ui
                .latest_terminal_allocation()
                .context("terminal allocation is unavailable for font reload")?;
            terminal_size_from_allocation(rect, scale, surface, &metrics)
                .context("terminal allocation/metrics are not drawable for font reload")?
        } else {
            tabs.last_broadcast_size()
                .context("terminal geometry is unavailable")?
        };
        let mut terminals = targets
            .iter()
            .map(|(id, terminal, _)| {
                terminal
                    .lock()
                    .map_err(|_| anyhow::anyhow!("session {id}: terminal lock unavailable"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let renderers = targets
            .iter()
            .map(|(id, _, bridge)| {
                bridge
                    .renderer()
                    .with_context(|| format!("session {id}: renderer unavailable"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut renderers = renderers
            .iter()
            .enumerate()
            .map(|(index, renderer)| {
                renderer.lock().map_err(|_| {
                    anyhow::anyhow!("session {}: renderer lock unavailable", targets[index].0)
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut transactions = terminals
            .iter_mut()
            .enumerate()
            .map(|(index, terminal)| {
                preflight(index, ReloadStage::Model).with_context(|| {
                    format!(
                        "session {}: reader barrier/model preparation",
                        targets[index].0
                    )
                })?;
                terminal
                    .prepare_configuration_resize(size)
                    .with_context(|| {
                        format!(
                            "session {}: reader barrier/model preparation",
                            targets[index].0
                        )
                    })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut replacements = Vec::new();
        if font_changed {
            for (index, font) in fonts.into_iter().enumerate() {
                preflight(index, ReloadStage::Gpu).with_context(|| {
                    format!("session {}: GPU font preparation", targets[index].0)
                })?;
                replacements.push(
                    renderers[index]
                        .prepare_fonts(gpu, font, &transactions[index].update(settings.colors))
                        .with_context(|| {
                            format!("session {}: GPU font preparation", targets[index].0)
                        })?,
                );
            }
        }
        // Conservatively retry geometry after any rejected native group. In particular,
        // compensation failure can mutate a model while the accepted settings stay old.
        self.geometry_recovery_pending = true;
        harbor_terminal::resize_configuration_group_with_check(&mut transactions, |index, size| {
            preflight(index, ReloadStage::Pty(size))
        })
        .map_err(|error| {
            anyhow::anyhow!(
                "{}",
                error
                    .failures
                    .into_iter()
                    .map(|(index, message)| format!("session {}: {message}", targets[index].0))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })?;
        // From this point all publication consists of infallible ownership/state moves.
        if font_changed {
            for (renderer, replacement) in renderers.iter_mut().zip(replacements) {
                **renderer = replacement;
            }
        }
        for (transaction, renderer) in transactions.into_iter().zip(renderers.iter()) {
            transaction.commit(settings.colors, Some(renderer.viewport()), scale);
        }
        self.font_warnings = warnings.clone();
        self.font_settings = settings.font.clone();
        self.metrics = metrics;
        self.appearance = TerminalAppearance::from_palette(settings.colors);
        self.shell_command = configured_shell(&settings.shell);
        // Effective clipboard delivery remains the startup policy, including for new tabs.
        tabs.accept_configuration_size(size);
        self.geometry_recovery_pending = false;
        Ok(warnings)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_new_terminal(
        &self,
        gpu: TerminalGpuAccess<'_>,
        fonts: harbor_terminal::FontBook,
        size: TerminalSize,
        surface: (u32, u32),
        backdrop_available: bool,
        clipboard_delivery: harbor_terminal::ClipboardDelivery,
    ) -> anyhow::Result<(Terminal, TerminalRenderPipeline)> {
        let mut terminal =
            Terminal::new_headless_with_appearance(size.rows, size.cols, self.appearance);
        terminal.set_backdrop_available(backdrop_available);
        terminal.set_clipboard_delivery(clipboard_delivery);
        let renderer = TerminalRenderPipeline::new(
            gpu,
            (surface.0.max(1), surface.1.max(1)),
            fonts,
            self.metrics,
            &terminal.read_update(std::time::Instant::now()),
        )?;
        Ok((terminal, renderer))
    }
}

/// Factory configuration for spawning new Host-owned terminal tabs.
pub(crate) struct TerminalTabFactory {
    gpu: Arc<SharedGpu>,
    format: wgpu::TextureFormat,
    configuration: TerminalConfiguration,
    backdrop_available: bool,
    event_proxy: EventLoopProxy<AppEvent>,
    input_gate: Arc<std::sync::atomic::AtomicBool>,
    clipboard_delivery: harbor_terminal::ClipboardDelivery,
}

impl TerminalTabFactory {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        gpu: Arc<SharedGpu>,
        format: wgpu::TextureFormat,
        shell_command: ShellCommand,
        font_settings: harbor_config::FontSettings,
        font_warnings: Vec<String>,
        metrics: TextMetrics,
        appearance: TerminalAppearance,
        backdrop_available: bool,
        event_proxy: EventLoopProxy<AppEvent>,
        input_gate: Arc<std::sync::atomic::AtomicBool>,
        clipboard_delivery: harbor_terminal::ClipboardDelivery,
    ) -> Self {
        Self {
            gpu,
            format,
            configuration: TerminalConfiguration {
                shell_command,
                font_settings,
                font_warnings,
                geometry_recovery_pending: false,
                metrics,
                appearance,
            },
            backdrop_available,
            event_proxy,
            input_gate,
            clipboard_delivery,
        }
    }

    pub(crate) fn metrics(&self) -> TextMetrics {
        self.configuration.metrics
    }

    pub(crate) fn default_terminal_size(&self, surface_size: (u32, u32)) -> TerminalSize {
        Terminal::terminal_size_for(surface_size, &self.configuration.metrics)
    }

    pub(crate) fn create_resources(
        &self,
        tab_id: TabId,
        draw_id: ExternalDrawId,
        size: TerminalSize,
        surface_size: (u32, u32),
    ) -> anyhow::Result<TerminalTabResources> {
        let fonts = load_system_fonts(&self.configuration.font_settings)?;
        let endpoints = PtyEndpoints::spawn_shell(
            harbor_pty::TerminalSize {
                rows: size.rows,
                cols: size.cols,
            },
            &self.configuration.shell_command,
        )?;
        let shell_name = endpoints.shell_name().to_owned();
        let output_event_proxy = self.event_proxy.clone();
        let hyperlink_event_proxy = self.event_proxy.clone();
        let gpu = TerminalGpuAccess::new(self.gpu.device(), self.gpu.queue(), self.format);
        let (terminal, renderer) = self.configuration.prepare_new_terminal(
            gpu,
            fonts,
            size,
            surface_size,
            self.backdrop_available,
            self.clipboard_delivery,
        )?;
        // Only the session takes endpoint ownership and starts the reader.
        let terminal = terminal.start_session_from_endpoints(endpoints, move || {
            output_event_proxy
                .send_event(AppEvent::TerminalOutputReady(tab_id))
                .is_ok()
        })?;
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(terminal));
        let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
            draw_id,
            Arc::clone(&terminal),
            renderer,
            Arc::clone(&self.input_gate),
            Arc::new(move |uri| {
                let _ = hyperlink_event_proxy.send_event(AppEvent::OpenHyperlink(uri));
            }),
        );
        Ok(TerminalTabResources::new(terminal, bridge, shell_name))
    }
}

/// Coordinator managing Host-owned terminal tab models and their declarative UI projection.
pub(crate) struct TabCoordinator {
    tabs: TabManager,
    workspace_ui: WorkspaceUiController,
    factory: TerminalTabFactory,
}

impl TabCoordinator {
    pub(crate) fn apply_configuration(
        &mut self,
        settings: &harbor_config::Settings,
        scale: f32,
        surface: (u32, u32),
    ) -> anyhow::Result<Vec<String>> {
        let gpu = TerminalGpuAccess::new(
            self.factory.gpu.device(),
            self.factory.gpu.queue(),
            self.factory.format,
        );
        self.factory.configuration.apply(
            &mut self.tabs,
            &self.workspace_ui,
            gpu,
            settings,
            scale,
            surface,
            |_, _| Ok(()),
        )
    }
    pub(crate) fn new(
        tabs: TabManager,
        workspace_ui: WorkspaceUiController,
        factory: TerminalTabFactory,
    ) -> Self {
        Self {
            tabs,
            workspace_ui,
            factory,
        }
    }

    pub(crate) fn next_app_action(
        &self,
        actions: &mut impl Iterator<Item = harbor_app::command::AppCommandRequest>,
        input_blocked: bool,
    ) -> Option<AppActionAdmission> {
        next_app_action(actions, &self.workspace_ui, &self.tabs, input_blocked)
    }
    pub(crate) fn dispatch_terminal_command(
        &self,
        command: harbor_app::command::AppCommand,
    ) -> Result<harbor_terminal::TerminalEventOutcome, &'static str> {
        self.tabs.dispatch_terminal_command(command)
    }

    pub(crate) fn dispatch_search_command(
        &mut self,
        command: harbor_app::command::AppCommand,
    ) -> harbor_app::SearchCommandOutcome {
        self.tabs
            .execute_search_command(command, &self.workspace_ui)
    }

    pub(crate) fn advance_notifications(
        &self,
        now: std::time::Instant,
    ) -> (bool, Option<std::time::Instant>) {
        self.workspace_ui.advance_notifications(now)
    }

    pub(crate) fn advance_search(&mut self) -> (bool, bool) {
        let tick = self.tabs.advance_search();
        self.workspace_ui
            .publish_search_status(self.tabs.search_status());
        (tick.pending, tick.changed)
    }

    pub(crate) fn active_terminal(&self) -> Option<Arc<Mutex<Terminal>>> {
        self.tabs.active_terminal()
    }

    pub(crate) fn active_live(&self, source: TabId) -> bool {
        self.tabs.active_live(source)
    }

    pub(crate) fn sync_ui(&self, window: &Window) {
        self.workspace_ui.sync_search_source(self.tabs.active_id());
        if let Some(title) = self.tabs.active_title() {
            window.set_title(&format_window_title(title));
        }
        self.sync_tab_ui(window);
    }

    pub(crate) fn sync_tab_ui(&self, window: &Window) {
        self.workspace_ui.tabs().sync(
            self.tabs.snapshots(),
            self.tabs.active_bridge(),
            logical_window_width(window),
        );
    }

    pub(crate) fn update_presentation(&self, window: &Window) -> bool {
        self.workspace_ui
            .tabs()
            .update_presentation(logical_window_width(window))
    }

    pub(crate) fn process_output(
        &mut self,
        tab_id: TabId,
    ) -> harbor_app::tab_manager::TabOutputOutcome {
        self.tabs.process_output(tab_id)
    }

    pub(crate) fn apply_pending_allocation(
        &mut self,
        scale_factor: f32,
        physical_size: (u32, u32),
    ) -> bool {
        let Some(rect) = self.workspace_ui.latest_terminal_allocation() else {
            return false;
        };
        let Some(size) = terminal_size_from_allocation(
            rect,
            scale_factor,
            physical_size,
            &self.factory.metrics(),
        ) else {
            return false;
        };
        self.tabs.resize_all_if_changed(size)
    }

    pub(crate) fn terminal_focus(&self) -> harbor_widget::widgets::FocusHandle {
        self.workspace_ui.terminal_focus()
    }

    pub(crate) fn apply_tab_outcome(
        &mut self,
        host: &mut WinitWindowHost,
        outcome: TabActionOutcome,
        focus: TabFocusPolicy,
    ) -> TabDrainOutcome {
        let mut result = TabDrainOutcome::default();
        if outcome.close_window {
            self.sync_ui(host.window());
            result.close_window = true;
            return result;
        }
        let model_changed =
            outcome.active_bridge_changed || outcome.unread_changed || outcome.request_redraw;
        let focus_requested = focus != TabFocusPolicy::PreserveRail;
        if !model_changed && !focus_requested {
            return result;
        }

        if outcome.active_bridge_changed {
            result.merge_wait(host.cancel_active_input_ownership().wait);
        }
        if model_changed {
            self.sync_ui(host.window());
            result.merge_wait(
                host.invalidate_external(harbor_widget::effects::ExternalInvalidation::new())
                    .wait,
            );
        }
        let viewport = host.viewport().clone();
        self.apply_pending_allocation(viewport.scale_factor, viewport.physical_size);
        let focus_handle = match focus {
            TabFocusPolicy::PreserveRail => None,
            TabFocusPolicy::RailTab(id) => self.workspace_ui.tabs().tab_focus(id),
            TabFocusPolicy::Terminal => Some(self.workspace_ui.terminal_focus()),
        };
        if let Some(focus_handle) = focus_handle {
            result.merge_wait(host.request_focus(&focus_handle).wait);
        }
        result
    }

    pub(crate) fn execute_tab_command(
        &mut self,
        host: &mut WinitWindowHost,
        command: TabCommand,
        requested_focus: TabFocusPolicy,
    ) -> TabDrainOutcome {
        let focus = match (requested_focus, command) {
            (TabFocusPolicy::PreserveRail, TabCommand::Close(id)) => self
                .tabs
                .neighbor_for_close(id)
                .map(TabFocusPolicy::RailTab)
                .unwrap_or(TabFocusPolicy::PreserveRail),
            (focus, _) => focus,
        };
        let surface_size = host.viewport().physical_size;
        let size = self
            .tabs
            .last_broadcast_size()
            .unwrap_or_else(|| self.factory.default_terminal_size(surface_size));
        let outcome = match self.tabs.execute_command(command, |tab_id, draw_id| {
            self.factory
                .create_resources(tab_id, draw_id, size, surface_size)
        }) {
            Ok(outcome) => {
                if command == TabCommand::New {
                    self.workspace_ui.publish_configuration_notice(
                        harbor_app::notifications::ConfigurationNoticeSource::Operation,
                        None,
                    );
                }
                outcome
            }
            Err(error) => {
                let message =
                    format!("New session failed: {error:#}. Existing sessions are unchanged.");
                tracing::warn!(%message, "failed to create terminal tab");
                self.workspace_ui.report_configuration_result(message);
                return TabDrainOutcome {
                    wait: host.request_frame().wait,
                    close_window: false,
                    failed: true,
                };
            }
        };
        self.apply_tab_outcome(host, outcome, focus)
    }
}

pub(crate) fn logical_window_width(window: &Window) -> f64 {
    logical_width_from_physical(window.inner_size().width, window.scale_factor())
}

pub(crate) fn logical_width_from_physical(width: u32, scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        f64::from(width) / scale
    } else {
        0.0
    }
}

#[cfg(test)]
#[path = "action_batch_tests.rs"]
mod action_batch_tests;
#[cfg(test)]
#[path = "configuration_reload_tests.rs"]
mod configuration_reload_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_width_conversion_handles_dpi_zero_and_invalid_scale() {
        assert_eq!(logical_width_from_physical(1_350, 1.5), 900.0);
        assert_eq!(logical_width_from_physical(0, 2.0), 0.0);
        assert_eq!(logical_width_from_physical(900, 0.0), 0.0);
        assert_eq!(logical_width_from_physical(900, f64::NAN), 0.0);
        assert_eq!(logical_width_from_physical(900, f64::INFINITY), 0.0);
    }
    #[test]
    fn native_window_title_appends_product_suffix() {
        assert_eq!(format_window_title("pwsh.exe"), "pwsh.exe — Harbor");
        assert_eq!(
            format_window_title("project — Harbor"),
            "project — Harbor — Harbor"
        );
    }

    #[test]
    fn tab_drain_outcome_defaults_to_not_closing() {
        let outcome = TabDrainOutcome::default();
        assert!(!outcome.close_window);
    }
}
