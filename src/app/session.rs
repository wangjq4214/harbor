//! Multi-session tab management and pane tree view composition.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::terminal_widget_bridge::TerminalWidgetBridge;
use harbor_terminal::Terminal;
use harbor_types::{PaneId, PaneLayoutNode, SessionId, SplitDirection, TabBarPosition};
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::split_container::SplitContainer;
use harbor_widget::widgets::tab_bar::{OnClose, OnNew, OnSelect, TabBar, TabItem};
use harbor_widget::widgets::tabbed_layout::TabbedLayout;

/// Top-level application state managing all terminal sessions and pane trees.
pub struct AppSessionState {
    pub sessions: Vec<harbor_types::TerminalSession>,
    pub active_session: SessionId,
    pub terminals: HashMap<PaneId, Arc<Mutex<Terminal>>>,
    pub gate_active: Arc<AtomicBool>,
    pub tab_scroll_offset: Arc<std::sync::atomic::AtomicU32>,
    next_session_id: u64,
    next_pane_id: u64,
}

impl AppSessionState {
    /// Initializes application state with an initial default single-pane session.
    pub fn new(initial_terminal: Arc<Mutex<Terminal>>, gate_active: Arc<AtomicBool>) -> Self {
        let session_id = SessionId(1);
        let pane_id = PaneId(1);

        let mut terminals = HashMap::new();
        terminals.insert(pane_id, initial_terminal);

        let default_session = harbor_types::TerminalSession::new(session_id, pane_id, "Terminal 1");

        Self {
            sessions: vec![default_session],
            active_session: session_id,
            terminals,
            gate_active,
            tab_scroll_offset: Arc::new(std::sync::atomic::AtomicU32::new(0.0f32.to_bits())),
            next_session_id: 2,
            next_pane_id: 2,
        }
    }

    /// Allocates a new unique `SessionId`.
    fn allocate_session_id(&mut self) -> SessionId {
        let id = SessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }

    /// Allocates a new unique `PaneId`.
    pub fn allocate_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    /// Returns a reference to the currently active session.
    pub fn active_session(&self) -> Option<&harbor_types::TerminalSession> {
        self.sessions.iter().find(|s| s.id == self.active_session)
    }

    /// Returns a mutable reference to the currently active session.
    pub fn active_session_mut(&mut self) -> Option<&mut harbor_types::TerminalSession> {
        self.sessions
            .iter_mut()
            .find(|s| s.id == self.active_session)
    }

