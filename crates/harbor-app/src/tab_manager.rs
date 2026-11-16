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
            panes: vec![TerminalPane {
                id: binding.pane,
                session: binding.session,
            }],
        });
        self.cancel_search();
        self.clear_active_preedit();
        self.active = Some(binding.workspace);
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
                terminal
                    .lock()
                    .is_ok_and(|terminal| !terminal.is_session_closed())
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
        self.sessions.remove(index);
    }

    pub fn close_active(&mut self) -> TabActionOutcome {
        match self.active {
            Some(id) => self.close(id),
            None => TabActionOutcome::default(),
        }
    }

    /// Drains output for a live tab. Only an active tab asks Runtime to invalidate its mounted
    /// external draw; inactive tabs update model state without exposing a schedule provider.
    pub fn process_output(&mut self, id: SessionId) -> TabOutputOutcome {
        let active = self.active_session_id() == Some(id);
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
                Self::advance_tab_resize(tab, false, &mut outcome);
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
