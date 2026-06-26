use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::{pty::PtySession, render::Render, renderer::Renderer};

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// Raw bytes read from the shell process and decoded by the renderer.
    PtyOutput(Vec<u8>),
}

/// Application state holding the window and its renderer.
pub(crate) struct App {
    /// Proxy cloned into worker threads so they can wake the UI thread safely.
    event_proxy: EventLoopProxy<AppEvent>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
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
                let terminal_size = self
                    .renderer
                    .as_mut()
                    .and_then(|renderer| renderer.resize(size.width, size.height));
                if let (Some(size), Some(pty)) = (terminal_size, self.pty.as_mut())
                    && let Err(error) = pty.resize(size)
                {
                    tracing::error!(error = %format_args!("{error:#}"), "failed to resize pty");
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    tracing::trace!("redraw requested");
                    renderer.render(());
                }
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
        let renderer =
            pollster::block_on(Renderer::new(window.clone())).map_err(AppError::Renderer)?;
        let terminal_size = renderer.terminal_size();
        tracing::info!("renderer ready");

        self.renderer = Some(renderer);
        self.window = Some(window);

        if self.pty.is_none() {
            self.start_pty(terminal_size)?;
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        Ok(())
    }

    fn start_pty(
        &mut self,
        size: crate::terminal::TerminalSize,
    ) -> std::result::Result<(), AppError> {
        // The reader thread returns false when the event loop has gone away, which lets
        // the pump stop instead of keeping a detached background loop alive.
        let event_proxy = self.event_proxy.clone();
        let pty = PtySession::start_shell_reader(size, move |output| {
            event_proxy.send_event(AppEvent::PtyOutput(output)).is_ok()
        })
        .map_err(AppError::Pty)?;
        self.pty = Some(pty);
        Ok(())
    }

    fn write_pty_output(&mut self, output: &[u8]) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        renderer.write_terminal_output(output);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
