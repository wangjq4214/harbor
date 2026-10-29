#[path = "tests/pty.rs"]
mod pty;

use super::*;

#[test]
fn pty_size_from_terminal_clamps() {
    let size = TerminalSize {
        rows: 100,
        cols: 200,
    };
    let pty_size = PtySize::from_terminal(size).unwrap();
    assert_eq!(pty_size.rows, 100);
    assert_eq!(pty_size.cols, 200);
}

#[test]
fn pty_size_overflow_rejected() {
    let size = TerminalSize {
        rows: i16::MAX as usize + 1,
        cols: 80,
    };
    assert!(PtySize::from_terminal(size).is_err());
}

#[test]
fn reader_stop_acknowledges_after_normal_read_without_reentry() {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        time::Duration,
    };

    let (shutdown, completion) = ReaderShutdown::new();
    let stopping = shutdown.stopping();
    let reads = Arc::new(AtomicUsize::new(0));
    let (normal_read_finished, wait_for_stop) = mpsc::channel();
    let (allow_reentry, reentry_allowed) = mpsc::channel();
    let thread_reads = Arc::clone(&reads);
    let reader = std::thread::spawn(move || {
        let _completion = completion;
        thread_reads.fetch_add(1, Ordering::Relaxed);
        normal_read_finished.send(()).unwrap();
        reentry_allowed.recv().unwrap();

        if !stopping.load(Ordering::Acquire) {
            thread_reads.fetch_add(1, Ordering::Relaxed);
        }
    });

    wait_for_stop.recv_timeout(Duration::from_secs(1)).unwrap();
    shutdown.request_stop();
    allow_reentry.send(()).unwrap();
    assert!(
        shutdown.wait_for_completion(Duration::from_secs(1)),
        "reader must acknowledge that it will not re-enter ReadFile"
    );
    reader.join().unwrap();
    assert_eq!(reads.load(Ordering::Relaxed), 1);
}
