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
use std::sync::Arc;
use std::thread::JoinHandle;
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

// ── PtySession ───────────────────────────────────────────────────────────────

/// Running shell session plus the background output reader.
pub struct PtySession {
    /// Platform-owned pseudo terminal and child-process handles.
    pty: RawPty,
    /// Joining is unnecessary on shutdown; the handle keeps the reader thread owned.
    _reader: JoinHandle<()>,
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
        let reader = std::thread::spawn(|| pump_pty_output(reader, output_handler));

        Ok(Self {
            pty,
            _reader: reader,
        })
    }

    pub fn resize(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        tracing::info!(rows = size.rows, cols = size.cols, "resizing pty");

        self.pty.resize(PtySize::from_terminal(size)?)
    }

    /// Forwards keyboard input bytes to the underlying PTY.
    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
        self.pty.write(data)
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

fn pump_pty_output<F>(mut reader: PtyReader, output_handler: F)
where
    F: Fn(Vec<u8>) -> bool,
{
    let mut buffer = [0_u8; 4096];
    tracing::info!("pty output pump started");
    loop {
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
                // A false return means the UI event loop rejected the message, so the
                // shell output pump should terminate instead of buffering unreachable data.
                if !output_handler(buffer[..bytes].to_vec()) {
                    tracing::info!("pty output pump stopped after handler rejection");
                    break;
                }
            }
            Err(error) => {
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
}
