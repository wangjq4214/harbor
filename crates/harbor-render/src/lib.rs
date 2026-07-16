//! Renderer-owned UI frame and paint command contracts.

use anyhow::Result;
use harbor_gpu::{GpuRuntime, GpuSurface};
use harbor_types::{Rect, RgbaColor};
use std::{collections::HashSet, sync::Arc};
use winit::window::Window;

/// Read-only, GPU-handle-free values supplied by a render target during layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderEnvironment {
    logical_width: f32,
    logical_height: f32,
    scale_factor: f64,
}

impl RenderEnvironment {
    pub const fn new(logical_width: f32, logical_height: f32, scale_factor: f64) -> Self {
        Self {
            logical_width,
            logical_height,
            scale_factor,
        }
    }

    pub const fn logical_size(self) -> (f32, f32) {
        (self.logical_width, self.logical_height)
    }

    pub const fn scale_factor(self) -> f64 {
        self.scale_factor
    }
}

/// Stable renderer cache identity supplied by a reconciled Widget.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RenderIdentity(u64);

impl RenderIdentity {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// A positioned glyph emitted by a UI visual projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub character: char,
    pub bounds: Rect,
    pub color: RgbaColor,
}

/// A single ordered render operation emitted through [`PaintContext`].
#[derive(Debug, PartialEq)]
pub enum PaintCommand<'a> {
    FillRect {
        rect: Rect,
        color: RgbaColor,
        clip: Rect,
    },
    Text {
        origin: (f32, f32),
        text: &'a str,
        color: RgbaColor,
        clip: Rect,
    },
    GlyphBatch {
        glyphs: &'a [Glyph],
        clip: Rect,
    },
}

impl PaintCommand<'_> {
    pub const fn clip(&self) -> Rect {
        match self {
            Self::FillRect { clip, .. }
            | Self::Text { clip, .. }
            | Self::GlyphBatch { clip, .. } => *clip,
        }
    }
}

/// Frame-scoped ordered command recorder for one Widget paint scope.
pub struct PaintContext<'a> {
    environment: RenderEnvironment,
    bounds: Rect,
    clip: Rect,
    commands: Vec<PaintCommand<'a>>,
    identities: HashSet<RenderIdentity>,
}

impl<'a> PaintContext<'a> {
    pub fn new(environment: RenderEnvironment, bounds: Rect) -> Self {
        Self {
            environment,
            bounds,
            clip: bounds,
            commands: Vec::new(),
            identities: HashSet::new(),
        }
    }

    pub const fn environment(&self) -> RenderEnvironment {
        self.environment
    }

    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Records a solid rectangle without changing the current paint order.
    pub fn fill_rect(&mut self, rect: Rect, color: RgbaColor) {
        self.commands.push(PaintCommand::FillRect {
            rect,
            color,
            clip: self.clip,
        });
    }

    /// Records a positioned text run.
    pub fn draw_text(&mut self, origin: (f32, f32), text: &'a str, color: RgbaColor) {
        self.commands.push(PaintCommand::Text {
            origin,
            text,
            color,
            clip: self.clip,
        });
    }

    /// Records positioned glyphs without exposing renderer resources.
    pub fn draw_glyph_batch(&mut self, glyphs: &'a [Glyph]) {
        self.commands.push(PaintCommand::GlyphBatch {
            glyphs,
            clip: self.clip,
        });
    }

    /// Runs `paint` with a clip that can only further restrict this context.
    pub fn with_clip(&mut self, clip: Rect, paint: impl FnOnce(&mut Self)) {
        let previous = self.clip;
        self.clip = previous.intersect(clip);
        paint(self);
        self.clip = previous;
    }

    /// Marks commands emitted by `paint` as belonging to one reconciled Widget.
    pub fn with_identity(&mut self, identity: RenderIdentity, paint: impl FnOnce(&mut Self)) {
        self.identities.insert(identity);
        paint(self);
    }

    pub fn visited_identities(&self) -> &HashSet<RenderIdentity> {
        &self.identities
    }

    pub fn finish(self) -> Vec<PaintCommand<'a>> {
        self.commands
    }
}

/// Result of attempting one renderer target frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameOutcome {
    /// The target acquired, cleared, submitted, and presented a frame.
    Presented,
    /// Surface acquisition did not succeed, so no paint callback ran.
    Skipped,
}

/// Renderer-owned owner of one shared GPU runtime.
pub struct UiRenderer {
    runtime: Arc<GpuRuntime>,
}

impl UiRenderer {
    /// Creates a renderer and its first target from the host-owned main window.
    pub async fn new(window: Arc<Window>) -> Result<(Self, RenderTarget)> {
        let scale_factor = window.scale_factor();
        let (runtime, surface) = GpuRuntime::new(window).await?;
        let runtime = Arc::new(runtime);
        let target = RenderTarget::new(Arc::clone(&runtime), surface, scale_factor);
        Ok((Self { runtime }, target))
    }

    /// Attaches a renderer target to another host-owned window.
    pub fn attach_window(&self, window: Arc<Window>) -> Result<RenderTarget> {
        let scale_factor = window.scale_factor();
        let surface = self.runtime.create_surface(window)?;
        Ok(RenderTarget::new(
            Arc::clone(&self.runtime),
            surface,
            scale_factor,
        ))
    }
}

/// Opaque renderer target for one host-owned native window.
pub struct RenderTarget {
    runtime: Arc<GpuRuntime>,
    surface: GpuSurface,
    environment: RenderEnvironment,
    cached_identities: HashSet<RenderIdentity>,
}

