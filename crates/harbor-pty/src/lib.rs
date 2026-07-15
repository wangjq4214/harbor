//! PTY (pseudo-terminal) abstraction for Harbor.
//!
//! Manages the shell child process and provides a background reader thread
//! that coalesces output into a shared buffer. Wakes the event loop via the
//! [`WakeHandler`] trait when new output arrives.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};
#[cfg(unix)]
use unix::{Pty as RawPty, PtyReader};
#[cfg(windows)]
use windows::{Pty as RawPty, PtyReader};

use anyhow::ensure;
use harbor_types::TerminalSize;

// ── WakeHandler ──────────────────────────────────────────────────────────────

/// Trait for signaling the main event loop that PTY output is available.
///
/// The handler is cloned into the background reader thread. Implementations
/// typically wrap an `EventLoopProxy` or similar wake mechanism.
pub trait WakeHandler: Send + Sync + 'static {
    /// Signal that PTY output is ready for consumption. Returns `false` when
    /// the event loop has been dropped and the reader should terminate.
    fn wake(&self) -> bool;
}

// ── PtySize ──────────────────────────────────────────────────────────────────

/// ConPTY-compatible terminal size; Windows APIs require signed 16-bit cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PtySize {
    /// Rows in the pseudo terminal viewport.
    pub rows: i16,
    /// Columns in the pseudo terminal viewport.
    pub cols: i16,
}

// ── PendingState ─────────────────────────────────────────────────────────────

/// Shared buffer between PTY reader thread and main thread for output coalescing.
struct PendingState {
    buffer: Vec<u8>,
    wake_pending: bool,
}

/// Coordinates a reader shutdown with the platform-specific reaper.
pub(crate) struct ReaderShutdown {
    stopping: Arc<AtomicBool>,
    completed: mpsc::Receiver<()>,
}

struct ReaderCompletion {
    completed: Option<mpsc::Sender<()>>,
}

impl ReaderShutdown {
    fn new() -> (Self, ReaderCompletion) {
        let (completed_tx, completed) = mpsc::channel();
        (
            Self {
                stopping: Arc::new(AtomicBool::new(false)),
                completed,
            },
            ReaderCompletion {
                completed: Some(completed_tx),
            },
        )
    }

    fn stopping(&self) -> Arc<AtomicBool> {
        self.stopping.clone()
    }

    pub(crate) fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    pub(crate) fn wait_for_completion(&self, timeout: std::time::Duration) -> bool {
        matches!(
            self.completed.recv_timeout(timeout),
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected)
        )
    }
}

impl Drop for ReaderCompletion {
    fn drop(&mut self) {
        if let Some(completed) = self.completed.take() {
            let _ = completed.send(());
        }
    }
}

// ── PtySession ───────────────────────────────────────────────────────────────

/// Running shell session plus the background output reader.
pub struct PtySession {
    /// Platform-owned pseudo terminal and child-process handles.
    pty: Option<RawPty>,
    /// The reader confirms it has released its output handle before the platform PTY closes.
    reader: Option<JoinHandle<()>>,
    reader_shutdown: Option<ReaderShutdown>,
}

impl PtySize {
    fn from_terminal(size: TerminalSize) -> anyhow::Result<Self> {
        ensure!(
            size.rows <= i16::MAX as usize && size.cols <= i16::MAX as usize,
            "terminal size exceeds pty limits"
        );
        Ok(Self {
            rows: size.rows as i16,
            cols: size.cols as i16,
        })
    }
}

impl PtySession {
    pub fn start_shell_reader<F>(size: TerminalSize, output_handler: F) -> anyhow::Result<Self>
    where
        F: Fn(Vec<u8>) -> bool + Send + 'static,
    {
        // Convert once at the boundary so platform modules only deal with API-sized values.
        tracing::info!(
            rows = size.rows,
            cols = size.cols,
            "spawning pty shell reader"
        );

        let (pty, reader) = RawPty::spawn_shell(PtySize::from_terminal(size)?)?;
        tracing::info!("pty shell spawned");
        let (reader_shutdown, reader_completion) = ReaderShutdown::new();
        let stopping = reader_shutdown.stopping();
        let reader = std::thread::spawn(move || {
            let _reader_completion = reader_completion;
            pump_pty_output(reader, output_handler, &stopping);
        });

        Ok(Self {
            pty: Some(pty),
            reader: Some(reader),
            reader_shutdown: Some(reader_shutdown),
        })
    }

