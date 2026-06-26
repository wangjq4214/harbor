use anyhow::bail;

use super::PtySize;

pub(crate) struct Pty;

pub(crate) struct PtyReader;

impl Pty {
    pub(crate) fn spawn_shell(_size: PtySize) -> anyhow::Result<(Self, PtyReader)> {
        bail!("pty is not implemented on unix")
    }

    pub(crate) fn resize(&mut self, _size: PtySize) -> anyhow::Result<()> {
        Ok(())
    }
}

impl PtyReader {
    pub(crate) fn read(&mut self, _buffer: &mut [u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}
