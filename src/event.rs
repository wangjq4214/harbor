//! Cross-thread wake events for the winit event loop.
//!
//! Kept separate from shell so host I/O (`pty`) does not depend on the shell.
//! Frame scheduling policy lives in `harbor_widget::scheduler`.

use harbor_widget::effects::ExternalInvalidation;

use crate::tab_manager::TabId;

/// Events posted back to the winit event loop from background workers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppEvent {
    /// The terminal reader queued output for one Host-owned session.
    TerminalOutputReady(TabId),
}

/// Maps host wake events to source-agnostic runtime invalidation.
pub(crate) fn external_invalidation_for_app_event(event: AppEvent) -> Option<ExternalInvalidation> {
    match event {
        AppEvent::TerminalOutputReady(_) => Some(ExternalInvalidation::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_output_event_maps_only_to_generic_external_invalidation() {
        let event = AppEvent::TerminalOutputReady(TabId(7));
        assert_eq!(
            external_invalidation_for_app_event(event),
            Some(ExternalInvalidation::new())
        );
    }
}