    /// Returns the currently focused `PaneId` within the active session.
    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.active_session().map(|s| s.active_pane)
    }

    /// Sets the active pane focus for the active session.
    #[allow(dead_code)]
    pub fn set_active_pane(&mut self, pane_id: PaneId) {
        if let Some(session) = self.active_session_mut() {
            if session.layout.find_leaf(pane_id) {
                session.active_pane = pane_id;
            }
        }
    }

    /// Returns the active `Terminal` instance.
    pub fn active_terminal(&self) -> Option<Arc<Mutex<Terminal>>> {
        let pane_id = self.active_pane_id()?;
        self.terminals.get(&pane_id).cloned()
    }

    /// Creates and activates a new session with the given terminal.
    pub fn new_session(
        &mut self,
        terminal: Arc<Mutex<Terminal>>,
        title: impl Into<String>,
    ) -> SessionId {
        let session_id = self.allocate_session_id();
        let pane_id = self.allocate_pane_id();

        self.terminals.insert(pane_id, terminal);
        let title_str = title.into();
        let display_title = if title_str.is_empty() {
            format!("Terminal {}", session_id.0)
        } else {
            title_str
        };

        let session = harbor_types::TerminalSession::new(session_id, pane_id, display_title);
        self.sessions.push(session);
        self.active_session = session_id;
        session_id
    }

    /// Switches the active session to `target`.
    pub fn switch_session(&mut self, target: SessionId) -> bool {
        if self.sessions.iter().any(|s| s.id == target) {
            self.active_session = target;
            true
        } else {
            false
        }
    }

    /// Switches to the adjacent session (forward or backward).
    pub fn cycle_session(&mut self, forward: bool) {
        if self.sessions.is_empty() {
            return;
        }
        let current_idx = self
            .sessions
            .iter()
            .position(|s| s.id == self.active_session)
            .unwrap_or(0);

        let next_idx = if forward {
            (current_idx + 1) % self.sessions.len()
        } else if current_idx > 0 {
            current_idx - 1
        } else {
            self.sessions.len() - 1
        };

        self.active_session = self.sessions[next_idx].id;
    }

    /// Closes the specified session, tearing down all its panes.
    /// Returns the newly activated `SessionId`, or `None` if no sessions remain.
    pub fn close_session(&mut self, target: SessionId) -> Option<SessionId> {
        let idx = self.sessions.iter().position(|s| s.id == target)?;
        let session = self.sessions.remove(idx);

        // Tear down all terminals belonging to this session
        for pane_id in session.layout.collect_panes() {
            self.terminals.remove(&pane_id);
        }

        if self.sessions.is_empty() {
            return None;
        }

        if self.active_session == target {
            let next_idx = if idx < self.sessions.len() {
                idx
            } else {
                self.sessions.len() - 1
            };
            self.active_session = self.sessions[next_idx].id;
        }

        Some(self.active_session)
    }

    /// Splits the currently active pane in the active session.
    pub fn split_active_pane(
        &mut self,
        new_terminal: Arc<Mutex<Terminal>>,
        direction: SplitDirection,
    ) -> Option<PaneId> {
        let active_pane = self.active_pane_id()?;
        let new_pane_id = self.allocate_pane_id();
        self.terminals.insert(new_pane_id, new_terminal);

        let session = self.active_session_mut()?;
        session
            .layout
            .split_leaf(active_pane, new_pane_id, direction);
        session.active_pane = new_pane_id;
        Some(new_pane_id)
    }

    /// Closes the currently active pane. If it is the last pane in the session, closes the session.
    pub fn close_active_pane(&mut self) -> Option<PaneId> {
        let active_pane = self.active_pane_id()?;

        let (removed, is_session_empty) = {
            let session = self.active_session_mut()?;
            if let PaneLayoutNode::Leaf(id) = session.layout {
                if id == active_pane {
                    (active_pane, true)
                } else {
                    return None;
                }
            } else {
                let removed = session.layout.remove_leaf(active_pane)?;
                session.active_pane = session.layout.first_leaf();
                (removed, false)
            }
        };

        if is_session_empty {
            let target_session = self.active_session;
            self.close_session(target_session);
            Some(removed)
        } else {
            self.terminals.remove(&removed);
            Some(removed)
        }
    }

    /// Recursively builds the View for a `PaneLayoutNode`.
    pub(crate) fn build_pane_node(
        node: &PaneLayoutNode,
        terminals: &HashMap<PaneId, Arc<Mutex<Terminal>>>,
        gate_active: Arc<AtomicBool>,
        cx: &mut BuildCx,
    ) -> View {
        match node {
            PaneLayoutNode::Leaf(pane_id) => {
                if let Some(term) = terminals.get(pane_id) {
                    let bridge = TerminalWidgetBridge::with_draw_id(
                        pane_id.0,
                        Arc::clone(term),
                        gate_active,
                    );
                    bridge.build(cx)
                } else {
                    Column::new().build(cx)
                }
            }
            PaneLayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first_view =
                    Self::build_pane_node(first, terminals, Arc::clone(&gate_active), cx);
                let second_view = Self::build_pane_node(second, terminals, gate_active, cx);
                let split = SplitContainer::new(*direction, *ratio).views(first_view, second_view);
                split.build(cx)
            }
        }
    }

    /// Compiles the complete application root component for the active session and tab bar.
    pub fn build_root_component(&self, tab_position: TabBarPosition) -> SessionRootComponent {
        let tab_items: Vec<TabItem> = self
            .sessions
            .iter()
            .map(|s| TabItem::new(s.id, s.title.clone(), s.id == self.active_session))
            .collect();

        let active_idx = self
            .sessions
            .iter()
            .position(|s| s.id == self.active_session)
            .unwrap_or(0);

        let active_layout = self.active_session().map(|s| s.layout.clone());

        SessionRootComponent {
            tab_position,
            tab_items,
            active_idx,
            active_layout,
            terminals: self.terminals.clone(),
            gate_active: Arc::clone(&self.gate_active),
            tab_scroll_offset: Arc::clone(&self.tab_scroll_offset),
            on_select: None,
            on_close: None,
            on_new: None,
        }
    }
}

