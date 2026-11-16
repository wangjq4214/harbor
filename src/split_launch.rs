//! Split-only launch transaction, owned by production pane command admission.
//! Host owns one operation through claim/cancel; layout attachment is atomic with claim.

use harbor_app::tab_manager::{PaneBinding, TabManager, TerminalTabResources};
use harbor_pty::{ShellCommand, ShutdownReceipt};
use harbor_widget::scene::primitive::ExternalDrawId;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);
static NEXT_ATTEMPT: AtomicU64 = AtomicU64::new(1);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const PENDING: u8 = 0;
const READY: u8 = 1;
const FAILED: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperationId(u64);

#[derive(Clone, Copy)]
struct AttemptId(u64);

/// A distinct observer belongs to each attempt, not to the destination pane or active tab.
#[derive(Clone)]
pub(crate) struct StartupProbe {
    ready: Vec<u8>,
    failed: Vec<u8>,
    status: Arc<AtomicU8>,
    ready_end: Arc<AtomicU64>,
}
impl StartupProbe {
    fn new(operation: OperationId) -> anyhow::Result<Self> {
        #[allow(deprecated)] // Checked allocation on older stable toolchains too.
        let attempt = NEXT_ATTEMPT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map(AttemptId)
            .map_err(|_| anyhow::anyhow!("split attempt identity exhausted"))?;
        let marker = |state| {
            format!(
                "\x1b]9001;harbor;split;{};{};{state}\x07",
                operation.0, attempt.0
            )
            .into_bytes()
        };
        Ok(Self {
            ready: marker("ready"),
            failed: marker("failed"),
            status: Arc::new(AtomicU8::new(PENDING)),
            ready_end: Arc::new(AtomicU64::new(u64::MAX)),
        })
    }
    /// Bounded streaming matcher; no terminal contents or runtime environment are retained.
    pub(crate) fn observer(&self) -> impl FnMut(&[u8]) + Send + 'static {
        let probe = self.clone();
        let mut tail = Vec::new();
        let mut observed_bytes = 0_u64;
        move |bytes| {
            if probe.status.load(Ordering::Acquire) != PENDING {
                return;
            }
            if bytes.is_empty() {
                probe.status.store(FAILED, Ordering::Release);
                return;
            }
            for &byte in bytes {
                observed_bytes = observed_bytes.saturating_add(1);
                tail.push(byte);
                if tail.ends_with(&probe.ready) {
                    probe.ready_end.store(observed_bytes, Ordering::Relaxed);
                    probe.status.store(READY, Ordering::Release);
                    return;
                }
                if tail.ends_with(&probe.failed) {
                    probe.status.store(FAILED, Ordering::Release);
                    return;
                }
                if tail.len() >= probe.ready.len().max(probe.failed.len()) {
                    tail.remove(0);
                }
            }
        }
    }
    fn command(&self, recipe: &harbor_config::ProjectRecipe) -> anyhow::Result<ShellCommand> {
        crate::profile_launch::project::split_command(
            recipe,
            std::str::from_utf8(&self.ready)?,
            std::str::from_utf8(&self.failed)?,
        )
    }
}

/// Native shutdown is asynchronous. No replacement is even spawned before this receipt.
pub(crate) enum Cleanup {
    Native(ShutdownReceipt),
    #[cfg(test)]
    Controlled(Arc<AtomicBool>),
}
impl Cleanup {
    fn complete(&self) -> bool {
        match self {
            Self::Native(receipt) => receipt.is_complete(),
            #[cfg(test)]
            Self::Controlled(done) => done.load(Ordering::Acquire),
        }
    }
}

