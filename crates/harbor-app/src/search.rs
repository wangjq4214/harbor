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
use std::time::{Duration, Instant};

const SEARCH_BUSY_DELAY: Duration = Duration::from_millis(200);

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
    // Presentation delay only: matching still starts immediately.
    busy_since: Option<Instant>,
    status: String,
}

impl SessionSearch {
    fn update_status(&mut self) {
        self.update_status_at(Instant::now());
    }

    fn update_status_at(&mut self, now: Instant) -> bool {
        let searching = self.job.is_some()
            || self.completion.is_some()
            || (self.source.is_some() && self.observed.is_none());
        let show_busy = if searching {
            let since = *self.busy_since.get_or_insert(now);
            now.saturating_duration_since(since) >= SEARCH_BUSY_DELAY
        } else {
            self.busy_since = None;
            false
        };
        let count = self.accepted.as_ref().map_or(0, |a| a.ranges().len());
        let position = if count == 0 {
            "No Result".into()
        } else {
            let current = self
                .selected
                .map_or_else(|| "—".into(), |i| (i + 1).to_string());
            format!("{current} / {count}")
        };
        let complete = self
            .accepted
            .as_ref()
            .is_some_and(|a| a.results().status() == SearchStatus::Complete);
        let status = if show_busy {
            if count > 0 {
                format!("Searching… · {position}")
            } else {
                "Searching…".into()
            }
        } else if (self.accepted.is_some() && !complete) || (searching && count > 0) {
            // Keep limited coverage explicit, even before the busy hint is shown.
            if count > 0 {
                format!("partial/limited · {position}")
            } else {
                "partial/limited · 0".into()
            }
        } else {
            // Keep the initial label until a result or the delayed busy hint replaces it.
            // Pending work remains distinct from completed negative coverage internally.
            position
        };
        if self.status == status {
            return false;
        }
        self.status = status;
        true
    }

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
        self.busy_since = None;
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
        self.update_status();
    }

    fn accept(&mut self, terminal: &Terminal, completion: SearchResults) -> bool {
        let Ok(accepted) = self.gate.accept(terminal, completion) else {
            self.observed = None;
            self.update_status();
            return false;
        };
        self.presented_gate = std::mem::take(&mut self.gate);
        self.selected = None;
        self.publish(accepted);
        true
    }

    fn publish(&mut self, accepted: AcceptedSearch) {
        self.selected = self
            .selected
            .filter(|&index| index < accepted.ranges().len());
        if let Some(bridge) = &self.bridge {
            bridge.set_decorations(accepted.ranges().to_vec());
        }
        self.accepted = Some(accepted);
        self.update_status();
    }
}

/// Host scheduling result. Pending work requests another event-loop turn, not a
/// search inside the renderer. Changed presentation needs a frame independently.
pub struct SearchTick {
    pub pending: bool,
    pub changed: bool,
}

pub struct SearchCommandOutcome {
    pub focus: Option<harbor_widget::FocusHandle>,
    pub completion: Result<crate::command::CommandCompletion, &'static str>,
}

impl SearchCommandOutcome {
    /// Direct match navigation preserves focus; palette teardown needs a live destination.
    pub fn focus_for_source(
        &self,
        source: crate::command::CommandSource,
        ui: &crate::workspace::WorkspaceUiController,
    ) -> Option<harbor_widget::FocusHandle> {
        self.focus.or_else(|| match source {
            crate::command::CommandSource::Direct => None,
            crate::command::CommandSource::Palette { .. } => Some(if ui.search_query().is_some() {
                ui.search_focus()
            } else {
                ui.terminal_focus()
            }),
        })
    }
}

impl TabManager {
    /// Shared typed host execution path, also exercised by headless routing tests.
    pub fn dispatch_search_command(
        &mut self,
        command: crate::command::AppCommand,
        ui: &crate::workspace::WorkspaceUiController,
    ) -> Option<harbor_widget::FocusHandle> {
        self.execute_search_command(command, ui).focus
    }

    /// Completion records actual reduction rather than merely dispatching a request.
    pub fn execute_search_command(
        &mut self,
        command: crate::command::AppCommand,
        ui: &crate::workspace::WorkspaceUiController,
    ) -> SearchCommandOutcome {
        use crate::command::{AppCommand, CommandCompletion};
        let (focus, completion) = match command {
            AppCommand::SearchOpen => {
                let Some(source) = self.active_id() else {
                    return SearchCommandOutcome {
                        focus: None,
                        completion: Err("No active terminal"),
                    };
                };
                ui.open_search(source);
                (Some(ui.search_focus()), CommandCompletion::Applied)
            }
            AppCommand::SearchClose => {
                let open = ui.search_query().is_some();
                self.cancel_search();
                ui.close_search();
                (
                    Some(ui.terminal_focus()),
                    if open {
                        CommandCompletion::Applied
                    } else {
                        CommandCompletion::Unchanged
                    },
                )
            }
            AppCommand::SearchChanged => {
                if let Some((source, query)) = ui.search_query()
                    && self.active_id() == Some(source)
                {
                    self.set_search_query(&query);
                }
                (None, CommandCompletion::Applied)
            }
            AppCommand::SearchNext | AppCommand::SearchPrevious => {
                let changed = self.navigate_search(command == AppCommand::SearchPrevious);
                ui.publish_search_status(self.search_status());
                (
                    None,
                    if changed {
                        CommandCompletion::Applied
                    } else {
                        CommandCompletion::Unchanged
                    },
                )
            }
            _ => {
                return SearchCommandOutcome {
                    focus: None,
                    completion: Err("Not a search operation"),
                };
            }
        };
        SearchCommandOutcome {
            focus,
            completion: Ok(completion),
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
            self.search.status = "No Result".into();
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
        if self.search.status.is_empty() {
            "No Result"
        } else {
            &self.search.status
        }
    }

    /// Exactly one read/scan page OR one delayed completion acceptance per turn.
    pub fn advance_search(&mut self) -> SearchTick {
        self.advance_search_at(Instant::now())
    }

    fn advance_search_at(&mut self, now: Instant) -> SearchTick {
        let mut tick = self.advance_search_work();
        // Pending work already polls host turns. Reveal a slow-search hint there,
        // not via an input debounce, a sleeping worker or an idle redraw timer.
        if self.search.source.is_some() {
            tick.changed |= self.search.update_status_at(now);
        }
        tick
    }

    fn advance_search_work(&mut self) -> SearchTick {
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
                    self.search.selected = None;
                    self.search.update_status();
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
            self.search.update_status();
            true
        } else {
            self.search.presented_gate.cancel();
            self.search.accepted = None;
            self.search.observed = None;
            self.search.selected = None;
            self.search.update_status();
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
