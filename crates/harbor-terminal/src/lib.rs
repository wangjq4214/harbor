#![allow(unused_imports)]

mod damage;
mod input;
mod normal_buf;
mod parser;
pub mod render;
mod screen;
pub mod selection_model;
#[cfg(test)]
mod terminal_tests;

// Re-exports for the main crate.
pub use damage::DirtyRange;
pub use normal_buf::NormalBuf;
pub use parser::TerminalParser;
pub use render::{
    Background, Cursor, Decoration, GpuContext, RenderViewport, Scrollbar, Selection,
    SurfaceDisposition, SurfaceStatus, TerminalRenderPipeline, Text, UploadMode, UploadPlan,
    UploadPolicy, surface_disposition,
};
pub use screen::{AltScreenAction, Cell, CellAttrs, Color, CursorShape, Screen, SelectionBounds};
pub use selection_model::{
    AutoScroll, GenPos, SelectionGranularity, SelectionModel, SelectionOutcome, SelectionRange,
};

pub use harbor_text::{AtlasGlyph, FontBook, TextMetrics, load_system_fonts};
pub use harbor_types::should_confirm_multiline;
pub use harbor_types::{
    InputModes, PasteDisposition, TerminalSize, TerminalSnapshot, UpdateDamage, safe_preview_line,
};
pub use harbor_widget::scene::primitive::ExternalDrawId;

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
use input::TerminalInputEncoder;

/// The maximum number of parser chunks buffered between the blocking PTY reader
/// and the UI thread. Backpressure here bounds memory without ever blocking UI.
const PTY_QUEUE_CAPACITY: usize = 32;

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
}

/// Stateful terminal engine owning screen state, parser, rendering, and PTY I/O.
pub struct Terminal {
    /// Incremental ANSI/VT parser.
    parser: TerminalParser,
    /// Screen (primary buffer; alt screen handled internally via in_alt).
    normal: Screen,
    /// When true, `process_output` skips the scroll-to-bottom snap.
    suppress_scroll_snap: bool,
    /// Identifier for CustomPaint widget delegate drawing.
    draw_id: ExternalDrawId,
    /// The PTY reader, writer, and shutdown owner for this terminal instance.
    pty: Option<TerminalPty>,
    /// Encapsulated GPU render pipeline.
    renderer: Option<TerminalRenderPipeline>,
}

impl Terminal {
    /// Creates a rendered terminal and takes ownership of one PTY's endpoints.
    ///
    /// The reader is consumed by a dedicated blocking thread; the writer is used
    /// synchronously by UI-thread input handling. `pty_control` is the concrete
    /// platform lifecycle owner that preserves resize and safe reader reaping.
    #[allow(clippy::too_many_arguments)]
    pub fn new<R, W>(
        size: TerminalSize,
        pty_read: R,
        pty_write: W,
        pty_control: PtyControl,
        gpu: &GpuContext,
        font_book: FontBook,
        metrics: TextMetrics,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let mut terminal = Self::new_headless(size.rows, size.cols);
        let snap = terminal.normal.terminal_snapshot();

        let renderer = TerminalRenderPipeline::new(gpu, font_book, metrics, &snap)
            .expect("terminal render pipeline init");

        terminal.renderer = Some(renderer);
        terminal.pty = Some(TerminalPty::new(
            pty_read,
            pty_write,
            Some(pty_control),
            wake,
        ));
        terminal
    }

    /// Calculates the grid dimensions used by a rendered terminal at the current surface size.
    pub fn terminal_size_for(gpu: &GpuContext, metrics: &TextMetrics) -> TerminalSize {
        let (width, height) = gpu.surface_size();
        RenderViewport::new(metrics.cell_width, metrics.line_height)
            .compute_grid_size(width, height)
    }

