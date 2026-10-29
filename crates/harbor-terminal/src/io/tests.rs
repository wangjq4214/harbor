use super::*;

#[test]
fn snapped_reader_output_at_live_bottom_preserves_local_damage() {
    let mut io = TerminalIo::new_headless();
    let mut screen = Screen::new(3, 10);
    let mut pointer = PointerInteraction::new();
    screen.clear_dirty();
    io.process_reader_bytes(&mut screen, &mut pointer, b"x");
    assert_eq!(screen.view_offset(), 0);
    assert_eq!(
        screen.dirty_ranges(),
        vec![crate::DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 2,
        }]
    );
}

#[test]
fn snapped_reader_control_only_chunks_preserve_existing_damage() {
    let mut io = TerminalIo::new_headless();
    let mut screen = Screen::new(3, 10);
    let mut pointer = PointerInteraction::new();
    screen.clear_dirty();
    for bytes in [b"".as_slice(), b"\x1b[0m", b"\x1b[", b"0m"] {
        io.process_reader_bytes(&mut screen, &mut pointer, bytes);
        assert!(screen.dirty_ranges().is_empty(), "chunk={bytes:?}");
    }
    screen.mark_range_dirty(1, 2, 3);
    let pending = screen.dirty_ranges();
    io.process_reader_bytes(&mut screen, &mut pointer, b"\x1b[0m");
    assert_eq!(
        screen.dirty_ranges(),
        pending,
        "no-op snap must not clear pending damage"
    );
}

