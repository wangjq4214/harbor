//! Background owner of the PTY, parser, and mutable terminal model.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::JoinHandle,
};

use harbor_pty::{Pty, PtyReaderStatus, WakeHandler};
use harbor_render::{PtyFacade, TerminalFacade};
use harbor_terminal::Terminal;
use harbor_types::{
    Cell, CursorShape, InputModes, SelectionBounds, TerminalCommand, TerminalSize,
    TerminalSnapshot, TerminalUpdate, TerminalView, UpdateDamage, WorkerStatus,
};
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;

enum PtyMessage {
    Bytes(Vec<u8>),
    Status(PtyReaderStatus),
}

struct Mailbox {
    update: Option<TerminalUpdate>,
    status: WorkerStatus,
}

/// Main-thread handle for the terminal worker.
pub(crate) struct TerminalWorkerClient {
    control_tx: Sender<TerminalCommand>,
    signal_tx: Sender<()>,
    mailbox: Arc<Mutex<Mailbox>>,
    _thread: Option<JoinHandle<()>>,
}

struct NoopWake;

impl WakeHandler for NoopWake {
    fn wake(&self) -> bool {
        true
    }
}

impl TerminalWorkerClient {
    pub(crate) fn start(
        size: TerminalSize,
        event_proxy: EventLoopProxy<AppEvent>,
    ) -> anyhow::Result<Self> {
        let (control_tx, control_rx) = mpsc::channel();
        let (signal_tx, signal_rx) = mpsc::channel();
        let (pty_tx, pty_rx) = mpsc::channel();
        let pty_signal_tx = signal_tx.clone();
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update: None,
            status: WorkerStatus::Ready,
        }));
        let worker_mailbox = Arc::clone(&mailbox);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<anyhow::Result<()>>(0);
        let panic_mailbox = Arc::clone(&mailbox);
        let notifier: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = event_proxy.send_event(AppEvent::WorkerUpdateReady);
        });
        let panic_notifier = Arc::clone(&notifier);
        let thread = std::thread::Builder::new()
            .name("terminal-worker".into())
            .spawn(move || {
                run_worker_catching_panic(&panic_mailbox, panic_notifier.as_ref(), || {
                    worker_main(
                        size,
                        true,
                        notifier,
                        control_rx,
                        signal_rx,
                        pty_signal_tx,
                        pty_rx,
                        pty_tx,
                        worker_mailbox,
                        ready_tx,
                    )
                });
            })?;
        let startup = ready_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("terminal worker exited during startup"))?;
        startup?;

        Ok(Self {
            control_tx,
            signal_tx,
            mailbox,
            _thread: Some(thread),
        })
    }

    pub(crate) fn send(&self, command: TerminalCommand) -> bool {
        if self.control_tx.send(command).is_err() {
            return false;
        }
        self.signal_tx.send(()).is_ok()
    }

    pub(crate) fn shutdown(&self) {
        let _ = self.send(TerminalCommand::Shutdown);
    }

    pub(crate) fn take_update(&self) -> Option<TerminalUpdate> {
        lock(&self.mailbox).update.take()
    }

    pub(crate) fn status(&self) -> WorkerStatus {
        lock(&self.mailbox).status.clone()
    }
}

impl Drop for TerminalWorkerClient {
    fn drop(&mut self) {
        self.shutdown();
        // Deliberately do not join here. PTY shutdown is owned by the worker and
        // must never block the winit event loop or its teardown path.
        let _ = self._thread.take();
    }
}

