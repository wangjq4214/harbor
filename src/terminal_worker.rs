//! Background owner of the PTY, parser, and mutable terminal model.

use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender, TryRecvError},
    },
    thread::JoinHandle,
    time::Instant,
};

use harbor_pty::{Pty, PtyReaderStatus, WakeHandler};
use harbor_render::{RenderMetrics, TerminalFacade};
use harbor_terminal::Terminal;
use harbor_types::{
    Cell, CopySelectionResult, CursorShape, InputModes, SelectionBounds, TerminalCommand,
    TerminalSize, TerminalSnapshot, TerminalUpdate, TerminalView, UpdateDamage, WorkerStatus,
};
use winit::event_loop::EventLoopProxy;

use crate::{app::input::InputEncoder, event::AppEvent};

enum PtyMessage {
    Bytes(Vec<u8>),
    Status(PtyReaderStatus),
}

/// Owns the PTY receiver before the PTY so receiver teardown precedes reader joins,
/// including panic unwinding from the worker loop.
struct PtyResources {
    pty_rx: Receiver<PtyMessage>,
    pty: Pty,
}

struct Mailbox {
    update_notification_pending: bool,
    update: Option<TerminalUpdate>,
    acknowledgements: VecDeque<u64>,
    copy_results: VecDeque<CopySelectionResult>,
    status: WorkerStatus,
}

