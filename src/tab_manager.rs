//! Host-owned terminal tab/session state and routing.

use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, anyhow};
use harbor_terminal::{Terminal, TerminalSize};
use harbor_widget::scene::primitive::ExternalDrawId;

use crate::terminal_view::TerminalWidgetBridge;

/// Stable terminal-session identity. Values are monotonic and never reused.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TabId(pub(crate) u64);

impl TabId {
    #[cfg(test)]
    const fn get(self) -> u64 {
        self.0
    }
}

/// Resources produced by the Host's per-tab factory.
pub(crate) struct TerminalTabResources {
    terminal: Arc<Mutex<Terminal>>,
    bridge: TerminalWidgetBridge,
}

impl TerminalTabResources {
    pub(crate) fn new(terminal: Arc<Mutex<Terminal>>, bridge: TerminalWidgetBridge) -> Self {
        Self { terminal, bridge }
    }
}

struct TerminalTab {
    id: TabId,
    title: String,
    unread: bool,
    draw_id: ExternalDrawId,
    terminal: Arc<Mutex<Terminal>>,
    bridge: TerminalWidgetBridge,
}

/// Read-only product state for a tab rail or Host test.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TabSnapshot {
    pub(crate) id: TabId,
    pub(crate) title: String,
    pub(crate) unread: bool,
    pub(crate) draw_id: ExternalDrawId,
    pub(crate) active: bool,
}

/// Effects the Host must apply after a model transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TabActionOutcome {
    pub(crate) active_bridge_changed: bool,
    pub(crate) request_redraw: bool,
    pub(crate) unread_changed: bool,
    pub(crate) close_window: bool,
}

impl TabActionOutcome {
    const fn unchanged() -> Self {
        Self {
            active_bridge_changed: false,
            request_redraw: false,
            unread_changed: false,
            close_window: false,
        }
    }
}

/// Classification of one tab-qualified PTY wake.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TabOutputOutcome {
    pub(crate) request_active_invalidation: bool,
    pub(crate) unread_changed: bool,
}

