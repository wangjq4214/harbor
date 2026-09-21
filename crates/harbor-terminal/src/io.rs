//! PTY I/O and ANSI parsing — extracted from `Terminal` to separate
//! I/O lifecycle from screen state and GPU rendering.

use std::{
    borrow::Cow,
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
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

const RESIZE_BARRIER_TIMEOUT: Duration = Duration::from_secs(2);
const BARRIER_INTERRUPT_RETRY: Duration = Duration::from_millis(10);

type ReaderInterrupt = Arc<dyn Fn(&JoinHandle<()>) -> anyhow::Result<()> + Send + Sync + 'static>;
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

pub(crate) struct ResizeBarrier {
    commands: Sender<ReaderCommand>,
    epoch: u64,
}

impl Drop for ResizeBarrier {
    fn drop(&mut self) {
        let _ = self.commands.send(ReaderCommand::Resume(self.epoch));
    }
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
    next_barrier_epoch: u64,
    test_interrupt: Option<ReaderInterrupt>,
    barrier_timeout: Duration,
}

impl TerminalPty {
    fn new<R, W>(
        reader: R,
        writer: W,
        control: Option<PtyControl>,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let (output_tx, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let (commands, reader_commands) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let reader_wake_pending = Arc::clone(&wake_pending);
        let reader = std::thread::Builder::new()
            .name("harbor-terminal-reader".into())
            .spawn(move || {
                pump_reader(
                    reader,
                    output_tx,
                    reader_commands,
                    reader_wake_pending,
                    wake,
                )
            })
            .expect("failed to start terminal PTY reader");
        Self {
            output,
            commands,
            writer: Box::new(writer),
            reader: Some(reader),
            control,
            wake_pending,
            next_barrier_epoch: 1,
            test_interrupt: None,
            barrier_timeout: RESIZE_BARRIER_TIMEOUT,
        }
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

#[cfg(test)]
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

// ── ConPTY Redraw Filter ──────────────────────────────────────────────

/// Filters unsolicited full-screen redraw bursts emitted by Windows ConPTY on resize.
///
/// Because Harbor performs client-side reflow and maintains its own scrollback ring buffer,
/// ConPTY's post-resize redraw (which homes the cursor with `\x1b[H` and rewrites lines with `\r\n`)
/// would corrupt the reflowed layout and scroll lines into scrollback as hard linebreaks.
#[derive(Debug)]
pub(crate) struct ConptyRedrawFilter {
    state: ConptyRedrawState,
    tail_buffer: Vec<u8>,
    /// Resize notifications not yet matched to the start of a redraw burst.
    pending_redraws: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConptyRedrawState {
    Idle,
    Armed { armed_at: Instant },
    Suppressing { started_at: Instant },
}

impl ConptyRedrawFilter {
    pub(crate) fn new() -> Self {
        Self {
            state: ConptyRedrawState::Idle,
            tail_buffer: Vec::new(),
            pending_redraws: 0,
        }
    }

    pub(crate) fn arm(&mut self) {
        self.pending_redraws = self.pending_redraws.saturating_add(1);
        let now = Instant::now();
        match self.state {
            ConptyRedrawState::Idle => {
                self.tail_buffer.clear();
                self.state = ConptyRedrawState::Armed { armed_at: now };
            }
            ConptyRedrawState::Armed { .. } => {
                self.state = ConptyRedrawState::Armed { armed_at: now };
            }
            ConptyRedrawState::Suppressing { .. } => {
                // A rapid follow-up resize must not expose the unfinished prior redraw.
                self.state = ConptyRedrawState::Suppressing { started_at: now };
            }
        }
    }

    pub(crate) fn filter<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        const TIMEOUT: Duration = Duration::from_millis(500);

        match self.state {
            ConptyRedrawState::Idle => Cow::Borrowed(bytes),
            ConptyRedrawState::Armed { armed_at } => {
                if armed_at.elapsed() > TIMEOUT {
                    self.state = ConptyRedrawState::Idle;
                    self.pending_redraws = 0;
                    if self.tail_buffer.is_empty() {
                        return Cow::Borrowed(bytes);
                    }
                    let mut released = std::mem::take(&mut self.tail_buffer);
                    released.extend_from_slice(bytes);
                    return Cow::Owned(released);
                }
                self.filter_armed(bytes)
            }
            ConptyRedrawState::Suppressing { started_at } => {
                if started_at.elapsed() > TIMEOUT {
                    self.state = ConptyRedrawState::Idle;
                    self.pending_redraws = 0;
                    self.tail_buffer.clear();
                    return Cow::Borrowed(bytes);
                }
                self.filter_suppressing(bytes)
            }
        }
    }

    fn filter_armed<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        if self.tail_buffer.is_empty() {
            if let Some(start) = conpty_redraw_start(bytes) {
                self.pending_redraws = self.pending_redraws.saturating_sub(1);
                self.state = ConptyRedrawState::Suppressing {
                    started_at: Instant::now(),
                };
                let resumed = self.filter_suppressing(&bytes[start..]);
                if start == 0 {
                    return resumed;
                }
                let mut output = bytes[..start].to_vec();
                output.extend_from_slice(resumed.as_ref());
                return Cow::Owned(output);
            }

            let keep = possible_redraw_start_suffix(bytes);
            if keep == 0 {
                return Cow::Borrowed(bytes);
            }
            self.tail_buffer
                .extend_from_slice(&bytes[bytes.len() - keep..]);
            return Cow::Borrowed(&bytes[..bytes.len() - keep]);
        }

        let mut combined = std::mem::take(&mut self.tail_buffer);
        combined.extend_from_slice(bytes);
        if let Some(start) = conpty_redraw_start(&combined) {
            self.pending_redraws = self.pending_redraws.saturating_sub(1);
            self.state = ConptyRedrawState::Suppressing {
                started_at: Instant::now(),
            };
            let resumed = self.filter_suppressing(&combined[start..]);
            let mut output = combined[..start].to_vec();
            output.extend_from_slice(resumed.as_ref());
            return Cow::Owned(output);
        }

        let keep = possible_redraw_start_suffix(&combined);
        let emit_end = combined.len() - keep;
        self.tail_buffer.extend_from_slice(&combined[emit_end..]);
        Cow::Owned(combined[..emit_end].to_vec())
    }

    fn filter_suppressing<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        const CURSOR_SHOW: &[u8] = b"\x1b[?25h";

        // Check if CURSOR_SHOW was split across tail_buffer and bytes
        if !self.tail_buffer.is_empty() {
            let mut combined = self.tail_buffer.clone();
            let check_len = (CURSOR_SHOW.len() - 1).min(bytes.len());
            combined.extend_from_slice(&bytes[..check_len]);
            if let Some(pos) = find_subsequence(&combined, CURSOR_SHOW) {
                let match_end = pos + CURSOR_SHOW.len();
                let consumed_from_bytes = match_end.saturating_sub(self.tail_buffer.len());
                self.tail_buffer.clear();
                return self.finish_suppression(&bytes[consumed_from_bytes.min(bytes.len())..]);
            }
            self.tail_buffer.clear();
        }

        if let Some(pos) = find_subsequence(bytes, CURSOR_SHOW) {
            let resume_idx = pos + CURSOR_SHOW.len();
            self.tail_buffer.clear();
            self.finish_suppression(&bytes[resume_idx..])
        } else {
            let keep = (CURSOR_SHOW.len() - 1).min(bytes.len());
            self.tail_buffer.clear();
            self.tail_buffer
                .extend_from_slice(&bytes[bytes.len() - keep..]);
            Cow::Borrowed(&[])
        }
    }

    fn finish_suppression<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        if self.pending_redraws == 0 {
            self.state = ConptyRedrawState::Idle;
            return Cow::Borrowed(bytes);
        }
        self.state = ConptyRedrawState::Armed {
            armed_at: Instant::now(),
        };
        self.filter_armed(bytes)
    }
}

const CONPTY_REDRAW_STARTS: [&[u8]; 7] = [
    b"\x1b[H",
    b"\x1b[1;1H",
    b"\x1b[;H",
    b"\x1b[0;0H",
    b"\x1b[1H",
    b"\x1b[2J",
    b"\x1b[?25l",
];

fn conpty_redraw_start(bytes: &[u8]) -> Option<usize> {
    let marker_start = CONPTY_REDRAW_STARTS
        .iter()
        .filter_map(|marker| find_subsequence(bytes, marker))
        .min()?;
    let mut prefix = &bytes[..marker_start];
    while let Some(rest) = strip_leading_redraw_prefix(prefix) {
        prefix = rest;
    }
    Some(if prefix.is_empty() { 0 } else { marker_start })
}

fn possible_redraw_start_suffix(bytes: &[u8]) -> usize {
    let max_prefix = CONPTY_REDRAW_STARTS
        .iter()
        .map(|marker| marker.len().saturating_sub(1))
        .max()
        .unwrap_or(0)
        .min(bytes.len());
    (1..=max_prefix)
        .rev()
        .find(|&length| {
            let suffix = &bytes[bytes.len() - length..];
            CONPTY_REDRAW_STARTS
                .iter()
                .any(|marker| marker.starts_with(suffix))
        })
        .unwrap_or(0)
}

fn strip_leading_redraw_prefix(bytes: &[u8]) -> Option<&[u8]> {
    if let Some(rest) = bytes.strip_prefix(b"\x1b[0m") {
        Some(rest)
    } else if let Some(rest) = bytes.strip_prefix(b"\x1b[m") {
        Some(rest)
    } else if let Some(rest) = bytes.strip_prefix(b"\r") {
        Some(rest)
    } else if let Some(rest) = bytes.strip_prefix(b"\n") {
        Some(rest)
    } else {
        None
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ── TerminalIo ────────────────────────────────────────────────────────

/// PTY I/O and ANSI/VT parsing — owns the parser, PTY endpoints, and input encoding.
///
/// Created once per terminal instance. The PTY is optional (absent in headless/test mode).
pub(crate) struct TerminalIo {
    /// Incremental ANSI/VT parser.
    parser: TerminalParser,
    /// PTY reader, writer, and shutdown owner. None in headless mode.
    pty: Option<TerminalPty>,
    /// When true, `process_output` skips the scroll-to-bottom snap.
    suppress_scroll_snap: bool,
    /// Set on the first observed reader/session disconnect so close clears once.
    session_closed: bool,
    /// Filter for ConPTY unsolicited full-screen redraw burst on resize.
    conpty_filter: ConptyRedrawFilter,
}

impl TerminalIo {
    /// Creates a TerminalIo with PTY endpoints for a live terminal.
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
        Self::with_pty(Some(TerminalPty::new(
            pty_read,
            pty_write,
            pty_control,
            wake,
        )))
    }
    #[cfg(test)]
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
        let mut pty = TerminalPty::new(pty_read, pty_write, None, wake);
        pty.set_test_barrier(interrupt, timeout);
        Self::with_pty(Some(pty))
    }

    /// Creates a headless TerminalIo without PTY resources (for tests).
    pub(crate) fn new_headless() -> Self {
        Self::with_pty(None)
    }

    fn with_pty(pty: Option<TerminalPty>) -> Self {
        Self {
            parser: TerminalParser::default(),
            pty,
            suppress_scroll_snap: false,
            session_closed: false,
            conpty_filter: ConptyRedrawFilter::new(),
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
            let before_projection = (pointer.has_selection_state()
                || screen.requires_anchor_baseline())
            .then(|| screen.content_projection().ok())
            .flatten();
            let result = self.parser.put_bytes(screen, remaining);
            remaining = &remaining[result.consumed..];
            match screen.finish_anchor_mutations(before_projection.as_ref()) {
                Ok((mutations, projection)) => {
                    pointer.reconcile_selection(&mutations, &projection);
                }
                Err(error) => {
                    tracing::error!(generation = error.generation, column = error.column, kind = ?error.kind, "content anchor reconciliation failed");
                    pointer.clear();
                }
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

    // ── PTY I/O ───────────────────────────────────────────────────────

    /// Drains all reader-thread output in FIFO order into the terminal parser.
    ///
    /// A wake remains pending until this has observed an empty queue. The second
    /// receive after clearing the flag closes the producer/consumer race: bytes
    /// queued just before the clear are consumed here, while later bytes post a
    /// fresh wake.
    pub(crate) fn drain(&mut self, screen: &mut Screen, pointer: &mut PointerInteraction) -> bool {
        let mut events = Vec::new();
        let mut disconnected = false;
        {
            let Some(pty) = self.pty.as_ref() else {
                return false;
            };
            loop {
                match pty.output.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => {
                        pty.wake_pending.store(false, Ordering::Release);
                        match pty.output.try_recv() {
                            Ok(event) => events.push(event),
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        pty.wake_pending.store(false, Ordering::Release);
                        while let Ok(event) = pty.output.try_recv() {
                            events.push(event);
                        }
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut consumed_bytes = false;
        for event in events {
            if let ReaderEvent::Bytes(bytes) = event {
                consumed_bytes = true;
                self.process_reader_bytes(screen, pointer, &bytes);
            }
        }
        if disconnected {
            self.observe_reader_disconnect(screen);
        }
        consumed_bytes
    }

    /// Pauses a live PTY reader at an acknowledged event-stream boundary.
    pub(crate) fn acquire_resize_barrier(
        &mut self,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
    ) -> anyhow::Result<Option<ResizeBarrier>> {
        let Some(pty) = self.pty.as_mut() else {
            return Ok(None);
        };
        if pty.control.is_none() && pty.test_interrupt.is_none() {
            anyhow::bail!(
                "cannot resize a terminal with an active reader but no interrupt capability"
            );
        }

        let epoch = pty.next_barrier_epoch;
        pty.next_barrier_epoch = pty
            .next_barrier_epoch
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("PTY resize barrier epoch exhausted"))?;
        let commands = pty.commands.clone();
        commands
            .send(ReaderCommand::Barrier(epoch))
            .map_err(|_| anyhow::anyhow!("terminal reader stopped before resize barrier"))?;
        let barrier = ResizeBarrier { commands, epoch };

        let deadline = Instant::now() + pty.barrier_timeout;
        let mut acquisition = self.interrupt_reader_for_barrier();
        let mut chunks = Vec::new();
        let mut acknowledged = false;
        let mut disconnected = false;
        while acquisition.is_ok() && !acknowledged {
            let now = Instant::now();
            if now >= deadline {
                acquisition = Err(anyhow::anyhow!(
                    "timed out waiting for PTY resize barrier {epoch}"
                ));
                break;
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(BARRIER_INTERRUPT_RETRY);
            let event = {
                let pty = self
                    .pty
                    .as_ref()
                    .expect("active PTY must remain owned during resize barrier");
                pty.output.recv_timeout(wait)
            };
            match event {
                Ok(ReaderEvent::Bytes(bytes)) => chunks.push(bytes),
                Ok(ReaderEvent::BarrierAck(ack_epoch)) if ack_epoch == epoch => {
                    acknowledged = true;
                    if let Some(pty) = self.pty.as_ref() {
                        pty.wake_pending.store(false, Ordering::Release);
                    }
                }
                Ok(ReaderEvent::BarrierAck(_)) => {}
                Err(RecvTimeoutError::Timeout) => {
                    acquisition = self.interrupt_reader_for_barrier();
                }
                Err(RecvTimeoutError::Disconnected) => {
                    disconnected = true;
                    acquisition = Err(anyhow::anyhow!(
                        "terminal reader stopped before resize barrier {epoch}"
                    ));
                }
            }
        }

        // Close the wake flag/queue race before returning on either success or failure.
        // A failed interrupt can leave the reader publishing one final pre-barrier chunk;
        // consume it under the unchanged old geometry or let a later publish post a fresh wake.
        if let Some(pty) = self.pty.as_ref() {
            pty.wake_pending.store(false, Ordering::Release);
            loop {
                match pty.output.try_recv() {
                    Ok(ReaderEvent::Bytes(bytes)) => chunks.push(bytes),
                    Ok(ReaderEvent::BarrierAck(ack_epoch)) if ack_epoch == epoch => {
                        acknowledged = true;
                    }
                    Ok(ReaderEvent::BarrierAck(_)) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        // Parsing and synchronous terminal replies happen only after the reader-side
        // wait has completed, so they cannot extend the barrier acquisition deadline.
        for bytes in chunks {
            self.process_reader_bytes(screen, pointer, &bytes);
        }
        if disconnected {
            self.observe_reader_disconnect(screen);
        }
        acquisition?;
        debug_assert!(acknowledged);
        Ok(Some(barrier))
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

    pub(crate) fn arm_conpty_resize_redraw_filter(&mut self) {
        if cfg!(windows) || cfg!(test) {
            self.conpty_filter.arm();
        }
    }

    fn process_reader_bytes(
        &mut self,
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        bytes: &[u8],
    ) {
        let filtered = self.conpty_filter.filter(bytes);
        if !filtered.is_empty() {
            self.feed_pty_output_snapped(screen, pointer, filtered.as_ref());
            let replies = screen.drain_replies();
            if !replies.is_empty()
                && let Err(error) = self.write_pty(&replies)
            {
                tracing::warn!(error = %error, "failed to write terminal replies to pty");
            }
        }
    }

    fn observe_reader_disconnect(&mut self, screen: &mut Screen) {
        if !self.session_closed {
            self.session_closed = true;
            screen.clear_synchronized_output();
        }
    }

    /// Writes bytes synchronously to the terminal's PTY input endpoint.
    pub(crate) fn write_pty(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let Some(pty) = self.pty.as_mut() else {
            anyhow::bail!("terminal has no active pty");
        };
        pty.writer.write_all(bytes).map_err(Into::into)
    }

    /// Resizes the PTY to the given terminal dimensions.
    pub(crate) fn resize_pty(&mut self, size: TerminalSize) -> anyhow::Result<()> {
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

    // ── event handling ────────────────────────────────────────────────

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

        if let TerminalEvent::Focus(focus) = &event {
            if screen.observe_focus(*focus) {
                self.write_pty(input::encode_focus(*focus))?;
                return Ok(true);
            }
            return Ok(false);
        }

        if matches!(&event, TerminalEvent::Pointer(_))
            && screen.input_modes().mouse_tracking != crate::model::MouseTrackingMode::Disabled
        {
            if let Some(bytes) = input::encode(&event, screen.input_modes()) {
                self.write_pty(&bytes)?;
                return Ok(true);
            }
            // A supported tracking mode without SGR encoding is intentionally
            // consumed rather than falling back to local selection/scrollback.
            return Ok(false);
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

    #[test]
    fn conpty_filter_passes_through_when_idle() {
        let mut filter = ConptyRedrawFilter::new();
        let data = b"hello world\r\n";
        assert_eq!(filter.filter(data).as_ref(), data);
    }

    #[test]
    fn conpty_filter_suppresses_redraw_in_single_chunk() {
        let mut filter = ConptyRedrawFilter::new();
        filter.arm();
        let redraw = b"\x1b[?25l\x1b[HMicrosoft Windows [Version 10.0]\r\n(c) Microsoft Corporation.\r\n\x1b[?25h";
        assert_eq!(filter.filter(redraw).as_ref(), b"");
        // Filter is now disarmed back to Idle
        let normal_data = b"subsequent user input";
        assert_eq!(filter.filter(normal_data).as_ref(), normal_data);
    }

    #[test]
    fn conpty_filter_preserves_post_redraw_bytes_in_same_chunk() {
        let mut filter = ConptyRedrawFilter::new();
        filter.arm();
        let data = b"\x1b[?25l\x1b[1;1HMicrosoft Windows\r\n\x1b[?25hprompt> ";
        assert_eq!(filter.filter(data).as_ref(), b"prompt> ");
        assert_eq!(filter.state, ConptyRedrawState::Idle);
    }

    #[test]
    fn conpty_filter_suppresses_across_multiple_chunks() {
        let mut filter = ConptyRedrawFilter::new();
        filter.arm();
        let chunk1 = b"\x1b[?25l\x1b[Hchunk one of redraw\r\n";
        assert_eq!(filter.filter(chunk1).as_ref(), b"");
        assert!(matches!(
            filter.state,
            ConptyRedrawState::Suppressing { .. }
        ));

        let chunk2 = b"chunk two of redraw\r\n\x1b[?25hnormal output";
        assert_eq!(filter.filter(chunk2).as_ref(), b"normal output");
        assert_eq!(filter.state, ConptyRedrawState::Idle);
    }

    #[test]
    fn conpty_filter_handles_split_cursor_show_marker() {
        let mut filter = ConptyRedrawFilter::new();
        filter.arm();
        // CURSOR_SHOW is \x1b[?25h (6 bytes). Split between \x1b[?25 and h.
        let chunk1 = b"\x1b[?25l\x1b[Hpart1\x1b[?25";
        assert_eq!(filter.filter(chunk1).as_ref(), b"");

        let chunk2 = b"hafter_split";
        assert_eq!(filter.filter(chunk2).as_ref(), b"after_split");
        assert_eq!(filter.state, ConptyRedrawState::Idle);
    }

    #[test]
    fn conpty_filter_remains_armed_after_non_redraw_output() {
        let mut filter = ConptyRedrawFilter::new();
        filter.arm();
        let normal = b"C:\\Users> ";
        assert_eq!(filter.filter(normal).as_ref(), normal);
        assert!(matches!(filter.state, ConptyRedrawState::Armed { .. }));

        let redraw = b"\x1b[?25l\x1b[Hredraw\x1b[?25h";
        assert_eq!(filter.filter(redraw).as_ref(), b"");
        assert_eq!(filter.state, ConptyRedrawState::Idle);
    }

    #[test]
    fn conpty_filter_handles_split_redraw_start_marker() {
        let mut filter = ConptyRedrawFilter::new();
        filter.arm();

        assert_eq!(filter.filter(b"ordinary\x1b[?2").as_ref(), b"ordinary");
        assert!(matches!(filter.state, ConptyRedrawState::Armed { .. }));
        assert_eq!(
            filter.filter(b"5l\x1b[Hredraw\x1b[?25hafter").as_ref(),
            b"after"
        );
        assert_eq!(filter.state, ConptyRedrawState::Idle);
    }

    #[test]
    fn conpty_filter_preserves_suppression_across_overlapping_resizes() {
        let mut filter = ConptyRedrawFilter::new();
        filter.arm();
        assert_eq!(filter.filter(b"\x1b[?25l\x1b[Hfirst redraw").as_ref(), b"");

        filter.arm();
        assert!(matches!(
            filter.state,
            ConptyRedrawState::Suppressing { .. }
        ));
        assert_eq!(
            filter
                .filter(b"\x1b[?25h\x1b[?25l\x1b[Hsecond redraw\x1b[?25hnormal")
                .as_ref(),
            b"normal"
        );
        assert_eq!(filter.state, ConptyRedrawState::Idle);
    }

    #[test]
    fn conpty_filter_does_not_suppress_cls_when_idle() {
        let mut filter = ConptyRedrawFilter::new();
        let cls = b"\x1b[2J\x1b[H";
        assert_eq!(filter.filter(cls).as_ref(), cls);
    }
}
