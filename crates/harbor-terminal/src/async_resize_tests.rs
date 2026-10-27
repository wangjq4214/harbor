//! Deterministic consumer-side tests. The reader protocol itself is tested in io::tests.
use super::*;
use crate::Terminal;

struct RecordingWriter(Arc<Mutex<Vec<u8>>>);
impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Fixture {
    terminal: Arc<Mutex<Terminal>>,
    events: mpsc::SyncSender<ReaderEvent>,
    commands: Receiver<ReaderCommand>,
    replies: Arc<Mutex<Vec<u8>>>,
    finish: Option<Sender<()>>,
}

impl Fixture {
    fn new() -> Self {
        let (events, output) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let (commands, command_rx) = mpsc::channel();
        let (finish, finished) = mpsc::channel();
        // Keep a real reader handle alive, but gate publication and ACK delivery
        // explicitly so no wall-clock sleep is needed for consumer-side assertions.
        let reader = std::thread::spawn(move || {
            let _ = finished.recv();
        });
        let replies = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = Terminal::new_headless(3, 20);
        terminal.io = TerminalIo::with_session(TerminalSession::with_pty(TerminalPty {
            output,
            commands,
            writer: Box::new(RecordingWriter(Arc::clone(&replies))),
            reader: Some(reader),
            control: None,
            wake_pending: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Mutex::new(|| true)),
            next_barrier_epoch: 1,
            test_interrupt: Some(Arc::new(|_| Ok(()))),
            barrier_timeout: RESIZE_BARRIER_TIMEOUT,
        }));
        Self {
            terminal: Arc::new(Mutex::new(terminal)),
            events,
            commands: command_rx,
            replies,
            finish: Some(finish),
        }
    }

    fn epoch(&self) -> u64 {
        loop {
            match self
                .commands
                .try_recv()
                .expect("request must return before ACK")
            {
                ReaderCommand::Barrier(epoch) => return epoch,
                ReaderCommand::Resume(_) => {}
            }
        }
    }

    fn bytes(&self, bytes: &[u8]) {
        self.events
            .send(ReaderEvent::Bytes(bytes.to_vec()))
            .unwrap();
    }

    fn ack(&self, epoch: u64) {
        self.events.send(ReaderEvent::BarrierAck(epoch)).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.finish.take();
        self.terminal
            .lock()
            .unwrap()
            .io
            .session
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
fn gated_ack_keeps_terminal_accessible_merges_targets_and_dispatches_from_normal_drain() {
    let f = Fixture::new();
    let target = TerminalSize { rows: 4, cols: 10 };
    assert!(
        !f.terminal
            .lock()
            .unwrap()
            .try_resize_if_changed(target)
            .unwrap()
    );
    let epoch = f.epoch();
    // Ordinary access succeeds before ACK/timeout. Actual Runtime event
    // dispatch during group acquisition is covered by application native tests.
    {
        let terminal = f.terminal.try_lock().expect("no lock across ACK wait");
        assert_eq!(terminal.screen().cols(), 20);
    }
    for cols in [8, 12, 6, 10] {
        assert!(
            !f.terminal
                .lock()
                .unwrap()
                .try_resize_if_changed(TerminalSize { rows: 4, cols })
                .unwrap()
        );
    }
    assert!(
        matches!(f.commands.try_recv(), Err(TryRecvError::Empty)),
        "one acquisition, not one per target"
    );
    f.bytes(b"\x1b[2;20HX\x1b[6n\x1b[2;");
    f.ack(epoch);
    {
        let mut terminal = f.terminal.lock().unwrap();
        // An input/frame drain, not a resize-specific receive, sees the ACK first.
        terminal.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer));
        assert_eq!(terminal.screen().cols(), 20);
        assert!(terminal.io.resize_barrier_ready().unwrap());
        assert_eq!(*f.replies.lock().unwrap(), b"\x1b[2;20R");
        assert!(
            !terminal.drain_pty(),
            "ACK-only completion must not be classified as child output/unread text"
        );
        assert_eq!(
            (terminal.screen().rows(), terminal.screen().cols()),
            (4, 10)
        );
    }
    f.bytes(b"3HA\x1b[6n");
    f.terminal.lock().unwrap().drain_pty();
    let terminal = f.terminal.lock().unwrap();
    assert_eq!(terminal.row_text(1).chars().nth(2), Some('A'));
    assert_eq!(*f.replies.lock().unwrap(), b"\x1b[2;20R\x1b[2;4R");
}