pub(crate) struct TerminalWorkerClient {
    control_tx: Sender<TerminalCommand>,
    signal_tx: SyncSender<()>,
    mailbox: Arc<Mutex<Mailbox>>,
    next_request_id: AtomicU64,
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
        metrics: Arc<RenderMetrics>,
    ) -> anyhow::Result<Self> {
        let (control_tx, control_rx) = mpsc::channel();
        let (signal_tx, signal_rx) = mpsc::sync_channel(1);
        let (pty_tx, pty_rx) = mpsc::sync_channel(64);
        let pty_signal_tx = signal_tx.clone();
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update_notification_pending: false,
            update: None,
            acknowledgements: VecDeque::new(),
            copy_results: VecDeque::new(),
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
                        metrics,
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
            next_request_id: AtomicU64::new(1),
            _thread: Some(thread),
        })
    }

    pub(crate) fn send(&self, command: TerminalCommand) -> bool {
        if self.control_tx.send(command).is_err() {
            return false;
        }
        signal_wake(&self.signal_tx)
    }

    pub(crate) fn shutdown(&self) {
        let _ = self.send(TerminalCommand::Shutdown);
    }

    pub(crate) fn take_update(&self) -> Option<TerminalUpdate> {
        let update = {
            let mut state = lock(&self.mailbox);
            let update = state.update.take();
            state.update_notification_pending = false;
            update
        };
        if update.is_some() {
            // Let the worker publish a snapshot deferred while this update was pending.
            let _ = signal_wake(&self.signal_tx);
        }
        update
    }

    pub(crate) fn take_acknowledgement(&self) -> Option<u64> {
        lock(&self.mailbox).acknowledgements.pop_front()
    }

    pub(crate) fn request_copy(&self, bounds: SelectionBounds) -> Option<u64> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        self.send(TerminalCommand::CopySelection { request_id, bounds })
            .then_some(request_id)
    }

    pub(crate) fn request_resize(&self, size: TerminalSize) -> Option<u64> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        self.send(TerminalCommand::Resize { request_id, size })
            .then_some(request_id)
    }

    pub(crate) fn request_scroll_viewport(&self, rows: isize) -> Option<u64> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        self.send(TerminalCommand::ScrollViewport { request_id, rows })
            .then_some(request_id)
    }

    pub(crate) fn request_scroll_to_top(&self) -> Option<u64> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        self.send(TerminalCommand::ScrollToTop { request_id })
            .then_some(request_id)
    }

    pub(crate) fn request_scroll_to_bottom(&self) -> Option<u64> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        self.send(TerminalCommand::ScrollToBottom { request_id })
            .then_some(request_id)
    }

    pub(crate) fn take_copy_result(&self) -> Option<CopySelectionResult> {
        lock(&self.mailbox).copy_results.pop_front()
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
    metrics: Arc<RenderMetrics>,
    start_pty: bool,
    notifier: Arc<dyn Fn() + Send + Sync>,
    control_rx: Receiver<TerminalCommand>,
    signal_rx: Receiver<()>,
    pty_signal_tx: SyncSender<()>,
    pty_rx: Receiver<PtyMessage>,
    pty_tx: SyncSender<PtyMessage>,
    mailbox: Arc<Mutex<Mailbox>>,
    ready_tx: mpsc::SyncSender<anyhow::Result<()>>,
) {
    let mut terminal = Terminal::new(size.rows, size.cols);
    let mut revision = 0;
    let mut snapshot_dirty = false;
    publish_snapshot(
        &mut terminal,
        &mailbox,
        &metrics,
        notifier.as_ref(),
        revision,
        false,
        None,
    );
    // Readiness is acknowledged only after the PTY has started successfully.

    let mut resources = PtyResources {
        pty_rx,
        pty: Pty::new(NoopWake),
    };
    let pty = &mut resources.pty;
    let pty_rx = &resources.pty_rx;
    if start_pty {
        let output_tx = pty_tx.clone();
        let status_tx = pty_tx;
        let output_signal = pty_signal_tx.clone();
        let status_signal = pty_signal_tx;
        if let Err(error) = pty.start_with_handlers(
            size,
            move |bytes| {
                output_tx.send(PtyMessage::Bytes(bytes)).is_ok() && signal_wake(&output_signal)
            },
            move |status| {
                let _ = status_tx.send(PtyMessage::Status(status));
                let _ = signal_wake(&status_signal);
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
    let mut pending_pty_status = None;
    loop {
        let mut progressed = false;

        match control_rx.try_recv() {
            Ok(command) => {
                progressed = true;
                let publishes_snapshot = matches!(
                    &command,
                    TerminalCommand::PtyOutputBytes(_)
                        | TerminalCommand::Resize { .. }
                        | TerminalCommand::ScrollViewport { .. }
                        | TerminalCommand::ScrollToTop { .. }
                        | TerminalCommand::ScrollToBottom { .. }
                );
                if apply_command(
                    command,
                    &mut terminal,
                    pty,
                    &mailbox,
                    &metrics,
                    notifier.as_ref(),
                    &mut revision,
                ) {
                    break;
                }
                if publishes_snapshot {
                    snapshot_dirty = false;
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
                snapshot_dirty = true;

                // Bound parser work per wake so control commands and shutdown remain observable.
                let mut terminal_status = None;
                for _ in 1..64 {
                    match pty_rx.try_recv() {
                        Ok(PtyMessage::Bytes(bytes)) => {
                            terminal.process_output(&bytes);
                            snapshot_dirty = true;
                        }
                        Ok(PtyMessage::Status(status)) => {
                            terminal_status = Some(status);
                            break;
                        }
                        Err(TryRecvError::Disconnected) => {
                            pty_closed = true;
                            break;
                        }
                        Err(TryRecvError::Empty) => break,
                    }
                }

                revision = revision.saturating_add(1);
                if !mailbox_has_update(&mailbox) {
                    publish_snapshot(
                        &mut terminal,
                        &mailbox,
                        &metrics,
                        notifier.as_ref(),
                        revision,
                        true,
                        None,
                    );
                    snapshot_dirty = false;
                }
                set_status(&mailbox, WorkerStatus::Idle);
                pending_pty_status = terminal_status;
            }
            Ok(PtyMessage::Status(status)) => {
                progressed = true;
                pending_pty_status = Some(status);
            }
            Err(TryRecvError::Disconnected) => pty_closed = true,
            Err(TryRecvError::Empty) => {
                if snapshot_dirty && !mailbox_has_update(&mailbox) {
                    publish_snapshot(
                        &mut terminal,
                        &mailbox,
                        &metrics,
                        notifier.as_ref(),
                        revision,
                        true,
                        None,
                    );
                    snapshot_dirty = false;
                    progressed = true;
                }
            }
        }

        if let Some(status) = pending_pty_status.take() {
            if snapshot_dirty && mailbox_has_update(&mailbox) {
                pending_pty_status = Some(status);
            } else {
                if snapshot_dirty {
                    publish_snapshot(
                        &mut terminal,
                        &mailbox,
                        &metrics,
                        notifier.as_ref(),
                        revision,
                        true,
                        None,
                    );
                    snapshot_dirty = false;
                }
                if apply_pty_status(status, &mailbox, notifier.as_ref()) {
                    break;
                }
            }
        }

        if control_closed && pty_closed && pending_pty_status.is_none() {
            if snapshot_dirty && mailbox_has_update(&mailbox) {
                // Wait for the UI to consume the pending update before flushing.
            } else {
                if snapshot_dirty {
                    publish_snapshot(
                        &mut terminal,
                        &mailbox,
                        &metrics,
                        notifier.as_ref(),
                        revision,
                        true,
                        None,
                    );
                    snapshot_dirty = false;
                }
                break;
            }
        }
        if progressed {
            continue;
        }
        match signal_rx.recv() {
            Ok(()) => {}
            Err(_) => break,
        }
    }

    if !matches!(
        lock(&mailbox).status,
        WorkerStatus::Failed { .. } | WorkerStatus::Stopped
    ) {
        set_status(&mailbox, WorkerStatus::Stopped);
        notify(notifier.as_ref());
    }
}
fn apply_pty_status(
    status: PtyReaderStatus,
    mailbox: &Arc<Mutex<Mailbox>>,
    notifier: &dyn Fn(),
) -> bool {
    match status {
        PtyReaderStatus::Eof => {
            set_status(mailbox, WorkerStatus::Stopped);
            notify(notifier);
            true
        }
        PtyReaderStatus::Error(error) => {
            set_status(mailbox, WorkerStatus::Failed { message: error });
            notify(notifier);
            true
        }
    }
}

fn encode_input(
    request: &harbor_types::InputRequest,
    modes: harbor_types::InputModes,
) -> Option<std::borrow::Cow<'static, [u8]>> {
    use harbor_types::InputKey;
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    let key = match &request.key {
        InputKey::Character(ch) => Key::Character(ch.clone().into()),
        InputKey::Enter => Key::Named(NamedKey::Enter),
        InputKey::Backspace => Key::Named(NamedKey::Backspace),
        InputKey::Tab => Key::Named(NamedKey::Tab),
        InputKey::Escape => Key::Named(NamedKey::Escape),
        InputKey::Space => Key::Named(NamedKey::Space),
        InputKey::ArrowUp => Key::Named(NamedKey::ArrowUp),
        InputKey::ArrowDown => Key::Named(NamedKey::ArrowDown),
        InputKey::ArrowRight => Key::Named(NamedKey::ArrowRight),
        InputKey::ArrowLeft => Key::Named(NamedKey::ArrowLeft),
        InputKey::Home => Key::Named(NamedKey::Home),
        InputKey::End => Key::Named(NamedKey::End),
        InputKey::F1 => Key::Named(NamedKey::F1),
        InputKey::F2 => Key::Named(NamedKey::F2),
        InputKey::F3 => Key::Named(NamedKey::F3),
        InputKey::F4 => Key::Named(NamedKey::F4),
        InputKey::F5 => Key::Named(NamedKey::F5),
        InputKey::F6 => Key::Named(NamedKey::F6),
        InputKey::F7 => Key::Named(NamedKey::F7),
        InputKey::F8 => Key::Named(NamedKey::F8),
        InputKey::F9 => Key::Named(NamedKey::F9),
        InputKey::F10 => Key::Named(NamedKey::F10),
        InputKey::F11 => Key::Named(NamedKey::F11),
        InputKey::F12 => Key::Named(NamedKey::F12),
        InputKey::Insert => Key::Named(NamedKey::Insert),
        InputKey::Delete => Key::Named(NamedKey::Delete),
        InputKey::PageUp => Key::Named(NamedKey::PageUp),
        InputKey::PageDown => Key::Named(NamedKey::PageDown),
    };
    let mut modifiers = ModifiersState::default();
    if request.modifiers.shift() {
        modifiers.insert(ModifiersState::SHIFT);
    }
    if request.modifiers.alt() {
        modifiers.insert(ModifiersState::ALT);
    }
    if request.modifiers.control() {
        modifiers.insert(ModifiersState::CONTROL);
    }
    if request.modifiers.super_key() {
        modifiers.insert(ModifiersState::SUPER);
    }
    InputEncoder::key(
        &key,
        request.text.as_deref(),
        modifiers,
        modes,
        request.is_numpad,
    )
}

fn apply_command(
    command: TerminalCommand,
    terminal: &mut Terminal,
    pty: &mut Pty,
    mailbox: &Arc<Mutex<Mailbox>>,
    metrics: &RenderMetrics,
    notifier: &dyn Fn(),
    revision: &mut u64,
) -> bool {
    match command {
        TerminalCommand::PtyOutputBytes(bytes) => {
            set_status(mailbox, WorkerStatus::Processing);
            terminal.process_output(&bytes);
            *revision = revision.saturating_add(1);
            publish_snapshot(terminal, mailbox, metrics, notifier, *revision, true, None);
            set_status(mailbox, WorkerStatus::Idle);
        }
        TerminalCommand::Input(request) => {
            if let Some(bytes) = encode_input(&request, terminal.screen().input_modes()) {
                pty.write(&bytes);
            }
            notify(notifier);
        }
        TerminalCommand::PasteText(text) => {
            let modes = terminal.screen().input_modes();
            pty.write(&modes.paste(text.as_bytes()));
            notify(notifier);
        }
        TerminalCommand::Resize { request_id, size } => {
            if terminal.resize_terminal_if_changed(size) {
                pty.resize(size);
                *revision = revision.saturating_add(1);
            }
            publish_snapshot(
                terminal,
                mailbox,
                metrics,
                notifier,
                *revision,
                true,
                Some(request_id),
            );
        }
        TerminalCommand::ScrollViewport { request_id, rows } => {
            if rows >= 0 {
                terminal.scroll_viewport_down(rows as usize);
            } else {
                terminal.scroll_viewport_up(rows.unsigned_abs());
            }
            *revision = revision.saturating_add(1);
            publish_snapshot(
                terminal,
                mailbox,
                metrics,
                notifier,
                *revision,
                true,
                Some(request_id),
            );
        }
        TerminalCommand::ScrollToTop { request_id } => {
            terminal.scroll_viewport_to_top();
            *revision = revision.saturating_add(1);
            publish_snapshot(
                terminal,
                mailbox,
                metrics,
                notifier,
                *revision,
                true,
                Some(request_id),
            );
        }
        TerminalCommand::ScrollToBottom { request_id } => {
            terminal.scroll_viewport_to_bottom();
            *revision = revision.saturating_add(1);
            publish_snapshot(
                terminal,
                mailbox,
                metrics,
                notifier,
                *revision,
                true,
                Some(request_id),
            );
        }
        TerminalCommand::SetSelectionDragActive(active) => {
            terminal.set_suppress_scroll_snap(active);
        }
        TerminalCommand::CopySelection { request_id, bounds } => {
            let result = CopySelectionResult {
                request_id,
                text: terminal.screen().selected_text(bounds),
            };
            lock(mailbox).copy_results.push_back(result);
            notify(notifier);
        }
        TerminalCommand::RequestSnapshot { .. } => {}
        TerminalCommand::Shutdown => return true,
    }
    false
}

fn mailbox_has_update(mailbox: &Arc<Mutex<Mailbox>>) -> bool {
    lock(mailbox).update.is_some()
}

fn publish_snapshot(
    terminal: &mut Terminal,
    mailbox: &Arc<Mutex<Mailbox>>,
    metrics: &RenderMetrics,
    notifier: &dyn Fn(),
    revision: u64,
    overwrite_is_gap: bool,
    acknowledged_request_id: Option<u64>,
) {
    let started = Instant::now();
    let snapshot = terminal.snapshot();
    metrics.record_snapshot_build(started.elapsed());
    let mut update =
        TerminalUpdate::with_acknowledgement(revision, snapshot, acknowledged_request_id);
    let (overwritten, should_notify) = {
        let mut state = lock(mailbox);
        let overwritten = overwrite_is_gap && state.update.is_some();
        if overwritten {
            update.damage = UpdateDamage::FullUpload;
        }
        if let Some(request_id) = acknowledged_request_id {
            state.acknowledgements.push_back(request_id);
        }
        state.update = Some(update);
        let should_notify = mark_update_notification_pending(&mut state);
        (overwritten, should_notify)
    };
    metrics.record_mailbox(overwritten, 0);
    terminal.clear_screen_dirty();
    if should_notify {
        notify(notifier);
    }
}

fn set_status(mailbox: &Arc<Mutex<Mailbox>>, status: WorkerStatus) {
    lock(mailbox).status = status;
}

fn notify(notifier: &dyn Fn()) {
    notifier();
}

fn mark_update_notification_pending(state: &mut Mailbox) -> bool {
    let should_notify = !state.update_notification_pending;
    state.update_notification_pending = true;
    should_notify
}

fn signal_wake(signal: &SyncSender<()>) -> bool {
    match signal.try_send(()) {
        Ok(()) | Err(mpsc::TrySendError::Full(())) => true,
        Err(mpsc::TrySendError::Disconnected(())) => false,
    }
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

    fn request_copy(&self, bounds: SelectionBounds) -> Option<u64> {
        self.worker.request_copy(bounds)
    }

    fn send_input(&self, request: harbor_types::InputRequest) {
        let _ = self.worker.send(TerminalCommand::Input(request));
    }

    fn send_paste(&self, text: String) {
        let _ = self.worker.send(TerminalCommand::PasteText(text));
    }

    fn scroll_viewport_up(&self, n: usize) -> Option<u64> {
        self.worker.request_scroll_viewport(-(n as isize))
    }

    fn scroll_viewport_down(&self, n: usize) -> Option<u64> {
        self.worker.request_scroll_viewport(n as isize)
    }

    fn scroll_viewport_to_top(&self) -> Option<u64> {
        self.worker.request_scroll_to_top()
    }

    fn scroll_viewport_to_bottom(&self) -> Option<u64> {
        self.worker.request_scroll_to_bottom()
    }

    fn set_suppress_scroll_snap(&self, active: bool) {
        let _ = self
            .worker
            .send(TerminalCommand::SetSelectionDragActive(active));
    }

    fn is_alt_screen(&self) -> bool {
        self.snapshot.is_alt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn update_notifications_coalesce_until_mailbox_is_consumed() {
        let mut mailbox = Mailbox {
            update_notification_pending: false,
            update: None,
            acknowledgements: VecDeque::new(),
            copy_results: VecDeque::new(),
            status: WorkerStatus::Ready,
        };

        assert!(mark_update_notification_pending(&mut mailbox));
        assert!(!mark_update_notification_pending(&mut mailbox));
        mailbox.update_notification_pending = false;
        assert!(mark_update_notification_pending(&mut mailbox));
    }

    #[test]
    fn signal_wake_is_bounded_and_nonblocking() {
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(signal_wake(&tx));
        assert!(signal_wake(&tx));
        assert!(rx.try_recv().is_ok());
        drop(rx);
        assert!(!signal_wake(&tx));
    }

    #[test]
    fn worker_consumes_ordered_output_and_publishes_revisioned_state() {
        let (control_tx, control_rx) = mpsc::channel();
        let metrics = Arc::new(RenderMetrics::default());
        let observed_metrics = Arc::clone(&metrics);
        let (signal_tx, signal_rx) = mpsc::sync_channel(1);
        let (pty_tx, pty_rx) = mpsc::sync_channel(64);
        let pty_signal_tx = signal_tx.clone();
        let test_pty_tx = pty_tx.clone();
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update_notification_pending: false,
            update: None,
            acknowledgements: VecDeque::new(),
            copy_results: VecDeque::new(),
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
                metrics,
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
        let initial = {
            let mut state = lock(&mailbox);
            state.update_notification_pending = false;
            state.update.take().expect("initial snapshot is available")
        };
        assert_eq!(initial.revision, 0);
        while wake_rx.try_recv().is_ok() {}

        test_pty_tx.send(PtyMessage::Bytes(b"ab".to_vec())).unwrap();
        signal_wake(&signal_tx);
        let first_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !lock(&mailbox).update.as_ref().is_some_and(|update| {
            update
                .snapshot
                .cells
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>()
                .starts_with("ab")
        }) {
            assert!(std::time::Instant::now() < first_deadline);
            std::thread::yield_now();
        }
        assert!(
            wake_rx.recv_timeout(Duration::from_secs(1)).is_ok(),
            "worker did not notify the first snapshot"
        );
        let first_idle_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while lock(&mailbox).status != WorkerStatus::Idle {
            assert!(std::time::Instant::now() < first_idle_deadline);
            std::thread::yield_now();
        }
        {
            lock(&mailbox).status = WorkerStatus::Ready;
        }
        test_pty_tx.send(PtyMessage::Bytes(b"cd".to_vec())).unwrap();
        signal_wake(&signal_tx);
        let processing_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while lock(&mailbox).status != WorkerStatus::Idle {
            assert!(std::time::Instant::now() < processing_deadline);
            std::thread::yield_now();
        }
        {
            let mut state = lock(&mailbox);
            let first = state
                .update
                .take()
                .expect("first update remains in mailbox");
            assert!(
                first
                    .snapshot
                    .cells
                    .iter()
                    .map(|cell| cell.ch)
                    .collect::<String>()
                    .starts_with("ab")
            );
            state.update_notification_pending = false;
        }
        signal_wake(&signal_tx);

        let latest = {
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            loop {
                if lock(&mailbox).update.as_ref().is_some_and(|update| {
                    update
                        .snapshot
                        .cells
                        .iter()
                        .map(|cell| cell.ch)
                        .collect::<String>()
                        == "abcd"
                }) {
                    break lock(&mailbox)
                        .update
                        .take()
                        .expect("latest update remains in mailbox");
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "worker did not publish the deferred revision"
                );
                std::thread::yield_now();
            }
        };
        assert!(
            wake_rx.recv_timeout(Duration::from_secs(1)).is_ok(),
            "worker did not notify the deferred snapshot"
        );
        assert!(
            wake_rx.recv_timeout(Duration::from_millis(20)).is_err(),
            "burst enqueued duplicate update wakes"
        );
        assert!((1..=2).contains(&latest.revision));
        assert_eq!(
            latest
                .snapshot
                .cells
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>(),
            "abcd"
        );
        assert!(matches!(
            latest.damage,
            UpdateDamage::Ranges(_) | UpdateDamage::FullUpload
        ));
        assert_eq!(observed_metrics.snapshot().snapshot_build_count, 3);

        control_tx.send(TerminalCommand::Shutdown).unwrap();
        signal_wake(&signal_tx);
        thread.join().unwrap();
        assert_eq!(lock(&mailbox).status, WorkerStatus::Stopped);
    }
    #[test]
    fn bounded_pty_queue_unblocks_when_receiver_is_dropped() {
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(PtyMessage::Bytes(vec![0])).unwrap();
        let sender = std::thread::spawn(move || tx.send(PtyMessage::Bytes(vec![1])));

        drop(rx);
        assert!(sender.join().unwrap().is_err());
    }

    #[test]
    fn worker_input_encoding_uses_authoritative_modes() {
        let request = harbor_types::InputRequest {
            key: harbor_types::InputKey::ArrowUp,
            text: None,
            modifiers: harbor_types::InputModifiers::default(),
            is_numpad: false,
        };
        assert_eq!(
            encode_input(
                &request,
                harbor_types::InputModes {
                    application_cursor: true,
                    ..Default::default()
                }
            )
            .as_deref(),
            Some(b"\x1bOA".as_slice())
        );
    }

    #[test]
    fn worker_copy_selection_returns_async_result() {
        let mut terminal = Terminal::new(1, 4);
        terminal.put_str("ab");
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update_notification_pending: false,
            update: None,
            acknowledgements: VecDeque::new(),
            copy_results: VecDeque::new(),
            status: WorkerStatus::Ready,
        }));
        let mut revision = 0;
        let mut pty = Pty::new(NoopWake);
        let metrics = RenderMetrics::default();
        apply_command(
            TerminalCommand::CopySelection {
                request_id: 7,
                bounds: SelectionBounds {
                    start_row: 0,
                    start_col: 0,
                    end_row: 0,
                    end_col: 1,
                },
            },
            &mut terminal,
            &mut pty,
            &mailbox,
            &metrics,
            &|| {},
            &mut revision,
        );
        assert_eq!(
            lock(&mailbox).copy_results.pop_front(),
            Some(CopySelectionResult {
                request_id: 7,
                text: "ab".to_owned(),
            })
        );
    }

    #[test]
    fn worker_acknowledges_the_snapshot_command_request_id() {
        let mut terminal = Terminal::new(1, 4);
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update_notification_pending: false,
            update: None,
            acknowledgements: VecDeque::new(),
            copy_results: VecDeque::new(),
            status: WorkerStatus::Ready,
        }));
        let mut revision = 0;
        let mut pty = Pty::new(NoopWake);
        let metrics = RenderMetrics::default();

        apply_command(
            TerminalCommand::ScrollViewport {
                request_id: 42,
                rows: 1,
            },
            &mut terminal,
            &mut pty,
            &mailbox,
            &metrics,
            &|| {},
            &mut revision,
        );

        assert_eq!(
            lock(&mailbox)
                .update
                .as_ref()
                .and_then(|update| update.acknowledged_request_id),
            Some(42)
        );

        apply_command(
            TerminalCommand::ScrollViewport {
                request_id: 43,
                rows: -1,
            },
            &mut terminal,
            &mut pty,
            &mailbox,
            &metrics,
            &|| {},
            &mut revision,
        );
        let mut state = lock(&mailbox);
        assert_eq!(
            state.acknowledgements.iter().copied().collect::<Vec<_>>(),
            vec![42, 43]
        );
        assert_eq!(state.acknowledgements.pop_front(), Some(42));
        assert_eq!(state.acknowledgements.pop_front(), Some(43));
        assert!(state.acknowledgements.is_empty());
    }
    #[test]
    fn worker_preserves_failed_status_after_pty_error() {
        let (signal_tx, signal_rx) = mpsc::sync_channel(1);
        let (control_tx, control_rx) = mpsc::channel();
        let (pty_tx, pty_rx) = mpsc::sync_channel(64);
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update_notification_pending: false,
            update: None,
            acknowledgements: VecDeque::new(),
            copy_results: VecDeque::new(),
            status: WorkerStatus::Ready,
        }));
        let (ready_tx, ready_rx) = mpsc::sync_channel::<anyhow::Result<()>>(0);
        let test_pty_tx = pty_tx.clone();
        let test_signal_tx = signal_tx.clone();
        let worker_mailbox = Arc::clone(&mailbox);
        let metrics = Arc::new(RenderMetrics::default());
        let thread = std::thread::spawn(move || {
            worker_main(
                TerminalSize { rows: 1, cols: 4 },
                metrics,
                false,
                Arc::new(|| {}),
                control_rx,
                signal_rx,
                test_signal_tx,
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
        test_pty_tx
            .send(PtyMessage::Status(PtyReaderStatus::Error(
                "read failed".to_owned(),
            )))
            .unwrap();
        signal_wake(&signal_tx);
        drop(control_tx);
        thread.join().unwrap();

        assert_eq!(
            lock(&mailbox).status,
            WorkerStatus::Failed {
                message: "read failed".to_owned()
            }
        );
    }

    #[test]
    fn worker_panic_is_reported_without_rethrowing_to_caller() {
        let mailbox = Arc::new(Mutex::new(Mailbox {
            update_notification_pending: false,
            update: None,
            acknowledgements: VecDeque::new(),
            copy_results: VecDeque::new(),
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
