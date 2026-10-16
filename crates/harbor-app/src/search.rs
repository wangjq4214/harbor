//! Application-owned current-session search lifecycle. Cooperative work runs on
//! host turns, never in paint/hover. One job, one completion and one result batch.
use crate::{
    tab_manager::{TabId, TabManager},
    terminal_view::TerminalWidgetBridge,
};
use harbor_terminal::{
    Terminal,
    literal_search::{AcceptedSearch, LiteralSearch, SearchJob, SearchResults, SearchStatus},
    read_model::{ContentVersion, ReadBudget, ReadScope},
};

#[derive(Default)]
pub(crate) struct SessionSearch {
    source: Option<TabId>,
    bridge: Option<TerminalWidgetBridge>,
    query: String,
    gate: LiteralSearch,
    // The last published batch remains actionable while a same-query refresh runs.
    // Actual edits/cancellation invalidate BOTH tokens, never just the worker.
    presented_gate: LiteralSearch,
    job: Option<SearchJob>,
    completion: Option<SearchResults>,
    accepted: Option<AcceptedSearch>,
    observed: Option<ContentVersion>,
    selected: Option<usize>,
    status: String,
}

impl SessionSearch {
    fn cancel(&mut self) {
        self.gate.cancel();
        self.presented_gate.cancel();
        if let Some(bridge) = self.bridge.take() {
            bridge.set_decorations(Vec::new());
        }
        self.job = None;
        self.completion = None;
        self.accepted = None;
        self.source = None;
        self.query.clear();
        self.observed = None;
        self.selected = None;
        self.status.clear();
    }

    fn start(&mut self, terminal: &Terminal) {
        self.observed = Some(terminal.content_version());
        self.job = self
            .gate
            .start(
                terminal,
                &self.query,
                ReadScope::Retained {
                    start_row: 0,
                    row_count: usize::MAX,
                },
                ReadBudget::default(),
            )
            .ok();
        self.status = format!(
            "Searching… — {} validated matches (incomplete coverage)",
            self.accepted.as_ref().map_or(0, |a| a.ranges().len())
        );
    }

    fn accept(&mut self, terminal: &Terminal, completion: SearchResults) -> bool {
        let Ok(accepted) = self.gate.accept(terminal, completion) else {
            self.observed = None;
            self.status = "Content changed; refreshing…".into();
            return false;
        };
        self.presented_gate = std::mem::take(&mut self.gate);
        self.selected = None;
        self.publish(accepted);
        true
    }

    fn publish(&mut self, accepted: AcceptedSearch) {
        let status = accepted.results().status();
        self.status = format!(
            "{} matches — {}",
            accepted.ranges().len(),
            match status {
                SearchStatus::Complete => "complete retained scope",
                SearchStatus::Partial(_) => "partial/limited coverage",
                _ => "incomplete coverage",
            }
        );
        if self.job.is_some() {
            self.status.push_str(" — refreshing…");
        }
        if let Some(bridge) = &self.bridge {
            bridge.set_decorations(accepted.ranges().to_vec());
        }
        self.accepted = Some(accepted);
    }
}

/// Host scheduling result. Pending work requests another event-loop turn, not a
/// search inside the renderer. Changed presentation needs a frame independently.
pub struct SearchTick {
    pub pending: bool,
    pub changed: bool,
}

impl TabManager {
    /// Shared typed host execution path, also exercised by headless routing tests.
    pub fn dispatch_search_command(
        &mut self,
        command: crate::command::AppCommand,
        ui: &crate::workspace::WorkspaceUiController,
    ) -> Option<harbor_widget::FocusHandle> {
        use crate::command::AppCommand;
        match command {
            AppCommand::SearchOpen => {
                if let Some(source) = self.active_id() {
                    ui.open_search(source);
                }
                Some(ui.search_focus())
            }
            AppCommand::SearchClose => {
                self.cancel_search();
                ui.close_search();
                Some(ui.terminal_focus())
            }
            AppCommand::SearchChanged => {
                if let Some((source, query)) = ui.search_query()
                    && self.active_id() == Some(source)
                {
                    self.set_search_query(&query);
                }
                None
            }
            AppCommand::SearchNext | AppCommand::SearchPrevious => {
                self.navigate_search(command == AppCommand::SearchPrevious);
                None
            }
            _ => None,
        }
    }