    /// Creates a headless Terminal without GPU or PTY resources (for parser tests).
    pub fn new_headless(rows: usize, cols: usize) -> Self {
        Self {
            parser: TerminalParser::default(),
            normal: Screen::new(rows, cols),
            suppress_scroll_snap: false,
            draw_id: 1,
            pty: None,
            renderer: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_headless_with_io<R, W>(
        rows: usize,
        cols: usize,
        reader: R,
        writer: W,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let mut terminal = Self::new_headless(rows, cols);
        terminal.pty = Some(TerminalPty::new(reader, writer, None, wake));
        terminal
    }

    /// Fixed identifier linking this Terminal to its CustomPaint widget.
    pub fn draw_id(&self) -> ExternalDrawId {
        self.draw_id
    }

    /// Prepares GPU resources for all render components.
    pub fn prepare(&mut self, gpu: &GpuContext, damage: Option<&UpdateDamage>) {
        let snap = self.normal.terminal_snapshot();
        if let Some(renderer) = &mut self.renderer {
            renderer.prepare(gpu, &snap, damage);
        }
    }

    /// Coordinates prepare + draw for all components during widget external draw pass.
    pub fn render(
        &mut self,
        draw_id: ExternalDrawId,
        _rect: harbor_widget::layout::Rect,
        pass: &mut wgpu::RenderPass,
        gpu: &GpuContext,
    ) {
        if draw_id != self.draw_id {
            return;
        }
        self.drain_pty();
        self.prepare(gpu, None);
        if let Some(renderer) = &self.renderer {
            renderer.draw(pass);
        }
    }

    /// Resizes the terminal grid and forwards changed dimensions to its PTY.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.resize_if_changed(TerminalSize { rows, cols });
    }

    pub fn resize_gpu(&mut self, size: TerminalSize, gpu: &GpuContext) {
        self.resize_if_changed(size);
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(gpu);
        }
    }