/// Root UI component encapsulating the TabBar and active Session pane layout tree.
#[derive(Clone)]
pub struct SessionRootComponent {
    pub tab_position: TabBarPosition,
    pub tab_items: Vec<TabItem>,
    pub active_idx: usize,
    pub active_layout: Option<PaneLayoutNode>,
    pub terminals: HashMap<PaneId, Arc<Mutex<Terminal>>>,
    pub gate_active: Arc<AtomicBool>,
    pub tab_scroll_offset: Arc<std::sync::atomic::AtomicU32>,
    pub on_select: Option<OnSelect>,
    pub on_close: Option<OnClose>,
    pub on_new: Option<OnNew>,
}

impl Component for SessionRootComponent {
    fn build(&self, cx: &mut BuildCx) -> View {
        let mut tab_bar = TabBar::new(self.tab_position, self.tab_items.clone(), self.active_idx)
            .with_scroll_offset(Arc::clone(&self.tab_scroll_offset));
        if let Some(on_select) = &self.on_select {
            let cb = Arc::clone(on_select);
            tab_bar = tab_bar.on_select(move |idx| cb(idx));
        }
        if let Some(on_close) = &self.on_close {
            let cb = Arc::clone(on_close);
            tab_bar = tab_bar.on_close(move |idx| cb(idx));
        }
        if let Some(on_new) = &self.on_new {
            let cb = Arc::clone(on_new);
            tab_bar = tab_bar.on_new(move || cb());
        }
        let tab_bar_view = tab_bar.build(cx);

        let pane_tree_view = if let Some(layout) = &self.active_layout {
            AppSessionState::build_pane_node(
                layout,
                &self.terminals,
                Arc::clone(&self.gate_active),
                cx,
            )
        } else {
            Column::new().build(cx)
        };

        let layout = TabbedLayout::new(self.tab_position, tab_bar_view, pane_tree_view);
        layout.build(cx)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::widgets::custom_paint::CustomPaint;
    use std::sync::atomic::Ordering;

    #[test]
    fn session_state_initial_creation() {
        let gate = Arc::new(AtomicBool::new(false));
        let term = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let state = AppSessionState::new(term, gate);

        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.active_session, SessionId(1));
        assert_eq!(state.active_pane_id(), Some(PaneId(1)));
    }

    #[test]
    fn session_state_add_and_switch_sessions() {
        let gate = Arc::new(AtomicBool::new(false));
        let term1 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let mut state = AppSessionState::new(term1, gate);

        let term2 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let session2 = state.new_session(term2, "Tab 2");
        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.active_session, session2);

        state.switch_session(SessionId(1));
        assert_eq!(state.active_session, SessionId(1));