fn run_worker_catching_panic<F>(mailbox: &Arc<Mutex<Mailbox>>, notifier: &dyn Fn(), worker: F)
where
    F: FnOnce(),
{
    if catch_unwind(AssertUnwindSafe(worker)).is_err() {
        set_status(
            mailbox,
            WorkerStatus::Failed {
                message: "terminal worker panicked".to_owned(),
            },
        );
        notifier();
    }
}
fn worker_main(
    size: TerminalSize,
    start_pty: bool,
    notifier: Arc<dyn Fn() + Send + Sync>,
    control_rx: Receiver<TerminalCommand>,
    signal_rx: Receiver<()>,
    pty_signal_tx: Sender<()>,
    pty_rx: Receiver<PtyMessage>,
    pty_tx: Sender<PtyMessage>,
    mailbox: Arc<Mutex<Mailbox>>,
    ready_tx: mpsc::SyncSender<anyhow::Result<()>>,
) {
    let mut terminal = Terminal::new(size.rows, size.cols);
    let mut revision = 0;
    publish_snapshot(&mut terminal, &mailbox, notifier.as_ref(), revision, false);
    // Readiness is acknowledged only after the PTY has started successfully.

    let mut pty = Pty::new(NoopWake);
    if start_pty {
        let output_tx = pty_tx.clone();
        let status_tx = pty_tx;
        let output_signal = pty_signal_tx.clone();
        let status_signal = pty_signal_tx;
        if let Err(error) = pty.start_with_handlers(
            size,
            move |bytes| {
                output_tx.send(PtyMessage::Bytes(bytes)).is_ok() && output_signal.send(()).is_ok()
            },
            move |status| {
                let _ = status_tx.send(PtyMessage::Status(status));
                let _ = status_signal.send(());
            },
        ) {
            set_status(
                &mailbox,
                WorkerStatus::Failed {
                    message: format!("failed to start pty: {error:#}"),
                },
            );
            notify(notifier.as_ref());
            let _ = ready_tx.send(Err(anyhow::anyhow!("failed to start pty: {error:#}")));
            return;
        }
    }
    let _ = ready_tx.send(Ok(()));

    let mut control_closed = false;
    let mut pty_closed = false;
    loop {
        let mut progressed = false;

        match control_rx.try_recv() {
            Ok(command) => {
                progressed = true;
                if apply_command(
                    command,
                    &mut terminal,
                    &mut pty,
                    &mailbox,
                    notifier.as_ref(),
                    &mut revision,
                ) {
                    break;
                }
            }
            Err(TryRecvError::Disconnected) => control_closed = true,
            Err(TryRecvError::Empty) => {}
        }

        match pty_rx.try_recv() {
            Ok(PtyMessage::Bytes(bytes)) => {
                progressed = true;
                set_status(&mailbox, WorkerStatus::Processing);
                terminal.process_output(&bytes);
                publish_snapshot(
                    &mut terminal,
                    &mailbox,
                    notifier.as_ref(),
                    revision.saturating_add(1),
                    true,
                );
                revision = revision.saturating_add(1);
                set_status(&mailbox, WorkerStatus::Idle);
                notify(notifier.as_ref());
            }
            Ok(PtyMessage::Status(PtyReaderStatus::Eof)) => {
                progressed = true;
                set_status(&mailbox, WorkerStatus::Stopped);
                notify(notifier.as_ref());
                break;
            }
            Ok(PtyMessage::Status(PtyReaderStatus::Error(error))) => {
                progressed = true;
                set_status(&mailbox, WorkerStatus::Failed { message: error });
                notify(notifier.as_ref());
            }
            Err(TryRecvError::Disconnected) => pty_closed = true,
            Err(TryRecvError::Empty) => {}
        }

        if control_closed && pty_closed {
            break;
        }
        if progressed {
            continue;
        }
        match signal_rx.recv() {
            Ok(()) => {}
            Err(_) => break,
        }
    }

    set_status(&mailbox, WorkerStatus::Stopped);
    notify(notifier.as_ref());
}

fn apply_command(
    command: TerminalCommand,
    terminal: &mut Terminal,
    pty: &mut Pty,
    mailbox: &Arc<Mutex<Mailbox>>,
    notifier: &dyn Fn(),
    revision: &mut u64,
) -> bool {
    match command {
        TerminalCommand::PtyOutputBytes(bytes) => {
            set_status(mailbox, WorkerStatus::Processing);
            terminal.process_output(&bytes);
            *revision = revision.saturating_add(1);
            publish_snapshot(terminal, mailbox, notifier, *revision, true);
            set_status(mailbox, WorkerStatus::Idle);
            notify(notifier);
        }
        TerminalCommand::WritePtyInput(bytes) => pty.write(&bytes),
        TerminalCommand::Resize(size) => {
            if terminal.resize_terminal_if_changed(size) {
                pty.resize(size);
                *revision = revision.saturating_add(1);
                publish_snapshot(terminal, mailbox, notifier, *revision, true);
                notify(notifier);
            }
        }
        TerminalCommand::ScrollViewport { rows } => {
            if rows >= 0 {
                terminal.scroll_viewport_down(rows as usize);
            } else {
                terminal.scroll_viewport_up(rows.unsigned_abs());
            }
            *revision = revision.saturating_add(1);
            publish_snapshot(terminal, mailbox, notifier, *revision, true);
            notify(notifier);
        }
        TerminalCommand::ScrollToTop => {
            terminal.scroll_viewport_to_top();
            *revision = revision.saturating_add(1);
            publish_snapshot(terminal, mailbox, notifier, *revision, true);
            notify(notifier);
        }
        TerminalCommand::ScrollToBottom => {
            terminal.scroll_viewport_to_bottom();
            *revision = revision.saturating_add(1);
            publish_snapshot(terminal, mailbox, notifier, *revision, true);
            notify(notifier);
        }
        TerminalCommand::SetSelectionDragActive(active) => {
            terminal.set_suppress_scroll_snap(active);
        }
        TerminalCommand::CopySelection { .. } | TerminalCommand::RequestSnapshot { .. } => {}
        TerminalCommand::Shutdown => return true,
    }
    false
}