/// Factory must retain the shutdown receipt even when resource construction fails after spawn.
pub(crate) struct AttemptLaunch {
    pub(crate) resources: anyhow::Result<TerminalTabResources>,
    pub(crate) cleanup: Option<Cleanup>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaunchKind {
    Inherited,
    DefaultUnknownContext,
    DefaultStartupFailure,
}

enum Next {
    Fallback,
    Cancelled,
    Failed(anyhow::Error),
}
enum State {
    Idle,
    Waiting {
        resources: Box<TerminalTabResources>,
        probe: StartupProbe,
        deadline: Instant,
        cleanup: Option<Cleanup>,
    },
    Cleaning {
        cleanup: Option<Cleanup>,
        next: Next,
    },
    Done,
}

pub(crate) struct SplitOperation {
    id: OperationId,
    source: PaneBinding,
    target: PaneBinding,
    draw: ExternalDrawId,
    context: Option<harbor_config::ProjectRecipe>,
    default: ShellCommand,
    live: Arc<AtomicBool>,
    state: State,
    metadata: Vec<harbor_terminal::TerminalOutputEvent>,
}

pub(crate) enum SplitPoll {
    Pending,
    Ready(Box<SplitReady>),
    Failed(anyhow::Error),
    Cancelled,
    Finished,
}

/// One-shot completion; consuming it revalidates operation + complete source binding.
/// Call claim and attach in the same Host turn; never substitute the active workspace.
pub(crate) struct SplitReady {
    id: OperationId,
    source: PaneBinding,
    target: PaneBinding,
    draw: ExternalDrawId,
    live: Arc<AtomicBool>,
    kind: LaunchKind,
    metadata: Vec<harbor_terminal::TerminalOutputEvent>,
    resources: TerminalTabResources,
}
pub(crate) struct PreparedSplit {
    pub(crate) operation: OperationId,
    pub(crate) source: PaneBinding,
    pub(crate) target: PaneBinding,
    pub(crate) draw: ExternalDrawId,
    pub(crate) kind: LaunchKind,
    /// At most the latest title/cwd/shell-marker observations, to seed attachment.
    /// Project recognition is retained in the engine; pre-publication clipboard is discarded.
    pub(crate) metadata: Vec<harbor_terminal::TerminalOutputEvent>,
    pub(crate) resources: TerminalTabResources,
}
impl SplitReady {
    pub(crate) fn claim(
        self,
        tabs: &TabManager,
        expected: OperationId,
    ) -> anyhow::Result<PreparedSplit> {
        anyhow::ensure!(
            self.id == expected && source_live(tabs, self.source),
            "split completion is stale"
        );
        anyhow::ensure!(
            self.live.swap(false, Ordering::AcqRel),
            "split operation is retired"
        );
        anyhow::ensure!(
            self.resources
                .terminal()
                .lock()
                .is_ok_and(|t| !t.is_session_closed() && !t.shell_process_exited()),
            "split session already exited"
        );
        Ok(PreparedSplit {
            operation: self.id,
            source: self.source,
            target: self.target,
            draw: self.draw,
            kind: self.kind,
            metadata: self.metadata,
            resources: self.resources,
        })
    }
}
/// Coalesce display metadata while pending instead of growing an unconsumed side-effect queue.
fn retain_metadata(
    events: &mut Vec<harbor_terminal::TerminalOutputEvent>,
    event: harbor_terminal::TerminalOutputEvent,
) {
    use harbor_terminal::TerminalOutputEvent::*;
    let category = |event: &harbor_terminal::TerminalOutputEvent| match event {
        TitleChanged(_) | TitleReset => Some(0),
        WorkingDirectoryChanged(_) | WorkingDirectoryReset => Some(1),
        ShellIntegration(_) | ShellIntegrationReset => Some(2),
        _ => None,
    };
    if let Some(key) = category(&event) {
        events.retain(|previous| category(previous) != Some(key));
        events.push(event);
    }
}

fn source_live(tabs: &TabManager, source: PaneBinding) -> bool {
    tabs.contains_binding(source)
        && tabs.session_terminal(source.session).is_some_and(|t| {
            t.lock()
                .is_ok_and(|t| !t.is_session_closed() && !t.shell_process_exited())
        })
}
impl SplitOperation {
    /// Capture current opt-in recognition, not initial launch cwd or remembered agent cwd.
    /// This only captures data/reserves identities. It never connects in response to metadata.
    pub(crate) fn begin(tabs: &mut TabManager, default: ShellCommand) -> anyhow::Result<Self> {
        let source = tabs
            .active_binding()
            .ok_or_else(|| anyhow::anyhow!("no split source"))?;
        anyhow::ensure!(source_live(tabs, source), "split source is closed");
        let context = tabs
            .session_terminal(source.session)
            .and_then(|t| t.lock().ok().and_then(|t| t.project_snapshot()))
            .map(|mut recipe| {
                recipe.agent = None;
                recipe
            });
        #[allow(deprecated)] // Retain compatibility with toolchains predating Atomic::try_update.
        let id = NEXT_OPERATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map(OperationId)
            .map_err(|_| anyhow::anyhow!("split operation identity exhausted"))?;
        let (target, draw) = tabs.reserve_split_ids(source)?;
        Ok(Self {
            id,
            source,
            target,
            draw,
            context,
            default,
            live: Arc::new(AtomicBool::new(true)),
            state: State::Idle,
            metadata: Vec::new(),
        })
    }
    pub(crate) fn id(&self) -> OperationId {
        self.id
    }
    pub(crate) fn target(&self) -> (PaneBinding, ExternalDrawId) {
        (self.target, self.draw)
    }
    pub(crate) fn cancel(&mut self) {
        self.live.store(false, Ordering::Release);
    }
    /// Host must service unpublished output/authentication and poll until a terminal outcome.
    /// No pending presentation/input handler may route to whichever pane is currently active.
    pub(crate) fn pending_resources(&self) -> Option<&TerminalTabResources> {
        match &self.state {
            State::Waiting { resources, .. } => Some(resources),
            _ => None,
        }
    }
    pub(crate) fn next_poll(&self, now: Instant) -> Option<Instant> {
        match &self.state {
            State::Done => None,
            State::Waiting { deadline, .. } => Some((*deadline).min(now + POLL_INTERVAL)),
            _ => Some(now + POLL_INTERVAL),
        }
    }
    pub(crate) fn poll(
        &mut self,
        tabs: &TabManager,
        now: Instant,
        mut launch: impl FnMut(&ShellCommand, Option<&StartupProbe>) -> AttemptLaunch,
    ) -> SplitPoll {
        let state = std::mem::replace(&mut self.state, State::Done);
        if matches!(state, State::Done) {
            return SplitPoll::Finished;
        }
        let cancelled = !self.live.load(Ordering::Acquire) || !source_live(tabs, self.source);
        match state {
            State::Idle if cancelled => SplitPoll::Cancelled,
            State::Idle => {
                let probe = match StartupProbe::new(self.id) {
                    Ok(probe) => probe,
                    Err(error) => return SplitPoll::Failed(error),
                };
                match self.context.as_ref().map(|r| probe.command(r)) {
                    Some(Ok(command)) => {
                        let attempt = launch(&command, Some(&probe));
                        match attempt.resources {
                            Ok(resources) => {
                                self.state = State::Waiting {
                                    resources: Box::new(resources),
                                    probe,
                                    deadline: now + STARTUP_TIMEOUT,
                                    cleanup: attempt.cleanup,
                                }
                            }
                            Err(_) => {
                                self.state = State::Cleaning {
                                    cleanup: attempt.cleanup,
                                    next: Next::Fallback,
                                }
                            }
                        }
                        SplitPoll::Pending
                    }
                    _ => self.launch_default(&mut launch, LaunchKind::DefaultUnknownContext),
                }
            }
            State::Waiting {
                resources,
                probe,
                deadline,
                cleanup,
            } => {
                let (closed, parsed) = resources.terminal().lock().map_or((true, 0), |mut t| {
                    // Keep the bounded pump moving while unpublished, without authorizing
                    // clipboard or other external effects from a not-yet-attached child.
                    t.drain_pty();
                    for event in t.drain_output_events() {
                        retain_metadata(&mut self.metadata, event);
                    }
                    (
                        t.is_session_closed() || t.shell_process_exited(),
                        t.parsed_output_bytes(),
                    )
                });
                let status = probe.status.load(Ordering::Acquire);
                // Raw readiness precedes queued parsing. Keep bounded polls unpublished
                // until bytes through that marker and their side effects have been drained.
                if !cancelled
                    && !closed
                    && status == READY
                    && parsed >= probe.ready_end.load(Ordering::Relaxed)
                {
                    return self.ready(*resources, LaunchKind::Inherited);
                }
                if cancelled || status == FAILED || closed || now >= deadline {
                    drop(resources); // retire bridge/I/O before waiting for native teardown
                    self.metadata.clear(); // No inherited metadata may label a default result.
                    self.state = State::Cleaning {
                        cleanup,
                        next: if cancelled {
                            Next::Cancelled
                        } else {
                            Next::Fallback
                        },
                    };
                } else {
                    self.state = State::Waiting {
                        resources,
                        probe,
                        deadline,
                        cleanup,
                    };
                }
                SplitPoll::Pending
            }
            State::Cleaning { cleanup, next } => {
                if cleanup.as_ref().is_some_and(|receipt| !receipt.complete()) {
                    self.state = State::Cleaning { cleanup, next };
                    return SplitPoll::Pending;
                }
                if cancelled {
                    return SplitPoll::Cancelled;
                }
                match next {
                    Next::Fallback => {
                        self.launch_default(&mut launch, LaunchKind::DefaultStartupFailure)
                    }
                    Next::Cancelled => SplitPoll::Cancelled,
                    Next::Failed(error) => SplitPoll::Failed(error),
                }
            }
            State::Done => unreachable!(),
        }
    }
    fn launch_default(
        &mut self,
        launch: &mut impl FnMut(&ShellCommand, Option<&StartupProbe>) -> AttemptLaunch,
        kind: LaunchKind,
    ) -> SplitPoll {
        // Exact accepted default policy; never merge source environment/args/directory.
        let attempt = launch(&self.default, None);
        match attempt.resources {
            Ok(resources) => self.ready(resources, kind),
            Err(error) => {
                self.state = State::Cleaning {
                    cleanup: attempt.cleanup,
                    next: Next::Failed(error),
                };
                SplitPoll::Pending
            }
        }
    }
    fn ready(&mut self, resources: TerminalTabResources, kind: LaunchKind) -> SplitPoll {
        SplitPoll::Ready(Box::new(SplitReady {
            id: self.id,
            source: self.source,
            target: self.target,
            draw: self.draw,
            live: self.live.clone(),
            kind,
            resources,
            metadata: std::mem::take(&mut self.metadata),
        }))
    }
}
impl Drop for SplitOperation {
    fn drop(&mut self) {
        self.live.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests;
