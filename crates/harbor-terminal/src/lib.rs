mod damage;
mod input;
mod io;
mod normal_buf;
mod parser;
mod pointer;
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
    InputModes, MouseTrackingMode, PasteDisposition, TerminalSize, TerminalSnapshot, UpdateDamage,
    safe_preview_line,
};
use io::TerminalIo;
pub use normal_buf::NormalBuf;
pub use parser::TerminalParser;
pub use pointer::PointerInteraction;
pub use render::{
    Background, Cursor, Decoration, GpuContext, RenderViewport, Scrollbar, Selection,
    TerminalPrepareOptions, TerminalRenderPipeline, Text, UploadMode, UploadPlan, UploadPolicy,
    alpha_mode_supports_transparency,
};
pub use screen::{
    AltScreenAction, Cell, CellAttrs, CharacterProtection, Color, CursorShape, CursorStyleArg,
    Screen, ScreenReader, SelectionBounds,
};
pub use selection_model::{
    AutoScroll, GenPos, SelectionGranularity, SelectionModel, SelectionOutcome, SelectionRange,
};
use std::io::{Read, Write};
use std::time::Instant;
pub use types::{
    FrameDemand, RenderTarget, TerminalAppearance, TerminalEvent, TerminalEventOutcome,
    TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent, TerminalModifiers,
    TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
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
        gpu: &GpuContext,
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
        gpu: &GpuContext,
        font_book: FontBook,
        metrics: TextMetrics,
        appearance: TerminalAppearance,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let mut terminal = Self::new_headless(size.rows, size.cols);
        terminal.appearance = appearance;
        let snap = terminal.screen.terminal_snapshot();

        let renderer = TerminalRenderPipeline::new(
            gpu,
            font_book,
            metrics,
            &snap,
            appearance.clear_rgba(false),
        )
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
            pointer: PointerInteraction::new(),
            appearance: TerminalAppearance::default(),
            backdrop_available: false,
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
        self.appearance.clear_rgba(backdrop_available)
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
    pub fn prepare(&mut self, gpu: &GpuContext, damage: Option<&UpdateDamage>) {
        let now = Instant::now();
        let snap = self.screen.terminal_snapshot();
        if let Some(renderer) = &mut self.renderer {
            let tint = self.appearance.clear_rgba(self.backdrop_available);
            renderer.prepare(
                gpu,
                &snap,
                damage,
                now,
                self.pointer.bounds(),
                TerminalPrepareOptions {
                    cursor_focused: true,
                    tint,
                },
            );
        }
    }

    /// Coordinates prepare + draw for all components from a terminal-owned render target.
    pub fn render(&mut self, target: RenderTarget, pass: &mut wgpu::RenderPass, gpu: &GpuContext) {
        self.render_with_cursor_focus(target, pass, gpu, true);
    }

    /// Coordinates prepare + draw while rendering the cursor only for the focused pane.
    pub fn render_with_cursor_focus(
        &mut self,
        target: RenderTarget,
        pass: &mut wgpu::RenderPass,
        gpu: &GpuContext,
        cursor_focused: bool,
    ) {
        let Some(metrics) = self.text_metrics().copied() else {
            return;
        };
        let viewport = RenderViewport::from_target(target, &metrics);
        self.pointer.set_viewport(viewport);
        self.pointer.set_input_scale(target.scale_factor);
        let grid = viewport.compute_grid_size();
        let grid_changed = self.resize_if_changed(grid);
        self.ingest_and_blink(|io, screen| io.drain(screen));
        let now = Instant::now();
        let _ = self.pointer.tick(&mut self.screen, now);
        let snap = self.screen.terminal_snapshot();
        if let Some(renderer) = &mut self.renderer {
            renderer.sync_viewport(viewport, grid_changed);
            let tint = self.appearance.clear_rgba(self.backdrop_available);
            renderer.prepare(
                gpu,
                &snap,
                None,
                now,
                self.pointer.bounds(),
                TerminalPrepareOptions {
                    cursor_focused,
                    tint,
                },
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
        gpu: &GpuContext,
    ) {
        self.draw_retained_with_cursor_focus(target, pass, gpu, true);
    }

    /// Replays retained buffers; inactive panes retain their prepared steady cursor.
    pub fn draw_retained_with_cursor_focus(
        &mut self,
        target: RenderTarget,
        pass: &mut wgpu::RenderPass,
        gpu: &GpuContext,
        cursor_focused: bool,
    ) {
        if self.retain_geometry_changed(target)
            || self
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.cursor_focus_changed(cursor_focused))
        {
            self.render_with_cursor_focus(target, pass, gpu, cursor_focused);
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
        self.frame_demand_with_cursor_focus(now, true)
    }

    /// Reports frame demand while suppressing cursor-only work for an unfocused pane.
    pub fn frame_demand_with_cursor_focus(
        &mut self,
        now: Instant,
        cursor_focused: bool,
    ) -> FrameDemand {
        let drained = self.drain_pty();
        let snap = self.snapshot();
        let mut demand = match &self.renderer {
            Some(renderer) => renderer.cursor.frame_demand(&snap, now, cursor_focused),
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

    fn ingest_screen<R>(&mut self, ingest: impl FnOnce(&mut TerminalIo, &mut Screen) -> R) -> R {
        let was_eligible = self.screen.ordinary_present_eligible();
        let result = ingest(&mut self.io, &mut self.screen);
        if !was_eligible && self.screen.ordinary_present_eligible() {
            self.pending_ordinary_present = true;
        }
        result
    }

    /// Ingests PTY/parser work and resets blink when the cursor moved.
    fn ingest_and_blink<R>(&mut self, ingest: impl FnOnce(&mut TerminalIo, &mut Screen) -> R) -> R {
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
        self.ingest_and_blink(|io, screen| io.feed_pty_output(screen, bytes));
    }

    /// Feeds raw PTY bytes into the terminal parser, snapping to bottom first.
    pub fn process_output(&mut self, output: &[u8]) {
        self.ingest_and_blink(|io, screen| io.feed_pty_output_snapped(screen, output));
    }

    /// Drains all reader-thread output in FIFO order into the terminal parser.
    pub fn drain_pty(&mut self) -> bool {
        self.ingest_and_blink(|io, screen| io.drain(screen))
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
            TerminalEvent::Pointer(pointer) => {
                if !self.pointer.has_viewport()
                    || self.screen.input_modes().mouse_tracking
                        != harbor_types::MouseTrackingMode::Disabled
                {
                    let mut reported = self.pointer.prepare_mouse_event(*pointer);
                    if let Some(position) = self
                        .pointer
                        .report_position(reported.position, &self.screen.terminal_snapshot())
                    {
                        reported.position = position;
                    }
                    let wrote = self.ingest_screen(|io, screen| {
                        io.handle_event(screen, TerminalEvent::Pointer(reported))
                    })?;
                    outcome.capture_pointer = match pointer.phase {
                        TerminalPointerPhase::Down => {
                            self.pointer.begin_vt_capture(pointer.pointer_id);
                            Some(pointer.pointer_id)
                        }
                        _ => None,
                    };
                    outcome.release_pointer = match pointer.phase {
                        TerminalPointerPhase::Up | TerminalPointerPhase::Cancel
                            if self.pointer.end_vt_capture(pointer.pointer_id) =>
                        {
                            Some(pointer.pointer_id)
                        }
                        _ => None,
                    };
                    self.maybe_reset_blink(before, wrote);
                    return Ok(outcome);
                }
                // Preserve the review position while applying local pointer
                // intent; queued output must not snap it before hit testing.
                self.io.set_suppress_scroll_snap(true);
                self.ingest_screen(|io, screen| io.drain(screen));
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
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown { key, modifiers }) => {
                if *key == TerminalKey::Escape {
                    outcome = self.pointer.clear_selection_outcome();
                    if outcome.release_pointer.is_some() {
                        self.io.set_suppress_scroll_snap(false);
                    }
                } else if matches!(*key, TerminalKey::Character('c' | 'C')) && modifiers.ctrl {
                    if self.pointer.has_non_empty_selection() {
                        outcome.clipboard_text = self
                            .pointer
                            .bounds()
                            .map(|bounds| self.screen.selected_text(bounds));
                        outcome.redraw = true;
                    } else if modifiers.shift {
                        // Copying with no selection still clears the host
                        // clipboard, matching the always-copy shortcut.
                        outcome.clipboard_text = Some(String::new());
                    } else {
                        let wrote = self
                            .ingest_screen(|io, screen| io.handle_event(screen, event.clone()))?;
                        self.maybe_reset_blink(before, wrote);
                        return Ok(outcome);
                    }
                } else {
                    outcome = self.pointer.on_key_press_outcome();
                    if outcome.release_pointer.is_some() {
                        self.io.set_suppress_scroll_snap(false);
                    }
                    let wrote =
                        self.ingest_screen(|io, screen| io.handle_event(screen, event.clone()))?;
                    self.maybe_reset_blink(before, wrote);
                    return Ok(outcome);
                }
            }
            TerminalEvent::Focus(TerminalFocusEvent::Lost) => {
                outcome = self.pointer.cancel();
                self.io.set_suppress_scroll_snap(false);
            }
            _ => {
                let wrote =
                    self.ingest_screen(|io, screen| io.handle_event(screen, event.clone()))?;
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

    /// Drains pending PTY output before returning the current terminal snapshot.
    pub fn drain_and_snapshot(&mut self) -> TerminalSnapshot {
        self.ingest_and_blink(|io, screen| io.drain(screen));
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
        self.pointer.clear();
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
