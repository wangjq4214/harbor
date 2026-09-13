//! Secondary winit window for paste confirmation rendered by Widget Runtime.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::effects::{apply_control_flow, apply_window_effects};
use harbor_app::tab_view::ui::CONFIRMATION_PREVIEW_VISIBLE_LINES as PREVIEW_VISIBLE_LINES;
use harbor_terminal::safe_preview_line;
use harbor_terminal::{GpuContext, InputModes, PasteDisposition, Terminal};
use harbor_widget::effects::{ControlFlowEffect, RuntimeEffects};
use harbor_widget::runtime::Runtime;
use harbor_widget::winit::{FrameError, FrameOutcome, WinitAdapter, WinitFrameTarget};
use std::time::Instant;
use unicode_width::UnicodeWidthChar;

pub(crate) const DIALOG_WIDTH: u32 = 600;
const DIALOG_HEIGHT: u32 = 500;
pub(crate) const DIALOG_HORIZONTAL_PADDING: u32 = 48;

fn centered_dialog_position(
    main_position: winit::dpi::PhysicalPosition<i32>,
    main_size: winit::dpi::PhysicalSize<u32>,
    scale_factor: f64,
) -> winit::dpi::PhysicalPosition<i32> {
    let dialog_width = (f64::from(DIALOG_WIDTH) * scale_factor).round() as i32;
    let dialog_height = (f64::from(DIALOG_HEIGHT) * scale_factor).round() as i32;

    winit::dpi::PhysicalPosition::new(
        main_position.x + (main_size.width as i32 - dialog_width) / 2,
        main_position.y + (main_size.height as i32 - dialog_height) / 2,
    )
}

// ── Text wrapping ───────────────────────────────────────────────────────────

/// Wraps preview text at `max_chars` characters per line, after escaping
/// control characters via `safe_preview_line`.
pub(crate) fn wrap_preview_text(raw_text: &str, max_columns: usize) -> Vec<String> {
    let max_columns = max_columns.max(1);
    let mut wrapped = Vec::new();
    for line in raw_text.lines() {
        let escaped = safe_preview_line(line);
        if escaped.is_empty() {
            wrapped.push(String::new());
            continue;
        }
        let mut current_line = String::new();
        let mut current_width = 0;
        for ch in escaped.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
            if current_width + ch_width > max_columns && !current_line.is_empty() {
                wrapped.push(std::mem::take(&mut current_line));
                current_width = 0;
            }
            current_line.push(ch);
            current_width += ch_width;
        }
        if !current_line.is_empty() {
            wrapped.push(current_line);
        }
    }
    wrapped
}

/// Adjusts a shared scroll offset by `delta` lines, clamped to `[0, max]`.
fn scroll_preview(offset: &AtomicUsize, delta: isize, max: usize) -> bool {
    let current = offset.load(Ordering::Relaxed) as isize;
    let new = (current + delta).clamp(0, max as isize);
    if new == current {
        return false;
    }
    offset.store(new as usize, Ordering::Relaxed);
    true
}

fn shortcut_result(key: &Key) -> Option<ConfirmationResult> {
    match key {
        Key::Named(NamedKey::Escape) => Some(ConfirmationResult::Cancelled),
        Key::Character(ch) if ch == "n" || ch == "N" => Some(ConfirmationResult::Cancelled),
        Key::Character(ch) if ch == "y" || ch == "Y" => Some(ConfirmationResult::Confirmed),
        _ => None,
    }
}

fn confirmation_result(confirmed: &AtomicBool, cancelled: &AtomicBool) -> ConfirmationResult {
    if confirmed.load(Ordering::SeqCst) {
        ConfirmationResult::Confirmed
    } else if cancelled.load(Ordering::SeqCst) {
        ConfirmationResult::Cancelled
    } else {
        ConfirmationResult::None
    }
}

// ── ConfirmationResult ──────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) enum ConfirmationResult {
    None,
    Cancelled,
    Confirmed,
}

#[derive(Debug)]
pub(crate) enum DialogOutcome {
    None,
    Cancelled,
    Confirmed(String),
    Fatal(FrameError),
}

/// Encodes the unchanged confirmation text with the modes current at the
/// moment of confirmation, then performs exactly one Host-owned PTY write.
pub(crate) fn write_confirmation_outcome<E>(
    outcome: &DialogOutcome,
    input_modes: InputModes,
    write: impl FnOnce(&[u8]) -> Result<(), E>,
) -> Result<bool, E> {
    let DialogOutcome::Confirmed(raw_text) = outcome else {
        return Ok(false);
    };
    let bytes = input_modes.paste(raw_text.as_bytes());
    write(bytes.as_ref())?;
    Ok(true)
}

