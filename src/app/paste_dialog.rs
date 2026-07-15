//! Confirmation dialog for multi-line paste when bracketed paste is OFF.

use std::sync::Arc;

use harbor_gpu::gpu::ColoredVertex;
use harbor_gpu::{AtlasGlyph, GpuContext, TextMetrics};
use harbor_terminal::safe_preview_line;
use harbor_ui::{
    Button, ButtonState, Dialog, DialogRuntime, Key as UiKey, Rect, Text as UiText, WindowSpec,
};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PasteDialogAction {
    Paste,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteDialogResult {
    None,
    Confirmed,
    Cancelled,
}

const PASTE_ACTION_KEY: UiKey = UiKey(1);
const CANCEL_ACTION_KEY: UiKey = UiKey(2);
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

fn paste_dialog_spec(line_count: usize) -> Dialog<PasteDialogAction, ()> {
    Dialog::new(WindowSpec::fixed("Paste confirmation", 600.0, 400.0), ())
        .title(UiText::new(format!("Paste {line_count} lines?")))
        .actions(vec![
            Button::new(UiText::new("Paste"), PasteDialogAction::Paste).key(PASTE_ACTION_KEY),
            Button::new(UiText::new("Cancel"), PasteDialogAction::Cancel).key(CANCEL_ACTION_KEY),
        ])
        .initial_focus(CANCEL_ACTION_KEY)
}

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
    dialog: Dialog<PasteDialogAction, ()>,
    runtime: DialogRuntime<PasteDialogAction>,
    bg_pipeline: Arc<wgpu::RenderPipeline>,
    bg_vertex_buffer: wgpu::Buffer,
    bg_vertex_count: u32,
    text_vertex_buffer: wgpu::Buffer,
    text_vertex_count: u32,
    dirty: bool,
    /// All wrapped preview lines (split by \n, each wrapped to fit).
    /// Populated lazily on first `prepare()` after metrics are known.
    wrapped_lines: Vec<String>,
    /// Total number of wrapped lines (for scrollbar range).
    total_preview_lines: usize,
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
        let dialog = paste_dialog_spec(raw_text.lines().count());
        let mut runtime = DialogRuntime::default();
        runtime.sync(&dialog);
        let width = dialog.window.preferred_width;
        let height = dialog.window.preferred_height;

        let mut window_attrs = Window::default_attributes()
            .with_title(&dialog.window.title)
            .with_inner_size(winit::dpi::LogicalSize::new(width, height))
            .with_resizable(dialog.window.resizable);

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
            let dialog_x = main_x + (main_w - width as f64) / 2.0;
            let dialog_y = main_y + (main_h - height as f64) / 2.0;
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
            width: width as u32,
            height: height as u32,
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
            size: (8192 * std::mem::size_of::<harbor_gpu::gpu::TexturedVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            window,
            surface,
            surface_config,
            raw_text,
            dialog,
            runtime,
            bg_pipeline: gpu.colored_quad_pipeline(),
            bg_vertex_buffer,
            bg_vertex_count: 0,
            text_vertex_buffer,
            text_vertex_count: 0,
            dirty: true,
            wrapped_lines: Vec::new(),
            total_preview_lines: 0,
            preview_initialized: false,
        }
    }

    pub(crate) fn window_id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    fn action_bounds(&self) -> [Rect; 2] {
        let width = self.dialog.window.preferred_width;
        let button_y = self.dialog.window.preferred_height - 50.0;
        [
            Rect {
                x: width / 2.0 - BUTTON_WIDTH - 20.0,
                y: button_y,
                width: BUTTON_WIDTH,
                height: BUTTON_HEIGHT,
            },
            Rect {
                x: width / 2.0 + 20.0,
                y: button_y,
                width: BUTTON_WIDTH,
                height: BUTTON_HEIGHT,
            },
        ]
    }

    fn result(action: PasteDialogAction) -> PasteDialogResult {
        match action {
            PasteDialogAction::Paste => PasteDialogResult::Confirmed,
            PasteDialogAction::Cancel => PasteDialogResult::Cancelled,
        }
    }

    pub(crate) fn handle_event(&mut self, event: &WindowEvent) -> PasteDialogResult {
        let result = match event {
            WindowEvent::CloseRequested => PasteDialogResult::Cancelled,
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } if key_event.state == ElementState::Pressed => match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => PasteDialogResult::Cancelled,
                Key::Character(ch) if ch == "y" || ch == "Y" => PasteDialogResult::Confirmed,
                Key::Character(ch) if ch == "n" || ch == "N" => PasteDialogResult::Cancelled,
                _ => self
                    .runtime
                    .handle_key(&self.dialog, event)
                    .copied()
                    .map_or(PasteDialogResult::None, Self::result),
            },
            _ => {
                let action_bounds = self.action_bounds();
                self.runtime
                    .handle_pointer(&self.dialog, event, &action_bounds)
                    .copied()
                    .map_or(PasteDialogResult::None, Self::result)
            }
        };

        if matches!(
            event,
            WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::CursorMoved { .. }
        ) {
            self.runtime.scroll_offset = self
                .runtime
                .scroll_offset
                .min(self.total_preview_lines.saturating_sub(VISIBLE_LINES));
            self.dirty = true;
        }
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

        let surf_w = self.dialog.window.preferred_width;
        let surf_h = self.dialog.window.preferred_height;
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
            self.runtime.scroll_offset = self
                .runtime
                .scroll_offset
                .min(self.total_preview_lines.saturating_sub(VISIBLE_LINES));
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
        let [paste_bounds, cancel_bounds] = self.action_bounds();
        let paste_state = self.runtime.focused_state(&self.dialog, 0);
        let cancel_state = self.runtime.focused_state(&self.dialog, 1);
        let paste_color = match paste_state {
            ButtonState::Focused | ButtonState::Pressed => [0.15, 0.45, 0.15, 1.0],
            ButtonState::Normal | ButtonState::Hover | ButtonState::Disabled => {
                [0.15, 0.15, 0.15, 1.0]
            }
        };
        let cancel_color = match cancel_state {
            ButtonState::Focused | ButtonState::Pressed => [0.45, 0.15, 0.15, 1.0],
            ButtonState::Normal | ButtonState::Hover | ButtonState::Disabled => {
                [0.15, 0.15, 0.15, 1.0]
            }
        };

        bg_verts.extend_from_slice(&colored_quad(
            paste_bounds.x,
            paste_bounds.y,
            paste_bounds.x + paste_bounds.width,
            paste_bounds.y + paste_bounds.height,
            paste_color,
            surf_w,
            surf_h,
        ));
        bg_verts.extend_from_slice(&colored_quad(
            cancel_bounds.x,
            cancel_bounds.y,
            cancel_bounds.x + cancel_bounds.width,
            cancel_bounds.y + cancel_bounds.height,
            cancel_color,
            surf_w,
            surf_h,
        ));

        // ── Scrollbar ───────────────────────────────────────────────────
        let preview_top = HEADER_HEIGHT + PREVIEW_TOP_GAP;
        let preview_area_h = paste_bounds.y - PREVIEW_BOTTOM_GAP - preview_top;
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
                self.runtime.scroll_offset as f32 / max_scroll
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
        let mut text_verts: Vec<harbor_gpu::gpu::TexturedVertex> = Vec::new();

        // Header: "Paste N lines?"
        let header_text = &self
            .dialog
            .title
            .as_ref()
            .expect("paste dialog title")
            .content;
        let header_y = TEXT_PADDING + ascent.ceil();
        build_text_quads(
            header_text,
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
        let visible_end =
            (self.runtime.scroll_offset + VISIBLE_LINES).min(self.total_preview_lines);
        let mut line_y = preview_top + ascent.ceil();
        for i in self.runtime.scroll_offset..visible_end {
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
        let label_y = paste_bounds.y + ascent.ceil() + 4.0;
        let paste_label = if matches!(paste_state, ButtonState::Focused | ButtonState::Pressed) {
            format!("[ {} ]", self.dialog.actions[0].child.content)
        } else {
            format!("  {}  ", self.dialog.actions[0].child.content)
        };
        let cancel_label = if matches!(cancel_state, ButtonState::Focused | ButtonState::Pressed) {
            format!("[ {} ]", self.dialog.actions[1].child.content)
        } else {
            format!("  {}  ", self.dialog.actions[1].child.content)
        };
        let paste_label_w = paste_label.chars().count() as f32 * cell_w;
        let cancel_label_w = cancel_label.chars().count() as f32 * cell_w;
        build_text_quads(
            &paste_label,
            paste_bounds.x + (paste_bounds.width - paste_label_w) / 2.0,
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
            &cancel_label,
            cancel_bounds.x + (cancel_bounds.width - cancel_label_w) / 2.0,
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
    verts: &mut Vec<harbor_gpu::gpu::TexturedVertex>,
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
            verts.extend_from_slice(&harbor_gpu::gpu::TexturedVertex::from_pixel_rect(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_spec_uses_ui_actions_and_cancel_focus() {
        let dialog = paste_dialog_spec(3);
        let mut runtime = DialogRuntime::default();
        runtime.sync(&dialog);

        assert_eq!(
            dialog.window,
            WindowSpec::fixed("Paste confirmation", 600.0, 400.0)
        );
        assert_eq!(
            dialog.title.as_ref().map(|title| title.content.as_str()),
            Some("Paste 3 lines?")
        );
        assert_eq!(dialog.actions[0].child.content, "Paste");
        assert_eq!(dialog.actions[0].intent, PasteDialogAction::Paste);
        assert_eq!(dialog.actions[1].child.content, "Cancel");
        assert_eq!(dialog.actions[1].intent, PasteDialogAction::Cancel);
        assert_eq!(runtime.focused_state(&dialog, 1), ButtonState::Focused);
    }
}
