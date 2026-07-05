use std::sync::Arc;

use anyhow::Result;
use winit::window::Window;

use crate::{
    background::BackgroundLayer,
    config::BACKGROUND,
    cursor::CursorLayer,
    decoration::DecorationLayer,
    font::load_system_fonts,
    gpu::GpuContext,
    metrics::TextMetrics,
    terminal::{Screen, TerminalSize, TextLayer},
};

pub(crate) struct Renderer {
    /// Shared GPU handles (surface / device / queue / config).
    gpu: GpuContext,
    /// Solid-color background rectangles behind each non-default cell.
    background_layer: BackgroundLayer,
    /// Text rendering layer (glyph atlas + pipeline).
    text_layer: TextLayer,
    /// Cursor rendering layer (blinking block cursor).
    cursor_layer: CursorLayer,
    /// Decoration layer (underline, strikethrough).
    decoration_layer: DecorationLayer,
}
impl Renderer {
    /// Creates the GPU context, loads fonts, and initialises text and cursor layers.
    ///
    /// `screen` provides the initial terminal grid for the first glyph-atlas build.
    /// The caller bootstraps with a 1×1 `Terminal`, calls `terminal_size()` to
    /// compute the real grid dimensions, resizes the terminal, then calls
    /// `prepare_layers(screen)`.
    pub(crate) async fn new(window: Arc<Window>, screen: &Screen) -> Result<Self> {
        let gpu = GpuContext::new(window).await?;
        let fonts = load_system_fonts()?;
        let metrics = TextMetrics::new(&fonts);
        let text_layer = TextLayer::new(&gpu, fonts, metrics, screen)?;

        let cursor_layer = CursorLayer::new(&gpu, metrics);
        let background_layer =
            BackgroundLayer::new(&gpu, screen, metrics.cell_width, metrics.line_height);
        let decoration_layer = DecorationLayer::new(&gpu, screen, metrics);

        tracing::info!(
            rows = text_layer.terminal_size(&gpu).rows,
            cols = text_layer.terminal_size(&gpu).cols,
            "renderer ready"
        );

        Ok(Self {
            gpu,
            background_layer,
            text_layer,
            cursor_layer,
            decoration_layer,
        })
    }

    /// Reconfigures the surface on window resize, marks cursor layer dirty,
    /// and returns the new terminal grid size.
    /// Callers compare against the current terminal dimensions and resize
    /// terminal/pty as needed.
    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Option<TerminalSize> {
        if width == 0 || height == 0 {
            tracing::trace!("ignored zero-sized resize");
            return None;
        }
        self.gpu.resize(width, height);
        self.background_layer.mark_dirty();
        self.text_layer.mark_dirty();
        self.cursor_layer.mark_dirty();
        self.decoration_layer.mark_dirty();
        Some(self.text_layer.terminal_size(&self.gpu))
    }

    /// Terminal grid dimensions for the current surface size.
    pub(crate) fn terminal_size(&self) -> TerminalSize {
        self.text_layer.terminal_size(&self.gpu)
    }

    /// Refreshes GPU resources for text and cursor layers (called after
    /// terminal content changes).
    pub(crate) fn prepare_layers(&mut self, screen: &Screen) {
        use crate::render::Layer;
        tracing::trace!(
            width = self.gpu.surface_size().0,
            height = self.gpu.surface_size().1,
            rows = screen.rows(),
            cols = screen.cols(),
            "preparing layers"
        );
        self.background_layer.prepare(&self.gpu, Some(screen));
        self.text_layer.prepare(&self.gpu, Some(screen));
        self.cursor_layer.prepare(&self.gpu, Some(screen));
        self.decoration_layer.prepare(&self.gpu, Some(screen));
    }

    /// Sets cursor visibility.  Only updates the flag, does not trigger GPU upload.
    pub(crate) fn set_cursor_visible(&mut self, visible: bool, screen: &Screen) {
        self.cursor_layer.set_visible(visible, screen);
    }

    /// Uploads cursor vertices when position/visibility changed.
    /// Called before `render()` when only the cursor blink toggles (no terminal output).
    pub(crate) fn prepare_cursor(&mut self, screen: &Screen) {
        use crate::render::Layer;
        self.cursor_layer.prepare(&self.gpu, Some(screen));
    }

    /// Acquires the surface texture, clears it, draws text and cursor layers,
    /// submits commands, and presents the frame.
    pub(crate) fn render(&mut self) {
        use crate::render::Layer;

        let output = match self.gpu.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
                tracing::warn!("surface texture suboptimal");
                output
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                tracing::warn!("surface lost; reconfiguring");
                let (w, h) = self.gpu.surface_size();
                self.gpu.resize(w, h);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                tracing::warn!("surface outdated; reconfiguring");
                let (w, h) = self.gpu.surface_size();
                self.gpu.resize(w, h);
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
        let mut encoder = self
            .gpu
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
                        load: wgpu::LoadOp::Clear(BACKGROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.background_layer.draw(&mut render_pass);
            self.text_layer.draw(&mut render_pass);
            self.cursor_layer.draw(&mut render_pass);
            self.decoration_layer.draw(&mut render_pass);
        }

        self.gpu.queue().submit(Some(encoder.finish()));
        self.gpu.present(output);
    }
}
