//! Application-owned file observation, independent of widget HMR.
//!
//! Poll contents, not an open file handle or timestamps: editor replacements and
//! missing parent directories must not detach observation. Never parse or publish
//! settings here; a wake asks the application thread to read the *current* path.
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const SETTLE_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug)]
pub(crate) enum WatchEvent {
    Reload,
    /// Only the error kind crosses the thread boundary, never file contents.
    Unavailable(io::ErrorKind),
}

#[derive(PartialEq, Eq)]
enum Sample {
    Contents(Vec<u8>),
    Unavailable(io::ErrorKind),
}

impl Sample {
    fn read(path: &std::path::Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => Self::Contents(bytes),
            Err(error) => Self::Unavailable(error.kind()),
        }
    }
}

#[derive(Default)]
struct Observation {
    latest: Option<Sample>,
    changed_at: Option<Instant>,
}

impl Observation {
    fn observe(&mut self, sample: Sample, now: Instant, settle: Duration) -> Option<WatchEvent> {
        if self.latest.as_ref() != Some(&sample) {
            self.latest = Some(sample);
            self.changed_at = Some(now);
        }
        let changed = self.changed_at?;
        if now.saturating_duration_since(changed) < settle {
            return None;
        }
        self.changed_at = None;
        Some(match self.latest.as_ref()? {
            // A missing file is a strict reload failure, not startup fallback.
            Sample::Contents(_) | Sample::Unavailable(io::ErrorKind::NotFound) => {
                WatchEvent::Reload
            }
            Sample::Unavailable(kind) => WatchEvent::Unavailable(*kind),
        })
    }
}

/// A single wake drains the latest settled observation, including faults. This
/// prevents an old queued fault from hiding a recovery whose wake was coalesced.
#[derive(Default)]
struct PendingObservation(Mutex<Option<WatchEvent>>);

impl PendingObservation {
    fn replace(&self, event: WatchEvent) -> bool {
        self.0.lock().unwrap().replace(event).is_none()
    }

    fn take(&self) -> Option<WatchEvent> {
        self.0.lock().unwrap().take()
    }
}

pub(crate) struct ConfigurationWatch {
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<()>>,
    pending: Arc<PendingObservation>,
}

impl ConfigurationWatch {
    pub(crate) fn start(
        path: Option<PathBuf>,
        emit: impl Fn() -> bool + Send + 'static,
    ) -> io::Result<Self> {
        Self::start_with_timing(path, emit, POLL_INTERVAL, SETTLE_INTERVAL)
    }

    fn start_with_timing(
        path: Option<PathBuf>,
        emit: impl Fn() -> bool + Send + 'static,
        poll: Duration,
        settle: Duration,
    ) -> io::Result<Self> {
        let path = path.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not resolve configuration path",
            )
        })?;
        let (stop, stopped) = mpsc::channel();
        let pending = Arc::new(PendingObservation::default());
        let worker_pending = Arc::clone(&pending);
        let worker = thread::Builder::new()
            .name("configuration-watch".into())
            .spawn(move || {
                let mut observation = Observation::default();
                loop {
                    if let Some(event) =
                        observation.observe(Sample::read(&path), Instant::now(), settle)
                    {
                        // Take on the UI thread happens before its current-file
                        // read. A later observation can then post a fresh wake.
                        if worker_pending.replace(event) && !emit() {
                            break;
                        }
                    }
                    match stopped.recv_timeout(poll) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
            pending,
        })
    }

    pub(crate) fn take_pending(&self) -> Option<WatchEvent> {
        self.pending.take()
    }
}

impl Drop for ConfigurationWatch {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Suppress only consecutive identical automatic failures, not manual feedback,
/// changed failures, or successful recovery. Store already-redacted messages.
#[derive(Default)]
pub(crate) struct ReloadFeedback {
    last_failure: Option<String>,
}

impl ReloadFeedback {
    pub(crate) fn failure(&mut self, message: &str, automatic: bool) -> bool {
        let repeated = self.last_failure.as_deref() == Some(message);
        self.last_failure = Some(message.to_owned());
        !automatic || !repeated
    }