    pub fn cancel_search(&mut self) {
        self.search.cancel();
    }

    pub fn set_search_query(&mut self, query: &str) {
        self.cancel_search();
        if query.len() > harbor_terminal::literal_search::MAX_QUERY_BYTES {
            self.search.status = "Query exceeds 128 UTF-8 bytes".into();
            return;
        }
        if query.is_empty() {
            self.search.status = "Enter a literal (128 UTF-8 bytes maximum)".into();
            return;
        }
        let (Some(id), Some(terminal), Some(bridge)) = (
            self.active_id(),
            self.active_terminal(),
            self.active_bridge(),
        ) else {
            return;
        };
        let Ok(terminal) = terminal.lock() else {
            return;
        };
        self.search.source = Some(id);
        self.search.bridge = Some(bridge);
        self.search.query = query.into();
        self.search.start(&terminal);
    }

    pub fn search_status(&self) -> &str {
        &self.search.status
    }

    /// Exactly one read/scan page OR one delayed completion acceptance per turn.
    pub fn advance_search(&mut self) -> SearchTick {
        let idle = SearchTick {
            pending: false,
            changed: false,
        };
        let Some(source) = self.search.source else {
            return idle;
        };
        if self.active_id() != Some(source) {
            self.cancel_search();
            return SearchTick {
                pending: false,
                changed: true,
            };
        }
        let Some(terminal) = self.active_terminal() else {
            self.cancel_search();
            return idle;
        };
        let Ok(terminal) = terminal.lock() else {
            return idle;
        };
        let version = terminal.content_version();
        // Refresh displayed evidence before scheduling more work. A resolvable
        // logical position is never sufficient evidence of unchanged text.
        let mut changed = false;
        if self
            .search
            .accepted
            .as_ref()
            .is_some_and(|a| a.results().observed_version() != version)
        {
            let previous = self.search.accepted.take().unwrap();
            match self
                .search
                .presented_gate
                .accept(&terminal, previous.into_results())
            {
                Ok(accepted) => self.search.publish(accepted),
                Err(_) => {
                    self.search.presented_gate.cancel();
                    self.search.observed = None;
                    if let Some(bridge) = &self.search.bridge {
                        bridge.set_decorations(Vec::new());
                    }
                }
            }
            changed = true;
        }
        if let Some(completion) = self.search.completion.take() {
            self.search.accept(&terminal, completion);
            return SearchTick {
                pending: self.search.observed != Some(version),
                changed: true,
            };
        }
        if self.search.job.is_none() && self.search.observed != Some(version) {
            self.search.start(&terminal);
            changed = true;
        }
        if let Some(job) = &mut self.search.job {
            if job.step(&terminal) != SearchStatus::Running {
                self.search.completion = self.search.job.take().map(SearchJob::into_results);
            }
            return SearchTick {
                pending: true,
                changed,
            };
        }
        SearchTick {
            pending: false,
            changed,
        }
    }

    pub fn navigate_search(&mut self, previous: bool) -> bool {
        if self.search.source != self.active_id() || self.search.source.is_none() {
            return false;
        }
        let Some(terminal) = self.active_terminal() else {
            return false;
        };
        let Ok(mut terminal) = terminal.lock() else {
            return false;
        };
        let Some(accepted) = &self.search.accepted else {
            return false;
        };
        let count = accepted.ranges().len();
        if count == 0 {
            return false;
        }
        let index = match (self.search.selected, previous) {
            (None, false) => 0,
            (None, true) => count - 1,
            (Some(index), false) => (index + 1) % count,
            (Some(index), true) => (index + count - 1) % count,
        };
        let target = self
            .search
            .presented_gate
            .target(&terminal, accepted, index);
        if let Some(target) = target
            && terminal.navigate_content_target(&target)
        {
            self.search.selected = Some(index);
            true
        } else {
            self.search.presented_gate.cancel();
            self.search.accepted = None;
            self.search.observed = None;
            if let Some(bridge) = &self.search.bridge {
                bridge.set_decorations(Vec::new());
            }
            false
        }
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
