//! Ordinary geometry intent and progress, separate from host transaction admission.
use crate::TerminalSize;

/// Non-consuming state. A failed acquisition needs an advance for error delivery
/// and cleanup; it is not a successfully acquired reader barrier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeStatus {
    Idle,
    ConfigurationOwned,
    WaitingForReader { target: TerminalSize },
    ReadyToCommit { target: TerminalSize },
    Failed { target: TerminalSize },
}

impl ResizeStatus {
    /// Work eligible for a bounded host admission slot, including failure cleanup.
    pub fn needs_advance(self) -> bool {
        matches!(self, Self::ReadyToCommit { .. } | Self::Failed { .. })
    }
}

/// A request or progress result. Pending never means geometry was published.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use]
pub enum ResizeOutcome {
    Unchanged,
    ConfigurationOwned,
    Pending(ResizeStatus),
    Committed(TerminalSize),
}

impl ResizeOutcome {
    pub fn committed(self) -> bool {
        matches!(self, Self::Committed(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terminal;
    #[test]
    fn normalized_requests_and_configuration_completed_intent_are_not_pending() {
        let mut terminal = Terminal::new_headless(2, 4);
        let size = TerminalSize { rows: 1, cols: 2 };
        assert_eq!(
            terminal
                .request_resize(TerminalSize { rows: 0, cols: 0 })
                .unwrap(),
            ResizeOutcome::Pending(ResizeStatus::ReadyToCommit { target: size })
        );
        assert_eq!(terminal.screen().cols(), 4, "request must not commit");
        let mut transaction = terminal.prepare_configuration_resize(size).unwrap();
        transaction.resize_pty().unwrap();
        transaction.commit(harbor_config::Palette::default(), None, 1.0);
        assert_eq!(terminal.advance_resize().unwrap(), ResizeOutcome::Unchanged);
        assert_eq!(terminal.resize_status(), ResizeStatus::Idle);
        assert_eq!(
            terminal.request_resize(size).unwrap(),
            ResizeOutcome::Unchanged
        );
    }
}
