//! Host-owned terminal tab/session state and routing.

use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, anyhow};
use harbor_terminal::{
    ClipboardDelivery, ClipboardWrite, ShellIntegrationMarker, Terminal, TerminalOutputEvent,
    TerminalSize, WorkingDirectoryMetadata,
};
use harbor_widget::scene::primitive::ExternalDrawId;

use crate::terminal_view::TerminalWidgetBridge;

/// Stable terminal-session identity. Values are monotonic and never reused.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TabId(pub u64);

impl TabId {
    #[inline]
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Strongly-typed 1-based index for tab rail navigation (1..=9).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TabIndex(u8);

impl TabIndex {
    #[allow(dead_code)]
    /// Creates a `TabIndex` if `index` is within the valid range 1..=9.
    pub(crate) const fn new(index: u8) -> Option<Self> {
        if index >= 1 && index <= 9 {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Creates a `TabIndex` for a known-valid digit in 1..=9.
    #[inline]
    pub(crate) const fn from_valid_u8(index: u8) -> Self {
        debug_assert!(index >= 1 && index <= 9);
        Self(index)
    }

    /// Returns the 0-based index suitable for array/slice indexing.
    #[inline]
    pub(crate) const fn to_zero_based(self) -> usize {
        (self.0 - 1) as usize
    }

    #[allow(dead_code)]
    /// Returns the 1-based index (1..=9).
    #[inline]
    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}

/// Resources produced by the Host's per-tab factory.
pub struct TerminalTabResources {
    terminal: Arc<Mutex<Terminal>>,
    bridge: TerminalWidgetBridge,
    shell_fallback: String,
}

impl TerminalTabResources {
    pub fn new(
        terminal: Arc<Mutex<Terminal>>,
        bridge: TerminalWidgetBridge,
        shell_fallback: impl Into<String>,
    ) -> Self {
        Self {
            terminal,
            bridge,
            shell_fallback: shell_fallback.into(),
        }
    }
}

struct TerminalTab {
    id: TabId,
    title: String,
    shell_fallback: String,
    working_directory: Option<WorkingDirectoryMetadata>,
    shell_integration: Option<ShellIntegrationMarker>,
    unread: bool,
    terminal: Arc<Mutex<Terminal>>,
    bridge: TerminalWidgetBridge,
}

/// Read-only product state for a tab rail or Host test.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabSnapshot {
    pub(crate) id: TabId,
    pub(crate) title: String,
    pub(crate) working_directory: Option<WorkingDirectoryMetadata>,
    pub(crate) shell_integration: Option<ShellIntegrationMarker>,
    pub(crate) unread: bool,
    pub(crate) draw_id: ExternalDrawId,
    pub(crate) active: bool,
}

impl TabSnapshot {
    /// Returns the latest validated OSC 7 metadata retained for this tab.
    pub const fn working_directory(&self) -> Option<&WorkingDirectoryMetadata> {
        self.working_directory.as_ref()
    }

    /// Returns the latest validated OSC 133 marker retained for this tab.
    pub const fn shell_integration(&self) -> Option<ShellIntegrationMarker> {
        self.shell_integration
    }
}

/// Effects the Host must apply after a model transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TabActionOutcome {
    pub active_bridge_changed: bool,
    pub request_redraw: bool,
    pub unread_changed: bool,
    pub active_title_changed: bool,
    pub close_window: bool,
}

/// A bounded clipboard candidate carrying its stable originating session.
/// Routing alone does not authorize the host clipboard effect.
#[derive(Clone, Eq, PartialEq)]
pub struct SessionClipboardWrite {
    pub source: TabId,
    pub contents: ClipboardWrite,
}

impl std::fmt::Debug for SessionClipboardWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionClipboardWrite")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// Classification of one tab-qualified PTY wake.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TabOutputOutcome {
    pub request_active_invalidation: bool,
    pub unread_changed: bool,
    pub title_changed: bool,
    pub active_title_changed: bool,
    pub metadata_changed: bool,
    pub clipboard_write: Option<SessionClipboardWrite>,
}

fn normalize_shell_title(title: String, shell_fallback: &str) -> String {
    let basename = title
        .rsplit(['\\', '/'])
        .find(|component| !component.is_empty())
        .unwrap_or(&title);
    let suffix_start = basename.len().saturating_sub(4);
    let shell_name = basename
        .get(..suffix_start)
        .filter(|_| {
            basename
                .get(suffix_start..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".exe"))
        })
        .unwrap_or(basename);

    if shell_name.eq_ignore_ascii_case(shell_fallback) {
        shell_fallback.to_owned()
    } else {
        title
    }
}

