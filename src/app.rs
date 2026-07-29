//! Application shell: winit lifecycle, window bootstrap, frame render.

mod confirmation;
pub(crate) mod translate;

use std::{
    cell::Cell,
    sync::{Arc, Mutex},
    time::Instant,
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use crate::event::{AppEvent, FrameControlFlow, FrameScheduler, RedrawReason};
use confirmation::ConfirmationWindow;
use harbor_pty::PtyEndpoints;
use harbor_terminal::{
    GpuContext, SurfaceDisposition, SurfaceStatus, Terminal, TextMetrics, load_system_fonts,
    surface_disposition,
};
use harbor_widget::input::event::UiEvent;
use translate::{ImeState, winit_to_uievent_with_ime};

// ── Thread-local GPU context scope for widget external draw pass ──────────────

thread_local! {
    static CURRENT_GPU: Cell<Option<*const GpuContext>> = const { Cell::new(None) };
}

/// Executes a closure with `gpu` set as the thread-local active GPU context.
/// Resets to `None` on return or unwind.
pub(crate) fn with_current_gpu<R>(gpu: &GpuContext, f: impl FnOnce() -> R) -> R {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            CURRENT_GPU.with(|c| c.set(None));
        }
    }
    CURRENT_GPU.with(|c| c.set(Some(gpu as *const GpuContext)));
    let _guard = Guard;
    f()
}

/// Accesses the active GPU context if within a `with_current_gpu` scope.
pub(crate) fn current_gpu<R>(f: impl FnOnce(&GpuContext) -> R) -> Option<R> {
    CURRENT_GPU.with(|c| {
        let ptr = c.get()?;
        let gpu = unsafe { &*ptr };
        Some(f(gpu))
    })
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

fn routes_terminal_input(
    gate_active: bool,
    event_draw_id: harbor_terminal::ExternalDrawId,
    terminal_draw_id: harbor_terminal::ExternalDrawId,
) -> bool {
    !gate_active && event_draw_id == terminal_draw_id
}

fn route_terminal_inputs(
    gate_active: bool,
    terminal_draw_id: harbor_terminal::ExternalDrawId,
    events: impl IntoIterator<Item = (harbor_terminal::ExternalDrawId, UiEvent)>,
    mut handle: impl FnMut(UiEvent),
) {
    for (event_draw_id, event) in events {
        if routes_terminal_input(gate_active, event_draw_id, terminal_draw_id) {
            handle(event);
        }
    }
}

fn redraw_reason_for_app_event(event: AppEvent) -> Option<RedrawReason> {
    match event {
        AppEvent::TerminalOutputReady => Some(RedrawReason::TerminalOutput),
    }
}

/// Outcome of a dialog-overlay event dispatch.
struct DialogResult {
    outcome: DialogOutcome,
    /// True when ScaleFactorChanged was handled, requiring a main-window redraw.
    needs_redraw: Option<RedrawReason>,
}

enum DialogOutcome {
    None,
    Cancelled,
    Confirmed(String),
}

/// Owns the optional paste-confirmation dialog and mediates its lifecycle.
struct DialogOverlay {
    window: Option<ConfirmationWindow>,
}

impl DialogOverlay {
    fn is_active(&self) -> bool {
        self.window.is_some()
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.window_id())
    }

    /// Dispatches a window event to the active confirmation dialog.
    fn handle_event(
        &mut self,
        event: &WindowEvent,
        scale: f32,
        gpu: Option<&GpuContext>,
        terminal: Option<&Arc<Mutex<Terminal>>>,
    ) -> DialogResult {
        let Some(mut confirmation) = self.window.take() else {
            return DialogResult {
                outcome: DialogOutcome::None,
                needs_redraw: None,
            };
        };
        let mut needs_redraw = None;
        let outcome = match confirmation.handle_event(event, scale) {
            confirmation::ConfirmationResult::Cancelled => DialogOutcome::Cancelled,
            confirmation::ConfirmationResult::Confirmed => {
                let raw_text = confirmation.raw_text().to_owned();
                DialogOutcome::Confirmed(raw_text)
            }
            confirmation::ConfirmationResult::None => {
                if matches!(event, WindowEvent::RedrawRequested)
                    && let (Some(gpu), Some(terminal)) = (gpu, terminal)
                {
                    let term = terminal.lock().unwrap();
                    confirmation.render(gpu.device(), gpu.queue(), &|ch| {
                        term.text_glyph(ch).copied()
                    });
                }
                if let WindowEvent::ScaleFactorChanged { scale_factor, .. } = event
                    && let Some(gpu) = gpu
                {
                    confirmation.scale_factor_changed(gpu.device(), *scale_factor);
                    needs_redraw = Some(RedrawReason::Resize);
                }
                if let WindowEvent::Resized(size) = event
                    && let Some(gpu) = gpu
                {
                    confirmation.resize(gpu.device(), size.width, size.height);
                }
                self.window = Some(confirmation);
                DialogOutcome::None
            }
        };
        DialogResult {
            outcome,
            needs_redraw,
        }
    }

    /// Installs a new confirmation dialog, replacing any existing one.
    #[allow(dead_code)]
    fn open(&mut self, confirmation: ConfirmationWindow) {
        self.window = Some(confirmation);
    }
}

