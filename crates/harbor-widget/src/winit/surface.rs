//! Per-window drawable viewport, configuration dirtiness, and recovery budget.

use crate::renderer::Viewport;

/// Per-window drawable viewport, configuration dirtiness, and recovery budget.
pub(super) struct SurfaceState {
    viewport: Viewport,
    configuration_dirty: bool,
    recovery_attempted: bool,
}

impl SurfaceState {
    pub(super) fn new(width: u32, height: u32, scale: f32) -> Self {
        Self {
            viewport: Viewport::new(width, height, scale),
            configuration_dirty: width > 0 && height > 0,
            recovery_attempted: false,
        }
    }

    pub(super) fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub(super) fn update(&mut self, width: u32, height: u32, scale: f32) -> bool {
        let next = Viewport::new(width, height, scale);
        if self.viewport == next {
            return false;
        }
        self.viewport = next;
        self.configuration_dirty = true;
        self.recovery_attempted = false;
        true
    }

    pub(super) fn can_acquire(&self) -> bool {
        self.viewport.is_drawable()
    }

    pub(super) fn configuration_dirty(&self) -> bool {
        self.configuration_dirty
    }

    pub(super) fn mark_configured(&mut self) {
        self.configuration_dirty = false;
    }

    pub(super) fn allow_recovery_retry(&mut self) -> bool {
        if self.recovery_attempted {
            return false;
        }
        self.recovery_attempted = true;
        true
    }

    pub(super) fn reset_after_success(&mut self) {
        self.recovery_attempted = false;
    }

    pub(super) fn reset_recovery_budget(&mut self) {
        self.recovery_attempted = false;
    }

    #[cfg(test)]
    pub(super) fn recovery_attempted(&self) -> bool {
        self.recovery_attempted
    }
}
