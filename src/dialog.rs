//! Secondary winit window for paste confirmation rendered by Widget Runtime.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::effects::merge_wait;
use harbor_app::command::CommandCompletion;
use harbor_app::confirmation::CONFIRMATION_PREVIEW_VISIBLE_LINES as PREVIEW_VISIBLE_LINES;
use harbor_terminal::safe_preview_line;
use harbor_terminal::{InputModes, PasteDisposition, Terminal};
use harbor_widget::effects::ControlFlowEffect;
use harbor_widget::winit::{
    FrameError, HostFrameOutcome, HostIdleOutcome, HostInitContext, WinitWindowHost,
    WinitWindowHostBuilder,
};
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
        wrapped.push(current_line);
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

/// A fault observed at the approval boundary always wins over a queued button/shortcut.
fn guard_confirmation(
    result: ConfirmationResult,
    fault: Option<harbor_widget::winit::GpuFault>,
) -> ConfirmationResult {
    match fault {
        Some(fault) => ConfirmationResult::Fatal(FrameError::SharedGpu(fault)),
        None => result,
    }
}

// ── ConfirmationResult ──────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) enum ConfirmationResult {
    None,
    Cancelled,
    Confirmed,
    Fatal(FrameError),
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

/// Outcome of a paste confirmation event dispatch.
#[derive(Debug)]
pub(crate) enum PasteEventOutcome {
    Handled {
        request_redraw: bool,
        wait: Option<ControlFlowEffect>,
        palette_completion: Option<Result<CommandCompletion, &'static str>>,
    },
    Fatal {
        error: FrameError,
        wait: Option<ControlFlowEffect>,
    },
}

/// Controller encapsulating modal paste confirmation and clipboard interaction.
pub(crate) struct PasteController {
    window: Option<ConfirmationWindow>,
    input_gate: Arc<AtomicBool>,
    // Completion origin only; confirmation retains the existing current-target policy.
    palette_invocation: bool,
}

impl PasteController {
    pub(crate) fn new(input_gate: Arc<AtomicBool>) -> Self {
        Self {
            window: None,
            input_gate,
            palette_invocation: false,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.window_id())
    }

    pub(crate) fn cancel(&mut self) {
        self.window = None;
        self.input_gate.store(false, Ordering::Release);
        self.palette_invocation = false;
    }

    pub(crate) fn about_to_wait(&mut self, now: Instant) -> HostIdleOutcome {
        self.window
            .as_mut()
            .map(|window| window.about_to_wait(now))
            .unwrap_or_default()
    }

    pub(crate) fn handle_dialog_event(
        &mut self,
        event: &WindowEvent,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
    ) -> PasteEventOutcome {
        let (result, wait) = self.handle_event(event);

        match &result {
            DialogOutcome::Cancelled | DialogOutcome::Confirmed(_) => {
                let completion = complete_paste_confirmation(&result, active_terminal);
                let palette_completion =
                    std::mem::take(&mut self.palette_invocation).then_some(completion);
                self.input_gate.store(false, Ordering::Release);
                PasteEventOutcome::Handled {
                    request_redraw: true,
                    wait,
                    palette_completion,
                }
            }
            DialogOutcome::Fatal(error) => {
                self.input_gate.store(false, Ordering::Release);
                self.palette_invocation = false;
                PasteEventOutcome::Fatal {
                    error: error.clone(),
                    wait,
                }
            }
            DialogOutcome::None => PasteEventOutcome::Handled {
                request_redraw: false,
                wait,
                palette_completion: None,
            },
        }
    }

