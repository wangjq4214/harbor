//! Application shell: winit lifecycle, window bootstrap, frame render.

mod caps;
mod cursor;
mod input;
mod interaction;
mod keyboard;
mod paste_dialog;
mod scrollbar;
mod selection;

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

use crate::{
    event::AppEvent,
    pty::{Pty, PtyWakeHandler},
    terminal::Terminal,
};
use harbor_render::{FrameOutcome, RenderTarget, UiRenderer};
use harbor_ui::{
    Key as UiKey, Terminal as UiTerminal, TerminalIntent, TerminalScroll, WidgetRuntime,
};
use interaction::TerminalInteraction;
use keyboard::KeyboardDispatch;
use paste_dialog::{PasteDialog, PasteDialogResult};
/// Result of a shell-owned terminal interaction event.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum EventResult {
    Handled,
    Continue,
    ConfirmPaste(String),
}


/// Application state holding the window and its renderer.
pub(crate) struct App {
    /// The primary window, wrapped in `Arc` so the renderer can share ownership.
    window: Option<Arc<Window>>,
    /// Renderer-owned resources for the main terminal window.
    renderer: Option<UiRenderer>,
    /// Render target that records and presents the main widget tree.
    render_target: Option<RenderTarget>,
    /// Immutable terminal widget configuration rebuilt from the latest screen snapshot.
    ui: Option<UiTerminal>,
    /// Retained state for the terminal's static widget tree.
    ui_runtime: Option<WidgetRuntime<UiTerminal, harbor_ui::TerminalIntent>>,
    /// Shell-owned terminal interaction state.
    interaction: Option<TerminalInteraction>,
    /// Terminal model: byte-stream parser plus visible screen.
    terminal: Option<Terminal>,
    /// Shell process with background output reader.
    pty: Pty,
    /// Coalesced pending resize (raw pixel dims); applied in `about_to_wait`.
    pending_resize: Option<(u32, u32, f64)>,
    /// Currently active keyboard modifiers (tracked via `ModifiersChanged`).
    modifiers: ModifiersState,
    /// Active paste confirmation dialog (None when no confirmation is pending).
    paste_dialog: Option<PasteDialog>,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
    #[error("failed to start shell pty")]
    Pty(#[source] anyhow::Error),
}

// ── ApplicationHandler (winit lifecycle) ──────────────────────────────────
impl ApplicationHandler<AppEvent> for App {
    /// Called on start or wake from suspend.  Bootstraps the window, GPU,
    /// component tree, terminal, and PTY on first call.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    /// Handles PTY output events from the background reader thread.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        let AppEvent::PtyOutputReady = event;
        let (Some(terminal), Some(window)) = (self.terminal.as_mut(), self.window.as_ref()) else {
            return;
        };
        let output = self.pty.drain_output();
        // Spurious wake (reader sent event but main already drained) — skip.
        if output.is_empty() {
            return;
        }
        terminal.process_output(&output);
        window.request_redraw();
    }

