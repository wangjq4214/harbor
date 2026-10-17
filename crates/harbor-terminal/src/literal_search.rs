//! Cooperative, headless literal search over the bounded retained-content API.
//!
//! `LiteralSearch` is a query/lifetime gate, not a scheduler or session owner.
//! Call `start` for requested work, `SearchJob::step` outside painting, `accept`
//! before publication and `target` again before navigation. Hosts must cancel on
//! actual session close/replacement; shell EOF alone is not a cancellation signal.
//! Store `AcceptedSearch` plus an index, not a bare `ActionTarget`, for later
//! search activation: the neutral target alone does not carry query authorization.
//!
//! Matching is exact, case-sensitive UTF-8 (including overlapping occurrences).
//! Byte occurrences expand outward to complete shared text units; identical
//! expanded ranges are deduplicated. No normalization, regex or source mutation.
//! Soft wraps/pages join; hard breaks, including empty lines, reset the matcher.
//!
//! Limits: 128 query bytes, 1024 result ranges, one `ReadBudget` page per step
//! (default and hard maxima are those of `read_model`). Streaming KMP retains no
//! history text, only a <=128-byte pattern/prefix table and <=129 one-unit proofs
//! (128 at step boundaries). Each result has <=128 row proofs. Joining a match
//! performs <=128 joins, each copying <=128 proofs. Scanning performs <=2 byte
//! comparisons per input byte amortized; counters expose actual work. Cancellation
//! is checked before/after the bounded read, every unit and every 256 scanned
//! bytes. A read/individual bounded range operation is not interruptible. Cancel
//! marks an atomic token immediately; a delayed job releases its bounded resources
//! on its next step or drop. No worker, queue, result archive or retry is retained.
//! These are per-owned-job/result bounds, not permission for an unbounded host
//! queue: a host must drop obsolete jobs/completions instead of archiving them.
//! The suffix deque's allocated capacity is <=256 records (<=129 live); results
//! retain <=1024 records. Allocator bookkeeping is not counted as text payload.
//!
//! Continuations capture a fixed scope end. After output, explicitly rebase the
//! checkpoint and refresh suffix evidence; incompatible content makes work stale,
//! never silently joins revisions. Untouched history can therefore make forward
//! progress under output. Any revision change makes negative coverage partial,
//! even when positive ranges survive validation. Complete means the selected scope
//! at one unchanged revision, not all future output or all history. Stale work
//! needs a fresh explicitly requested job; no hidden full-history restart.
//!
//! ```
//! use harbor_terminal::{Terminal, literal_search::{LiteralSearch, SearchStatus},
//!     read_model::{ReadBudget, ReadScope}};
//! let mut terminal = Terminal::new_headless(2, 4);
//! terminal.put_str("abcdef");
//! let mut search = LiteralSearch::default();
//! let mut job = search.start(&terminal, "cde",
//!     ReadScope::Retained { start_row: 0, row_count: usize::MAX },
//!     ReadBudget::default()).unwrap();
//! while job.step(&terminal) == SearchStatus::Running {}
//! let results = search.accept(&terminal, job.into_results()).unwrap();
//! assert_eq!(results.ranges().len(), 1);
//! assert!(search.target(&terminal, &results, 0).is_some());
//! search.cancel();
//! assert!(search.target(&terminal, &results, 0).is_none());
//! ```

use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::Terminal;
use crate::read_model::{
    ActionTarget, ContentRange, ContentVersion, FragmentEnd, MAX_READ_PAYLOAD_BYTES, MAX_READ_ROWS,
    MAX_READ_UNITS, ReadBudget, ReadContinuation, ReadError, ReadScope, TargetAction,
    fresh_identity,
};

