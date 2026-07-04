use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

use crate::{cursor::CursorBlink, pty::Pty, renderer::Renderer, terminal::Terminal};

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// Raw bytes read from the shell process and decoded by the renderer.
    PtyOutput(Vec<u8>),
}

/// Application state holding the window and its renderer.
pub(crate) struct App {
    /// The primary window, wrapped in `Arc` so the renderer can share ownership.
    window: Option<Arc<Window>>,
    /// Handles all drawing to the window surface.
    renderer: Option<Renderer>,
    /// Terminal model: byte-stream parser plus visible screen.
    terminal: Option<Terminal>,
    /// Shell process with background output reader.
    pty: Pty,
    cursor_blink: CursorBlink,
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

/// Handles the winit lifecycle and window events.
impl ApplicationHandler<AppEvent> for App {
    /// Called on start or wake from suspend.  Bootstraps the window, renderer,
    /// terminal, and PTY on first call; no-ops on repeated resumes to keep
    /// existing state intact.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    /// Handles custom events posted from background workers.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            // PTY reader thread sent decoded bytes — feed them to the terminal parser.
            AppEvent::PtyOutput(output) => {
                if let (Some(renderer), Some(window), Some(terminal)) = (
                    self.renderer.as_mut(),
                    self.window.as_ref(),
                    self.terminal.as_mut(),
                ) {
                    terminal.process_output(renderer, window, &output);
                }
            }
        }
    }

    /// Called when the event loop is about to block.  Drives cursor blink:
    /// requests a redraw on visibility toggle, returns the next blink
    /// deadline for `ControlFlow::WaitUntil`.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let next = self.cursor_blink.check_blink(self.window.as_deref());
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
    }

    /// Dispatches window-level events: resize, redraw, close, keyboard input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
                event_loop.exit();
            }

            // Resize: update surface config, then resize terminal and pty if
            // the terminal grid changed, then request redraw.
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                let new_size = self
                    .renderer
                    .as_mut()
                    .and_then(|renderer| renderer.resize(size.width, size.height));

                if let (Some(new_size), Some(renderer), Some(terminal)) =
                    (new_size, self.renderer.as_mut(), self.terminal.as_mut())
                {
                    terminal.resize_with_renderer(renderer, new_size);
                    self.pty.resize(new_size);
                }
                window.request_redraw();
            }

            // Redraw: sync cursor blink visibility, upload cursor vertices,
            // commit the blink state, then draw a frame and present it.
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                if let (Some(renderer), Some(terminal)) =
                    (self.renderer.as_mut(), self.terminal.as_ref())
                {
                    let screen = terminal.screen();

                    renderer.set_cursor_visible(self.cursor_blink.visible(), screen);
                    renderer.prepare_cursor(screen);
                }
                self.cursor_blink.commit_frame();
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.render();
                }
            }

            // Keyboard press → forward the key event to the PTY stdin pipe.
            WindowEvent::KeyboardInput {
                device_id: _,
                event,
                is_synthetic: _,
            } if event.state == ElementState::Pressed => {
                let Some(bytes) = keyboard_input_bytes(&event.logical_key, event.text.as_deref())
                else {
                    return;
                };
                self.pty.write(&bytes);
            }
            _ => {}
        }
    }
}

// ── App (own methods) ─────────────────────────────────────────────────────

impl App {
    /// Creates the application shell with no initial window, renderer, or PTY.
    /// These are lazily initialised on the first `resumed` call.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            window: None,
            renderer: None,
            terminal: None,
            pty: Pty::new(event_proxy),
            cursor_blink: CursorBlink::new(),
        }
    }

    /// Creates the main window and renderer; keeps existing state on
    /// repeated resumes (e.g. after suspend/resume on mobile platforms).
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);

        // Bootstrap with a 1×1 terminal so the glyph atlas has a screen to
        // build from.  The renderer computes the real grid size from font
        // metrics and surface dimensions.
        let mut terminal = Terminal::new(1, 1);
        let renderer = pollster::block_on(Renderer::new(window.clone(), terminal.screen()))
            .map_err(AppError::Renderer)?;
        let size = renderer.terminal_size();
        terminal.resize(size.rows, size.cols);
        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");

        self.renderer = Some(renderer);
        self.terminal = Some(terminal);
        self.pty.start(size).map_err(AppError::Pty)?;
        self.window = Some(window.clone());
        window.request_redraw();
        Ok(())
    }
}
// ── Key mapping ───────────────────────────────────────────────────────────

/// Maps a logical key + optional text to the byte sequence to write to the
/// PTY.  Named control/navigation keys are dispatched by `logical_key` first
/// so they are never intercepted by whatever winit places in `text`.
fn keyboard_input_bytes(logical_key: &Key, text: Option<&str>) -> Option<Vec<u8>> {
    match logical_key {
        Key::Named(NamedKey::Enter) => Some(b"\r".to_vec()),
        Key::Named(NamedKey::Backspace) => Some(b"\x7f".to_vec()),
        Key::Named(NamedKey::Tab) => Some(b"\t".to_vec()),
        Key::Named(NamedKey::Escape) => Some(b"\x1b".to_vec()),
        // Arrow keys → standard VT100 escape sequences.
        Key::Named(NamedKey::ArrowUp) => Some(b"\x1b[A".to_vec()),
        Key::Named(NamedKey::ArrowDown) => Some(b"\x1b[B".to_vec()),
        Key::Named(NamedKey::ArrowRight) => Some(b"\x1b[C".to_vec()),
        Key::Named(NamedKey::ArrowLeft) => Some(b"\x1b[D".to_vec()),
        // For everything else, send the UTF-8 text if present.
        _ => {
            let t = text?;
            if t.is_empty() {
                None
            } else {
                Some(t.as_bytes().to_vec())
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, NamedKey};

    fn k(name: NamedKey) -> Key {
        Key::Named(name)
    }

    #[test]
    fn backspace_with_unexpected_text_still_sends_del() {
        // If winit ever puts "\x17" (Ctrl+W = kill-word) in `text` for
        // Backspace, the logical_key match must still win.
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some("\x17")),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_no_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), None),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_empty_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some("")),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn enter_sends_cr() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Enter), None),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn escape_sends_esc() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Escape), None),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn arrow_up() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::ArrowUp), None),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn printable_character() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("a".into()), Some("a")),
            Some(b"a".to_vec())
        );
    }

    #[test]
    fn unrecognized_named_key_no_text_ignored() {
        assert_eq!(keyboard_input_bytes(&k(NamedKey::F1), None), None);
    }

    #[test]
    fn empty_text_ignored() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("".into()), Some("")),
            None
        );
    }
}
