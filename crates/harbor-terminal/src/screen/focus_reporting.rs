use crate::TerminalFocusEvent;

use super::ModeStatus;

/// Session-owned state for DEC private focus reporting mode `?1004`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FocusReporting {
    enabled: bool,
    observed: Option<TerminalFocusEvent>,
}

impl FocusReporting {
    pub(crate) const MODE: usize = 1004;

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Records the latest host focus state and reports whether it is a reportable edge.
    pub(crate) fn observe(&mut self, next: TerminalFocusEvent) -> bool {
        let changed = self.observed != Some(next);
        self.observed = Some(next);
        self.enabled && changed
    }

    pub(crate) fn mode_status(self) -> ModeStatus {
        ModeStatus::from(self.enabled)
    }

    pub(crate) fn reset_for_ris(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_reset_and_observe_without_reporting() {
        let mut reporting = FocusReporting::default();

        assert_eq!(reporting.mode_status(), ModeStatus::Reset);
        assert!(!reporting.observe(TerminalFocusEvent::Gained));

        reporting.set_enabled(true);
        assert!(!reporting.observe(TerminalFocusEvent::Gained));
        assert!(reporting.observe(TerminalFocusEvent::Lost));
    }

    #[test]
    fn should_report_only_real_edges_while_enabled() {
        let mut reporting = FocusReporting::default();
        reporting.set_enabled(true);

        assert!(reporting.observe(TerminalFocusEvent::Gained));
        assert!(!reporting.observe(TerminalFocusEvent::Gained));
        assert!(reporting.observe(TerminalFocusEvent::Lost));
        assert!(!reporting.observe(TerminalFocusEvent::Lost));
        assert!(reporting.observe(TerminalFocusEvent::Gained));
    }

    #[test]
    fn should_preserve_observation_across_mode_toggles() {
        let mut reporting = FocusReporting::default();
        assert!(!reporting.observe(TerminalFocusEvent::Lost));

        reporting.set_enabled(true);
        assert!(!reporting.observe(TerminalFocusEvent::Lost));
        assert!(reporting.observe(TerminalFocusEvent::Gained));

        reporting.set_enabled(false);
        assert!(!reporting.observe(TerminalFocusEvent::Lost));
        reporting.set_enabled(true);
        assert!(!reporting.observe(TerminalFocusEvent::Lost));
    }

    #[test]
    fn should_clear_mode_and_observation_for_ris() {
        let mut reporting = FocusReporting::default();
        reporting.set_enabled(true);
        assert!(reporting.observe(TerminalFocusEvent::Gained));

        reporting.reset_for_ris();

        assert_eq!(reporting.mode_status(), ModeStatus::Reset);
        reporting.set_enabled(true);
        assert!(reporting.observe(TerminalFocusEvent::Gained));
    }
}
