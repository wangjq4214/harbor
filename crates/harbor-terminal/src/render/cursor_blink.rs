//! Idle cursor blink phase machine keyed by controlled instants.

use harbor_config::BLINK_INTERVAL_MS;
use std::time::{Duration, Instant};

/// Owns the blink cycle start and pending immediate-redraw flag for a cursor.
#[derive(Clone, Debug)]
pub struct CursorBlinkState {
    cycle_start: Instant,
    pending_redraw: bool,
}

impl CursorBlinkState {
    /// Starts a visible-phase cycle at `now` without requesting an immediate redraw.
    pub fn new(now: Instant) -> Self {
        Self {
            cycle_start: now,
            pending_redraw: false,
        }
    }

    /// Restarts the cycle at the visible phase and marks an immediate redraw.
    pub fn reset(&mut self, now: Instant) {
        self.cycle_start = now;
        self.pending_redraw = true;
    }

    /// Whether the blink phase at `now` is the visible half of the cycle.
    pub fn phase_visible(&self, now: Instant) -> bool {
        phase_index(now, self.cycle_start).is_multiple_of(2)
    }

    /// Earliest Instant at which the blink phase after `now` begins.
    pub fn next_deadline(&self, now: Instant) -> Instant {
        let next_ms = (phase_index(now, self.cycle_start) + 1) * BLINK_INTERVAL_MS;
        self.cycle_start
            .checked_add(Duration::from_millis(next_ms))
            .unwrap_or(now)
    }

    /// Whether a blink reset still needs an immediate host frame.
    pub fn pending_redraw(&self) -> bool {
        self.pending_redraw
    }

    /// Clears and returns the pending immediate-redraw flag.
    pub fn take_pending_redraw(&mut self) -> bool {
        std::mem::take(&mut self.pending_redraw)
    }
}

fn phase_index(now: Instant, cycle_start: Instant) -> u64 {
    now.saturating_duration_since(cycle_start).as_millis() as u64 / BLINK_INTERVAL_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Instant {
        Instant::now()
    }

    #[test]
    fn visible_at_cycle_start_with_next_deadline() {
        let t0 = base();
        let blink = CursorBlinkState::new(t0);
        assert!(blink.phase_visible(t0));
        assert_eq!(
            blink.next_deadline(t0),
            t0 + Duration::from_millis(BLINK_INTERVAL_MS)
        );
        assert!(!blink.pending_redraw());
    }

    #[test]
    fn phase_flips_at_interval_boundaries() {
        let t0 = base();
        let blink = CursorBlinkState::new(t0);
        let just_before = t0 + Duration::from_millis(BLINK_INTERVAL_MS - 1);
        let at_boundary = t0 + Duration::from_millis(BLINK_INTERVAL_MS);
        let after = t0 + Duration::from_millis(BLINK_INTERVAL_MS + 1);

        assert!(blink.phase_visible(just_before));
        assert_eq!(blink.next_deadline(just_before), at_boundary);

        assert!(!blink.phase_visible(at_boundary));
        assert_eq!(
            blink.next_deadline(at_boundary),
            t0 + Duration::from_millis(BLINK_INTERVAL_MS * 2)
        );

        assert!(!blink.phase_visible(after));
        assert_eq!(
            blink.next_deadline(after),
            t0 + Duration::from_millis(BLINK_INTERVAL_MS * 2)
        );
    }

    #[test]
    fn late_query_advances_deadline_to_next_future_boundary() {
        let t0 = base();
        let blink = CursorBlinkState::new(t0);
        let late = t0 + Duration::from_millis(BLINK_INTERVAL_MS * 5 + 10);
        assert!(!blink.phase_visible(late));
        assert_eq!(
            blink.next_deadline(late),
            t0 + Duration::from_millis(BLINK_INTERVAL_MS * 6)
        );
    }

    #[test]
    fn reset_from_hidden_phase_is_visible_and_pending() {
        let t0 = base();
        let mut blink = CursorBlinkState::new(t0);
        let hidden = t0 + Duration::from_millis(BLINK_INTERVAL_MS);
        assert!(!blink.phase_visible(hidden));

        blink.reset(hidden);
        assert!(blink.phase_visible(hidden));
        assert!(blink.pending_redraw());
        assert_eq!(
            blink.next_deadline(hidden),
            hidden + Duration::from_millis(BLINK_INTERVAL_MS)
        );
        assert!(blink.take_pending_redraw());
        assert!(!blink.pending_redraw());
    }

    #[test]
    fn should_return_false_when_take_pending_without_reset() {
        // Arrange
        let mut blink = CursorBlinkState::new(base());

        // Act
        let taken = blink.take_pending_redraw();

        // Assert
        assert!(!taken);
        assert!(!blink.pending_redraw());
    }

    #[test]
    fn should_restart_visible_cycle_when_reset_during_visible_phase() {
        // Arrange
        let t0 = base();
        let mut blink = CursorBlinkState::new(t0);
        let mid_visible = t0 + Duration::from_millis(BLINK_INTERVAL_MS / 2);
        assert!(blink.phase_visible(mid_visible));

        // Act
        blink.reset(mid_visible);

        // Assert
        assert!(blink.phase_visible(mid_visible));
        assert!(blink.pending_redraw());
        assert_eq!(
            blink.next_deadline(mid_visible),
            mid_visible + Duration::from_millis(BLINK_INTERVAL_MS)
        );
    }

    #[test]
    fn should_treat_time_before_cycle_start_as_first_visible_phase() {
        // Arrange
        let t0 = base();
        let blink = CursorBlinkState::new(t0 + Duration::from_millis(BLINK_INTERVAL_MS));
        let before_start = t0;

        // Act + Assert
        assert!(blink.phase_visible(before_start));
        assert_eq!(
            blink.next_deadline(before_start),
            t0 + Duration::from_millis(BLINK_INTERVAL_MS * 2)
        );
    }
}
