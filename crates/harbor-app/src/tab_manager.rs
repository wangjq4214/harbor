//! Host-owned workspaces, pane bindings and independent session lifetimes.

use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, anyhow};
use harbor_terminal::{
    ClipboardDelivery, ClipboardWrite, ShellIntegrationMarker, Terminal, TerminalOutputEvent,
    TerminalSize, WorkingDirectoryMetadata,
};
use harbor_widget::scene::primitive::ExternalDrawId;

use crate::terminal_view::TerminalWidgetBridge;

/// Stable workspace (tab rail) identity; never a session or external draw identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TabId(pub u64);

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
    published: bool,
}

impl TerminalTabResources {
    /// Access an unpublished session while its Host startup operation owns it.
    /// Dropping the resource owner still retires this engine even if a clone remains.
    pub fn terminal(&self) -> &Arc<Mutex<Terminal>> {
        &self.terminal
    }
    /// Presentation/authentication seam for a still-unpublished startup attempt.
    /// The resource owner, not a retained presentation clone, controls retirement.
    pub fn bridge(&self) -> &TerminalWidgetBridge {
        &self.bridge
    }
    pub fn new(
        terminal: Arc<Mutex<Terminal>>,
        bridge: TerminalWidgetBridge,
        shell_fallback: impl Into<String>,
    ) -> Self {
        Self {
            terminal,
            bridge,
            shell_fallback: shell_fallback.into(),
            published: false,
        }
    }
}

impl Drop for TerminalTabResources {
    fn drop(&mut self) {
        if !self.published {
            self.bridge.retire();
            self.terminal
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .retire_session();
        }
    }
}

/// A pane is a presentation leaf, not an owner of the process. Workspace removal
/// retires its sessions explicitly; UI unmount/root replacement does not.
struct TerminalTab {
    id: TabId,
    focused: PaneId,
    panes: Vec<TerminalPane>,
    tree: crate::pane_layout::PaneTree,
    maximized: Option<PaneId>,
}

struct TerminalPane {
    id: PaneId,
    session: SessionId,
}

/// Host routing token reserved BEFORE resource creation. This is not the engine's
/// read_model::SessionId: content/revision proofs continue to use that engine identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionId(pub u64);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable presentation identity, independent of position, session and draw registration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PaneId(pub u64);

/// Source capability for delayed Host operations. Revalidate the complete binding
/// with contains_binding before attaching a result; never substitute the active pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneBinding {
    pub workspace: TabId,
    pub pane: PaneId,
    pub session: SessionId,
}

struct TerminalSession {
    id: SessionId,
    title: String,
    shell_fallback: String,
    working_directory: Option<WorkingDirectoryMetadata>,
    shell_integration: Option<ShellIntegrationMarker>,
    unread: bool,
    terminal: Arc<Mutex<Terminal>>,
    // One projection per session (mirrored sessions are out of scope).
    bridge: TerminalWidgetBridge,
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.bridge.retire();
        self.terminal
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .retire_session();
    }
}

/// Read-only product state for a tab rail or Host test.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabSnapshot {
    pub(crate) id: TabId,
    pub(crate) binding: PaneBinding,
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
    pub source: SessionId,
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
    /// A visible leaf of the active workspace may need presentation, regardless of focus.
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
    /// Any visible leaf changed; the historical field name does not imply keyboard focus.
    pub active_changed: bool,
    pub ready_remaining: bool,
    pub attempted: usize,
}