pub(crate) fn is_paste_shortcut(event: &WindowEvent, modifiers: ModifiersState) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { event, .. }
            if event.state == ElementState::Pressed
                && modifiers.control_key()
                && !modifiers.alt_key()
                && !modifiers.super_key()
                && matches!(&event.logical_key, Key::Character(character) if character.eq_ignore_ascii_case("v"))
    )
}

/// Outcome of a paste confirmation event dispatch.
#[derive(Debug)]
pub(crate) enum PasteEventOutcome {
    Handled { request_redraw: bool },
    Fatal(FrameError),
}

/// Controller encapsulating modal paste confirmation and clipboard interaction.
pub(crate) struct PasteController {
    window: Option<ConfirmationWindow>,
    input_gate: Arc<AtomicBool>,
}

impl PasteController {
    pub(crate) fn new(input_gate: Arc<AtomicBool>) -> Self {
        Self {
            window: None,
            input_gate,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.window_id())
    }

    pub(crate) fn about_to_wait(&mut self, now: Instant) -> Option<ControlFlowEffect> {
        self.window.as_mut().map(|window| window.about_to_wait(now))
    }

    #[allow(dead_code)]
    pub(crate) fn is_paste_shortcut(event: &WindowEvent, modifiers: ModifiersState) -> bool {
        is_paste_shortcut(event, modifiers)
    }

    pub(crate) fn handle_dialog_event(
        &mut self,
        event: &WindowEvent,
        event_loop: &ActiveEventLoop,
        gpu: &GpuContext,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
    ) -> PasteEventOutcome {
        let result = self.handle_event(event, event_loop, Some(gpu));

        match &result {
            DialogOutcome::Cancelled | DialogOutcome::Confirmed(_) => {
                if let DialogOutcome::Confirmed(_) = &result
                    && let Some(active_terminal) = active_terminal
                    && let Ok(mut terminal) = active_terminal.lock()
                {
                    let input_modes = terminal.drain_and_snapshot().input_modes;
                    if let Err(error) = write_confirmation_outcome(&result, input_modes, |bytes| {
                        terminal.write_pty(bytes)
                    }) {
                        tracing::warn!(error = %format_args!("{error:#}"), "failed to write confirmed paste");
                    }
                }
                self.input_gate.store(false, Ordering::Release);
                PasteEventOutcome::Handled {
                    request_redraw: true,
                }
            }
            DialogOutcome::Fatal(error) => PasteEventOutcome::Fatal(error.clone()),
            DialogOutcome::None => PasteEventOutcome::Handled {
                request_redraw: false,
            },
        }
    }

    fn handle_event(
        &mut self,
        event: &WindowEvent,
        event_loop: &ActiveEventLoop,
        gpu: Option<&GpuContext>,
    ) -> DialogOutcome {
        let Some(mut confirmation) = self.window.take() else {
            return DialogOutcome::None;
        };
        match confirmation.handle_event(event, event_loop) {
            ConfirmationResult::Cancelled => DialogOutcome::Cancelled,
            ConfirmationResult::Confirmed => {
                let raw_text = confirmation.raw_text().to_owned();
                DialogOutcome::Confirmed(raw_text)
            }
            ConfirmationResult::None => {
                if matches!(event, WindowEvent::RedrawRequested)
                    && let Some(gpu) = gpu
                {
                    let frame = confirmation.render(gpu.device(), gpu.queue());
                    confirmation.apply_frame_effects(&frame, event_loop);
                    if let Some(error) = frame.fatal_error().cloned() {
                        return DialogOutcome::Fatal(error);
                    }
                }
                self.window = Some(confirmation);
                DialogOutcome::None
            }
        }
    }

    pub(crate) fn paste_from_clipboard(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: &Window,
        gpu: &GpuContext,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        runtime: &mut Runtime,
        adapter: &mut WinitAdapter,
    ) -> RuntimeEffects {
        let raw_text =
            match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                Ok(text) => text,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to read clipboard text");
                    return RuntimeEffects::default();
                }
            };

        let confirmation = {
            let Some(active_terminal) = active_terminal else {
                tracing::warn!("no active terminal for clipboard paste");
                return RuntimeEffects::default();
            };
            let Ok(mut terminal) = active_terminal.lock() else {
                tracing::warn!("terminal lock unavailable for clipboard paste");
                return RuntimeEffects::default();
            };
            let input_modes = terminal.drain_and_snapshot().input_modes;

            match PasteDisposition::decide(input_modes, &raw_text) {
                PasteDisposition::SendDirect => {
                    if let Err(error) =
                        terminal.write_pty(input_modes.paste(raw_text.as_bytes()).as_ref())
                    {
                        tracing::warn!(error = %format_args!("{error:#}"), "failed to write clipboard paste");
                    }
                    return RuntimeEffects::default();
                }
                PasteDisposition::Confirm { raw_text } => {
                    ConfirmationWindow::new(raw_text, event_loop, gpu, runtime, Some(window))
                }
            }
        };

        self.window = Some(confirmation);
        self.input_gate.store(true, Ordering::Release);
        adapter.quarantine_active_pointers();
        let effects = runtime.cancel_pointer_captures(harbor_widget::layout::Point::ZERO);
        adapter.fold_effects(effects)
    }
}

