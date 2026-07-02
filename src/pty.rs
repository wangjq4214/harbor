#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix::{Pty, PtyReader};
#[cfg(windows)]
use windows::{Pty, PtyReader};

use std::thread::JoinHandle;

use anyhow::ensure;

use crate::terminal::TerminalSize;

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
    pty: Pty,
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

        let (pty, reader) = Pty::spawn_shell(PtySize::from_terminal(size)?)?;
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
