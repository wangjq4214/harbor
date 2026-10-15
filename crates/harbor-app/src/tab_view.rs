//! Product-owned tab snapshots, responsive presentation and interaction handles.

use crate::{
    tab_manager::{TabId, TabIndex, TabSnapshot},
    terminal_view::TerminalWidgetBridge,
};
use harbor_widget::{FocusHandle, ScrollController, signal::Signal};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[path = "tab_view_ui.rs"]
pub(crate) mod ui;

pub(crate) const EXPANDED_BREAKPOINT_DP: f32 = 900.0;
const EXPANDED_RAIL_WIDTH: f32 = 200.0;
pub(crate) const COMPACT_RAIL_WIDTH: f32 = 56.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RailPresentation {
    Expanded,
    Compact,
}
impl RailPresentation {
    pub(crate) fn for_logical_width(width: f64) -> Self {
        if width.is_finite() && width >= f64::from(EXPANDED_BREAKPOINT_DP) {
            Self::Expanded
        } else {
            Self::Compact
        }
    }
    fn width(self) -> f32 {
        match self {
            Self::Expanded => EXPANDED_RAIL_WIDTH,
            Self::Compact => COMPACT_RAIL_WIDTH,
        }
    }
}

/// Requests are queued during widget routing and reduced by the Host afterward.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabCommand {
    New,
    Close(TabId),
    CloseActive,
    Activate(TabId),
    Next,
    Previous,
    Numeric(TabIndex),
}

/// Focus disposition attached at the event source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabFocusPolicy {
    PreserveRail,
    RailTab(TabId),
    Terminal,
}

#[derive(Clone)]
pub(crate) struct TabUiState {
    pub(crate) snapshots: Vec<TabSnapshot>,
    pub(crate) active_bridge: Option<TerminalWidgetBridge>,
    pub(crate) presentation: RailPresentation,
}

/// Tab-only projection. Global notices/keybindings and the action inbox belong to the workspace.
#[derive(Clone)]
pub struct TabUiController {
    pub(crate) state: Signal<TabUiState>,
    tab_focus: Arc<Mutex<HashMap<TabId, FocusHandle>>>,
    pub(crate) scroll: ScrollController,
}

impl TabUiController {
    pub fn new(
        snapshots: Vec<TabSnapshot>,
        active_bridge: TerminalWidgetBridge,
        logical_width: f64,
    ) -> Self {
        let tab_focus = snapshots
            .iter()
            .map(|snapshot| (snapshot.id, FocusHandle::new()))
            .collect();
        Self {
            state: Signal::new(TabUiState {
                snapshots,
                active_bridge: Some(active_bridge),
                presentation: RailPresentation::for_logical_width(logical_width),
            }),
            tab_focus: Arc::new(Mutex::new(tab_focus)),
            scroll: ScrollController::new(),
        }
    }

    pub fn sync(
        &self,
        snapshots: Vec<TabSnapshot>,
        active_bridge: Option<TerminalWidgetBridge>,
        logical_width: f64,
    ) {
        if let Ok(mut handles) = self.tab_focus.lock() {
            handles.retain(|id, _| snapshots.iter().any(|snapshot| snapshot.id == *id));
            for snapshot in &snapshots {
                handles.entry(snapshot.id).or_insert_with(FocusHandle::new);
            }
        }
        self.state.set(TabUiState {
            snapshots,
            active_bridge,
            presentation: RailPresentation::for_logical_width(logical_width),
        });
    }

    pub fn update_presentation(&self, logical_width: f64) -> bool {
        let current = self.state.read().clone();
        let presentation = RailPresentation::for_logical_width(logical_width);
        if current.presentation == presentation {
            return false;
        }
        self.state.set(TabUiState {
            presentation,
            ..current
        });
        true
    }

    pub fn tab_focus(&self, id: TabId) -> Option<FocusHandle> {
        self.tab_focus.lock().ok()?.get(&id).copied()
    }
}
