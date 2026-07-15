use anyhow::bail;

use crate::PtySize;

pub struct Pty;

pub struct PtyReader;

impl Pty {
    pub fn spawn_shell(_size: PtySize) -> anyhow::Result<(Self, PtyReader)> {
        bail!("pty is not implemented on unix")
    }

    pub fn resize(&mut self, _size: PtySize) -> anyhow::Result<()> {
        Ok(())
    }

    /// Writes keyboard input — no-op on unix (PTY not yet implemented).
    pub fn write(&mut self, _data: &[u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}

impl PtyReader {
    pub fn read(&mut self, _buffer: &mut [u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}