    pub fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        if let Some(renderer) = &self.renderer {
            renderer.terminal_size(gpu)
        } else {
            TerminalSize {
                rows: self.normal.rows(),
                cols: self.normal.cols(),
            }
        }
    }

    pub fn text_metrics(&self) -> Option<&TextMetrics> {
        self.renderer.as_ref().map(|r| r.metrics())
    }

    pub fn text_glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.renderer.as_ref().and_then(|r| r.glyph(ch))
    }

    pub fn text_bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.renderer.as_ref().map(|r| r.text_bind_group())
    }

    pub fn text_bind_group_layout(&self) -> Option<&wgpu::BindGroupLayout> {
        self.renderer.as_ref().map(|r| r.text_bind_group_layout())
    }

    pub fn ensure_glyphs(&mut self, text: &str, gpu: &GpuContext) {
        if let Some(r) = &mut self.renderer {
            r.ensure_glyphs(text, gpu);
        }
    }

    pub fn put_str(&mut self, text: &str) {
        self.put_bytes(text.as_bytes());
    }

    /// Feeds raw PTY bytes through the streaming parser.
    pub fn put_bytes(&mut self, bytes: &[u8]) {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let result = {
                let parser = &mut self.parser;
                parser.put_bytes(&mut self.normal, remaining)
            };
            remaining = &remaining[result.consumed..];
            if let Some(action) = result.alt_request {
                self.normal.take_alt_request();
                self.suppress_scroll_snap = false;
                match action {
                    AltScreenAction::Enter => self.normal.enter_alt(),
                    AltScreenAction::Exit => self.normal.exit_alt(),
                }
            }
        }
    }

    /// Feeds raw PTY bytes into the terminal parser.
    pub fn process_output(&mut self, output: &[u8]) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }
        if !self.normal.is_alt() && !self.suppress_scroll_snap {
            self.normal.scroll_to_bottom();
        }
        self.put_bytes(output);
    }

    /// Drains all reader-thread output in FIFO order into the terminal parser.
    ///
    /// A wake remains pending until this has observed an empty queue. The second
    /// receive after clearing the flag closes the producer/consumer race: bytes
    /// queued just before the clear are consumed here, while later bytes post a
    /// fresh wake.
    pub fn drain_pty(&mut self) -> bool {
        let mut changed = false;
        loop {
            let received = self.pty.as_ref().map(|pty| pty.output.try_recv());
            match received {
                Some(Ok(bytes)) => {
                    self.process_output(&bytes);
                    changed = true;
                }
                Some(Err(TryRecvError::Empty | TryRecvError::Disconnected)) => {
                    let Some(pty) = self.pty.as_ref() else {
                        return changed;
                    };
                    pty.wake_pending.store(false, Ordering::Release);
                    match pty.output.try_recv() {
                        Ok(bytes) => {
                            self.process_output(&bytes);
                            changed = true;
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => return changed,
                    }
                }
                None => return changed,
            }
        }
    }

    /// Writes bytes synchronously to the terminal's PTY input endpoint.
    pub fn write_pty(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let Some(pty) = self.pty.as_mut() else {
            anyhow::bail!("terminal has no active pty");
        };
        pty.writer.write_all(bytes).map_err(Into::into)
    }

    /// Drains new output, encodes a widget event using current modes, and writes it to the PTY.
    pub fn handle_event(
        &mut self,
        event: harbor_widget::input::event::UiEvent,
    ) -> anyhow::Result<()> {
        self.drain_pty();
        let Some(bytes) = TerminalInputEncoder::encode(&event, self.normal.input_modes()) else {
            return Ok(());
        };
        // Terminal-bound input resumes the live viewport so typed text and the
        // shell response are visible immediately after browsing scrollback.
        self.normal.scroll_to_bottom();
        self.write_pty(&bytes)
    }

    /// Returns the renderable screen snapshot owned by this terminal.
    pub fn screen(&self) -> &Screen {
        &self.normal
    }

    /// Returns the current GPU-independent terminal state for the UI/update contract.
    pub fn snapshot(&self) -> TerminalSnapshot {
        self.normal.terminal_snapshot()
    }

    /// Drains pending PTY output before returning the current terminal snapshot.
    pub fn drain_and_snapshot(&mut self) -> TerminalSnapshot {
        self.drain_pty();
        self.snapshot()
    }

    /// Mutable screen access for tests.
    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.normal
    }

    /// Resets the screen's dirty-row tracking.
    pub fn clear_screen_dirty(&mut self) {
        self.normal.clear_dirty();
    }

    pub fn row_text(&self, row: usize) -> String {
        self.screen().row_text(row)
    }

    /// Resizes the terminal grid without GPU resources. Returns true if size changed.
    pub fn resize_if_changed(&mut self, new_size: TerminalSize) -> bool {
        let new_size = TerminalSize {
            rows: new_size.rows.max(1),
            cols: new_size.cols.max(1),
        };
        let current = TerminalSize {
            rows: self.screen().rows(),
            cols: self.screen().cols(),
        };

        if new_size == current {
            return false;
        }

        self.normal.resize(new_size.rows, new_size.cols);
        self.suppress_scroll_snap = false;
        if let Some(pty) = self.pty.as_mut()
            && let Some(control) = pty.control.as_mut()
            && let Err(error) = control.resize(new_size)
        {
            tracing::error!(error = %format_args!("{error:#}"), "failed to resize terminal pty");
        }
        true
    }

    pub fn scroll_viewport_up(&mut self, n: usize) {
        self.normal.scroll_up(n);
    }

    pub fn scroll_viewport_down(&mut self, n: usize) {
        self.normal.scroll_down(n);
    }

    pub fn scroll_viewport_to_top(&mut self) {
        let scroll_count = self.normal.scroll_count();
        self.normal.scroll_up(scroll_count);
    }

    pub fn scroll_viewport_to_bottom(&mut self) {
        self.normal.scroll_to_bottom();
    }

    pub fn is_alt_screen(&self) -> bool {
        self.normal.is_alt()
    }

    pub fn set_suppress_scroll_snap(&mut self, suppress: bool) {
        self.suppress_scroll_snap = suppress;
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let Some(pty) = self.pty.take() else {
            return;
        };
        let TerminalPty {
            output,
            writer,
            reader,
            control,
            wake_pending: _,
        } = pty;

        // Disconnect the reader before transferring its thread to the PTY
        // shutdown reaper. A blocked bounded send then exits without waiting on
        // the UI thread.
        drop(output);
        drop(writer);
        if let (Some(control), Some(reader)) = (control, reader) {
            control.shutdown(reader);
        }
        // Generic test readers have no platform cancellation capability; their
        // JoinHandle is deliberately detached after the output receiver closes.
    }
}
