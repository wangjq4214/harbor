//! Cross-thread wake events for the winit event loop.
//!
//! Kept separate from shell so host I/O (`pty`) does not depend on the shell.
//! Frame scheduling policy lives in `harbor_widget::scheduler`.

use harbor_widget::effects::ExternalInvalidation;

use harbor_app::tab_manager::TabId;

/// Events posted back to the winit event loop from background workers.
#[derive(Debug)]
pub(crate) enum AppEvent {
    /// The terminal reader queued output for one Host-owned session.
    TerminalOutputReady(TabId),
    /// A confirmed terminal hyperlink click ready for Host policy.
    OpenHyperlink(String),
    /// Default-on configuration observation; never carries a parsed candidate.
    ConfigurationWatch,
    /// Device-level work; not terminal invalidation or a redraw.
    GpuWake,
    /// Opaque adapter work transported to the owning main-window Host.
    #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
    WidgetHostWork(harbor_widget::winit::WidgetHmrWork),
}

impl From<harbor_widget::winit::GpuWakeEvent> for AppEvent {
    fn from(_: harbor_widget::winit::GpuWakeEvent) -> Self {
        Self::GpuWake
    }
}

/// Maps host wake events to source-agnostic runtime invalidation.
pub(crate) fn external_invalidation_for_app_event(
    event: &AppEvent,
) -> Option<ExternalInvalidation> {
    match event {
        AppEvent::TerminalOutputReady(_) => Some(ExternalInvalidation::new()),
        AppEvent::OpenHyperlink(_) | AppEvent::ConfigurationWatch | AppEvent::GpuWake => None,
        #[cfg(all(feature = "widget-hot-reload", target_os = "windows", debug_assertions))]
        AppEvent::WidgetHostWork(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_wake_does_not_request_render_invalidation() {
        assert!(matches!(
            AppEvent::from(harbor_widget::winit::GpuWakeEvent),
            AppEvent::GpuWake
        ));
        assert_eq!(
            external_invalidation_for_app_event(&AppEvent::GpuWake),
            None
        );
    }

    #[test]
    fn terminal_output_event_maps_only_to_generic_external_invalidation() {
        let event = AppEvent::TerminalOutputReady(TabId(7));
        assert_eq!(
            external_invalidation_for_app_event(&event),
            Some(ExternalInvalidation::new())
        );
    }
}
