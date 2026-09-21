mod content_anchor;
mod damage;
mod input;
mod io;
mod logical_content;
mod model;
mod normal_buf;
mod parser;
mod pointer;
mod primary_reflow;
pub mod render;
mod screen;
pub mod selection_model;
#[cfg(test)]
mod terminal_tests;
mod types;

// Re-exports for the main crate.
pub use damage::DirtyRange;
pub use harbor_config::Color;
use harbor_config::Palette;
use harbor_pty::{PtyControl, PtyEndpoints};
pub use harbor_text::{AtlasGlyph, FontBook, TextMetrics, load_system_fonts, load_system_ui_fonts};
use io::TerminalIo;
pub use model::should_confirm_multiline;
pub use model::{
    InputModes, MouseTrackingMode, PasteDisposition, TerminalSize, TerminalSnapshot, UpdateDamage,
    safe_preview_line,
};
pub use normal_buf::NormalBuf;
pub use parser::TerminalParser;
pub use pointer::PointerInteraction;
pub use render::{
    Background, Cursor, Decoration, RenderViewport, Scrollbar, Selection, TerminalGpuAccess,
    TerminalRenderPipeline, Text, UploadMode, UploadPlan, UploadPolicy,
    alpha_mode_supports_transparency,
};
pub use screen::{
    AltScreenAction, Cell, CellAttrs, CharacterProtection, CursorShape, CursorStyleArg, Screen,
    ScreenReader, SelectionBounds,
};
pub use selection_model::{
    AutoScroll, GenPos, SelectionGranularity, SelectionModel, SelectionOutcome, SelectionRange,
};
use std::io::{Read, Write};
use std::time::Instant;
pub use types::{
    FrameDemand, Preedit, RenderTarget, ShellIntegrationMarker, TerminalAppearance, TerminalEvent,
    TerminalEventOutcome, TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent,
    TerminalModifiers, TerminalOutputEvent, TerminalPointerButton, TerminalPointerEvent,
    TerminalPointerPhase, WorkingDirectoryMetadata,
};