/// Ordered Host-owned terminal sessions.
pub(crate) struct TabManager {
    tabs: Vec<TerminalTab>,
    active: Option<TabId>,
    next_tab_id: u64,
    next_draw_id: ExternalDrawId,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // T0007 binds the transition API to product commands.
impl TabManager {
    pub(crate) const fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_tab_id: 1,
            next_draw_id: 1,
        }
    }

    /// Reserves identities before invoking the factory, so even a failed factory cannot allow a
    /// queued stale wake to target a later session.
    pub(crate) fn create_tab(
        &mut self,
        factory: impl FnOnce(TabId, ExternalDrawId) -> Result<TerminalTabResources>,
    ) -> Result<TabActionOutcome> {
        let (id, draw_id) = self.reserve_ids()?;
        let resources = factory(id, draw_id).context("create terminal tab resources")?;
        if resources.bridge.draw_id() != draw_id {
            return Err(anyhow!(
                "terminal bridge draw id mismatch: allocated {draw_id}, received {}",
                resources.bridge.draw_id()
            ));
        }

        let title = format!("Terminal {}", id.0);
        self.tabs.push(TerminalTab {
            id,
            title,
            unread: false,
            draw_id,
            terminal: resources.terminal,
            bridge: resources.bridge,
        });
        self.active = Some(id);
        Ok(TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed: false,
            close_window: false,
        })
    }

    pub(crate) fn active_id(&self) -> Option<TabId> {
        self.active
    }

    /// Selects the right neighbor, then the left, for rail focus after a close.
    pub(crate) fn neighbor_for_close(&self, id: TabId) -> Option<TabId> {
        let index = self.tabs.iter().position(|tab| tab.id == id)?;
        self.tabs
            .get(index + 1)
            .or_else(|| index.checked_sub(1).and_then(|index| self.tabs.get(index)))
            .map(|tab| tab.id)
    }

    pub(crate) fn active_terminal(&self) -> Option<Arc<Mutex<Terminal>>> {
        let id = self.active?;
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| Arc::clone(&tab.terminal))
    }

    pub(crate) fn active_bridge(&self) -> Option<TerminalWidgetBridge> {
        let id = self.active?;
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.bridge.clone())
    }

    pub(crate) fn snapshots(&self) -> Vec<TabSnapshot> {
        self.tabs
            .iter()
            .map(|tab| TabSnapshot {
                id: tab.id,
                title: tab.title.clone(),
                unread: tab.unread,
                draw_id: tab.draw_id,
                active: self.active == Some(tab.id),
            })
            .collect()
    }

    pub(crate) fn activate(&mut self, id: TabId) -> TabActionOutcome {
        if self.active == Some(id) || !self.tabs.iter().any(|tab| tab.id == id) {
            return TabActionOutcome::unchanged();
        }
        self.active = Some(id);
        let unread_changed = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == id)
            .is_some_and(|tab| std::mem::take(&mut tab.unread));
        TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed,
            close_window: false,
        }
    }

    pub(crate) fn activate_next(&mut self) -> TabActionOutcome {
        let Some(active) = self.active else {
            return TabActionOutcome::unchanged();
        };
        let Some(index) = self.tabs.iter().position(|tab| tab.id == active) else {
            return TabActionOutcome::unchanged();
        };
        let next = (index + 1) % self.tabs.len();
        self.activate(self.tabs[next].id)
    }

    pub(crate) fn activate_previous(&mut self) -> TabActionOutcome {
        let Some(active) = self.active else {
            return TabActionOutcome::unchanged();
        };
        let Some(index) = self.tabs.iter().position(|tab| tab.id == active) else {
            return TabActionOutcome::unchanged();
        };
        let previous = if index == 0 {
            self.tabs.len() - 1
        } else {
            index - 1
        };
        self.activate(self.tabs[previous].id)
    }

    pub(crate) fn activate_numeric(&mut self, one_based: usize) -> TabActionOutcome {
        let Some(index) = one_based.checked_sub(1) else {
            return TabActionOutcome::unchanged();
        };
        let Some(tab) = self.tabs.get(index) else {
            return TabActionOutcome::unchanged();
        };
        self.activate(tab.id)
    }

    pub(crate) fn close(&mut self, id: TabId) -> TabActionOutcome {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return TabActionOutcome::unchanged();
        };
        let was_active = self.active == Some(id);
        self.tabs.remove(index);

        if self.tabs.is_empty() {
            self.active = None;
            return TabActionOutcome {
                active_bridge_changed: was_active,
                request_redraw: false,
                unread_changed: false,
                close_window: true,
            };
        }
        if !was_active {
            return TabActionOutcome {
                request_redraw: true,
                ..TabActionOutcome::unchanged()
            };
        }

        // The old right neighbor moves into `index`; when there was none, use the new last item.
        let replacement = self.tabs[index.min(self.tabs.len() - 1)].id;
        self.active = Some(replacement);
        let unread_changed = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == replacement)
            .is_some_and(|tab| std::mem::take(&mut tab.unread));
        TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed,
            close_window: false,
        }
    }

    /// Drains output for a live tab. Only an active tab asks Runtime to invalidate its mounted
    /// external draw; inactive tabs update model state without exposing a schedule provider.
    pub(crate) fn process_output(&mut self, id: TabId) -> TabOutputOutcome {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
            return TabOutputOutcome::default();
        };
        let Ok(mut terminal) = tab.terminal.lock() else {
            return TabOutputOutcome::default();
        };
        let ingested_output = terminal.drain_pty();
        if self.active == Some(id) {
            TabOutputOutcome {
                request_active_invalidation: true,
                unread_changed: false,
            }
        } else {
            let unread_changed = ingested_output && !std::mem::replace(&mut tab.unread, true);
            TabOutputOutcome {
                request_active_invalidation: false,
                unread_changed,
            }
        }
    }

    /// Provides the safe live-tab resize point consumed by T0008.
    pub(crate) fn resize_all(&mut self, size: TerminalSize) {
        if size.rows == 0 || size.cols == 0 {
            return;
        }
        for tab in &mut self.tabs {
            if let Ok(mut terminal) = tab.terminal.lock() {
                terminal.resize(size.rows, size.cols);
            }
        }
    }

    fn reserve_ids(&mut self) -> Result<(TabId, ExternalDrawId)> {
        let id = self.next_tab_id;
        let draw_id = self.next_draw_id;
        self.next_tab_id = self
            .next_tab_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("terminal tab id space exhausted"))?;
        self.next_draw_id = self
            .next_draw_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("external draw id space exhausted"))?;
        Ok((TabId(id), draw_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn resources(id: ExternalDrawId) -> TerminalTabResources {
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
        let bridge =
            TerminalWidgetBridge::new(id, Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
        TerminalTabResources::new(terminal, bridge)
    }

    fn create(manager: &mut TabManager) -> (TabId, ExternalDrawId) {
        manager
            .create_tab(|_, draw_id| Ok(resources(draw_id)))
            .unwrap();
        let snapshot = manager.snapshots().pop().unwrap();
        (snapshot.id, snapshot.draw_id)
    }

    #[test]
    fn create_allocates_ordered_distinct_ids_and_activates_new_tab() {
        let mut manager = TabManager::new();
        let (a, draw_a) = create(&mut manager);
        let (b, draw_b) = create(&mut manager);

        assert_ne!(a, b);
        assert_ne!(draw_a, draw_b);
        assert_eq!(manager.active_id(), Some(b));
        assert_eq!(
            manager
                .snapshots()
                .iter()
                .map(|tab| tab.title.as_str())
                .collect::<Vec<_>>(),
            ["Terminal 1", "Terminal 2"]
        );
    }

    #[test]
    fn activation_wraps_supports_numeric_lookup_and_clears_unread() {
        let mut manager = TabManager::new();
        let (a, _) = create(&mut manager);
        let (b, _) = create(&mut manager);
        let (c, _) = create(&mut manager);
        manager
            .tabs
            .iter_mut()
            .find(|tab| tab.id == a)
            .unwrap()
            .unread = true;

        assert!(manager.activate_next().active_bridge_changed);
        assert_eq!(manager.active_id(), Some(a));
        assert!(!manager.snapshots()[0].unread);
        assert!(manager.activate_previous().active_bridge_changed);
        assert_eq!(manager.active_id(), Some(c));
        assert!(manager.activate_numeric(2).active_bridge_changed);
        assert_eq!(manager.active_id(), Some(b));
        assert_eq!(manager.activate_numeric(9), TabActionOutcome::unchanged());
    }

    #[test]
    fn inactive_output_marks_unread_only_when_bytes_are_ingested() {
        use std::io::Cursor;
        use std::time::Duration;

        let (wake_tx, wake_rx) = std::sync::mpsc::channel();
        let mut manager = TabManager::new();
        manager
            .create_tab(|_, draw_id| {
                #[allow(clippy::arc_with_non_send_sync)]
                let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                    4,
                    20,
                    Cursor::new(b"background".to_vec()),
                    std::io::sink(),
                    move || wake_tx.send(()).is_ok(),
                )));
                let bridge = TerminalWidgetBridge::new(
                    draw_id,
                    Arc::clone(&terminal),
                    Arc::new(AtomicBool::new(false)),
                );
                Ok(TerminalTabResources::new(terminal, bridge))
            })
            .unwrap();
        let background = manager.active_id().unwrap();
        create(&mut manager);
        wake_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("reader should announce queued output");

        assert!(manager.process_output(background).unread_changed);
        assert!(!manager.process_output(background).unread_changed);
        assert!(
            manager
                .snapshots()
                .iter()
                .find(|tab| tab.id == background)
                .unwrap()
                .unread
        );
    }

    #[test]
    fn close_active_selects_right_then_left_and_final_requests_window_close() {
        let mut manager = TabManager::new();
        let (a, _) = create(&mut manager);
        let (b, _) = create(&mut manager);
        let (c, _) = create(&mut manager);
        manager.activate(b);

        assert_eq!(manager.neighbor_for_close(b), Some(c));
        assert!(manager.close(b).active_bridge_changed);
        assert_eq!(manager.active_id(), Some(c));
        assert_eq!(manager.neighbor_for_close(c), Some(a));
        assert!(manager.close(c).active_bridge_changed);
        assert_eq!(manager.active_id(), Some(a));
        assert_eq!(manager.neighbor_for_close(a), None);
        assert_eq!(manager.neighbor_for_close(TabId(u64::MAX)), None);
        let final_outcome = manager.close(a);
        assert!(final_outcome.close_window);
        assert_eq!(manager.active_id(), None);
    }

    #[test]
    fn close_inactive_requests_redraw_while_stale_ids_are_noops() {
        let mut manager = TabManager::new();
        let (a, _) = create(&mut manager);
        let (b, _) = create(&mut manager);
        assert_eq!(
            manager.close(a),
            TabActionOutcome {
                request_redraw: true,
                ..TabActionOutcome::unchanged()
            }
        );
        assert_eq!(manager.active_id(), Some(b));
        assert_eq!(manager.close(a), TabActionOutcome::unchanged());
        assert_eq!(manager.process_output(a), TabOutputOutcome::default());
    }

    #[test]
    fn terminals_keep_independent_screen_state() {
        let mut manager = TabManager::new();
        let (a, _) = create(&mut manager);
        let terminal_a = manager
            .tabs
            .iter()
            .find(|tab| tab.id == a)
            .unwrap()
            .terminal
            .clone();
        terminal_a.lock().unwrap().process_output(b"alpha");
        let (b, _) = create(&mut manager);
        let terminal_b = manager
            .tabs
            .iter()
            .find(|tab| tab.id == b)
            .unwrap()
            .terminal
            .clone();
        terminal_b.lock().unwrap().process_output(b"beta");

        assert!(terminal_a.lock().unwrap().row_text(0).contains("alpha"));
        assert!(!terminal_a.lock().unwrap().row_text(0).contains("beta"));
        assert!(terminal_b.lock().unwrap().row_text(0).contains("beta"));
    }

    #[test]
    fn closing_tab_releases_terminal_and_bridge_ownership() {
        let mut manager = TabManager::new();
        let mut weak = None;
        manager
            .create_tab(|_, draw_id| {
                let made = resources(draw_id);
                weak = Some(Arc::downgrade(&made.terminal));
                Ok(made)
            })
            .unwrap();
        let id = manager.active_id().unwrap();

        assert!(manager.close(id).close_window);
        assert!(weak.unwrap().upgrade().is_none());
    }

    #[test]
    fn runtime_mount_and_schedule_follow_only_the_selected_bridge() {
        use harbor_widget::scene::primitive::Primitive;
        use harbor_widget::widgets::sized_box::SizedBox;

        fn mounted_external_ids(runtime: &harbor_widget::runtime::Runtime) -> Vec<ExternalDrawId> {
            runtime
                .pending_delta()
                .expect("root update produces a scene delta")
                .added
                .iter()
                .filter_map(|item| match item.primitive {
                    Primitive::External { draw, .. } => Some(draw),
                    _ => None,
                })
                .collect()
        }

        let mut manager = TabManager::new();
        let (a, draw_a) = create(&mut manager);
        manager
            .tabs
            .iter()
            .find(|tab| tab.id == a)
            .unwrap()
            .terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[?2026hhidden");
        let (b, draw_b) = create(&mut manager);
        let mut runtime = harbor_widget::runtime::Runtime::new();
        runtime.set_root(manager.active_bridge().unwrap());
        let active_b = runtime.update(std::time::Instant::now());
        assert_eq!(mounted_external_ids(&runtime), [draw_b]);
        assert!(!active_b.has_deferred_externals);

        manager.activate(a);
        runtime.set_root(manager.active_bridge().unwrap());
        let active_a = runtime.update(std::time::Instant::now());
        assert_eq!(mounted_external_ids(&runtime), [draw_a]);
        assert!(active_a.has_deferred_externals);

        assert!(manager.close(a).active_bridge_changed);
        runtime.set_root(manager.active_bridge().unwrap());
        let back_to_b = runtime.update(std::time::Instant::now());
        assert_eq!(mounted_external_ids(&runtime), [draw_b]);
        assert!(!back_to_b.has_deferred_externals);

        assert!(manager.close(b).close_window);
        runtime.set_root(SizedBox::new(harbor_widget::layout::Size::ZERO));
        runtime.update(std::time::Instant::now());
        assert!(!runtime.has_external_draws());
    }

    #[test]
    fn failed_factory_burns_ids_without_mutating_tabs() {
        let mut manager = TabManager::new();
        let failure = manager.create_tab(|_, _| Err(anyhow!("spawn failed")));
        assert!(failure.is_err());
        assert!(manager.snapshots().is_empty());

        let (id, draw_id) = create(&mut manager);
        assert_eq!(id.get(), 2);
        assert_eq!(draw_id, 2);
    }

    #[test]
    fn exhausted_id_space_returns_error_instead_of_wrapping() {
        let mut manager = TabManager {
            next_tab_id: u64::MAX,
            ..TabManager::new()
        };
        assert!(
            manager
                .create_tab(|_, draw_id| Ok(resources(draw_id)))
                .is_err()
        );
        assert!(manager.tabs.is_empty());
    }
}
