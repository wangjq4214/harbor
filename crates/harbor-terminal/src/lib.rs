#[cfg(test)]
mod clipboard_attachment_tests;
mod content_anchor;
#[cfg(all(test, not(feature = "renderer")))]
mod core_tests;
#[path = "render/cursor_blink.rs"]
pub mod cursor_blink;
mod damage;
mod input;
mod io;
#[path = "render/layout.rs"]
pub mod layout;
mod logical_content;
mod model;
mod normal_buf;
#[cfg(test)]
mod palette_tests;
mod parser;
mod pointer;
mod primary_reflow;
#[cfg(feature = "renderer")]
pub mod render;
mod screen;
mod scrollbar_geometry;
pub mod selection_model;
#[cfg(all(test, feature = "renderer"))]
mod terminal_tests;
mod types;
mod update;

// Re-exports for the main crate.
pub use cursor_blink::CursorBlinkState;
pub use harbor_config::Color;
use harbor_config::Palette;
use harbor_pty::PtyEndpoints;
pub use harbor_text::{AtlasGlyph, FontBook, TextMetrics, load_system_fonts, load_system_ui_fonts};
use io::TerminalIo;
pub use layout::RenderViewport;
pub use model::should_confirm_multiline;
pub use model::{DirtyRange, UnderlineStyle};
pub use model::{
    InputModes, MouseTrackingMode, PasteDisposition, TerminalSize, TerminalSnapshot, UpdateDamage,
    safe_preview_line,
};
pub use normal_buf::NormalBuf;
pub use parser::TerminalParser;
pub use pointer::PointerInteraction;
#[cfg(feature = "renderer")]
pub use render::{
    Background, Cursor, Decoration, Scrollbar, Selection, TerminalGpuAccess,
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
pub use types::{ClipboardDelivery, ClipboardWrite};
pub use types::{
    FrameDemand, Preedit, RenderTarget, ShellIntegrationMarker, TerminalAppearance, TerminalEvent,
    TerminalEventOutcome, TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent,
    TerminalModifiers, TerminalOutputEvent, TerminalPointerButton, TerminalPointerEvent,
    TerminalPointerPhase, WorkingDirectoryMetadata,
};
pub use update::TerminalUpdate;

/// GPU-independent terminal state, input, and PTY session.
pub struct Terminal {
    /// Screen (primary buffer; alt screen handled via `saved_primary`).
    screen: Screen,
    /// PTY I/O and ANSI/VT parsing. None until initialized with PTY endpoints.
    io: TerminalIo,
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
    /// GPU-independent cursor blink and redraw timing.
    blink: CursorBlinkState,
    pending_ordinary_present: bool,
    /// Redraw for transient preedit changes, retained until preparation or acknowledgement.
    pending_preedit_redraw: bool,
    /// True until a renderer explicitly acknowledges a coherent update.
    update_needs_full: bool,
    /// Invalidates acknowledgements read before a renderer projection was lost.
    projection_epoch: u64,
}

impl Terminal {
    /// Attaches the sole PTY session to a newly created headless engine.
    ///
    /// Construct fallible GPU resources first: if they fail, the intact endpoint
    /// bundle is dropped using its unstarted-session shutdown protocol.
    pub fn start_session_from_endpoints(
        self,
        endpoints: PtyEndpoints,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(!self.io.has_session(), "terminal session already attached");
        let (pty_read, pty_write, pty_control) = endpoints.into_parts();
        let io = TerminalIo::try_new(pty_read, pty_write, Some(pty_control), wake)?;
        Ok(self.attach_io(io))
    }

    fn attach_io(mut self, mut io: TerminalIo) -> Self {
        // Host policy may be configured on the headless engine before session creation.
        // Carry it over before the newly queued PTY bytes are ever parsed.
        io.set_clipboard_delivery(self.io.clipboard_delivery());
        self.io = io;
        self.blink = CursorBlinkState::new(Instant::now());
        self
    }

    /// Calculates the grid dimensions used by a rendered terminal at an explicit surface size.
    #[cfg(feature = "renderer")]
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

    /// Creates a GPU-free engine with the requested appearance, before attaching a PTY session.
    pub fn new_headless_with_appearance(
        rows: usize,
        cols: usize,
        appearance: TerminalAppearance,
    ) -> Self {
        Self {
            screen: Screen::with_palette(rows, cols, appearance.palette()),
            io: TerminalIo::new_headless(),
            pointer: PointerInteraction::new(),
            appearance,
            backdrop_available: false,
            preedit: None,
            blink: CursorBlinkState::new(Instant::now()),
            pending_ordinary_present: false,
            pending_preedit_redraw: false,
            update_needs_full: true,
            projection_epoch: 0,
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

    /// Advances the GPU-independent engine for a live host draw. A failed PTY resize
    /// leaves the old grid intact so a subsequent draw can retry it.
    pub fn prepare_render_frame(
        &mut self,
        target: RenderTarget,
        metrics: &TextMetrics,
        now: Instant,
    ) -> (RenderViewport, bool) {
        let viewport = RenderViewport::from_target(target, metrics);
        self.pointer.set_viewport(viewport);
        self.pointer.set_input_scale(target.scale_factor);
        let grid_changed = self.resize_if_changed(viewport.compute_grid_size());
        self.ingest_and_blink(|io, screen, pointer| io.drain(screen, pointer));
        let _ = self.pointer.tick(&mut self.screen, now);
        (viewport, grid_changed)
    }

    /// Engine-owned cursor-blink phase for the current renderer projection.
    pub fn blink_visible_at(&self, now: Instant) -> bool {
        self.blink.phase_visible(now)
    }
    /// Whether retained projection or logical pointer mapping needs a live frame.
    /// A scale-only DPI change keeps physical geometry but changes input coordinates.
    pub fn retained_geometry_changed(
        &self,
        current_viewport: RenderViewport,
        target: RenderTarget,
        metrics: &TextMetrics,
    ) -> bool {
        target.scale_factor != self.pointer.input_scale()
            || retain_geometry_changed(
                current_viewport,
                TerminalSize {
                    rows: self.screen.rows(),
                    cols: self.screen.cols(),
                },
                target,
                metrics,
            )
    }

    /// Host-neutral frame demand from ingested PTY, Cursor blink, and screen cursor flags.
    ///
    /// The engine owns blink timing even without a GPU renderer.
    pub fn frame_demand(&mut self, now: Instant) -> FrameDemand {
        let drained = self.drain_pty();
        let snap = self.snapshot();
        let mut demand = FrameDemand {
            redraw_now: self.screen.ordinary_present_eligible()
                && (self.blink.pending_redraw()
                    || self.pending_ordinary_present
                    || self.pending_preedit_redraw),
            deadline: (snap.cursor_visible && snap.cursor_blink)
                .then(|| self.blink.next_deadline(now)),
            ordinary_present_eligible: true,
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
        let released = self.pending_ordinary_present;
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
        self.blink.reset(Instant::now());
    }

    // ── resize ────────────────────────────────────────────────────────

    /// Resizes the terminal grid and forwards changed dimensions to its PTY.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.resize_if_changed(TerminalSize { rows, cols });
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
    /// Whether reader EOF/disconnection is observed or the reader has already finished.
    /// Checks reader completion even before UI output draining. Headless terminals remain open.
    pub fn is_session_closed(&self) -> bool {
        self.io.is_session_closed()
    }

    /// Set bounded pending clipboard retention; authorization remains the host's responsibility.
    pub fn set_clipboard_delivery(&mut self, delivery: ClipboardDelivery) {
        self.io.set_clipboard_delivery(delivery);
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
                    self.pending_preedit_redraw |= outcome.redraw;
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
                            if self.pointer.release_vt_or_pending(pointer.pointer_id) =>
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

    /// Reads a complete engine projection without consuming damage or pending redraw.
    /// The first projection and projections after `invalidate_update` require a full upload.
    /// Call `acknowledge_update` only once the renderer has prepared this exact state.
    pub fn read_update(&self, now: Instant) -> TerminalUpdate {
        let snapshot = self.snapshot();
        let damage = if self.update_needs_full {
            UpdateDamage::FullUpload
        } else {
            UpdateDamage::Ranges(snapshot.dirty_ranges.clone())
        };
        let mut frame_demand = FrameDemand {
            redraw_now: self.screen.ordinary_present_eligible()
                && (self.blink.pending_redraw()
                    || self.pending_ordinary_present
                    || self.pending_preedit_redraw),
            deadline: (snapshot.cursor_visible && snapshot.cursor_blink)
                .then(|| self.blink.next_deadline(now)),
            ordinary_present_eligible: self.screen.ordinary_present_eligible(),
        };
        if let Some(deadline) = self.pointer.auto_scroll_deadline() {
            frame_demand.redraw_now |= deadline <= now;
            frame_demand.deadline =
                Some(frame_demand.deadline.map_or(deadline, |d| d.min(deadline)));
        }
        TerminalUpdate {
            snapshot,
            damage,
            selection: self.pointer.bounds(),
            preedit: self.preedit.clone(),
            appearance: TerminalAppearance::from_palette(self.screen.active_palette()),
            backdrop_available: self.backdrop_available,
            frame_demand,
            projection_epoch: self.projection_epoch,
        }
    }

    /// Acknowledge only an update whose content still matches the current engine.
    /// Returns false on intervening changes, so a later read can replay them.
    pub fn acknowledge_update(&mut self, update: &TerminalUpdate) -> bool {
        if self.projection_epoch != update.projection_epoch {
            return false;
        }
        if self.snapshot() != update.snapshot
            || self.pointer.bounds() != update.selection
            || self.preedit != update.preedit
            || TerminalAppearance::from_palette(self.screen.active_palette()) != update.appearance
            || self.backdrop_available != update.backdrop_available
        {
            return false;
        }
        self.screen.clear_dirty();
        self.update_needs_full = false;
        self.blink.take_pending_redraw();
        self.pending_ordinary_present = false;
        self.pending_preedit_redraw = false;
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
        true
    }

    /// Request a full reconstruction when a renderer's retained projection is uncertain.
    pub fn invalidate_update(&mut self) {
        self.update_needs_full = true;
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
    }

    /// Clears transient IME composition without affecting terminal protocol state.
    pub fn clear_preedit(&mut self) -> bool {
        let cleared = self.preedit.take().is_some();
        if cleared {
            self.blink.reset(Instant::now());
            self.pending_preedit_redraw = true;
        }
        cleared
    }

    /// Returns the current transient IME composition, if one is active.
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }

    /// Computes the IME anchor using metrics owned by a separately hosted renderer.
    pub fn ime_candidate_position_with_metrics(
        &self,
        target: RenderTarget,
        metrics: &TextMetrics,
    ) -> Option<(f32, f32)> {
        let preedit = self.preedit.as_ref()?;
        let viewport = RenderViewport::from_target(target, metrics);
        let snap = self.screen.terminal_snapshot();
        let layout = crate::layout::layout_preedit(
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
        let prepared_screen = match screen.prepare_resize_with_viewport(
            new_size.rows,
            new_size.cols,
            io.reflow_viewport(),
        ) {
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
    TerminalAppearance::from_palette(palette).clear_rgba(backdrop_available)
}

fn retain_geometry_changed(
    current_viewport: RenderViewport,
    current_grid: TerminalSize,
    target: RenderTarget,
    metrics: &TextMetrics,
) -> bool {
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

        assert!(!retain_geometry_changed(viewport, grid, target, &metrics));
    }

    #[test]
    fn should_invalidate_retain_when_allocation_changes() {
        let metrics = sample_metrics();
        let committed = RenderTarget::new((0.0, 0.0), (800, 600), (800, 600));
        let resized = RenderTarget::new((0.0, 0.0), (400, 300), (400, 300));
        let viewport = RenderViewport::from_target(committed, &metrics);
        let grid = viewport.compute_grid_size();

        assert!(retain_geometry_changed(viewport, grid, resized, &metrics));
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
            viewport, stale_grid, target, &metrics
        ));
    }
}