/// Stateful terminal engine owning screen state, I/O, and rendering.
pub struct Terminal {
    /// Screen (primary buffer; alt screen handled via `saved_primary`).
    screen: Screen,
    /// PTY I/O and ANSI/VT parsing. None until initialized with PTY endpoints.
    io: TerminalIo,
    /// Encapsulated GPU render pipeline.
    renderer: Option<TerminalRenderPipeline>,
    /// Terminal-owned pointer and selection state.
    pointer: PointerInteraction,
    /// Terminal-owned default-background tint and fallback policy.
    appearance: TerminalAppearance,
    /// Host compositor fact: true when an acrylic backdrop is available.
    /// Drives the default-background tint alpha via `appearance.clear_rgba`.
    backdrop_available: bool,
    /// Terminal-owned transient IME composition presentation state.
    preedit: Option<Preedit>,
    /// Set when ingest returns synchronized output to eligible; consumed by `frame_demand`.
    pending_ordinary_present: bool,
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
        gpu: TerminalGpuAccess<'_>,
        initial_surface_size: (u32, u32),
        font_book: FontBook,
        metrics: TextMetrics,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        Self::new_with_appearance(
            size,
            pty_read,
            pty_write,
            pty_control,
            gpu,
            initial_surface_size,
            font_book,
            metrics,
            TerminalAppearance::default(),
            wake,
        )
    }

    /// Creates a rendered terminal with an explicitly owned appearance policy.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_appearance<R, W>(
        size: TerminalSize,
        pty_read: R,
        pty_write: W,
        pty_control: PtyControl,
        gpu: TerminalGpuAccess<'_>,
        initial_surface_size: (u32, u32),
        font_book: FontBook,
        metrics: TextMetrics,
        appearance: TerminalAppearance,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let mut terminal = Self::new_headless_with_appearance(size.rows, size.cols, appearance);
        let snap = terminal.screen.terminal_snapshot();

        let renderer = TerminalRenderPipeline::new(
            gpu,
            initial_surface_size,
            font_book,
            metrics,
            &snap,
            appearance.clear_rgba(false),
            appearance.palette(),
        )
        .expect("terminal render pipeline init");

        terminal.renderer = Some(renderer);
        terminal.io = TerminalIo::new(pty_read, pty_write, Some(pty_control), wake);
        terminal
    }

    /// Fallibly creates a rendered terminal while preserving pre-reader PTY teardown on error.
    ///
    /// Renderer construction happens before [`PtyEndpoints::into_parts`], so a renderer error
    /// drops the intact endpoint bundle through its safe unstarted-session shutdown path.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new_with_appearance_from_endpoints(
        size: TerminalSize,
        endpoints: PtyEndpoints,
        gpu: TerminalGpuAccess<'_>,
        initial_surface_size: (u32, u32),
        font_book: FontBook,
        metrics: TextMetrics,
        appearance: TerminalAppearance,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> anyhow::Result<Self> {
        let mut terminal = Self::new_headless_with_appearance(size.rows, size.cols, appearance);
        let snap = terminal.screen.terminal_snapshot();
        let renderer = TerminalRenderPipeline::new(
            gpu,
            initial_surface_size,
            font_book,
            metrics,
            &snap,
            appearance.clear_rgba(false),
            appearance.palette(),
        )?;
        let (pty_read, pty_write, pty_control) = endpoints.into_parts();
        terminal.renderer = Some(renderer);
        terminal.io = TerminalIo::new(pty_read, pty_write, Some(pty_control), wake);
        Ok(terminal)
    }

    /// Calculates the grid dimensions used by a rendered terminal at an explicit surface size.
    pub fn terminal_size_for(surface_size: (u32, u32), metrics: &TextMetrics) -> TerminalSize {
        RenderViewport::with_surface(
            metrics.cell_width,
            metrics.line_height,
            surface_size,
            surface_size,
        )
        .compute_grid_size()
    }

    /// Creates a headless Terminal without GPU or PTY resources (for parser tests).
    pub fn new_headless(rows: usize, cols: usize) -> Self {
        Self::new_headless_with_appearance(rows, cols, TerminalAppearance::default())
    }

    pub(crate) fn new_headless_with_appearance(
        rows: usize,
        cols: usize,
        appearance: TerminalAppearance,
    ) -> Self {
        Self {
            screen: Screen::with_palette(rows, cols, appearance.palette()),
            io: TerminalIo::new_headless(),
            renderer: None,
            pointer: PointerInteraction::new(),
            appearance,
            backdrop_available: false,
            preedit: None,
            pending_ordinary_present: false,
        }
    }

    /// Creates a headless Terminal with test PTY endpoints (no GPU).
    pub fn new_headless_with_io<R, W>(
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

    /// Returns the terminal-owned default clear color for the host environment.
    pub fn clear_rgba(&self, backdrop_available: bool) -> [f32; 4] {
        clear_rgba_for_palette(self.screen.active_palette(), backdrop_available)
    }

    /// Returns the configured tint used by a host compositor backdrop.
    pub fn appearance_rgba(&self) -> [f32; 4] {
        self.appearance.rgba()
    }

    /// Records the host compositor backdrop fact for the default background.
    ///
    /// Set once at startup; the background layer rebuilds when the resolved
    /// tint changes on the next prepare.
    pub fn set_backdrop_available(&mut self, available: bool) {
        self.backdrop_available = available;
    }

    /// Prepares GPU resources for all render components.
    pub fn prepare(&mut self, gpu: TerminalGpuAccess<'_>, damage: Option<&UpdateDamage>) {
        let now = Instant::now();
        let snap = self.screen.terminal_snapshot();
        let palette = self.screen.active_palette();
        if let Some(renderer) = &mut self.renderer {
            renderer.sync_palette(palette);
            let tint = clear_rgba_for_palette(palette, self.backdrop_available);
            renderer.prepare(
                gpu,
                &snap,
                damage,
                self.preedit.as_ref(),
                now,
                self.pointer.bounds(),
                tint,
            );
        }
    }

    /// Coordinates prepare + draw for all components from a terminal-owned render target.
    pub fn render(
        &mut self,
        target: RenderTarget,
        pass: &mut wgpu::RenderPass,
        gpu: TerminalGpuAccess<'_>,
    ) {
        let Some(metrics) = self.text_metrics().copied() else {
            return;
        };
        let viewport = RenderViewport::from_target(target, &metrics);
        self.pointer.set_viewport(viewport);
        self.pointer.set_input_scale(target.scale_factor);
        let grid = viewport.compute_grid_size();
        let grid_changed = self.resize_if_changed(grid);
        self.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer));
        let now = Instant::now();
        let _ = self.pointer.tick(&mut self.screen, now);
        let snap = self.screen.terminal_snapshot();
        let palette = self.screen.active_palette();
        if let Some(renderer) = &mut self.renderer {
            renderer.sync_viewport(viewport, grid_changed);
            renderer.sync_palette(palette);
            let tint = clear_rgba_for_palette(palette, self.backdrop_available);
            renderer.prepare(
                gpu,
                &snap,
                None,
                self.preedit.as_ref(),
                now,
                self.pointer.bounds(),
                tint,
            );
            renderer.draw(pass);
        }
    }

    /// Replays last committed GPU buffers without preparing the live Screen.
    ///
    /// Falls back to a live encode when viewport or grid geometry changed so
    /// the terminal rect matches the new allocation.
    pub fn draw_retained(
        &mut self,
        target: RenderTarget,
        pass: &mut wgpu::RenderPass,
        gpu: TerminalGpuAccess<'_>,
    ) {
        if self.retain_geometry_changed(target) {
            self.render(target, pass, gpu);
            return;
        }
        if let Some(renderer) = &self.renderer {
            renderer.draw(pass);
        }
    }

    fn retain_geometry_changed(&self, target: RenderTarget) -> bool {
        retain_geometry_changed(
            self.renderer.as_ref().map(|renderer| renderer.viewport()),
            TerminalSize {
                rows: self.screen.rows(),
                cols: self.screen.cols(),
            },
            target,
            self.text_metrics(),
        )
    }

    /// Host-neutral frame demand from ingested PTY, Cursor blink, and screen cursor flags.
    ///
    /// Without a renderer/Cursor, returns an empty demand aside from synchronized-output
    /// eligibility and a redraw notify when this ingest released ordinary presentation.
    pub fn frame_demand(&mut self, now: Instant) -> FrameDemand {
        let drained = self.drain_pty();
        let snap = self.snapshot();
        let mut demand = match &self.renderer {
            Some(renderer) => renderer.cursor.frame_demand(&snap, now),
            None => FrameDemand::empty(),
        };
        if let Some(deadline) = self.pointer.auto_scroll_deadline() {
            demand.redraw_now |= deadline <= now;
            demand.deadline = Some(
                demand
                    .deadline
                    .map_or(deadline, |current| current.min(deadline)),
            );
        }
        demand.ordinary_present_eligible = self.screen.ordinary_present_eligible();
        let released = std::mem::take(&mut self.pending_ordinary_present);
        if demand.ordinary_present_eligible && (drained || released) {
            demand.redraw_now = true;
        }
        demand
    }

    fn ingest_screen<R>(
        &mut self,
        ingest: impl FnOnce(&mut TerminalIo, &mut Screen, &mut PointerInteraction) -> R,
    ) -> R {
        let was_eligible = self.screen.ordinary_present_eligible();
        let result = ingest(&mut self.io, &mut self.screen, &mut self.pointer);
        if !was_eligible && self.screen.ordinary_present_eligible() {
            self.pending_ordinary_present = true;
        }
        result
    }

    /// Ingests PTY/parser work and resets blink when the cursor moved.
    fn ingest_and_blink<R>(
        &mut self,
        ingest: impl FnOnce(&mut TerminalIo, &mut Screen, &mut PointerInteraction) -> R,
    ) -> R {
        let before = self.cursor_pos();
        let result = self.ingest_screen(ingest);
        self.maybe_reset_blink(before, false);
        result
    }

    fn cursor_pos(&self) -> (usize, usize) {
        (self.screen.cursor_x(), self.screen.cursor_y())
    }

    fn maybe_reset_blink(&mut self, before: (usize, usize), input_wrote: bool) {
        if !(input_wrote || before != self.cursor_pos()) {
            return;
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.cursor.reset_blink(Instant::now());
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

    // ── I/O delegation ────────────────────────────────────────────────

    pub fn put_str(&mut self, text: &str) {
        self.put_bytes(text.as_bytes());
    }

    /// Feeds raw PTY bytes through the streaming parser.
    pub fn put_bytes(&mut self, bytes: &[u8]) {
        self.ingest_and_blink(|io, screen, pointer| io.feed_pty_output(screen, pointer, bytes));
    }

    /// Feeds raw PTY bytes into the terminal parser, snapping to bottom first.
    pub fn process_output(&mut self, output: &[u8]) {
        self.ingest_and_blink(|io, screen, pointer| {
            io.feed_pty_output_snapped(screen, pointer, output)
        });
    }

    /// Drains all reader-thread output in FIFO order into the terminal parser.
    pub fn drain_pty(&mut self) -> bool {
        self.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer))
    }
    /// Drains parser side effects exactly once in FIFO order.
    pub fn drain_output_events(&mut self) -> Vec<TerminalOutputEvent> {
        self.io.drain_output_events()
    }

    /// Writes bytes synchronously to the terminal's PTY input endpoint.
    pub fn write_pty(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.io.write_pty(bytes)
    }

    /// Drains new output, encodes a terminal event using current modes, and writes it to the PTY.
    pub fn handle_event(&mut self, event: TerminalEvent) -> anyhow::Result<()> {
        let _ = self.handle_event_with_outcome(event)?;
        Ok(())
    }

    /// Handles an event and returns host-facing interaction effects.
    pub fn handle_event_with_outcome(
        &mut self,
        event: TerminalEvent,
    ) -> anyhow::Result<TerminalEventOutcome> {
        let before = self.cursor_pos();
        let mut outcome = TerminalEventOutcome::default();

        match &event {
            TerminalEvent::Preedit(next) => {
                let had_preedit = self.preedit.is_some();
                if next.is_empty() {
                    outcome.redraw = self.clear_preedit();
                } else {
                    if !had_preedit {
                        self.screen.scroll_to_bottom();
                        self.io.set_suppress_scroll_snap(false);
                    }
                    outcome.redraw = self.preedit.as_ref() != Some(next);
                    self.preedit = Some(next.clone());
                }
                return Ok(outcome);
            }
            TerminalEvent::Pointer(pointer) => {
                if !self.pointer.has_viewport()
                    || self.screen.input_modes().mouse_tracking
                        != crate::model::MouseTrackingMode::Disabled
                {
                    let interrupted = self.pointer.cancel_local_pointer();
                    outcome.redraw = interrupted.redraw;
                    outcome.release_pointer = interrupted.release_pointer;
                    let mut reported = self.pointer.prepare_mouse_event(*pointer);
                    if let Some(position) = self
                        .pointer
                        .report_position(reported.position, &self.screen.terminal_snapshot())
                    {
                        reported.position = position;
                    }
                    let wrote = self.ingest_screen(|io, screen, pointer| {
                        io.handle_event(screen, pointer, TerminalEvent::Pointer(reported))
                    })?;
                    outcome.capture_pointer = match pointer.phase {
                        TerminalPointerPhase::Down => {
                            self.pointer.begin_vt_capture(pointer.pointer_id);
                            Some(pointer.pointer_id)
                        }
                        _ => None,
                    };
                    let vt_release = match pointer.phase {
                        TerminalPointerPhase::Up | TerminalPointerPhase::Cancel
                            if self.pointer.end_vt_capture(pointer.pointer_id) =>
                        {
                            Some(pointer.pointer_id)
                        }
                        _ => None,
                    };
                    outcome.release_pointer = vt_release.or(outcome.release_pointer);
                    self.maybe_reset_blink(before, wrote);
                    return Ok(outcome);
                }
                // Preserve the review position while applying local pointer
                // intent; queued output must not snap it before hit testing.
                self.io.set_suppress_scroll_snap(true);
                self.ingest_screen(|io, screen, pointer| io.drain(screen, pointer));
                outcome = self
                    .pointer
                    .handle_pointer(&mut self.screen, *pointer, Instant::now());
                if outcome.capture_pointer.is_some() || outcome.redraw {
                    self.io.set_suppress_scroll_snap(true);
                }
                if !self.pointer.has_active_pointer() && self.screen.view_offset() == 0 {
                    self.io.set_suppress_scroll_snap(false);
                }
            }
            TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(_)) => {
                outcome.redraw = self.clear_preedit();
                let wrote = self.ingest_screen(|io, screen, pointer| {
                    io.handle_event(screen, pointer, event.clone())
                })?;
                self.maybe_reset_blink(before, wrote);
                return Ok(outcome);
            }
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown { key, .. }) => {
                if *key == TerminalKey::Escape {
                    outcome = self.pointer.clear_selection_outcome();
                    if outcome.release_pointer.is_some() {
                        self.io.set_suppress_scroll_snap(false);
                    }
                } else {
                    outcome = self.pointer.on_key_press_outcome();
                    if outcome.release_pointer.is_some() {
                        self.io.set_suppress_scroll_snap(false);
                    }
                    let wrote = self.ingest_screen(|io, screen, pointer| {
                        io.handle_event(screen, pointer, event.clone())
                    })?;
                    self.maybe_reset_blink(before, wrote);
                    return Ok(outcome);
                }
            }
            TerminalEvent::Focus(focus) => {
                if matches!(focus, TerminalFocusEvent::Lost) {
                    outcome = self.pointer.cancel();
                    outcome.redraw |= self.clear_preedit();
                    self.io.set_suppress_scroll_snap(false);
                }
                let wrote = self.ingest_screen(|io, screen, pointer| {
                    io.handle_event(screen, pointer, event.clone())
                })?;
                self.maybe_reset_blink(before, wrote);
                return Ok(outcome);
            }
            _ => {
                let wrote = self.ingest_screen(|io, screen, pointer| {
                    io.handle_event(screen, pointer, event.clone())
                })?;
                self.maybe_reset_blink(before, wrote);
                return Ok(outcome);
            }
        }

        self.maybe_reset_blink(before, false);
        Ok(outcome)
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

    /// Clears transient IME composition without affecting terminal protocol state.
    pub fn clear_preedit(&mut self) -> bool {
        let cleared = self.preedit.take().is_some();
        if cleared && let Some(renderer) = &mut self.renderer {
            renderer.cursor.reset_blink(Instant::now());
        }
        cleared
    }

    /// Returns the current transient IME composition, if one is active.
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }

    /// Computes the physical candidate-window anchor from the current live cursor.
    pub fn ime_candidate_position(&self, target: RenderTarget) -> Option<(f32, f32)> {
        let preedit = self.preedit.as_ref()?;
        let metrics = self.text_metrics()?;
        let viewport = RenderViewport::from_target(target, metrics);
        let snap = self.screen.terminal_snapshot();
        let layout = crate::render::layout_preedit(
            preedit,
            (snap.cursor_x, snap.cursor_y),
            snap.rows,
            snap.cols,
        );
        let (row, col) = layout.caret;
        let (mut x, mut y) = viewport.cell_pos(row, col.min(snap.cols));
        let max_x = viewport.allocation_origin.0 + viewport.allocation_size.0 as f32;
        let max_y = viewport.allocation_origin.1 + viewport.allocation_size.1 as f32;
        x = x.clamp(viewport.allocation_origin.0, max_x);
        y = y.clamp(viewport.allocation_origin.1, max_y);
        Some((x, y))
    }

    /// Drains pending PTY output before returning the current terminal snapshot.
    pub fn drain_and_snapshot(&mut self) -> TerminalSnapshot {
        self.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer));
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
    ///
    /// PTY failures are logged and leave the screen unchanged so a later caller can retry.
    pub fn resize_if_changed(&mut self, new_size: TerminalSize) -> bool {
        match self.try_resize_if_changed(new_size) {
            Ok(changed) => changed,
            Err(error) => {
                tracing::error!(error = %format_args!("{error:#}"), "failed to resize terminal pty");
                false
            }
        }
    }

    /// Resizes the PTY and terminal grid atomically from the caller's perspective.
    ///
    /// Preparation is detached; PTY success is followed only by infallible ownership moves.
    pub fn try_resize_if_changed(&mut self, new_size: TerminalSize) -> anyhow::Result<bool> {
        let normalized = TerminalSize {
            rows: new_size.rows.max(1),
            cols: new_size.cols.max(2),
        };
        let barrier = if normalized
            != (TerminalSize {
                rows: self.screen.rows(),
                cols: self.screen.cols(),
            }) {
            self.ingest_and_blink(|io, screen, pointer| io.acquire_resize_barrier(screen, pointer))?
        } else {
            None
        };
        let result = Self::try_resize_transaction(
            &mut self.screen,
            &mut self.pointer,
            &mut self.io,
            normalized,
            |io, size| io.resize_pty(size),
        );
        drop(barrier);
        result
    }

    fn try_resize_transaction(
        screen: &mut Screen,
        pointer: &mut PointerInteraction,
        io: &mut TerminalIo,
        new_size: TerminalSize,
        resize_pty: impl FnOnce(&mut TerminalIo, TerminalSize) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        let started = Instant::now();
        let new_size = TerminalSize {
            rows: new_size.rows.max(1),
            cols: new_size.cols.max(2),
        };
        let current = TerminalSize {
            rows: screen.rows(),
            cols: screen.cols(),
        };
        let active_history_rows = screen.scroll_count();
        let saved_primary_history_rows = screen.saved_primary_scroll_count();
        if new_size == current {
            tracing::debug!(
                old_rows = current.rows,
                old_cols = current.cols,
                new_rows = new_size.rows,
                new_cols = new_size.cols,
                active_history_rows,
                saved_primary_history_rows,
                total_us = started.elapsed().as_micros() as u64,
                outcome = "unchanged",
                "terminal resize transaction"
            );
            return Ok(false);
        }

        let prepare_started = Instant::now();
        let prepared_screen = match screen.prepare_resize(new_size.rows, new_size.cols) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::debug!(
                    old_rows = current.rows,
                    old_cols = current.cols,
                    new_rows = new_size.rows,
                    new_cols = new_size.cols,
                    active_history_rows,
                    saved_primary_history_rows,
                    prepare_us = prepare_started.elapsed().as_micros() as u64,
                    total_us = started.elapsed().as_micros() as u64,
                    failed_stage = "prepare",
                    error = %error,
                    outcome = "error",
                    "terminal resize transaction"
                );
                return Err(error.into());
            }
        };
        let prepared_pointer = pointer.prepare_resize(&prepared_screen);
        let prepare_us = prepare_started.elapsed().as_micros() as u64;

        let pty_started = Instant::now();
        if let Err(error) = resize_pty(io, new_size) {
            tracing::debug!(
                old_rows = current.rows,
                old_cols = current.cols,
                new_rows = new_size.rows,
                new_cols = new_size.cols,
                active_history_rows,
                saved_primary_history_rows,
                prepare_us,
                pty_us = pty_started.elapsed().as_micros() as u64,
                total_us = started.elapsed().as_micros() as u64,
                failed_stage = "pty",
                error = %format_args!("{error:#}"),
                outcome = "error",
                "terminal resize transaction"
            );
            return Err(error);
        }
        let pty_us = pty_started.elapsed().as_micros() as u64;

        let commit_started = Instant::now();
        screen.commit_resize(prepared_screen);
        pointer.commit_resize(prepared_pointer);
        io.reset_scroll_snap();
        let commit_us = commit_started.elapsed().as_micros() as u64;
        tracing::debug!(
            old_rows = current.rows,
            old_cols = current.cols,
            new_rows = new_size.rows,
            new_cols = new_size.cols,
            active_history_rows,
            saved_primary_history_rows,
            prepare_us,
            pty_us,
            commit_us,
            total_us = started.elapsed().as_micros() as u64,
            outcome = "changed",
            "terminal resize transaction"
        );
        Ok(true)
    }

    /// Returns whether copying the current selection would produce non-empty text.
    pub fn has_non_empty_selection(&self) -> bool {
        self.pointer.has_non_empty_selection()
    }

    /// Returns selected text, or an empty string when no non-empty selection exists.
    pub fn selection_text(&self) -> String {
        self.pointer
            .bounds()
            .map(|bounds| self.screen.selected_text(bounds))
            .unwrap_or_default()
    }

    /// Copies the current selection and clears all terminal-owned selection state.
    ///
    /// The host remains responsible for applying the returned clipboard and pointer effects.
    pub fn command_copy_selection(&mut self) -> TerminalEventOutcome {
        let text = self.selection_text();
        let mut outcome = self.pointer.clear_selection_outcome();
        if outcome.release_pointer.is_some() {
            self.io.set_suppress_scroll_snap(false);
        }
        outcome.clipboard_text = Some(text);
        outcome
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

    /// Scrolls one primary-screen page toward older history.
    pub fn command_page_up(&mut self) -> TerminalEventOutcome {
        self.command_scroll(|screen| screen.scroll_up(screen.rows()))
    }

    /// Scrolls one primary-screen page toward live content.
    pub fn command_page_down(&mut self) -> TerminalEventOutcome {
        self.command_scroll(|screen| screen.scroll_down(screen.rows()))
    }

    /// Scrolls to the oldest retained primary-screen history.
    pub fn command_scroll_to_top(&mut self) -> TerminalEventOutcome {
        self.command_scroll(|screen| screen.scroll_up(screen.scroll_count()))
    }

    /// Returns the primary-screen viewport to live content.
    pub fn command_scroll_to_bottom(&mut self) -> TerminalEventOutcome {
        self.command_scroll(Screen::scroll_to_bottom)
    }

    fn command_scroll(&mut self, operation: impl FnOnce(&mut Screen)) -> TerminalEventOutcome {
        self.drain_pty();
        if self.screen.is_alt() {
            return TerminalEventOutcome::default();
        }
        let mut outcome = self.pointer.on_key_press_outcome();
        if outcome.release_pointer.is_some() {
            self.io.set_suppress_scroll_snap(false);
        }
        let before = self.screen.view_offset();
        operation(&mut self.screen);
        outcome.redraw |= before != self.screen.view_offset();
        outcome
    }
}

