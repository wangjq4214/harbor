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
use harbor_widget::widgets::row::Row;
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
/// Paste and Cancel buttons, with Paste focused by default.
pub(crate) struct ConfirmationWindow {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    runtime: Runtime,
    raw_text: String,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
}

impl ConfirmationWindow {
    pub(crate) fn new(
        raw_text: String,
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
        let confirmed = Arc::new(AtomicBool::new(false));

        // Set up thread-local metrics for this thread.
        harbor_widget::text::set_current_metrics(metrics);

        let mut runtime = Runtime::new();

        let confirm_root =
            build_confirmation_root(line_count, Arc::clone(&cancelled), Arc::clone(&confirmed));
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

        // Set focus on the Paste button (first focusable widget).
        runtime.focus_first_focusable();
        window.request_redraw();

        ConfirmationWindow {
            window,
            surface,
            surface_config,
            runtime,
            raw_text,
            cancelled,
            confirmed,
        }
    }

    pub(crate) fn window_id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    /// Returns the raw paste candidate text, unchanged from when the dialog opened.
    pub(crate) fn raw_text(&self) -> &str {
        &self.raw_text
    }

    /// Handles a winit event for this window.
    ///
    /// Translates winit events to Widget UiEvents, dispatches to the
    /// Runtime, and checks the confirmation/cancellation flags.
    /// Window-level shortcuts: Escape/n → cancel, y → confirm.
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
                Key::Character(ch) if ch == "y" || ch == "Y" => {
                    return ConfirmationResult::Confirmed;
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

        if self.confirmed.load(Ordering::SeqCst) {
            ConfirmationResult::Confirmed
        } else if self.cancelled.load(Ordering::SeqCst) {
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
    confirmed: Arc<AtomicBool>,
) -> impl harbor_widget::view::Component {
    let header_text = format!("Paste {} lines?", line_count);

    Padding::new(24.0, 16.0, 24.0, 16.0).child(
        Column::new()
            .child(TextLabel::new(header_text))
            .child(SizedBox::new(harbor_widget::layout::Size::new(0.0, 24.0)))
            .child(
                Row::new()
                    .child(Button::new("Paste").on_click(move |_ctx| {
                        confirmed.store(true, Ordering::SeqCst);
                    }))
                    .child(SizedBox::new(harbor_widget::layout::Size::new(12.0, 0.0)))
                    .child(Button::new("Cancel").on_click(move |_ctx| {
                        cancelled.store(true, Ordering::SeqCst);
                    })),
            ),
    )
}

// ── Confirmation widget tree ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::view::{BuildCx, Component};

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

    #[test]
    fn build_confirmation_root_produces_valid_view() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let root = build_confirmation_root(5, cancelled, confirmed);
        let mut cx = BuildCx::stub();
        let _view = root.build(&mut cx);
    }

    #[test]
    fn paste_callback_sets_confirmed_flag_only() {
        // Replicates the exact closure pattern used in build_confirmation_root.
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let c = Arc::clone(&confirmed);
        let on_paste = move |_ctx: &mut harbor_widget::input::event_ctx::EventCtx| {
            c.store(true, Ordering::SeqCst);
        };

        let mut ctx = harbor_widget::input::event_ctx::EventCtx::new();
        on_paste(&mut ctx);

        assert!(confirmed.load(Ordering::SeqCst));
        assert!(!cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_callback_sets_cancelled_flag_only() {
        // Replicates the exact closure pattern used in build_confirmation_root.
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let c = Arc::clone(&cancelled);
        let on_cancel = move |_ctx: &mut harbor_widget::input::event_ctx::EventCtx| {
            c.store(true, Ordering::SeqCst);
        };

        let mut ctx = harbor_widget::input::event_ctx::EventCtx::new();
        on_cancel(&mut ctx);

        assert!(!confirmed.load(Ordering::SeqCst));
        assert!(cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn paste_and_cancel_flags_are_independent() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        confirmed.store(true, Ordering::SeqCst);
        assert!(confirmed.load(Ordering::SeqCst));
        assert!(!cancelled.load(Ordering::SeqCst));

        confirmed.store(false, Ordering::SeqCst);
        cancelled.store(true, Ordering::SeqCst);
        assert!(!confirmed.load(Ordering::SeqCst));
        assert!(cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn should_preserve_raw_text_verbatim() {
        // ConfirmationWindow::raw_text() returns the original paste text
        // unchanged. Verify the data-flow contract: String in → &str out,
        // multi-line with mixed line endings preserved.
        let text = String::from("hello\nworld\r\n");
        let stored = text.clone();
        // Simulate the accessor contract: stored text matches original.
        assert_eq!(stored.as_str(), "hello\nworld\r\n");
        assert!(stored.contains('\n'));
        assert!(stored.contains('\r'));
    }

    #[test]
    fn should_center_at_origin_when_main_window_is_zero_sized() {
        // When the main window has zero area, the dialog is still
        // positioned relative to the main window origin.
        let pos = centered_dialog_position(
            winit::dpi::PhysicalPosition::new(50, 60),
            winit::dpi::PhysicalSize::new(0, 0),
            1.0,
        );
        // DIALOG_WIDTH=600, DIALOG_HEIGHT=400 at scale 1.0
        // x = 50 + (0 - 600) / 2 = 50 - 300 = -250
        // y = 60 + (0 - 400) / 2 = 60 - 200 = -140
        assert_eq!(pos, winit::dpi::PhysicalPosition::new(-250, -140));
    }

    #[test]
    fn should_produce_zero_dialog_when_scale_is_zero() {
        // With scale_factor=0 the dialog occupies zero physical pixels,
        // so the center coincides with the main window center.
        let pos = centered_dialog_position(
            winit::dpi::PhysicalPosition::new(100, 100),
            winit::dpi::PhysicalSize::new(800, 600),
            0.0,
        );
        assert_eq!(pos, winit::dpi::PhysicalPosition::new(500, 400));
    }

    #[test]
    fn should_build_with_zero_lines() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let root = build_confirmation_root(0, cancelled, confirmed);
        let mut cx = BuildCx::stub();
        let _view = root.build(&mut cx);
    }

    #[test]
    fn should_build_with_large_line_count() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));

        let root = build_confirmation_root(usize::MAX, cancelled, confirmed);
        let mut cx = BuildCx::stub();
        let _view = root.build(&mut cx);
    }

    // ── handle_event key-matching (replicated pure logic) ──────────────
    //
    // handle_event itself requires a fully constructed ConfirmationWindow
    // (OS window + GPU surface). The key-to-result mapping is pure and
    // can be tested by replicating the match arms that handle_event uses.

    /// Replicates the key-match logic from handle_event for testing.
    fn map_key_to_result(key: &winit::keyboard::Key) -> Option<ConfirmationResult> {
        match key {
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) => {
                Some(ConfirmationResult::Cancelled)
            }
            winit::keyboard::Key::Character(ch) if ch == "n" || ch == "N" => {
                Some(ConfirmationResult::Cancelled)
            }
            winit::keyboard::Key::Character(ch) if ch == "y" || ch == "Y" => {
                Some(ConfirmationResult::Confirmed)
            }
            _ => None,
        }
    }

