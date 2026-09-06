//! Cross-thread wake events for the winit event loop.
//!
//! Kept separate from shell so host I/O (`pty`) does not depend on the shell.
//! Frame scheduling policy lives in `harbor_widget::scheduler`.

use harbor_widget::effects::ExternalInvalidation;

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// The terminal reader queued output for UI-thread parsing.
    TerminalOutputReady,
}

/// Maps host wake events to source-agnostic runtime invalidation.
pub(crate) fn external_invalidation_for_app_event(
    event: AppEvent,
) -> Option<ExternalInvalidation> {
    match event {
        AppEvent::TerminalOutputReady => Some(ExternalInvalidation::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_output_event_maps_only_to_generic_external_invalidation() {
        assert_eq!(
            external_invalidation_for_app_event(AppEvent::TerminalOutputReady),
            Some(ExternalInvalidation::new())
        );
    }
}
