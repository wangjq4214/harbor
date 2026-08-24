//! PTY I/O and ANSI parsing — extracted from `Terminal` to separate
//! I/O lifecycle from screen state and GPU rendering.

use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::JoinHandle,
};

use harbor_pty::PtyControl;
use harbor_types::{AltScreenAction, TerminalSize};

use crate::input::TerminalInputEncoder;
use crate::parser::TerminalParser;
use crate::screen::Screen;
use crate::types::{TerminalEvent, TerminalKey, TerminalKeyboardEvent, TerminalPointerPhase};

/// The maximum number of parser chunks buffered between the blocking PTY reader
/// and the UI thread. Backpressure here bounds memory without ever blocking UI.
pub(crate) const PTY_QUEUE_CAPACITY: usize = 32;

// ── TerminalPty ───────────────────────────────────────────────────────

/// I/O and shutdown resources sharing the terminal's lifetime.
struct TerminalPty {
    output: Receiver<Vec<u8>>,
    writer: Box<dyn Write + Send>,
    reader: Option<JoinHandle<()>>,
    control: Option<PtyControl>,
    wake_pending: Arc<AtomicBool>,
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
        let wake_pending = Arc::new(AtomicBool::new(false));
        let reader_wake_pending = Arc::clone(&wake_pending);
        let reader = std::thread::Builder::new()
            .name("harbor-terminal-reader".into())
            .spawn(move || pump_reader(reader, output_tx, reader_wake_pending, wake))
            .expect("failed to start terminal PTY reader");
        Self {
            output,
            writer: Box::new(writer),
            reader: Some(reader),
            control,
            wake_pending,
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

fn pump_reader<R>(
    mut reader: R,
    output: mpsc::SyncSender<Vec<u8>>,
    wake_pending: Arc<AtomicBool>,
    wake: impl Fn() -> bool,
) where
    R: Read,
{
    let mut buffer = [0; 4096];
    loop {
        let length = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => length,
            Err(error) => {
                tracing::warn!(error = %error, "terminal pty reader stopped after read error");
                break;
            }
        };
        if output.send(buffer[..length].to_vec()).is_err() {
            break;
        }
        if !wake_pending.swap(true, Ordering::AcqRel) && !wake() {
            break;
        }
    }
    // One close-observation wake so a surviving view can drain disconnect
    // and release synchronized-output suppression without waiting for recovery.
    if !wake_pending.swap(true, Ordering::AcqRel) {
        let _ = wake();
    }
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
        Self {
            parser: TerminalParser::default(),
            pty: Some(TerminalPty::new(pty_read, pty_write, pty_control, wake)),
            suppress_scroll_snap: false,
            session_closed: false,
        }
    }

    /// Creates a headless TerminalIo without PTY resources (for tests).
    pub(crate) fn new_headless() -> Self {
        Self {
            parser: TerminalParser::default(),
            pty: None,
            suppress_scroll_snap: false,
            session_closed: false,
        }
    }

    // ── byte processing ───────────────────────────────────────────────

