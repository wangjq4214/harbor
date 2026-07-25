//! Secondary winit window for paste confirmation rendered by Widget Runtime.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    window::Window,
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::app::winit_to_uievent;
use harbor_render::GpuContext;
use harbor_text::TextMetrics;
use harbor_widget::runtime::Runtime;
use harbor_widget::widgets::button::Button;
use harbor_widget::widgets::column::Column;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::widgets::text_label::TextLabel;

const DIALOG_WIDTH: u32 = 600;
const DIALOG_HEIGHT: u32 = 400;

fn centered_dialog_position(
    main_position: winit::dpi::PhysicalPosition<i32>,
    main_size: winit::dpi::PhysicalSize<u32>,
    scale_factor: f64,
) -> winit::dpi::PhysicalPosition<i32> {
    let dialog_width = (f64::from(DIALOG_WIDTH) * scale_factor).round() as i32;
    let dialog_height = (f64::from(DIALOG_HEIGHT) * scale_factor).round() as i32;

    winit::dpi::PhysicalPosition::new(
        main_position.x + (main_size.width as i32 - dialog_width) / 2,
        main_position.y + (main_size.height as i32 - dialog_height) / 2,
    )
}

// ── ConfirmationResult ──────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) enum ConfirmationResult {
    None,
    Cancelled,
    Confirmed,
}

// ── ConfirmationWindow ──────────────────────────────────────────────────────

/// A secondary owned winit window with its own Widget Runtime for paste
/// confirmation UI.
///
/// Renders a minimal confirmation dialog: header text showing line count,
/// and a Cancel button (focused by default).
pub(crate) struct ConfirmationWindow {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    runtime: Runtime,
    text_metrics: TextMetrics,
    cancelled: Arc<AtomicBool>,
}

