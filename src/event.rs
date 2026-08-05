//! Cross-thread wake events for the winit event loop.
//!
//! Kept separate from `app` so host I/O (`pty`) does not depend on the shell.
//! Frame scheduling policy lives in `harbor_widget::scheduler`.

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// The terminal reader queued output for UI-thread parsing.
    TerminalOutputReady,
}