pub const MAX_QUERY_BYTES: usize = MAX_READ_ROWS;
pub const MAX_SEARCH_RESULTS: usize = 1024;
pub const CANCEL_SCAN_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchProvenance {
    pub query: QueryId,
    /// Original consistency point, including engine/session, buffer and generation.
    pub content: ContentVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryError {
    Empty,
    TooLong,
    InvalidBudget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartialReason {
    ContentChanged,
    ResultLimit,
    Read(ReadError),
    /// Exhausted the stream's byte-position representation; never wrap offsets.
    PositionLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchStatus {
    Running,
    Complete,
    Partial(PartialReason),
    Stale,
    Cancelled,
}

/// Per-step work, not cumulative history coverage. Copy/row counts describe
/// successful pages; a failed boundary read exposes no page counters, but still
/// obeys the submitted hard budget. `read_calls` includes that failed attempt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchWork {
    /// At most one read (strict or explicitly rebased) per step.
    pub read_calls: usize,
    pub rows_visited: usize,
    pub copied_units: usize,
    pub copied_payload_bytes: usize,
    pub scanned_bytes: usize,
    pub comparisons: usize,
    pub range_refreshes: usize,
    pub range_joins: usize,
}

/// Owned completion or partial progress. Fields cannot be forged into provenance.
/// This is NOT actionable until accepted, nor is acceptance perpetual validity.
#[derive(Debug)]
pub struct SearchResults {
    provenance: SearchProvenance,
    observed: ContentVersion,
    scope: ReadScope,
    selected_rows: Option<usize>,
    scope_clipped: bool,
    status: SearchStatus,
    ranges: Vec<ContentRange>,
}

impl SearchResults {
    pub fn provenance(&self) -> SearchProvenance {
        self.provenance
    }
    pub fn observed_version(&self) -> ContentVersion {
        self.observed
    }
    pub fn scope(&self) -> ReadScope {
        self.scope
    }
    pub fn selected_rows(&self) -> Option<usize> {
        self.selected_rows
    }
    pub fn scope_clipped(&self) -> bool {
        self.scope_clipped
    }
    pub fn status(&self) -> SearchStatus {
        self.status
    }
    pub fn ranges(&self) -> &[ContentRange] {
        &self.ranges
    }
}

/// Prepared presentation data. Revalidate at every later publication/navigation;
/// callers must never treat a stored range or this wrapper as a permanent lease.
#[derive(Debug)]
pub struct AcceptedSearch(SearchResults);

impl AcceptedSearch {
    pub fn results(&self) -> &SearchResults {
        &self.0
    }
    pub fn ranges(&self) -> &[ContentRange] {
        self.0.ranges()
    }
    /// Consumes the wrapper for re-publication through `LiteralSearch::accept`.
    pub fn into_results(self) -> SearchResults {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rejection {
    ObsoleteQuery,
    WrongContent,
    StaleWork,
    StaleText,
}

struct CurrentQuery {
    provenance: SearchProvenance,
    cancelled: Arc<AtomicBool>,
}

/// Retains one current ID/token, not a query string or prior jobs/results.
#[derive(Default)]
pub struct LiteralSearch {
    current: Option<CurrentQuery>,
}

fn same_generation(a: ContentVersion, b: ContentVersion) -> bool {
    a.session == b.session && a.buffer == b.buffer && a.generation == b.generation
}

impl LiteralSearch {
    /// Invalid input leaves the current query intact. Starting valid work cancels
    /// the previous query; the returned job can be moved, but is not cloneable.
    pub fn start(
        &mut self,
        terminal: &Terminal,
        query: &str,
        scope: ReadScope,
        budget: ReadBudget,
    ) -> Result<SearchJob, QueryError> {
        if query.is_empty() {
            return Err(QueryError::Empty);
        }
        if query.len() > MAX_QUERY_BYTES {
            return Err(QueryError::TooLong);
        }
        if budget.rows == 0
            || budget.rows > MAX_READ_ROWS
            || budget.units == 0
            || budget.units > MAX_READ_UNITS
            || budget.payload_bytes == 0
            || budget.payload_bytes > MAX_READ_PAYLOAD_BYTES
        {
            return Err(QueryError::InvalidBudget);
        }
        let pattern = query.as_bytes().to_vec();
        let mut prefix = vec![0; pattern.len()];
        let mut matched = 0;
        for i in 1..pattern.len() {
            while matched > 0 && pattern[i] != pattern[matched] {
                matched = prefix[matched - 1];
            }
            if pattern[i] == pattern[matched] {
                matched += 1;
            }
            prefix[i] = matched;
        }
        self.cancel();
        let provenance = SearchProvenance {
            query: QueryId(fresh_identity()),
            content: terminal.content_version(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.current = Some(CurrentQuery {
            provenance,
            cancelled: cancelled.clone(),
        });
        Ok(SearchJob {
            cancelled,
            budget,
            work: SearchWork::default(),
            results: SearchResults {
                provenance,
                observed: provenance.content,
                scope,
                selected_rows: None,
                scope_clipped: false,
                status: SearchStatus::Running,
                ranges: Vec::new(),
            },
            scan: Some(ScanState {
                pattern,
                prefix,
                matched: 0,
                bytes: 0,
                recent: VecDeque::new(),
                continuation: None,
            }),
        })
    }

    /// Also use for session close/replacement. Does not infer shell/process EOF.
    pub fn cancel(&mut self) {
        if let Some(old) = self.current.take() {
            old.cancelled.store(true, Ordering::Release);
        }
    }

    fn eligible(&self, terminal: &Terminal, results: &SearchResults) -> Result<(), Rejection> {
        let current = self.current.as_ref().ok_or(Rejection::ObsoleteQuery)?;
        if current.provenance != results.provenance {
            return Err(Rejection::ObsoleteQuery);
        }
        if !same_generation(results.provenance.content, terminal.content_version()) {
            return Err(Rejection::WrongContent);
        }
        if matches!(
            results.status,
            SearchStatus::Stale | SearchStatus::Cancelled
        ) {
            return Err(Rejection::StaleWork);
        }
        Ok(())
    }

    /// Bounded by 1024 ranges * 128 row proofs, with no source decoding. A single
    /// invalid range rejects the batch; callers may request fresh bounded work.
    /// Unchanged positive ranges may survive unrelated output, but old negative
    /// coverage can never be promoted to complete at a new revision.
    pub fn accept(
        &self,
        terminal: &Terminal,
        mut results: SearchResults,
    ) -> Result<AcceptedSearch, Rejection> {
        self.eligible(terminal, &results)?;
        for range in &mut results.ranges {
            *range = terminal
                .refresh_content_range(range)
                .ok_or(Rejection::StaleText)?;
        }
        results.observed = terminal.content_version();
        if results.status == SearchStatus::Complete
            && results.observed != results.provenance.content
        {
            results.status = SearchStatus::Partial(PartialReason::ContentChanged);
        }
        Ok(AcceptedSearch(results))
    }

    /// Neutral navigation data only; never moves a cursor, opens or executes.
    /// Checks live query/session/buffer/generation and text again, not just anchors.
    pub fn target(
        &self,
        terminal: &Terminal,
        results: &AcceptedSearch,
        index: usize,
    ) -> Option<ActionTarget> {
        self.eligible(terminal, &results.0).ok()?;
        let range = terminal.refresh_content_range(results.ranges().get(index)?)?;
        Some(ActionTarget {
            range,
            action: TargetAction::Navigate,
        })
    }
}

impl Drop for LiteralSearch {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct RecentUnit {
    end_byte: u64,
    range: ContentRange,
}

struct ScanState {
    pattern: Vec<u8>,
    prefix: Vec<usize>,
    matched: usize,
    bytes: u64,
    recent: VecDeque<RecentUnit>,
    continuation: Option<ReadContinuation>,
}

/// No Terminal reference, full-page cache or full-history copy is retained.
/// `step` does one page; dropping is immediate bounded resource reclamation.
pub struct SearchJob {
    cancelled: Arc<AtomicBool>,
    budget: ReadBudget,
    scan: Option<ScanState>,
    results: SearchResults,
    work: SearchWork,
}

impl SearchJob {
    pub fn status(&self) -> SearchStatus {
        self.results.status
    }
    pub fn last_work(&self) -> SearchWork {
        self.work
    }

    fn stop(&mut self, status: SearchStatus) -> SearchStatus {
        self.results.status = status;
        self.scan = None;
        if matches!(status, SearchStatus::Cancelled | SearchStatus::Stale) {
            self.results.ranges.clear();
            self.results.ranges.shrink_to_fit();
        }
        status
    }

    /// Consumes the job, releasing pattern/suffix/continuation. Running results
    /// explicitly retain Running coverage; they are never complete no-match.
    pub fn into_results(mut self) -> SearchResults {
        if self.cancelled.load(Ordering::Acquire) {
            self.stop(SearchStatus::Cancelled);
        }
        self.results
    }

    pub fn step(&mut self, terminal: &Terminal) -> SearchStatus {
        let token = self.cancelled.clone();
        self.step_inner(terminal, || token.load(Ordering::Acquire))
    }

    // Injectable observation makes cancellation during a synchronous page/scan
    // deterministic in tests; production always observes the shared atomic token.
    fn step_inner(
        &mut self,
        terminal: &Terminal,
        mut cancelled: impl FnMut() -> bool,
    ) -> SearchStatus {
        self.work = SearchWork::default();
        if cancelled() {
            return self.stop(SearchStatus::Cancelled);
        }
        if !same_generation(self.results.provenance.content, terminal.content_version()) {
            return self.stop(SearchStatus::Stale);
        }
        if self.status() != SearchStatus::Running {
            return self.status();
        }
        let scan = self.scan.as_mut().expect("running scan");
        if self.results.observed != terminal.content_version() {
            // Every retained suffix proof must survive before cross-page matching.
            for unit in &mut scan.recent {
                self.work.range_refreshes += 1;
                let Some(range) = terminal.refresh_content_range(&unit.range) else {
                    return self.stop(SearchStatus::Stale);
                };
                unit.range = range;
            }
            if let Some(cursor) = &scan.continuation {
                let Ok(rebased) = terminal.rebase_content_read(cursor) else {
                    return self.stop(SearchStatus::Stale);
                };
                scan.continuation = Some(rebased);
            }
        }
        self.results.observed = terminal.content_version();
        self.work.read_calls = 1;
        let page = match &scan.continuation {
            Some(cursor) => terminal.continue_content_read(cursor, self.budget),
            None => terminal.read_content(self.results.scope, self.budget),
        };
        let page = match page {
            Ok(page) => page,
            Err(ReadError::StaleContinuation) => return self.stop(SearchStatus::Stale),
            Err(error) => return self.stop(SearchStatus::Partial(PartialReason::Read(error))),
        };
        self.work.rows_visited = page.rows_visited;
        self.work.copied_units = page.copied_units;
        self.work.copied_payload_bytes = page.copied_payload_bytes;
        self.results.selected_rows = Some(page.selected_rows);
        self.results.scope_clipped = page.scope_clipped;
        if cancelled() {
            return self.stop(SearchStatus::Cancelled);
        }
        let scan = self.scan.as_mut().expect("running scan");
        for fragment in &page.fragments {
            // Guard logical adjacency as well as version proofs. Empty fragments
            // do not invent searchable characters or a missing retained prefix.
            if let Some(previous) = scan.recent.back()
                && (previous.range.end().line != fragment.line()
                    || previous.range.end().offset != fragment.atom_start())
            {
                scan.reset_line();
            }
            for (i, unit) in fragment.units().iter().enumerate() {
                if cancelled() {
                    return self.stop(SearchStatus::Cancelled);
                }
                let Some(end_byte) = scan.bytes.checked_add(unit.text.len() as u64) else {
                    return self.stop(SearchStatus::Partial(PartialReason::PositionLimit));
                };
                scan.recent.push_back(RecentUnit {
                    end_byte,
                    range: fragment.range(i..i + 1).expect("complete read unit"),
                });
                for byte in unit.text.bytes() {
                    if self.work.scanned_bytes.is_multiple_of(CANCEL_SCAN_BYTES) && cancelled() {
                        return self.stop(SearchStatus::Cancelled);
                    }
                    self.work.scanned_bytes += 1;
                    scan.bytes += 1;
                    let start = scan.bytes.saturating_sub(scan.pattern.len() as u64);
                    while scan
                        .recent
                        .front()
                        .is_some_and(|unit| unit.end_byte <= start)
                    {
                        scan.recent.pop_front();
                    }
                    loop {
                        self.work.comparisons += 1;
                        if byte == scan.pattern[scan.matched] {
                            scan.matched += 1;
                            break;
                        }
                        if scan.matched == 0 {
                            break;
                        }
                        scan.matched = scan.prefix[scan.matched - 1];
                    }
                    if scan.matched == scan.pattern.len() {
                        let mut units = scan.recent.iter();
                        let mut range = units.next().expect("matched source").range.clone();
                        for unit in units {
                            self.work.range_joins += 1;
                            range = range
                                .join(&unit.range)
                                .expect("adjacent bounded unit proofs");
                        }
                        if !self.results.ranges.last().is_some_and(|last| {
                            last.start() == range.start() && last.end() == range.end()
                        }) {
                            self.results.ranges.push(range);
                            if self.results.ranges.len() == MAX_SEARCH_RESULTS {
                                return self
                                    .stop(SearchStatus::Partial(PartialReason::ResultLimit));
                            }
                        }
                        scan.matched = scan.prefix[scan.matched - 1];
                    }
                }
            }
            if matches!(
                fragment.end(),
                FragmentEnd::HardBreak | FragmentEnd::RetainedEnd
            ) {
                scan.reset_line();
            }
        }
        scan.continuation = page.continuation;
        if cancelled() {
            return self.stop(SearchStatus::Cancelled);
        }
        if self
            .scan
            .as_ref()
            .expect("running scan")
            .continuation
            .is_none()
        {
            let status = if self.results.observed == self.results.provenance.content {
                SearchStatus::Complete
            } else {
                SearchStatus::Partial(PartialReason::ContentChanged)
            };
            return self.stop(status);
        }
        self.status()
    }
}

impl ScanState {
    fn reset_line(&mut self) {
        self.matched = 0;
        self.bytes = 0;
        self.recent.clear();
    }
}

#[cfg(test)]
mod tests;
