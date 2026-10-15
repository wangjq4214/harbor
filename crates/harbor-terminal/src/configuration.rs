//! Application-coordinated detached geometry and palette preparation.
use crate::io::ResizeBarrier;
use crate::pointer::PreparedPointerResize;
use crate::screen::PreparedScreenResize;
use crate::{Terminal, TerminalAppearance, TerminalSize, TerminalUpdate, UpdateDamage};
use harbor_config::Palette;
use std::time::Instant;

/// Holds exclusive engine access and the reader barrier until commit or rejection.
/// The application prepares every resource before calling `resize_pty` on any member.
pub struct ConfigurationResize<'a> {
    terminal: &'a mut Terminal,
    old: TerminalSize,
    new: TerminalSize,
    pty_required: bool,
    prepared: Option<(PreparedScreenResize, PreparedPointerResize)>,
    _barrier: Option<ResizeBarrier>,
    pty_changed: bool,
}

impl Terminal {
    pub fn prepare_configuration_resize(
        &mut self,
        size: TerminalSize,
    ) -> anyhow::Result<ConfigurationResize<'_>> {
        let new = TerminalSize {
            rows: size.rows.max(1),
            cols: size.cols.max(2),
        };
        let old = TerminalSize {
            rows: self.screen.rows(),
            cols: self.screen.cols(),
        };
        let pty_required = old != new && !self.io.is_session_closed();
        let barrier = if pty_required {
            self.ingest_and_blink(|io, screen, pointer| io.acquire_resize_barrier(screen, pointer))?
        } else {
            None
        };
        let prepared = if old != new {
            let screen = self.screen.prepare_resize_with_viewport(
                new.rows,
                new.cols,
                self.io.reflow_viewport(),
            )?;
            let pointer = self.pointer.prepare_resize(&screen);
            Some((screen, pointer))
        } else {
            None
        };
        Ok(ConfigurationResize {
            terminal: self,
            old,
            new,
            prepared,
            pty_required,
            _barrier: barrier,
            pty_changed: false,
        })
    }
}

impl ConfigurationResize<'_> {
    /// Full candidate projection, without consuming live damage or advancing its baseline.
    pub fn update(&self, baseline: Palette) -> TerminalUpdate {
        let mut update = self.terminal.read_update(Instant::now());
        if let Some((screen, pointer)) = &self.prepared {
            update.snapshot = screen.snapshot(&self.terminal.screen);
            update.selection = pointer.bounds();
        }
        update.appearance =
            TerminalAppearance::from_palette(self.terminal.screen.configuration_palette(baseline));
        update.damage = UpdateDamage::FullUpload;
        update
    }

    pub fn resize_pty(&mut self) -> anyhow::Result<()> {
        self.resize_pty_with(|terminal, size| terminal.io.resize_pty(size))
    }

    fn resize_pty_with(
        &mut self,
        mut effect: impl FnMut(&mut Terminal, TerminalSize) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if self.pty_required && !self.pty_changed {
            effect(self.terminal, self.new)?;
            self.pty_changed = true;
        }
        Ok(())
    }

    /// Restore already changed PTY geometry before rejecting the group.
    /// On double failure, reconcile the model to the last successful PTY size,
    /// retaining the old configuration/font policy; the caller must report recovery failure.
    pub fn rollback_pty(&mut self) -> anyhow::Result<()> {
        self.rollback_pty_with(|terminal, size| terminal.io.resize_pty(size))
    }

    fn rollback_pty_with(
        &mut self,
        mut effect: impl FnMut(&mut Terminal, TerminalSize) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if !self.pty_changed {
            return Ok(());
        }
        self.pty_changed = false;
        if let Err(error) = effect(self.terminal, self.old) {
            self.install_geometry();
            anyhow::bail!(
                "compensating PTY resize to {}x{} failed: {error:#}; model reconciled to last successful PTY geometry {}x{}; session needs recovery",
                self.old.cols,
                self.old.rows,
                self.new.cols,
                self.new.rows
            );
        }
        Ok(())
    }

    fn install_geometry(&mut self) {
        if let Some((screen, pointer)) = self.prepared.take() {
            self.terminal.screen.commit_resize(screen);
            self.terminal.pointer.commit_resize(pointer);
            self.terminal.io.reset_scroll_snap();
        }
        self.terminal.invalidate_update();
    }

    /// Called only after all group PTY effects succeeded. Publication is infallible.
    pub fn commit(
        mut self,
        baseline: Palette,
        viewport: Option<crate::RenderViewport>,
        scale: f32,
    ) {
        assert!(
            !self.pty_required || self.pty_changed,
            "configuration resize must complete its PTY effect before publication"
        );
        self.install_geometry();
        self.terminal.screen.accept_configuration_palette(baseline);
        if let Some(viewport) = viewport {
            self.terminal.pointer.set_viewport(viewport);
            self.terminal.pointer.set_input_scale(scale);
        }
        self.pty_changed = false;
    }
}

