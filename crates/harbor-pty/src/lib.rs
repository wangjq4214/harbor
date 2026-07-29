//! PTY (pseudo-terminal) abstraction for Harbor.
//!
//! Manages the shell child process and provides transferable endpoints
//! (`PtyEndpoints`) for the terminal engine.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::{
    io::{self, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};
#[cfg(unix)]
use unix::{Pty as RawPty, PtyReader, PtyWriter as RawPtyWriter};
#[cfg(windows)]
use windows::{Pty as RawPty, PtyReader, PtyWriter as RawPtyWriter};

use anyhow::ensure;
use harbor_types::TerminalSize;

// ── PtySize ──────────────────────────────────────────────────────────────────

/// ConPTY-compatible terminal size; Windows APIs require signed 16-bit cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PtySize {
    /// Rows in the pseudo terminal viewport.
    pub rows: i16,
    /// Columns in the pseudo terminal viewport.
    pub cols: i16,
}

// ── ReaderShutdown ───────────────────────────────────────────────────────────
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

    #[cfg(test)]
    pub(crate) fn stopping(&self) -> Arc<AtomicBool> {
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

// ── PtySize ──────────────────────────────────────────────────────────────────
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

/// The read, write, and lifecycle capabilities of one live PTY.
///
/// [`PtyEndpoints::into_parts`] is deliberately the only way to transfer its
/// capabilities. The opaque [`PtyControl`] keeps ConPTY resources inside this
/// crate and requires the terminal reader thread for safe shutdown.
pub struct PtyEndpoints {
    reader: Option<PtyReaderEndpoint>,
    writer: Option<PtyWriter>,
    control: Option<PtyControl>,
}

/// Read endpoint that acknowledges shutdown only after its blocking read has
/// returned and the reader thread releases it.
pub struct PtyReaderEndpoint {
    reader: PtyReader,
    _completion: ReaderCompletion,
}

/// Write-only PTY input endpoint for the terminal UI thread.
pub struct PtyWriter {
    writer: RawPtyWriter,
}

/// Opaque owner for PTY resize and shutdown resources.
///
/// It can only be created by [`PtyEndpoints`]. Its public operations are
/// capability-oriented so platform handles never cross the crate boundary.
pub struct PtyControl {
    pty: Option<RawPty>,
    reader_shutdown: Option<ReaderShutdown>,
}

impl PtyEndpoints {
    /// Spawns a shell and returns its independent terminal-owned endpoints.
    pub fn spawn_shell(size: TerminalSize) -> anyhow::Result<Self> {
        let (pty, reader) = RawPty::spawn_shell(PtySize::from_terminal(size)?)?;
        let (reader_shutdown, completion) = ReaderShutdown::new();
        let (reader, writer, pty) = pty.into_endpoints(reader);
        Ok(Self {
            reader: Some(PtyReaderEndpoint {
                reader,
                _completion: completion,
            }),
            writer: Some(PtyWriter { writer }),
            control: Some(PtyControl {
                pty: Some(pty),
                reader_shutdown: Some(reader_shutdown),
            }),
        })
    }

    /// Transfers the I/O endpoints and their shutdown owner to a terminal.
    pub fn into_parts(mut self) -> (PtyReaderEndpoint, PtyWriter, PtyControl) {
        (
            self.reader
                .take()
                .expect("pty endpoints can only be split once"),
            self.writer
                .take()
                .expect("pty endpoints can only be split once"),
            self.control
                .take()
                .expect("pty endpoints can only be split once"),
        )
    }
}

impl Drop for PtyEndpoints {
    fn drop(&mut self) {
        // No reader thread exists until the endpoints are transferred to Terminal,
        // so ordinary platform teardown is safe on this unstarted path.
        if let Some(control) = self.control.take() {
            control.close_without_reader();
        }
    }
}

impl Read for PtyReaderEndpoint {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buffer).map_err(endpoint_io_error)
    }
}

impl Write for PtyWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.writer.write(buffer).map_err(endpoint_io_error)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PtyControl {
    /// Resizes the live pseudo terminal.
    pub fn resize(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        self.pty
            .as_mut()
            .expect("pty control is unavailable during shutdown")
            .resize(PtySize::from_terminal(size)?)
    }

    /// Transfers the PTY and terminal reader to the platform shutdown reaper.
    ///
    /// This never waits on the UI thread. On Windows the reaper first cancels
    /// the blocking reader and observes its completion before ConPTY is closed.
    pub fn shutdown(mut self, reader: JoinHandle<()>) {
        let pty = self
            .pty
            .take()
            .expect("pty control must retain its platform pty until shutdown");
        let reader_shutdown = self
            .reader_shutdown
            .take()
            .expect("pty control must retain its reader shutdown protocol");
        RawPty::shutdown(pty, reader, reader_shutdown);
    }

    fn close_without_reader(mut self) {
        drop(self.pty.take());
        drop(self.reader_shutdown.take());
    }
}

impl Drop for PtyControl {
    fn drop(&mut self) {
        if let Some(pty) = self.pty.take() {
            // A control separated from its terminal has no reader JoinHandle to
            // cancel. Leaking is safer than closing ConPTY while ReadFile may
            // still own the output handle; normal endpoint and Terminal drops
            // take an explicit safe teardown path above.
            tracing::error!("pty control dropped without its terminal reader; leaking session");
            std::mem::forget(pty);
        }
    }
}

fn endpoint_io_error(error: anyhow::Error) -> io::Error {
    io::Error::other(error)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
    fn windows_endpoints_write_resize_and_shutdown_without_blocking_ui() {
        use std::time::{Duration, Instant};

        let (mut reader, mut writer, mut control) =
            PtyEndpoints::spawn_shell(TerminalSize { rows: 24, cols: 80 })
                .expect("Windows ConPTY endpoints should start")
                .into_parts();
        let reader_thread = std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while reader.read(&mut buffer).is_ok_and(|length| length != 0) {}
        });

        writer
            .write_all(b"\r")
            .expect("endpoint writer should forward all bytes");
        control
            .resize(TerminalSize { rows: 25, cols: 81 })
            .expect("endpoint control should forward resize");
        drop(writer);

        let started = Instant::now();
        control.shutdown(reader_thread);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "endpoint shutdown must transfer cleanup rather than block the UI"
        );
    }
}
