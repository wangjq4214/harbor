use std::sync::Arc;

use anyhow::{Context as _, Result};
use winit::window::Window;

use crate::{
    render::Render,
    terminal::{Terminal, TerminalSize},
    text::TextRenderer,
};

const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.36,
    g: 0.20,
    b: 0.08,
    a: 1.0,
};

/// Owns the wgpu surface, device queue, and per-frame rendering resources.
pub(crate) struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    text: TextRenderer,
    terminal: Terminal,
}

impl Renderer {
    /// Creates the GPU surface, device, and text renderer for the window.
    pub(crate) async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        tracing::trace!(
            width = size.width,
            height = size.height,
            "creating renderer"
        );
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).context("create surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .context("request adapter")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("request device")?;
        // Prefer an sRGB format so fragment shader colors display as expected.
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        tracing::info!(
            width = config.width,
            height = config.height,
            ?format,
            "renderer configured"
        );

        // Text metrics are needed before the real terminal dimensions are known, so
        // bootstrap with a one-cell grid and immediately resize from the measured font.
        let mut terminal = Terminal::new(1, 1);
        let text = TextRenderer::new(
            &device,
            &queue,
            config.format,
            &terminal,
            config.width,
            config.height,
        )?;
        let terminal_size = text.terminal_size(config.width, config.height);
        terminal.resize(terminal_size.rows, terminal_size.cols);
        let mut renderer = Self {
            surface,
            device,
            queue,
            config,
            text,
            terminal,
        };
        renderer.refresh_text();
        Ok(renderer)
    }

    /// Reconfigures the surface and size-dependent resources after a window resize.
    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Option<TerminalSize> {
        if width == 0 || height == 0 {
            tracing::trace!("ignored zero-sized resize");
            return None;
        }

        self.config.width = width;
        self.config.height = height;
        tracing::trace!(width, height, "renderer resized");
        self.surface.configure(&self.device, &self.config);
        let terminal_size = self.resize_terminal_to_surface();
        self.refresh_text();
        terminal_size
    }

    pub(crate) fn terminal_size(&self) -> TerminalSize {
        TerminalSize {
            rows: self.terminal.rows,
            cols: self.terminal.cols,
        }
    }

    fn resize_terminal_to_surface(&mut self) -> Option<TerminalSize> {
        let size = self
            .text
            .terminal_size(self.config.width, self.config.height);
        if self.terminal.rows == size.rows && self.terminal.cols == size.cols {
            return None;
        }

        self.terminal.resize(size.rows, size.cols);
        Some(size)
    }

    fn refresh_text(&mut self) {
        self.text.update(
            &self.device,
            &self.queue,
            &self.terminal,
            self.config.width,
            self.config.height,
        );
    }

    /// Appends pty output to the terminal grid and refreshes text draw resources.
    pub(crate) fn write_terminal_output(&mut self, output: &[u8]) {
        if output.is_empty() {
            return;
        }

        let text = String::from_utf8_lossy(output);
        self.terminal.put_str(&text);
        self.refresh_text();
    }
}

impl Render for Renderer {
    /// Acquires the current surface texture, clears it, draws text, and presents it.
    fn render(&mut self, (): ()) {
        // Surface state changes during minimize, resize, or driver events; draw only with a valid texture.
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
                tracing::warn!("surface texture suboptimal");
                output
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                tracing::warn!("surface lost; reconfiguring");
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                tracing::warn!("surface outdated; reconfiguring");
                self.surface.configure(&self.device, &self.config);
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
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // End the render pass borrow before submitting the command buffer.
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
            self.text.render(&mut render_pass);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
    }
}
