use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use crate::renderer::Renderer;

#[derive(Default)]
pub(crate) struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
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
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    tracing::trace!("redraw requested");
                    renderer.render();
                }
            }
            _ => {}
        }
    }
}

impl App {
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);
        let renderer =
            pollster::block_on(Renderer::new(window.clone())).map_err(AppError::Renderer)?;
        tracing::info!("renderer ready");
        window.request_redraw();

        self.renderer = Some(renderer);
        self.window = Some(window);
        Ok(())
    }
}
