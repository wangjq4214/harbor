//! Ordinary geometry intent and progress, separate from host transaction admission.
use std::time::Instant;

use crate::io::TerminalIo;
use crate::pointer::PointerInteraction;
use crate::screen::Screen;
use crate::{Terminal, TerminalSize};

/// Non-consuming state. A failed acquisition needs an advance for error delivery
/// and cleanup; it is not a successfully acquired reader barrier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeStatus {
    Idle,
    ConfigurationOwned,
    WaitingForReader { target: TerminalSize },
    ReadyToCommit { target: TerminalSize },
    Failed { target: TerminalSize },
}

impl ResizeStatus {
    /// Work eligible for a bounded host admission slot, including failure cleanup.
    pub fn needs_advance(self) -> bool {
        matches!(self, Self::ReadyToCommit { .. } | Self::Failed { .. })
    }
}

/// A request or progress result. Pending never means geometry was published.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use]
pub enum ResizeOutcome {
    Unchanged,
    ConfigurationOwned,
    Pending(ResizeStatus),
    Committed(TerminalSize),
}

impl ResizeOutcome {
    pub fn committed(self) -> bool {
        matches!(self, Self::Committed(_))
    }
}

impl Terminal {
    /// Resizes the terminal grid without GPU resources. Returns true if size changed.
    ///
    /// PTY failures are logged and leave the screen unchanged so a later caller can retry.
    pub fn resize_if_changed(&mut self, new_size: TerminalSize) -> bool {
        match self.try_resize_if_changed(new_size) {
            Ok(changed) => changed,
            Err(error) => {
                tracing::error!(error = %format_args!("{error:#}"), "failed to resize terminal pty");
                false
            }
        }
    }

    /// Compatibility convenience: true only on commit. Prefer request_resize plus
    /// advance_resize to distinguish waiting, configuration ownership, and errors.
    pub fn try_resize_if_changed(&mut self, new_size: TerminalSize) -> anyhow::Result<bool> {
        match self.request_resize(new_size)? {
            ResizeOutcome::Pending(_) if !self.host_driven_resize => self.finish_pending_resize(),
            _ => Ok(false),
        }
    }

    /// Publish latest intent without running a geometry transaction. Hosts admit
    /// expensive work through advance_resize; standalone drains also advance it.
    /// Reader acquisition is nonblocking and failures retain their error channel.
    pub fn request_resize(&mut self, new_size: TerminalSize) -> anyhow::Result<ResizeOutcome> {
        let normalized = new_size.normalized();
        if self.io.configuration_barrier_active() {
            return Ok(ResizeOutcome::ConfigurationOwned);
        }
        if normalized
            == (TerminalSize {
                rows: self.screen.rows(),
                cols: self.screen.cols(),
            })
        {
            self.pending_resize = None;
            self.io.cancel_resize_barrier();
            return Ok(ResizeOutcome::Unchanged);
        }
        if self.pending_resize.is_none()
            && let Err(error) = self.io.request_resize_barrier(Instant::now())
        {
            self.io.cancel_resize_barrier();
            return Err(error);
        }
        self.pending_resize = Some(normalized);
        Ok(ResizeOutcome::Pending(self.resize_status()))
    }

    /// Transfer ordinary resize admission to the host. Requests and all automatic
    /// drain/render paths then leave expensive transactions to advance_resize().
    /// Standalone engines retain automatic completion by default.
    pub fn use_host_resize_scheduling(&mut self) {
        self.host_driven_resize = true;
    }

    /// Abandon ordinary queued work without taking over a configuration-owned barrier.
    pub fn cancel_resize(&mut self) {
        if self.pending_resize.take().is_some() && !self.io.configuration_barrier_active() {
            self.io.cancel_resize_barrier();
        }
    }

    /// Non-consuming state; failed acquisition is explicitly distinct from readiness.
    pub fn resize_status(&self) -> ResizeStatus {
        if self.io.configuration_barrier_active() {
            return ResizeStatus::ConfigurationOwned;
        }
        let Some(target) = self.pending_resize else {
            return ResizeStatus::Idle;
        };
        match self.io.resize_barrier_ready() {
            Ok(false) => ResizeStatus::WaitingForReader { target },
            Ok(true) => ResizeStatus::ReadyToCommit { target },
            Err(_) => ResizeStatus::Failed { target },
        }
    }

    /// Admit at most one transaction. A multi-terminal host must bound calls across
    /// all sessions. Failed acquisition/transactions release the reader and return Err.
    pub fn advance_resize(&mut self) -> anyhow::Result<ResizeOutcome> {
        if self.io.configuration_barrier_active() {
            return Ok(ResizeOutcome::ConfigurationOwned);
        }
        let Some(size) = self.pending_resize else {
            return Ok(ResizeOutcome::Unchanged);
        };
        if self.finish_pending_resize()? {
            Ok(ResizeOutcome::Committed(size))
        } else if self.pending_resize.is_none() {
            Ok(ResizeOutcome::Unchanged)
        } else {
            Ok(ResizeOutcome::Pending(self.resize_status()))
        }
    }

