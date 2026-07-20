//! Application shell: winit lifecycle, window bootstrap, frame render.

mod input;
mod paste_dialog;
mod ui;

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    app::input::InputEncoder,
    event::AppEvent,
    terminal_worker::{TerminalWorkerClient, WorkerUiFacade, empty_snapshot},
};
use harbor_render::{EventResult, GpuContext, TerminalFacade, TextMetrics, load_system_fonts};
use harbor_types::{RevisionedUpdateReceiver, TerminalSize, TerminalSnapshot, WorkerStatus};
use harbor_ui::DialogResult;
use paste_dialog::PasteDialog;
use ui::UiRoot;

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
    /// Component tree (owns all rendering state + handles events).
    ui: Option<UiRoot>,
    /// Last complete terminal state accepted by the renderer.
    latest_snapshot: Option<TerminalSnapshot>,
    /// Main-thread revision guard for the worker mailbox.
    updates: RevisionedUpdateReceiver,
    /// Background owner of PTY, parser, and mutable terminal model.
    worker: Option<TerminalWorkerClient>,
    /// Last worker health state observed by the UI.
    worker_status: WorkerStatus,
    /// Proxy used by the worker to wake the winit event loop.
    event_proxy: EventLoopProxy<AppEvent>,
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
    #[error("failed to start terminal worker")]
    Worker(#[source] anyhow::Error),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
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

    /// Handles terminal-worker update wakes without touching the worker model.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        let AppEvent::WorkerUpdateReady = event;
        let changed = self.consume_worker_updates();
        if changed && let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Called when the event loop is about to block. Applies pending resize,
    /// then drives component deadlines (cursor blink, scrollbar auto-hide).
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let changed = self.consume_worker_updates();
        if changed && let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        let (Some(ui), Some(snapshot), Some(worker), Some(window)) = (
            self.ui.as_mut(),
            self.latest_snapshot.as_ref(),
            self.worker.as_ref(),
            self.window.as_ref(),
        ) else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };

        if let Some(new_size) = self.pending_resize.take() {
            let _ = worker.send(harbor_types::TerminalCommand::Resize(new_size));
        }

        let mut facade = WorkerUiFacade::new(snapshot, worker);
        let deadline = ui.compact_deadline(&mut facade, window);
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    /// Dispatches window-level events: resize, redraw, close, keyboard input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let changed = self.consume_worker_updates();
        if changed && let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }

        // ── Dialog window handling ──────────────────────────────────────
        let dialog_opt = self.paste_dialog.take();
        if let Some(mut dialog) = dialog_opt {
            if dialog.window_id() == window_id {
                let result = dialog.handle_event(&event);
                match result {
                    DialogResult::Confirmed => {
                        let text = dialog.raw_text.clone();
                        let modes = self
                            .latest_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.input_modes)
                            .unwrap_or_default();
                        if let Some(worker) = self.worker.as_ref() {
                            let bytes = modes.paste(text.as_bytes());
                            let _ = worker.send(harbor_types::TerminalCommand::WritePtyInput(
                                bytes.into_owned(),
                            ));
                        }
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                        return;
                    }
                    DialogResult::Cancelled => {
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                        return;
                    }
                    DialogResult::None => {
                        if matches!(&event, WindowEvent::RedrawRequested) {
                            if let (Some(gpu), Some(ui)) = (self.gpu.as_ref(), self.ui.as_mut()) {
                                let ensure_text = format!(
                                    "[ Paste ][ Cancel ]Paste {} lines?",
                                    dialog.raw_text.lines().count()
                                );
                                ui.ensure_glyphs(&ensure_text, gpu.device(), gpu.queue());
                            }
                            if let (Some(gpu), Some(ui)) = (self.gpu.as_ref(), self.ui.as_ref()) {
                                let metrics = ui.text_metrics();
                                dialog.prepare(
                                    gpu,
                                    metrics,
                                    |ch| ui.text_glyph(ch).copied(),
                                    ui.text_pipeline(),
                                    ui.text_bind_group(),
                                );
                            }
                            if let (Some(gpu), Some(ui)) = (self.gpu.as_ref(), self.ui.as_ref()) {
                                dialog.render(gpu, ui.text_pipeline(), ui.text_bind_group());
                            }
                        }
                        self.paste_dialog = Some(dialog);
                    }
                }
            } else {
                self.paste_dialog = Some(dialog);
            }
        }
        let dialog_active = self.paste_dialog.is_some();

        let (Some(gpu), Some(ui), Some(snapshot), Some(worker), Some(window)) = (
            self.gpu.as_mut(),
            self.ui.as_mut(),
            self.latest_snapshot.as_ref(),
            self.worker.as_ref(),
            self.window.as_ref(),
        ) else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        let handled = {
            let mut facade = WorkerUiFacade::new(snapshot, worker);
            ui.handle_event(&event, &mut facade, window, gpu, self.modifiers)
        };

        if let EventResult::ConfirmPaste(raw_text) = &handled {
            if self.paste_dialog.is_none() {
                let ensure_text = format!(
                    "[ Paste ][ Cancel ]Paste {} lines?0123456789",
                    raw_text.lines().count()
                );
                ui.ensure_glyphs(&ensure_text, gpu.device(), gpu.queue());
                self.paste_dialog = Some(PasteDialog::new(
                    raw_text.clone(),
                    event_loop,
                    gpu,
                    Some(window),
                ));
            }
            ui.prepare(gpu, snapshot);
            window.request_redraw();
            return;
        }

        let is_copy = self.modifiers.control_key()
            && matches!(&event, WindowEvent::KeyboardInput { event: kbd, .. }
                if kbd.state == ElementState::Pressed
                && matches!(&kbd.logical_key, Key::Character(ch) if ch == "c" || ch == "C")
            );

        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
            && kbd.text.is_some()
            && !(handled == EventResult::Handled && is_copy)
        {
            let mut facade = WorkerUiFacade::new(snapshot, worker);
            facade.scroll_viewport_to_bottom();
        }

        if handled == EventResult::Handled {
            ui.prepare(gpu, snapshot);
            window.request_redraw();
            return;
        }

        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
            && let Some(navigation) =
                scrollback_navigation(&kbd.logical_key, self.modifiers, snapshot.is_alt)
        {
            let page_rows = snapshot.rows;
            let mut facade = WorkerUiFacade::new(snapshot, worker);
            match navigation {
                ScrollbackNavigation::PageUp => facade.scroll_viewport_up(page_rows),
                ScrollbackNavigation::PageDown => facade.scroll_viewport_down(page_rows),
                ScrollbackNavigation::Top => facade.scroll_viewport_to_top(),
                ScrollbackNavigation::Bottom => facade.scroll_viewport_to_bottom(),
            }
            ui.prepare(gpu, snapshot);
            window.request_redraw();
            return;
        }

        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
        {
            ui.prepare(gpu, snapshot);
            window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
                worker.shutdown();
                event_loop.exit();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                if size.width == 0 || size.height == 0 {
                    return;
                }
                gpu.resize(size.width, size.height);
                ui.resize(gpu, (size.width, size.height));
                self.pending_resize = Some(ui.terminal_size(gpu));
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                self.render_frame();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if snapshot.is_alt {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y * 3.0) as isize,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as isize,
                };
                let mut facade = WorkerUiFacade::new(snapshot, worker);
                if lines > 0 {
                    facade.scroll_viewport_up(lines as usize);
                } else if lines < 0 {
                    facade.scroll_viewport_down((-lines) as usize);
                }
                ui.prepare(gpu, snapshot);
                window.request_redraw();
            }
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
                    snapshot.input_modes,
                    is_numpad,
                ) else {
                    return;
                };
                let _ = worker.send(harbor_types::TerminalCommand::WritePtyInput(
                    bytes.into_owned(),
                ));
            }
            _ => {}
        }
    }
}

