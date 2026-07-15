//! Confirmation dialog for multi-line paste when bracketed paste is OFF.

use std::sync::Arc;
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    window::Window,
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use harbor_render::gpu::ColoredVertex;
use harbor_render::{AtlasGlyph, GpuContext, TextMetrics};
use harbor_terminal::safe_preview_line;
use harbor_ui::{DialogButton, DialogResult};

const DIALOG_WIDTH: u32 = 600;

const DIALOG_HEIGHT: u32 = 400;
const HEADER_HEIGHT: f32 = 40.0;
const BUTTON_HEIGHT: f32 = 30.0;
const BUTTON_WIDTH: f32 = 120.0;
const TEXT_PADDING: f32 = 16.0;
/// Number of visible preview lines.
const VISIBLE_LINES: usize = 5;
/// Scrollbar width in pixels.
const SCROLLBAR_WIDTH: f32 = 8.0;
/// Vertical gap between preview area top and header bottom.
const PREVIEW_TOP_GAP: f32 = 6.0;
/// Vertical gap between preview area bottom and buttons top.
const PREVIEW_BOTTOM_GAP: f32 = 12.0;

fn background_color() -> [f32; 4] {
    harbor_config::BACKGROUND
}

/// A secondary winit window that asks the user to confirm a multi-line paste.
pub(crate) struct PasteDialog {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    #[allow(dead_code)]
    surface_config: wgpu::SurfaceConfiguration,
    /// Raw (unmodified) text to send to the PTY on confirm.
    pub(crate) raw_text: String,
    /// Logical line count (before wrapping).
    logical_line_count: usize,
    focused_button: DialogButton,
    bg_pipeline: Arc<wgpu::RenderPipeline>,
    bg_vertex_buffer: wgpu::Buffer,
    bg_vertex_count: u32,
    text_vertex_buffer: wgpu::Buffer,
    text_vertex_count: u32,
    dirty: bool,
    /// ── #23 preview state ─────────────────────────────────────────────────
    /// All wrapped preview lines (split by \n, each wrapped to fit).
    /// Populated lazily on first `prepare()` after metrics are known.
    wrapped_lines: Vec<String>,
    /// Total number of wrapped lines (for scrollbar range).
    total_preview_lines: usize,
    /// Scroll offset in wrapped lines from top.
    scroll_offset: usize,
    /// Whether wrapped_lines have been computed.
    preview_initialized: bool,
}

