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
use unix::{Pty as RawPty, PtyReader, PtyWriter as RawPtyWriter, reader_io_error};
#[cfg(windows)]
use windows::{Pty as RawPty, PtyReader, PtyWriter as RawPtyWriter, reader_io_error};

use anyhow::ensure;

/// Per-child launch policy. Environment values are never included in Debug output.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct ShellCommand {
    pub program: Option<String>,
    pub args: Vec<String>,
    pub environment: std::collections::BTreeMap<String, String>,
    pub directory: Option<std::path::PathBuf>,
}

impl ShellCommand {
    pub fn new(program: Option<String>, args: Vec<String>) -> Self {
        Self {
            program,
            args,
            ..Self::default()
        }
    }
}

impl std::fmt::Debug for ShellCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellCommand")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("environment", &"<redacted>")
            .field("directory", &self.directory)
            .finish()
    }
}

// ── PtySize ──────────────────────────────────────────────────────────────────

/// Terminal dimensions accepted by the PTY API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    pub rows: usize,
    pub cols: usize,
}

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
    shell_name: String,
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
    pub fn spawn_shell(size: TerminalSize, command: &ShellCommand) -> anyhow::Result<Self> {
        let (pty, reader, shell_name) =
            RawPty::spawn_shell(PtySize::from_terminal(size)?, command)?;
        let (reader_shutdown, completion) = ReaderShutdown::new();
        let (reader, writer, pty) = pty.into_endpoints(reader);
        Ok(Self {
            shell_name,
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

    /// Display name of the executable that actually started after platform fallback.
    pub fn shell_name(&self) -> &str {
        &self.shell_name
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
            // SAFETY: This bundle still owns its reader; it was never transferred
            // to a terminal or spawned onto a reader thread.
            unsafe { control.shutdown_unstarted() };
        }
    }
}

impl Read for PtyReaderEndpoint {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buffer).map_err(reader_io_error)
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
    /// Interrupts the reader thread's current blocking read without terminating the PTY.
    ///
    /// The reader protocol decides whether the interruption is a resize barrier or shutdown.
    pub fn interrupt_reader(&self, reader: &JoinHandle<()>) -> anyhow::Result<()> {
        self.pty
            .as_ref()
            .expect("pty control is unavailable during shutdown")
            .interrupt_reader(reader)
    }

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
    /// Closes a PTY whose reader thread was never started (e.g. thread spawn failed).
    ///
    /// # Safety
    /// The caller must guarantee that no thread has started reading this PTY.
    /// After a reader starts, use `shutdown(reader)` so the platform reaper
    /// observes its completion before closing ConPTY.
    pub unsafe fn shutdown_unstarted(mut self) {
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
mod tests;
