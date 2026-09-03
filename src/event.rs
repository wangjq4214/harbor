//! Cross-thread wake events for the winit event loop.
//!
//! Kept separate from `app` so host I/O (`pty`) does not depend on the shell.
//! Frame scheduling policy lives in `harbor_widget::scheduler`.

/// Events posted back to the winit event loop from background workers and UI handlers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppEvent {
    /// The terminal reader queued output for UI-thread parsing.
    TerminalOutputReady,
    /// A tab was selected by visual index.
    SelectSession(usize),
    /// A tab close button was clicked for the given index.
    CloseSession(usize),
    /// The new tab "+" button was clicked.
    NewSession,
}