    fn handle_event(&mut self, event: &WindowEvent) -> (DialogOutcome, Option<ControlFlowEffect>) {
        let Some(mut confirmation) = self.window.take() else {
            return (DialogOutcome::None, None);
        };
        let event_outcome = confirmation.handle_event(event);
        let result = match guard_confirmation(event_outcome.result, confirmation.host.gpu_fault()) {
            ConfirmationResult::Cancelled => DialogOutcome::Cancelled,
            ConfirmationResult::Confirmed => {
                DialogOutcome::Confirmed(confirmation.raw_text().to_owned())
            }
            ConfirmationResult::Fatal(error) => DialogOutcome::Fatal(error),
            ConfirmationResult::None => {
                self.window = Some(confirmation);
                DialogOutcome::None
            }
        };
        (result, event_outcome.wait)
    }
    pub(crate) fn paste_from_clipboard(
        &mut self,
        event_loop: &ActiveEventLoop,
        main_host: &mut WinitWindowHost,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        palette_invocation: bool,
    ) -> Result<(HostIdleOutcome, CommandCompletion), &'static str> {
        if self.is_active() {
            return Err("Rejected: paste confirmation is active");
        }
        let raw_text = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .map_err(|error| {
                tracing::warn!(error = %error, "failed to read clipboard text");
                "Failed: clipboard text unavailable"
            })?;
        match prepare_paste(active_terminal, &raw_text)? {
            PasteDisposition::SendDirect => Ok((
                HostIdleOutcome::default(),
                if raw_text.is_empty() {
                    CommandCompletion::Unchanged
                } else {
                    CommandCompletion::Applied
                },
            )),
            PasteDisposition::Confirm { raw_text } => {
                let confirmation = ConfirmationWindow::new(raw_text, event_loop, main_host).map_err(|error| {
                    tracing::warn!(error = %format_args!("{error:#}"), "failed to create confirmation window");
                    "Failed: could not open paste confirmation"
                })?;
                self.window = Some(confirmation);
                self.input_gate.store(true, Ordering::Release);
                self.palette_invocation = palette_invocation;
                Ok((
                    main_host.cancel_active_input_ownership(),
                    CommandCompletion::PendingConfirmation,
                ))
            }
        }
    }
}

/// The production decision/write seam: confirmation never writes bytes before approval.
fn prepare_paste(
    active_terminal: Option<&Arc<Mutex<Terminal>>>,
    raw_text: &str,
) -> Result<PasteDisposition, &'static str> {
    let terminal = active_terminal.ok_or("Rejected: no active terminal for paste")?;
    let mut terminal = terminal
        .lock()
        .map_err(|_| "Failed: terminal lock unavailable for paste")?;
    let input_modes = terminal.drain_and_snapshot().input_modes;
    let disposition = PasteDisposition::decide(input_modes, raw_text);
    if matches!(disposition, PasteDisposition::SendDirect) {
        terminal.write_pty(input_modes.paste(raw_text.as_bytes()).as_ref()).map_err(|error| {
            tracing::warn!(error = %format_args!("{error:#}"), "failed to write clipboard paste");
            "Failed: could not write clipboard paste"
        })?;
    }
    Ok(disposition)
}

/// Finish the same Host write path at approval time; no target or input modes are captured.
fn complete_paste_confirmation(
    outcome: &DialogOutcome,
    active_terminal: Option<&Arc<Mutex<Terminal>>>,
) -> Result<CommandCompletion, &'static str> {
    let DialogOutcome::Confirmed(raw_text) = outcome else {
        return Err("Cancelled: paste was not confirmed");
    };
    let terminal = active_terminal.ok_or("Rejected: no active terminal for confirmed paste")?;
    let mut terminal = terminal
        .lock()
        .map_err(|_| "Failed: terminal lock unavailable for confirmed paste")?;
    let modes = terminal.drain_and_snapshot().input_modes;
    write_confirmation_outcome(outcome, modes, |bytes| terminal.write_pty(bytes)).map_err(
        |error| {
            tracing::warn!(error = %format_args!("{error:#}"), "failed to write confirmed paste");
            "Failed: could not write confirmed paste"
        },
    )?;
    Ok(if raw_text.is_empty() {
        CommandCompletion::Unchanged
    } else {
        CommandCompletion::Applied
    })
}