    /// Called when the event loop is about to block. Applies pending resize,
    /// then drives component deadlines (cursor blink, scrollbar auto-hide).
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(interaction), Some(terminal), Some(window)) = (
            self.interaction.as_mut(),
            self.terminal.as_mut(),
            self.window.as_ref(),
        ) else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };

        // Apply coalesced resize before blocking.
        if let Some((width, height, scale)) = self.pending_resize.take() {
            if let Some(render_target) = self.render_target.as_mut() {
                render_target.resize(width, height, scale);
                let environment = render_target.environment();
                let (logical_w, logical_h) = environment.logical_size();
                let metrics = environment.text_metrics();
                if let Some(ui) = self.ui.as_ref()
                    && let TerminalIntent::Resize(new_size) = ui.resize_intent(
                        harbor_ui::Rect {
                            x: 0.0,
                            y: 0.0,
                            width: logical_w,
                            height: logical_h,
                        },
                        metrics.cell_width,
                        metrics.line_height,
                    )
                    && terminal.resize_terminal_if_changed(new_size)
                {
                    self.pty.resize(new_size);
                }
            }
            window.request_redraw();
        }

        let deadline = interaction.deadline(terminal, window);

        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    /// Dispatches window-level events: resize, redraw, close, keyboard input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // ── Dialog window handling ──────────────────────────────────────
        // Use take/replace to avoid simultaneous mutable borrows.
        let dialog_opt = self.paste_dialog.take();
        if let Some(mut dialog) = dialog_opt {
            if dialog.window_id() == window_id {
                let result = dialog.handle_event(&event);
                match result {
                    PasteDialogResult::Confirmed => {
                        let text = dialog.raw_text.clone();
                        let modes = self
                            .terminal
                            .as_ref()
                            .map(|t| t.screen().input_modes())
                            .unwrap_or_default();
                        crate::terminal::send_paste(modes, &text, &mut self.pty);
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                        // dialog dropped; don't put back
                        return;
                    }
                    PasteDialogResult::Cancelled => {
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                        // dialog dropped; don't put back
                        return;
                    }
                    PasteDialogResult::None => {
                        // Put dialog back; render below
                        if matches!(&event, WindowEvent::RedrawRequested) {
                            dialog.render();
                        }
                        self.paste_dialog = Some(dialog);
                    }
                }
            } else {
                // Event not for dialog window; put dialog back
                self.paste_dialog = Some(dialog);
            }
        }
        let dialog_active = self.paste_dialog.is_some();

        let (
            Some(ui),
            Some(ui_runtime),
            Some(renderer),
            Some(interaction),
            Some(terminal),
            Some(pty),
            Some(window),
        ) = (
            self.ui.as_ref(),
            self.ui_runtime.as_mut(),
            self.renderer.as_ref(),
            self.interaction.as_mut(),
            self.terminal.as_mut(),
            Some(&mut self.pty),
            self.window.as_ref(),
        )
        else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        // A paste confirmation is application-modal: only redraw and process
        // shutdown for the owner window while its native dialog is open.
        if dialog_active
            && !matches!(
                &event,
                WindowEvent::RedrawRequested | WindowEvent::CloseRequested
            )
        {
            return;
        }

        // Interactive layers first — each gets only the rights it needs.
        let handled = interaction.handle_event(&event, terminal, window, pty, self.modifiers);

        // Multi-line paste confirmation: create dialog instead of sending to PTY.
        // Duplicate paste while dialog is open is a no-op.
        if let EventResult::ConfirmPaste(raw_text) = &handled {
            if self.paste_dialog.is_none() {
                self.paste_dialog = Some(PasteDialog::new(
                    raw_text.clone(),
                    event_loop,
                    renderer,
                    Some(window),
                ));
            }
            window.request_redraw();
            return;
        }

        // Keyboard dispatch: pure decision + ordered side effects
        // (scroll-to-bottom, scrollback nav, redraw, PTY forward).
        // Keyboard events go through dispatch even when Handled
        // (e.g. paste scrolls to bottom, copy needs redraw).
        let dispatch = match &event {
            WindowEvent::KeyboardInput { event: kbd, .. } if kbd.state == ElementState::Pressed => {
                KeyboardDispatch::decide(
                    &kbd.logical_key,
                    kbd.text.as_deref(),
                    kbd.location,
                    self.modifiers,
                    terminal.is_alt_screen(),
                    terminal.screen().input_modes(),
                    &handled,
                )
            }
            _ => KeyboardDispatch::none(),
        };
        if dispatch.needs_redraw {
            dispatch.apply(terminal, pty, window);
        } else if handled == EventResult::Handled {
            // Non-keyboard Handled event (e.g. scrollbar consumed mouse)
            window.request_redraw();
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
                event_loop.exit();
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }

            // Resize the renderer target and terminal grid in `about_to_wait`
            // to coalesce rapid resize events.  Calling surface.configure and
            // render_frame on every Resized during a drag causes visible stutter.
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                if size.width == 0 || size.height == 0 {
                    return;
                }
                self.pending_resize = Some((size.width, size.height, window.scale_factor()));
            }

            // Redraw: draw a frame and present it.
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                self.render_frame();
            }

            // Terminal owns wheel interpretation and emits a semantic intent;
            // the shell applies the resulting viewport transition.
            WindowEvent::MouseWheel { .. } => {
                let intent = self.render_target.as_ref().and_then(|target| {
                    let environment = target.environment();
                    let (width, height) = environment.logical_size();
                    match ui_runtime.event(
                        ui,
                        &event,
                        harbor_ui::Rect {
                            x: 0.0,
                            y: 0.0,
                            width,
                            height,
                        },
                    ) {
                        harbor_ui::WidgetEventResult::Intent(intent) => Some(intent),
                        harbor_ui::WidgetEventResult::Ignored
                        | harbor_ui::WidgetEventResult::Handled => None,
                    }
                });
                if let Some(TerminalIntent::Scroll(TerminalScroll::Lines(lines))) = intent {
                    if lines > 0 {
                        terminal.scroll_viewport_up(lines as usize);
                    } else {
                        terminal.scroll_viewport_down((-lines) as usize);
                    }
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

// ── App (own methods) ─────────────────────────────────────────────────────

impl App {
    /// Creates the application shell with no initial window, GPU, or terminal.
    /// These are lazily initialised on the first `resumed` call.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            window: None,
            renderer: None,
            render_target: None,
            ui: None,
            ui_runtime: None,
            interaction: None,
            terminal: None,
            pty: Pty::new(PtyWakeHandler::new(event_proxy)),
            pending_resize: None,
            modifiers: ModifiersState::default(),
            paste_dialog: None,
        }
    }

    /// Creates the main window, renderer target, and terminal widget tree.
    /// Keeps existing state on repeated resumes (e.g. after suspend/resume).
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);

        // Paint the terminal background immediately so the OS does not show a white window
        // while the renderer initializes.
        #[cfg(target_os = "windows")]
        paint_gdi_background(&window);

        let (renderer, render_target) =
            pollster::block_on(UiRenderer::new(window.clone())).map_err(AppError::Renderer)?;
        let environment = render_target.environment();
        let (width, height) = environment.logical_size();
        let metrics = environment.text_metrics();
        let mut terminal = Terminal::new(1, 1);
        let size = match UiTerminal::new(UiKey(0)).resize_intent(
            harbor_ui::Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            metrics.cell_width,
            metrics.line_height,
        ) {
            TerminalIntent::Resize(size) => size,
            TerminalIntent::Scroll(_) => unreachable!("terminal size intent must resize"),
        };
        terminal.resize(size.rows, size.cols);
        let ui = UiTerminal::with_snapshot(UiKey(0), Arc::new(terminal.screen().snapshot()));
        let ui_runtime = WidgetRuntime::new(&ui);
        let interaction = TerminalInteraction::new(metrics.cell_width, metrics.line_height);
        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");

        self.renderer = Some(renderer);
        self.render_target = Some(render_target);
        self.ui = Some(ui);
        self.ui_runtime = Some(ui_runtime);
        self.interaction = Some(interaction);
        self.terminal = Some(terminal);
        self.window = Some(window.clone());
        self.pty.start(size).map_err(AppError::Pty)?;
        window.request_redraw();
        Ok(())
    }

    /// Records the main terminal widget tree and clears dirt only after presentation.
    fn render_frame(&mut self) {
        if let (Some(ui), Some(ui_runtime), Some(terminal), Some(interaction)) = (
            self.ui.as_mut(),
            self.ui_runtime.as_mut(),
            self.terminal.as_ref(),
            self.interaction.as_ref(),
        ) {
            let next = ui
                .with_render_snapshot(Arc::new(terminal.screen().snapshot()))
                .with_visual_state(interaction.visual_state());
            ui_runtime.reconcile(ui, &next);
            *ui = next;
        }
        let (Some(render_target), Some(ui), Some(ui_runtime)) = (
            self.render_target.as_mut(),
            self.ui.as_ref(),
            self.ui_runtime.as_mut(),
        ) else {
            return;
        };
        let environment = render_target.environment();
        let (width, height) = environment.logical_size();
        ui_runtime.layout(
            ui,
            environment,
            harbor_ui::BoxConstraints::tight(width, height),
        );
        let outcome = render_target.render(|context| ui_runtime.paint(ui, context));
        if outcome == FrameOutcome::Presented
            && let Some(terminal) = self.terminal.as_mut()
        {
            terminal.clear_screen_dirty();
        }
    }
}

