//! Host frame lifecycle telemetry: steady-state font metrics emission.

use std::time::{Duration, Instant};

/// Documented 5s dwell after first present for the `steady_state` lifecycle marker.
pub(crate) const FONT_STEADY_STATE_DWELL: Duration = Duration::from_secs(5);
/// Avoids an immediate redraw loop while a hidden startup surface is skipped.
pub(crate) const HIDDEN_STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameLifecycleEvent {
    FirstPresent,
    SteadyState { dwell_ms: u64 },
}

pub(crate) trait FrameLifecycleSink {
    fn emit(&self, event: FrameLifecycleEvent);
}

pub(crate) struct TracingFrameLifecycleSink;

impl FrameLifecycleSink for TracingFrameLifecycleSink {
    fn emit(&self, event: FrameLifecycleEvent) {
        match event {
            FrameLifecycleEvent::FirstPresent => tracing::info!(
                target: "harbor.font.lifecycle",
                phase = "first_present",
                "font lifecycle"
            ),
            FrameLifecycleEvent::SteadyState { dwell_ms } => tracing::info!(
                target: "harbor.font.lifecycle",
                phase = "steady_state",
                dwell_ms,
                "font lifecycle"
            ),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingFrameLifecycleSink {
    events: std::cell::RefCell<Vec<FrameLifecycleEvent>>,
}

#[cfg(test)]
impl RecordingFrameLifecycleSink {
    pub(crate) fn events(&self) -> Vec<FrameLifecycleEvent> {
        self.events.borrow().clone()
    }
}

#[cfg(test)]
impl FrameLifecycleSink for RecordingFrameLifecycleSink {
    fn emit(&self, event: FrameLifecycleEvent) {
        self.events.borrow_mut().push(event);
    }
}

/// Host frame lifecycle telemetry.
pub(crate) struct FrameState {
    /// Set after the first successful surface present.
    first_present_at: Option<Instant>,
    /// Once-only gate for the steady-state dwell marker.
    steady_state_emitted: bool,
    lifecycle: std::rc::Rc<dyn FrameLifecycleSink>,
}

impl FrameState {
    pub(crate) fn new(lifecycle: std::rc::Rc<dyn FrameLifecycleSink>) -> Self {
        Self {
            first_present_at: None,
            steady_state_emitted: false,
            lifecycle,
        }
    }

    /// Records the first successful present and emits `first_present` once.
    pub(crate) fn mark_first_present(&mut self) {
        self.mark_first_present_at(Instant::now());
    }

    pub(crate) fn mark_first_present_at(&mut self, at: Instant) {
        if self.first_present_at.is_some() {
            return;
        }
        self.first_present_at = Some(at);
        self.lifecycle.emit(FrameLifecycleEvent::FirstPresent);
    }

    /// Emits `steady_state` once after the documented dwell past first present.
    pub(crate) fn maybe_emit_steady_state_at(&mut self, now: Instant) {
        if self.steady_state_emitted {
            return;
        }
        let Some(presented_at) = self.first_present_at else {
            return;
        };
        let dwell = now.saturating_duration_since(presented_at);
        if dwell < FONT_STEADY_STATE_DWELL {
            return;
        }
        self.steady_state_emitted = true;
        self.lifecycle.emit(FrameLifecycleEvent::SteadyState {
            dwell_ms: dwell.as_millis() as u64,
        });
    }

    /// Returns the future font-lifecycle telemetry deadline, or emits the
    /// marker when due. Does not choose a winit control-flow mode.
    pub(crate) fn next_steady_state_deadline(&mut self) -> Option<Instant> {
        self.next_steady_state_deadline_at(Instant::now())
    }

    pub(crate) fn next_steady_state_deadline_at(&mut self, now: Instant) -> Option<Instant> {
        if self.steady_state_emitted {
            return None;
        }
        let presented_at = self.first_present_at?;
        let deadline = presented_at + FONT_STEADY_STATE_DWELL;
        if now >= deadline {
            self.maybe_emit_steady_state_at(now);
            return None;
        }
        Some(deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_frame() -> FrameState {
        FrameState::new(std::rc::Rc::new(TracingFrameLifecycleSink))
    }

    #[derive(Clone)]
    struct RecordingFrameLifecycleSinkHandle(std::rc::Rc<RecordingFrameLifecycleSink>);

    impl FrameLifecycleSink for RecordingFrameLifecycleSinkHandle {
        fn emit(&self, event: FrameLifecycleEvent) {
            self.0.emit(event);
        }
    }

    fn recording_frame() -> (FrameState, std::rc::Rc<RecordingFrameLifecycleSink>) {
        let sink = std::rc::Rc::new(RecordingFrameLifecycleSink::default());
        let handle = std::rc::Rc::new(RecordingFrameLifecycleSinkHandle(sink.clone()));
        (FrameState::new(handle), sink)
    }

    #[test]
    fn should_emit_first_present_once_when_marked() {
        let (mut frame, sink) = recording_frame();
        let at = Instant::now();

        frame.mark_first_present_at(at);
        frame.mark_first_present_at(at + Duration::from_millis(1));

        assert_eq!(sink.events(), vec![FrameLifecycleEvent::FirstPresent]);
    }

    #[test]
    fn should_not_emit_steady_state_when_first_present_missing() {
        let (mut frame, sink) = recording_frame();

        frame.maybe_emit_steady_state_at(Instant::now());

        assert!(sink.events().is_empty());
    }

    #[test]
    fn should_not_emit_steady_state_when_dwell_incomplete() {
        let (mut frame, sink) = recording_frame();
        let presented_at = Instant::now();
        let before_dwell = presented_at + FONT_STEADY_STATE_DWELL - Duration::from_millis(1);

        frame.mark_first_present_at(presented_at);
        frame.maybe_emit_steady_state_at(before_dwell);

        assert_eq!(sink.events(), vec![FrameLifecycleEvent::FirstPresent]);
    }

    #[test]
    fn should_emit_steady_state_once_when_dwell_elapsed() {
        let (mut frame, sink) = recording_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);

        frame.mark_first_present_at(presented_at);
        frame.maybe_emit_steady_state_at(after_dwell);
        frame.maybe_emit_steady_state_at(after_dwell + Duration::from_secs(1));

        assert_eq!(
            sink.events(),
            vec![
                FrameLifecycleEvent::FirstPresent,
                FrameLifecycleEvent::SteadyState { dwell_ms: 5050 },
            ]
        );
    }

    #[test]
    fn should_return_future_deadline_when_first_present_marked() {
        let (mut frame, _) = recording_frame();
        let presented_at = Instant::now();
        let expected_deadline = presented_at + FONT_STEADY_STATE_DWELL;

        frame.mark_first_present_at(presented_at);
        let deadline = frame.next_steady_state_deadline_at(presented_at);

        assert_eq!(deadline, Some(expected_deadline));
    }

    #[test]
    fn should_return_no_deadline_when_first_present_missing() {
        let (mut frame, _) = recording_frame();

        let deadline = frame.next_steady_state_deadline_at(Instant::now());

        assert_eq!(deadline, None);
    }

    #[test]
    fn should_emit_steady_state_without_deadline_when_dwell_already_elapsed() {
        let (mut frame, sink) = recording_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);

        frame.mark_first_present_at(presented_at);
        let deadline = frame.next_steady_state_deadline_at(after_dwell);

        assert_eq!(
            sink.events(),
            vec![
                FrameLifecycleEvent::FirstPresent,
                FrameLifecycleEvent::SteadyState { dwell_ms: 5050 },
            ]
        );
        assert_eq!(deadline, None);
    }

    #[test]
    fn should_return_no_deadline_when_steady_state_already_emitted() {
        // Arrange
        let mut frame = empty_frame();
        let presented_at = Instant::now();
        let after_dwell = presented_at + FONT_STEADY_STATE_DWELL + Duration::from_millis(50);
        frame.mark_first_present_at(presented_at);
        frame.maybe_emit_steady_state_at(after_dwell);

        // Act
        let deadline = frame.next_steady_state_deadline_at(after_dwell);

        // Assert
        assert_eq!(deadline, None);
    }
}