impl PasteDialog {
    pub(crate) fn new(
        raw_text: String,
        event_loop: &ActiveEventLoop,
        gpu: &GpuContext,
        main_window: Option<&Window>,
    ) -> Self {
        let logical_line_count = raw_text.lines().count();

        let mut window_attrs = Window::default_attributes()
            .with_title("")
            .with_decorations(false)
            .with_inner_size(winit::dpi::LogicalSize::new(DIALOG_WIDTH, DIALOG_HEIGHT))
            .with_resizable(false);

        // Owned window on Windows: keep dialog above main window in z-order
        // and tie minimize/destroy together.
        #[cfg(target_os = "windows")]
        if let Some(main) = main_window {
            let hwnd = main.window_handle().ok().and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            });
            if let Some(hwnd) = hwnd {
                // SAFETY: HWND from raw-window-handle is the Win32 window handle.
                window_attrs = window_attrs.with_owner_window(hwnd);
            }
        }

        // Center dialog over the main window.
        if let Some(main) = main_window
            && let Ok(outer) = main.outer_position()
        {
            let main_size = main.inner_size();
            let main_x = outer.x as f64;
            let main_y = outer.y as f64;
            let main_w = main_size.width as f64;
            let main_h = main_size.height as f64;
            let dialog_x = main_x + (main_w - DIALOG_WIDTH as f64) / 2.0;
            let dialog_y = main_y + (main_h - DIALOG_HEIGHT as f64) / 2.0;
            window_attrs = window_attrs.with_position(winit::dpi::LogicalPosition::new(
                dialog_x.max(0.0),
                dialog_y.max(0.0),
            ));
        }

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("create paste dialog window"),
        );

        let surface = gpu.create_surface(Arc::clone(&window));
        let caps = gpu.surface_capabilities(&surface);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| *f == gpu.format())
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

        let bg_vertex_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("paste dialog bg vertices"),
            size: (2048 * std::mem::size_of::<ColoredVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let text_vertex_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("paste dialog text vertices"),
            size: (8192 * std::mem::size_of::<harbor_render::gpu::TexturedVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            window,
            surface,
            surface_config,
            raw_text,
            logical_line_count,
            focused_button: DialogButton::Cancel,
            bg_pipeline: gpu.colored_quad_pipeline(),
            bg_vertex_buffer,
            bg_vertex_count: 0,
            text_vertex_buffer,
            text_vertex_count: 0,
            dirty: true,
            wrapped_lines: Vec::new(),
            total_preview_lines: 0,
            scroll_offset: 0,
            preview_initialized: false,
        }
    }

    pub(crate) fn window_id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub(crate) fn handle_event(&mut self, event: &WindowEvent) -> DialogResult {
        let result = match event {
            WindowEvent::CloseRequested => DialogResult::Cancelled,

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } if key_event.state == ElementState::Pressed => {
                match &key_event.logical_key {
                    Key::Named(NamedKey::Escape) => DialogResult::Cancelled,

                    // y / n shortcuts for confirm / cancel
                    Key::Character(ch) if ch == "y" || ch == "Y" => DialogResult::Confirmed,
                    Key::Character(ch) if ch == "n" || ch == "N" => DialogResult::Cancelled,

                    Key::Named(NamedKey::Enter) => match self.focused_button {
                        DialogButton::Paste => DialogResult::Confirmed,
                        DialogButton::Cancel => DialogResult::Cancelled,
                    },

                    Key::Named(NamedKey::Tab) => {
                        self.focused_button = match self.focused_button {
                            DialogButton::Paste => DialogButton::Cancel,
                            DialogButton::Cancel => DialogButton::Paste,
                        };
                        self.dirty = true;
                        DialogResult::None
                    }

                    // Preview scrolling
                    Key::Named(NamedKey::ArrowUp) => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(1);
                        self.dirty = true;
                        DialogResult::None
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        let max = self.total_preview_lines.saturating_sub(VISIBLE_LINES);
                        if self.scroll_offset < max {
                            self.scroll_offset += 1;
                        }
                        self.dirty = true;
                        DialogResult::None
                    }
                    Key::Named(NamedKey::PageUp) => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(VISIBLE_LINES);
                        self.dirty = true;
                        DialogResult::None
                    }
                    Key::Named(NamedKey::PageDown) => {
                        let max = self.total_preview_lines.saturating_sub(VISIBLE_LINES);
                        self.scroll_offset = (self.scroll_offset + VISIBLE_LINES).min(max);
                        self.dirty = true;
                        DialogResult::None
                    }

                    _ => DialogResult::None,
                }
            }

            // Mouse wheel -> scroll preview
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => *y as isize,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        if pos.y > 0.0 {
                            -1
                        } else if pos.y < 0.0 {
                            1
                        } else {
                            0
                        }
                    }
                };
                if lines != 0 {
                    let max = self.total_preview_lines.saturating_sub(VISIBLE_LINES) as isize;
                    let new = (self.scroll_offset as isize - lines).clamp(0, max.max(0));
                    if new != self.scroll_offset as isize {
                        self.scroll_offset = new as usize;
                        self.dirty = true;
                    }
                }
                DialogResult::None
            }

            // Mouse button click -> detect button hit
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let button_result = match self.focused_button {
                    DialogButton::Paste => DialogResult::Confirmed,
                    DialogButton::Cancel => DialogResult::Cancelled,
                };
                return button_result;
            }

            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                let btn_y = DIALOG_HEIGHT as f32 - 50.0;
                let paste_x = DIALOG_WIDTH as f32 / 2.0 - BUTTON_WIDTH - 20.0;
                let cancel_x = DIALOG_WIDTH as f32 / 2.0 + 20.0;

                if y >= btn_y && y <= btn_y + BUTTON_HEIGHT {
                    if x >= paste_x && x <= paste_x + BUTTON_WIDTH {
                        if self.focused_button != DialogButton::Paste {
                            self.focused_button = DialogButton::Paste;
                            self.dirty = true;
                        }
                    } else if x >= cancel_x
                        && x <= cancel_x + BUTTON_WIDTH
                        && self.focused_button != DialogButton::Cancel
                    {
                        self.focused_button = DialogButton::Cancel;
                        self.dirty = true;
                    }
                }
                DialogResult::None
            }

            WindowEvent::RedrawRequested => DialogResult::None,
            _ => DialogResult::None,
        };
        if self.dirty {
            self.window.request_redraw();
        }
        result
    }

    pub(crate) fn prepare(
        &mut self,
        gpu: &GpuContext,
        metrics: &TextMetrics,
        glyph: impl Fn(char) -> Option<AtlasGlyph>,
        _text_pipeline: &wgpu::RenderPipeline,
        _text_bind_group: &wgpu::BindGroup,
    ) {
        if !self.dirty && self.preview_initialized {
            return;
        }
        self.dirty = false;

        let surf_w = DIALOG_WIDTH as f32;
        let surf_h = DIALOG_HEIGHT as f32;
        let bg = background_color();
        let cell_w = metrics.cell_width;
        let line_h = metrics.line_height;
        let ascent = metrics.ascent;

        // ── Lazy preview initialization ─────────────────────────────────
        let preview_area_left = TEXT_PADDING;
        let preview_area_right = surf_w - TEXT_PADDING - SCROLLBAR_WIDTH;
        let max_chars = ((preview_area_right - preview_area_left) / cell_w).max(1.0) as usize;

        if !self.preview_initialized {
            self.wrapped_lines = wrap_text_lines(&self.raw_text, max_chars);
            self.total_preview_lines = self.wrapped_lines.len();
            // Clamp scroll offset after initialization.
            let max = self.total_preview_lines.saturating_sub(VISIBLE_LINES);
            if self.scroll_offset > max {
                self.scroll_offset = max;
            }
            self.preview_initialized = true;
        }

        // ── Build background quads ──────────────────────────────────────
        let mut bg_verts: Vec<ColoredVertex> = Vec::new();
        // Full background
        bg_verts.extend_from_slice(&colored_quad(0.0, 0.0, surf_w, surf_h, bg, surf_w, surf_h));

        // Header bar
        let header_color = [0.12, 0.12, 0.12, 1.0];
        bg_verts.extend_from_slice(&colored_quad(
            0.0,
            0.0,
            surf_w,
            HEADER_HEIGHT,
            header_color,
            surf_w,
            surf_h,
        ));

        // Button area background
        let btn_y = surf_h - 50.0;
        let paste_x = surf_w / 2.0 - BUTTON_WIDTH - 20.0;
        let cancel_x = surf_w / 2.0 + 20.0;

        let paste_color = if self.focused_button == DialogButton::Paste {
            [0.15, 0.45, 0.15, 1.0]
        } else {
            [0.15, 0.15, 0.15, 1.0]
        };
        let cancel_color = if self.focused_button == DialogButton::Cancel {
            [0.45, 0.15, 0.15, 1.0]
        } else {
            [0.15, 0.15, 0.15, 1.0]
        };

        bg_verts.extend_from_slice(&colored_quad(
            paste_x,
            btn_y,
            paste_x + BUTTON_WIDTH,
            btn_y + BUTTON_HEIGHT,
            paste_color,
            surf_w,
            surf_h,
        ));
        bg_verts.extend_from_slice(&colored_quad(
            cancel_x,
            btn_y,
            cancel_x + BUTTON_WIDTH,
            btn_y + BUTTON_HEIGHT,
            cancel_color,
            surf_w,
            surf_h,
        ));

        // ── Scrollbar ───────────────────────────────────────────────────
        let preview_top = HEADER_HEIGHT + PREVIEW_TOP_GAP;
        let preview_area_h = btn_y - PREVIEW_BOTTOM_GAP - preview_top;
        let scrollbar_x = preview_area_right;
        let scrollbar_bg_color = [0.08, 0.08, 0.08, 1.0];
        let scrollbar_thumb_color = [0.25, 0.25, 0.25, 1.0];

        // Scrollbar track
        bg_verts.extend_from_slice(&colored_quad(
            scrollbar_x,
            preview_top,
            scrollbar_x + SCROLLBAR_WIDTH,
            preview_top + preview_area_h,
            scrollbar_bg_color,
            surf_w,
            surf_h,
        ));

        // Scrollbar thumb
        if self.total_preview_lines > VISIBLE_LINES {
            let total = self.total_preview_lines.max(1) as f32;
            let visible = VISIBLE_LINES as f32;
            let thumb_h = (visible / total) * preview_area_h;
            let thumb_h = thumb_h.max(16.0); // minimum thumb height
            let max_scroll = self.total_preview_lines.saturating_sub(VISIBLE_LINES) as f32;
            let scroll_frac = if max_scroll > 0.0 {
                self.scroll_offset as f32 / max_scroll
            } else {
                0.0
            };
            let thumb_y = preview_top + scroll_frac * (preview_area_h - thumb_h);
            bg_verts.extend_from_slice(&colored_quad(
                scrollbar_x,
                thumb_y,
                scrollbar_x + SCROLLBAR_WIDTH,
                thumb_y + thumb_h,
                scrollbar_thumb_color,
                surf_w,
                surf_h,
            ));
        }

        self.bg_vertex_count = bg_verts.len() as u32;
        gpu.queue()
            .write_buffer(&self.bg_vertex_buffer, 0, bytemuck::cast_slice(&bg_verts));

        // ── Build text quads ────────────────────────────────────────────
        let fg_color = [1.0, 1.0, 1.0, 1.0];
        let mut text_verts: Vec<harbor_render::gpu::TexturedVertex> = Vec::new();

        // Header: "Paste N lines?"
        let header_text = format!("Paste {} lines?", self.logical_line_count);
        let header_y = TEXT_PADDING + ascent.ceil();
        build_text_quads(
            &header_text,
            preview_area_left,
            header_y,
            cell_w,
            line_h,
            fg_color,
            surf_w,
            surf_h,
            &glyph,
            &mut text_verts,
        );

        // Preview lines: only render visible subset; call safe_preview_line lazily.
        let visible_end = (self.scroll_offset + VISIBLE_LINES).min(self.total_preview_lines);
        let mut line_y = preview_top + ascent.ceil();
        for i in self.scroll_offset..visible_end {
            if let Some(raw_line) = self.wrapped_lines.get(i) {
                let escaped = safe_preview_line(raw_line);
                build_text_quads(
                    &escaped,
                    preview_area_left,
                    line_y,
                    cell_w,
                    line_h,
                    fg_color,
                    surf_w,
                    surf_h,
                    &glyph,
                    &mut text_verts,
                );
            }
            line_y += line_h;
        }

        // Button labels
        let label_y = btn_y + ascent.ceil() + 4.0;
        let paste_label = if self.focused_button == DialogButton::Paste {
            "[ Paste ]"
        } else {
            "  Paste  "
        };
        let cancel_label = if self.focused_button == DialogButton::Cancel {
            "[ Cancel ]"
        } else {
            "  Cancel  "
        };
        let paste_label_w = paste_label.chars().count() as f32 * cell_w;
        let cancel_label_w = cancel_label.chars().count() as f32 * cell_w;
        build_text_quads(
            paste_label,
            paste_x + (BUTTON_WIDTH - paste_label_w) / 2.0,
            label_y,
            cell_w,
            line_h,
            fg_color,
            surf_w,
            surf_h,
            &glyph,
            &mut text_verts,
        );
        build_text_quads(
            cancel_label,
            cancel_x + (BUTTON_WIDTH - cancel_label_w) / 2.0,
            label_y,
            cell_w,
            line_h,
            fg_color,
            surf_w,
            surf_h,
            &glyph,
            &mut text_verts,
        );

        self.text_vertex_count = text_verts.len() as u32;
        gpu.queue().write_buffer(
            &self.text_vertex_buffer,
            0,
            bytemuck::cast_slice(&text_verts),
        );
    }

    pub(crate) fn render(
        &self,
        gpu: &GpuContext,
        text_pipeline: &wgpu::RenderPipeline,
        text_bind_group: &wgpu::BindGroup,
    ) {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(o) => o,
            _ => return,
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("paste dialog"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("paste dialog pass"),
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

            if self.bg_vertex_count > 0 {
                pass.set_pipeline(&self.bg_pipeline);
                pass.set_vertex_buffer(0, self.bg_vertex_buffer.slice(..));
                pass.draw(0..self.bg_vertex_count, 0..1);
            }

            if self.text_vertex_count > 0 {
                pass.set_pipeline(text_pipeline);
                pass.set_vertex_buffer(0, self.text_vertex_buffer.slice(..));
                pass.set_bind_group(0, text_bind_group, &[]);
                pass.draw(0..self.text_vertex_count, 0..1);
            }
        }

        gpu.queue().submit(Some(encoder.finish()));
        gpu.queue().present(output);
    }

    #[allow(dead_code)]
    pub(crate) fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn colored_quad(
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    color: [f32; 4],
    surf_w: f32,
    surf_h: f32,
) -> [ColoredVertex; 6] {
    ColoredVertex::from_pixel_rect(left, top, right, bottom, color, surf_w, surf_h)
}