fn clear_rgba_for_palette(palette: Palette, backdrop_available: bool) -> [f32; 4] {
    let rgba = palette.background.components();
    if backdrop_available {
        rgba
    } else {
        [rgba[0], rgba[1], rgba[2], 1.0]
    }
}

fn retain_geometry_changed(
    current_viewport: Option<RenderViewport>,
    current_grid: TerminalSize,
    target: RenderTarget,
    metrics: Option<&TextMetrics>,
) -> bool {
    let (Some(current_viewport), Some(metrics)) = (current_viewport, metrics) else {
        return false;
    };
    let viewport = RenderViewport::from_target(target, metrics);
    let grid = viewport.compute_grid_size();
    viewport != current_viewport || grid != current_grid
}

#[cfg(test)]
mod retain_geometry_tests {
    use super::*;
    use harbor_text::TextMetrics;

    fn sample_metrics() -> TextMetrics {
        TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 16.0,
            underline_thickness: 2.0,
            strikethrough_position: 10.0,
            strikethrough_thickness: 2.0,
        }
    }

    #[test]
    fn should_keep_retain_valid_when_viewport_and_grid_match() {
        let metrics = sample_metrics();
        let target = RenderTarget::new((0.0, 0.0), (800, 600), (800, 600));
        let viewport = RenderViewport::from_target(target, &metrics);
        let grid = viewport.compute_grid_size();

        assert!(!retain_geometry_changed(
            Some(viewport),
            grid,
            target,
            Some(&metrics)
        ));
    }

    #[test]
    fn should_invalidate_retain_when_allocation_changes() {
        let metrics = sample_metrics();
        let committed = RenderTarget::new((0.0, 0.0), (800, 600), (800, 600));
        let resized = RenderTarget::new((0.0, 0.0), (400, 300), (400, 300));
        let viewport = RenderViewport::from_target(committed, &metrics);
        let grid = viewport.compute_grid_size();

        assert!(retain_geometry_changed(
            Some(viewport),
            grid,
            resized,
            Some(&metrics)
        ));
    }

    #[test]
    fn should_keep_retain_when_renderer_or_metrics_are_absent() {
        let metrics = sample_metrics();
        let target = RenderTarget::new((0.0, 0.0), (800, 600), (800, 600));
        let viewport = RenderViewport::from_target(target, &metrics);
        let grid = viewport.compute_grid_size();

        assert!(!retain_geometry_changed(None, grid, target, None));
        assert!(!retain_geometry_changed(Some(viewport), grid, target, None));
        assert!(!retain_geometry_changed(None, grid, target, Some(&metrics)));
    }

    #[test]
    fn should_invalidate_retain_when_grid_mismatches_viewport() {
        let metrics = sample_metrics();
        let target = RenderTarget::new((0.0, 0.0), (800, 600), (800, 600));
        let viewport = RenderViewport::from_target(target, &metrics);
        let grid = viewport.compute_grid_size();
        let stale_grid = TerminalSize {
            rows: grid.rows + 1,
            cols: grid.cols,
        };

        assert!(retain_geometry_changed(
            Some(viewport),
            stale_grid,
            target,
            Some(&metrics)
        ));
    }
}