fn publish_snapshot(
    terminal: &mut Terminal,
    mailbox: &Arc<Mutex<Mailbox>>,
    notifier: &dyn Fn(),
    revision: u64,
    overwrite_is_gap: bool,
) {
    let snapshot = terminal.snapshot();
    let mut update = TerminalUpdate::from_snapshot(revision, snapshot);
    let mut state = lock(mailbox);
    if overwrite_is_gap && state.update.is_some() {
        update.damage = UpdateDamage::FullUpload;
    }
    state.update = Some(update);
    drop(state);
    terminal.clear_screen_dirty();
    notify(notifier);
}

fn set_status(mailbox: &Arc<Mutex<Mailbox>>, status: WorkerStatus) {
    lock(mailbox).status = status;
}

fn notify(notifier: &dyn Fn()) {
    notifier();
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn empty_snapshot(rows: usize, cols: usize) -> TerminalSnapshot {
    TerminalSnapshot {
        rows,
        cols,
        cells: vec![Cell::default(); rows.saturating_mul(cols)],
        cursor_x: 0,
        cursor_y: 0,
        cursor_visible: true,
        cursor_blink: false,
        cursor_shape: CursorShape::default(),
        scroll_count: 0,
        view_offset: 0,
        history_start: 0,
        is_alt: false,
        input_modes: InputModes::default(),
        dirty_ranges: Vec::new(),
    }
}

/// UI-side read-only view over the most recently published worker snapshot.
pub(crate) struct WorkerUiFacade<'a> {
    snapshot: &'a TerminalSnapshot,
    worker: &'a TerminalWorkerClient,
    view: SnapshotView<'a>,
}

struct SnapshotView<'a> {
    snapshot: &'a TerminalSnapshot,
}

impl<'a> WorkerUiFacade<'a> {
    pub(crate) fn new(snapshot: &'a TerminalSnapshot, worker: &'a TerminalWorkerClient) -> Self {
        Self {
            snapshot,
            worker,
            view: SnapshotView { snapshot },
        }
    }
}

impl TerminalView for SnapshotView<'_> {
    fn rows(&self) -> usize {
        self.snapshot.rows
    }

    fn cols(&self) -> usize {
        self.snapshot.cols
    }

    fn scroll_count(&self) -> usize {
        self.snapshot.scroll_count
    }

    fn view_offset(&self) -> usize {
        self.snapshot.view_offset
    }

    fn history_start(&self) -> u64 {
        self.snapshot.history_start
    }

    fn cursor_visible(&self) -> bool {
        self.snapshot.cursor_visible
    }

    fn cursor_blink(&self) -> bool {
        self.snapshot.cursor_blink
    }

    fn input_modes(&self) -> InputModes {
        self.snapshot.input_modes
    }

    fn cell_at_generation(&self, generation: u64, col: usize) -> Option<&Cell> {
        let visible_start = self.snapshot.history_start
            + self
                .snapshot
                .scroll_count
                .saturating_sub(self.snapshot.view_offset) as u64;
        let row = generation.checked_sub(visible_start)? as usize;
        if row >= self.snapshot.rows || col >= self.snapshot.cols {
            return None;
        }
        self.snapshot.cells.get(row * self.snapshot.cols + col)
    }
}