impl Drop for ConfigurationResize<'_> {
    fn drop(&mut self) {
        // Safeguard for abandoned transactions and unwinding. Normal group errors
        // explicitly call rollback_pty so their diagnostics reach application feedback.
        if let Err(error) = self.rollback_pty() {
            tracing::error!(%error, "abandoned configuration resize recovery failed");
        }
        // ResizeBarrier's drop resumes the reader on every completion path.
    }
}

/// Failures preserve their member indexes so the application can label stable session IDs.
#[derive(Debug)]
pub struct ResizeGroupError {
    pub failures: Vec<(usize, String)>,
}

pub fn resize_configuration_group(
    transactions: &mut [ConfigurationResize<'_>],
) -> Result<(), ResizeGroupError> {
    resize_configuration_group_with_check(transactions, |_, _| Ok(()))
}

/// Runs a bounded check before each required native effect, including compensation.
/// A rejected check prevents that effect; an accepted check always performs the real
/// PTY resize. No terminal or platform handles escape, and group rollback is shared.
pub fn resize_configuration_group_with_check(
    transactions: &mut [ConfigurationResize<'_>],
    mut check: impl FnMut(usize, TerminalSize) -> anyhow::Result<()>,
) -> Result<(), ResizeGroupError> {
    resize_group_with(transactions, |index, terminal, size| {
        check(index, size)?;
        terminal.io.resize_pty(size)
    })
}
pub(crate) fn resize_group_with(
    transactions: &mut [ConfigurationResize<'_>],
    mut effect: impl FnMut(usize, &mut Terminal, TerminalSize) -> anyhow::Result<()>,
) -> Result<(), ResizeGroupError> {
    for index in 0..transactions.len() {
        if let Err(error) =
            transactions[index].resize_pty_with(|terminal, size| effect(index, terminal, size))
        {
            let mut failures = vec![(index, format!("PTY resize failed: {error:#}"))];
            for prior in (0..index).rev() {
                if let Err(error) = transactions[prior]
                    .rollback_pty_with(|terminal, size| effect(prior, terminal, size))
                {
                    failures.push((prior, format!("recovery failed: {error:#}")));
                }
            }
            return Err(ResizeGroupError { failures });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_config::Rgba;

    #[test]
    fn later_session_failure_compensates_without_publishing_and_retry_succeeds() {
        let mut a = Terminal::new_headless(4, 12);
        let mut b = Terminal::new_headless(4, 12);
        let baseline = a.read_update(Instant::now()).appearance.palette();
        let mut next = baseline;
        next.background = Rgba::from_rgb8(1, 2, 3);
        let size = TerminalSize { rows: 3, cols: 8 };
        let mut transactions = vec![
            a.prepare_configuration_resize(size).unwrap(),
            b.prepare_configuration_resize(size).unwrap(),
        ];
        let mut calls = Vec::new();
        let failure = resize_group_with(&mut transactions, |index, _, size| {
            calls.push((index, size));
            if index == 1 {
                anyhow::bail!("injected later-session failure");
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(failure.failures.len(), 1);
        assert_eq!(
            calls,
            vec![
                (0, size),
                (1, size),
                (0, TerminalSize { rows: 4, cols: 12 })
            ]
        );
        drop(transactions);
        for terminal in [&a, &b] {
            assert_eq!(terminal.snapshot().cols, 12);
            assert_eq!(
                terminal.read_update(Instant::now()).appearance.palette(),
                baseline
            );
        }
        let mut transactions = vec![
            a.prepare_configuration_resize(size).unwrap(),
            b.prepare_configuration_resize(size).unwrap(),
        ];
        resize_configuration_group(&mut transactions).unwrap();
        for tx in transactions {
            tx.commit(next, None, 1.0);
        }
        for terminal in [&a, &b] {
            assert_eq!(terminal.snapshot().cols, 8);
            assert_eq!(
                terminal.read_update(Instant::now()).appearance.palette(),
                next
            );
            assert!(terminal.projection_requires_full_update());
        }
    }

    #[test]
    fn compensation_failure_is_explicit_reconciles_model_and_keeps_old_palette() {
        let mut a = Terminal::new_headless(4, 12);
        let mut b = Terminal::new_headless(4, 12);
        let baseline = a.read_update(Instant::now()).appearance.palette();
        let size = TerminalSize { rows: 3, cols: 8 };
        let mut transactions = vec![
            a.prepare_configuration_resize(size).unwrap(),
            b.prepare_configuration_resize(size).unwrap(),
        ];
        let mut count = 0;
        let failure = resize_group_with(&mut transactions, |_, _, _| {
            count += 1;
            if count > 1 {
                anyhow::bail!("injected native failure");
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(failure.failures.len(), 2);
        assert_eq!(failure.failures[1].0, 0);
        assert!(failure.failures[1].1.contains("session needs recovery"));
        drop(transactions);
        assert_eq!(a.snapshot().cols, 8);
        assert_eq!(b.snapshot().cols, 12);
        for terminal in [&a, &b] {
            assert_eq!(
                terminal.read_update(Instant::now()).appearance.palette(),
                baseline
            );
        }
    }

    #[test]
    fn completed_session_can_accept_model_geometry_without_native_resize_or_barrier() {
        let mut terminal =
            Terminal::new_headless_with_io(4, 20, std::io::empty(), std::io::sink(), || true);
        let deadline = Instant::now() + std::time::Duration::from_secs(1);
        while !terminal.is_session_closed() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut transactions = vec![
            terminal
                .prepare_configuration_resize(TerminalSize { rows: 3, cols: 12 })
                .unwrap(),
        ];
        resize_group_with(&mut transactions, |_, _, _| {
            panic!("completed session has no native resize effect")
        })
        .unwrap();
        transactions
            .pop()
            .unwrap()
            .commit(Palette::default(), None, 1.0);
        drop(transactions);
        assert_eq!(terminal.snapshot().cols, 12);
    }

    #[test]
    fn rejected_preparation_preserves_damage_content_and_palette() {
        let mut terminal = Terminal::new_headless(4, 12);
        terminal.io.feed_pty_output(
            &mut terminal.screen,
            &mut terminal.pointer,
            b"newest marker",
        );
        let before = terminal.read_update(Instant::now());
        let transaction = terminal
            .prepare_configuration_resize(TerminalSize { rows: 3, cols: 8 })
            .unwrap();
        let projected = transaction.update(Palette::default());
        assert_eq!(projected.snapshot.cols, 8);
        drop(transaction);
        let after = terminal.read_update(Instant::now());
        assert_eq!(after.snapshot.cells, before.snapshot.cells);
        assert_eq!(after.snapshot.dirty_ranges, before.snapshot.dirty_ranges);
        assert_eq!(after.appearance.palette(), before.appearance.palette());
    }
}