// ── ConfirmationWindow ──────────────────────────────────────────────────────

struct ConfirmationRootState {
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    max_scroll: usize,
    preview_scroll_offset: Arc<AtomicUsize>,
}

pub(crate) struct ConfirmationEventOutcome {
    pub(crate) result: ConfirmationResult,
    pub(crate) wait: Option<ControlFlowEffect>,
}

/// A thin application wrapper around a secondary Host-owned native window.
///
/// Paste result and preview policy remain here; generic native/window/widget
/// lifecycle is delegated to `WinitWindowHost`.
pub(crate) struct ConfirmationWindow {
    host: WinitWindowHost,
    raw_text: String,
    root_state: ConfirmationRootState,
}

impl ConfirmationWindow {
    pub(crate) fn new(
        raw_text: String,
        event_loop: &ActiveEventLoop,
        main_host: &WinitWindowHost,
    ) -> anyhow::Result<Self> {
        Self::with_presentation(raw_text, None, event_loop, main_host)
    }

    /// Clipboard mode receives only a bounded preview; the host retains the exact payload.
    pub(crate) fn clipboard(
        preview: String,
        header: String,
        event_loop: &ActiveEventLoop,
        main_host: &WinitWindowHost,
    ) -> anyhow::Result<Self> {
        Self::with_presentation(preview, Some(header), event_loop, main_host)
    }

    fn with_presentation(
        raw_text: String,
        clipboard_header: Option<String>,
        event_loop: &ActiveEventLoop,
        main_host: &WinitWindowHost,
    ) -> anyhow::Result<Self> {
        let main_window = main_host.window();
        let line_count = raw_text.lines().count();

        let mut window_attrs = Window::default_attributes()
            .with_title("")
            .with_decorations(false)
            .with_inner_size(winit::dpi::LogicalSize::new(DIALOG_WIDTH, DIALOG_HEIGHT))
            .with_resizable(false);

        // Owned window on Windows: keep dialog above main window in z-order.
        #[cfg(target_os = "windows")]
        if let Ok(handle) = main_window.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            window_attrs = window_attrs.with_owner_window(handle.hwnd.get());
        }

        // Center dialog over the main window using physical pixels.
        if let Ok(outer) = main_window.outer_position() {
            window_attrs = window_attrs.with_position(centered_dialog_position(
                outer,
                main_window.outer_size(),
                main_window.scale_factor(),
            ));
        }

