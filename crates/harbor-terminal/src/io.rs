//! PTY I/O and ANSI parsing — extracted from `Terminal` to separate
//! I/O lifecycle from screen state and GPU rendering.

use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::model::TerminalSize;
use harbor_pty::PtyControl;

use crate::input;
use crate::parser::TerminalParser;
use crate::pointer::PointerInteraction;
use crate::screen::Screen;
use crate::types::{TerminalEvent, TerminalPointerPhase};

mod chunk;
use chunk::{ReaderBuffers, ReaderChunk, ReaderRecycle};
/// The maximum number of parser events buffered between the blocking PTY reader
/// and the UI thread. Backpressure here bounds memory without blocking UI.
pub(crate) const PTY_QUEUE_CAPACITY: usize = 32;
// Each reader chunk is at most 4096 bytes. Bound work even if the queue refills.
const PTY_DRAIN_EVENT_BUDGET: usize = 16;

const RESIZE_BARRIER_TIMEOUT: Duration = Duration::from_secs(2);
const BARRIER_INTERRUPT_RETRY: Duration = Duration::from_millis(10);

#[cfg(test)]
mod allocation_tests;
#[cfg(test)]
#[path = "async_resize_tests.rs"]
mod async_resize_tests;

type ReaderInterrupt = Arc<dyn Fn(&JoinHandle<()>) -> anyhow::Result<()> + Send + Sync + 'static>;
type Wake = Arc<Mutex<dyn Fn() -> bool + Send>>;
#[derive(Debug)]
enum ReaderEvent {
    Bytes(ReaderChunk),
    BarrierAck(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderCommand {
    Barrier(u64),
    Resume(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderCommandStatus {
    Idle,
    Resumed,
    Disconnected,
}

#[derive(Debug)]
enum BarrierStatus {
    Waiting,
    Ready,
    Failed(String),
    Released,
}

/// An owned, non-borrowing cancellation lease for application-coordinated acquisition.
/// Dropping it releases the reader even without reacquiring the Terminal lock.
pub struct ConfigurationBarrier {
    release: Option<ResizeBarrier>,
}

impl ConfigurationBarrier {
    /// ACK readiness is not configuration acceptance. The host must continue
    /// draining/ticking the terminal and coordinate every member before commit.
    pub fn is_ready(&self) -> anyhow::Result<bool> {
        self.release
            .as_ref()
            .map_or(Ok(true), ResizeBarrier::is_ready)
    }
}

pub(crate) struct ResizeBarrier {
    commands: Sender<ReaderCommand>,
    epoch: u64,
    status: Arc<Mutex<BarrierStatus>>,
}

impl ResizeBarrier {
    fn is_ready(&self) -> anyhow::Result<bool> {
        match &*self.status.lock().unwrap_or_else(|e| e.into_inner()) {
            BarrierStatus::Waiting => Ok(false),
            BarrierStatus::Ready => Ok(true),
            BarrierStatus::Failed(error) => Err(anyhow::anyhow!("{error}")),
            BarrierStatus::Released => anyhow::bail!("PTY resize barrier was cancelled"),
        }
    }

    fn lease(&self) -> Self {
        Self {
            commands: self.commands.clone(),
            epoch: self.epoch,
            status: Arc::clone(&self.status),
        }
    }
}

impl Drop for ResizeBarrier {
    fn drop(&mut self) {
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(*status, BarrierStatus::Failed(_)) {
            *status = BarrierStatus::Released;
        }
        let _ = self.commands.send(ReaderCommand::Resume(self.epoch));
    }
}

struct PendingBarrier {
    barrier: ResizeBarrier,
    deadline: Instant,
    retry: Instant,
    configuration: bool,
}
// ── TerminalPty ───────────────────────────────────────────────────────

/// I/O and shutdown resources sharing the terminal's lifetime.
struct TerminalPty {
    output: Receiver<ReaderEvent>,
    recycle: ReaderRecycle,
    commands: Sender<ReaderCommand>,
    writer: Box<dyn Write + Send>,
    reader: Option<JoinHandle<()>>,
    control: Option<PtyControl>,
    wake_pending: Arc<AtomicBool>,
    wake: Wake,
    next_barrier_epoch: u64,
    test_interrupt: Option<ReaderInterrupt>,
    barrier_timeout: Duration,
}

impl TerminalPty {
    fn try_new<R, W>(
        reader: R,
        writer: W,
        control: Option<PtyControl>,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> anyhow::Result<Self>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        // SAFETY: std::thread::Builder::spawn returns Err only when it did not
        // launch a thread; it drops the closure and captured reader on failure.
        unsafe {
            Self::try_new_with_spawn(reader, writer, control, wake, |pump| {
                std::thread::Builder::new()
                    .name("harbor-terminal-reader".into())
                    .spawn(pump)
            })
        }
    }

    /// # Safety
    /// If `spawn` returns an error, it must not have started the pump and
    /// must have dropped the pump closure (including its reader endpoint).
    unsafe fn try_new_with_spawn<R, W>(
        reader: R,
        writer: W,
        control: Option<PtyControl>,
        wake: impl Fn() -> bool + Send + 'static,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<JoinHandle<()>>,
    ) -> anyhow::Result<Self>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let (output_tx, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let (commands, reader_commands) = mpsc::channel();
        let (reader_buffers, recycle) = ReaderBuffers::pair();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let reader_wake_pending = Arc::clone(&wake_pending);
        let wake: Wake = Arc::new(Mutex::new(wake));
        let reader_wake = Arc::clone(&wake);
        let reader = spawn(Box::new(move || {
            pump_reader(
                reader,
                output_tx,
                reader_commands,
                reader_wake_pending,
                move || (reader_wake.lock().unwrap_or_else(|e| e.into_inner()))(),
                reader_buffers,
            )
        }));
        let reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                // SAFETY: a failed spawn drops its closure and captured reader
                // before returning; no reader thread was started for this PTY.
                if let Some(control) = control {
                    unsafe { control.shutdown_unstarted() };
                }
                return Err(error.into());
            }
        };
        Ok(Self {
            output,
            recycle,
            commands,
            writer: Box::new(writer),
            reader: Some(reader),
            control,
            wake_pending,
            wake,
            next_barrier_epoch: 1,
            test_interrupt: None,
            barrier_timeout: RESIZE_BARRIER_TIMEOUT,
        })
    }
}

impl Drop for TerminalPty {
    fn drop(&mut self) {
        // Disconnect the receiver so the reader thread exits its send loop.
        // The writer is dropped automatically.
        let control = self.control.take();
        let reader = self.reader.take();
        match (control, reader) {
            (Some(control), Some(reader)) => control.shutdown(reader),
            (None, Some(reader)) => {
                // Keep-alive test readers park after their last chunk; unpark so
                // they observe EOF and exit instead of leaking the thread.
                reader.thread().unpark();
            }
            _ => {}
        }
    }
}

#[cfg(all(test, feature = "renderer"))]
impl TerminalPty {
    fn set_test_barrier(
        &mut self,
        interrupt: impl Fn(&JoinHandle<()>) -> anyhow::Result<()> + Send + Sync + 'static,
        timeout: Duration,
    ) {
        self.test_interrupt = Some(Arc::new(interrupt));
        self.barrier_timeout = timeout;
    }
}

fn pump_reader<R>(
    reader: R,
    output: mpsc::SyncSender<ReaderEvent>,
    commands: Receiver<ReaderCommand>,
    wake_pending: Arc<AtomicBool>,
    wake: impl Fn() -> bool,
    buffers: ReaderBuffers,
) where
    R: Read,
{
    pump_reader_with_after_read(reader, output, commands, wake_pending, wake, || {}, buffers);
}

fn pump_reader_with_after_read<R>(
    mut reader: R,
    output: mpsc::SyncSender<ReaderEvent>,
    commands: Receiver<ReaderCommand>,
    wake_pending: Arc<AtomicBool>,
    wake: impl Fn() -> bool,
    mut after_read: impl FnMut(),
    mut buffers: ReaderBuffers,
) where
    R: Read,
{
    let notify = || {
        if !wake_pending.swap(true, Ordering::AcqRel) {
            wake()
        } else {
            true
        }
    };
    while let Ok(ReaderCommandStatus::Idle | ReaderCommandStatus::Resumed) =
        service_reader_commands(&commands, &output, &notify)
    {
        let mut chunk = buffers.take();
        let length = match reader.read(chunk.read_buffer()) {
            Ok(0) => break,
            Ok(length) => length,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                buffers.put_unread(chunk);
                match service_reader_commands(&commands, &output, &notify) {
                    Ok(ReaderCommandStatus::Resumed) => continue,
                    Ok(ReaderCommandStatus::Idle | ReaderCommandStatus::Disconnected) | Err(()) => {
                        tracing::warn!(error = %error, "terminal pty reader stopped after unexpected interruption");
                        break;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "terminal pty reader stopped after read error");
                break;
            }
        };
        after_read();
        chunk.finish(length);
        if output.send(ReaderEvent::Bytes(chunk)).is_err() {
            break;
        }
        if !notify() {
            break;
        }
    }
    // One close-observation wake so a surviving view can drain disconnect
    // and release synchronized-output suppression without waiting for recovery.
    if !wake_pending.swap(true, Ordering::AcqRel) {
        let _ = wake();
    }
}

/// Services at most one pending barrier and returns whether the reader may continue.
fn service_reader_commands(
    commands: &Receiver<ReaderCommand>,
    output: &mpsc::SyncSender<ReaderEvent>,
    notify: &impl Fn() -> bool,
) -> Result<ReaderCommandStatus, ()> {
    loop {
        let command = match commands.try_recv() {
            Ok(command) => command,
            Err(TryRecvError::Empty) => return Ok(ReaderCommandStatus::Idle),
            Err(TryRecvError::Disconnected) => {
                return Ok(ReaderCommandStatus::Disconnected);
            }
        };
        let ReaderCommand::Barrier(epoch) = command else {
            continue;
        };
        output
            .send(ReaderEvent::BarrierAck(epoch))
            .map_err(|_| ())?;
        if !notify() {
            return Err(());
        }
        loop {
            match commands.recv() {
                Ok(ReaderCommand::Resume(resume_epoch)) if resume_epoch == epoch => break,
                Ok(_) => {}
                Err(_) => return Ok(ReaderCommandStatus::Disconnected),
            }
        }
        return Ok(ReaderCommandStatus::Resumed);
    }
}

// ── TerminalSession ───────────────────────────────────────────────────

/// GPU-independent PTY session adapter: owns the endpoints, reader, resize
/// coordination, and teardown. The logical I/O state below owns VT parsing.
struct TerminalSession {
    // Drop the barrier (Resume) before platform shutdown joins the reader.
    barrier: Option<PendingBarrier>,
    pty: Option<TerminalPty>,
    closed_observed: bool,
}

impl TerminalSession {
    fn headless() -> Self {
        Self {
            barrier: None,
            pty: None,
            closed_observed: false,
        }
    }

    fn with_pty(pty: TerminalPty) -> Self {
        Self {
            barrier: None,
            pty: Some(pty),
            closed_observed: false,
        }
    }

    fn observe_disconnect(&mut self) -> bool {
        if self.closed_observed {
            false
        } else {
            self.closed_observed = true;
            true
        }
    }

    /// Receive one event at a time, never collecting a refilling queue into a vector.
    /// A wake stays pending until the queue is empty. The second receive
    /// after clearing it closes the producer/consumer race.
    fn next_event(&mut self) -> Result<Option<ReaderEvent>, TryRecvError> {
        let Some(pty) = self.pty.as_ref() else {
            return Ok(None);
        };
        match pty.output.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => {
                pty.wake_pending.store(false, Ordering::Release);
                match pty.output.try_recv() {
                    Ok(event) => Ok(Some(event)),
                    Err(TryRecvError::Empty) => Ok(None),
                    Err(error) => Err(error),
                }
            }
            Err(error) => {
                pty.wake_pending.store(false, Ordering::Release);
                Err(error)
            }
        }
    }

    fn recycle_chunk(&mut self, chunk: ReaderChunk) {
        if let Some(pty) = self.pty.as_mut() {
            pty.recycle.recycle(chunk);
        }
    }

    fn flush_recycled(&mut self) {
        if let Some(pty) = self.pty.as_mut() {
            pty.recycle.flush();
        }
    }
    fn request_resize_barrier(&mut self, configuration: bool, now: Instant) -> anyhow::Result<()> {
        if self.barrier.as_ref().is_some_and(|pending| {
            matches!(
                *pending
                    .barrier
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
                BarrierStatus::Released | BarrierStatus::Failed(_)
            )
        }) {
            self.barrier = None;
        }
        if self.barrier.is_some() {
            return Ok(());
        }
        let Some(pty) = self.pty.as_mut() else {
            return Ok(());
        };
        anyhow::ensure!(
            pty.control.is_some() || pty.test_interrupt.is_some(),
            "cannot resize a terminal with an active reader but no interrupt capability"
        );
        let epoch = pty.next_barrier_epoch;
        pty.next_barrier_epoch = epoch
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("PTY resize barrier epoch exhausted"))?;
        let barrier = ResizeBarrier {
            commands: pty.commands.clone(),
            epoch,
            status: Arc::new(Mutex::new(BarrierStatus::Waiting)),
        };
        barrier
            .commands
            .send(ReaderCommand::Barrier(epoch))
            .map_err(|_| anyhow::anyhow!("terminal reader stopped before resize barrier"))?;
        self.barrier = Some(PendingBarrier {
            barrier,
            deadline: now + pty.barrier_timeout,
            retry: now + BARRIER_INTERRUPT_RETRY,
            configuration,
        });
        if let Err(error) = self.interrupt_reader_for_barrier() {
            self.fail_resize_barrier(error.to_string());
            return Err(error);
        }
        Ok(())
    }

