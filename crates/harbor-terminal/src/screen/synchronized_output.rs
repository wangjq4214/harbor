//! Saturating DEC private synchronized-output nesting (`?2026`).

use super::ModeStatus;

/// Session-owned nesting counter for DEC private mode `?2026`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SynchronizedOutput {
    depth: u32,
}

impl SynchronizedOutput {
    pub(crate) const MODE: usize = 2026;

    pub(crate) fn enable(&mut self) {
        self.depth = self.depth.saturating_add(1);
    }

    pub(crate) fn disable(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }

    pub(crate) fn ordinary_present_eligible(self) -> bool {
        self.depth == 0
    }

    pub(crate) fn mode_status(self) -> ModeStatus {
        ModeStatus::from(!self.ordinary_present_eligible())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_be_eligible_and_reset_when_default() {
        // Arrange
        let sync = SynchronizedOutput::default();

        // Act
        let eligible = sync.ordinary_present_eligible();
        let status = sync.mode_status();

        // Assert
        assert!(eligible);
        assert_eq!(status, ModeStatus::Reset);
    }

    #[test]
    fn should_be_ineligible_and_set_when_enabled_once() {
        // Arrange
        let mut sync = SynchronizedOutput::default();

        // Act
        sync.enable();

        // Assert
        assert!(!sync.ordinary_present_eligible());
        assert_eq!(sync.mode_status(), ModeStatus::Set);
    }

    #[test]
    fn should_restore_eligible_and_reset_when_matching_disable() {
        // Arrange
        let mut sync = SynchronizedOutput::default();
        sync.enable();

        // Act
        sync.disable();

        // Assert
        assert!(sync.ordinary_present_eligible());
        assert_eq!(sync.mode_status(), ModeStatus::Reset);
    }

    #[test]
    fn should_stay_ineligible_when_one_disable_leaves_nesting() {
        // Arrange
        let mut sync = SynchronizedOutput::default();
        sync.enable();
        sync.enable();

        // Act
        sync.disable();

        // Assert
        assert!(!sync.ordinary_present_eligible());
        assert_eq!(sync.mode_status(), ModeStatus::Set);
    }

    #[test]
    fn should_restore_eligible_when_nested_enables_are_fully_unwound() {
        // Arrange
        let mut sync = SynchronizedOutput::default();
        sync.enable();
        sync.enable();
        sync.disable();

        // Act
        sync.disable();

        // Assert
        assert!(sync.ordinary_present_eligible());
        assert_eq!(sync.mode_status(), ModeStatus::Reset);
    }

    #[test]
    fn should_keep_eligible_when_disabled_at_zero() {
        // Arrange
        let mut sync = SynchronizedOutput::default();

        // Act
        sync.disable();

        // Assert
        assert!(sync.ordinary_present_eligible());
        assert_eq!(sync.mode_status(), ModeStatus::Reset);
    }

    #[test]
    fn should_stay_set_when_enabled_at_u32_max() {
        // Arrange
        let mut sync = SynchronizedOutput { depth: u32::MAX };

        // Act
        sync.enable();

        // Assert
        assert!(!sync.ordinary_present_eligible());
        assert_eq!(sync.mode_status(), ModeStatus::Set);
    }

    #[test]
    fn should_remain_ineligible_when_disabled_once_after_saturation() {
        // Arrange
        let mut sync = SynchronizedOutput { depth: u32::MAX };
        sync.enable();

        // Act
        sync.disable();

        // Assert
        assert!(!sync.ordinary_present_eligible());
        assert_eq!(sync.mode_status(), ModeStatus::Set);
    }
}
