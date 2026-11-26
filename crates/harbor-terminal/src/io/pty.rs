//! PTY reader thread lifecycle, chunk pumping, and barrier command servicing.

use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::JoinHandle,
    time::Duration,
};

use harbor_pty::PtyControl;

use super::chunk::{ReaderBuffers, ReaderChunk, ReaderRecycle};

/// The maximum number of parser events buffered between the blocking PTY reader
/// and the UI thread. Backpressure here bounds memory without blocking UI.
pub(crate) const PTY_QUEUE_CAPACITY: usize = 32;
pub(super) const RESIZE_BARRIER_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) type ReaderInterrupt =
    Arc<dyn Fn(&JoinHandle<()>) -> anyhow::Result<()> + Send + Sync + 'static>;
pub(super) type Wake = Arc<Mutex<dyn Fn() -> bool + Send>>;

#[derive(Debug)]
pub(super) enum ReaderEvent {
    Bytes(ReaderChunk),
    BarrierAck(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReaderCommand {
    Barrier(u64),
    Resume(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderCommandStatus {
    Idle,
    Resumed,
    Disconnected,
}

/// I/O and shutdown resources sharing the terminal's lifetime.
pub(super) struct TerminalPty {
    pub(super) output: Option<Receiver<ReaderEvent>>,
    pub(super) recycle: ReaderRecycle,
    pub(super) commands: Sender<ReaderCommand>,
    pub(super) writer: Box<dyn Write + Send>,
    pub(super) reader: Option<JoinHandle<()>>,
    pub(super) control: Option<PtyControl>,
    pub(super) wake_pending: Arc<AtomicBool>,
    pub(super) wake: Wake,
    pub(super) next_barrier_epoch: u64,
    pub(super) test_interrupt: Option<ReaderInterrupt>,
    pub(super) barrier_timeout: Duration,
}

impl TerminalPty {
    pub(super) fn try_new<R, W>(
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
    pub(super) unsafe fn try_new_with_spawn<R, W>(
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
            output: Some(output),
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
        // Disconnect the receiver before shutdown so a reader blocked in send unblocks.
        // The writer is dropped automatically.
        drop(self.output.take());
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
    pub(super) fn set_test_barrier(
        &mut self,
        interrupt: impl Fn(&JoinHandle<()>) -> anyhow::Result<()> + Send + Sync + 'static,
        timeout: Duration,
    ) {
        self.test_interrupt = Some(Arc::new(interrupt));
        self.barrier_timeout = timeout;
    }
}

pub(super) fn pump_reader<R>(
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

pub(super) fn pump_reader_with_after_read<R>(
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
                    Ok(ReaderCommandStatus::Idle | ReaderCommandStatus::Resumed) => continue,
                    Ok(ReaderCommandStatus::Disconnected) | Err(()) => {
                        tracing::warn!(error = %error, "terminal pty reader stopped after interruption");
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
