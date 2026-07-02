use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoopProxy},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    pty::PtySession,
    render::Render,
    renderer::Renderer,
    terminal::{Terminal, TerminalSize},
};

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// Raw bytes read from the shell process and decoded by the renderer.
    PtyOutput(Vec<u8>),
}

/// Application state holding the window and its renderer.
pub(crate) struct App {
    /// Proxy cloned into worker threads so they can wake the UI thread safely.
    event_proxy: EventLoopProxy<AppEvent>,
    /// The primary window, wrapped in `Arc` so the renderer can share ownership.
    window: Option<Arc<Window>>,
    /// Handles all drawing to the window surface.
    renderer: Option<Renderer>,
    /// Visible terminal screen and its byte-stream parser.
    terminal: Option<Terminal>,
    /// Owns the shell process and keeps its reader thread alive while the app runs.
    pty: Option<PtySession>,
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

/// Handles the winit lifecycle and window events.
impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::PtyOutput(output) => self.write_pty_output(&output),
        }
    }

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
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                let new_size = self
                    .renderer
                    .as_mut()
                    .and_then(|renderer| renderer.resize(size.width, size.height));

                if let (Some(new_size), Some(terminal)) = (new_size, self.terminal.as_mut()) {
                    let current = TerminalSize {
                        rows: terminal.screen().rows(),
                        cols: terminal.screen().cols(),
                    };
                    if new_size != current {
                        terminal.resize(new_size.rows, new_size.cols);
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.refresh_text(terminal.screen());
                        }
                    }
                    if let Some(pty) = self.pty.as_mut()
                        && let Err(error) = pty.resize(new_size)
                    {
                        tracing::error!(error = %format_args!("{error:#}"), "failed to resize pty");
                    }
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.render(());
                }
            }
            // Keyboard press -> forward to PTY stdin.
            WindowEvent::KeyboardInput {
                device_id: _,
                event,
                is_synthetic: _,
            } if event.state == ElementState::Pressed => {
                self.handle_keyboard_input(&event);
            }
            _ => {}
        }
    }
}

impl App {
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            event_proxy,
            window: None,
            renderer: None,
            terminal: None,
            pty: None,
        }
    }

    /// Creates the main window and renderer; keeps existing state on repeated resumes.
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);

        // Bootstrap with a one-cell terminal so the glyph atlas has a screen to build from.
        // The renderer computes the real grid size from font metrics and surface dimensions.
        let mut terminal = Terminal::new(1, 1);
        let renderer = pollster::block_on(Renderer::new(window.clone(), terminal.screen()))
            .map_err(AppError::Renderer)?;
        let size = renderer.terminal_size();
        terminal.resize(size.rows, size.cols);
        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");

        self.renderer = Some(renderer);
        self.terminal = Some(terminal);
        self.start_pty(size)?;
        self.window = Some(window.clone());
        window.request_redraw();
        Ok(())
    }

    fn start_pty(
        &mut self,
        size: crate::terminal::TerminalSize,
    ) -> std::result::Result<(), AppError> {
        // The reader thread returns false when the event loop has gone away, which lets
        // the pump stop instead of keeping a detached background loop alive.
        let event_proxy = self.event_proxy.clone();

        tracing::info!(rows = size.rows, cols = size.cols, "starting pty");
        let pty = PtySession::start_shell_reader(size, move |output| {
            let bytes = output.len();
            let delivered = event_proxy.send_event(AppEvent::PtyOutput(output)).is_ok();
            if !delivered {
                tracing::warn!(bytes, "dropped pty output because event loop is closed");
            }
            delivered
        })
        .map_err(AppError::Pty)?;

        self.pty = Some(pty);
        Ok(())
    }

    fn write_pty_output(&mut self, output: &[u8]) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }

        if let Some(terminal) = self.terminal.as_mut() {
            terminal.put_bytes(output);
        }
        if let (Some(renderer), Some(terminal)) = (self.renderer.as_mut(), self.terminal.as_ref()) {
            renderer.refresh_text(terminal.screen());
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Converts a winit key-press event into bytes and writes them to the PTY.
    ///
    /// Printable characters use the UTF-8 text from `KeyEvent::text`. Named control
    /// keys are translated to the terminal escape sequences that `cmd.exe` expects.
    /// All other keys are silently ignored.
    fn handle_keyboard_input(&mut self, event: &KeyEvent) {
        let bytes: Vec<u8> = if let Some(text) = event.text.as_deref() {
            // Printable character — send its UTF-8 encoding directly.
            if !text.is_empty() {
                text.as_bytes().to_vec()
            } else {
                return;
            }
        } else {
            // Named control/navigation key — translate to terminal input sequences.
            match &event.logical_key {
                Key::Named(NamedKey::Enter) => b"\r".to_vec(),
                Key::Named(NamedKey::Backspace) => b"\x08".to_vec(),
                Key::Named(NamedKey::Tab) => b"\t".to_vec(),
                Key::Named(NamedKey::Escape) => b"\x1b".to_vec(),
                Key::Named(NamedKey::ArrowUp) => b"\x1b[A".to_vec(),
                Key::Named(NamedKey::ArrowDown) => b"\x1b[B".to_vec(),
                Key::Named(NamedKey::ArrowRight) => b"\x1b[C".to_vec(),
                Key::Named(NamedKey::ArrowLeft) => b"\x1b[D".to_vec(),
                _ => return,
            }
        };

        // Safety guard: skip empty byte sequences.
        if bytes.is_empty() {
            return;
        }

        // Forward the byte sequence to the PTY's stdin pipe.
        let Some(pty) = self.pty.as_mut() else {
            tracing::warn!("no pty session to write keyboard input to");
            return;
        };
        if let Err(error) = pty.write(&bytes) {
            tracing::warn!(
                error = %format_args!("{error:#}"),
                "failed to write keyboard input to pty"
            );
        }
    }
}