impl ConfirmationWindow {
    pub(crate) fn new(
        raw_text: &str,
        event_loop: &ActiveEventLoop,
        gpu: &GpuContext,
        metrics: TextMetrics,
        text_bind_group_layout: &wgpu::BindGroupLayout,
        text_bind_group: &wgpu::BindGroup,
        main_window: Option<&Window>,
    ) -> Self {
        let line_count = raw_text.lines().count();

        let mut window_attrs = Window::default_attributes()
            .with_title("")
            .with_decorations(false)
            .with_inner_size(winit::dpi::LogicalSize::new(DIALOG_WIDTH, DIALOG_HEIGHT))
            .with_resizable(false);

        // Owned window on Windows: keep dialog above main window in z-order.
        #[cfg(target_os = "windows")]
        if let Some(main) = main_window {
            let hwnd = main.window_handle().ok().and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            });
            if let Some(hwnd) = hwnd {
                window_attrs = window_attrs.with_owner_window(hwnd);
            }
        }

        // Center dialog over the main window using physical pixels.
        if let Some(main) = main_window
            && let Ok(outer) = main.outer_position()
        {
            window_attrs = window_attrs.with_position(centered_dialog_position(
                outer,
                main.outer_size(),
                main.scale_factor(),
            ));
        }

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("create confirmation window"),
        );

        let surface = gpu.create_surface(Arc::clone(&window));

        let caps = gpu.surface_capabilities(&surface);
        let format = gpu.format();
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| *f == format)
            .unwrap_or(caps.formats[0]);
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: DIALOG_WIDTH,
            height: DIALOG_HEIGHT,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(gpu.device(), &surface_config);

        // ── Widget Runtime setup ──────────────────────────────────────
        let cancelled = Arc::new(AtomicBool::new(false));

        // Set up thread-local metrics for this thread.
        harbor_widget::text::set_current_metrics(metrics);

        let mut runtime = Runtime::new();

        let confirm_root = build_confirmation_root(line_count, Arc::clone(&cancelled));
        runtime.set_root(confirm_root);

        // Init quad renderer (needed for button backgrounds/borders).
        runtime.init_renderer(gpu.device(), gpu.format());

        // Init text renderer with the shared glyph atlas.
        runtime.init_text_renderer(
            gpu.device(),
            gpu.format(),
            text_bind_group_layout,
            text_bind_group,
        );

        // Trigger initial build + layout.
        let viewport = harbor_widget::renderer::Viewport::new(DIALOG_WIDTH, DIALOG_HEIGHT, 1.0);
        runtime.set_viewport(viewport);
        runtime.update(std::time::Instant::now());

        // Set focus on the Cancel button (first focusable widget).
        runtime.focus_first_focusable();
        window.request_redraw();

        ConfirmationWindow {
            window,
            surface,
            surface_config,
            runtime,
            text_metrics: metrics,
            cancelled,
        }
    }

    pub(crate) fn window_id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    /// Handles a winit event for this window.
    ///
    /// Translates winit events to Widget UiEvents, dispatches to the
    /// Runtime, and checks the cancellation flag. Window-level shortcuts
    /// (Escape, n) trigger cancellation directly.
    pub(crate) fn handle_event(
        &mut self,
        event: &WindowEvent,
        scale_factor: f32,
    ) -> ConfirmationResult {
        match event {
            WindowEvent::CloseRequested => return ConfirmationResult::Cancelled,

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } if key_event.state == ElementState::Pressed => match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => return ConfirmationResult::Cancelled,
                Key::Character(ch) if ch == "n" || ch == "N" => {
                    return ConfirmationResult::Cancelled;
                }
                _ => {}
            },

            _ => {}
        }

        // Translate to UiEvent and dispatch.
        if let Some(ui_event) = winit_to_uievent(
            event,
            scale_factor,
            winit::keyboard::ModifiersState::default(),
        ) {
            self.runtime.dispatch(ui_event, std::time::Instant::now());
        }

        if self.cancelled.load(Ordering::SeqCst) {
            ConfirmationResult::Cancelled
        } else {
            ConfirmationResult::None
        }
    }

    /// Renders one frame: update Runtime, register text runs, encode, submit, present.
    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        glyph_fn: &harbor_widget::text::GlyphFn<'_>,
    ) {
        // Ensure layout is up-to-date before encode.
        self.runtime.update(std::time::Instant::now());

        // Register any text runs queued by widgets during the paint pass.
        self.runtime.register_pending_text_runs(glyph_fn);

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(o) => o,
            wgpu::CurrentSurfaceTexture::Suboptimal(o) => o,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                let (w, h) = (self.surface_config.width, self.surface_config.height);
                self.surface.configure(device, &self.surface_config);
                // Update viewport for the reconfigured surface
                self.runtime
                    .set_viewport(harbor_widget::renderer::Viewport::new(w, h, 1.0));
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return,
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("confirmation window"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("confirmation pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let scale = 1.0f32; // Non-resizable window at 1x
            let viewport =
                harbor_widget::renderer::Viewport::new(DIALOG_WIDTH, DIALOG_HEIGHT, scale);
            self.runtime.encode(queue, &mut pass, viewport, None);
        }

        queue.submit(Some(encoder.finish()));
        queue.present(output);
    }

    /// Handles resize events.
    pub(crate) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(device, &self.surface_config);
        let viewport = harbor_widget::renderer::Viewport::new(
            width,
            height,
            self.window.scale_factor() as f32,
        );
        self.runtime.set_viewport(viewport);
        self.runtime.update(std::time::Instant::now());
    }
}

fn build_confirmation_root(
    line_count: usize,
    cancelled: Arc<AtomicBool>,
) -> impl harbor_widget::view::Component {
    let header_text = format!("Paste {} lines?", line_count);

    Padding::new(24.0, 16.0, 24.0, 16.0).child(
        Column::new()
            .child(TextLabel::new(header_text))
            .child(SizedBox::new(harbor_widget::layout::Size::new(0.0, 24.0)))
            .child(Button::new("Cancel").on_click(move |_ctx| {
                cancelled.store(true, Ordering::SeqCst);
            })),
    )
}

// ── Confirmation widget tree ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_dialog_in_main_window() {
        assert_eq!(
            centered_dialog_position(
                winit::dpi::PhysicalPosition::new(100, 100),
                winit::dpi::PhysicalSize::new(1_000, 800),
                1.0,
            ),
            winit::dpi::PhysicalPosition::new(300, 300),
        );
    }

    #[test]
    fn centers_scaled_dialog_on_negative_coordinate_monitor() {
        assert_eq!(
            centered_dialog_position(
                winit::dpi::PhysicalPosition::new(-1_920, 100),
                winit::dpi::PhysicalSize::new(1_200, 900),
                1.5,
            ),
            winit::dpi::PhysicalPosition::new(-1_770, 250),
        );
    }
}
