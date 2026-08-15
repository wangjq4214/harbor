mod damage;
mod input;
mod io;
mod normal_buf;
mod parser;
pub mod render;
mod screen;
pub mod selection_model;
#[cfg(test)]
mod terminal_tests;
mod types;

// Re-exports for the main crate.
pub use damage::DirtyRange;
use harbor_pty::PtyControl;
pub use harbor_text::{AtlasGlyph, FontBook, TextMetrics, load_system_fonts};
pub use harbor_types::should_confirm_multiline;
pub use harbor_types::{
    InputModes, PasteDisposition, TerminalSize, TerminalSnapshot, UpdateDamage, safe_preview_line,
};
use io::TerminalIo;
pub use normal_buf::NormalBuf;
pub use parser::TerminalParser;
pub use render::{
    Background, Cursor, Decoration, GpuContext, RenderViewport, Scrollbar, Selection,
    TerminalRenderPipeline, Text, UploadMode, UploadPlan, UploadPolicy,
};
pub use screen::{
    AltScreenAction, Cell, CellAttrs, CharacterProtection, Color, CursorShape, CursorStyleArg,
    Screen, ScreenReader, SelectionBounds,
};
pub use selection_model::{
    AutoScroll, GenPos, SelectionGranularity, SelectionModel, SelectionOutcome, SelectionRange,
};
use std::io::{Read, Write};
pub use types::{
    RenderTarget, TerminalEvent, TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent,
    TerminalModifiers, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
};

/// Stateful terminal engine owning screen state, I/O, and rendering.
pub struct Terminal {
    /// Screen (primary buffer; alt screen handled internally via in_alt).
    screen: Screen,
    /// PTY I/O and ANSI/VT parsing. None until initialized with PTY endpoints.
    io: TerminalIo,
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
        let snap = terminal.screen.terminal_snapshot();

        let renderer = TerminalRenderPipeline::new(gpu, font_book, metrics, &snap)
            .expect("terminal render pipeline init");

        terminal.renderer = Some(renderer);
        terminal.io = TerminalIo::new(pty_read, pty_write, Some(pty_control), wake);
        terminal
    }

    /// Calculates the grid dimensions used by a rendered terminal at the current surface size.
    pub fn terminal_size_for(gpu: &GpuContext, metrics: &TextMetrics) -> TerminalSize {
        let (width, height) = gpu.surface_size();
        RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            (width, height),
            (width, height),
        )
        .compute_grid_size()
    }

    /// Creates a headless Terminal without GPU or PTY resources (for parser tests).
    pub fn new_headless(rows: usize, cols: usize) -> Self {
        Self {
            screen: Screen::new(rows, cols),
            io: TerminalIo::new_headless(),
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
        terminal.io = TerminalIo::new(reader, writer, None, wake);
        terminal
    }

    // ── render orchestration ──────────────────────────────────────────

    /// Prepares GPU resources for all render components.
    pub fn prepare(&mut self, gpu: &GpuContext, damage: Option<&UpdateDamage>) {
        let snap = self.screen.terminal_snapshot();
        if let Some(renderer) = &mut self.renderer {
            renderer.prepare(gpu, &snap, damage);
        }
    }

    /// Coordinates prepare + draw for all components from a terminal-owned render target.
    pub fn render(&mut self, target: RenderTarget, pass: &mut wgpu::RenderPass, gpu: &GpuContext) {
        let Some(metrics) = self.text_metrics().copied() else {
            return;
        };
        let viewport = RenderViewport::from_target(target, &metrics);
        let grid = viewport.compute_grid_size();
        let grid_changed = self.resize_if_changed(grid);
        self.io.drain(&mut self.screen);
        let snap = self.screen.terminal_snapshot();
        if let Some(renderer) = &mut self.renderer {
            renderer.sync_viewport(viewport, grid_changed);
            renderer.prepare(gpu, &snap, None);
            renderer.draw(pass);
        }
    }

    // ── resize ────────────────────────────────────────────────────────

    /// Resizes the terminal grid and forwards changed dimensions to its PTY.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.resize_if_changed(TerminalSize { rows, cols });
    }

    // ── text / glyphs ─────────────────────────────────────────────────

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

    // ── I/O delegation ────────────────────────────────────────────────

    pub fn put_str(&mut self, text: &str) {
        self.put_bytes(text.as_bytes());
    }

    /// Feeds raw PTY bytes through the streaming parser.
    pub fn put_bytes(&mut self, bytes: &[u8]) {
        self.io.feed_pty_output(&mut self.screen, bytes);
    }

    /// Feeds raw PTY bytes into the terminal parser, snapping to bottom first.
    pub fn process_output(&mut self, output: &[u8]) {
        self.io.feed_pty_output_snapped(&mut self.screen, output);
    }

    /// Drains all reader-thread output in FIFO order into the terminal parser.
    pub fn drain_pty(&mut self) -> bool {
        self.io.drain(&mut self.screen)
    }

    /// Writes bytes synchronously to the terminal's PTY input endpoint.
    pub fn write_pty(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.io.write_pty(bytes)
    }

    /// Drains new output, encodes a widget event using current modes, and writes it to the PTY.
    pub fn handle_event(
        &mut self,
        event: harbor_widget::input::event::UiEvent,
    ) -> anyhow::Result<()> {
        self.io.handle_event(&mut self.screen, event)
    }

    /// When true, `process_output` skips the scroll-to-bottom snap.
    pub fn set_suppress_scroll_snap(&mut self, suppress: bool) {
        self.io.set_suppress_scroll_snap(suppress);
    }

    // ── screen access ─────────────────────────────────────────────────

    /// Returns the renderable screen snapshot owned by this terminal.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// Returns the current GPU-independent terminal state for the UI/update contract.
    pub fn snapshot(&self) -> TerminalSnapshot {
        self.screen.terminal_snapshot()
    }

    /// Drains pending PTY output before returning the current terminal snapshot.
    pub fn drain_and_snapshot(&mut self) -> TerminalSnapshot {
        self.io.drain(&mut self.screen);
        self.snapshot()
    }

    /// Mutable screen access for tests.
    #[cfg(test)]
    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.screen
    }

    /// Resets the screen's dirty-row tracking.
    pub fn clear_screen_dirty(&mut self) {
        self.screen.clear_dirty();
    }

    pub fn row_text(&self, row: usize) -> String {
        self.screen.row_text(row)
    }

    /// Resizes the terminal grid without GPU resources. Returns true if size changed.
    pub fn resize_if_changed(&mut self, new_size: TerminalSize) -> bool {
        let new_size = TerminalSize {
            rows: new_size.rows.max(1),
            cols: new_size.cols.max(1),
        };
        let current = TerminalSize {
            rows: self.screen.rows(),
            cols: self.screen.cols(),
        };

        if new_size == current {
            return false;
        }

        self.screen.resize(new_size.rows, new_size.cols);
        self.io.reset_scroll_snap();
        self.io.resize_pty(new_size);
        true
    }

    // ── viewport scroll ───────────────────────────────────────────────

    pub fn scroll_viewport_up(&mut self, n: usize) {
        self.screen.scroll_up(n);
    }

    pub fn scroll_viewport_down(&mut self, n: usize) {
        self.screen.scroll_down(n);
    }

    pub fn scroll_viewport_to_top(&mut self) {
        let scroll_count = self.screen.scroll_count();
        self.screen.scroll_up(scroll_count);
    }

    pub fn scroll_viewport_to_bottom(&mut self) {
        self.screen.scroll_to_bottom();
    }

    pub fn is_alt_screen(&self) -> bool {
        self.screen.is_alt()
    }
}