// One ready active attempt plus at least one rotating background opportunity.
const RESIZE_BATCH_LIMIT: usize = 2;

/// Effects of the single ordinary resize admission batch in a host turn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResizeBatchOutcome {
    pub active_changed: bool,
    pub ready_remaining: bool,
    pub attempted: usize,
}

/// Ordered Host-owned terminal sessions.
pub struct TabManager {
    pub(crate) search: crate::search::SessionSearch,
    tabs: Vec<TerminalTab>,
    active: Option<TabId>,
    next_tab_id: u64,
    next_draw_id: ExternalDrawId,
    last_broadcast_size: Option<TerminalSize>,
    resize_target: Option<TerminalSize>,
    resize_cursor: usize,
    clipboard_delivery: Option<ClipboardDelivery>,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // T0007 binds the transition API to product commands.
impl TabManager {
    pub fn new() -> Self {
        Self {
            search: crate::search::SessionSearch::default(),
            tabs: Vec::new(),
            active: None,
            next_tab_id: 1,
            next_draw_id: ExternalDrawId::new(1),
            last_broadcast_size: None,
            resize_target: None,
            resize_cursor: 0,
            clipboard_delivery: None,
        }
    }

    /// Reserves identities before invoking the factory, so even a failed factory cannot allow a
    /// queued stale wake to target a later session.
    pub fn create_tab(
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

        if let Some(delivery) = self.clipboard_delivery {
            resources
                .terminal
                .lock()
                .map_err(|_| anyhow!("terminal lock poisoned during tab creation"))?
                .set_clipboard_delivery(delivery);
        }
        // A tab's output/input/draw callbacks may dispatch ACKs, never admit resize work.
        resources
            .terminal
            .lock()
            .map_err(|_| anyhow!("terminal lock poisoned during tab creation"))?
            .use_host_resize_scheduling();
        self.last_broadcast_size = None;
        let title = resources.shell_fallback.clone();
        self.tabs.push(TerminalTab {
            id,
            title,
            shell_fallback: resources.shell_fallback,
            working_directory: None,
            shell_integration: None,
            unread: false,
            terminal: resources.terminal,
            bridge: resources.bridge,
        });
        self.cancel_search();
        self.clear_active_preedit();
        self.active = Some(id);
        Ok(TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed: false,
            active_title_changed: true,
            close_window: false,
        })
    }

    /// The shared tab reduction used by Host command draining; only resource creation
    /// is supplied by the Host. The model preserves numeric bounds/final-tab close.
    pub fn execute_command(
        &mut self,
        command: crate::tab_view::TabCommand,
        create: impl FnOnce(TabId, ExternalDrawId) -> Result<TerminalTabResources>,
    ) -> Result<TabActionOutcome> {
        use crate::tab_view::TabCommand;
        Ok(match command {
            TabCommand::New => return self.create_tab(create),
            TabCommand::Close(id) => self.close(id),
            TabCommand::CloseActive => self.close_active(),
            TabCommand::Activate(id) => self.activate(id),
            TabCommand::Next => self.activate_next(),
            TabCommand::Previous => self.activate_previous(),
            TabCommand::Numeric(index) => self.activate_numeric(index),
        })
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active
    }

    /// Session eligibility only; the host must separately validate window focus.
    pub fn active_live(&self, id: TabId) -> bool {
        self.active == Some(id)
            && self.tabs.iter().any(|tab| {
                tab.id == id
                    && tab
                        .terminal
                        .lock()
                        .is_ok_and(|terminal| !terminal.is_session_closed())
            })
    }

    /// Bounded display-only label, never a replacement for stable source identity.
    pub fn source_label(&self, id: TabId) -> String {
        const MAX_TITLE_CHARS: usize = 80;
        match self.tabs.iter().find(|tab| tab.id == id) {
            Some(tab) if !tab.title.is_empty() => tab.title.chars().take(MAX_TITLE_CHARS).collect(),
            _ => format!("Session {id}"),
        }
    }

    /// Overrides delivery on all current and subsequently created sessions.
    /// Without an override, each factory's terminal delivery setting is preserved.
    pub fn set_clipboard_delivery(&mut self, delivery: ClipboardDelivery) {
        self.clipboard_delivery = Some(delivery);
        for tab in &self.tabs {
            if let Ok(mut terminal) = tab.terminal.lock() {
                terminal.set_clipboard_delivery(delivery);
            }
        }
    }

