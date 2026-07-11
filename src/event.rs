//! Cross-thread wake events for the winit event loop.
//!
//! Kept separate from `app` so host I/O (`pty`) does not depend on the shell.

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// Lightweight wake: PTY reader has appended bytes to the shared buffer.
    PtyOutputReady,
}
