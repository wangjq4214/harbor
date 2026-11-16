//! READY is startup success, not a promise that the child will survive attachment.
use super::*;
use std::{collections::VecDeque, io::Read, sync::mpsc};

struct Backlog<F> {
    chunks: VecDeque<Vec<u8>>,
    observe: F,
    queued: mpsc::Sender<()>,
    hold: mpsc::Receiver<()>,
}
impl<F: FnMut(&[u8])> Read for Backlog<F> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if let Some(chunk) = self.chunks.pop_front() {
            bytes[..chunk.len()].copy_from_slice(&chunk);
            (self.observe)(&chunk);
            Ok(chunk.len())
        } else {
            let _ = self.queued.send(());
            let _ = self.hold.recv();
            Ok(0)
        }
    }
}

#[test]
fn parsed_ready_then_exit_before_poll_never_falls_back() {
    ready_then_exit_before_poll(false);
}

#[test]
fn backlogged_ready_then_exit_before_poll_never_falls_back() {
    ready_then_exit_before_poll(true);
}

fn ready_then_exit_before_poll(backlog: bool) {
    for closure in ["none", "before-poll", "while-cleaning"] {
        let (mut tabs, source) = fixture();
        let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
        let now = Instant::now();
        let clean = Arc::new(AtomicBool::new(false));
        let (queued_tx, queued_rx) = mpsc::channel();
        let (_hold_tx, hold_rx) = mpsc::channel();
        let mut hold_rx = Some(hold_rx);
        let mut probe = None;
        op.poll(&tabs, now, |_, p| {
            let p = p.unwrap();
            probe = Some(p.clone());
            let r = if backlog {
                let mut chunks: VecDeque<_> = (0..24).map(|_| b"startup\r\n".to_vec()).collect();
                chunks.push_back(p.ready.clone());
                let reader = Backlog {
                    chunks,
                    observe: p.observer(),
                    queued: queued_tx.clone(),
                    hold: hold_rx.take().unwrap(),
                };
                #[allow(clippy::arc_with_non_send_sync)]
                let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                    6,
                    80,
                    reader,
                    std::io::sink(),
                    || true,
                )));
                TerminalTabResources::new(
                    terminal.clone(),
                    TerminalWidgetBridge::new(
                        op_draw(),
                        terminal,
                        Arc::new(AtomicBool::new(false)),
                    ),
                    "backlogged-child",
                )
            } else {
                let r = resources(op_draw());
                r.terminal().lock().unwrap().put_bytes(&p.ready);
                (p.observer())(&p.ready);
                r
            };
            AttemptLaunch {
                resources: Ok(r),
                cleanup: Some(Cleanup::Controlled(clean.clone())),
            }
        });
        if backlog {
            queued_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        let p = probe.unwrap();
        assert_eq!(p.status.load(Ordering::Acquire), READY);
        let child = op.pending_resources().unwrap().terminal().clone();
        {
            let mut child = child.lock().unwrap();
            assert_eq!(
                child.parsed_output_bytes() >= p.ready_end.load(Ordering::Relaxed),
                !backlog
            );
            child.retire_session(); // Exit before the next Waiting poll, not after claim.
        }
        (p.observer())(&[]); // EOF must not revoke observed success either.
        if closure == "before-poll" {
            tabs.close(source.workspace);
        }
        assert!(matches!(op.poll(&tabs, now, no_launch), SplitPoll::Pending));
        assert!(
            op.pending_resources().is_none(),
            "dead child must not publish"
        );
        assert!(op.metadata.is_empty());
        assert!(child.lock().unwrap().is_session_closed());
        if closure == "while-cleaning" {
            tabs.close(source.workspace);
        }
        assert!(matches!(op.poll(&tabs, now, no_launch), SplitPoll::Pending));
        clean.store(true, Ordering::Release);
        match op.poll(&tabs, now, no_launch) {
            SplitPoll::Cancelled if closure != "none" => {}
            SplitPoll::Failed(error) if closure == "none" => {
                assert_eq!(error.to_string(), "split session exited before attachment");
            }
            _ => panic!("expected terminal exit/cancellation, never fallback"),
        }
        assert!(matches!(
            op.poll(&tabs, now, no_launch),
            SplitPoll::Finished
        ));
        assert!(op.next_poll(now).is_none());
    }
}