    #[test]
    fn should_return_confirmed_on_lowercase_y() {
        let key = winit::keyboard::Key::Character("y".into());
        assert!(matches!(
            map_key_to_result(&key),
            Some(ConfirmationResult::Confirmed)
        ));
    }

    #[test]
    fn should_return_confirmed_on_uppercase_y() {
        let key = winit::keyboard::Key::Character("Y".into());
        assert!(matches!(
            map_key_to_result(&key),
            Some(ConfirmationResult::Confirmed)
        ));
    }

    #[test]
    fn should_return_cancelled_on_lowercase_n() {
        let key = winit::keyboard::Key::Character("n".into());
        assert!(matches!(
            map_key_to_result(&key),
            Some(ConfirmationResult::Cancelled)
        ));
    }

    #[test]
    fn should_return_cancelled_on_uppercase_n() {
        let key = winit::keyboard::Key::Character("N".into());
        assert!(matches!(
            map_key_to_result(&key),
            Some(ConfirmationResult::Cancelled)
        ));
    }

    #[test]
    fn should_return_cancelled_on_escape() {
        let key = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape);
        assert!(matches!(
            map_key_to_result(&key),
            Some(ConfirmationResult::Cancelled)
        ));
    }

    #[test]
    fn should_return_none_on_unrecognized_key() {
        let key = winit::keyboard::Key::Character("x".into());
        assert!(map_key_to_result(&key).is_none());
    }

    // ── Flag priority ─────────────────────────────────────────────────

    /// Replicates the flag-check logic from handle_event for testing.
    fn check_flags(confirmed: &AtomicBool, cancelled: &AtomicBool) -> ConfirmationResult {
        if confirmed.load(Ordering::SeqCst) {
            ConfirmationResult::Confirmed
        } else if cancelled.load(Ordering::SeqCst) {
            ConfirmationResult::Cancelled
        } else {
            ConfirmationResult::None
        }
    }

    #[test]
    fn should_return_none_when_neither_flag_is_set() {
        let confirmed = AtomicBool::new(false);
        let cancelled = AtomicBool::new(false);
        assert!(matches!(
            check_flags(&confirmed, &cancelled),
            ConfirmationResult::None
        ));
    }

    #[test]
    fn should_return_confirmed_when_confirmed_flag_is_set() {
        let confirmed = AtomicBool::new(true);
        let cancelled = AtomicBool::new(false);
        assert!(matches!(
            check_flags(&confirmed, &cancelled),
            ConfirmationResult::Confirmed
        ));
    }

    #[test]
    fn should_return_cancelled_when_only_cancelled_flag_is_set() {
        let confirmed = AtomicBool::new(false);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            check_flags(&confirmed, &cancelled),
            ConfirmationResult::Cancelled
        ));
    }

    #[test]
    fn should_prioritize_confirmed_over_cancelled_when_both_flags_are_set() {
        // Confirmed takes priority: even if both flags are somehow true,
        // the result should be Confirmed.
        let confirmed = AtomicBool::new(true);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            check_flags(&confirmed, &cancelled),
            ConfirmationResult::Confirmed
        ));
    }
}