    /// Selects the right neighbor, then the left, for rail focus after a close.
    pub fn neighbor_for_close(&self, id: TabId) -> Option<TabId> {
        let index = self.tabs.iter().position(|tab| tab.id == id)?;
        self.tabs
            .get(index + 1)
            .or_else(|| index.checked_sub(1).and_then(|index| self.tabs.get(index)))
            .map(|tab| tab.id)
    }

    fn active_tab(&self) -> Option<&TerminalTab> {
        let id = self.active?;
        self.tabs.iter().find(|tab| tab.id == id)
    }

    fn clear_active_preedit(&self) {
        if let Some(tab) = self.active_tab()
            && let Ok(mut terminal) = tab.terminal.lock()
        {
            terminal.clear_preedit();
        }
    }

    pub fn active_terminal(&self) -> Option<Arc<Mutex<Terminal>>> {
        self.active_tab().map(|tab| Arc::clone(&tab.terminal))
    }
    /// Stable identities and shared engines/bridges for one application-thread transaction.
    pub fn configuration_targets(
        &self,
    ) -> Vec<(TabId, Arc<Mutex<Terminal>>, TerminalWidgetBridge)> {
        self.tabs
            .iter()
            .map(|tab| (tab.id, Arc::clone(&tab.terminal), tab.bridge.clone()))
            .collect()
    }

    pub fn accept_configuration_size(&mut self, size: TerminalSize) {
        self.resize_target = Some(size);
        self.last_broadcast_size = Some(size);
    }

    pub fn active_bridge(&self) -> Option<TerminalWidgetBridge> {
        self.active_tab().map(|tab| tab.bridge.clone())
    }

    pub fn active_title(&self) -> Option<&str> {
        self.active_tab().map(|tab| tab.title.as_str())
    }

    pub fn snapshots(&self) -> Vec<TabSnapshot> {
        self.tabs
            .iter()
            .map(|tab| TabSnapshot {
                id: tab.id,
                title: tab.title.clone(),
                working_directory: tab.working_directory.clone(),
                shell_integration: tab.shell_integration,
                unread: tab.unread,
                draw_id: tab.bridge.draw_id(),
                active: self.active == Some(tab.id),
            })
            .collect()
    }