    fn fail_resize_barrier(&mut self, error: String) {
        if let Some(pending) = self.barrier.as_ref() {
            *pending
                .barrier
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = BarrierStatus::Failed(error);
            let _ = pending
                .barrier
                .commands
                .send(ReaderCommand::Resume(pending.barrier.epoch));
        }
    }

    fn advance_resize_barrier(&mut self, now: Instant) {
        let Some(pending) = self.barrier.as_ref() else {
            return;
        };
        if !matches!(
            *pending
                .barrier
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
            BarrierStatus::Waiting
        ) {
            return;
        }
        if now >= pending.deadline {
            self.fail_resize_barrier(format!(
                "timed out waiting for PTY resize barrier {}",
                pending.barrier.epoch
            ));
        } else if now >= pending.retry {
            if let Err(error) = self.interrupt_reader_for_barrier() {
                self.fail_resize_barrier(error.to_string());
            } else if let Some(pending) = self.barrier.as_mut() {
                pending.retry = now + BARRIER_INTERRUPT_RETRY;
            }
        }
    }

    fn acknowledge_resize_barrier(&mut self, epoch: u64) {
        if let Some(pending) = self.barrier.as_ref()
            && pending.barrier.epoch == epoch
        {
            let mut status = pending
                .barrier
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if matches!(*status, BarrierStatus::Waiting) {
                *status = BarrierStatus::Ready;
            }
        }
    }