    pub fn resize(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        tracing::info!(rows = size.rows, cols = size.cols, "resizing pty");

        self.pty
            .as_mut()
            .expect("pty session is unavailable during shutdown")
            .resize(PtySize::from_terminal(size)?)
    }

    /// Forwards keyboard input bytes to the underlying PTY.
    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
        self.pty
            .as_mut()
            .expect("pty session is unavailable during shutdown")
            .write(data)
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let pty = self
                .pty
                .take()
                .expect("pty session must own its platform pty until shutdown");
            let reader = self
                .reader
                .take()
                .expect("pty session must own its output reader until shutdown");
            let reader_shutdown = self
                .reader_shutdown
                .take()
                .expect("pty session must own its reader shutdown protocol");
            RawPty::shutdown(pty, reader, reader_shutdown);
        }

        #[cfg(not(windows))]
        {
            drop(self.reader_shutdown.take());
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
}

// ── Pty ──────────────────────────────────────────────────────────────────────

/// Owns the shell process and coordinates PTY lifecycle.
///
/// Wraps `PtySession` behind an `Option` so the controller can exist before
/// the session is started.  Posts output bytes back to the event loop via the
/// [`WakeHandler`] captured at construction.
pub struct Pty {
    session: Option<PtySession>,
    wake_handler: Arc<dyn WakeHandler>,
    pending: Arc<Mutex<PendingState>>,
}

impl Pty {
    /// Creates a controller with no active PTY session.
    pub fn new(wake_handler: impl WakeHandler) -> Self {
        Self {
            session: None,
            wake_handler: Arc::new(wake_handler),
            pending: Arc::new(Mutex::new(PendingState {
                buffer: Vec::new(),
                wake_pending: false,
            })),
        }
    }

    /// Starts the PTY session with a shell reader thread.
    ///
    /// The reader appends bytes to the shared `PendingState` buffer and calls
    /// `wake_handler.wake()` if no wake is already pending. Bulk processing
    /// happens on the main thread via [`drain_output`](Self::drain_output).
    pub fn start(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        let wake_handler = Arc::clone(&self.wake_handler);
        let pending = self.pending.clone();

        tracing::info!(rows = size.rows, cols = size.cols, "starting pty");
        let pty = PtySession::start_shell_reader(size, move |output| {
            {
                let mut state = pending.lock();
                state.buffer.extend_from_slice(&output);
                if state.wake_pending {
                    return true;
                }
                state.wake_pending = true;
            }
            wake_handler.wake()
        })?;

        self.session = Some(pty);
        Ok(())
    }

    /// Forwards a terminal resize to the PTY session.
    ///
    /// Failures are logged but not propagated — a PTY resize is best-effort
    /// and should never abort the UI.
    pub fn resize(&mut self, size: TerminalSize) {
        if let Some(session) = self.session.as_mut()
            && let Err(error) = session.resize(size)
        {
            tracing::error!(error = %format_args!("{error:#}"), "failed to resize pty");
        }
    }

    /// Writes keyboard input bytes to the PTY's stdin pipe.
    ///
    /// `None` session and write failures are both logged as warnings — the
    /// write is advisory and should never crash the UI.
    pub fn write(&mut self, data: &[u8]) {
        let Some(session) = self.session.as_mut() else {
            tracing::warn!("no pty session to write keyboard input to");
            return;
        };
        if let Err(error) = session.write(data) {
            tracing::warn!(
                error = %format_args!("{error:#}"),
                "failed to write keyboard input to pty"
            );
        } else {
            tracing::debug!(
                bytes = %String::from_utf8_lossy(data).escape_debug(),
                "pty write"
            );
        }
    }

    /// Atomically takes all pending PTY output bytes and clears the wake flag.
    /// Returns an empty `Vec` when there is nothing to process.
    pub fn drain_output(&self) -> Vec<u8> {
        let mut state = self.pending.lock();
        let taken = std::mem::take(&mut state.buffer);
        state.wake_pending = false;
        taken
    }
}

// ── pump_pty_output ──────────────────────────────────────────────────────────