/// Paints the terminal background color into the window before the renderer
/// is ready, preventing the OS from showing a white window during startup.
///
/// The linear-light BACKGROUND values (0.36, 0.20, 0.08) are converted to
/// sRGB bytes (162, 124, 80) for GDI. COLORREF format is 0x00BBGGRR.
#[cfg(target_os = "windows")]
fn paint_gdi_background(window: &Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    unsafe extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
        fn CreateSolidBrush(color: u32) -> isize;
        fn FillRect(hdc: isize, rect: *const Rect, brush: isize) -> i32;
        fn DeleteObject(obj: isize) -> i32;
    }

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };

    let hwnd = h.hwnd.get();
    let size = window.inner_size();
    // BACKGROUND linear (0.36, 0.20, 0.08) → sRGB (162, 124, 80).
    // COLORREF byte order is 0x00BBGGRR.
    let color: u32 = 162 | (124 << 8) | (80 << 16);
    let rect = Rect {
        left: 0,
        top: 0,
        right: size.width as i32,
        bottom: size.height as i32,
    };

    unsafe {
        let hdc = GetDC(hwnd);
        if hdc != 0 {
            let brush = CreateSolidBrush(color);
            FillRect(hdc, &rect, brush);
            ReleaseDC(hwnd, hdc);
            DeleteObject(brush);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::keyboard::{scrollback_navigation, ScrollbackNavigation};
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    fn key(name: NamedKey) -> Key {
        Key::Named(name)
    }

    #[test]
    fn bare_navigation_keys_are_owned_in_normal_screen() {
        assert_eq!(
            scrollback_navigation(&key(NamedKey::PageUp), ModifiersState::default(), false),
            Some(ScrollbackNavigation::PageUp)
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::PageDown), ModifiersState::default(), false),
            Some(ScrollbackNavigation::PageDown)
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::Home), ModifiersState::default(), false),
            Some(ScrollbackNavigation::Top)
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::End), ModifiersState::default(), false),
            Some(ScrollbackNavigation::Bottom)
        );
    }

    #[test]
    fn modified_or_alt_screen_navigation_is_not_owned() {
        assert_eq!(
            scrollback_navigation(&key(NamedKey::PageUp), ModifiersState::SHIFT, false),
            None
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::Home), ModifiersState::CONTROL, false),
            None
        );
        assert_eq!(
            scrollback_navigation(&key(NamedKey::End), ModifiersState::default(), true),
            None
        );
    }
}