    fn resize_barrier_ready(&self) -> anyhow::Result<bool> {
        self.barrier
            .as_ref()
            .map_or(Ok(self.pty.is_none()), |pending| pending.barrier.is_ready())
    }

    fn resize_deadline(&self) -> Option<Instant> {
        self.barrier.as_ref().and_then(|pending| {
            matches!(
                *pending
                    .barrier
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
                BarrierStatus::Waiting
            )
            .then_some(pending.retry.min(pending.deadline))
        })
    }

    fn take_resize_barrier(&mut self) -> anyhow::Result<Option<ResizeBarrier>> {
        anyhow::ensure!(
            self.resize_barrier_ready()?,
            "PTY resize pending: acquire and poll the reader barrier first"
        );
        Ok(self.barrier.take().map(|pending| pending.barrier))
    }

    fn interrupt_reader_for_barrier(&self) -> anyhow::Result<()> {
        let pty = self
            .pty
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("terminal has no active PTY"))?;
        let reader = pty
            .reader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("terminal PTY reader is unavailable"))?;
        if let Some(interrupt) = &pty.test_interrupt {
            return interrupt(reader);
        }
        let control = pty
            .control
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("terminal PTY has no reader control"))?;
        control.interrupt_reader(reader)
    }

    fn write_pty(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let Some(pty) = self.pty.as_mut() else {
            anyhow::bail!("terminal has no active pty");
        };
        pty.writer.write_all(bytes).map_err(Into::into)
    }

    fn reflow_viewport(&self) -> crate::primary_reflow::ReflowViewport {
        use crate::primary_reflow::ReflowViewport;
        if cfg!(windows) && self.pty.as_ref().is_some_and(|pty| pty.control.is_some()) {
            ReflowViewport::PreserveLiveTop
        } else {
            ReflowViewport::PullHistory
        }
    }

    fn resize_pty(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        if let Some(pty) = self.pty.as_mut()
            && let Some(control) = pty.control.as_mut()
        {
            control.resize(harbor_pty::TerminalSize {
                rows: size.rows,
                cols: size.cols,
            })?;
        }
        Ok(())
    }
}