#[test]
fn snapped_reader_output_still_invalidates_a_real_viewport_snap() {
    let mut io = TerminalIo::new_headless();
    let mut screen = Screen::new(3, 10);
    let mut pointer = PointerInteraction::new();
    io.feed_pty_output(&mut screen, &mut pointer, b"a\r\nb\r\nc\r\nd");
    screen.scroll_up(1);
    assert_eq!(screen.view_offset(), 1);
    screen.clear_dirty();
    io.process_reader_bytes(&mut screen, &mut pointer, b"\x1b[0m");
    assert_eq!(screen.view_offset(), 0);
    assert_eq!(
        screen.dirty_ranges(),
        (0..3)
            .map(|row| crate::DirtyRange {
                row,
                start_col: 0,
                end_col: 10,
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn snapped_reader_respects_review_suppression_and_alt_screen() {
    let mut io = TerminalIo::new_headless();
    let mut screen = Screen::new(3, 10);
    let mut pointer = PointerInteraction::new();
    io.feed_pty_output(&mut screen, &mut pointer, b"a\r\nb\r\nc\r\nd");
    screen.scroll_up(1);
    io.suppress_scroll_snap = true;
    io.process_reader_bytes(&mut screen, &mut pointer, b"\x1b[0m");
    assert_eq!(screen.view_offset(), 1);
    io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?1049h");
    assert!(screen.is_alt());
    assert!(!io.suppress_scroll_snap);
    screen.clear_dirty();
    io.process_reader_bytes(&mut screen, &mut pointer, b"x");
    assert_eq!(
        screen.dirty_ranges(),
        vec![crate::DirtyRange {
            // The ASCII parser conservatively includes the adjacent cell for wide repair.
            row: 0,
            start_col: 0,
            end_col: 2,
        }]
    );
}

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
        let mut io = TerminalIo::with_session(TerminalSession::with_pty(TerminalPty {
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
        }));
        let mut screen = Screen::new(3, 20);
        let mut pointer = PointerInteraction::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        let result = io.session.request_resize_barrier(false, Instant::now());
        loop {
            assert!(Instant::now() < deadline);
            match io.session.next_event() {
                Ok(Some(ReaderEvent::Bytes(bytes))) => {
                    assert_eq!(screen.cols(), 20, "callback must use pre-resize geometry");
                    io.process_reader_bytes(&mut screen, &mut pointer, &bytes);
                    assert!(
                        format!("{:?}", io.parser)
                            .matches("ClipboardWrite(")
                            .count()
                            <= 1
                    );
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
                    io.session.acknowledge_resize_barrier(epoch)
                }
                Ok(None)
                    if processed.load(Ordering::SeqCst) == CHUNKS
                        && (interrupt_failure || io.session.resize_barrier_ready().unwrap()) =>
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
        let events = io.drain_output_events();
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
            !io.session
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
            let barrier = io.session.take_resize_barrier().unwrap().unwrap();
            assert_eq!(barrier.epoch, 7);
            assert!(
                !io.session
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
        io.session
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

#[test]
fn empty_reader_chunk_drains_pending_replies_without_snapping_review() {
    struct Writer(Arc<Mutex<Vec<u8>>>);
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let (tx, output) = mpsc::sync_channel(2);
    let (commands, _command_rx) = mpsc::channel();
    let replies = Arc::new(Mutex::new(Vec::new()));
    let mut io = TerminalIo::with_session(TerminalSession::with_pty(TerminalPty {
        output,
        commands,
        writer: Box::new(Writer(Arc::clone(&replies))),
        reader: None,
        control: None,
        wake_pending: Arc::new(AtomicBool::new(true)),
        wake: Arc::new(Mutex::new(|| true)),
        next_barrier_epoch: 1,
        test_interrupt: None,
        barrier_timeout: RESIZE_BARRIER_TIMEOUT,
    }));
    let mut screen = Screen::new(2, 8);
    let mut pointer = PointerInteraction::new();
    io.feed_pty_output(&mut screen, &mut pointer, b"one\r\ntwo\r\nthree\r\nfour");
    io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[1;2H\x1b[6n");
    screen.scroll_up(2);
    let review_offset = screen.view_offset();
    assert!(review_offset > 0);
    io.feed_pty_output_snapped(&mut screen, &mut pointer, b"");
    assert_eq!(screen.view_offset(), review_offset);
    assert!(replies.lock().unwrap().is_empty());

    tx.send(ReaderEvent::Bytes(Vec::new())).unwrap();
    assert!(io.drain(&mut screen, &mut pointer));
    assert_eq!(screen.view_offset(), review_offset);
    assert_eq!(*replies.lock().unwrap(), b"\x1b[1;2R");
    assert!(!io.drain(&mut screen, &mut pointer));
    assert_eq!(*replies.lock().unwrap(), b"\x1b[1;2R");

    // A query split across chunks and an alt-screen exit retain reply order.
    tx.send(ReaderEvent::Bytes(b"\x1b[?47h\x1b[2;3H\x1b[6".to_vec()))
        .unwrap();
    assert!(io.drain(&mut screen, &mut pointer));
    assert!(screen.is_alt());
    assert_eq!(*replies.lock().unwrap(), b"\x1b[1;2R");
    tx.send(ReaderEvent::Bytes(b"n\x1b[?47l\x1b[6n".to_vec()))
        .unwrap();
    assert!(io.drain(&mut screen, &mut pointer));
    assert!(!screen.is_alt());
    assert_eq!(*replies.lock().unwrap(), b"\x1b[1;2R\x1b[2;3R\x1b[1;2R");
    assert!(!io.drain(&mut screen, &mut pointer));
}

#[test]
fn selection_is_reconciled_before_a_mid_chunk_alt_swap_and_releases_capture() {
    use crate::{
        RenderViewport, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    };
    let mut io = TerminalIo::new_headless();
    let mut screen = Screen::new(2, 10);
    let mut pointer = PointerInteraction::new();
    pointer.set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
    io.feed_pty_output(&mut screen, &mut pointer, b"abcdef");
    let event =
        |x, phase| TerminalPointerEvent::new((x, 1.0), phase, TerminalPointerButton::Left, 7);
    let now = Instant::now();
    pointer.handle_pointer(&mut screen, event(21.0, TerminalPointerPhase::Down), now);
    pointer.handle_pointer(&mut screen, event(41.0, TerminalPointerPhase::Move), now);

    io.feed_pty_output(
        &mut screen,
        &mut pointer,
        b"\x1b[1;1H\x1b[4hX\x1b[4l\x1b[?47hALT",
    );
    assert!(screen.is_alt());
    assert!(screen.row_text(0).starts_with("ALT"));
    let released = pointer.handle_pointer(&mut screen, event(41.0, TerminalPointerPhase::Up), now);
    assert_eq!(released.release_pointer, Some(7));
    io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?47l");
    assert_eq!(
        pointer.bounds(),
        Some(crate::SelectionBounds {
            start_row: 0,
            start_col: 3,
            end_row: 0,
            end_col: 5,
        })
    );
}

#[test]
fn alternate_selection_restores_for_47_and_clears_for_1047_and_1049() {
    use crate::{
        RenderViewport, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    };
    for clear_entry in [b"\x1b[?1047h".as_slice(), b"\x1b[?1049h".as_slice()] {
        let mut io = TerminalIo::new_headless();
        let mut screen = Screen::new(2, 20);
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
        let event = |x, phase, id| {
            TerminalPointerEvent::new((x, 1.0), phase, TerminalPointerButton::Left, id)
        };
        let now = Instant::now();
        io.feed_pty_output(&mut screen, &mut pointer, b"primary content");
        pointer.handle_pointer(&mut screen, event(11.0, TerminalPointerPhase::Down, 1), now);
        pointer.handle_pointer(&mut screen, event(41.0, TerminalPointerPhase::Move, 1), now);
        pointer.handle_pointer(&mut screen, event(41.0, TerminalPointerPhase::Up, 1), now);
        let primary = pointer.bounds();
        assert!(primary.is_some());

        io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?47halt content");
        pointer.handle_pointer(&mut screen, event(1.0, TerminalPointerPhase::Down, 2), now);
        pointer.handle_pointer(&mut screen, event(61.0, TerminalPointerPhase::Move, 2), now);
        let alternate = pointer.bounds();
        assert!(alternate.is_some());
        assert_ne!(alternate, primary);
        io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?47l");
        assert_eq!(pointer.bounds(), primary);
        let released =
            pointer.handle_pointer(&mut screen, event(61.0, TerminalPointerPhase::Up, 2), now);
        assert_eq!(released.release_pointer, Some(2));

        io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?47h");
        assert_eq!(pointer.bounds(), alternate);
        assert!(screen.row_text(0).starts_with("alt content"));
        io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?47l");
        io.feed_pty_output(&mut screen, &mut pointer, clear_entry);
        assert_eq!(pointer.bounds(), None);
        assert!(screen.row_text(0).trim().is_empty());
        io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?47l");
        assert_eq!(pointer.bounds(), primary);
        io.feed_pty_output(&mut screen, &mut pointer, b"\x1b[?47h");
        assert_eq!(pointer.bounds(), None);
        assert!(screen.row_text(0).trim().is_empty());
    }
}

#[test]
fn local_pointer_suppresses_snap_until_review_and_capture_end() {
    use crate::{
        RenderViewport, TerminalEvent, TerminalPointerButton, TerminalPointerEvent,
        TerminalPointerPhase,
    };
    let mut terminal = crate::Terminal::new_headless(3, 12);
    terminal
        .pointer
        .set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
    for _ in 0..8 {
        terminal.put_bytes(b"line\r\n");
    }
    terminal.scroll_viewport_up(2);
    let review = terminal.screen().view_offset();
    assert!(review > 0);
    let event = |phase| {
        TerminalEvent::Pointer(TerminalPointerEvent::new(
            (1.0, 1.0),
            phase,
            TerminalPointerButton::Left,
            7,
        ))
    };
    let pressed = terminal
        .handle_event_with_outcome(event(TerminalPointerPhase::Down))
        .unwrap();
    assert_eq!(pressed.capture_pointer, Some(7));
    terminal.process_output(b"X");
    assert_eq!(terminal.screen().view_offset(), review);
    let released = terminal
        .handle_event_with_outcome(event(TerminalPointerPhase::Up))
        .unwrap();
    assert_eq!(released.release_pointer, Some(7));
    terminal.process_output(b"Y");
    assert_eq!(terminal.screen().view_offset(), review);

    terminal.scroll_viewport_to_bottom();
    terminal
        .handle_event(event(TerminalPointerPhase::Move))
        .unwrap();
    terminal.scroll_viewport_up(2);
    terminal.process_output(b"Z");
    assert_eq!(terminal.screen().view_offset(), 0);
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