fn pump_pty_output<F>(mut reader: PtyReader, output_handler: F, stopping: &AtomicBool)
where
    F: Fn(Vec<u8>) -> bool,
{
    let mut buffer = [0_u8; 4096];
    tracing::info!("pty output pump started");
    loop {
        // The reaper keeps cancelling until this acknowledgement path runs, so a read that
        // completes normally during shutdown cannot lead to another blocking ReadFile.
        if stopping.load(Ordering::Acquire) {
            tracing::debug!("pty output pump stopped before read");
            break;
        }

        match reader.read(&mut buffer) {
            Ok(0) => {
                tracing::info!("pty output stream reached eof");
                break;
            }
            Ok(bytes) => {
                tracing::debug!(
                    bytes = %String::from_utf8_lossy(&buffer[..bytes]).escape_debug(),
                    "pty output"
                );
                if !output_handler(buffer[..bytes].to_vec()) {
                    tracing::info!("pty output pump stopped after handler rejection");
                    break;
                }
            }
            Err(error) => {
                #[cfg(windows)]
                if stopping.load(Ordering::Acquire) && PtyReader::is_shutdown_error(&error) {
                    tracing::debug!("pty output read cancelled during shutdown");
                    break;
                }

                tracing::error!(error = %format_args!("{error:#}"), "failed to read pty output");
                break;
            }
        }
    }
    tracing::info!("pty output pump stopped");
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct TestWakeHandler(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl WakeHandler for TestWakeHandler {
        fn wake(&self) -> bool {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        }
    }

    #[test]
    fn pty_controller_created_without_session() {
        let handler = TestWakeHandler(Default::default());
        let pty: Pty = Pty::new(handler);
        let output = pty.drain_output();
        assert!(output.is_empty());
    }

    #[test]
    fn pty_size_from_terminal_clamps() {
        let size = TerminalSize {
            rows: 100,
            cols: 200,
        };
        let pty_size = PtySize::from_terminal(size).unwrap();
        assert_eq!(pty_size.rows, 100);
        assert_eq!(pty_size.cols, 200);
    }

    #[test]
    fn pty_size_overflow_rejected() {
        let size = TerminalSize {
            rows: i16::MAX as usize + 1,
            cols: 80,
        };
        assert!(PtySize::from_terminal(size).is_err());
    }

    #[test]
    fn pending_state_drain_clears_wake_flag() {
        let state = PendingState {
            buffer: vec![1, 2, 3],
            wake_pending: true,
        };
        let pending = Arc::new(Mutex::new(state));
        {
            let mut s = pending.lock();
            let taken = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            assert_eq!(taken, vec![1, 2, 3]);
        }
        let s = pending.lock();
        assert!(!s.wake_pending);
    }
    #[test]
    fn reader_stop_acknowledges_after_normal_read_without_reentry() {
        use std::{
            sync::{
                atomic::{AtomicUsize, Ordering},
                mpsc,
            },
            time::Duration,
        };

        let (shutdown, completion) = ReaderShutdown::new();
        let stopping = shutdown.stopping();
        let reads = Arc::new(AtomicUsize::new(0));
        let (normal_read_finished, wait_for_stop) = mpsc::channel();
        let (allow_reentry, reentry_allowed) = mpsc::channel();
        let thread_reads = Arc::clone(&reads);
        let reader = std::thread::spawn(move || {
            let _completion = completion;
            thread_reads.fetch_add(1, Ordering::Relaxed);
            normal_read_finished.send(()).unwrap();
            reentry_allowed.recv().unwrap();

            if !stopping.load(Ordering::Acquire) {
                thread_reads.fetch_add(1, Ordering::Relaxed);
            }
        });

        wait_for_stop.recv_timeout(Duration::from_secs(1)).unwrap();
        shutdown.request_stop();
        allow_reentry.send(()).unwrap();
        assert!(
            shutdown.wait_for_completion(Duration::from_secs(1)),
            "reader must acknowledge that it will not re-enter ReadFile"
        );
        reader.join().unwrap();
        assert_eq!(reads.load(Ordering::Relaxed), 1);
    }

    #[cfg(windows)]
    #[test]
    fn windows_idle_reader_drop_returns_before_shutdown_budget() {
        use std::time::{Duration, Instant};

        let session = PtySession::start_shell_reader(TerminalSize { rows: 24, cols: 80 }, |_| true)
            .expect("Windows ConPTY session should start");
        std::thread::sleep(Duration::from_millis(25));

        let started = Instant::now();
        drop(session);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "PtySession::drop must transfer shutdown rather than wait for the reader"
        );
    }
}