// ── TerminalIo (logical parser and input) ─────────────────────────────

/// Parser and input encoding on the logical/UI thread, with a separately
/// owned PTY session adapter.
pub(crate) struct TerminalIo {
    parser: TerminalParser,
    session: TerminalSession,
    suppress_scroll_snap: bool,
}

impl TerminalIo {
    pub(crate) fn new<R, W>(
        pty_read: R,
        pty_write: W,
        pty_control: Option<PtyControl>,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        Self::try_new(pty_read, pty_write, pty_control, wake)
            .expect("failed to start terminal PTY reader")
    }

    pub(crate) fn try_new<R, W>(
        pty_read: R,
        pty_write: W,
        pty_control: Option<PtyControl>,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> anyhow::Result<Self>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        Ok(Self::with_session(TerminalSession::with_pty(
            TerminalPty::try_new(pty_read, pty_write, pty_control, wake)?,
        )))
    }

    #[cfg(all(test, feature = "renderer"))]
    pub(crate) fn new_with_test_barrier<R, W>(
        pty_read: R,
        pty_write: W,
        interrupt: impl Fn(&JoinHandle<()>) -> anyhow::Result<()> + Send + Sync + 'static,
        timeout: Duration,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let mut pty = TerminalPty::try_new(pty_read, pty_write, None, wake)
            .expect("failed to start test PTY reader");
        pty.set_test_barrier(interrupt, timeout);
        Self::with_session(TerminalSession::with_pty(pty))
    }

