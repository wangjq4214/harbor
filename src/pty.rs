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

use crate::terminal::TerminalSize;

use winit::event_loop::EventLoopProxy;

use crate::app::AppEvent;

/// ConPTY-compatible terminal size; Windows APIs require signed 16-bit cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PtySize {
    /// Rows in the pseudo terminal viewport.
    pub(crate) rows: i16,
    /// Columns in the pseudo terminal viewport.
    pub(crate) cols: i16,
}

/// Shared buffer between PTY reader thread and main thread for output coalescing.
struct PendingState {
    buffer: Vec<u8>,
    wake_pending: bool,
}

/// Running shell session plus the background output reader.
pub(crate) struct PtySession {
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
    pub(crate) fn start_shell_reader<F>(
        size: TerminalSize,
        output_handler: F,
    ) -> anyhow::Result<Self>
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

    pub(crate) fn resize(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        tracing::info!(rows = size.rows, cols = size.cols, "resizing pty");

        self.pty.resize(PtySize::from_terminal(size)?)
    }

    /// Forwards keyboard input bytes to the underlying PTY.
    pub(crate) fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
        self.pty.write(data)
    }
}

/// Owns the shell process and coordinates PTY lifecycle.
///
/// Wraps `PtySession` behind an `Option` so the controller can exist before
/// the session is started.  Posts output bytes back to the event loop via the
/// proxy captured at construction.
pub(crate) struct Pty {
    session: Option<PtySession>,
    event_proxy: EventLoopProxy<AppEvent>,
    pending: Arc<Mutex<PendingState>>,
}