/// Ordered Host-owned terminal sessions.
pub struct TabManager {
    pub(crate) search: crate::search::SessionSearch,
    tabs: Vec<TerminalTab>,
    sessions: Vec<TerminalSession>,
    active: Option<TabId>,
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    next_draw_id: ExternalDrawId,
    last_broadcast_size: Option<TerminalSize>,
    resize_target: Option<TerminalSize>,
    resize_cursor: usize,
    clipboard_delivery: Option<ClipboardDelivery>,
    split_reservations:
        std::collections::HashMap<PaneId, (PaneBinding, PaneBinding, ExternalDrawId)>,
    pane_targets: std::collections::HashMap<SessionId, TerminalSize>,
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
            sessions: Vec::new(),
            active: None,
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            next_draw_id: ExternalDrawId::new(1),
            last_broadcast_size: None,
            resize_target: None,
            resize_cursor: 0,
            clipboard_delivery: None,
            split_reservations: Default::default(),
            pane_targets: Default::default(),
        }
    }

    /// Reserves identities before invoking the factory, so even a failed factory cannot allow a
    /// queued stale wake to target a later session.
    pub fn create_tab(
        &mut self,
        factory: impl FnOnce(SessionId, ExternalDrawId) -> Result<TerminalTabResources>,
    ) -> Result<TabActionOutcome> {
        let (binding, draw_id) = self.reserve_ids()?;
        let mut resources =
            factory(binding.session, draw_id).context("create terminal tab resources")?;
        if resources.bridge.draw_id() != draw_id {
            return Err(anyhow!(
                "terminal bridge draw id mismatch: allocated {draw_id}, received {}",
                resources.bridge.draw_id()
            ));
        }

        anyhow::ensure!(
            resources.bridge.is_bound_to(&resources.terminal),
            "terminal bridge engine mismatch"
        );
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
        self.sessions.push(TerminalSession {
            id: binding.session,
            title,
            shell_fallback: resources.shell_fallback.clone(),
            working_directory: None,
            shell_integration: None,
            unread: false,
            terminal: Arc::clone(&resources.terminal),
            bridge: resources.bridge.clone(),
        });
        resources.published = true;
        self.tabs.push(TerminalTab {
            id: binding.workspace,
            focused: binding.pane,
            tree: crate::pane_layout::PaneTree::Leaf(binding.pane),
            maximized: None,
            panes: vec![TerminalPane {
                id: binding.pane,
                session: binding.session,
            }],
        });
        self.cancel_search();
        self.clear_active_preedit();
        self.active = Some(binding.workspace);
        self.freeze_hidden_grids();
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
        create: impl FnOnce(SessionId, ExternalDrawId) -> Result<TerminalTabResources>,
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

    /// Resolve a captured routing identity back to its current workspace/pane,
    /// without assuming one session per tab or converting numeric IDs.
    pub fn binding_for_session(&self, id: SessionId) -> Option<PaneBinding> {
        self.tabs.iter().find_map(|tab| {
            tab.panes
                .iter()
                .find(|pane| pane.session == id)
                .map(|pane| PaneBinding {
                    workspace: tab.id,
                    pane: pane.id,
                    session: id,
                })
        })
    }

    /// Current focused leaf. Capture this before asynchronous work.
    pub fn active_binding(&self) -> Option<PaneBinding> {
        let workspace = self.tabs.iter().find(|tab| Some(tab.id) == self.active)?;
        let pane = workspace
            .panes
            .iter()
            .find(|pane| pane.id == workspace.focused)?;
        Some(PaneBinding {
            workspace: workspace.id,
            pane: pane.id,
            session: pane.session,
        })
    }

    pub fn active_session_id(&self) -> Option<SessionId> {
        self.active_binding().map(|binding| binding.session)
    }

    pub fn contains_binding(&self, binding: PaneBinding) -> bool {
        self.tabs.iter().any(|tab| {
            tab.id == binding.workspace
                && tab
                    .panes
                    .iter()
                    .any(|pane| pane.id == binding.pane && pane.session == binding.session)
        }) && self
            .sessions
            .iter()
            .any(|session| session.id == binding.session)
    }

    /// Resolve the original session, not whichever pane is now focused. Callers
    /// still apply their own foreground/permission policy (e.g. paste and OSC 52).
    pub fn session_terminal(&self, id: SessionId) -> Option<Arc<Mutex<Terminal>>> {
        self.sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| Arc::clone(&session.terminal))
    }

    pub fn session_working_directory(&self, id: SessionId) -> Option<&WorkingDirectoryMetadata> {
        self.sessions
            .iter()
            .find(|session| session.id == id)?
            .working_directory
            .as_ref()
    }

    /// Session eligibility only; the host separately validates window focus.
    pub fn active_live(&self, id: SessionId) -> bool {
        self.active_session_id() == Some(id)
            && self.session_terminal(id).is_some_and(|terminal| {
                terminal.lock().is_ok_and(|terminal| {
                    !terminal.is_session_closed() && !terminal.shell_process_exited()
                })
            })
    }

    /// Bounded display-only label, never a replacement for stable source identity.
    pub fn source_label(&self, id: SessionId) -> String {
        const MAX_TITLE_CHARS: usize = 80;
        match self.sessions.iter().find(|tab| tab.id == id) {
            Some(tab) if !tab.title.is_empty() => tab.title.chars().take(MAX_TITLE_CHARS).collect(),
            _ => format!("Session {id}"),
        }
    }

    /// Overrides delivery on all current and subsequently created sessions.
    /// Without an override, each factory's terminal delivery setting is preserved.
    pub fn set_clipboard_delivery(&mut self, delivery: ClipboardDelivery) {
        self.clipboard_delivery = Some(delivery);
        for tab in &self.sessions {
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

    fn active_tab(&self) -> Option<&TerminalSession> {
        let id = self.active_session_id()?;
        self.sessions.iter().find(|session| session.id == id)
    }

    fn clear_active_preedit(&self) {
        if let Some(tab) = self.active_tab()
            && let Ok(mut terminal) = tab.terminal.lock()
        {
            terminal.clear_preedit();
        }
    }

    /// Latest source metadata; interpretation remains explicit Host policy.
    pub fn active_working_directory(&self) -> Option<&WorkingDirectoryMetadata> {
        self.active_tab()?.working_directory.as_ref()
    }
    pub fn active_terminal(&self) -> Option<Arc<Mutex<Terminal>>> {
        self.active_tab().map(|tab| Arc::clone(&tab.terminal))
    }
    /// Stable identities and shared engines/bridges for one application-thread transaction.
    pub fn configuration_targets(
        &self,
    ) -> Vec<(SessionId, Arc<Mutex<Terminal>>, TerminalWidgetBridge)> {
        self.sessions
            .iter()
            .map(|tab| (tab.id, Arc::clone(&tab.terminal), tab.bridge.clone()))
            .collect()
    }

    pub fn accept_configuration_size(&mut self, size: TerminalSize) {
        for session in &self.sessions {
            self.pane_targets.insert(session.id, size);
        }
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
            .filter_map(|workspace| {
                let pane = workspace
                    .panes
                    .iter()
                    .find(|pane| pane.id == workspace.focused)?;
                let tab = self
                    .sessions
                    .iter()
                    .find(|session| session.id == pane.session)?;
                Some(TabSnapshot {
                    id: workspace.id,
                    binding: PaneBinding {
                        workspace: workspace.id,
                        pane: pane.id,
                        session: pane.session,
                    },
                    title: tab.title.clone(),
                    working_directory: tab.working_directory.clone(),
                    shell_integration: tab.shell_integration,
                    unread: workspace.panes.iter().any(|pane| {
                        self.sessions
                            .iter()
                            .any(|session| session.id == pane.session && session.unread)
                    }),
                    draw_id: tab.bridge.draw_id(),
                    active: self.active == Some(workspace.id),
                })
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
        self.freeze_hidden_grids();
        let session = self.tabs[index]
            .panes
            .iter()
            .find(|pane| pane.id == self.tabs[index].focused)
            .unwrap()
            .session;
        let unread_changed = self.take_unread(session);
        TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed,
            active_title_changed: true,
            close_window: false,
        }
    }

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
        self.split_reservations
            .retain(|_, (source, _, _)| source.workspace != id);
        for pane in removed.panes {
            self.retire_session(pane.session);
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
        self.active = Some(self.tabs[replacement_index].id);
        self.freeze_hidden_grids();
        let unread_changed = self
            .active_session_id()
            .is_some_and(|id| self.take_unread(id));
        TabActionOutcome {
            active_bridge_changed: true,
            request_redraw: true,
            unread_changed,
            active_title_changed: true,
            close_window: false,
        }
    }

    fn take_unread(&mut self, id: SessionId) -> bool {
        self.sessions
            .iter_mut()
            .find(|session| session.id == id)
            .is_some_and(|session| std::mem::take(&mut session.unread))
    }

    fn retire_session(&mut self, id: SessionId) {
        let Some(index) = self.sessions.iter().position(|session| session.id == id) else {
            return;
        };
        // Drop retires PTY/reader ownership even while UI/configuration clones remain.
        self.pane_targets.remove(&id);
        self.sessions.remove(index);
    }

    /// Source-qualified shell exit reduction. EOF and OSC 133 command completion
    /// alone do not establish shell exit. Late duplicate wakes are harmless.
    pub fn close_exited_session(&mut self, session: SessionId) -> TabActionOutcome {
        let Some(binding) = self.binding_for_session(session) else {
            return TabActionOutcome::default();
        };
        let exited = self.session_terminal(session).is_some_and(|terminal| {
            terminal
                .lock()
                .is_ok_and(|terminal| terminal.shell_process_exited())
        });
        if exited {
            self.close_pane(binding)
        } else {
            TabActionOutcome::default()
        }
    }

    pub fn close_active(&mut self) -> TabActionOutcome {
        match self.active {
            Some(id) => self.close(id),
            None => TabActionOutcome::default(),
        }
    }

    /// Drain one live session. All visible leaves may invalidate presentation;
    /// hidden sessions ingest the same bounded output without a schedule provider.
    pub fn process_output(&mut self, id: SessionId) -> TabOutputOutcome {
        let active = self.active_session_id() == Some(id);
        let visible = self
            .visible_bindings()
            .iter()
            .any(|binding| binding.session == id);
        let Some(tab) = self.sessions.iter_mut().find(|tab| tab.id == id) else {
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
                TerminalOutputEvent::Project(_) => {} // Retained by the terminal, not UI snapshots.
            }
        }
        let metadata_changed = tab.working_directory != previous_working_directory
            || tab.shell_integration != previous_shell_integration;
        let title_changed = tab.title != previous_title;
        let unread_changed =
            !visible && ingested_output && !std::mem::replace(&mut tab.unread, true);
        TabOutputOutcome {
            request_active_invalidation: visible,
            unread_changed,
            title_changed,
            active_title_changed: active && title_changed,
            metadata_changed,
            clipboard_write,
        }
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
                .any(|session| session.id == target.session
                    || session.bridge.draw_id() == draw
                    || Arc::ptr_eq(&session.terminal, &resources.terminal)),
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
            match event {
                TerminalOutputEvent::TitleChanged(title) => {
                    session.title = normalize_shell_title(title, &session.shell_fallback)
                }
                TerminalOutputEvent::TitleReset => {
                    session.title.clone_from(&session.shell_fallback)
                }
                TerminalOutputEvent::WorkingDirectoryChanged(metadata) => {
                    session.working_directory = Some(metadata)
                }
                TerminalOutputEvent::WorkingDirectoryReset => session.working_directory = None,
                TerminalOutputEvent::ShellIntegration(marker) => {
                    session.shell_integration = Some(marker)
                }
                TerminalOutputEvent::ShellIntegrationReset => session.shell_integration = None,
                _ => {}
            }
        }
    }
    /// Model/presentation seam only; production focus/close command policy is T0004.
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

    fn freeze_hidden_grids(&mut self) {
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
            let Some(size) = crate::terminal_view::terminal_size_from_allocation(
                allocation.rect,
                scale,
                surface,
                metrics,
            ) else {
                continue;
            };
            self.pane_targets.insert(session.id, size);
            if let Ok(mut terminal) = session.terminal.lock() {
                let context = harbor_widget::scene::primitive::ExternalDrawContext::new(
                    allocation.rect,
                    harbor_widget::renderer::Viewport::new(surface.0, surface.1, scale),
                );
                terminal.set_input_geometry(
                    crate::terminal_view::render_target_from_context(&context),
                    metrics,
                );
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
    /// even on failure/cancellation; T0003 owns layout attachment, not reservation.
    pub fn reserve_split_ids(
        &mut self,
        source: PaneBinding,
    ) -> Result<(PaneBinding, ExternalDrawId)> {
        anyhow::ensure!(self.contains_binding(source), "split source is unavailable");
        let pane = PaneId(self.next_pane_id);
        let session = SessionId(self.next_session_id);
        let draw = self.next_draw_id;
        let next_pane = self
            .next_pane_id
            .checked_add(1)
            .context("pane identity exhausted")?;
        let next_session = self
            .next_session_id
            .checked_add(1)
            .context("session identity exhausted")?;
        let next_draw = draw.checked_add(1).context("draw identity exhausted")?;
        self.next_pane_id = next_pane;
        self.next_session_id = next_session;
        self.next_draw_id = next_draw;
        self.split_reservations.insert(
            pane,
            (
                source,
                PaneBinding {
                    workspace: source.workspace,
                    pane,
                    session,
                },
                draw,
            ),
        );
        Ok((
            PaneBinding {
                workspace: source.workspace,
                pane,
                session,
            },
            draw,
        ))
    }
    fn reserve_ids(&mut self) -> Result<(PaneBinding, ExternalDrawId)> {
        let binding = PaneBinding {
            workspace: TabId(self.next_tab_id),
            pane: PaneId(self.next_pane_id),
            session: SessionId(self.next_session_id),
        };
        let draw_id = self.next_draw_id;
        // Compute every successor before publishing any counter.
        let tab = self
            .next_tab_id
            .checked_add(1)
            .context("workspace id space exhausted")?;
        let pane = self
            .next_pane_id
            .checked_add(1)
            .context("pane id space exhausted")?;
        let session = self
            .next_session_id
            .checked_add(1)
            .context("session id space exhausted")?;
        let draw = self
            .next_draw_id
            .checked_add(1)
            .context("external draw id space exhausted")?;
        self.next_tab_id = tab;
        self.next_pane_id = pane;
        self.next_session_id = session;
        self.next_draw_id = draw;
        Ok((binding, draw_id))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod identity_tests;
