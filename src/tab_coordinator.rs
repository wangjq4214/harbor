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
#[cfg(test)]
use std::sync::atomic::Ordering;
use winit::{event_loop::EventLoopProxy, window::Window};

use crate::event::AppEvent;
use harbor_app::{
    command::AppCommandRequest,
    tab_manager::{TabActionOutcome, TabId, TabManager, TerminalTabResources},
    tab_view::{TabCommand, TabFocusPolicy, TabUiController},
    terminal_view::{TerminalWidgetBridge, terminal_size_from_allocation},
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

#[cfg(test)]
fn process_ungated_action_batch<A>(
    actions: impl IntoIterator<Item = A>,
    input_gate: &std::sync::atomic::AtomicBool,
    mut process: impl FnMut(A) -> bool,
) {
    for action in actions {
        if input_gate.load(Ordering::Acquire) || !process(action) {
            return;
        }
    }
}

/// Window-independent factory policy and the real application-thread transaction.
/// Platform/window ownership stays in TerminalTabFactory; no publication occurs until
/// all model/render preparations and group PTY effects have succeeded.
struct TerminalConfiguration {
    shell_command: ShellCommand,
    font_settings: harbor_config::FontSettings,
    metrics: TextMetrics,
    appearance: TerminalAppearance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReloadStage {
    Font,
    Metrics,
    Model,
    Gpu,
    Pty(TerminalSize),
}

impl TerminalConfiguration {
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        tabs: &mut TabManager,
        tab_ui: &TabUiController,
        gpu: TerminalGpuAccess<'_>,
        settings: &harbor_config::Settings,
        font_changed: bool,
        scale: f32,
        surface: (u32, u32),
        mut preflight: impl FnMut(usize, ReloadStage) -> anyhow::Result<()>,
    ) -> anyhow::Result<Vec<String>> {
        let targets = tabs.configuration_targets();
        anyhow::ensure!(!targets.is_empty(), "no applicable terminal sessions");
        let mut warnings = Vec::new();
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
            let rect = tab_ui
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
        self.font_settings = settings.font.clone();
        self.metrics = metrics;
        self.appearance = TerminalAppearance::from_palette(settings.colors);
        self.shell_command = configured_shell(&settings.shell);
        // Effective clipboard delivery remains the startup policy, including for new tabs.
        tabs.accept_configuration_size(size);
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
    tab_ui: TabUiController,
    factory: TerminalTabFactory,
}

impl TabCoordinator {
    pub(crate) fn report_configuration_result(&self, message: String) {
        self.tab_ui.report_configuration_result(message);
    }

    pub(crate) fn publish_keybindings(&self, bindings: harbor_app::command::ResolvedKeybindings) {
        self.tab_ui.publish_keybindings(bindings);
    }

    pub(crate) fn apply_configuration(
        &mut self,
        settings: &harbor_config::Settings,
        font_changed: bool,
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
            &self.tab_ui,
            gpu,
            settings,
            font_changed,
            scale,
            surface,
            |_, _| Ok(()),
        )
    }
    pub(crate) fn new(
        tabs: TabManager,
        tab_ui: TabUiController,
        factory: TerminalTabFactory,
    ) -> Self {
        Self {
            tabs,
            tab_ui,
            factory,
        }
    }

    pub(crate) fn active_terminal(&self) -> Option<Arc<Mutex<Terminal>>> {
        self.tabs.active_terminal()
    }

    pub(crate) fn active_live(&self, source: TabId) -> bool {
        self.tabs.active_live(source)
    }

    pub(crate) fn sync_ui(&self, window: &Window) {
        if let Some(title) = self.tabs.active_title() {
            window.set_title(&format_window_title(title));
        }
        self.sync_tab_ui(window);
    }

    pub(crate) fn sync_tab_ui(&self, window: &Window) {
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
        let Some(rect) = self.tab_ui.latest_terminal_allocation() else {
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

    pub(crate) fn create_terminal_tab(
        &mut self,
        surface_size: (u32, u32),
    ) -> anyhow::Result<TabActionOutcome> {
        let size = self
            .tabs
            .last_broadcast_size()
            .unwrap_or_else(|| self.factory.default_terminal_size(surface_size));
        self.tabs.create_tab(|tab_id, draw_id| {
            self.factory
                .create_resources(tab_id, draw_id, size, surface_size)
        })
    }

    pub(crate) fn terminal_focus(&self) -> harbor_widget::widgets::FocusHandle {
        self.tab_ui.terminal_focus()
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
            TabFocusPolicy::RailTab(id) => self.tab_ui.tab_focus(id),
            TabFocusPolicy::Terminal => Some(self.tab_ui.terminal_focus()),
        };
        if let Some(focus_handle) = focus_handle {
            result.merge_wait(host.request_focus(&focus_handle).wait);
        }
        result
    }

    pub(crate) fn drain_actions(&self) -> Vec<AppCommandRequest> {
        self.tab_ui.drain_actions()
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
        let outcome = match command {
            TabCommand::New => match self.create_terminal_tab(host.viewport().physical_size) {
                Ok(outcome) => outcome,
                Err(error) => {
                    let message =
                        format!("New session failed: {error:#}. Existing sessions are unchanged.");
                    tracing::warn!(%message, "failed to create terminal tab");
                    self.tab_ui.report_configuration_result(message);
                    return TabDrainOutcome {
                        wait: host.request_frame().wait,
                        close_window: false,
                    };
                }
            },
            TabCommand::Close(id) => self.tabs.close(id),
            TabCommand::CloseActive => self.tabs.close_active(),
            TabCommand::Activate(id) => self.tabs.activate(id),
            TabCommand::Next => self.tabs.activate_next(),
            TabCommand::Previous => self.tabs.activate_previous(),
            TabCommand::Numeric(index) => self.tabs.activate_numeric(index),
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
#[path = "configuration_reload_tests.rs"]
mod configuration_reload_tests;

#[cfg(test)]
mod tests {
    use super::*;

    use harbor_widget::Store;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn gated_action_batch_applies_no_actions_and_is_not_replayed() {
        let store = Store::<(), u8>::new(());
        let dispatcher = store.dispatcher();
        dispatcher.dispatch(1);
        dispatcher.dispatch(2);
        let input_gate = AtomicBool::new(true);
        let mut processed = Vec::new();

        process_ungated_action_batch(store.drain_actions(), &input_gate, |action| {
            processed.push(action);
            true
        });

        assert!(processed.is_empty());
        assert!(store.drain_actions().is_empty());
    }

    #[test]
    fn gate_raised_mid_batch_discards_the_unprocessed_remainder() {
        let store = Store::<(), u8>::new(());
        let dispatcher = store.dispatcher();
        dispatcher.dispatch(1);
        dispatcher.dispatch(2);
        dispatcher.dispatch(3);
        let input_gate = AtomicBool::new(false);
        let mut processed = Vec::new();

        process_ungated_action_batch(store.drain_actions(), &input_gate, |action| {
            processed.push(action);
            input_gate.store(true, Ordering::Release);
            true
        });

        assert_eq!(processed, [1]);
        assert!(store.drain_actions().is_empty());
    }

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
