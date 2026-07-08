use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    background::BackgroundComponent,
    cursor::CursorComponent,
    decoration::DecorationComponent,
    font::{FontBook, load_system_fonts},
    gpu::GpuContext,
    metrics::TextMetrics,
    pty::Pty,
    render::{Component, EventContext, EventResult},
    scrollbar::ScrollbarComponent,
    terminal::TextLayer,
    terminal::{Screen, Terminal, TerminalSize},
};

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// Raw bytes read from the shell process and decoded by the renderer.
    PtyOutput(Vec<u8>),
}

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


/// Container for all UI components. Owns GPU resources and delegates
/// render / event calls to each component in z-order.
pub(crate) struct UiRoot {
    /// Solid-color background behind each non-default cell.
    background: BackgroundComponent,
    /// Text rendering: glyph atlas + vertex buffer for every grid cell.
    text: TextLayer,
    /// Underline / strikethrough decoration overlay.
    decoration: DecorationComponent,
    /// Cursor rendering + blink timer.
    cursor: CursorComponent,
    /// Scrollbar: visibility state machine + GPU thumb.
    scrollbar: ScrollbarComponent,
}

impl UiRoot {
    /// Creates all five UI components from the GPU context and font metrics.
    /// The `screen` provides the initial grid state for atlas construction.
    /// `_fonts` is consumed by `TextLayer::new`.
    pub(crate) fn new(
        gpu: &GpuContext,
        screen: &Screen,
        _fonts: FontBook,
        metrics: TextMetrics,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            background: BackgroundComponent::new(gpu, screen, metrics.cell_width, metrics.line_height),
            text: TextLayer::new(gpu, _fonts, metrics, screen)?,
            decoration: DecorationComponent::new(gpu, screen, metrics),
            cursor: CursorComponent::new(gpu, metrics),
            scrollbar: ScrollbarComponent::new(gpu, screen),
        })
    }

    /// Returns the cell dimensions (rows × cols) for the current surface size.
    pub(crate) fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        self.text.terminal_size(gpu)
    }

    /// Uploads dirty GPU resources for all five components.
    /// Called after terminal content changes or resize.
    pub(crate) fn prepare(&mut self, gpu: &GpuContext, screen: &Screen) {
        self.background.prepare(gpu, Some(screen));
        self.text.prepare(gpu, Some(screen));
        self.decoration.prepare(gpu, Some(screen));
        self.cursor.prepare(gpu, Some(screen));
        self.scrollbar.prepare(gpu, Some(screen));
    }

    /// Issues draw calls for all five components in z-order (back to front).
    /// Binds pipelines and vertex buffers; no GPU allocation.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.background.draw(pass);
        self.text.draw(pass);
        self.decoration.draw(pass);
        self.cursor.draw(pass);
        self.scrollbar.draw(pass);
    }

    /// Called when the window surface is resized. Forwards to all components
    /// so they can mark their GPU resources as needing re-upload.
    pub(crate) fn resize(&mut self, gpu: &GpuContext, size: (u32, u32)) {
        Component::resize(&mut self.background, gpu, size);
        Component::resize(&mut self.text, gpu, size);
        Component::resize(&mut self.decoration, gpu, size);
        Component::resize(&mut self.cursor, gpu, size);
        Component::resize(&mut self.scrollbar, gpu, size);
    }

    /// Bubble: last-declared component gets events first (top layer).
    /// Pure-rendering components (background/text/decoration) use default
    /// handle_event which returns Continue, so they don't block.
    pub(crate) fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
        ctx: &mut EventContext<'_>,
    ) -> EventResult {
        if self.scrollbar.handle_event(event, ctx) == EventResult::Handled {
            return EventResult::Handled;
        }
        if self.cursor.handle_event(event, ctx) == EventResult::Handled {
            return EventResult::Handled;
        }
        EventResult::Continue
    }

    /// Collects the next wake deadline from all interactive components,
    /// returning the earliest timeout (cursor blink or scrollbar auto-hide).
    pub(crate) fn compact_deadline(
        &mut self,
        ctx: &mut EventContext<'_>,
    ) -> Option<std::time::Instant> {
        let mut deadline: Option<std::time::Instant> = None;
        if let Some(d) = self.cursor.on_about_to_wait(ctx) {
            deadline = Some(deadline.map_or(d, |cur| cur.min(d)));
        }
        if let Some(d) = self.scrollbar.on_about_to_wait(ctx) {
            deadline = Some(deadline.map_or(d, |cur| cur.min(d)));
        }
        deadline
    }
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
        let AppEvent::PtyOutput(output) = event;
        let (Some(gpu), Some(ui), Some(terminal), Some(window)) = (
            self.gpu.as_mut(),
            self.ui.as_mut(),
            self.terminal.as_mut(),
            self.window.as_ref(),
        ) else {
            return;
        };
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

        let mut deadline: Option<std::time::Instant> = None;
        let mut ctx = EventContext {
            gpu,
            terminal,
            window,
            pty,
            modifiers: self.modifiers,
            deadline: &mut deadline,
        };
        deadline = ui.compact_deadline(&mut ctx);

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

        // Let interactive components handle events first (scrollbar, cursor).
        let mut deadline: Option<std::time::Instant> = None;
        let mut ctx = EventContext {
            gpu,
            terminal,
            window,
            pty,
            modifiers: self.modifiers,
            deadline: &mut deadline,
        };
        if ui.handle_event(&event, &mut ctx) == EventResult::Handled {
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

                let Some(bytes) =
                    keyboard_input_bytes(&event.logical_key, event.text.as_deref(), self.modifiers)
                else {
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
        self.pty.start(size).map_err(AppError::Pty)?;
        self.window = Some(window.clone());
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
// ── Key mapping ───────────────────────────────────────────────────────────

/// Maps a logical key + optional text + modifier state to the byte sequence
/// to write to the PTY.  Named control/navigation keys are dispatched by
/// `logical_key` first.  When Ctrl is held and the key is a single ASCII
/// letter (a–z / A–Z), the corresponding control character (0x01–0x1A) is
/// emitted regardless of what winit places in `text`.
fn keyboard_input_bytes(
    logical_key: &Key,
    text: Option<&str>,
    modifiers: ModifiersState,
) -> Option<Vec<u8>> {
    // Ctrl+letter → control character (0x01–0x1A).
    if modifiers.control_key()
        && let Key::Character(ch) = logical_key
        && let Some(ctrl_byte) = ctrl_letter_to_byte(ch)
    {
        return Some(vec![ctrl_byte]);
    }

    // If it's some other character with Ctrl held, fall through —
    // winit may have placed a control character in `text` already.
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

/// Converts a single-character `SmolStr` to its control-character byte
/// (`letter & 0x1F`).  Returns `None` for multi-codepoint strings or
/// non-ASCII letters.
fn ctrl_letter_to_byte(ch: &str) -> Option<u8> {
    let mut chars = ch.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None; // more than one codepoint — not a simple letter
    }
    match c {
        'a'..='z' => Some((c as u8) - b'a' + 1),
        'A'..='Z' => Some((c as u8) - b'A' + 1),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    fn k(name: NamedKey) -> Key {
        Key::Named(name)
    }

    fn mods() -> ModifiersState {
        ModifiersState::default()
    }

    fn ctrl() -> ModifiersState {
        ModifiersState::CONTROL
    }

    #[test]
    fn backspace_with_unexpected_text_still_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some("\x17"), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_no_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), None, mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_empty_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some(""), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn enter_sends_cr() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Enter), None, mods()),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn escape_sends_esc() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Escape), None, mods()),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn arrow_up() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::ArrowUp), None, mods()),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn printable_character() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("a".into()), Some("a"), mods()),
            Some(b"a".to_vec())
        );
    }

    #[test]
    fn unrecognized_named_key_no_text_ignored() {
        assert_eq!(keyboard_input_bytes(&k(NamedKey::F1), None, mods()), None);
    }

    #[test]
    fn empty_text_ignored() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("".into()), Some(""), mods()),
            None
        );
    }

    // ── Ctrl+letter → control character ───────────────────────────────

    #[test]
    fn ctrl_c_sends_etx_with_text() {
        // Ctrl held + 'c' → 0x03, even if winit puts plain "c" in text.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), Some("c"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_c_sends_etx_without_text() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), None, ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_d_sends_eot() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("d".into()), Some("d"), ctrl()),
            Some(b"\x04".to_vec())
        );
    }

    #[test]
    fn ctrl_shift_c_still_sends_etx() {
        // Ctrl+Shift+C = Ctrl+C → 0x03.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("C".into()), Some("C"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn c_without_ctrl_is_plain_text() {
        // 'c' without Ctrl held → normal text.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), Some("c"), mods()),
            Some(b"c".to_vec())
        );
    }

    #[test]
    fn c_without_text_or_ctrl_is_none() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), None, mods()),
            None
        );
    }

    #[test]
    fn ctrl_non_letter_falls_through_to_text() {
        // Ctrl+1 is not a letter — should use winit's text if provided.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("1".into()), Some("1"), ctrl()),
            Some(b"1".to_vec())
        );
    }
}
