use std::thread::JoinHandle;

use anyhow::bail;

use crate::{PtySize, ReaderShutdown};

pub struct Pty;

pub struct PtyReader;

pub struct PtyWriter;

impl Pty {
    pub fn spawn_shell(_size: PtySize) -> anyhow::Result<(Self, PtyReader)> {
        bail!("pty is not implemented on unix")
    }

    pub fn resize(&mut self, _size: PtySize) -> anyhow::Result<()> {
        Ok(())
    }

    pub(crate) fn into_endpoints(self, reader: PtyReader) -> (PtyReader, PtyWriter, Self) {
        (reader, PtyWriter, self)
    }

    /// Writes keyboard input — no-op on unix (PTY not yet implemented).
    pub fn write(&mut self, _data: &[u8]) -> anyhow::Result<usize> {
        Ok(0)
    }

    pub(crate) fn shutdown(_pty: Self, reader: JoinHandle<()>, reader_shutdown: ReaderShutdown) {
        reader_shutdown.request_stop();
        let _ = reader.join();
    }
}

impl PtyWriter {
    pub(crate) fn write(&mut self, _data: &[u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}

impl PtyReader {
    pub fn read(&mut self, _buffer: &mut [u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}
