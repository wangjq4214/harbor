//! Pane focus, split tree mutations, and unpublished split reservation lifecycle.

use super::{
    PaneBinding, TabActionOutcome, TabManager, TerminalPane, TerminalSession, TerminalTab,
    TerminalTabResources,
};
use anyhow::{Result, anyhow};
use harbor_terminal::TerminalOutputEvent;
use harbor_widget::scene::primitive::ExternalDrawId;
use std::sync::Arc;

impl TabManager {
    /// Focus is presentation state; a delayed click cannot activate another workspace.
    pub fn focus_pane(&mut self, binding: PaneBinding) -> TabActionOutcome {
        if self.active != Some(binding.workspace)
            || !self.contains_binding(binding)
            || self.active_binding() == Some(binding)
        {
            return TabActionOutcome::default();
        }
        self.cancel_search();
        self.clear_active_preedit();
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == binding.workspace)
            .unwrap();
        tab.focused = binding.pane;
        if tab.maximized.is_some() {
            tab.maximized = Some(binding.pane);
        }
        self.freeze_hidden_grids();
        let unread_changed = self.take_unread(binding.session);
        TabActionOutcome {
            active_bridge_changed: true,
            active_title_changed: true,
            request_redraw: true,
            unread_changed,
            ..Default::default()
        }
    }

    pub fn execute_pane_command(
        &mut self,
        command: crate::command::PaneCommand,
    ) -> TabActionOutcome {
        use crate::command::PaneCommand;
        let Some(binding) = self.active_binding() else {
            return TabActionOutcome::default();
        };
        match command {
            PaneCommand::Focus(target) => {
                if !self.visible_bindings().contains(&target) {
                    return TabActionOutcome::default();
                }
                self.focus_pane(target)
            }
            PaneCommand::Next | PaneCommand::Previous => {
                let tab = self
                    .tabs
                    .iter()
                    .find(|tab| tab.id == binding.workspace)
                    .unwrap();
                let index = tab
                    .panes
                    .iter()
                    .position(|pane| pane.id == binding.pane)
                    .unwrap();
                let count = tab.panes.len();
                let index = if command == PaneCommand::Next {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                };
                let pane = &tab.panes[index];
                self.focus_pane(PaneBinding {
                    workspace: binding.workspace,
                    pane: pane.id,
                    session: pane.session,
                })
            }
            PaneCommand::Maximize => self.maximize_pane(binding),
            PaneCommand::Close => self.close_pane(binding),
            PaneCommand::SplitHorizontal | PaneCommand::SplitVertical => {
                TabActionOutcome::default()
            }
        }
    }

    /// Current visible leaves, independent of keyboard focus.
    pub fn visible_bindings(&self) -> Vec<PaneBinding> {
        let Some(tab) = self.tabs.iter().find(|tab| Some(tab.id) == self.active) else {
            return Vec::new();
        };
        tab.panes
            .iter()
            .filter(|pane| tab.maximized.is_none_or(|id| id == pane.id))
            .map(|pane| PaneBinding {
                workspace: tab.id,
                pane: pane.id,
                session: pane.session,
            })
            .collect()
    }

    pub fn active_layout(&self) -> Option<crate::pane_layout::PaneLayout> {
        use crate::pane_layout::{PaneLayout, PaneTree};
        let tab = self.tabs.iter().find(|tab| Some(tab.id) == self.active)?;
        fn project(tabs: &TabManager, tab: &TerminalTab, tree: &PaneTree) -> Option<PaneLayout> {
            Some(match tree {
                PaneTree::Leaf(id) => {
                    let pane = tab.panes.iter().find(|pane| pane.id == *id)?;
                    let session = tabs
                        .sessions
                        .iter()
                        .find(|session| session.id == pane.session)?;
                    PaneLayout::Leaf {
                        binding: PaneBinding {
                            workspace: tab.id,
                            pane: *id,
                            session: pane.session,
                        },
                        bridge: session.bridge.clone(),
                    }
                }
                PaneTree::Split {
                    axis,
                    ratio,
                    first,
                    second,
                } => PaneLayout::Split {
                    axis: *axis,
                    ratio: ratio.clone(),
                    first: Box::new(project(tabs, tab, first)?),
                    second: Box::new(project(tabs, tab, second)?),
                },
            })
        }
        project(
            self,
            tab,
            &tab.maximized
                .map_or_else(|| tab.tree.clone(), PaneTree::Leaf),
        )
    }

    /// Atomic publication of independent resources at the reserved original leaf.
    /// On every rejection the unpublished owner retires its resources. No focus lookup.
    pub fn attach_split(
        &mut self,
        source: PaneBinding,
        target: PaneBinding,
        draw: ExternalDrawId,
        mut resources: TerminalTabResources,
        axis: harbor_widget::SplitAxis,
    ) -> Result<TabActionOutcome> {
        let live_engine = self
            .sessions
            .iter()
            .any(|session| Arc::ptr_eq(&session.terminal, &resources.terminal));
        let live_registration = self
            .sessions
            .iter()
            .any(|session| session.bridge.shares_registration(&resources.bridge));
        if live_engine || live_registration {
            // A malformed owner must not revoke a published survivor on rejection.
            if !live_engine {
                resources
                    .terminal
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .retire_session();
            }
            if !live_registration {
                resources.bridge.retire();
            }
            resources.published = true;
            return Err(anyhow!("split resources are already published"));
        }
        let reservation = self.split_reservations.remove(&target.pane);
        anyhow::ensure!(
            reservation == Some((source, target, draw)) && self.contains_binding(source),
            "stale split attachment"
        );
        anyhow::ensure!(
            resources.bridge.draw_id() == draw && resources.bridge.is_bound_to(&resources.terminal),
            "split registration mismatch"
        );
        anyhow::ensure!(
            !self
                .sessions
                .iter()
                .any(|session| session.id == target.session || session.bridge.draw_id() == draw),
            "split resources are not independent"
        );
        {
            let mut terminal = resources
                .terminal
                .lock()
                .map_err(|_| anyhow!("split terminal lock unavailable"))?;
            anyhow::ensure!(
                !terminal.is_session_closed() && !terminal.shell_process_exited(),
                "split session is closed"
            );
            terminal.use_host_resize_scheduling();
            if let Some(delivery) = self.clipboard_delivery {
                terminal.set_clipboard_delivery(delivery);
            }
        }
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == source.workspace)
            .expect("validated workspace");
        let mut tree = tab.tree.clone();
        anyhow::ensure!(
            tree.split(source.pane, target.pane, axis),
            "split source has no layout leaf"
        );
        tab.tree = tree;
        tab.panes.push(TerminalPane {
            id: target.pane,
            session: target.session,
        });
        // A new leaf is visible on restoration; maximizing never destroys the tree.
        self.sessions.push(TerminalSession {
            id: target.session,
            title: resources.shell_fallback.clone(),
            shell_fallback: resources.shell_fallback.clone(),
            working_directory: None,
            shell_integration: None,
            unread: false,
            terminal: Arc::clone(&resources.terminal),
            bridge: resources.bridge.clone(),
        });
        resources.published = true;
        Ok(TabActionOutcome {
            active_bridge_changed: self.active == Some(source.workspace),
            request_redraw: true,
            ..Default::default()
        })
    }

    /// Seed only bounded startup display metadata; never replay startup clipboard effects.
    pub fn seed_split_metadata(&mut self, binding: PaneBinding, events: Vec<TerminalOutputEvent>) {
        if !self.contains_binding(binding) {
            return;
        }
        let session = self
            .sessions
            .iter_mut()
            .find(|session| session.id == binding.session)
            .unwrap();
        for event in events {
            session.apply_metadata_event(event);
        }
    }

    pub fn maximize_pane(&mut self, binding: PaneBinding) -> TabActionOutcome {
        if !self.contains_binding(binding) {
            return TabActionOutcome::default();
        }
        self.focus_pane(binding);
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == binding.workspace)
            .unwrap();
        tab.maximized = (tab.maximized != Some(binding.pane)).then_some(binding.pane);
        self.freeze_hidden_grids();
        TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            ..Default::default()
        }
    }

    pub fn close_pane(&mut self, binding: PaneBinding) -> TabActionOutcome {
        if !self.contains_binding(binding) {
            return TabActionOutcome::default();
        }
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == binding.workspace)
            .unwrap();
        if tab.panes.len() == 1 {
            return self.close(binding.workspace);
        }
        let focus_changed = self.active == Some(binding.workspace) && tab.focused == binding.pane;
        tab.panes.retain(|pane| pane.id != binding.pane);
        tab.tree = tab
            .tree
            .clone()
            .remove(binding.pane)
            .expect("surviving leaf");
        if tab.focused == binding.pane {
            tab.focused = tab.panes[0].id;
        }
        if tab.maximized == Some(binding.pane) {
            tab.maximized = None;
        }
        self.split_reservations
            .retain(|_, (source, _, _)| *source != binding);
        if focus_changed {
            self.cancel_search();
        }
        self.retire_session(binding.session);
        TabActionOutcome {
            active_bridge_changed: focus_changed,
            request_redraw: true,
            active_title_changed: focus_changed,
            ..Default::default()
        }
    }

    /// Host cancellation discards an unpublished reservation without reusing its IDs.
    pub fn cancel_split_reservation(&mut self, target: PaneBinding) {
        if self
            .split_reservations
            .get(&target.pane)
            .is_some_and(|(_, reserved, _)| *reserved == target)
        {
            self.split_reservations.remove(&target.pane);
        }
    }

    /// Reserve an unpublished split leaf in the original workspace. IDs are burned
    /// even on failure/cancellation.
    pub fn reserve_split_ids(
        &mut self,
        source: PaneBinding,
    ) -> Result<(PaneBinding, ExternalDrawId)> {
        anyhow::ensure!(self.contains_binding(source), "split source is unavailable");
        let (target, draw) = self.reserve_pane_ids(Some(source.workspace))?;
        self.split_reservations
            .insert(target.pane, (source, target, draw));
        Ok((target, draw))
    }
}
