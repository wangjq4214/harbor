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

/// The maximum number of parser events buffered between the blocking PTY reader
/// and the UI thread. Backpressure here bounds memory without blocking UI.
pub(crate) const PTY_QUEUE_CAPACITY: usize = 32;
// Each reader chunk is at most 4096 bytes. Bound work even if the queue refills.
const PTY_DRAIN_EVENT_BUDGET: usize = 16;

const RESIZE_BARRIER_TIMEOUT: Duration = Duration::from_secs(2);
const BARRIER_INTERRUPT_RETRY: Duration = Duration::from_millis(10);

#[cfg(test)]
#[path = "async_resize_tests.rs"]
mod async_resize_tests;

type ReaderInterrupt = Arc<dyn Fn(&JoinHandle<()>) -> anyhow::Result<()> + Send + Sync + 'static>;
type Wake = Arc<Mutex<dyn Fn() -> bool + Send>>;
#[derive(Debug)]
enum ReaderEvent {
    Bytes(Vec<u8>),
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
) where
    R: Read,
{
    pump_reader_with_after_read(reader, output, commands, wake_pending, wake, || {});
}

fn pump_reader_with_after_read<R>(
    mut reader: R,
    output: mpsc::SyncSender<ReaderEvent>,
    commands: Receiver<ReaderCommand>,
    wake_pending: Arc<AtomicBool>,
    wake: impl Fn() -> bool,
    mut after_read: impl FnMut(),
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
    let mut buffer = [0; 4096];
    while let Ok(ReaderCommandStatus::Idle | ReaderCommandStatus::Resumed) =
        service_reader_commands(&commands, &output, &notify)
    {
        let length = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => length,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
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
        if output
            .send(ReaderEvent::Bytes(buffer[..length].to_vec()))
            .is_err()
        {
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
        Self::feed_parser_bytes(
            &mut self.parser,
            &mut self.suppress_scroll_snap,
            screen,
            pointer,
            bytes,
        );
    }

    fn feed_parser_bytes(
        parser: &mut TerminalParser,
        suppress_scroll_snap: &mut bool,
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
            let result = parser.put_bytes(screen, remaining);
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
                *suppress_scroll_snap = false;
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
        Self::feed_parser_bytes_snapped(
            &mut self.parser,
            &mut self.suppress_scroll_snap,
            screen,
            pointer,
            output,
        );
    }

    fn feed_parser_bytes_snapped(
        parser: &mut TerminalParser,
        suppress_scroll_snap: &mut bool,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        output: &[u8],
    ) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }
        if !screen.is_alt() && !*suppress_scroll_snap {
            screen.scroll_to_bottom();
        }
        Self::feed_parser_bytes(parser, suppress_scroll_snap, screen, pointer, output);
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
        for _ in 0..PTY_DRAIN_EVENT_BUDGET {
            match self.session.next_event() {
                Ok(Some(ReaderEvent::Bytes(bytes))) => {
                    consumed_bytes = true;
                    self.process_reader_bytes(screen, pointer, &bytes);
                }
                Ok(Some(ReaderEvent::BarrierAck(epoch))) => {
                    self.session.acknowledge_resize_barrier(epoch);
                }
                Ok(None) => return consumed_bytes,
                Err(_) => {
                    self.observe_reader_disconnect(screen);
                    return consumed_bytes;
                }
            }
        }
        // The reader coalesces wakes until the queue is empty. We stopped before
        // observing that state, so explicitly request another turn, including
        // when synchronized output suppresses ordinary presentation. A harmless
        // extra wake is possible if the last event exhausted the queue.
        if let Some(pty) = self.session.pty.as_ref() {
            let _ = (pty.wake.lock().unwrap_or_else(|e| e.into_inner()))();
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
        Self::process_reader_bytes_with(
            &mut self.parser,
            &mut self.suppress_scroll_snap,
            &mut self.session,
            screen,
            pointer,
            bytes,
        );
    }

    fn process_reader_bytes_with(
        parser: &mut TerminalParser,
        suppress_scroll_snap: &mut bool,
        session: &mut TerminalSession,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        bytes: &[u8],
    ) {
        // Resize does not provide an output-stream boundary. Keep partial sequences
        // and service protocol queries under the current (pre-resize) geometry.
        Self::feed_parser_bytes_snapped(parser, suppress_scroll_snap, screen, pointer, bytes);
        let replies = screen.drain_replies();
        if !replies.is_empty()
            && let Err(error) = session.write_pty(&replies)
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
mod tests {
    use super::*;

    #[test]
    fn bounded_drain_rewakes_and_preserves_order_through_synchronized_output_and_eof() {
        let (tx, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let (commands, _command_rx) = mpsc::channel();
        let (wake_tx, wake_rx) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(true));
        let mut io = TerminalIo::with_session(TerminalSession::with_pty(TerminalPty {
            output,
            commands,
            writer: Box::new(std::io::sink()),
            reader: None,
            control: None,
            wake_pending: Arc::clone(&wake_pending),
            wake: Arc::new(Mutex::new(move || wake_tx.send(()).is_ok())),
            next_barrier_epoch: 1,
            test_interrupt: None,
            barrier_timeout: RESIZE_BARRIER_TIMEOUT,
        }));
        tx.send(ReaderEvent::Bytes(b"\x1b[?2026h".to_vec()))
            .unwrap();
        for _ in 1..PTY_DRAIN_EVENT_BUDGET {
            tx.send(ReaderEvent::Bytes(b"a".to_vec())).unwrap();
        }
        tx.send(ReaderEvent::Bytes(b"\x1b[?2026lb".to_vec()))
            .unwrap();
        drop(tx);

        let mut screen = Screen::new(2, 80);
        let mut pointer = PointerInteraction::default();
        assert!(io.drain(&mut screen, &mut pointer));
        assert!(!screen.ordinary_present_eligible());
        assert_eq!(
            screen.row_text(0).trim_end(),
            "a".repeat(PTY_DRAIN_EVENT_BUDGET - 1)
        );
        wake_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("continuation wake");
        assert!(wake_pending.load(Ordering::Acquire));
        assert!(!io.session.closed_observed);

        assert!(io.drain(&mut screen, &mut pointer));
        assert_eq!(
            screen.row_text(0).trim_end(),
            format!("{}b", "a".repeat(PTY_DRAIN_EVENT_BUDGET - 1))
        );
        assert!(screen.ordinary_present_eligible());
        assert!(io.session.closed_observed);
        assert!(!wake_pending.load(Ordering::Acquire));
        assert!(wake_rx.try_recv().is_err());
    }

    #[test]
    fn exact_budget_empty_queue_rearms_for_later_producer_wake() {
        let (tx, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let (commands, _command_rx) = mpsc::channel();
        let (wake_tx, wake_rx) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(true));
        let mut io = TerminalIo::with_session(TerminalSession::with_pty(TerminalPty {
            output,
            commands,
            writer: Box::new(std::io::sink()),
            reader: None,
            control: None,
            wake_pending: Arc::clone(&wake_pending),
            wake: Arc::new(Mutex::new(move || wake_tx.send(()).is_ok())),
            next_barrier_epoch: 1,
            test_interrupt: None,
            barrier_timeout: RESIZE_BARRIER_TIMEOUT,
        }));
        for _ in 0..PTY_DRAIN_EVENT_BUDGET {
            tx.send(ReaderEvent::Bytes(b"x".to_vec())).unwrap();
        }
        let mut screen = Screen::new(2, 80);
        let mut pointer = PointerInteraction::default();
        assert!(io.drain(&mut screen, &mut pointer));
        wake_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!io.drain(&mut screen, &mut pointer));
        assert!(!wake_pending.load(Ordering::Acquire));
        assert!(wake_rx.try_recv().is_err());

        // The real producer sends before atomically arming its coalesced wake.
        tx.send(ReaderEvent::Bytes(b"y".to_vec())).unwrap();
        assert!(!wake_pending.swap(true, Ordering::AcqRel));
        let wake = &io.session.pty.as_ref().unwrap().wake;
        assert!((wake.lock().unwrap())());
        wake_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(io.drain(&mut screen, &mut pointer));
        assert_eq!(
            screen.row_text(0).trim_end(),
            format!("{}y", "x".repeat(PTY_DRAIN_EVENT_BUDGET))
        );
        assert!(!wake_pending.load(Ordering::Acquire));
    }

    #[test]
    fn live_refilling_reader_progresses_on_continuation_wakes() {
        struct ChunkReader {
            count: usize,
            full: Sender<()>,
            finish: Receiver<()>,
        }
        impl Read for ChunkReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.count == PTY_QUEUE_CAPACITY * 2 {
                    self.finish.recv().unwrap();
                    return Ok(0);
                }
                self.count += 1;
                if self.count == PTY_QUEUE_CAPACITY + 1 {
                    self.full.send(()).unwrap();
                }
                buffer[0] = b'z';
                Ok(1)
            }
        }
        let (full_tx, full_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let (wake_tx, wake_rx) = mpsc::channel();
        let mut io = TerminalIo::new(
            ChunkReader {
                count: 0,
                full: full_tx,
                finish: finish_rx,
            },
            std::io::sink(),
            None,
            move || wake_tx.send(()).is_ok(),
        );
        full_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut screen = Screen::new(2, 100);
        let mut pointer = PointerInteraction::default();
        while screen.row_text(0).trim_end().len() < PTY_QUEUE_CAPACITY * 2 {
            wake_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("reader or continuation wake");
            io.drain(&mut screen, &mut pointer);
        }
        assert_eq!(
            screen.row_text(0).trim_end(),
            "z".repeat(PTY_QUEUE_CAPACITY * 2)
        );
        finish_tx.send(()).unwrap();
        wake_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("EOF wake");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !io.session.closed_observed {
            io.drain(&mut screen, &mut pointer);
            assert!(
                Instant::now() < deadline,
                "reader disconnect was not observed"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn osc52_resize_barrier_streams_refilling_queue_on_success_and_failure() {
        for interrupt_failure in [false, true] {
            const CHUNKS: usize = 201;
            let generated = Arc::new(AtomicUsize::new(0));
            let processed = Arc::new(AtomicUsize::new(0));
            let published = Arc::new(AtomicUsize::new(0));
            let high_water = Arc::new(AtomicUsize::new(0));
            let (output_tx, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
            let (commands, command_rx) = mpsc::channel();
            let (full_tx, full_rx) = mpsc::channel();
            let (all_queued_tx, all_queued_rx) = mpsc::channel();
            let producer_generated = Arc::clone(&generated);
            let producer_processed = Arc::clone(&processed);
            let producer_published = Arc::clone(&published);
            let producer_high_water = Arc::clone(&high_water);
            let reader = std::thread::spawn(move || {
                for index in 0..CHUNKS {
                    let count = producer_generated.fetch_add(1, Ordering::SeqCst) + 1;
                    producer_high_water.fetch_max(
                        count - producer_processed.load(Ordering::SeqCst),
                        Ordering::SeqCst,
                    );
                    let bytes = if index == CHUNKS - 1 {
                        b"\x1b]0;after\x07\x1b[2;20HX\x1b[6n".to_vec()
                    } else {
                        let mut bytes = b"\x1b]52;c;".to_vec();
                        bytes.extend_from_slice(&b"YWFh".repeat(1022));
                        bytes.push(7);
                        assert_eq!(bytes.len(), 4096);
                        bytes
                    };
                    if count == PTY_QUEUE_CAPACITY + 1 {
                        full_tx.send(()).unwrap();
                    }
                    output_tx.send(ReaderEvent::Bytes(bytes)).unwrap();
                    producer_published.fetch_add(1, Ordering::SeqCst);
                }
                let ReaderCommand::Barrier(epoch) = command_rx.recv().unwrap() else {
                    panic!("missing barrier");
                };
                output_tx.send(ReaderEvent::BarrierAck(epoch - 1)).unwrap();
                output_tx.send(ReaderEvent::BarrierAck(epoch)).unwrap();
                all_queued_tx.send(()).unwrap();
                assert_eq!(command_rx.recv().unwrap(), ReaderCommand::Resume(epoch));
            });
            full_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            #[derive(Clone)]
            struct Writer(Arc<std::sync::Mutex<Vec<u8>>>);
            impl Write for Writer {
                fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                    self.0.lock().unwrap().extend_from_slice(bytes);
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let replies = Arc::new(std::sync::Mutex::new(Vec::new()));
            let mut session = TerminalSession::with_pty(TerminalPty {
                output,
                commands,
                writer: Box::new(Writer(Arc::clone(&replies))),
                reader: Some(reader),
                control: None,
                wake_pending: Arc::new(AtomicBool::new(true)),
                wake: Arc::new(Mutex::new(|| true)),
                next_barrier_epoch: 7,
                test_interrupt: Some(Arc::new(move |_| {
                    if interrupt_failure {
                        anyhow::bail!("injected interrupt failure");
                    }
                    Ok(())
                })),
                barrier_timeout: Duration::from_secs(5),
            });
            let mut parser = TerminalParser::default();
            let mut suppress_scroll_snap = false;
            let mut screen = Screen::new(3, 20);
            let mut pointer = PointerInteraction::default();
            let deadline = Instant::now() + Duration::from_secs(10);
            let result = session.request_resize_barrier(false, Instant::now());
            loop {
                assert!(Instant::now() < deadline);
                match session.next_event() {
                    Ok(Some(ReaderEvent::Bytes(bytes))) => {
                        assert_eq!(screen.cols(), 20, "callback must use pre-resize geometry");
                        TerminalIo::process_reader_bytes_with(
                            &mut parser,
                            &mut suppress_scroll_snap,
                            &mut session,
                            &mut screen,
                            &mut pointer,
                            &bytes,
                        );
                        assert!(format!("{parser:?}").matches("ClipboardWrite(").count() <= 1);
                        let count = processed.fetch_add(1, Ordering::SeqCst) + 1;
                        // Keep the queue full deterministically, including in the final try_recv loop.
                        let required = (count + PTY_QUEUE_CAPACITY).min(CHUNKS);
                        while published.load(Ordering::SeqCst) < required {
                            assert!(Instant::now() < deadline, "producer failed to refill");
                            std::thread::yield_now();
                        }
                        if count == CHUNKS {
                            all_queued_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        }
                    }
                    Ok(Some(ReaderEvent::BarrierAck(epoch))) => {
                        session.acknowledge_resize_barrier(epoch)
                    }
                    Ok(None)
                        if processed.load(Ordering::SeqCst) == CHUNKS
                            && (interrupt_failure || session.resize_barrier_ready().unwrap()) =>
                    {
                        break;
                    }
                    Err(_) if interrupt_failure => break,
                    _ => std::thread::yield_now(),
                }
            }
            assert_eq!(processed.load(Ordering::SeqCst), CHUNKS);
            assert!(
                (PTY_QUEUE_CAPACITY + 1..=PTY_QUEUE_CAPACITY + 2)
                    .contains(&high_water.load(Ordering::SeqCst))
            );
            let events = parser.drain_output_events();
            assert_eq!(events.len(), 2);
            assert!(
                matches!(&events[0], crate::TerminalOutputEvent::ClipboardWrite(write) if write.as_str().len() == 3066)
            );
            assert_eq!(
                events[1],
                crate::TerminalOutputEvent::TitleChanged("after".into())
            );
            assert_eq!(screen.row_text(1).chars().nth(19), Some('X'));
            assert_eq!(*replies.lock().unwrap(), b"\x1b[2;20R");
            assert!(
                !session
                    .pty
                    .as_ref()
                    .unwrap()
                    .wake_pending
                    .load(Ordering::Acquire)
            );
            if interrupt_failure {
                assert!(result.is_err());
            } else {
                result.unwrap();
                let barrier = session.take_resize_barrier().unwrap().unwrap();
                assert_eq!(barrier.epoch, 7);
                assert!(
                    !session
                        .pty
                        .as_ref()
                        .unwrap()
                        .reader
                        .as_ref()
                        .unwrap()
                        .is_finished()
                );
                drop(barrier);
            }
            session
                .pty
                .as_mut()
                .unwrap()
                .reader
                .take()
                .unwrap()
                .join()
                .unwrap();
        }
    }

    #[test]
    fn osc52_streaming_refill_retains_only_queue_plus_current_chunk() {
        struct FloodReader {
            generated: Arc<AtomicUsize>,
            processed: Arc<AtomicUsize>,
            high_water: Arc<AtomicUsize>,
            full: Sender<()>,
            fixture: Vec<u8>,
        }
        impl Read for FloodReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let index = self.generated.load(Ordering::SeqCst);
                if index == 200 {
                    return Ok(0);
                }
                buffer[..self.fixture.len()].copy_from_slice(&self.fixture);
                let generated = self.generated.fetch_add(1, Ordering::SeqCst) + 1;
                let outstanding = generated - self.processed.load(Ordering::SeqCst);
                self.high_water.fetch_max(outstanding, Ordering::SeqCst);
                if generated == PTY_QUEUE_CAPACITY + 1 {
                    self.full.send(()).unwrap();
                }
                Ok(self.fixture.len())
            }
        }
        let generated = Arc::new(AtomicUsize::new(0));
        let processed = Arc::new(AtomicUsize::new(0));
        let high_water = Arc::new(AtomicUsize::new(0));
        let (full_tx, full_rx) = mpsc::channel();
        let mut fixture = b"\x1b]52;c;".to_vec();
        fixture.extend_from_slice(&b"YWFh".repeat(1022));
        fixture.push(7);
        assert_eq!(fixture.len(), 4096);
        let mut io = TerminalIo::new(
            FloodReader {
                generated: Arc::clone(&generated),
                processed: Arc::clone(&processed),
                high_water: Arc::clone(&high_water),
                full: full_tx,
                fixture,
            },
            std::io::sink(),
            None,
            || true,
        );
        // Start with a full queue and a producer blocked while publishing one more chunk.
        full_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut screen = Screen::new(2, 20);
        let mut pointer = PointerInteraction::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match io.session.next_event() {
                Ok(Some(ReaderEvent::Bytes(bytes))) => {
                    io.process_reader_bytes(&mut screen, &mut pointer, &bytes);
                    processed.fetch_add(1, Ordering::SeqCst);
                }
                Ok(Some(ReaderEvent::BarrierAck(_))) => unreachable!(),
                Ok(None) => {
                    assert!(Instant::now() < deadline, "refilling reader did not finish");
                    std::thread::yield_now();
                }
                Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => unreachable!(),
            }
        }
        assert_eq!(processed.load(Ordering::SeqCst), 200);
        // 32 queued chunks + one being parsed + one producer-local/read chunk.
        assert!(
            (PTY_QUEUE_CAPACITY + 1..=PTY_QUEUE_CAPACITY + 2)
                .contains(&high_water.load(Ordering::SeqCst))
        );
        let events = io.drain_output_events();
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], crate::TerminalOutputEvent::ClipboardWrite(write) if write.as_str().len() == 3066)
        );
        io.observe_reader_disconnect(&mut screen);
        assert!(io.is_session_closed());
        assert!(
            !io.session
                .pty
                .as_ref()
                .unwrap()
                .wake_pending
                .load(Ordering::Acquire)
        );
    }

    #[test]
    fn osc52_finished_reader_is_closed_before_ui_observes_disconnect() {
        let mut io = TerminalIo::new(std::io::empty(), std::io::sink(), None, || true);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !io
            .session
            .pty
            .as_ref()
            .unwrap()
            .reader
            .as_ref()
            .unwrap()
            .is_finished()
        {
            assert!(Instant::now() < deadline, "EOF reader did not finish");
            std::thread::yield_now();
        }
        assert!(!io.session.closed_observed);
        assert!(io.is_session_closed());
        io.set_clipboard_delivery(crate::ClipboardDelivery::Latest);
        let mut screen = Screen::new(2, 20);
        let mut pointer = PointerInteraction::default();
        io.feed_pty_output(&mut screen, &mut pointer, b"\x1b]52;c;Zg==\x07");
        assert!(io.drain_output_events().is_empty());
        io.drain(&mut screen, &mut pointer);
        assert!(io.session.closed_observed);
    }

    #[test]
    fn osc52_observed_eof_releases_pending_clipboard_but_preserves_other_events() {
        let mut io = TerminalIo::new(std::io::empty(), std::io::sink(), None, || true);
        // Joining the EOF reader makes disconnect observation deterministic, without sleeps.
        io.session
            .pty
            .as_mut()
            .unwrap()
            .reader
            .take()
            .unwrap()
            .join()
            .unwrap();
        let mut screen = Screen::new(2, 20);
        let mut pointer = PointerInteraction::default();
        assert!(!io.is_session_closed());
        io.feed_pty_output(
            &mut screen,
            &mut pointer,
            b"\x1b]0;before\x07\x1b]52;c;Zg==\x07\x1b]0;after\x07",
        );
        io.drain(&mut screen, &mut pointer);
        assert!(io.is_session_closed());
        assert_eq!(
            io.drain_output_events(),
            vec![
                crate::TerminalOutputEvent::TitleChanged("before".into()),
                crate::TerminalOutputEvent::TitleChanged("after".into()),
            ]
        );
        // A stale delivery reconfiguration cannot resurrect clipboard work on this session.
        io.set_clipboard_delivery(crate::ClipboardDelivery::Latest);
        io.feed_pty_output(
            &mut screen,
            &mut pointer,
            b"\x1b]52;c;Zw==\x07\x1b]0;stale\x07",
        );
        assert_eq!(
            io.drain_output_events(),
            vec![crate::TerminalOutputEvent::TitleChanged("stale".into())]
        );
        io.drain(&mut screen, &mut pointer);
        assert!(io.is_session_closed());
    }

    use std::sync::atomic::AtomicUsize;

    struct ParkedReader {
        started: Option<Sender<()>>,
        dropped: Sender<()>,
    }

    impl Read for ParkedReader {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            if let Some(started) = self.started.take() {
                started.send(()).unwrap();
            }
            std::thread::park();
            Ok(0)
        }
    }

    impl Drop for ParkedReader {
        fn drop(&mut self) {
            let _ = self.dropped.send(());
        }
    }

    struct TrackedWriter(Arc<AtomicUsize>);

    impl Write for TrackedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Drop for TrackedWriter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn hidden_engine_retains_session_endpoints_until_close() {
        let (started_tx, started_rx) = mpsc::channel();
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let writer_drops = Arc::new(AtomicUsize::new(0));
        let hidden = crate::Terminal::new_headless_with_io(
            2,
            8,
            ParkedReader {
                started: Some(started_tx),
                dropped: dropped_tx,
            },
            TrackedWriter(Arc::clone(&writer_drops)),
            || true,
        );
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let mut tabs = vec![hidden, crate::Terminal::new_headless(2, 8)];

        // A different active tab must not close the hidden session.
        tabs[1].put_bytes(b"active");
        assert_eq!(writer_drops.load(Ordering::SeqCst), 0);
        assert!(dropped_rx.recv_timeout(Duration::from_millis(20)).is_err());
        tabs[0].write_pty(b"still alive").unwrap();

        // Close only that tab. The session owns both endpoint lifetimes; its
        // parked reader is unparked and exits rather than leaking a thread.
        tabs.remove(0);
        assert_eq!(writer_drops.load(Ordering::SeqCst), 1);
        dropped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(tabs[0].row_text(0), "active  ");
    }

    #[test]
    fn reader_spawn_failure_releases_unstarted_endpoints() {
        let (started_tx, _started_rx) = mpsc::channel();
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let writer_drops = Arc::new(AtomicUsize::new(0));
        // SAFETY: this injected failure drops the pump without starting it.
        let result = unsafe {
            TerminalPty::try_new_with_spawn(
                ParkedReader {
                    started: Some(started_tx),
                    dropped: dropped_tx,
                },
                TrackedWriter(Arc::clone(&writer_drops)),
                None,
                || true,
                |pump| {
                    drop(pump);
                    Err(std::io::Error::other("injected reader spawn failure"))
                },
            )
        };

        assert!(result.is_err());
        dropped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(writer_drops.load(Ordering::SeqCst), 1);
    }

    fn assert_resize_output_matches_parser(chunks: &[&[u8]], sizes: &[usize]) {
        let mut terminal = crate::Terminal::new_headless(4, 20);
        let mut expected = crate::Terminal::new_headless(4, 20);
        for cols in sizes {
            terminal.resize(4, *cols);
            expected.resize(4, *cols);
        }
        for chunk in chunks {
            terminal
                .io
                .process_reader_bytes(&mut terminal.screen, &mut terminal.pointer, chunk);
        }
        expected.process_output(&chunks.concat());
        assert_eq!(terminal.snapshot(), expected.snapshot());
    }

    #[test]
    fn resize_preserves_application_clear_and_following_output() {
        assert_resize_output_matches_parser(&[b"\x1b[2J\x1b[Hhello", b" world"], &[16]);
    }

    #[test]
    fn resize_preserves_multiple_application_redraws() {
        assert_resize_output_matches_parser(
            &[b"\x1b[?25l\x1b[Hfirst\x1b[?25h\r\n\x1b[?25lsecond\x1b[?25h"],
            &[16, 12],
        );
    }

    #[test]
    fn resize_output_is_independent_of_read_chunk_boundaries() {
        let bytes = b"\x1b[?25l\x1b[Hhello\x1b[?25hNORMAL";
        for chunk_size in 1..=bytes.len() {
            let chunks: Vec<_> = bytes.chunks(chunk_size).collect();
            assert_resize_output_matches_parser(&chunks, &[16, 12]);
        }
    }

    struct CompletedBeforePublishReader {
        state: u8,
    }

    impl Read for CompletedBeforePublishReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            match self.state {
                0 => {
                    self.state = 1;
                    buffer[..3].copy_from_slice(b"old");
                    Ok(3)
                }
                1 => {
                    self.state = 2;
                    buffer[..3].copy_from_slice(b"new");
                    Ok(3)
                }
                _ => Ok(0),
            }
        }
    }

    struct InterruptibleTestReader {
        state: u8,
        blocked: Sender<()>,
        interrupt: Receiver<()>,
    }

    impl Read for InterruptibleTestReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            match self.state {
                0 => {
                    self.state = 1;
                    self.blocked.send(()).unwrap();
                    self.interrupt.recv().unwrap();
                    Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
                }
                1 => {
                    self.state = 2;
                    buffer[..3].copy_from_slice(b"new");
                    Ok(3)
                }
                _ => Ok(0),
            }
        }
    }

    fn spawn_test_pump<R: Read + Send + 'static>(
        reader: R,
    ) -> (Sender<ReaderCommand>, Receiver<ReaderEvent>, JoinHandle<()>) {
        let (output_tx, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let (commands, command_rx) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn(move || {
            pump_reader(reader, output_tx, command_rx, wake_pending, || true);
        });
        (commands, output, thread)
    }

    fn spawn_test_pump_with_after_read<R: Read + Send + 'static, F: FnMut() + Send + 'static>(
        reader: R,
        after_read: F,
    ) -> (Sender<ReaderCommand>, Receiver<ReaderEvent>, JoinHandle<()>) {
        let (output_tx, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let (commands, command_rx) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn(move || {
            pump_reader_with_after_read(
                reader,
                output_tx,
                command_rx,
                wake_pending,
                || true,
                after_read,
            );
        });
        (commands, output, thread)
    }

    #[test]
    fn completed_read_is_published_before_barrier_ack_and_resume() {
        let (completed_tx, completed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let reader = CompletedBeforePublishReader { state: 0 };
        let mut first_read = true;
        let (commands, output, thread) = spawn_test_pump_with_after_read(reader, move || {
            if first_read {
                first_read = false;
                completed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
        });

        completed_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        commands.send(ReaderCommand::Barrier(7)).unwrap();
        release_tx.send(()).unwrap();

        assert!(matches!(
            output.recv_timeout(Duration::from_secs(1)).unwrap(),
            ReaderEvent::Bytes(bytes) if bytes == b"old"
        ));
        assert!(matches!(
            output.recv_timeout(Duration::from_secs(1)).unwrap(),
            ReaderEvent::BarrierAck(7)
        ));
        assert!(output.recv_timeout(Duration::from_millis(20)).is_err());

        commands.send(ReaderCommand::Resume(7)).unwrap();
        assert!(matches!(
            output.recv_timeout(Duration::from_secs(1)).unwrap(),
            ReaderEvent::Bytes(bytes) if bytes == b"new"
        ));
        drop(commands);
        thread.join().unwrap();
    }

    #[test]
    fn interrupted_blocking_read_acknowledges_then_waits_for_resume() {
        let (blocked_tx, blocked_rx) = mpsc::channel();
        let (interrupt_tx, interrupt_rx) = mpsc::channel();
        let reader = InterruptibleTestReader {
            state: 0,
            blocked: blocked_tx,
            interrupt: interrupt_rx,
        };
        let (commands, output, thread) = spawn_test_pump(reader);

        blocked_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        commands.send(ReaderCommand::Barrier(11)).unwrap();
        interrupt_tx.send(()).unwrap();

        assert!(matches!(
            output.recv_timeout(Duration::from_secs(1)).unwrap(),
            ReaderEvent::BarrierAck(11)
        ));
        assert!(output.recv_timeout(Duration::from_millis(20)).is_err());

        commands.send(ReaderCommand::Resume(12)).unwrap();
        assert!(output.recv_timeout(Duration::from_millis(20)).is_err());

        commands.send(ReaderCommand::Resume(11)).unwrap();
        assert!(matches!(
            output.recv_timeout(Duration::from_secs(1)).unwrap(),
            ReaderEvent::Bytes(bytes) if bytes == b"new"
        ));
        drop(commands);
        thread.join().unwrap();
    }
}
