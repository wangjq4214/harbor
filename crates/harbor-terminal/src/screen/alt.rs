//! Alternate-screen stack: tracks whether the terminal is in alt-screen mode
//! and holds the pending alt-screen request communicated between parser and Terminal.

use harbor_types::AltScreenAction;

/// Tracks alt-screen state and pending requests.
///
/// The actual save/restore of the full `Screen` state is handled by `Screen`
/// itself (via `enter_alt` / `exit_alt`), because swapping the entire screen
/// requires access to all engines.
#[derive(Debug, Default)]
pub(crate) struct AltScreenStack {
    /// Whether the alternate screen is currently active.
    in_alt: bool,
    /// Pending alt-screen request set by the parser, consumed by Terminal.
    alt_request: Option<AltScreenAction>,
}

impl AltScreenStack {
    pub(crate) fn new() -> Self {
        Self {
            in_alt: false,
            alt_request: None,
        }
    }

    /// Returns `true` when the alternate screen is active.
    pub(crate) fn is_alt(&self) -> bool {
        self.in_alt
    }

    /// Requests entry into the alternate screen.
    pub(crate) fn request_enter(&mut self) {
        self.alt_request = Some(AltScreenAction::Enter);
    }

    /// Requests exit from the alternate screen.
    pub(crate) fn request_exit(&mut self) {
        self.alt_request = Some(AltScreenAction::Exit);
    }

    /// Peeks at the pending alt-screen request without consuming it.
    pub(crate) fn alt_request(&self) -> Option<AltScreenAction> {
        self.alt_request
    }

    /// Takes the pending alt-screen request, resetting the field to `None`.
    pub(crate) fn take_alt_request(&mut self) -> Option<AltScreenAction> {
        self.alt_request.take()
    }

    /// Marks the alternate screen as active (called after `enter_alt` succeeds).
    pub(crate) fn mark_active(&mut self) {
        self.in_alt = true;
    }

    /// Marks the alternate screen as inactive (called after `exit_alt` succeeds).
    pub(crate) fn mark_inactive(&mut self) {
        self.in_alt = false;
    }
}