/// Runtime resources that exist while the window is alive.
struct AppRuntime {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    terminal: Option<Arc<Mutex<Terminal>>>,
    /// Widget framework runtime.
    widget_runtime: Option<harbor_widget::runtime::Runtime>,
    dialog: DialogOverlay,
}

/// State governing scheduling and surface recovery.
struct FrameState {
    scheduler: FrameScheduler,
    surface_recovery_attempted: bool,
}

/// Winit coordinator over concrete lifecycle state groups.
pub(crate) struct App {
    runtime: AppRuntime,
    frame: FrameState,
    event_proxy: EventLoopProxy<AppEvent>,
    modifiers: ModifiersState,
    ime: ImeState,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create pty endpoints")]
    Pty(#[source] anyhow::Error),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
}

// ── ApplicationHandler (winit lifecycle) ──────────────────────────────────
impl ApplicationHandler<AppEvent> for App {
    /// Called on start or wake from suspend. Bootstraps the window, GPU,
    /// terminal engine, and PTY on first call.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    /// Handles redraw wakes posted by the terminal reader thread.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        if let Some(reason) = redraw_reason_for_app_event(event) {
            self.request_redraw(reason);
        }
    }

    /// Called when the event loop is about to block.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.frame.scheduler.set_deadline(None);
        if self.frame.scheduler.should_request_continuous_redraw() {
            self.request_redraw(RedrawReason::Active);
        }
        self.set_control_flow(event_loop);
    }

    /// Dispatches window-level events: resize, redraw, close, and terminal input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let dialog_window_id = self.runtime.dialog.window_id();
        if dialog_window_id == Some(window_id) {
            let scale = self
                .runtime
                .window
                .as_ref()
                .map(|window| window.scale_factor() as f32)
                .unwrap_or(1.0);
            let result = self.runtime.dialog.handle_event(
                &event,
                scale,
                self.runtime.gpu.as_ref(),
                self.runtime.terminal.as_ref(),
            );
            if let Some(reason) = result.needs_redraw {
                self.request_redraw(reason);
            }
            match result.outcome {
                DialogOutcome::Cancelled => {
                    self.request_redraw(RedrawReason::Input);
                    return;
                }
                DialogOutcome::Confirmed(raw_text) => {
                    if let Some(terminal) = self.runtime.terminal.as_ref()
                        && let Ok(mut terminal) = terminal.lock()
                    {
                        let bytes = terminal
                            .drain_and_snapshot()
                            .input_modes
                            .paste(raw_text.as_bytes());
                        if let Err(error) = terminal.write_pty(&bytes) {
                            tracing::warn!(error = %format_args!("{error:#}"), "failed to write confirmed paste");
                        }
                    }
                    self.request_redraw(RedrawReason::Input);
                    return;
                }
                DialogOutcome::None => {}
            }
        }
        let gate_active = self.runtime.dialog.is_active();

        let (Some(gpu), Some(terminal), Some(window)) = (
            self.runtime.gpu.as_mut(),
            self.runtime.terminal.as_ref(),
            self.runtime.window.as_ref(),
        ) else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        if gate_active && matches!(&event, WindowEvent::KeyboardInput { .. }) {
            return;
        }

        if matches!(&event, WindowEvent::Focused(false)) {
            self.frame.scheduler.set_active(false);
        }

        if let WindowEvent::KeyboardInput { event: kbd, .. } = &event
            && kbd.state == ElementState::Pressed
        {
            let snapshot = terminal.lock().unwrap().drain_and_snapshot();
            if let Some(navigation) =
                scrollback_navigation(&kbd.logical_key, self.modifiers, snapshot.is_alt)
            {
                let mut terminal = terminal.lock().unwrap();
                match navigation {
                    ScrollbackNavigation::PageUp => terminal.scroll_viewport_up(snapshot.rows),
                    ScrollbackNavigation::PageDown => terminal.scroll_viewport_down(snapshot.rows),
                    ScrollbackNavigation::Top => terminal.scroll_viewport_to_top(),
                    ScrollbackNavigation::Bottom => terminal.scroll_viewport_to_bottom(),
                }
                Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
                return;
            }
            Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
        }

        if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
            let scale = window.scale_factor() as f32;
            let is_keyboard = matches!(&event, WindowEvent::KeyboardInput { .. });
            if (!gate_active || !is_keyboard)
                && let Some(ui_event) =
                    winit_to_uievent_with_ime(&event, scale, self.modifiers, &mut self.ime)
            {
                let frame_request = widget_runtime.dispatch(ui_event, Instant::now());
                if frame_request.needs_redraw {
                    Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
                }
                let draw_id = terminal.lock().unwrap().draw_id();
                route_terminal_inputs(
                    gate_active,
                    draw_id,
                    widget_runtime.drain_external_input(),
                    |external_event| {
                        if let Err(error) = terminal.lock().unwrap().handle_event(external_event) {
                            tracing::warn!(error = %format_args!("{error:#}"), "failed to write terminal input");
                        }
                    },
                );
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
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
                self.frame.surface_recovery_attempted = false;
                gpu.resize(size.width, size.height);
                let mut terminal = terminal.lock().unwrap();
                let terminal_size = terminal.terminal_size(gpu);
                terminal.resize_gpu(terminal_size, gpu);
                if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
                    let scale = window.scale_factor() as f32;
                    let viewport =
                        harbor_widget::renderer::Viewport::new(size.width, size.height, scale);
                    widget_runtime.set_viewport(viewport);
                    widget_runtime.update(Instant::now());
                }
                Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Resize);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                tracing::trace!(?scale_factor, "main window scale factor changed");
                let scale = scale_factor as f32;
                let (physical_width, physical_height) = gpu.surface_size();
                gpu.reconfigure();
                if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
                    let viewport = harbor_widget::renderer::Viewport::new(
                        physical_width,
                        physical_height,
                        scale,
                    );
                    widget_runtime.set_viewport(viewport);
                    widget_runtime.update(Instant::now());
                }
                Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Resize);
            }
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                self.frame.scheduler.redraw_requested();
                self.render_frame();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let snapshot = terminal.lock().unwrap().drain_and_snapshot();
                if snapshot.is_alt {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y * 3.0) as isize,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as isize,
                };
                if lines > 0 {
                    terminal.lock().unwrap().scroll_viewport_up(lines as usize);
                    Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
                } else if lines < 0 {
                    terminal
                        .lock()
                        .unwrap()
                        .scroll_viewport_down(lines.unsigned_abs());
                    Self::wake_redraw(&mut self.frame.scheduler, window, RedrawReason::Input);
                }
            }
            _ => {}
        }
    }
}