#[test]
fn budgeted_drain_does_not_commit_before_all_old_bytes_or_lose_ack_only_progress() {
    let f = Fixture::new();
    let target = TerminalSize { rows: 3, cols: 8 };
    assert!(
        !f.terminal
            .lock()
            .unwrap()
            .try_resize_if_changed(target)
            .unwrap()
    );
    let epoch = f.epoch();
    for _ in 0..PTY_DRAIN_EVENT_BUDGET + 1 {
        f.bytes(b"a");
    }
    f.ack(epoch);
    let mut terminal = f.terminal.lock().unwrap();
    terminal.drain_pty();
    assert_eq!(terminal.screen().cols(), 20);
    terminal.drain_pty();
    assert_eq!(terminal.screen().cols(), 8);
    let text = terminal.screen().selected_text(crate::SelectionBounds {
        start_row: terminal.screen().history_start(),
        start_col: 0,
        end_row: terminal.screen().history_start()
            + (terminal.screen().scroll_count() + terminal.screen().rows()) as u64
            - 1,
        end_col: terminal.screen().cols() - 1,
    });
    assert_eq!(
        text.chars().filter(|ch| *ch == 'a').count(),
        PTY_DRAIN_EVENT_BUDGET + 1
    );
}

#[test]
fn timeout_releases_epoch_and_late_ack_cannot_complete_retry() {
    let f = Fixture::new();
    let target = TerminalSize { rows: 3, cols: 8 };
    assert!(
        !f.terminal
            .lock()
            .unwrap()
            .try_resize_if_changed(target)
            .unwrap()
    );
    let old = f.epoch();
    {
        let mut terminal = f.terminal.lock().unwrap();
        terminal
            .io
            .session
            .advance_resize_barrier(Instant::now() + RESIZE_BARRIER_TIMEOUT);
        let error = terminal.finish_pending_resize().unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert_eq!(terminal.screen().cols(), 20);
    }
    assert_eq!(f.commands.try_recv().unwrap(), ReaderCommand::Resume(old));
    assert!(
        !f.terminal
            .lock()
            .unwrap()
            .try_resize_if_changed(target)
            .unwrap()
    );
    let new = f.epoch();
    assert_ne!(old, new);
    f.ack(old);
    f.terminal.lock().unwrap().drain_pty();
    assert_eq!(f.terminal.lock().unwrap().screen().cols(), 20);
    f.ack(new);
    f.terminal.lock().unwrap().drain_pty();
    assert_eq!(f.terminal.lock().unwrap().screen().cols(), 8);
}

#[test]
fn configuration_lease_blocks_ordinary_takeover_and_cancels_without_terminal_lock() {
    let f = Fixture::new();
    let lease = f
        .terminal
        .lock()
        .unwrap()
        .begin_configuration_resize()
        .unwrap();
    let epoch = f.epoch();
    assert!(!lease.is_ready().unwrap());
    assert!(
        !f.terminal
            .lock()
            .unwrap()
            .try_resize_if_changed(TerminalSize { rows: 4, cols: 9 })
            .unwrap()
    );
    assert!(matches!(f.commands.try_recv(), Err(TryRecvError::Empty)));
    {
        let terminal = f.terminal.lock().unwrap();
        // Dropping the lease cannot attempt this already-held mutex.
        drop(lease);
        assert_eq!(terminal.screen().cols(), 20);
        assert_eq!(f.commands.try_recv().unwrap(), ReaderCommand::Resume(epoch));
    }
    f.ack(epoch);
    assert!(
        !f.terminal
            .lock()
            .unwrap()
            .try_resize_if_changed(TerminalSize { rows: 4, cols: 9 })
            .unwrap()
    );
    let next = f.epoch();
    f.ack(next);
    f.terminal.lock().unwrap().drain_pty();
    assert_eq!(f.terminal.lock().unwrap().screen().cols(), 9);
}

#[test]
fn configuration_ready_remains_uncommitted_until_group_preparation() {
    let f = Fixture::new();
    let lease = f
        .terminal
        .lock()
        .unwrap()
        .begin_configuration_resize()
        .unwrap();
    let epoch = f.epoch();
    f.bytes(b"BEFORE");
    f.ack(epoch);
    f.terminal.lock().unwrap().drain_pty();
    assert!(lease.is_ready().unwrap());
    assert_eq!(f.terminal.lock().unwrap().screen().cols(), 20);
    // Reject prepared resources rather than resize/accept a member independently.
    let transaction = &mut *f.terminal.lock().unwrap();
    drop(
        transaction
            .prepare_configuration_resize(TerminalSize { rows: 4, cols: 10 })
            .unwrap(),
    );
    assert_eq!(transaction.screen().cols(), 20);
    drop(lease);
    assert!(
        f.commands
            .try_iter()
            .any(|cmd| cmd == ReaderCommand::Resume(epoch))
    );
}

