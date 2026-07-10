use anyhow::bail;

use super::{PtySize, ShellExtraEnvs};

pub(crate) struct Pty;

pub(crate) struct PtyReader;

impl Pty {
    pub(crate) fn spawn_shell(
        _size: PtySize,
        _extra_envs: ShellExtraEnvs,
    ) -> anyhow::Result<(Self, PtyReader)> {
        bail!("pty is not implemented on unix")
    }

    pub(crate) fn resize(&mut self, _size: PtySize) -> anyhow::Result<()> {
        Ok(())
    }

    /// Writes keyboard input — no-op on unix (PTY not yet implemented).
    pub(crate) fn write(&mut self, _data: &[u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}

impl PtyReader {
    pub(crate) fn read(&mut self, _buffer: &mut [u8]) -> anyhow::Result<usize> {
        Ok(0)
    }
}