fn build_text_quads(
    text: &str,
    start_x: f32,
    baseline_y: f32,
    cell_w: f32,
    _line_h: f32,
    color: [f32; 4],
    surf_w: f32,
    surf_h: f32,
    glyph: &impl Fn(char) -> Option<AtlasGlyph>,
    verts: &mut Vec<harbor_render::gpu::TexturedVertex>,
) {
    let mut pen_x = start_x;
    for ch in text.chars() {
        if let Some(g) = glyph(ch)
            && g.width > 0
            && g.height > 0
        {
            let glyph_left = pen_x + g.xmin as f32;
            let glyph_bottom = baseline_y - g.ymin as f32;
            let glyph_top = glyph_bottom - g.height as f32;
            let glyph_right = glyph_left + g.width as f32;
            verts.extend_from_slice(&harbor_render::gpu::TexturedVertex::from_pixel_rect(
                glyph_left,
                glyph_top,
                glyph_right,
                glyph_bottom,
                g.uv.left,
                g.uv.top,
                g.uv.right,
                g.uv.bottom,
                color,
                surf_w,
                surf_h,
            ));
        }
        pen_x += cell_w;
    }
}

/// Split raw text into logical lines, then wrap each line to fit `max_chars` columns.
/// Returns all wrapped segments as owned strings (without C0 escaping — that's
/// applied lazily by the renderer via `safe_preview_line`).
fn wrap_text_lines(raw_text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut result = Vec::new();
    let mut current_line = String::with_capacity(max_chars + 8);

    for ch in raw_text.chars() {
        if ch == '\n' {
            result.push(std::mem::take(&mut current_line));
            current_line.clear();
        } else {
            // CJK characters count as 2 for width purposes (unicode-width).
            // Use unicode-width for accurate measurement.
            let char_width = unicode_width::UnicodeWidthChar::width(ch)
                .unwrap_or(1)
                .max(1);
            if !current_line.is_empty() {
                let current_width: usize = current_line
                    .chars()
                    .map(|c| {
                        unicode_width::UnicodeWidthChar::width(c)
                            .unwrap_or(1)
                            .max(1)
                    })
                    .sum();
                if current_width + char_width > max_chars {
                    result.push(std::mem::take(&mut current_line));
                    current_line.clear();
                }
            }
            current_line.push(ch);
        }
    }
    // Don't forget the last line (even if empty).
    result.push(current_line);
    result
}