// ── App (own methods) ─────────────────────────────────────────────────────
impl App {
    /// Creates the application shell with no initial window, GPU, or worker.
    /// These are lazily initialised on the first `resumed` call.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            window: None,
            gpu: None,
            ui: None,
            latest_snapshot: None,
            updates: RevisionedUpdateReceiver::default(),
            worker: None,
            worker_status: WorkerStatus::Ready,
            event_proxy,
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

        #[cfg(target_os = "windows")]
        paint_gdi_background(&window);

        let font_handle = std::thread::Builder::new()
            .name("font-loader".into())
            .spawn(|| {
                #[cfg(target_os = "windows")]
                {
                    use windows::Win32::System::Threading::{
                        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
                    };
                    unsafe {
                        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                    }
                }
                load_system_fonts()
            })
            .expect("failed to spawn font-loader thread");

        let gpu =
            pollster::block_on(GpuContext::new(window.clone())).map_err(AppError::Renderer)?;
        gpu.clear_surface(bg_wgpu(harbor_config::BACKGROUND));

        let fonts = font_handle
            .join()
            .map_err(|_| AppError::Renderer(anyhow::anyhow!("font loader thread panicked")))?
            .map_err(AppError::Renderer)?;
        let metrics = TextMetrics::new(&fonts);

        let bootstrap = empty_snapshot(1, 1);
        let ui = UiRoot::new(&gpu, &bootstrap, fonts, metrics).map_err(AppError::Renderer)?;
        let size = ui.terminal_size(&gpu);
        let worker = TerminalWorkerClient::start(size, self.event_proxy.clone())
            .map_err(AppError::Worker)?;
        let initial = worker.take_update().ok_or_else(|| {
            AppError::Worker(anyhow::anyhow!("worker did not publish initial snapshot"))
        })?;

        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
        self.gpu = Some(gpu);
        self.ui = Some(ui);
        self.updates
            .accept(initial.clone())
            .expect("initial worker revision must be accepted");
        self.latest_snapshot = Some(initial.snapshot);
        self.worker_status = worker.status();
        self.worker = Some(worker);
        self.window = Some(window.clone());
        window.request_redraw();
        Ok(())
    }

    fn consume_worker_updates(&mut self) -> bool {
        let mut changed = false;
        loop {
            let update = self
                .worker
                .as_ref()
                .and_then(TerminalWorkerClient::take_update);
            let Some(update) = update else {
                break;
            };
            let Some(update) = self.updates.accept(update) else {
                continue;
            };
            self.latest_snapshot = Some(update.snapshot.clone());
            if let (Some(gpu), Some(ui)) = (self.gpu.as_mut(), self.ui.as_mut()) {
                ui.prepare_update(gpu, &update);
            }
            changed = true;
        }
        if let Some(worker) = self.worker.as_ref() {
            let status = worker.status();
            if status != self.worker_status {
                match &status {
                    WorkerStatus::Failed { .. } => {
                        tracing::error!(status = ?status, "terminal worker failed");
                    }
                    WorkerStatus::Stopped => {
                        tracing::info!(status = ?status, "terminal worker stopped");
                    }
                    WorkerStatus::Ready | WorkerStatus::Processing | WorkerStatus::Idle => {}
                }
                self.worker_status = status;
                changed = true;
            }
        }
        changed
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
                        load: wgpu::LoadOp::Clear(bg_wgpu(harbor_config::BACKGROUND)),
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