    /// Feeds raw PTY bytes through the streaming parser.
    pub(crate) fn feed_pty_output(&mut self, screen: &mut Screen, bytes: &[u8]) {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let result = self.parser.put_bytes(screen, remaining);
            remaining = &remaining[result.consumed..];
            if let Some(action) = result.alt_request {
                self.suppress_scroll_snap = false;
                match action {
                    AltScreenAction::Enter { clear } => screen.enter_alt(clear),
                    AltScreenAction::Exit => screen.exit_alt(),
                }
            }
        }
    }

    /// Feeds PTY output with an automatic scroll-to-bottom snap (unless suppressed).
    pub(crate) fn feed_pty_output_snapped(&mut self, screen: &mut Screen, output: &[u8]) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }
        if !screen.is_alt() && !self.suppress_scroll_snap {
            screen.scroll_to_bottom();
        }
        self.feed_pty_output(screen, output);
    }

    // ── PTY I/O ───────────────────────────────────────────────────────

    /// Drains all reader-thread output in FIFO order into the terminal parser.
    ///
    /// A wake remains pending until this has observed an empty queue. The second
    /// receive after clearing the flag closes the producer/consumer race: bytes
    /// queued just before the clear are consumed here, while later bytes post a
    /// fresh wake.
    pub(crate) fn drain(&mut self, screen: &mut Screen) -> bool {
        // Collect all available chunks first, then feed them. This avoids
        // overlapping borrows between self.pty (for reading) and the parser
        // (for feeding).
        let mut chunks: Vec<Vec<u8>> = Vec::new();
        let mut disconnected = false;
        {
            let Some(pty) = self.pty.as_ref() else {
                return false;
            };
            loop {
                match pty.output.try_recv() {
                    Ok(bytes) => chunks.push(bytes),
                    Err(TryRecvError::Empty) => {
                        pty.wake_pending.store(false, Ordering::Release);
                        match pty.output.try_recv() {
                            Ok(bytes) => chunks.push(bytes),
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        pty.wake_pending.store(false, Ordering::Release);
                        while let Ok(bytes) = pty.output.try_recv() {
                            chunks.push(bytes);
                        }
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for chunk in &chunks {
            self.feed_pty_output_snapped(screen, chunk);
            let replies = screen.drain_replies();
            if !replies.is_empty()
                && let Err(error) = self.write_pty(&replies)
            {
                tracing::warn!(error = %error, "failed to write terminal replies to pty");
            }
        }
        if disconnected && !self.session_closed {
            self.session_closed = true;
            screen.clear_synchronized_output();
        }
        !chunks.is_empty()
    }

    /// Writes bytes synchronously to the terminal's PTY input endpoint.
    pub(crate) fn write_pty(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let Some(pty) = self.pty.as_mut() else {
            anyhow::bail!("terminal has no active pty");
        };
        pty.writer.write_all(bytes).map_err(Into::into)
    }

    /// Resizes the PTY to the given terminal dimensions.
    pub(crate) fn resize_pty(&mut self, size: TerminalSize) {
        if let Some(pty) = self.pty.as_mut()
            && let Some(control) = pty.control.as_mut()
            && let Err(error) = control.resize(size)
        {
            tracing::error!(error = %format_args!("{error:#}"), "failed to resize terminal pty");
        }
    }

    // ── event handling ────────────────────────────────────────────────

    /// Drains new output, interprets scrollback navigation / wheel, then encodes
    /// remaining events for the PTY.
    ///
    /// Returns `true` when bytes were written to the PTY input endpoint.
    pub(crate) fn handle_event(
        &mut self,
        screen: &mut Screen,
        event: TerminalEvent,
    ) -> anyhow::Result<bool> {
        self.drain(screen);

        if matches!(&event, TerminalEvent::Pointer(_))
            && screen.input_modes().mouse_tracking != harbor_types::MouseTrackingMode::Disabled
        {
            if let Some(bytes) = TerminalInputEncoder::encode(&event, screen.input_modes()) {
                self.write_pty(&bytes)?;
                return Ok(true);
            }
            // A supported tracking mode without SGR encoding is intentionally
            // consumed rather than falling back to local selection/scrollback.
            return Ok(false);
        }

        if Self::try_scrollback_key(screen, &event) {
            return Ok(false);
        }
        if Self::try_scrollback_wheel(screen, &event) {
            return Ok(false);
        }

        let Some(bytes) = TerminalInputEncoder::encode(&event, screen.input_modes()) else {
            return Ok(false);
        };
        // Terminal-bound input resumes the live viewport so typed text and the
        // shell response are visible immediately after browsing scrollback.
        screen.scroll_to_bottom();
        self.suppress_scroll_snap = false;
        self.write_pty(&bytes)?;
        Ok(true)
    }

    /// Bare PageUp/PageDown/Home/End navigate scrollback on the primary screen.
    fn try_scrollback_key(screen: &mut Screen, event: &TerminalEvent) -> bool {
        let TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown { key, modifiers }) = event
        else {
            return false;
        };
        if screen.is_alt() || modifiers.shift || modifiers.ctrl || modifiers.alt || modifiers.meta {
            return false;
        }

        match key {
            TerminalKey::PageUp => {
                screen.scroll_up(screen.rows());
                true
            }
            TerminalKey::PageDown => {
                screen.scroll_down(screen.rows());
                true
            }
            TerminalKey::Home => {
                let scroll_count = screen.scroll_count();
                screen.scroll_up(scroll_count);
                true
            }
            TerminalKey::End => {
                screen.scroll_to_bottom();
                true
            }
            _ => false,
        }
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

        if screen.is_alt() {
            return true;
        }

        let lines = Self::wheel_to_lines(dy, is_pixel);
        if lines > 0 {
            screen.scroll_up(lines as usize);
        } else if lines < 0 {
            screen.scroll_down(lines.unsigned_abs());
        }
        true
    }

    fn wheel_to_lines(dy: f32, is_pixel: bool) -> isize {
        if is_pixel {
            (dy / 20.0) as isize
        } else {
            (dy * 3.0) as isize
        }
    }

    // ── mode control ──────────────────────────────────────────────────

    pub(crate) fn set_suppress_scroll_snap(&mut self, suppress: bool) {
        self.suppress_scroll_snap = suppress;
    }

    pub(crate) fn reset_scroll_snap(&mut self) {
        self.suppress_scroll_snap = false;
    }
}
