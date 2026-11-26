//! Host-admitted resize scheduling and allocation observation for [`TabManager`].

use super::{SessionId, TabManager, TerminalSession};
use harbor_terminal::TerminalSize;

// One ready active attempt plus at least one rotating background opportunity.
pub(super) const RESIZE_BATCH_LIMIT: usize = 2;

/// Effects of the single ordinary resize admission batch in a host turn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResizeBatchOutcome {
    /// Any visible leaf changed; the historical field name does not imply keyboard focus.
    pub active_changed: bool,
    pub ready_remaining: bool,
    pub attempted: usize,
}

impl TabManager {
    pub fn accept_configuration_size(&mut self, size: TerminalSize) {
        for session in &self.sessions {
            self.pane_targets.insert(session.id, size);
        }
        self.resize_target = Some(size);
        self.last_broadcast_size = Some(size);
    }

    /// Explicit uniform request helper for embedders/tests, not window allocation propagation.
    /// Production panes use observe_pane_allocations; hidden grids are not broadcast.
    /// The host must call advance_resizes once per turn and continue ready work.
    pub fn resize_all_if_changed(&mut self, size: TerminalSize) -> bool {
        if size.rows == 0 || size.cols == 0 {
            return false;
        }
        let size = TerminalSize {
            rows: size.rows.max(1),
            cols: size.cols.max(2),
        };
        if self.resize_target == Some(size) && self.last_broadcast_size == Some(size) {
            return false;
        }
        // Never deduplicate against completed geometry: A -> pending B -> A must
        // cancel B in every terminal, even when A was the last completed broadcast.
        self.resize_target = Some(size);
        for tab in &mut self.sessions {
            match tab.terminal.lock() {
                Ok(mut terminal) => {
                    if let Err(error) = terminal.request_resize(size) {
                        tracing::warn!(tab_id = tab.id.0, %error, "failed to request terminal resize");
                    }
                }
                Err(_) => {
                    tracing::warn!(tab_id = tab.id.0, "terminal lock unavailable during resize")
                }
            }
        }
        self.refresh_resize_completion()
    }

    fn refresh_resize_completion(&mut self) -> bool {
        let previous = self.last_broadcast_size;
        self.last_broadcast_size = self.resize_target.filter(|size| {
            !self.sessions.is_empty()
                && self.sessions.iter().all(|tab| {
                    tab.terminal.lock().is_ok_and(|terminal| {
                        terminal.screen().rows() == size.rows
                            && terminal.screen().cols() == size.cols
                    })
                })
        });
        self.last_broadcast_size.is_some() && previous != self.last_broadcast_size
    }

    /// Only the host's once-per-turn admission point calls this. All other paths
    /// merely submit targets or dispatch ordered reader events.
    pub fn advance_resizes(&mut self) -> ResizeBatchOutcome {
        let mut outcome = ResizeBatchOutcome::default();
        let visible = self.visible_bindings();
        let active = self.active_session_id();
        if let Some(tab) = self.sessions.iter().find(|tab| Some(tab.id) == active) {
            Self::advance_tab_resize(tab, true, &mut outcome);
        }
        // A persistent cursor, not Vec order or the current active identity, owns
        // background fairness. Failure attempts also consume a slot.
        for _ in 0..self.sessions.len() {
            if outcome.attempted == RESIZE_BATCH_LIMIT {
                break;
            }
            self.resize_cursor %= self.sessions.len();
            let index = self.resize_cursor;
            self.resize_cursor = (index + 1) % self.sessions.len();
            let tab = &self.sessions[index];
            if Some(tab.id) != active {
                let visible = visible.iter().any(|binding| binding.session == tab.id);
                Self::advance_tab_resize(tab, visible, &mut outcome);
            }
        }
        outcome.ready_remaining = self.sessions.iter().any(|tab| {
            tab.terminal
                .lock()
                .is_ok_and(|terminal| terminal.resize_status().needs_advance())
        });
        self.refresh_resize_completion();
        outcome
    }

    fn advance_tab_resize(tab: &TerminalSession, active: bool, outcome: &mut ResizeBatchOutcome) {
        let Ok(mut terminal) = tab.terminal.lock() else {
            return;
        };
        if !terminal.resize_status().needs_advance() {
            return;
        }
        outcome.attempted += 1;
        match terminal.advance_resize() {
            Ok(changed) => outcome.active_changed |= active && changed.committed(),
            Err(error) => {
                tracing::warn!(tab_id = tab.id.0, %error, "failed to complete terminal resize")
            }
        }
    }

    pub const fn last_broadcast_size(&self) -> Option<TerminalSize> {
        self.last_broadcast_size
    }

    pub(super) fn freeze_hidden_grids(&mut self) {
        let visible = self.visible_bindings();
        for session in &self.sessions {
            if self.pane_targets.contains_key(&session.id)
                && !visible.iter().any(|binding| binding.session == session.id)
                && let Ok(mut terminal) = session.terminal.lock()
            {
                terminal.cancel_resize();
                self.pane_targets.insert(
                    session.id,
                    TerminalSize {
                        rows: terminal.screen().rows(),
                        cols: terminal.screen().cols(),
                    },
                );
            }
        }
    }

    /// Submit only current visible, registration-qualified observations; no transactions here.
    pub fn observe_pane_allocations(
        &mut self,
        observations: &[crate::pane_layout::PaneAllocation],
        scale: f32,
        surface: (u32, u32),
        metrics: &harbor_terminal::TextMetrics,
    ) {
        self.freeze_hidden_grids();
        let visible = self.visible_bindings();
        for allocation in observations {
            if !visible.contains(&allocation.binding) {
                continue;
            }
            let Some(session) = self.sessions.iter().find(|session| {
                session.id == allocation.binding.session
                    && session.bridge.draw_id() == allocation.draw_id
            }) else {
                continue;
            };
            let Some((target, size)) = crate::terminal_view::terminal_target_and_size(
                allocation.rect,
                scale,
                surface,
                metrics,
            ) else {
                continue;
            };
            self.pane_targets.insert(session.id, size);
            if let Ok(mut terminal) = session.terminal.lock() {
                terminal.set_input_geometry(target, metrics);
                if let Err(error) = terminal.request_resize(size) {
                    tracing::warn!(%error, "failed to request pane resize");
                }
            }
        }
    }

    pub fn configuration_size(&self, id: SessionId) -> Option<TerminalSize> {
        self.pane_targets.get(&id).copied()
    }

    /// Accepted configuration geometry is per session, including retained hidden grids.
    pub fn accept_configuration_sizes(&mut self, sizes: &[(SessionId, TerminalSize)]) {
        for (id, size) in sizes {
            self.pane_targets.insert(*id, *size);
        }
        self.last_broadcast_size = None;
        self.resize_target = None;
    }
}
