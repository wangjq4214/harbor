//! Confirmation dialog for multi-line paste when bracketed paste is OFF.

use std::sync::Arc;

use harbor_gpu::GpuContext;
use harbor_terminal::safe_preview_line;
use harbor_ui::{
    BoxConstraints, Button, Dialog, Key as UiKey, PaintContext, ScrollView, Text as UiText,
    TextResources, WidgetEventResult, WidgetRuntime, WindowSpec,
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

type PasteDialogWidget = Dialog<PasteDialogAction, ScrollView<UiText>>;

fn preview_text(raw_text: &str) -> String {
    raw_text
        .split('\n')
        .map(safe_preview_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn paste_dialog_spec(raw_text: &str) -> PasteDialogWidget {
    Dialog::new(
        WindowSpec::fixed("Paste confirmation", 600.0, 400.0),
        ScrollView::new(UiText::new(preview_text(raw_text)).wrap()),
    )
    .title(UiText::new(format!("Paste {} lines?", raw_text.lines().count())))
    .actions(vec![
        Button::new(UiText::new("Paste"), PasteDialogAction::Paste).key(PASTE_ACTION_KEY),
        Button::new(UiText::new("Cancel"), PasteDialogAction::Cancel).key(CANCEL_ACTION_KEY),
    ])
    .initial_focus(CANCEL_ACTION_KEY)
}

/// A secondary native window whose contents are rendered by the UI widget runtime.
pub(crate) struct PasteDialog {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    #[allow(dead_code)]
    surface_config: wgpu::SurfaceConfiguration,
    /// Raw (unmodified) text to send to the PTY on confirm.
    pub(crate) raw_text: String,
    dialog: PasteDialogWidget,
    runtime: WidgetRuntime<PasteDialogWidget, PasteDialogAction>,
}

impl PasteDialog {
    pub(crate) fn new(
        raw_text: String,
        event_loop: &ActiveEventLoop,
        gpu: &GpuContext,
        main_window: Option<&Window>,
    ) -> Self {
        let dialog = paste_dialog_spec(&raw_text);
        let width = dialog.window.preferred_width;
        let height = dialog.window.preferred_height;

        let mut window_attrs = Window::default_attributes()
            .with_title(&dialog.window.title)
            .with_inner_size(winit::dpi::LogicalSize::new(width, height))
            .with_resizable(dialog.window.resizable);

        #[cfg(target_os = "windows")]
        if let Some(main) = main_window {
            let hwnd = main.window_handle().ok().and_then(|handle| match handle.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            });
            if let Some(hwnd) = hwnd {
                // SAFETY: HWND from raw-window-handle is the Win32 window handle.
                window_attrs = window_attrs.with_owner_window(hwnd);
            }
        }

        if let Some(main) = main_window
            && let Ok(outer) = main.outer_position()
        {
            let main_size = main.inner_size();
            let dialog_x = outer.x as f64 + (main_size.width as f64 - width as f64) / 2.0;
            let dialog_y = outer.y as f64 + (main_size.height as f64 - height as f64) / 2.0;
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
            .find(|format| *format == gpu.format())
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

        let mut runtime = WidgetRuntime::new(&dialog);
        runtime.layout(&dialog, BoxConstraints::tight(width, height));
        Self {
            window,
            surface,
            surface_config,
            raw_text,
            dialog,
            runtime,
        }
    }

    pub(crate) fn window_id(&self) -> winit::window::WindowId {
        self.window.id()
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
            WindowEvent::KeyboardInput { event: key_event, .. }
                if key_event.state == ElementState::Pressed =>
            {
                match &key_event.logical_key {
                    Key::Named(NamedKey::Escape) => PasteDialogResult::Cancelled,
                    Key::Character(ch) if ch == "y" || ch == "Y" => PasteDialogResult::Confirmed,
                    Key::Character(ch) if ch == "n" || ch == "N" => PasteDialogResult::Cancelled,
                    _ => match self.runtime.event(
                        &self.dialog,
                        event,
                        harbor_ui::Rect {
                            x: 0.0,
                            y: 0.0,
                            width: self.dialog.window.preferred_width,
                            height: self.dialog.window.preferred_height,
                        },
                    ) {
                        WidgetEventResult::Intent(action) => Self::result(action),
                        WidgetEventResult::Ignored | WidgetEventResult::Handled => {
                            PasteDialogResult::None
                        }
                    },
                }
            }
            _ => match self.runtime.event(
                &self.dialog,
                event,
                harbor_ui::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: self.dialog.window.preferred_width,
                    height: self.dialog.window.preferred_height,
                },
            ) {
                WidgetEventResult::Intent(action) => Self::result(action),
                WidgetEventResult::Ignored | WidgetEventResult::Handled => PasteDialogResult::None,
            },
        };

        if matches!(
            event,
            WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::CursorMoved { .. }
        ) {
            self.window.request_redraw();
        }
        result
    }

    pub(crate) fn render(&mut self, gpu: &GpuContext, text: &mut TextResources) {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
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
        let bounds = self.runtime.layout(
            &self.dialog,
            BoxConstraints::tight(
                self.dialog.window.preferred_width,
                self.dialog.window.preferred_height,
            ),
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("paste dialog pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.runtime.paint(
                &self.dialog,
                PaintContext { gpu, text, bounds },
                &mut pass,
            );
        }
        gpu.queue().submit(Some(encoder.finish()));
        gpu.queue().present(output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_spec_uses_widget_actions_and_cancel_focus() {
        let dialog = paste_dialog_spec("one\ntwo\nthree");
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
    }

    #[test]
    fn preview_escapes_control_characters_before_rendering() {
        assert_eq!(preview_text("one\u{1b}[31m\ntwo"), "one␛[31m\ntwo");
    }
}