    pub(crate) fn finish_pending_resize(&mut self) -> anyhow::Result<bool> {
        if self.io.configuration_barrier_active() {
            return Ok(false);
        }
        let Some(size) = self.pending_resize else {
            return Ok(false);
        };
        match self.io.resize_barrier_ready() {
            Ok(false) => return Ok(false),
            Err(error) => {
                self.pending_resize = None;
                self.io.cancel_resize_barrier();
                return Err(error);
            }
            Ok(true) => {}
        }
        let barrier = self.io.take_resize_barrier()?;
        self.pending_resize = None;
        let result = Self::try_resize_transaction(
            &mut self.screen,
            &mut self.pointer,
            &mut self.io,
            size,
            |io, size| io.resize_pty(size),
        );
        drop(barrier);
        if matches!(result, Ok(true)) {
            // Geometry requires presentation, but is not child output/unread text.
            self.pending_ordinary_present = true;
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn log_resize_transaction(
        current: TerminalSize,
        new_size: TerminalSize,
        active_history_rows: usize,
        saved_primary_history_rows: usize,
        prepare_us: Option<u64>,
        pty_us: Option<u64>,
        commit_us: Option<u64>,
        total_us: u64,
        failed_stage: Option<&str>,
        error: Option<&dyn std::fmt::Display>,
        outcome: &str,
    ) {
        tracing::debug!(
            old_rows = current.rows,
            old_cols = current.cols,
            new_rows = new_size.rows,
            new_cols = new_size.cols,
            active_history_rows,
            saved_primary_history_rows,
            prepare_us,
            pty_us,
            commit_us,
            total_us,
            failed_stage,
            error = error.map(tracing::field::display),
            outcome,
            "terminal resize transaction"
        );
    }

    pub(crate) fn try_resize_transaction(
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        io: &mut TerminalIo,
        new_size: TerminalSize,
        resize_pty: impl FnOnce(&mut TerminalIo, TerminalSize) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        let started = Instant::now();
        let new_size = new_size.normalized();
        let current = TerminalSize {
            rows: screen.rows(),
            cols: screen.cols(),
        };
        let active_history_rows = screen.scroll_count();
        let saved_primary_history_rows = screen.saved_primary_scroll_count();
        if new_size == current {
            Self::log_resize_transaction(
                current,
                new_size,
                active_history_rows,
                saved_primary_history_rows,
                None,
                None,
                None,
                started.elapsed().as_micros() as u64,
                None,
                None,
                "unchanged",
            );
            return Ok(false);
        }

        let prepare_started = Instant::now();
        let prepared_screen = match screen.prepare_resize_with_viewport(
            new_size.rows,
            new_size.cols,
            io.reflow_viewport(),
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                Self::log_resize_transaction(
                    current,
                    new_size,
                    active_history_rows,
                    saved_primary_history_rows,
                    Some(prepare_started.elapsed().as_micros() as u64),
                    None,
                    None,
                    started.elapsed().as_micros() as u64,
                    Some("prepare"),
                    Some(&error),
                    "error",
                );
                return Err(error.into());
            }
        };
        let prepared_pointer = pointer.prepare_resize(&prepared_screen);
        let prepare_us = prepare_started.elapsed().as_micros() as u64;

        let pty_started = Instant::now();
        if let Err(error) = resize_pty(io, new_size) {
            Self::log_resize_transaction(
                current,
                new_size,
                active_history_rows,
                saved_primary_history_rows,
                Some(prepare_us),
                Some(pty_started.elapsed().as_micros() as u64),
                None,
                started.elapsed().as_micros() as u64,
                Some("pty"),
                Some(&format_args!("{error:#}")),
                "error",
            );
            return Err(error);
        }
        let pty_us = pty_started.elapsed().as_micros() as u64;

        let commit_started = Instant::now();
        screen.commit_resize(prepared_screen);
        pointer.commit_resize(prepared_pointer);
        io.reset_scroll_snap();
        let commit_us = commit_started.elapsed().as_micros() as u64;
        Self::log_resize_transaction(
            current,
            new_size,
            active_history_rows,
            saved_primary_history_rows,
            Some(prepare_us),
            Some(pty_us),
            Some(commit_us),
            started.elapsed().as_micros() as u64,
            None,
            None,
            "changed",
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terminal;
    #[test]
    fn normalized_requests_and_configuration_completed_intent_are_not_pending() {
        let mut terminal = Terminal::new_headless(2, 4);
        let size = TerminalSize { rows: 1, cols: 2 };
        assert_eq!(
            terminal
                .request_resize(TerminalSize { rows: 0, cols: 0 })
                .unwrap(),
            ResizeOutcome::Pending(ResizeStatus::ReadyToCommit { target: size })
        );
        assert_eq!(terminal.screen().cols(), 4, "request must not commit");
        let mut transaction = terminal.prepare_configuration_resize(size).unwrap();
        transaction.resize_pty().unwrap();
        transaction.commit(harbor_config::Palette::default(), None, 1.0);
        assert_eq!(terminal.advance_resize().unwrap(), ResizeOutcome::Unchanged);
        assert_eq!(terminal.resize_status(), ResizeStatus::Idle);
        assert_eq!(
            terminal.request_resize(size).unwrap(),
            ResizeOutcome::Unchanged
        );
    }
}
