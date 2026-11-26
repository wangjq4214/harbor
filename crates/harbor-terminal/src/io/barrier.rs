//! Coordinated reader barrier leases and status tracking for PTY resize.

use std::sync::{Arc, Mutex, mpsc::Sender};
use std::time::Instant;

use super::pty::ReaderCommand;

#[derive(Debug)]
pub(super) enum BarrierStatus {
    Waiting,
    Ready,
    Failed(String),
    Released,
}

/// An owned, non-borrowing cancellation lease for application-coordinated acquisition.
/// Dropping it releases the reader even without reacquiring the Terminal lock.
pub struct ConfigurationBarrier {
    pub(super) release: Option<ResizeBarrier>,
}

impl ConfigurationBarrier {
    /// ACK readiness is not configuration acceptance. The host must continue
    /// draining/ticking the terminal and coordinate every member before commit.
    pub fn is_ready(&self) -> anyhow::Result<bool> {
        self.release
            .as_ref()
            .map_or(Ok(true), ResizeBarrier::is_ready)
    }
}

pub(crate) struct ResizeBarrier {
    pub(super) commands: Sender<ReaderCommand>,
    pub(super) epoch: u64,
    pub(super) status: Arc<Mutex<BarrierStatus>>,
}

impl ResizeBarrier {
    pub(super) fn is_ready(&self) -> anyhow::Result<bool> {
        match &*self.status.lock().unwrap_or_else(|e| e.into_inner()) {
            BarrierStatus::Waiting => Ok(false),
            BarrierStatus::Ready => Ok(true),
            BarrierStatus::Failed(error) => Err(anyhow::anyhow!("{error}")),
            BarrierStatus::Released => anyhow::bail!("PTY resize barrier was cancelled"),
        }
    }

    pub(super) fn is_waiting(&self) -> bool {
        matches!(
            *self.status.lock().unwrap_or_else(|e| e.into_inner()),
            BarrierStatus::Waiting
        )
    }

    pub(super) fn is_inactive(&self) -> bool {
        matches!(
            *self.status.lock().unwrap_or_else(|e| e.into_inner()),
            BarrierStatus::Released | BarrierStatus::Failed(_)
        )
    }

    pub(super) fn mark_ready(&self) {
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*status, BarrierStatus::Waiting) {
            *status = BarrierStatus::Ready;
        }
    }

    pub(super) fn set_failed(&self, error: String) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = BarrierStatus::Failed(error);
        let _ = self.commands.send(ReaderCommand::Resume(self.epoch));
    }

    pub(super) fn lease(&self) -> Self {
        Self {
            commands: self.commands.clone(),
            epoch: self.epoch,
            status: Arc::clone(&self.status),
        }
    }
}

impl Drop for ResizeBarrier {
    fn drop(&mut self) {
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(*status, BarrierStatus::Failed(_)) {
            *status = BarrierStatus::Released;
        }
        let _ = self.commands.send(ReaderCommand::Resume(self.epoch));
    }
}

pub(super) struct PendingBarrier {
    pub(super) barrier: ResizeBarrier,
    pub(super) deadline: Instant,
    pub(super) retry: Instant,
    pub(super) configuration: bool,
}
