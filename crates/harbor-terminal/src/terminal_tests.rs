//! Integration-style tests for the Terminal facade.
mod alternate;
mod appearance;
mod editing;
mod input;
mod io;
mod lifecycle;
mod metadata;
mod native;
mod paste;
mod pty;
mod resize;
mod scheduling;
mod selection;
mod sgr;
mod text;

use crate::io::PTY_QUEUE_CAPACITY;
use crate::screen::CellAttrs;
use crate::screen::Color;
use crate::{
    InputModes, PasteDisposition, Preedit, ShellIntegrationMarker, Terminal, TerminalAppearance,
    TerminalEvent, TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent, TerminalModifiers,
    TerminalOutputEvent, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    TerminalSize, WorkingDirectoryMetadata, safe_preview_line, should_confirm_multiline,
};
use harbor_config::{Palette, Rgba};
use std::borrow::Cow;
use std::time::Instant;

//
// `put_bytes` must NOT snap the viewport; `process_output` must.

fn wait_for_pty_wake(wake_rx: &std::sync::mpsc::Receiver<()>) {
    wake_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader should wake for queued output");
}

fn wait_for_pty_bytes(terminal: &mut Terminal, wake_rx: &std::sync::mpsc::Receiver<()>) {
    let deadline = Instant::now() + std::time::Duration::from_secs(1);
    loop {
        if terminal.drain_pty() {
            return;
        }
        let now = Instant::now();
        assert!(now < deadline, "reader should publish resumed output");
        wake_rx
            .recv_timeout(deadline.saturating_duration_since(now))
            .expect("reader should wake for resumed output");
    }
}

/// Test driver only: production callers must yield to their event loop instead.
fn resize_to_completion(terminal: &mut Terminal, size: TerminalSize) -> anyhow::Result<()> {
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    loop {
        terminal.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer));
        terminal.try_resize_if_changed(size)?;
        if terminal.screen().rows() == size.rows.max(1)
            && terminal.screen().cols() == size.cols.max(2)
        {
            return Ok(());
        }
        anyhow::ensure!(Instant::now() < deadline, "test resize failed to complete");
        std::thread::yield_now();
    }
}

struct ScriptedReader {
    chunks: std::collections::VecDeque<Vec<u8>>,
}

impl std::io::Read for ScriptedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let Some(chunk) = self.chunks.pop_front() else {
            std::thread::park();
            return Ok(0);
        };
        buffer[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

#[derive(Clone)]
struct RecordingWriter {
    bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    max_write: usize,
}

impl std::io::Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let length = buffer.len().min(self.max_write);
        self.bytes
            .lock()
            .expect("recording writer lock poisoned")
            .extend_from_slice(&buffer[..length]);
        Ok(length)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn terminal_with_io<R>(
    reader: R,
) -> (
    Terminal,
    std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    std::sync::mpsc::Receiver<()>,
)
where
    R: std::io::Read + Send + 'static,
{
    let bytes = Default::default();
    let writer = RecordingWriter {
        bytes: std::sync::Arc::clone(&bytes),
        max_write: 2,
    };
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let terminal =
        Terminal::new_headless_with_io(2, 8, reader, writer, move || wake_tx.send(()).is_ok());
    (terminal, bytes, wake_rx)
}

struct BarrierTestReader {
    state: u8,
    initial: Vec<u8>,
    blocked: std::sync::mpsc::Sender<()>,
    interrupted: std::sync::mpsc::Receiver<()>,
    after_resume: Vec<u8>,
}

impl std::io::Read for BarrierTestReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self.state {
            0 => {
                self.state = 1;
                buffer[..self.initial.len()].copy_from_slice(&self.initial);
                Ok(self.initial.len())
            }
            1 => {
                self.state = 2;
                self.blocked.send(()).unwrap();
                self.interrupted.recv().unwrap();
                Err(std::io::ErrorKind::Interrupted.into())
            }
            2 if !self.after_resume.is_empty() => {
                self.state = 3;
                buffer[..self.after_resume.len()].copy_from_slice(&self.after_resume);
                Ok(self.after_resume.len())
            }
            _ => Ok(0),
        }
    }
}

fn select_visible_viewport(terminal: &mut Terminal, pointer_id: u64) -> String {
    let cols = terminal.screen().cols();
    assert!(terminal.screen().rows() >= 6);
    let end = ((cols.saturating_sub(1) as f32 * 10.0) + 5.0, 70.0);
    terminal
        .pointer
        .set_viewport(crate::RenderViewport::with_padding(10.0, 20.0, 0.0));
    for (phase, position) in [
        (TerminalPointerPhase::Down, (5.0, 50.0)),
        (TerminalPointerPhase::Move, end),
        (TerminalPointerPhase::Up, end),
    ] {
        terminal
            .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                position,
                phase,
                TerminalPointerButton::Left,
                pointer_id,
            )))
            .unwrap();
    }
    terminal
        .command_copy_selection()
        .clipboard_text
        .expect("completed selection should be copied")
}

fn scroll_to_review_seam(terminal: &mut Terminal, next_marker: &str) {
    terminal.scroll_viewport_to_top();
    let max_steps = terminal.screen().scroll_count();
    for _ in 0..=max_steps {
        if terminal.row_text(2).trim_end() == "cdefg"
            && terminal.row_text(3).starts_with(next_marker)
        {
            return;
        }
        terminal.scroll_viewport_down(1);
    }
    panic!("review seam before {next_marker} was not retained");
}

struct CompletedScriptedReader {
    chunks: std::collections::VecDeque<Vec<u8>>,
    completed: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for CompletedScriptedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let Some(chunk) = self.chunks.pop_front() else {
            return Ok(0);
        };
        buffer[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

impl Drop for CompletedScriptedReader {
    fn drop(&mut self) {
        let _ = self.completed.send(());
    }
}

struct EofReader {
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    started: std::sync::mpsc::Sender<()>,
    exited: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for EofReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.started.send(());
        Ok(0)
    }
}

impl Drop for EofReader {
    fn drop(&mut self) {
        let _ = self.exited.send(());
    }
}

fn eof_reader() -> (
    EofReader,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::Receiver<()>,
) {
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (exited_tx, exited_rx) = std::sync::mpsc::channel();
    let reader = EofReader {
        reads: std::sync::Arc::clone(&reads),
        started: started_tx,
        exited: exited_tx,
    };
    (reader, reads, started_rx, exited_rx)
}

fn wait_for_reader_exit(exited_rx: &std::sync::mpsc::Receiver<()>) {
    exited_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("reader must drop before drain can observe disconnect");
}

struct BlockingThenChunkReader {
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    entered: std::sync::mpsc::Sender<()>,
    permit: std::sync::mpsc::Receiver<()>,
    completed: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for BlockingThenChunkReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.entered.send(());
        self.permit.recv().expect("test must release reader");
        buffer[..4].copy_from_slice(b"late");
        Ok(4)
    }
}

impl Drop for BlockingThenChunkReader {
    fn drop(&mut self) {
        let _ = self.completed.send(());
    }
}

struct FailingWriter;

impl std::io::Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("write failed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct BurstReader {
    remaining: usize,
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    completed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl std::io::Read for BurstReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            self.completed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(0);
        }
        self.remaining -= 1;
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        buffer[0] = b'x';
        Ok(1)
    }
}

struct ErrorReader {
    started: std::sync::mpsc::Sender<()>,
}

impl std::io::Read for ErrorReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.started.send(());
        Err(std::io::Error::other("read failed"))
    }
}