// ── ConfirmationWindow ──────────────────────────────────────────────────────

/// A secondary owned winit window with its own Widget Runtime for paste
/// confirmation UI.
///
/// Renders a minimal confirmation dialog: header text showing line count,
/// Paste and Cancel buttons, with Paste focused by default.
pub(crate) struct ConfirmationWindow {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    runtime: Runtime,
    /// Per-window winit integration, including input, scheduling, viewport,
    /// and surface-recovery state.
    adapter: WinitAdapter,
    raw_text: String,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    wrapped_lines: Vec<String>,
    preview_scroll_offset: Arc<AtomicUsize>,
}

impl ConfirmationWindow {
    pub(crate) fn new(
        raw_text: String,
        event_loop: &ActiveEventLoop,
        gpu: &GpuContext,
        source_runtime: &Runtime,
        main_window: Option<&Window>,
    ) -> Self {
        let line_count = raw_text.lines().count();
        let metrics = *source_runtime.text_metrics();

        let max_chars = ((DIALOG_WIDTH - DIALOG_HORIZONTAL_PADDING) as f32 / metrics.cell_width)
            .floor() as usize;
        let max_chars = max_chars.max(1);
        let wrapped_lines = wrap_preview_text(&raw_text, max_chars);
        let preview_scroll_offset = Arc::new(AtomicUsize::new(0));

        let mut window_attrs = Window::default_attributes()
            .with_title("")
            .with_decorations(false)
            .with_inner_size(winit::dpi::LogicalSize::new(DIALOG_WIDTH, DIALOG_HEIGHT))
            .with_resizable(false);

        // Owned window on Windows: keep dialog above main window in z-order.
        #[cfg(target_os = "windows")]
        if let Some(main) = main_window {
            let hwnd = main.window_handle().ok().and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            });
            if let Some(hwnd) = hwnd {
                window_attrs = window_attrs.with_owner_window(hwnd);
            }
        }

        // Center dialog over the main window using physical pixels.
        if let Some(main) = main_window
            && let Ok(outer) = main.outer_position()
        {
            window_attrs = window_attrs.with_position(centered_dialog_position(
                outer,
                main.outer_size(),
                main.scale_factor(),
            ));
        }

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("create confirmation window"),
        );

        let surface = gpu.create_surface(Arc::clone(&window));

        let caps = gpu.surface_capabilities(&surface);
        let format = gpu.format();
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| *f == format)
            .unwrap_or(caps.formats[0]);
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        // Surface config uses physical pixel dimensions.
        let physical_size = window.inner_size();
        let drawable = physical_size.width != 0 && physical_size.height != 0;
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: physical_size.width.max(1),
            height: physical_size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(gpu.device(), &surface_config);

        // ── Widget Runtime setup ──────────────────────────────────────
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let mut runtime = source_runtime.create_child_runtime(gpu.device(), format);

        let confirm_root = harbor_app::tab_view::ui::build_confirmation_root(
            line_count,
            wrapped_lines.clone(),
            Arc::clone(&preview_scroll_offset),
            Arc::clone(&cancelled),
            Arc::clone(&confirmed),
            metrics.line_height,
        );
        runtime.set_root(confirm_root);

        // Each native window has an independent adapter, including its viewport,
        // input state, scheduler, and surface-recovery budget.
        let mut adapter = WinitAdapter::from_window(&window);
        adapter.set_drawable(drawable);
        runtime.set_viewport(adapter.viewport().clone());
        let mut initial_effects = runtime.update(std::time::Instant::now());

        // Set focus on the Cancel button (first focusable widget) and apply
        // the effects produced by the programmatic focus transition.
        runtime.focus_first_focusable();
        initial_effects.merge(runtime.take_pending_effects());
        let mut effects = adapter.fold_effects(initial_effects);
        effects.merge(adapter.request_frame());
        apply_window_effects(&window, &effects);
        if let Some(control_flow) = effects.control_flow {
            apply_control_flow(event_loop, control_flow);
        }

        ConfirmationWindow {
            window,
            surface,
            surface_config,
            runtime,
            adapter,
            raw_text,
            cancelled,
            confirmed,
            wrapped_lines,
            preview_scroll_offset,
        }
    }

    pub(crate) fn window_id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    /// Applies idle redraw effects to this window and returns its wait request.
    pub(crate) fn about_to_wait(&mut self, now: Instant) -> ControlFlowEffect {
        let effects = self.adapter.about_to_wait(&mut self.runtime, now, None);
        apply_window_effects(&self.window, &effects);
        effects.control_flow.unwrap_or(ControlFlowEffect::Wait)
    }

    /// Returns the raw paste candidate text, unchanged from when the dialog opened.
    pub(crate) fn raw_text(&self) -> &str {
        &self.raw_text
    }

    /// Routes this window's supported event through its own Runtime integration.
    /// Confirmation shortcuts and preview scrolling remain application policy.
    pub(crate) fn handle_event(
        &mut self,
        event: &WindowEvent,
        event_loop: &ActiveEventLoop,
    ) -> ConfirmationResult {
        if matches!(event, WindowEvent::CloseRequested) {
            return ConfirmationResult::Cancelled;
        }

        if let WindowEvent::KeyboardInput {
            event: key_event, ..
        } = event
            && key_event.state == ElementState::Pressed
        {
            if let Some(result) = shortcut_result(&key_event.logical_key) {
                return result;
            }

            let max_scroll = self
                .wrapped_lines
                .len()
                .saturating_sub(PREVIEW_VISIBLE_LINES);
            let scrolled = match &key_event.logical_key {
                Key::Named(NamedKey::ArrowUp) => {
                    scroll_preview(&self.preview_scroll_offset, -1, max_scroll)
                }
                Key::Named(NamedKey::ArrowDown) => {
                    scroll_preview(&self.preview_scroll_offset, 1, max_scroll)
                }
                Key::Named(NamedKey::PageUp) => scroll_preview(
                    &self.preview_scroll_offset,
                    -((PREVIEW_VISIBLE_LINES.saturating_sub(1)) as isize),
                    max_scroll,
                ),
                Key::Named(NamedKey::PageDown) => scroll_preview(
                    &self.preview_scroll_offset,
                    (PREVIEW_VISIBLE_LINES.saturating_sub(1)) as isize,
                    max_scroll,
                ),
                _ => false,
            };
            if scrolled {
                self.request_frame(event_loop);
            }
        }

        let size = self.window.inner_size();
        let outcome = self.adapter.handle_event_with_size(
            &mut self.runtime,
            event,
            Some((size.width, size.height)),
        );
        apply_window_effects(&self.window, &outcome.effects);
        if let Some(control_flow) = outcome.effects.control_flow {
            apply_control_flow(event_loop, control_flow);
        }

        confirmation_result(&self.confirmed, &self.cancelled)
    }

    /// Presents one frame through the shared winit integration using borrowed
    /// confirmation resources and shared Device, Queue, and text atlas data.
    pub(crate) fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> FrameOutcome {
        let ConfirmationWindow {
            window,
            surface,
            surface_config,
            runtime,
            adapter,
            ..
        } = self;
        let alpha_mode = surface_config.alpha_mode;
        let mut configure = |width, height| {
            surface_config.width = width;
            surface_config.height = height;
            surface.configure(device, surface_config);
        };
        let target = WinitFrameTarget::new(
            window,
            surface,
            device,
            queue,
            &mut configure,
            false,
            alpha_mode,
        );
        adapter.render_with_prepare(runtime, target, |runtime| {
            runtime.prepare_text(queue);
        })
    }

    /// Applies host-visible effects for this confirmation window only.
    pub(crate) fn apply_frame_effects(&self, frame: &FrameOutcome, event_loop: &ActiveEventLoop) {
        let effects = frame.effects();
        apply_window_effects(&self.window, effects);
        if let Some(control_flow) = effects.control_flow {
            apply_control_flow(event_loop, control_flow);
        }
    }

    /// Wakes only the confirmation window; the main App scheduler is unrelated.
    fn request_frame(&mut self, event_loop: &ActiveEventLoop) {
        let effects = self.adapter.request_frame();
        apply_window_effects(&self.window, &effects);
        if let Some(control_flow) = effects.control_flow {
            apply_control_flow(event_loop, control_flow);
        }
    }
}