    pub fn activate(&mut self, id: TabId) -> TabActionOutcome {
        if self.active == Some(id) {
            return TabActionOutcome::default();
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return TabActionOutcome::default();
        };
        self.cancel_search();
        self.clear_active_preedit();
        self.active = Some(id);
        let unread_changed = std::mem::take(&mut self.tabs[index].unread);
        TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed,
            active_title_changed: true,
            close_window: false,
        }
    }

    pub fn activate_next(&mut self) -> TabActionOutcome {
        let Some(active) = self.active else {
            return TabActionOutcome::default();
        };
        let Some(index) = self.tabs.iter().position(|tab| tab.id == active) else {
            return TabActionOutcome::default();
        };
        let next = (index + 1) % self.tabs.len();
        self.activate(self.tabs[next].id)
    }

    pub fn activate_previous(&mut self) -> TabActionOutcome {
        let Some(active) = self.active else {
            return TabActionOutcome::default();
        };
        let Some(index) = self.tabs.iter().position(|tab| tab.id == active) else {
            return TabActionOutcome::default();
        };
        let previous = index.checked_sub(1).unwrap_or(self.tabs.len() - 1);
        self.activate(self.tabs[previous].id)
    }

    pub fn activate_numeric(&mut self, index: TabIndex) -> TabActionOutcome {
        let Some(tab) = self.tabs.get(index.to_zero_based()) else {
            return TabActionOutcome::default();
        };
        self.activate(tab.id)
    }

    pub fn close(&mut self, id: TabId) -> TabActionOutcome {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return TabActionOutcome::default();
        };
        let was_active = self.active == Some(id);
        if was_active {
            self.cancel_search();
        }
        let removed = self.tabs.remove(index);
        if let Ok(mut terminal) = removed.terminal.lock() {
            terminal.cancel_resize();
            if was_active {
                terminal.clear_preedit();
            }
        }

        if self.tabs.is_empty() {
            self.active = None;
            return TabActionOutcome {
                active_bridge_changed: was_active,
                request_redraw: false,
                unread_changed: false,
                active_title_changed: was_active,
                close_window: true,
            };
        }
        if !was_active {
            return TabActionOutcome {
                request_redraw: true,
                ..TabActionOutcome::default()
            };
        }

        // The old right neighbor moves into `index`; when there was none, use the new last item.
        let replacement_index = index.min(self.tabs.len() - 1);
        let replacement = &mut self.tabs[replacement_index];
        self.active = Some(replacement.id);
        let unread_changed = std::mem::take(&mut replacement.unread);
        TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed,
            active_title_changed: true,
            close_window: false,
        }
    }

    pub fn close_active(&mut self) -> TabActionOutcome {
        match self.active {
            Some(id) => self.close(id),
            None => TabActionOutcome::default(),
        }
    }

    /// Drains output for a live tab. Only an active tab asks Runtime to invalidate its mounted
    /// external draw; inactive tabs update model state without exposing a schedule provider.
    pub fn process_output(&mut self, id: TabId) -> TabOutputOutcome {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
            return TabOutputOutcome::default();
        };
        let Ok(mut terminal) = tab.terminal.lock() else {
            return TabOutputOutcome::default();
        };
        let ingested_output = terminal.drain_pty();
        let output_events = terminal.drain_output_events();
        drop(terminal);

        let previous_title = tab.title.clone();
        let previous_working_directory = tab.working_directory.clone();
        let previous_shell_integration = tab.shell_integration;
        let mut clipboard_write = None;
        for event in output_events {
            match event {
                TerminalOutputEvent::ClipboardWrite(contents) => {
                    clipboard_write = Some(SessionClipboardWrite {
                        source: id,
                        contents,
                    });
                }
                TerminalOutputEvent::TitleChanged(title) => {
                    tab.title = normalize_shell_title(title, &tab.shell_fallback);
                }
                TerminalOutputEvent::TitleReset => tab.title.clone_from(&tab.shell_fallback),
                TerminalOutputEvent::WorkingDirectoryChanged(metadata) => {
                    tab.working_directory = Some(metadata);
                }
                TerminalOutputEvent::WorkingDirectoryReset => tab.working_directory = None,
                TerminalOutputEvent::ShellIntegration(marker) => {
                    tab.shell_integration = Some(marker);
                }
                TerminalOutputEvent::ShellIntegrationReset => tab.shell_integration = None,
            }
        }
        let metadata_changed = tab.working_directory != previous_working_directory
            || tab.shell_integration != previous_shell_integration;
        let title_changed = tab.title != previous_title;
        let active = self.active == Some(id);
        let unread_changed =
            !active && ingested_output && !std::mem::replace(&mut tab.unread, true);
        TabOutputOutcome {
            request_active_invalidation: active,
            unread_changed,
            title_changed,
            active_title_changed: active && title_changed,
            metadata_changed,
            clipboard_write,
        }
    }

    /// Submit the latest valid allocation to every live terminal without running
    /// any expensive transaction. Returns true only on newly observed all-tab completion.
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
        for tab in &mut self.tabs {
            match tab.terminal.lock() {
                Ok(mut terminal) => {
                    if let Err(error) = terminal.request_resize(size) {
                        tracing::warn!(tab_id = tab.id.get(), %error, "failed to request terminal resize");
                    }
                }
                Err(_) => tracing::warn!(
                    tab_id = tab.id.get(),
                    "terminal lock unavailable during resize"
                ),
            }
        }
        self.refresh_resize_completion()
    }

    fn refresh_resize_completion(&mut self) -> bool {
        let previous = self.last_broadcast_size;
        self.last_broadcast_size = self.resize_target.filter(|size| {
            !self.tabs.is_empty()
                && self.tabs.iter().all(|tab| {
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
        let active = self.active;
        if let Some(tab) = self.tabs.iter().find(|tab| Some(tab.id) == active) {
            Self::advance_tab_resize(tab, true, &mut outcome);
        }
        // A persistent cursor, not Vec order or the current active identity, owns
        // background fairness. Failure attempts also consume a slot.
        for _ in 0..self.tabs.len() {
            if outcome.attempted == RESIZE_BATCH_LIMIT {
                break;
            }
            self.resize_cursor %= self.tabs.len();
            let index = self.resize_cursor;
            self.resize_cursor = (index + 1) % self.tabs.len();
            let tab = &self.tabs[index];
            if Some(tab.id) != active {
                Self::advance_tab_resize(tab, false, &mut outcome);
            }
        }
        outcome.ready_remaining = self.tabs.iter().any(|tab| {
            tab.terminal
                .lock()
                .is_ok_and(|terminal| terminal.resize_status().needs_advance())
        });
        self.refresh_resize_completion();
        outcome
    }

    fn advance_tab_resize(tab: &TerminalTab, active: bool, outcome: &mut ResizeBatchOutcome) {
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
                tracing::warn!(tab_id = tab.id.get(), %error, "failed to complete terminal resize")
            }
        }
    }

    pub const fn last_broadcast_size(&self) -> Option<TerminalSize> {
        self.last_broadcast_size
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
mod tests;