    pub(crate) fn clear(&mut self) {
        self.last_failure = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_fault_cannot_hide_a_coalesced_recovery() {
        let pending = PendingObservation::default();
        assert!(pending.replace(WatchEvent::Reload)); // queues one wake
        assert!(!pending.replace(WatchEvent::Unavailable(io::ErrorKind::PermissionDenied)));
        assert!(!pending.replace(WatchEvent::Reload)); // recovery replaces stale fault
        assert!(matches!(pending.take(), Some(WatchEvent::Reload)));
        assert!(pending.take().is_none());
        assert!(pending.replace(WatchEvent::Unavailable(io::ErrorKind::Other)));
        assert!(matches!(
            pending.take(),
            Some(WatchEvent::Unavailable(io::ErrorKind::Other))
        ));
        assert!(pending.replace(WatchEvent::Reload)); // after take, a fresh wake is required
    }

    #[test]
    fn debounce_replacement_missing_and_runtime_fault_recovery() {
        let mut observer = Observation::default();
        let now = Instant::now();
        let settle = Duration::from_millis(300);
        let sample = |bytes: &[u8]| Sample::Contents(bytes.to_vec());
        assert!(observer.observe(sample(b"first"), now, settle).is_none());
        assert!(
            observer
                .observe(
                    Sample::Unavailable(io::ErrorKind::NotFound),
                    now + settle,
                    settle
                )
                .is_none()
        );
        assert!(
            observer
                .observe(sample(b"invalid"), now + settle * 2, settle)
                .is_none()
        );
        assert!(
            observer
                .observe(sample(b"final"), now + settle * 3, settle)
                .is_none()
        );
        assert!(matches!(
            observer.observe(sample(b"final"), now + settle * 4, settle),
            Some(WatchEvent::Reload)
        ));
        assert!(
            observer
                .observe(sample(b"final"), now + settle * 5, settle)
                .is_none()
        );
        assert!(
            observer
                .observe(
                    Sample::Unavailable(io::ErrorKind::PermissionDenied),
                    now + settle * 6,
                    settle
                )
                .is_none()
        );
        assert!(matches!(
            observer.observe(
                Sample::Unavailable(io::ErrorKind::PermissionDenied),
                now + settle * 7,
                settle
            ),
            Some(WatchEvent::Unavailable(io::ErrorKind::PermissionDenied))
        ));
        assert!(
            observer
                .observe(
                    Sample::Unavailable(io::ErrorKind::PermissionDenied),
                    now + settle * 8,
                    settle
                )
                .is_none()
        );
        assert!(
            observer
                .observe(sample(b"final"), now + settle * 9, settle)
                .is_none()
        );
        assert!(matches!(
            observer.observe(sample(b"final"), now + settle * 10, settle),
            Some(WatchEvent::Reload)
        ));
    }

    #[test]
    fn persistent_missing_requests_strict_reload_and_error_changes_are_visible() {
        let mut observer = Observation::default();
        let now = Instant::now();
        assert!(matches!(
            observer.observe(
                Sample::Unavailable(io::ErrorKind::NotFound),
                now,
                Duration::ZERO
            ),
            Some(WatchEvent::Reload)
        ));
        assert!(matches!(
            observer.observe(
                Sample::Unavailable(io::ErrorKind::PermissionDenied),
                now,
                Duration::ZERO
            ),
            Some(WatchEvent::Unavailable(io::ErrorKind::PermissionDenied))
        ));
        assert!(matches!(
            observer.observe(
                Sample::Unavailable(io::ErrorKind::Other),
                now,
                Duration::ZERO
            ),
            Some(WatchEvent::Unavailable(io::ErrorKind::Other))
        ));
    }

    #[test]
    fn automatic_failure_suppression_preserves_manual_changed_errors_and_recovery() {
        let mut feedback = ReloadFeedback::default();
        assert!(feedback.failure("invalid color", true));
        assert!(!feedback.failure("invalid color", true));
        assert!(feedback.failure("invalid font", true));
        assert!(feedback.failure("invalid font", false));
        feedback.clear();
        assert!(feedback.failure("invalid font", true));
        assert!(!feedback.failure("invalid font", true));
        feedback.clear(); // an intervening watcher fault replaces the visible error
        assert!(feedback.failure("invalid font", true)); // surface remaining config error
    }

    #[test]
    fn setup_failure_is_returned_and_drop_interrupts_wait() {
        assert!(ConfigurationWatch::start(None, || true).is_err());
        let watch = ConfigurationWatch::start_with_timing(
            Some(std::env::temp_dir().join("harbor-missing-config-test")),
            || true,
            Duration::from_secs(60),
            Duration::from_secs(60),
        )
        .unwrap();
        let now = Instant::now();
        drop(watch);
        assert!(now.elapsed() < Duration::from_secs(2));
    }
}