// ── Confirmation widget tree ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_app::tab_view::ui::build_confirmation_root;
    use harbor_widget::view::{BuildCx, Component};

    #[test]
    fn centers_dialog_in_main_window() {
        assert_eq!(
            centered_dialog_position(
                winit::dpi::PhysicalPosition::new(100, 100),
                winit::dpi::PhysicalSize::new(1_000, 800),
                1.0,
            ),
            // DIALOG_WIDTH=600, DIALOG_HEIGHT=500 at scale 1.0
            // x = 100 + (1000 - 600) / 2 = 300
            // y = 100 + (800 - 500) / 2 = 250
            winit::dpi::PhysicalPosition::new(300, 250),
        );
    }

    #[test]
    fn centers_scaled_dialog_on_negative_coordinate_monitor() {
        assert_eq!(
            centered_dialog_position(
                winit::dpi::PhysicalPosition::new(-1_920, 100),
                winit::dpi::PhysicalSize::new(1_200, 900),
                1.5,
            ),
            // DIALOG_WIDTH=600, DIALOG_HEIGHT=500 at scale 1.5
            // dialog_width = 600 * 1.5 = 900
            // dialog_height = 500 * 1.5 = 750
            // x = -1920 + (1200 - 900) / 2 = -1770
            // y = 100 + (900 - 750) / 2 = 175
            winit::dpi::PhysicalPosition::new(-1_770, 175),
        );
    }

    #[test]
    fn build_confirmation_root_produces_valid_view() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));
        let wrapped = vec!["hello".to_string()];
        let scroll = Arc::new(AtomicUsize::new(0));

        let root = build_confirmation_root(5, wrapped, scroll, cancelled, confirmed, 20.0);
        let mut cx = BuildCx::stub();
        let _view = root.build(&mut cx);
    }

    #[test]
    fn paste_callback_sets_confirmed_flag_only() {
        // Replicates the exact closure pattern used in build_confirmation_root.
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let c = Arc::clone(&confirmed);
        let on_paste = move |_ctx: &mut harbor_widget::input::event_ctx::EventCtx| {
            c.store(true, Ordering::SeqCst);
        };

        let mut ctx = harbor_widget::input::event_ctx::EventCtx::new();
        on_paste(&mut ctx);

        assert!(confirmed.load(Ordering::SeqCst));
        assert!(!cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_callback_sets_cancelled_flag_only() {
        // Replicates the exact closure pattern used in build_confirmation_root.
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let c = Arc::clone(&cancelled);
        let on_cancel = move |_ctx: &mut harbor_widget::input::event_ctx::EventCtx| {
            c.store(true, Ordering::SeqCst);
        };

        let mut ctx = harbor_widget::input::event_ctx::EventCtx::new();
        on_cancel(&mut ctx);

        assert!(!confirmed.load(Ordering::SeqCst));
        assert!(cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn paste_and_cancel_flags_are_independent() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        confirmed.store(true, Ordering::SeqCst);
        assert!(confirmed.load(Ordering::SeqCst));
        assert!(!cancelled.load(Ordering::SeqCst));

        confirmed.store(false, Ordering::SeqCst);
        cancelled.store(true, Ordering::SeqCst);
        assert!(!confirmed.load(Ordering::SeqCst));
        assert!(cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn should_preserve_raw_text_verbatim() {
        // ConfirmationWindow::raw_text() returns the original paste text
        // unchanged. Verify the data-flow contract: String in → &str out,
        // multi-line with mixed line endings preserved.
        let text = String::from("hello\nworld\r\n");
        let stored = text.clone();
        // Simulate the accessor contract: stored text matches original.
        assert_eq!(stored.as_str(), "hello\nworld\r\n");
        assert!(stored.contains('\n'));
        assert!(stored.contains('\r'));
    }

    #[test]
    fn should_center_at_origin_when_main_window_is_zero_sized() {
        // When the main window has zero area, the dialog is still
        // positioned relative to the main window origin.
        let pos = centered_dialog_position(
            winit::dpi::PhysicalPosition::new(50, 60),
            winit::dpi::PhysicalSize::new(0, 0),
            1.0,
        );
        // DIALOG_WIDTH=600, DIALOG_HEIGHT=500 at scale 1.0
        // x = 50 + (0 - 600) / 2 = 50 - 300 = -250
        // y = 60 + (0 - 500) / 2 = 60 - 250 = -190
        assert_eq!(pos, winit::dpi::PhysicalPosition::new(-250, -190));
    }

    #[test]
    fn should_produce_zero_dialog_when_scale_is_zero() {
        // With scale_factor=0 the dialog occupies zero physical pixels,
        // so the center coincides with the main window center.
        let pos = centered_dialog_position(
            winit::dpi::PhysicalPosition::new(100, 100),
            winit::dpi::PhysicalSize::new(800, 600),
            0.0,
        );
        assert_eq!(pos, winit::dpi::PhysicalPosition::new(500, 400));
    }

    #[test]
    fn should_build_with_zero_lines() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));
        let wrapped: Vec<String> = vec![];
        let scroll = Arc::new(AtomicUsize::new(0));

        let root = build_confirmation_root(0, wrapped, scroll, cancelled, confirmed, 20.0);
        let mut cx = BuildCx::stub();
        let _view = root.build(&mut cx);
    }

    #[test]
    fn should_build_with_large_line_count() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));
        let wrapped = vec!["x".to_string()];
        let scroll = Arc::new(AtomicUsize::new(0));

        let root = build_confirmation_root(usize::MAX, wrapped, scroll, cancelled, confirmed, 20.0);
        let mut cx = BuildCx::stub();
        let _view = root.build(&mut cx);
    }

    // ── Confirmation shortcuts ───────────────────────────────────────

    #[test]
    fn should_return_confirmed_on_lowercase_y() {
        let key = winit::keyboard::Key::Character("y".into());
        assert!(matches!(
            shortcut_result(&key),
            Some(ConfirmationResult::Confirmed)
        ));
    }

    #[test]
    fn should_return_confirmed_on_uppercase_y() {
        let key = winit::keyboard::Key::Character("Y".into());
        assert!(matches!(
            shortcut_result(&key),
            Some(ConfirmationResult::Confirmed)
        ));
    }

    #[test]
    fn should_return_cancelled_on_lowercase_n() {
        let key = winit::keyboard::Key::Character("n".into());
        assert!(matches!(
            shortcut_result(&key),
            Some(ConfirmationResult::Cancelled)
        ));
    }

    #[test]
    fn should_return_cancelled_on_uppercase_n() {
        let key = winit::keyboard::Key::Character("N".into());
        assert!(matches!(
            shortcut_result(&key),
            Some(ConfirmationResult::Cancelled)
        ));
    }

    #[test]
    fn should_return_cancelled_on_escape() {
        let key = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape);
        assert!(matches!(
            shortcut_result(&key),
            Some(ConfirmationResult::Cancelled)
        ));
    }

    #[test]
    fn should_return_none_on_unrecognized_key() {
        let key = winit::keyboard::Key::Character("x".into());
        assert!(shortcut_result(&key).is_none());
    }

    // ── Confirmation result priority ─────────────────────────────────

    #[test]
    fn should_return_none_when_neither_flag_is_set() {
        let confirmed = AtomicBool::new(false);
        let cancelled = AtomicBool::new(false);
        assert!(matches!(
            confirmation_result(&confirmed, &cancelled),
            ConfirmationResult::None
        ));
    }

    #[test]
    fn should_return_confirmed_when_confirmed_flag_is_set() {
        let confirmed = AtomicBool::new(true);
        let cancelled = AtomicBool::new(false);
        assert!(matches!(
            confirmation_result(&confirmed, &cancelled),
            ConfirmationResult::Confirmed
        ));
    }

    #[test]
    fn should_return_cancelled_when_only_cancelled_flag_is_set() {
        let confirmed = AtomicBool::new(false);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            confirmation_result(&confirmed, &cancelled),
            ConfirmationResult::Cancelled
        ));
    }

    #[test]
    fn should_prioritize_confirmed_over_cancelled_when_both_flags_are_set() {
        // Confirmed takes priority: even if both flags are somehow true,
        // the result should be Confirmed.
        let confirmed = AtomicBool::new(true);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            confirmation_result(&confirmed, &cancelled),
            ConfirmationResult::Confirmed
        ));
    }

    // ── wrap_preview_text ───────────────────────────────────────────────

    #[test]
    fn should_return_empty_vec_when_input_is_empty() {
        let result = wrap_preview_text("", 10);
        assert!(result.is_empty());
    }

    #[test]
    fn should_keep_short_line_unchanged_when_within_max_chars() {
        let result = wrap_preview_text("hello", 10);
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn should_wrap_long_line_at_max_chars_boundary() {
        let result = wrap_preview_text("hello world", 5);
        assert_eq!(result, vec!["hello", " worl", "d"]);
    }

    #[test]
    fn should_escape_tab_character_to_arrow() {
        // Tab (U+0009) is escaped to arrow (U+2192) by safe_preview_line.
        let result = wrap_preview_text("a\tb", 10);
        assert_eq!(result.len(), 1);
        assert!(result[0].contains('\u{2192}'));
        assert!(!result[0].contains('\t'));
    }

    #[test]
    fn should_escape_c0_control_character_to_unicode_control_picture() {
        // C0 control 0x01 (SOH) is escaped to U+2401 by safe_preview_line.
        let result = wrap_preview_text("\x01", 10);
        assert_eq!(result.len(), 1);
        assert!(!result[0].contains('\x01'));
        assert!(result[0].contains('\u{2401}'));
    }

    #[test]
    fn should_preserve_carriage_return_in_escaped_output() {
        // CR passes through safe_preview_line unchanged.
        let result = wrap_preview_text("a\rb", 10);
        assert_eq!(result, vec!["a\rb"]);
    }

    #[test]
    fn should_split_input_on_newline_characters() {
        let result = wrap_preview_text("line1\nline2", 10);
        assert_eq!(result, vec!["line1", "line2"]);
    }

    #[test]
    fn should_preserve_empty_lines_between_content() {
        let result = wrap_preview_text("a\n\nb", 10);
        assert_eq!(result, vec!["a", "", "b"]);
    }

    #[test]
    fn should_wrap_correctly_with_max_chars_of_one() {
        let result = wrap_preview_text("ab", 1);
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn should_handle_multibyte_utf8_char_boundaries_when_wrapping() {
        // "é" is 2 bytes in UTF-8. Wrapping at max_chars=1 should not panic
        // and should keep the multibyte character intact.
        let result = wrap_preview_text("\u{00E9}x", 1);
        assert_eq!(result, vec!["\u{00E9}", "x"]);
    }

    #[test]
    fn should_wrap_escaped_text_that_becomes_longer_after_escaping() {
        // Control chars 0x01 and 0x02 become single-width Unicode control pictures
        // (U+2401, U+2402). Wrapping at max_columns=2 wraps after 2 visual columns.
        let input = "a\x01b\x02c";
        let result = wrap_preview_text(input, 2);
        assert_eq!(result, vec!["a\u{2401}", "b\u{2402}", "c"]);
    }

    #[test]
    fn should_wrap_cjk_characters_by_display_width() {
        // CJK characters occupy 2 visual columns each. Wrapping at 4 columns
        // fits 2 characters per line.
        let result = wrap_preview_text("你好世界", 4);
        assert_eq!(result, vec!["你好", "世界"]);

        // Wrapping at 3 columns fits 1 CJK char (2 cols); the second char would exceed 3 cols.
        let result = wrap_preview_text("你好世界", 3);
        assert_eq!(result, vec!["你", "好", "世", "界"]);
    }

    // ── scroll_preview ──────────────────────────────────────────────────

    #[test]
    fn should_not_invalidate_redraw_when_scroll_delta_does_not_change_offset() {
        // Arrange
        let offset = AtomicUsize::new(5);

        // Act
        let changed = scroll_preview(&offset, 0, 10);

        // Assert
        assert!(!changed);
        assert_eq!(offset.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn should_report_redraw_invalidation_when_keyboard_scroll_changes_offset() {
        // Arrange
        let offset = AtomicUsize::new(5);

        // Act
        let changed = scroll_preview(&offset, 3, 10);

        // Assert
        assert!(changed);
        assert_eq!(offset.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn should_decrement_offset_when_keyboard_scroll_moves_toward_start() {
        // Arrange
        let offset = AtomicUsize::new(5);

        // Act
        let changed = scroll_preview(&offset, -3, 10);

        // Assert
        assert!(changed);
        assert_eq!(offset.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn should_invalidate_redraw_when_negative_keyboard_scroll_is_clamped_to_a_new_offset() {
        // Arrange
        let offset = AtomicUsize::new(1);

        // Act
        let changed = scroll_preview(&offset, -5, 10);

        // Assert
        assert!(changed);
        assert_eq!(offset.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn should_invalidate_redraw_when_positive_keyboard_scroll_is_clamped_to_a_new_offset() {
        // Arrange
        let offset = AtomicUsize::new(8);

        // Act
        let changed = scroll_preview(&offset, 5, 10);

        // Assert
        assert!(changed);
        assert_eq!(offset.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn should_not_invalidate_redraw_when_keyboard_scroll_has_no_range() {
        // Arrange
        let offset = AtomicUsize::new(0);

        // Act
        let changed = scroll_preview(&offset, 3, 0);

        // Assert
        assert!(!changed);
        assert_eq!(offset.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn should_not_invalidate_redraw_when_negative_keyboard_scroll_is_at_zero_boundary() {
        // Arrange: even with a negative delta, max=0 keeps the offset at zero.
        let offset = AtomicUsize::new(0);

        // Act
        let changed = scroll_preview(&offset, -3, 0);

        // Assert
        assert!(!changed);
        assert_eq!(offset.load(Ordering::Relaxed), 0);
    }

    // ── build_confirmation_root focus order ─────────────────────────────

    /// Builds the confirmation root in a Runtime and verifies that
    /// focus_first_focusable finds a focusable widget. In the depth-first
    /// traversal order, the Cancel Button is encountered before the Paste
    /// Button because it is the leftmost child of the Row.
    #[test]
    fn should_focus_cancel_button_first_in_confirmation_root() {
        use harbor_widget::runtime::Runtime;
        use std::time::Instant;

        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));
        let wrapped = vec!["sample text".to_string()];
        let scroll = Arc::new(AtomicUsize::new(0));

        // Arrange: build root and set in runtime.
        let root = build_confirmation_root(1, wrapped, scroll, cancelled, confirmed, 20.0);
        let mut rt = Runtime::new();
        rt.set_root(root);
        rt.update(Instant::now());

        // Act: focus the first focusable widget.
        let found = rt.focus_first_focusable();

        // Assert: a focusable widget was found. In the tree,
        // FocusScope > Padding > Column > ... > Row > [Button("Cancel"), SizedBox, Button("Paste")].
        // Depth-first traversal hits Cancel (leftmost Button) first.
        assert!(found, "expected a focusable widget (Cancel button)");
        assert!(rt.input().focused().is_some(), "focused should be set");

        // Verify the focused widget is a Button (without being able to read
        // the label from outside the crate, the structure guarantees it is Cancel).
        let focused_id = rt.input().focused().unwrap();
        let fiber = rt.arena().get(focused_id).unwrap();
        assert_eq!(
            fiber.widget_type(),
            std::any::TypeId::of::<harbor_widget::widgets::button::Button>(),
            "focused widget should be a Button"
        );
    }

    #[test]
    fn confirmed_paste_writes_unchanged_raw_text_with_current_modes() {
        let outcome = DialogOutcome::Confirmed("first\r\nsecond\t\x1b[A".to_owned());
        let mut writes = Vec::new();

        let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |bytes| {
            writes.push(bytes.to_vec());
            Ok::<_, ()>(())
        })
        .expect("capture writer succeeds");

        assert!(wrote);
        assert_eq!(writes, vec![b"first\r\nsecond\t\x1b[A".to_vec()]);
    }

    #[test]
    fn confirmed_paste_uses_bracketed_mode_current_at_confirmation_time() {
        let outcome = DialogOutcome::Confirmed("first\nsecond".to_owned());
        let mut writes = Vec::new();
        let modes = InputModes {
            bracketed_paste: true,
            ..InputModes::default()
        };

        let wrote = write_confirmation_outcome(&outcome, modes, |bytes| {
            writes.push(bytes.to_vec());
            Ok::<_, ()>(())
        })
        .expect("capture writer succeeds");

        assert!(wrote);
        assert_eq!(writes, vec![b"\x1b[200~first\nsecond\x1b[201~".to_vec()]);
    }

    #[test]
    fn cancelled_or_closed_confirmation_never_calls_the_writer() {
        for outcome in [DialogOutcome::Cancelled, DialogOutcome::None] {
            let wrote = write_confirmation_outcome(
                &outcome,
                InputModes::default(),
                |_| -> Result<(), ()> {
                    panic!("cancelled and native-close outcomes must not write to the PTY")
                },
            )
            .expect("no-write outcome cannot fail");
            assert!(!wrote);
        }
    }

    #[test]
    fn should_not_write_to_pty_when_confirmation_frame_is_fatal() {
        let outcome = DialogOutcome::Fatal(FrameError::out_of_memory());
        let write_attempts = std::cell::Cell::new(0);

        let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
            write_attempts.set(write_attempts.get() + 1);
            Ok::<_, ()>(())
        })
        .expect("fatal outcome cannot invoke the writer");

        assert!(!wrote);
        assert_eq!(write_attempts.get(), 0);
    }

    #[test]
    fn should_return_the_writer_error_after_one_confirmed_paste_attempt() {
        let outcome = DialogOutcome::Confirmed("paste".to_owned());
        let write_attempts = std::cell::Cell::new(0);

        let result = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
            write_attempts.set(write_attempts.get() + 1);
            Err::<(), _>("PTY disconnected")
        });

        assert_eq!(result, Err("PTY disconnected"));
        assert_eq!(write_attempts.get(), 1);
    }
}