    pub(crate) fn new_headless() -> Self {
        Self::with_session(TerminalSession::headless())
    }

    pub(crate) fn has_session(&self) -> bool {
        self.session.pty.is_some()
    }

    fn with_session(session: TerminalSession) -> Self {
        Self {
            parser: TerminalParser::default(),
            session,
            suppress_scroll_snap: false,
        }
    }

    // ── byte processing ───────────────────────────────────────────────

    /// Feeds raw PTY bytes through the streaming parser.
    pub(crate) fn feed_pty_output(
        &mut self,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        bytes: &[u8],
    ) {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            screen.set_external_anchor_interest(pointer.has_selection_state());
            let before_projection = screen
                .tracks_content_anchors()
                .then(|| screen.content_projection().ok())
                .flatten();
            let result = self.parser.put_bytes(screen, remaining);
            remaining = &remaining[result.consumed..];
            if screen.tracks_content_anchors() {
                match screen.finish_anchor_mutations(before_projection.as_ref()) {
                    Ok((mutations, projection)) => {
                        pointer.reconcile_selection(&mutations, &projection);
                    }
                    Err(error) => {
                        tracing::error!(generation = error.generation, column = error.column, kind = ?error.kind, "content anchor reconciliation failed");
                        pointer.clear();
                    }
                }
            } else {
                screen.discard_anchor_mutations();
            }
            if let Some(action) = result.alt_request {
                self.suppress_scroll_snap = false;
                pointer.apply_alt_transition(screen, action);
            }
        }
    }

    /// Feeds PTY output with an automatic scroll-to-bottom snap (unless suppressed).
    pub(crate) fn feed_pty_output_snapped(
        &mut self,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        output: &[u8],
    ) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }
        if !screen.is_alt() && !self.suppress_scroll_snap {
            screen.scroll_to_bottom();
        }
        self.feed_pty_output(screen, pointer, output);
    }
    pub(crate) fn drain_output_events(&mut self) -> Vec<crate::TerminalOutputEvent> {
        self.parser.drain_output_events()
    }
    pub(crate) fn is_session_closed(&self) -> bool {
        self.session.closed_observed
            || self
                .session
                .pty
                .as_ref()
                .is_some_and(|pty| pty.reader.as_ref().is_some_and(JoinHandle::is_finished))
    }

    pub(crate) fn clipboard_delivery(&self) -> crate::ClipboardDelivery {
        self.parser.clipboard_delivery()
    }

    pub(crate) fn set_clipboard_delivery(&mut self, delivery: crate::ClipboardDelivery) {
        self.parser
            .set_clipboard_delivery(if self.is_session_closed() {
                crate::ClipboardDelivery::Discard
            } else {
                delivery
            });
    }

    // ── PTY I/O ───────────────────────────────────────────────────────

    /// Parses at most one budget of ordered reader events on the logical thread.
    pub(crate) fn drain(&mut self, screen: &mut Screen, pointer: &mut PointerInteraction) -> bool {
        self.session.advance_resize_barrier(Instant::now());
        let mut consumed_bytes = false;
        let mut budget_exhausted = true;
        for _ in 0..PTY_DRAIN_EVENT_BUDGET {
            match self.session.next_event() {
                Ok(Some(ReaderEvent::Bytes(bytes))) => {
                    consumed_bytes = true;
                    self.process_reader_bytes(screen, pointer, &bytes);
                    self.session.recycle_chunk(bytes);
                }
                Ok(Some(ReaderEvent::BarrierAck(epoch))) => {
                    self.session.acknowledge_resize_barrier(epoch);
                }
                Ok(None) => {
                    budget_exhausted = false;
                    break;
                }
                Err(_) => {
                    self.observe_reader_disconnect(screen);
                    budget_exhausted = false;
                    break;
                }
            }
        }
        // Return even a partial batch before yielding, including ACK/EOF paths.
        // No buffer is returned until parsing and protocol replies are finished.
        self.session.flush_recycled();
        if budget_exhausted {
            // Wakes stay coalesced until the queue is observed empty. A budget
            // stop therefore needs another turn even under synchronized output.
            if let Some(pty) = self.session.pty.as_ref() {
                let _ = (pty.wake.lock().unwrap_or_else(|e| e.into_inner()))();
            }
        }
        consumed_bytes
    }

    pub(crate) fn request_resize_barrier(&mut self, now: Instant) -> anyhow::Result<()> {
        self.session.request_resize_barrier(false, now)
    }

    pub(crate) fn begin_configuration_barrier(&mut self) -> anyhow::Result<ConfigurationBarrier> {
        self.session.barrier = None;
        if self.is_session_closed() {
            return Ok(ConfigurationBarrier { release: None });
        }
        self.session.request_resize_barrier(true, Instant::now())?;
        Ok(ConfigurationBarrier {
            release: self
                .session
                .barrier
                .as_ref()
                .map(|pending| pending.barrier.lease()),
        })
    }

    pub(crate) fn configuration_barrier_active(&self) -> bool {
        self.session.barrier.as_ref().is_some_and(|pending| {
            pending.configuration
                && !matches!(
                    *pending
                        .barrier
                        .status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()),
                    BarrierStatus::Released | BarrierStatus::Failed(_)
                )
        })
    }

    pub(crate) fn resize_barrier_ready(&self) -> anyhow::Result<bool> {
        self.session.resize_barrier_ready()
    }

    pub(crate) fn take_resize_barrier(&mut self) -> anyhow::Result<Option<ResizeBarrier>> {
        self.session.take_resize_barrier()
    }

    pub(crate) fn cancel_resize_barrier(&mut self) {
        self.session.barrier = None;
    }

    pub(crate) fn resize_deadline(&self) -> Option<Instant> {
        self.session.resize_deadline()
    }

    fn process_reader_bytes(
        &mut self,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        bytes: &[u8],
    ) {
        // Resize does not provide an output-stream boundary. Keep partial sequences
        // and service protocol queries under the current (pre-resize) geometry.
        self.feed_pty_output_snapped(screen, pointer, bytes);
        let replies = screen.drain_replies();
        if !replies.is_empty()
            && let Err(error) = self.session.write_pty(&replies)
        {
            tracing::warn!(error = %error, "failed to write terminal replies to pty");
        }
    }

    fn observe_reader_disconnect(&mut self, screen: &mut Screen) {
        self.session
            .fail_resize_barrier("terminal reader stopped before resize commit".to_owned());
        if self.session.observe_disconnect() {
            self.parser.close_session();
            screen.clear_synchronized_output();
        }
    }

    /// Writes bytes synchronously through the session's input endpoint.
    pub(crate) fn write_pty(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.session.write_pty(bytes)
    }

    pub(crate) fn reflow_viewport(&self) -> crate::primary_reflow::ReflowViewport {
        self.session.reflow_viewport()
    }

    pub(crate) fn resize_pty(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        self.session.resize_pty(size)
    }

    // ── event handling ────────────────────────────────────────────────

    /// Writes an already projected pointer with the exact modes used to project it.
    /// Do not drain here: that could change coordinate units after projection.
    pub(crate) fn write_mouse_event(
        &mut self,
        event: crate::types::TerminalPointerEvent,
        modes: crate::InputModes,
    ) -> anyhow::Result<bool> {
        let Some(bytes) = input::encode(&TerminalEvent::Pointer(event), modes) else {
            return Ok(false);
        };
        self.write_pty(&bytes)?;
        Ok(true)
    }

    /// Drains new output, interprets scrollback navigation / wheel, then encodes
    /// remaining events for the PTY.
    ///
    /// Returns `true` when bytes were written to the PTY input endpoint.
    pub(crate) fn handle_event(
        &mut self,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        event: TerminalEvent,
    ) -> anyhow::Result<bool> {
        self.drain(screen, pointer);
        self.handle_current_event(screen, event)
    }

    /// Handles input after the caller has drained output and resolved coordinates.
    pub(crate) fn handle_current_event(
        &mut self,
        screen: &mut Screen,
        event: TerminalEvent,
    ) -> anyhow::Result<bool> {
        if let TerminalEvent::Focus(focus) = &event {
            if screen.observe_focus(*focus) {
                self.write_pty(input::encode_focus(*focus))?;
                return Ok(true);
            }
            return Ok(false);
        }

        if let TerminalEvent::Pointer(pointer) = &event
            && screen.input_modes().mouse_tracking != crate::model::MouseTrackingMode::Disabled
        {
            return self.write_mouse_event(*pointer, screen.input_modes());
        }

        if Self::try_scrollback_wheel(screen, &event) {
            return Ok(false);
        }

        let Some(bytes) = input::encode(&event, screen.input_modes()) else {
            return Ok(false);
        };
        // Terminal-bound input resumes the live viewport so typed text and the
        // shell response are visible immediately after browsing scrollback.
        screen.scroll_to_bottom();
        self.suppress_scroll_snap = false;
        self.write_pty(&bytes)?;
        Ok(true)
    }

    /// Wheel events scroll the primary-screen viewport; alt-screen wheels are
    /// consumed without PTY write. Terminal owns line/pixel → row conversion.
    fn try_scrollback_wheel(screen: &mut Screen, event: &TerminalEvent) -> bool {
        let TerminalEvent::Pointer(pointer) = event else {
            return false;
        };
        let (dy, is_pixel) = match pointer.phase {
            TerminalPointerPhase::WheelLine { dy, .. } => (dy, false),
            TerminalPointerPhase::WheelPixel { dy, .. } => (dy, true),
            _ => return false,
        };

        screen.scroll_wheel(dy, is_pixel);
        true
    }

    // ── mode control ──────────────────────────────────────────────────

    pub(crate) fn set_suppress_scroll_snap(&mut self, suppress: bool) {
        self.suppress_scroll_snap = suppress;
    }

    pub(crate) fn reset_scroll_snap(&mut self) {
        self.suppress_scroll_snap = false;
    }
}

#[cfg(test)]
mod tests;