        state.cycle_session(true);
        assert_eq!(state.active_session, session2);
    }

    #[test]
    fn session_state_split_and_close_panes() {
        let gate = Arc::new(AtomicBool::new(false));
        let term1 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let mut state = AppSessionState::new(term1, gate);

        let term2 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let new_pane = state
            .split_active_pane(term2, SplitDirection::Horizontal)
            .expect("split pane");
        assert_eq!(new_pane, PaneId(2));
        assert_eq!(state.active_pane_id(), Some(PaneId(2)));

        // Close pane 2 -> pane 1 becomes active
        let closed = state.close_active_pane();
        assert_eq!(closed, Some(PaneId(2)));
        assert_eq!(state.active_pane_id(), Some(PaneId(1)));
    }

    #[test]
    fn session_state_close_session_removes_its_panes_and_promotes_remaining_session() {
        let gate = Arc::new(AtomicBool::new(false));
        let term1 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let mut state = AppSessionState::new(term1, Arc::clone(&gate));

        let term2 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let session2 = state.new_session(term2, "Second");
        let extra_pane = state
            .split_active_pane(
                Arc::new(Mutex::new(Terminal::new_headless(24, 80))),
                SplitDirection::Horizontal,
            )
            .expect("split pane");

        let active = state.close_session(session2);
        assert_eq!(active, Some(SessionId(1)));
        assert_eq!(state.active_session, SessionId(1));
        assert_eq!(state.sessions.len(), 1);
        assert!(!state.terminals.contains_key(&PaneId(2)));
        assert!(!state.terminals.contains_key(&extra_pane));
        assert!(state.terminals.contains_key(&PaneId(1)));
    }

    #[test]
    fn session_state_close_active_pane_promotes_the_sibling_leaf() {
        let gate = Arc::new(AtomicBool::new(false));
        let term = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let mut state = AppSessionState::new(term, gate);

        let pane2 = state
            .split_active_pane(
                Arc::new(Mutex::new(Terminal::new_headless(24, 80))),
                SplitDirection::Horizontal,
            )
            .expect("split pane");
        state.set_active_pane(PaneId(1));

        let removed = state.close_active_pane();
        assert_eq!(removed, Some(PaneId(1)));
        assert_eq!(state.active_pane_id(), Some(pane2));
        assert_eq!(
            state.active_session().map(|session| &session.layout),
            Some(&PaneLayoutNode::leaf(pane2))
        );
        assert!(!state.terminals.contains_key(&PaneId(1)));
        assert!(state.terminals.contains_key(&pane2));
    }

    #[test]
    fn session_state_builds_views_for_leaf_and_split_panes() {
        let gate = Arc::new(AtomicBool::new(false));
        let term = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let mut state = AppSessionState::new(term, Arc::clone(&gate));
        let mut cx = BuildCx::stub();

        let leaf_view = AppSessionState::build_pane_node(
            &state.active_session().expect("active session").layout,
            &state.terminals,
            Arc::clone(&state.gate_active),
            &mut cx,
        );
        assert_eq!(
            leaf_view.widget_type(),
            std::any::TypeId::of::<CustomPaint>()
        );

        state
            .split_active_pane(
                Arc::new(Mutex::new(Terminal::new_headless(24, 80))),
                SplitDirection::Horizontal,
            )
            .expect("split pane");
        let split_view = AppSessionState::build_pane_node(
            &state.active_session().expect("active session").layout,
            &state.terminals,
            Arc::clone(&state.gate_active),
            &mut cx,
        );
        assert_eq!(
            split_view.widget_type(),
            std::any::TypeId::of::<SplitContainer>()
        );
    }

    #[test]
    fn session_root_component_binds_callbacks_and_builds_left_tabbar() {
        let gate = Arc::new(AtomicBool::new(false));
        let term = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let state = AppSessionState::new(term, gate);

        let selected = Arc::new(std::sync::atomic::AtomicUsize::new(usize::MAX));
        let sel_clone = Arc::clone(&selected);

        let mut root_comp = state.build_root_component(TabBarPosition::Left);
        root_comp.on_select = Some(Arc::new(move |idx| {
            sel_clone.store(idx, Ordering::Relaxed);
        }));

        let mut cx = BuildCx::stub();
        let view = root_comp.build(&mut cx);
        assert_eq!(view.widget_type(), std::any::TypeId::of::<TabbedLayout>());

        if let Some(cb) = &root_comp.on_select {
            cb(0);
        }
        assert_eq!(selected.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn session_root_new_session_updates_tabs_and_preserves_sidebar_layout() {
        let gate = Arc::new(AtomicBool::new(false));
        let term1 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let mut state = AppSessionState::new(term1, gate);

        let term2 = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let session2 = state.new_session(term2, "Tab 2");
        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.active_session, session2);

        let root_comp = state.build_root_component(TabBarPosition::Left);
        assert_eq!(root_comp.tab_items.len(), 2);
        assert_eq!(root_comp.active_idx, 1);

        let mut cx = BuildCx::stub();
        let view = root_comp.build(&mut cx);
        assert_eq!(view.widget_type(), std::any::TypeId::of::<TabbedLayout>());

        let mut runtime = harbor_widget::runtime::Runtime::new();
        runtime.set_root(root_comp);
        let effects = runtime.update(std::time::Instant::now());
        assert!(effects.request_redraw);
    }
}
