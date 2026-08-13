use crate::contracts::FaceId;

/// Stable target used by the production lifecycle telemetry adapter.
pub(crate) const TARGET: &str = "harbor.font.lifecycle";

/// Source from which the process primary font was loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontSource {
    System,
    Configured,
}

impl FontSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Configured => "configured",
        }
    }
}

/// Domain events emitted while the font lifecycle progresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontLifecycleEvent {
    FontInit { source: FontSource, elapsed_ms: u64 },
    FirstFallback { scalar: char, face_id: FaceId },
}

/// Receives typed font lifecycle events.
pub(crate) trait FontLifecycleSink {
    fn emit(&self, event: FontLifecycleEvent);
}

/// Production adapter that exposes typed events through tracing.
#[derive(Default)]
pub(crate) struct TracingFontLifecycleSink;

impl FontLifecycleSink for TracingFontLifecycleSink {
    fn emit(&self, event: FontLifecycleEvent) {
        match event {
            FontLifecycleEvent::FontInit { source, elapsed_ms } => tracing::info!(
                target: TARGET,
                phase = "font_init",
                source = source.as_str(),
                elapsed_ms,
                "font lifecycle"
            ),
            FontLifecycleEvent::FirstFallback { scalar, face_id } => tracing::info!(
                target: TARGET,
                phase = "first_fallback",
                scalar = %scalar,
                face_id = face_id.get(),
                "font lifecycle"
            ),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingFontLifecycleSink {
    events: std::cell::RefCell<Vec<FontLifecycleEvent>>,
}

#[cfg(test)]
impl RecordingFontLifecycleSink {
    pub(crate) fn events(&self) -> Vec<FontLifecycleEvent> {
        self.events.borrow().clone()
    }
}

#[cfg(test)]
impl FontLifecycleSink for RecordingFontLifecycleSink {
    fn emit(&self, event: FontLifecycleEvent) {
        self.events.borrow_mut().push(event);
    }
}