// ── App (own methods) ─────────────────────────────────────────────────────
impl App {
    /// Creates the application shell with no initial window, GPU, or terminal.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            runtime: AppRuntime {
                window: None,
                gpu: None,
                terminal: None,
                widget_runtime: None,
                dialog: DialogOverlay { window: None },
            },
            frame: FrameState {
                scheduler: FrameScheduler::default(),
                surface_recovery_attempted: false,
            },
            event_proxy,
            modifiers: ModifiersState::default(),
            ime: ImeState::default(),
        }
    }

    /// Creates the main window, GPU context, font atlas, and terminal engine.
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.runtime.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);
        // Winit 0.30 emits composition commits only after IME is explicitly enabled.
        window.set_ime_allowed(true);

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

        let size = Terminal::terminal_size_for(&gpu, &metrics);
        let (pty_read, pty_write, pty_control) = PtyEndpoints::spawn_shell(size)
            .map_err(AppError::Pty)?
            .into_parts();
        let event_proxy = self.event_proxy.clone();
        let terminal = Terminal::new(
            size,
            pty_read,
            pty_write,
            pty_control,
            &gpu,
            fonts,
            metrics,
            move || {
                event_proxy
                    .send_event(AppEvent::TerminalOutputReady)
                    .is_ok()
            },
        );
        let terminal = Arc::new(Mutex::new(terminal));

        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");
        self.runtime.gpu = Some(gpu);
        self.runtime.terminal = Some(terminal);
        self.init_widget_runtime();
        self.runtime.window = Some(window.clone());
        self.request_redraw(RedrawReason::Input);
        Ok(())
    }

    fn request_redraw(&mut self, reason: RedrawReason) {
        if let Some(window) = self.runtime.window.as_ref() {
            Self::wake_redraw(&mut self.frame.scheduler, window, reason);
        }
    }

    fn wake_redraw(scheduler: &mut FrameScheduler, window: &Window, reason: RedrawReason) {
        if scheduler.wake(reason) {
            tracing::trace!(?reason, "requesting redraw");
            window.request_redraw();
        }
    }

    fn set_control_flow(&self, event_loop: &ActiveEventLoop) {
        match self.frame.scheduler.control_flow() {
            FrameControlFlow::Wait => event_loop.set_control_flow(ControlFlow::Wait),
            FrameControlFlow::WaitUntil(deadline) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
            FrameControlFlow::Poll => event_loop.set_control_flow(ControlFlow::Poll),
        }
    }

    fn render_frame(&mut self) {
        let (Some(gpu), Some(terminal)) =
            (self.runtime.gpu.as_mut(), self.runtime.terminal.as_ref())
        else {
            return;
        };
        if let Ok(mut terminal) = terminal.lock() {
            terminal.drain_pty();
        }

        let frame = gpu.get_current_texture();
        let status = match &frame {
            wgpu::CurrentSurfaceTexture::Success(_) => SurfaceStatus::Success,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) => SurfaceStatus::Suboptimal,
            wgpu::CurrentSurfaceTexture::Lost => SurfaceStatus::Lost,
            wgpu::CurrentSurfaceTexture::Outdated => SurfaceStatus::Outdated,
            wgpu::CurrentSurfaceTexture::Timeout => SurfaceStatus::Timeout,
            wgpu::CurrentSurfaceTexture::Occluded => SurfaceStatus::Occluded,
            wgpu::CurrentSurfaceTexture::Validation => SurfaceStatus::Validation,
        };
        let disposition = surface_disposition(status);
        let (output, reconfigure_after_present) = match (frame, disposition) {
            (wgpu::CurrentSurfaceTexture::Success(output), SurfaceDisposition::Present) => {
                (output, false)
            }
            (
                wgpu::CurrentSurfaceTexture::Suboptimal(output),
                SurfaceDisposition::PresentAndReconfigure,
            ) => {
                tracing::warn!("surface texture suboptimal; presenting then reconfiguring");
                (output, true)
            }
            (_, SurfaceDisposition::ReconfigureAndRedraw) => {
                tracing::warn!(?status, "surface requires reconfiguration");
                if !self.frame.surface_recovery_attempted {
                    self.frame.surface_recovery_attempted = true;
                    gpu.reconfigure();
                    self.request_redraw(RedrawReason::SurfaceRecovery);
                } else {
                    tracing::warn!(?status, "surface recovery deferred until external wake");
                }
                return;
            }
            (_, SurfaceDisposition::Skip) => {
                tracing::debug!(?status, "surface frame skipped");
                return;
            }
            _ => unreachable!("surface disposition must match texture status"),
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

            if let Some(widget_runtime) = self.runtime.widget_runtime.as_mut() {
                let scale = self.runtime.window.as_ref().unwrap().scale_factor() as f32;
                let (physical_w, physical_h) = gpu.surface_size();
                let viewport =
                    harbor_widget::renderer::Viewport::new(physical_w, physical_h, scale);

                with_current_gpu(gpu, || {
                    widget_runtime.encode(gpu.queue(), &mut render_pass, viewport);
                });
            }
        }

        let command_buffer = encoder.finish();
        gpu.queue().submit(Some(command_buffer));
        gpu.present(output);
        tracing::trace!(?status, "surface frame presented");
        if reconfigure_after_present && !self.frame.surface_recovery_attempted {
            self.frame.surface_recovery_attempted = true;
            gpu.reconfigure();
            self.request_redraw(RedrawReason::SurfaceSuboptimal);
        } else if status == SurfaceStatus::Success {
            self.frame.surface_recovery_attempted = false;
        }
    }

    /// Initializes the widget runtime with a terminal CustomPaint root.
    fn init_widget_runtime(&mut self) {
        use harbor_widget::widgets::custom_paint::CustomPaint;

        let terminal_arc = self.runtime.terminal.as_ref().unwrap().clone();
        let draw_id = terminal_arc.lock().unwrap().draw_id();

        let handler: Arc<harbor_widget::scene::primitive::ExternalDrawFn<'static>> =
            Arc::new(move |id, rect, pass| {
                current_gpu(|gpu| {
                    if let Ok(mut term) = terminal_arc.lock() {
                        term.render(id, rect, pass, gpu);
                    }
                });
            });

        let custom_paint = CustomPaint::new(draw_id).handler(handler);
        let gpu = self.runtime.gpu.as_ref().unwrap();
        let mut runtime = harbor_widget::runtime::Runtime::new();
        runtime.set_root(custom_paint);
        runtime.init_renderer(gpu.device(), gpu.format());
        runtime.update(Instant::now());
        runtime.focus_first_focusable();

        self.runtime.widget_runtime = Some(runtime);
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
/// wgpu surface is ready.
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

    #[test]
    fn external_input_requires_matching_draw_id_and_an_open_gate() {
        assert!(routes_terminal_input(false, 7, 7));
        assert!(!routes_terminal_input(false, 6, 7));
        assert!(!routes_terminal_input(true, 7, 7));
    }

    #[test]
    fn routes_only_matching_external_input_when_gate_is_open() {
        use harbor_widget::input::event::{Key as WidgetKey, KeyboardEvent, Modifiers, UiEvent};

        let matching = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Enter,
            modifiers: Modifiers::default(),
        });
        let other = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Escape,
            modifiers: Modifiers::default(),
        });
        let mut routed = Vec::new();

        route_terminal_inputs(false, 7, [(6, other), (7, matching.clone())], |event| {
            routed.push(event)
        });
        route_terminal_inputs(true, 7, [(7, matching.clone())], |event| routed.push(event));

        assert_eq!(routed, vec![matching]);
    }

    #[test]
    fn terminal_output_event_requests_terminal_output_redraw() {
        assert_eq!(
            redraw_reason_for_app_event(AppEvent::TerminalOutputReady),
            Some(RedrawReason::TerminalOutput)
        );
    }
}