#[test]
fn returning_to_current_size_cancels_without_committing_late_ack() {
    let f = Fixture::new();
    f.terminal
        .lock()
        .unwrap()
        .try_resize_if_changed(TerminalSize { rows: 4, cols: 9 })
        .unwrap();
    let epoch = f.epoch();
    assert!(
        !f.terminal
            .lock()
            .unwrap()
            .try_resize_if_changed(TerminalSize { rows: 3, cols: 20 })
            .unwrap()
    );
    assert_eq!(f.commands.try_recv().unwrap(), ReaderCommand::Resume(epoch));
    f.ack(epoch);
    f.bytes(b"retained");
    let mut terminal = f.terminal.lock().unwrap();
    terminal.drain_pty();
    assert!(!terminal.resize_pending());
    assert_eq!(terminal.screen().cols(), 20);
    assert_eq!(terminal.row_text(0).trim_end(), "retained");
}

#[test]
fn palette_only_preparation_does_not_cancel_pending_ordinary_geometry() {
    let f = Fixture::new();
    f.terminal
        .lock()
        .unwrap()
        .try_resize_if_changed(TerminalSize { rows: 4, cols: 9 })
        .unwrap();
    let epoch = f.epoch();
    {
        let mut terminal = f.terminal.lock().unwrap();
        let transaction = terminal
            .prepare_configuration_resize(TerminalSize { rows: 3, cols: 20 })
            .unwrap();
        transaction.commit(harbor_config::Palette::default(), None, 1.0);
        assert!(terminal.resize_pending());
    }
    assert!(matches!(f.commands.try_recv(), Err(TryRecvError::Empty)));
    f.ack(epoch);
    f.terminal.lock().unwrap().drain_pty();
    assert_eq!(f.terminal.lock().unwrap().screen().cols(), 9);
}

#[test]
fn configuration_preparation_cannot_take_an_ordinary_acknowledged_barrier() {
    let f = Fixture::new();
    f.terminal
        .lock()
        .unwrap()
        .try_resize_if_changed(TerminalSize { rows: 4, cols: 9 })
        .unwrap();
    let epoch = f.epoch();
    f.ack(epoch);
    let mut terminal = f.terminal.lock().unwrap();
    terminal.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer));
    assert!(
        terminal
            .prepare_configuration_resize(TerminalSize { rows: 5, cols: 10 })
            .is_err()
    );
    assert!(!terminal.drain_pty());
    assert_eq!(terminal.screen().cols(), 9);
}

#[test]
fn configuration_timeout_stays_observable_after_ordinary_retry() {
    let f = Fixture::new();
    let lease = f
        .terminal
        .lock()
        .unwrap()
        .begin_configuration_resize()
        .unwrap();
    let old = f.epoch();
    {
        let mut terminal = f.terminal.lock().unwrap();
        terminal
            .io
            .session
            .advance_resize_barrier(Instant::now() + RESIZE_BARRIER_TIMEOUT);
        assert!(
            lease
                .is_ready()
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
        assert!(
            !terminal
                .try_resize_if_changed(TerminalSize { rows: 4, cols: 9 })
                .unwrap()
        );
    }
    assert!(lease.is_ready().is_err());
    let new = f.epoch();
    assert_ne!(new, old);
    drop(lease);
    f.ack(old);
    f.ack(new);
    f.terminal.lock().unwrap().drain_pty();
    assert_eq!(f.terminal.lock().unwrap().screen().cols(), 9);
}

#[test]
fn resize_deadline_and_ack_progress_do_not_depend_on_ordinary_presentation() {
    let f = Fixture::new();
    {
        let mut terminal = f.terminal.lock().unwrap();
        terminal.put_str("\x1b[?25l\x1b[?2026h");
        assert!(
            !terminal
                .try_resize_if_changed(TerminalSize { rows: 4, cols: 9 })
                .unwrap()
        );
        let demand = terminal.frame_demand(Instant::now());
        assert!(!demand.ordinary_present_eligible);
        assert!(!demand.redraw_now);
        assert!(
            demand.deadline.is_some(),
            "hidden cursor must not remove resize deadline"
        );
    }
    let epoch = f.epoch();
    f.ack(epoch);
    let mut terminal = f.terminal.lock().unwrap();
    let demand = terminal.frame_demand(Instant::now());
    assert!(!demand.ordinary_present_eligible);
    assert_eq!(terminal.screen().cols(), 9);
    assert!(!terminal.resize_pending());
}

#[test]
fn reader_disconnect_fails_configuration_and_releases_its_epoch() {
    let mut f = Fixture::new();
    let lease = f
        .terminal
        .lock()
        .unwrap()
        .begin_configuration_resize()
        .unwrap();
    let epoch = f.epoch();
    let (unused, _rx) = mpsc::sync_channel(1);
    drop(std::mem::replace(&mut f.events, unused));
    f.terminal.lock().unwrap().drain_pty();
    assert!(
        lease
            .is_ready()
            .unwrap_err()
            .to_string()
            .contains("reader stopped")
    );
    assert!(f.terminal.lock().unwrap().is_session_closed());
    assert_eq!(f.terminal.lock().unwrap().screen().cols(), 20);
    assert_eq!(f.commands.try_recv().unwrap(), ReaderCommand::Resume(epoch));
}
