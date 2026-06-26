use std::sync::Arc;

use anyhow::{Context as _, Result};
use winit::window::Window;

use crate::{
    app::TerminalEvent,
    render::Render,
    terminal::{LockedTerminal, Terminal},
    text::TextRenderer,
    pty::Pty,
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
    pub(crate) terminal: LockedTerminal,
    pty: Pty,
}

impl Renderer {
    /// Creates the GPU surface, device, and text renderer for the window.
    pub(crate) async fn new(
        window: Arc<Window>,
        proxy: winit::event_loop::EventLoopProxy<TerminalEvent>,
    ) -> Result<Self> {
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

        let temp_terminal = Terminal::new(24, 80);
        let mut text = TextRenderer::new(
            &device,
            &queue,
            config.format,
            &temp_terminal,
            config.width,
            config.height,
        )?;

        let (cell_width, cell_height) = text.cell_size();
        let padding_width = 32.0;
        let padding_height = 32.0;
        let cols = (((size.width as f32 - padding_width) / cell_width) as usize).max(1);
        let rows = (((size.height as f32 - padding_height) / cell_height) as usize).max(1);

        let terminal = Arc::new(parking_lot::Mutex::new(Terminal::new(rows, cols)));
        text.update(&device, &queue, &terminal.lock(), config.width, config.height);

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let (pty, child) = Pty::spawn(&shell, rows as u16, cols as u16)
            .context("spawn pty failed")?;

        let mut reader = pty.try_clone_reader().context("clone pty reader failed")?;

        let terminal_clone = terminal.clone();
        let proxy_clone = proxy.clone();

        std::thread::spawn(move || {
            use std::io::Read as _;
            let mut buf = [0u8; 4096];
            let mut leftover = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut data = if leftover.is_empty() {
                            buf[..n].to_vec()
                        } else {
                            let mut temp = std::mem::take(&mut leftover);
                            temp.extend_from_slice(&buf[..n]);
                            temp
                        };

                        let len = data.len();
                        let mut truncate_at = len;

                        for i in 1..=4 {
                            if len >= i {
                                let idx = len - i;
                                let byte = data[idx];
                                if (byte & 0xE0) == 0xC0 {
                                    if i < 2 { truncate_at = idx; }
                                    break;
                                } else if (byte & 0xF0) == 0xE0 {
                                    if i < 3 { truncate_at = idx; }
                                    break;
                                } else if (byte & 0xF8) == 0xF0 {
                                    if i < 4 { truncate_at = idx; }
                                    break;
                                }
                            }
                        }

                        if truncate_at < len {
                            leftover = data.split_off(truncate_at);
                        }

                        let text = String::from_utf8_lossy(&data);
                        {
                            let mut term = terminal_clone.lock();
                            term.put_str(&text);
                        }
                        let _ = proxy_clone.send_event(TerminalEvent::Redraw);
                    }
                    Err(_) => break,
                }
            }

            let mut child = child;
            let status_msg = match child.wait() {
                Ok(status) => {
                    if status.success() {
                        "Process completed successfully".to_string()
                    } else {
                        format!("Process exited with status: {:?}", status)
                    }
                }
                Err(e) => format!("Process exited (failed to retrieve status: {})", e),
            };

            {
                let mut term = terminal_clone.lock();
                term.put_str(&format!("\n\r[{}]\n\r", status_msg));
                term.cursor_visible = false;
            }
            let _ = proxy_clone.send_event(TerminalEvent::Redraw);
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            text,
            terminal,
            pty,
        })
    }

    /// Reconfigures the surface and size-dependent resources after a window resize.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            tracing::trace!("ignored zero-sized resize");
            return;
        }

        self.config.width = width;
        self.config.height = height;
        tracing::trace!(width, height, "renderer resized");
        self.surface.configure(&self.device, &self.config);

        let (cell_width, cell_height) = self.text.cell_size();
        let cols = (((width as f32 - 32.0) / cell_width) as usize).max(1);
        let rows = (((height as f32 - 32.0) / cell_height) as usize).max(1);

        {
            let mut terminal = self.terminal.lock();
            terminal.resize(rows, cols);
        }

        let _ = self.pty.resize(rows as u16, cols as u16);

        {
            let terminal = self.terminal.lock();
            self.text
                .update(&self.device, &self.queue, &terminal, width, height);
        }
    }

    pub(crate) fn write_to_pty(&mut self, data: &[u8]) -> Result<()> {
        self.pty.write_all(data).context("write to pty")?;
        self.pty.flush().context("flush pty")?;
        Ok(())
    }
}

impl Render for Renderer {
    /// Acquires the current surface texture, clears it, draws text, and presents it.
    fn render(&mut self, (): ()) {
        // Lock terminal and update text renderer vertices to show latest terminal state
        {
            let terminal = self.terminal.lock();
            self.text.update(&self.device, &self.queue, &terminal, self.config.width, self.config.height);
        }

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
