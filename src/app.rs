//! Application shell: winit lifecycle, window bootstrap, frame render.

mod caps;
mod cursor;
mod input;
mod interaction;
mod paste_dialog;
mod scrollbar;
mod selection;

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    app::input::InputEncoder,
    event::AppEvent,
    pty::{Pty, PtyWakeHandler},
    terminal::Terminal,
};
use harbor_gpu::GpuContext;
use harbor_types::TerminalSize;
use harbor_ui::{
    Key as UiKey, Terminal as UiTerminal, TerminalIntent, TerminalScroll, TextMetrics,
    TextResources, WidgetRuntime, load_system_fonts,
};
use interaction::TerminalInteraction;
use paste_dialog::{PasteDialog, PasteDialogResult};

/// Result of a shell-owned terminal interaction event.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum EventResult {
    Handled,
    Continue,
    ConfirmPaste(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollbackNavigation {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

fn scrollback_navigation(
    logical_key: &Key,
    modifiers: ModifiersState,
    is_alt_screen: bool,
) -> Option<ScrollbackNavigation> {
    if is_alt_screen
        || modifiers.shift_key()
        || modifiers.control_key()
        || modifiers.alt_key()
        || modifiers.super_key()
    {
        return None;
    }

    match logical_key {
        Key::Named(NamedKey::PageUp) => Some(ScrollbackNavigation::PageUp),
        Key::Named(NamedKey::PageDown) => Some(ScrollbackNavigation::PageDown),
        Key::Named(NamedKey::Home) => Some(ScrollbackNavigation::Top),
        Key::Named(NamedKey::End) => Some(ScrollbackNavigation::Bottom),
        _ => None,
    }
}

/// Application state holding the window and its renderer.
pub(crate) struct App {
    /// The primary window, wrapped in `Arc` so the GPU context can share ownership.
    window: Option<Arc<Window>>,
    /// GPU context (surface / device / queue).
    gpu: Option<GpuContext>,
    /// Immutable terminal widget configuration rebuilt from the latest screen snapshot.
    ui: Option<UiTerminal>,
    /// Retained state for the terminal's static widget tree.
    ui_runtime: Option<WidgetRuntime<UiTerminal, harbor_ui::TerminalIntent>>,
    /// Shared glyph atlas and text pipeline for every widget in this window.
    text: Option<TextResources>,
    /// Shell-owned terminal interaction state.
    interaction: Option<TerminalInteraction>,
    /// Terminal model: byte-stream parser plus visible screen.
    terminal: Option<Terminal>,
    /// Shell process with background output reader.
    pty: Pty,
    /// Coalesced pending resize; applied in `about_to_wait` to avoid bounce.
    pending_resize: Option<TerminalSize>,
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
        let (Some(gpu), Some(interaction), Some(terminal), Some(window)) = (
            self.gpu.as_mut(),
            self.interaction.as_mut(),
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
        interaction.prepare(gpu, terminal.screen());
        window.request_redraw();
    }

    /// Called when the event loop is about to block. Applies pending resize,
    /// then drives component deadlines (cursor blink, scrollbar auto-hide).
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(interaction), Some(terminal), Some(pty), Some(window)) = (
            self.gpu.as_mut(),
            self.interaction.as_mut(),
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
            interaction.prepare(gpu, terminal.screen());
            pty.resize(new_size);
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
                        if matches!(&event, WindowEvent::RedrawRequested)
                            && let (Some(gpu), Some(text)) = (self.gpu.as_ref(), self.text.as_mut())
                        {
                            dialog.render(gpu, text);
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
            Some(gpu),
            Some(text),
            Some(interaction),
            Some(terminal),
            Some(pty),
            Some(window),
        ) = (
            self.ui.as_ref(),
            self.gpu.as_mut(),
            self.text.as_mut(),
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
        // Scope so gpu/terminal borrows are released before prepare-on-Handled.
        let handled = interaction.handle_event(&event, terminal, window, gpu, pty, self.modifiers);

        // Multi-line paste confirmation: create dialog instead of sending to PTY.
        // Duplicate paste while dialog is open is a no-op.
        if let EventResult::ConfirmPaste(raw_text) = &handled {
            if self.paste_dialog.is_none() {
                self.paste_dialog = Some(PasteDialog::new(
                    raw_text.clone(),
                    event_loop,
                    gpu,
                    Some(window),
                ));
            }
            interaction.prepare(gpu, terminal.screen());
            window.request_redraw();
            return;
        }

        // Detect Ctrl+C copy — the only keyboard event that should NOT
        // scroll to bottom (user is reading scrollback while copying).
        let is_copy = self.modifiers.control_key()
            && matches!(&event, WindowEvent::KeyboardInput { event: kbd, .. }
                if kbd.state == ElementState::Pressed
                && matches!(&kbd.logical_key, Key::Character(ch) if ch == "c" || ch == "C")
            );

        // Scroll to bottom on keyboard input that produces visible text.
        // Bare modifiers, arrow keys, F-keys, and Ctrl+C copy don't scroll.
        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
            && kbd.text.is_some()
            && !(handled == EventResult::Handled && is_copy)
        {
            terminal.scroll_viewport_to_bottom();
        }

        // Handled events (copy, paste): prepare + redraw so the GPU
        // vertex buffer reflects the cleared selection, then return
        // early to avoid forwarding the key to the PTY.
        if handled == EventResult::Handled {
            interaction.prepare(gpu, terminal.screen());
            window.request_redraw();
            return;
        }

        // Bare navigation keys own the normal-screen scrollback viewport.
        // Selection has already observed the press and cancelled itself.
        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
            && let Some(navigation) =
                scrollback_navigation(&kbd.logical_key, self.modifiers, terminal.is_alt_screen())
        {
            let page_rows = terminal.screen().rows();
            match navigation {
                ScrollbackNavigation::PageUp => terminal.scroll_viewport_up(page_rows),
                ScrollbackNavigation::PageDown => terminal.scroll_viewport_down(page_rows),
                ScrollbackNavigation::Top => terminal.scroll_viewport_to_top(),
                ScrollbackNavigation::Bottom => terminal.scroll_viewport_to_bottom(),
            }
            interaction.prepare(gpu, terminal.screen());
            window.request_redraw();
            return;
        }

        // Unhandled keyboard: prepare + redraw so the cleared
        // selection is rendered before the PTY output arrives.
        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
        {
            interaction.prepare(gpu, terminal.screen());
            window.request_redraw();
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
                interaction.resize(gpu, (size.width, size.height));
                let new_size = text.terminal_size(gpu);
                self.pending_resize = Some(new_size);
                window.request_redraw();
            }

            // Redraw: draw a frame and present it.
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                self.render_frame();
            }

            // Terminal owns wheel interpretation and emits a semantic intent;
            // the shell applies the resulting viewport transition.
            WindowEvent::MouseWheel { .. } => {
                if let Some(TerminalIntent::Scroll(TerminalScroll::Lines(lines))) =
                    ui.event_intent(&event)
                {
                    if lines > 0 {
                        terminal.scroll_viewport_up(lines as usize);
                    } else {
                        terminal.scroll_viewport_down((-lines) as usize);
                    }
                    interaction.prepare(gpu, terminal.screen());
                    window.request_redraw();
                }
            }

            // Keyboard press → forward the key event to the PTY stdin pipe.
            WindowEvent::KeyboardInput {
                device_id: _,
                event,
                is_synthetic: _,
            } if event.state == ElementState::Pressed && !dialog_active => {
                let is_numpad = event.location == winit::keyboard::KeyLocation::Numpad;
                let Some(bytes) = InputEncoder::key(
                    &event.logical_key,
                    event.text.as_deref(),
                    self.modifiers,
                    terminal.screen().input_modes(),
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
            ui_runtime: None,
            text: None,
            interaction: None,
            terminal: None,
            pty: Pty::new(PtyWakeHandler::new(event_proxy)),
            pending_resize: None,
            modifiers: ModifiersState::default(),
            paste_dialog: None,
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

        // Phase 1: paint the terminal background color via GDI immediately after
        // window creation. The GPU surface isn't ready yet, so this prevents
        // the OS from showing a white window during the ~1.5s GPU init period.
        #[cfg(target_os = "windows")]
        paint_gdi_background(&window);

        // Start font loading on a background thread so it overlaps with the
        // GPU context initialisation (both are IO/compute heavy).
        // On Windows, lower the thread priority so font IO+parse yields CPU
        // to the DX12 driver during request_adapter/request_device.
        let font_handle = std::thread::Builder::new()
            .name("font-loader".into())
            .spawn(|| {
                #[cfg(target_os = "windows")]
                {
                    use windows::Win32::System::Threading::{
                        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
                    };
                    // SAFETY: GetCurrentThread returns a pseudo-handle that is always valid.
                    unsafe {
                        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                    }
                }
                load_system_fonts()
            })
            .expect("failed to spawn font-loader thread");

        let gpu =
            pollster::block_on(GpuContext::new(window.clone())).map_err(AppError::Renderer)?;
        // Phase 2: submit one clear frame immediately after GPU init, before
        // waiting for fonts. Replaces the white window with the terminal
        // background color during the ~400ms font join wait.
        gpu.clear_surface(bg_wgpu(harbor_config::BACKGROUND));

        let fonts = font_handle
            .join()
            .map_err(|_| AppError::Renderer(anyhow::anyhow!("font loader thread panicked")))?
            .map_err(AppError::Renderer)?;
        let metrics = TextMetrics::new(&fonts);

        // Bootstrap with a 1×1 terminal so shared text resources can compute
        // the surface grid before the PTY starts.
        let mut terminal = Terminal::new(1, 1);
        let initial_snapshot = terminal.screen().snapshot();
        let text = TextResources::new(&gpu, fonts, metrics, &initial_snapshot)
            .map_err(AppError::Renderer)?;
        let size = text.terminal_size(&gpu);
        terminal.resize(size.rows, size.cols);
        let ui = UiTerminal::with_snapshot(UiKey(0), Arc::new(terminal.screen().snapshot()));
        let ui_runtime = WidgetRuntime::new(&ui);
        let interaction = TerminalInteraction::new(&gpu, terminal.screen(), metrics);
        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");

        self.gpu = Some(gpu);
        self.ui = Some(ui);
        self.ui_runtime = Some(ui_runtime);
        self.text = Some(text);
        self.interaction = Some(interaction);
        self.terminal = Some(terminal);
        self.window = Some(window.clone());
        self.pty.start(size).map_err(AppError::Pty)?;
        window.request_redraw();
        Ok(())
    }

    /// Acquires the surface texture, paints the terminal widget tree, and presents.
    fn render_frame(&mut self) {
        if let (Some(ui), Some(ui_runtime), Some(terminal)) = (
            self.ui.as_mut(),
            self.ui_runtime.as_mut(),
            self.terminal.as_ref(),
        ) {
            let next = ui.with_render_snapshot(Arc::new(terminal.screen().snapshot()));
            ui_runtime.reconcile(ui, &next);
            *ui = next;
        }
        let (Some(ui), Some(ui_runtime), Some(text), Some(gpu), Some(interaction)) = (
            self.ui.as_ref(),
            self.ui_runtime.as_mut(),
            self.text.as_mut(),
            self.gpu.as_mut(),
            self.interaction.as_mut(),
        ) else {
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
                        load: wgpu::LoadOp::Clear(bg_wgpu(harbor_config::BACKGROUND)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let bounds = ui_runtime.layout(
                ui,
                harbor_ui::BoxConstraints::tight(
                    gpu.surface_size().0 as f32,
                    gpu.surface_size().1 as f32,
                ),
            );
            ui_runtime.paint(
                ui,
                harbor_ui::PaintContext { gpu, text, bounds },
                &mut render_pass,
            );
            interaction.draw(&mut render_pass);
        }

        gpu.queue().submit(Some(encoder.finish()));
        gpu.present(output);
        if let Some(terminal) = self.terminal.as_mut() {
            terminal.clear_screen_dirty();
        }
    }
}

/// Converts `[f32;4]` from `harbor_config` to `wgpu::Color`.
fn bg_wgpu(c: [f32; 4]) -> wgpu::Color {
    wgpu::Color {
        r: c[0] as f64,
        g: c[1] as f64,
        b: c[2] as f64,
        a: c[3] as f64,
    }
}

/// Paints the terminal background color into the window using GDI, before the
/// wgpu surface is ready. Prevents the OS from showing a white window during
/// the GPU initialisation period.
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
    use super::*;
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
