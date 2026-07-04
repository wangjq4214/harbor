#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

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
}

impl Pty {
    /// Creates a controller with no active PTY session.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            session: None,
            event_proxy,
        }
    }

    /// Starts the PTY session with a shell reader thread.
    ///
    /// The reader posts `AppEvent::PtyOutput` back to the event loop; if the
    /// event loop has shut down, the reader returns `false` and stops.
    pub(crate) fn start(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        let event_proxy = self.event_proxy.clone();

        tracing::info!(rows = size.rows, cols = size.cols, "starting pty");
        let pty = PtySession::start_shell_reader(size, move |output| {
            let bytes = output.len();
            let delivered = event_proxy.send_event(AppEvent::PtyOutput(output)).is_ok();
            if !delivered {
                tracing::warn!(bytes, "dropped pty output because event loop is closed");
            }
            delivered
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