        let root_text = raw_text.clone();
        let builder = WinitWindowHostBuilder::new(
            window_attrs,
            move |context: HostInitContext<'_>, _setup: &()| {
                let metrics = *context.text_metrics();
                let max_columns = ((DIALOG_WIDTH - DIALOG_HORIZONTAL_PADDING) as f32
                    / metrics.cell_width)
                    .floor() as usize;
                let wrapped_lines = wrap_preview_text(&root_text, max_columns.max(1));
                let max_scroll = wrapped_lines.len().saturating_sub(PREVIEW_VISIBLE_LINES);
                let cancelled = Arc::new(AtomicBool::new(false));
                let confirmed = Arc::new(AtomicBool::new(false));
                let preview_scroll_offset = Arc::new(AtomicUsize::new(0));
                let root: Box<dyn harbor_widget::view::Component> =
                    if let Some(header) = clipboard_header {
                        Box::new(harbor_app::confirmation::build_clipboard_confirmation_root(
                            header,
                            wrapped_lines,
                            Arc::clone(&preview_scroll_offset),
                            Arc::clone(&cancelled),
                            Arc::clone(&confirmed),
                            metrics.line_height,
                        ))
                    } else {
                        Box::new(harbor_app::confirmation::build_confirmation_root(
                            line_count,
                            wrapped_lines,
                            Arc::clone(&preview_scroll_offset),
                            Arc::clone(&cancelled),
                            Arc::clone(&confirmed),
                            metrics.line_height,
                        ))
                    };
                let root = move |cx: &mut harbor_widget::view::BuildCx| root.build(cx);
                let root_state = ConfirmationRootState {
                    cancelled,
                    confirmed,
                    max_scroll,
                    preview_scroll_offset,
                };
                Ok::<_, anyhow::Error>((root, root_state))
            },
        )
        .reuse_gpu(Arc::clone(main_host.shared_gpu()))
        .focus_first(true);
        let (host, root_state) = pollster::block_on(builder.build_with_output(event_loop))?;

        Ok(Self {
            host,
            raw_text,
            root_state,
        })
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.host.window_id()
    }

    pub(crate) fn window(&self) -> &Window {
        self.host.window()
    }

    /// Applies idle redraw effects and returns this window's wait request.
    pub(crate) fn about_to_wait(&mut self, now: Instant) -> HostIdleOutcome {
        self.host.about_to_wait(now, None)
    }

    /// Returns the raw paste candidate text, unchanged from when the dialog opened.
    pub(crate) fn raw_text(&self) -> &str {
        &self.raw_text
    }

    /// Applies application shortcuts/preview policy, then delegates one generic event to Host.
    pub(crate) fn handle_event(&mut self, event: &WindowEvent) -> ConfirmationEventOutcome {
        if let Some(fault) = self.host.gpu_fault() {
            return ConfirmationEventOutcome {
                result: ConfirmationResult::Fatal(FrameError::SharedGpu(fault)),
                wait: None,
            };
        }
        if matches!(event, WindowEvent::CloseRequested) {
            return ConfirmationEventOutcome {
                result: ConfirmationResult::Cancelled,
                wait: None,
            };
        }

        let mut wait = None;
        if let WindowEvent::KeyboardInput {
            event: key_event, ..
        } = event
            && key_event.state == ElementState::Pressed
        {
            if let Some(result) = shortcut_result(&key_event.logical_key) {
                return ConfirmationEventOutcome {
                    result: guard_confirmation(result, self.host.gpu_fault()),
                    wait: None,
                };
            }

            let max_scroll = self.root_state.max_scroll;
            let scrolled = match &key_event.logical_key {
                Key::Named(NamedKey::ArrowUp) => {
                    scroll_preview(&self.root_state.preview_scroll_offset, -1, max_scroll)
                }
                Key::Named(NamedKey::ArrowDown) => {
                    scroll_preview(&self.root_state.preview_scroll_offset, 1, max_scroll)
                }
                Key::Named(NamedKey::PageUp) => scroll_preview(
                    &self.root_state.preview_scroll_offset,
                    -((PREVIEW_VISIBLE_LINES.saturating_sub(1)) as isize),
                    max_scroll,
                ),
                Key::Named(NamedKey::PageDown) => scroll_preview(
                    &self.root_state.preview_scroll_offset,
                    (PREVIEW_VISIBLE_LINES.saturating_sub(1)) as isize,
                    max_scroll,
                ),
                _ => false,
            };
            if scrolled {
                merge_wait(&mut wait, self.host.request_frame().wait);
            }
        }

        let host_outcome = self.host.handle_window_event(event);
        merge_wait(&mut wait, host_outcome.wait);
        let result = if let Some(HostFrameOutcome::Fatal { error, .. }) = host_outcome.frame {
            ConfirmationResult::Fatal(error)
        } else {
            confirmation_result(&self.root_state.confirmed, &self.root_state.cancelled)
        };
        ConfirmationEventOutcome {
            result: guard_confirmation(
                result,
                host_outcome.fatal.or_else(|| self.host.gpu_fault()),
            ),
            wait,
        }
    }
}

// ── Confirmation widget tree ────────────────────────────────────────────────

#[cfg(test)]
#[path = "paste_command_tests.rs"]
mod paste_command_tests;

#[cfg(test)]
mod tests;
