use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use crate::{render::Render, renderer::Renderer};

#[derive(Debug, Clone)]
pub(crate) enum TerminalEvent {
    Redraw,
}

/// Application state holding the window and its renderer.
pub(crate) struct App {
    proxy: winit::event_loop::EventLoopProxy<TerminalEvent>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    modifiers: winit::keyboard::ModifiersState,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
}

/// Handles the winit lifecycle and window events.
impl ApplicationHandler<TerminalEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: TerminalEvent) {
        match event {
            TerminalEvent::Redraw => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
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
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if key_event.state == winit::event::ElementState::Pressed {
                    if let Some(renderer) = self.renderer.as_mut() {
                        let is_ctrl = self.modifiers.control_key();
                        let mut handled = false;

                        if is_ctrl {
                            if let winit::keyboard::Key::Character(ref ch) = key_event.logical_key {
                                if ch.len() == 1 {
                                    let c = ch.chars().next().unwrap();
                                    if c.is_ascii_alphabetic() {
                                        let control_byte = c.to_ascii_lowercase() as u8 - b'a' + 1;
                                        let _ = renderer.write_to_pty(&[control_byte]);
                                        handled = true;
                                    }
                                }
                            }
                        }

                        if !handled {
                            match &key_event.logical_key {
                                winit::keyboard::Key::Named(named_key) => {
                                    let seq: Option<&[u8]> = match named_key {
                                        winit::keyboard::NamedKey::Space => Some(b" "),
                                        winit::keyboard::NamedKey::Enter => Some(b"\r"),
                                        winit::keyboard::NamedKey::Backspace => Some(b"\x7f"),
                                        winit::keyboard::NamedKey::Tab => Some(b"\t"),
                                        winit::keyboard::NamedKey::Escape => Some(b"\x1b"),
                                        winit::keyboard::NamedKey::ArrowUp => Some(b"\x1b[A"),
                                        winit::keyboard::NamedKey::ArrowDown => Some(b"\x1b[B"),
                                        winit::keyboard::NamedKey::ArrowRight => Some(b"\x1b[C"),
                                        winit::keyboard::NamedKey::ArrowLeft => Some(b"\x1b[D"),
                                        _ => None,
                                    };
                                    if let Some(bytes) = seq {
                                        let _ = renderer.write_to_pty(bytes);
                                    }
                                }
                                winit::keyboard::Key::Character(ch) => {
                                    let _ = renderer.write_to_pty(ch.as_bytes());
                                }
                                _ => {}
                            }
                        }
                    }
                }
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
                    renderer.render(());
                }
            }
            _ => {}
        }
    }
}

impl App {
    pub(crate) fn new(proxy: winit::event_loop::EventLoopProxy<TerminalEvent>) -> Self {
        Self {
            proxy,
            window: None,
            renderer: None,
            modifiers: winit::keyboard::ModifiersState::default(),
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
            pollster::block_on(Renderer::new(window.clone(), self.proxy.clone())).map_err(AppError::Renderer)?;
        tracing::info!("renderer ready");
        window.request_redraw();

        self.renderer = Some(renderer);
        self.window = Some(window);
        Ok(())
    }
}