impl TerminalFacade for WorkerUiFacade<'_> {
    fn view(&self) -> &dyn TerminalView {
        &self.view
    }

    fn render_snapshot(&self) -> harbor_types::RenderSnapshot {
        self.snapshot.render_snapshot()
    }

    fn selected_text(&self, bounds: SelectionBounds) -> String {
        let mut text = String::new();
        for generation in bounds.start_row..=bounds.end_row {
            let start = if generation == bounds.start_row {
                bounds.start_col
            } else {
                0
            };
            let end = if generation == bounds.end_row {
                bounds.end_col
            } else {
                self.snapshot.cols.saturating_sub(1)
            };
            let row_start = text.len();
            for col in start..=end {
                if let Some(cell) = self.view.cell_at_generation(generation, col)
                    && !cell.wide_continuation
                {
                    text.push(cell.ch);
                }
            }
            let trimmed = text[row_start..].trim_end().len();
            text.truncate(row_start + trimmed);
            if generation < bounds.end_row {
                text.push('\n');
            }
        }
        text
    }

    fn scroll_viewport_up(&mut self, n: usize) {
        let _ = self.worker.send(TerminalCommand::ScrollViewport {
            rows: -(n as isize),
        });
    }

    fn scroll_viewport_down(&mut self, n: usize) {
        let _ = self
            .worker
            .send(TerminalCommand::ScrollViewport { rows: n as isize });
    }

    fn scroll_viewport_to_top(&mut self) {
        let _ = self.worker.send(TerminalCommand::ScrollToTop);
    }

    fn scroll_viewport_to_bottom(&mut self) {
        let _ = self.worker.send(TerminalCommand::ScrollToBottom);
    }

    fn set_suppress_scroll_snap(&mut self, active: bool) {
        let _ = self
            .worker
            .send(TerminalCommand::SetSelectionDragActive(active));
    }

    fn is_alt_screen(&self) -> bool {
        self.snapshot.is_alt
    }
}

impl PtyFacade for WorkerUiFacade<'_> {
    fn write(&mut self, bytes: &[u8]) {
        let _ = self
            .worker
            .send(TerminalCommand::WritePtyInput(bytes.to_vec()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn worker_consumes_ordered_output_and_publishes_revisioned_state() {
        let (control_tx, control_rx) = mpsc::channel();
        let (signal_tx, signal_rx) = mpsc::channel();
        let (pty_tx, pty_rx) = mpsc::channel();
        let pty_signal_tx = signal_tx.clone();
        let test_pty_tx = pty_tx.clone();
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update: None,
            status: WorkerStatus::Ready,
        }));
        let (wake_tx, wake_rx) = mpsc::channel();
        let notifier: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = wake_tx.send(());
        });
        let (ready_tx, ready_rx) = mpsc::sync_channel::<anyhow::Result<()>>(0);
        let worker_mailbox = Arc::clone(&mailbox);
        let thread = std::thread::spawn(move || {
            worker_main(
                TerminalSize { rows: 1, cols: 4 },
                false,
                notifier,
                control_rx,
                signal_rx,
                pty_signal_tx,
                pty_rx,
                pty_tx,
                worker_mailbox,
                ready_tx,
            );
        });

        ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker publishes the initial snapshot")
            .expect("worker startup succeeds");
        let initial = lock(&mailbox)
            .update
            .take()
            .expect("initial snapshot is available");
        assert_eq!(initial.revision, 0);

        test_pty_tx.send(PtyMessage::Bytes(b"ab".to_vec())).unwrap();
        signal_tx.send(()).unwrap();
        test_pty_tx.send(PtyMessage::Bytes(b"cd".to_vec())).unwrap();
        signal_tx.send(()).unwrap();

        let latest = {
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            loop {
                if lock(&mailbox)
                    .update
                    .as_ref()
                    .is_some_and(|update| update.revision >= 2)
                {
                    break lock(&mailbox)
                        .update
                        .take()
                        .expect("latest update remains in mailbox");
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "worker did not publish the second revision"
                );
                let _ = wake_rx.recv_timeout(Duration::from_millis(10));
            }
        };
        assert_eq!(
            latest
                .snapshot
                .cells
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>(),
            "abcd"
        );
        assert_eq!(latest.damage, UpdateDamage::FullUpload);

        control_tx.send(TerminalCommand::Shutdown).unwrap();
        signal_tx.send(()).unwrap();
        thread.join().unwrap();
        assert_eq!(lock(&mailbox).status, WorkerStatus::Stopped);
    }

    #[test]
    fn worker_panic_is_reported_without_rethrowing_to_caller() {
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update: None,
            status: WorkerStatus::Ready,
        }));
        let notifier = || {};
        run_worker_catching_panic(&mailbox, &notifier, || panic!("parser panic"));
        assert_eq!(
            lock(&mailbox).status,
            WorkerStatus::Failed {
                message: "terminal worker panicked".to_owned()
            }
        );
    }
}
