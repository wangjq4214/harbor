//! Application shell: winit lifecycle, window bootstrap, frame render.

mod input;
mod ui;

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

use crate::{
    event::AppEvent,
    pty::Pty,
    render::{EventResult, GpuContext, TextMetrics, load_system_fonts},
    terminal::{Terminal, TerminalSize},
};
use input::keyboard_input_bytes;
use ui::UiRoot;

/// Application state holding the window and its renderer.
pub(crate) struct App {
    /// The primary window, wrapped in `Arc` so the GPU context can share ownership.
    window: Option<Arc<Window>>,
    /// GPU context (surface / device / queue).
    gpu: Option<GpuContext>,
    /// Component tree (owns all rendering state + handles events).
    ui: Option<UiRoot>,
    /// Terminal model: byte-stream parser plus visible screen.
    terminal: Option<Terminal>,
    /// Shell process with background output reader.
    pty: Pty,
    /// Coalesced pending resize; applied in `about_to_wait` to avoid bounce.
    pending_resize: Option<TerminalSize>,
    /// Currently active keyboard modifiers (tracked via `ModifiersChanged`).
    modifiers: ModifiersState,
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
        let (Some(gpu), Some(ui), Some(terminal), Some(window)) = (
            self.gpu.as_mut(),
            self.ui.as_mut(),
            self.terminal.as_mut(),
            self.window.as_ref(),
        ) else {
            return;
        };
        let output = self.pty.drain_output();
        // Spurious wake (reader sent event but main already drained) — skip.
        if output.is_empty() {
            return;
        }
        terminal.process_output(&output);
        ui.prepare(gpu, terminal.screen());
        terminal.clear_screen_dirty();
        window.request_redraw();
    }

    /// Called when the event loop is about to block. Applies pending resize,
    /// then drives component deadlines (cursor blink, scrollbar auto-hide).
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(ui), Some(terminal), Some(pty), Some(window)) = (
            self.gpu.as_mut(),
            self.ui.as_mut(),
            self.terminal.as_mut(),
            Some(&mut self.pty),
            self.window.as_ref(),
        ) else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };

        // Apply coalesced resize before blocking.
        if let Some(new_size) = self.pending_resize.take()
            && terminal.resize_terminal_if_changed(new_size)
        {
            ui.prepare(gpu, terminal.screen());
            terminal.clear_screen_dirty();
            pty.resize(new_size);
        }

        let deadline = ui.compact_deadline(terminal, window);

        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    /// Dispatches window-level events: resize, redraw, close, keyboard input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let (Some(gpu), Some(ui), Some(terminal), Some(pty), Some(window)) = (
            self.gpu.as_mut(),
            self.ui.as_mut(),
            self.terminal.as_mut(),
            Some(&mut self.pty),
            self.window.as_ref(),
        ) else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        // Interactive layers first — each gets only the rights it needs.
        // Scope so gpu/terminal borrows are released before prepare-on-Handled.
        let handled = ui.handle_event(&event, terminal, window, gpu, pty, self.modifiers);
        if handled == EventResult::Handled {
            ui.prepare(
                self.gpu.as_mut().unwrap(),
                self.terminal.as_ref().unwrap().screen(),
            );
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

            // Resize: update surface config immediately, defer terminal grid
            // resize to `about_to_wait` to coalesce bounce.
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                if size.width == 0 || size.height == 0 {
                    return;
                }
                gpu.resize(size.width, size.height);
                ui.resize(gpu, (size.width, size.height));
                let new_size = ui.terminal_size(gpu);
                self.pending_resize = Some(new_size);
                window.request_redraw();
            }

            // Redraw: draw a frame and present it.
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                self.render_frame();
            }

            // Mouse wheel → scroll viewport through history.
            WindowEvent::MouseWheel { delta, .. } => {
                if terminal.is_alt_screen() {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y * 3.0) as isize,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as isize,
                };
                if lines > 0 {
                    terminal.scroll_viewport_up(lines as usize);
                } else if lines < 0 {
                    terminal.scroll_viewport_down((-lines) as usize);
                }
                ui.prepare(gpu, terminal.screen());
                terminal.clear_screen_dirty();
                window.request_redraw();
            }

            // Keyboard press → forward the key event to the PTY stdin pipe.
            WindowEvent::KeyboardInput {
                device_id: _,
                event,
                is_synthetic: _,
            } if event.state == ElementState::Pressed => {
                terminal.scroll_viewport_to_bottom();

                let is_numpad = event.location == winit::keyboard::KeyLocation::Numpad;
                let Some(bytes) = keyboard_input_bytes(
                    &event.logical_key,
                    event.text.as_deref(),
                    self.modifiers,
                    terminal.screen().application_cursor(),
                    terminal.screen().application_keypad(),
                    is_numpad,
                ) else {
                    return;
                };
                pty.write(&bytes);
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
            gpu: None,
            ui: None,
            terminal: None,
            pty: Pty::new(event_proxy),
            pending_resize: None,
            modifiers: ModifiersState::default(),
        }
    }

    /// Creates the main window, GPU context, font atlas, and component tree.
    /// Keeps existing state on repeated resumes (e.g. after suspend/resume).
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);

        let gpu =
            pollster::block_on(GpuContext::new(window.clone())).map_err(AppError::Renderer)?;
        let fonts = load_system_fonts().map_err(AppError::Renderer)?;
        let metrics = TextMetrics::new(&fonts);

        // Bootstrap with a 1×1 terminal so the glyph atlas has a screen to
        // build from. UiRoot computes the real grid size from font metrics.
        let mut terminal = Terminal::new(1, 1);
        let ui =
            UiRoot::new(&gpu, terminal.screen(), fonts, metrics).map_err(AppError::Renderer)?;
        let size = ui.terminal_size(&gpu);
        terminal.resize(size.rows, size.cols);
        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");

        self.gpu = Some(gpu);
        self.ui = Some(ui);
        self.terminal = Some(terminal);
        self.window = Some(window.clone());
        self.pty.start(size).map_err(AppError::Pty)?;
        window.request_redraw();
        Ok(())
    }

    /// Acquires the surface texture, draws all components, and presents.
    /// Callers are responsible for calling `UiRoot::prepare` before this.
    fn render_frame(&mut self) {
        let Some((ui, gpu)) = self.ui.as_mut().zip(self.gpu.as_mut()) else {
            return;
        };

        // wgpu surface acquisition: handle transient failures by reconfiguring
        // or skipping the frame (the event loop will re-request a redraw).
        let output = match gpu.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
                tracing::warn!("surface texture suboptimal");
                output
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                tracing::warn!("surface lost; reconfiguring");
                let (w, h) = gpu.surface_size();
                gpu.resize(w, h);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                tracing::warn!("surface outdated; reconfiguring");
                let (w, h) = gpu.surface_size();
                gpu.resize(w, h);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                tracing::trace!("surface texture timeout");
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                tracing::trace!("surface occluded");
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                tracing::warn!("surface validation failed");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(crate::config::BACKGROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            ui.draw(&mut render_pass);
        }

        gpu.queue().submit(Some(encoder.finish()));
        gpu.present(output);
    }
}