impl RenderTarget {
    fn new(runtime: Arc<GpuRuntime>, surface: GpuSurface, scale_factor: f64) -> Self {
        let environment = environment_for(surface.size(), scale_factor);
        Self {
            runtime,
            surface,
            environment,
            cached_identities: HashSet::new(),
        }
    }

    pub const fn environment(&self) -> RenderEnvironment {
        self.environment
    }

    /// Whether this target retained renderer state for `identity` after its last presented frame.
    pub fn has_cached_identity(&self, identity: RenderIdentity) -> bool {
        self.cached_identities.contains(&identity)
    }

    /// Reconfigures this target before the host relays the updated environment to UI layout.
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f64) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface.resize(&self.runtime, width, height);
        self.environment = environment_for((width, height), scale_factor);
    }

    /// Runs one frame. A non-successful surface acquisition skips the paint callback.
    pub fn render<'a>(&mut self, paint: impl FnOnce(&mut PaintContext<'a>)) -> FrameOutcome {
        let output = match self.surface.acquire() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            _ => return FrameOutcome::Skipped,
        };
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: self.environment.logical_width,
            height: self.environment.logical_height,
        };
        let mut context = PaintContext::new(self.environment, bounds);
        paint(&mut context);
        self.cached_identities
            .clone_from(context.visited_identities());
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.runtime
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("harbor render target"),
                });
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("harbor render target clear"),
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
        }));
        self.runtime.queue().submit(Some(encoder.finish()));
        self.runtime.queue().present(output);
        FrameOutcome::Presented
    }
}

fn environment_for((width, height): (u32, u32), scale_factor: f64) -> RenderEnvironment {
    let effective_scale_factor = scale_factor.max(f64::MIN_POSITIVE);
    RenderEnvironment::new(
        width as f32 / effective_scale_factor as f32,
        height as f32 / effective_scale_factor as f32,
        effective_scale_factor,
    )
}

#[cfg(test)]
mod tests {
    use super::{PaintCommand, PaintContext, RenderEnvironment, RenderIdentity};
    use harbor_types::{Rect, RgbaColor};

    const BOUNDS: Rect = Rect {
        x: 10.0,
        y: 10.0,
        width: 100.0,
        height: 100.0,
    };

    #[test]
    fn paint_context_preserves_command_order_and_intersects_scoped_clips() {
        let environment = RenderEnvironment::new(200.0, 100.0, 1.0);
        let mut context = PaintContext::new(environment, BOUNDS);

        context.fill_rect(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
            RgbaColor::BLACK,
        );
        context.with_clip(
            Rect {
                x: 20.0,
                y: 20.0,
                width: 10.0,
                height: 10.0,
            },
            |context| {
                context.fill_rect(
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 50.0,
                        height: 50.0,
                    },
                    RgbaColor::WHITE,
                );
            },
        );
        context.fill_rect(
            Rect {
                x: 90.0,
                y: 90.0,
                width: 20.0,
                height: 20.0,
            },
            RgbaColor::WHITE,
        );

        let commands = context.finish();
        assert_eq!(commands.len(), 3);
        assert!(matches!(commands[0], PaintCommand::FillRect { .. }));
        assert_eq!(commands[0].clip(), BOUNDS);
        assert_eq!(
            commands[1].clip(),
            Rect {
                x: 20.0,
                y: 20.0,
                width: 10.0,
                height: 10.0,
            }
        );
        assert_eq!(commands[2].clip(), BOUNDS);
    }

    #[test]
    fn render_environment_is_target_specific_and_handle_free() {
        let main = RenderEnvironment::new(800.0, 600.0, 1.0);
        let dialog = RenderEnvironment::new(600.0, 400.0, 1.5);

        assert_eq!(main.logical_size(), (800.0, 600.0));
        assert_eq!(dialog.logical_size(), (600.0, 400.0));
        assert_eq!(dialog.scale_factor(), 1.5);
    }

    #[test]
    fn paint_context_tracks_render_identities_without_changing_command_order() {
        let mut context = PaintContext::new(RenderEnvironment::new(100.0, 100.0, 1.0), BOUNDS);
        context.with_identity(RenderIdentity::new(7), |context| {
            context.fill_rect(BOUNDS, RgbaColor::BLACK);
        });

        assert!(
            context
                .visited_identities()
                .contains(&RenderIdentity::new(7))
        );
        assert_eq!(context.finish().len(), 1);
    }

    #[test]
    #[ignore = "requires a native window and GPU"]
    fn render_target_presents_a_black_frame() {
        use std::sync::Arc;
        use winit::{
            application::ApplicationHandler,
            event_loop::{ActiveEventLoop, EventLoop},
            window::Window,
        };

        struct Smoke {
            outcome: Option<super::FrameOutcome>,
        }

        impl ApplicationHandler for Smoke {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                let window = Arc::new(
                    event_loop
                        .create_window(Window::default_attributes())
                        .unwrap(),
                );
                let (_, mut target) = pollster::block_on(super::UiRenderer::new(window)).unwrap();
                self.outcome = Some(target.render(|_| {}));
                event_loop.exit();
            }

            fn window_event(
                &mut self,
                _event_loop: &ActiveEventLoop,
                _window_id: winit::window::WindowId,
                _event: winit::event::WindowEvent,
            ) {
            }
        }

        let mut builder = EventLoop::builder();
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::EventLoopBuilderExtWindows;

            builder.with_any_thread(true);
        }
        let event_loop = builder.build().unwrap();
        let mut smoke = Smoke { outcome: None };
        event_loop.run_app(&mut smoke).unwrap();

        assert_eq!(smoke.outcome, Some(super::FrameOutcome::Presented));
    }
}
