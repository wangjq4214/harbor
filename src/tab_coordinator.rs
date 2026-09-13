//! Tab orchestration between Host-owned terminal tab models and declarative UI projection.

use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    time::Instant,
};

use harbor_pty::PtyEndpoints;
use harbor_pty::ShellCommand;
use harbor_terminal::{
    GpuContext, Terminal, TerminalAppearance, TerminalSize, TextMetrics, load_system_fonts,
};
use harbor_widget::{
    effects::RuntimeEffects, scene::primitive::ExternalDrawId, winit::WinitAdapter,
};
use winit::{
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window,
};

use crate::{effects::apply_effects, event::AppEvent};
use harbor_app::{
    tab_manager::{TabActionOutcome, TabId, TabManager, TerminalTabResources},
    tab_view::{TabCommand, TabFocusPolicy, TabUiController},
    terminal_view::{TerminalWidgetBridge, terminal_size_from_allocation},
};

/// Effects and actions the Host must apply after a tab transition.
#[derive(Debug, Default)]
pub(crate) struct TabOutcomeEffects {
    pub(crate) effects: RuntimeEffects,
    pub(crate) close_window: bool,
}

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

/// Factory configuration for spawning new Host-owned terminal tabs.
pub(crate) struct TerminalTabFactory {
    gpu: Arc<GpuContext>,
    shell_command: ShellCommand,
    font_settings: harbor_config::FontSettings,
    metrics: TextMetrics,
    appearance: TerminalAppearance,
    backdrop_available: bool,
    event_proxy: EventLoopProxy<AppEvent>,
    input_gate: Arc<std::sync::atomic::AtomicBool>,
}

impl TerminalTabFactory {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        gpu: Arc<GpuContext>,
        shell_command: ShellCommand,
        font_settings: harbor_config::FontSettings,
        metrics: TextMetrics,
        appearance: TerminalAppearance,
        backdrop_available: bool,
        event_proxy: EventLoopProxy<AppEvent>,
        input_gate: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            gpu,
            shell_command,
            font_settings,
            metrics,
            appearance,
            backdrop_available,
            event_proxy,
            input_gate,
        }
    }

    pub(crate) fn metrics(&self) -> TextMetrics {
        self.metrics
    }

    pub(crate) fn default_terminal_size(&self) -> TerminalSize {
        Terminal::terminal_size_for(&self.gpu, &self.metrics)
    }

    pub(crate) fn create_resources(
        &self,
        tab_id: TabId,
        draw_id: ExternalDrawId,
        size: TerminalSize,
    ) -> anyhow::Result<TerminalTabResources> {
        let fonts = load_system_fonts(&self.font_settings)?;
        let endpoints = PtyEndpoints::spawn_shell(
            harbor_pty::TerminalSize {
                rows: size.rows,
                cols: size.cols,
            },
            &self.shell_command,
        )?;
        let event_proxy = self.event_proxy.clone();
        let mut terminal = Terminal::try_new_with_appearance_from_endpoints(
            size,
            endpoints,
            &self.gpu,
            fonts,
            self.metrics,
            self.appearance,
            move || {
                event_proxy
                    .send_event(AppEvent::TerminalOutputReady(tab_id))
                    .is_ok()
            },
        )?;
        terminal.set_backdrop_available(self.backdrop_available);
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(terminal));
        let bridge = TerminalWidgetBridge::with_gpu(
            draw_id,
            Arc::clone(&terminal),
            Arc::clone(&self.gpu),
            Arc::clone(&self.input_gate),
        );
        Ok(TerminalTabResources::new(terminal, bridge))
    }
}

/// Coordinator managing Host-owned terminal tab models and their declarative UI projection.
pub(crate) struct TabCoordinator {
    tabs: TabManager,
    tab_ui: TabUiController,
    factory: TerminalTabFactory,
}

impl TabCoordinator {
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

    #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
    pub(crate) fn ui_controller(&self) -> TabUiController {
        self.tab_ui.clone()
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

    pub(crate) fn create_terminal_tab(&mut self) -> anyhow::Result<TabActionOutcome> {
        let size = self
            .tabs
            .last_broadcast_size()
            .unwrap_or_else(|| self.factory.default_terminal_size());
        self.tabs
            .create_tab(|tab_id, draw_id| self.factory.create_resources(tab_id, draw_id, size))
    }

    pub(crate) fn apply_tab_outcome(
        &mut self,
        window: &Window,
        runtime: &mut harbor_widget::runtime::Runtime,
        adapter: &mut WinitAdapter,
        outcome: TabActionOutcome,
        focus: TabFocusPolicy,
    ) -> TabOutcomeEffects {
        if outcome.close_window {
            self.sync_ui(window);
            runtime.set_root(harbor_widget::widgets::sized_box::SizedBox::new(
                harbor_widget::layout::Size::ZERO,
            ));
            let effects = adapter.fold_effects(runtime.update(Instant::now()));
            return TabOutcomeEffects {
                effects,
                close_window: true,
            };
        }
        let model_changed =
            outcome.active_bridge_changed || outcome.unread_changed || outcome.request_redraw;
        let focus_requested = focus != TabFocusPolicy::PreserveRail;
        if !model_changed && !focus_requested {
            return TabOutcomeEffects::default();
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
            effects.merge(
                runtime.invalidate_external(harbor_widget::effects::ExternalInvalidation::new()),
            );
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
        TabOutcomeEffects {
            effects,
            close_window: false,
        }
    }

    pub(crate) fn drain_tab_commands(
        &mut self,
        window: &Window,
        runtime: &mut harbor_widget::runtime::Runtime,
        adapter: &mut WinitAdapter,
        event_loop: &ActiveEventLoop,
    ) {
        let input_gate = Arc::clone(&self.factory.input_gate);
        process_ungated_action_batch(self.tab_ui.drain_actions(), &input_gate, |request| {
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
                        return true;
                    }
                },
                TabCommand::Close(id) => self.tabs.close(id),
                TabCommand::CloseActive => self.tabs.close_active(),
                TabCommand::Activate(id) => self.tabs.activate(id),
                TabCommand::Next => self.tabs.activate_next(),
                TabCommand::Previous => self.tabs.activate_previous(),
                TabCommand::Numeric(index) => self.tabs.activate_numeric(index),
            };
            let outcome_effects = self.apply_tab_outcome(window, runtime, adapter, outcome, focus);
            apply_effects(window, &outcome_effects.effects, event_loop);
            if outcome_effects.close_window {
                event_loop.exit();
                return false;
            }
            true
        });
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
    fn tab_outcome_effects_defaults_to_not_closing() {
        let outcome = TabOutcomeEffects::default();
        assert!(!outcome.close_window);
    }
}