impl Pty {
    /// Creates a controller with no active PTY session.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            session: None,
            event_proxy,
            pending: Arc::new(Mutex::new(PendingState {
                buffer: Vec::new(),
                wake_pending: false,
            })),
        }
    }

    /// Starts the PTY session with a shell reader thread.
    ///
    /// The reader appends bytes to the shared `PendingState` buffer and posts a
    /// lightweight `AppEvent::PtyOutputReady` wake if no wake is already pending.
    /// Bulk processing happens on the main thread via `drain_output`.
    pub(crate) fn start(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        let event_proxy = self.event_proxy.clone();
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
            event_proxy.send_event(AppEvent::PtyOutputReady).is_ok()
        })?;

        self.session = Some(pty);
        Ok(())
    }

    /// Forwards a terminal resize to the PTY session.
    ///
    /// Failures are logged but not propagated — a PTY resize is best-effort
    /// and should never abort the UI.
    pub(crate) fn resize(&mut self, size: TerminalSize) {
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
    pub(crate) fn write(&mut self, data: &[u8]) {
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
    pub(crate) fn drain_output(&self) -> Vec<u8> {
        let mut state = self.pending.lock();
        let taken = std::mem::take(&mut state.buffer);
        state.wake_pending = false;
        taken
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Single append + take: verifies bytes are preserved and buffer is cleared after take.
    #[test]
    fn single_append_take() {
        let state = Arc::new(Mutex::new(PendingState {
            buffer: Vec::new(),
            wake_pending: false,
        }));
        {
            let mut s = state.lock();
            s.buffer.extend_from_slice(b"hello");
            s.wake_pending = true;
        }
        let taken = {
            let mut s = state.lock();
            let t = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            t
        };
        assert_eq!(taken, b"hello");
        assert!(state.lock().buffer.is_empty());
    }

    /// Multiple appends produce only one wake; take yields all concatenated bytes.
    #[test]
    fn multiple_appends_one_wake() {
        let state = Arc::new(Mutex::new(PendingState {
            buffer: Vec::new(),
            wake_pending: false,
        }));
        let mut wake_count = 0u32;
        for data in [b"abc", b"def", b"ghi"] {
            let mut s = state.lock();
            s.buffer.extend_from_slice(data);
            if !s.wake_pending {
                s.wake_pending = true;
                wake_count += 1;
            }
        }
        assert_eq!(
            wake_count, 1,
            "only the first append should set wake_pending"
        );

        let taken = {
            let mut s = state.lock();
            let t = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            t
        };
        assert_eq!(taken, b"abcdefghi");
        assert!(state.lock().buffer.is_empty());
    }

    /// Append A, drain (take + clear flag), then append B: B produces a second wake.
    #[test]
    fn drain_then_append_produces_second_wake() {
        let state = Arc::new(Mutex::new(PendingState {
            buffer: Vec::new(),
            wake_pending: false,
        }));

        // First batch
        {
            let mut s = state.lock();
            s.buffer.extend_from_slice(b"first");
            s.wake_pending = true;
        }
        let first = {
            let mut s = state.lock();
            let t = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            t
        };
        assert_eq!(first, b"first");

        // Second batch — wake_pending is clear, so append should set it
        let mut second_wake = false;
        {
            let mut s = state.lock();
            s.buffer.extend_from_slice(b"second");
            if !s.wake_pending {
                s.wake_pending = true;
                second_wake = true;
            }
        }
        assert!(second_wake, "second batch should set wake_pending");

        let second = {
            let mut s = state.lock();
            let t = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            t
        };
        assert_eq!(second, b"second");
    }

    /// Lock-serialized concurrent append: reader thread appends *after* drain has
    /// released the lock, verifies the reader re-arms wake_pending and no data is lost.
    #[test]
    fn concurrent_append_does_not_lose_data() {
        use std::sync::Barrier;

        let state = Arc::new(Mutex::new(PendingState {
            buffer: Vec::new(),
            wake_pending: false,
        }));
        let barrier = Arc::new(Barrier::new(2));

        // Pre-load data (reader append followed by wake)
        {
            let mut s = state.lock();
            s.buffer.extend_from_slice(b"first");
            s.wake_pending = true;
        }

        let state2 = state.clone();
        let barrier2 = barrier.clone();
        let handle = std::thread::spawn(move || {
            // Wait for main to signal that drain has completed.
            barrier2.wait();

            // Now append as the reader thread would after drain.
            // wake_pending must be false (drain cleared it), so we set it.
            let mut s = state2.lock();
            s.buffer.extend_from_slice(b"second");
            let should_wake = !s.wake_pending;
            if should_wake {
                s.wake_pending = true;
            }
            drop(s);
            should_wake
        });

        // Main: drain the first batch
        let first_batch = {
            let mut s = state.lock();
            let t = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            t
        };
        assert_eq!(first_batch, b"first");

        // Signal reader that drain is done — it will now append and re-arm
        barrier.wait();

        let should_wake = handle.join().unwrap();
        assert!(should_wake, "reader should re-arm wake_pending after drain");

        // Second batch from the reader's post-drain append
        let second_batch = {
            let mut s = state.lock();
            let t = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            t
        };
        assert_eq!(second_batch, b"second");
    }

    /// End-to-end simulated read-coalesce-process cycle.
    #[test]
    fn drain_output_end_to_end() {
        let pending = Arc::new(Mutex::new(PendingState {
            buffer: Vec::new(),
            wake_pending: false,
        }));

        // Simulate PTY reader thread: 5 consecutive appends, only the first sends an event
        let mut events_sent = 0u32;
        for chunk in [b"chunk1", b"chunk2", b"chunk3", b"chunk4", b"chunk5"] {
            let mut s = pending.lock();
            s.buffer.extend_from_slice(chunk);
            if !s.wake_pending {
                s.wake_pending = true;
                events_sent += 1;
            }
        }
        assert_eq!(events_sent, 1);

        // Simulate drain_output
        let batch = {
            let mut s = pending.lock();
            let t = std::mem::take(&mut s.buffer);
            s.wake_pending = false;
            t
        };
        assert_eq!(batch, b"chunk1chunk2chunk3chunk4chunk5");
        assert!(pending.lock().buffer.is_empty());
        assert!(!pending.lock().wake_pending);
    }
}
