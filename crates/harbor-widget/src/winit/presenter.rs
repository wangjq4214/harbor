//! Frame presentation: acquisition policy, wgpu encode sequence, and frame outcomes.

use super::WinitAdapter;
use crate::effects::RuntimeEffects;
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use std::time::Instant;
use winit::window::Window;

impl WinitAdapter {
    /// Executes one complete integration frame.
    pub fn render<'frame, 'surface>(
        &mut self,
        runtime: &mut Runtime,
        target: WinitFrameTarget<'frame, 'surface>,
    ) -> FrameOutcome {
        self.render_with_prepare(runtime, target, |_| {})
    }

    /// Executes one complete integration frame after the runtime update and
    /// before GPU encoding. Hosts use this to register frame-local resources
    /// produced during the update without owning presentation policy.
    pub fn render_with_prepare<'frame, 'surface>(
        &mut self,
        runtime: &mut Runtime,
        mut target: WinitFrameTarget<'frame, 'surface>,
        prepare: impl FnOnce(&mut Runtime),
    ) -> FrameOutcome {
        let effects = self.redraw_requested(runtime, Instant::now());
        prepare(runtime);

        if !self.surface_state.can_acquire() {
            return FrameOutcome::skipped(effects);
        }

        if self.surface_state.configuration_dirty() {
            target.reconfigure(self.surface_state.viewport());
            self.surface_state.mark_configured();
        }

        let acquisition = classify_surface_texture(target.surface().get_current_texture());
        let commit = effects.force_present;
        let reconfigure_after_present = matches!(&acquisition, FrameAcquisition::Suboptimal(_));

        if matches!(&acquisition, FrameAcquisition::RecoveryRequired) {
            target.reconfigure(self.surface_state.viewport());
        }

        let outcome = self.finish_acquisition(effects, acquisition, |output| {
            execute_wgpu_frame(runtime, &target, output, commit)
        });

        if reconfigure_after_present {
            target.reconfigure(self.surface_state.viewport());
        }

        outcome
    }

    /// Test seam for acquisition disposition without a native surface.
    ///
    /// Mirrors production recovery/retry policy from [`Self::render`], omitting
    /// only Host-owned surface reconfiguration.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn finish_acquisition<T>(
        &mut self,
        effects: RuntimeEffects,
        acquisition: FrameAcquisition<T>,
        present: impl FnOnce(T) -> Result<(), FrameError>,
    ) -> FrameOutcome {
        match acquisition {
            FrameAcquisition::Presented(output) => {
                let outcome = self.finish_presentable(effects, output, false, present);
                if outcome.is_presented() {
                    self.surface_state.reset_after_success();
                }
                outcome
            }
            FrameAcquisition::Suboptimal(output) => {
                let outcome = self.finish_presentable(effects, output, true, present);
                if outcome.is_fatal() || !outcome.is_presented() {
                    return outcome;
                }
                let mut final_effects = outcome.into_effects();
                if self.surface_state.allow_recovery_retry() {
                    final_effects.merge(self.request_frame());
                }
                FrameOutcome::presented_suboptimal(final_effects)
            }
            FrameAcquisition::RecoveryRequired => {
                let mut recovery_effects = effects;
                if self.surface_state.allow_recovery_retry() {
                    recovery_effects.merge(self.request_frame());
                    FrameOutcome::recovery_required(recovery_effects)
                } else {
                    FrameOutcome::fatal(
                        FrameError::presentation(
                            "surface remained lost or outdated after reconfiguration",
                        ),
                        recovery_effects,
                    )
                }
            }
            FrameAcquisition::Skipped => FrameOutcome::skipped(effects),
            FrameAcquisition::Validation => FrameOutcome::fatal(
                FrameError::validation("surface returned a validation error"),
                effects,
            ),
        }
    }

    pub(super) fn finish_presentable<T>(
        &mut self,
        mut effects: RuntimeEffects,
        output: T,
        suboptimal: bool,
        present: impl FnOnce(T) -> Result<(), FrameError>,
    ) -> FrameOutcome {
        if let Err(error) = present(output) {
            return FrameOutcome::fatal(error, effects);
        }
        effects.merge(self.frame_completed(Instant::now()));
        if suboptimal {
            FrameOutcome::presented_suboptimal(effects)
        } else {
            FrameOutcome::presented(effects)
        }
    }
}

/// The acquisition classification used by the frame lifecycle policy.
///
/// Native wgpu statuses are mapped to this private seam before presentation,
/// allowing the policy to be tested without constructing a native Surface.
pub(super) enum FrameAcquisition<T> {
    Presented(T),
    Suboptimal(T),
    RecoveryRequired,
    Skipped,
    Validation,
}

pub(super) fn classify_surface_texture(
    texture: wgpu::CurrentSurfaceTexture,
) -> FrameAcquisition<wgpu::SurfaceTexture> {
    match texture {
        wgpu::CurrentSurfaceTexture::Success(output) => FrameAcquisition::Presented(output),
        wgpu::CurrentSurfaceTexture::Suboptimal(output) => FrameAcquisition::Suboptimal(output),
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            FrameAcquisition::RecoveryRequired
        }
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            FrameAcquisition::Skipped
        }
        wgpu::CurrentSurfaceTexture::Validation => FrameAcquisition::Validation,
    }
}

/// Runs the fixed GPU operation sequence for a presentable frame.
///
/// Keeping the sequence closure-driven makes the ordering contract testable
/// without constructing a native window or surface.
pub(super) fn execute_presented_frame<T, V, C, E>(
    acquire: impl FnOnce() -> Result<T, E>,
    create_view: impl FnOnce(&T) -> V,
    encode: impl FnOnce(V) -> Result<C, E>,
    submit: impl FnOnce(C),
    notify: impl FnOnce(),
    present: impl FnOnce(T),
) -> Result<(), E> {
    let texture = acquire()?;
    let view = create_view(&texture);
    let command = encode(view)?;
    submit(command);
    notify();
    present(texture);
    Ok(())
}

pub(super) fn execute_wgpu_frame(
    runtime: &mut Runtime,
    target: &WinitFrameTarget<'_, '_>,
    output: wgpu::SurfaceTexture,
    commit: bool,
) -> Result<(), FrameError> {
    let clear_color = frame_clear_color(
        resolve_frame_appearance(runtime, target.backdrop_available()),
        target.alpha_mode(),
    );
    let viewport = runtime
        .current_viewport()
        .cloned()
        .unwrap_or_else(|| Viewport::new(1, 1, 1.0));
    execute_presented_frame(
        || Ok(output),
        |output| {
            output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default())
        },
        |view| {
            let mut encoder =
                target
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("harbor main frame encoder"),
                    });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("harbor main render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                runtime.encode(target.queue(), &mut pass, viewport, commit);
            }
            Ok(encoder.finish())
        },
        |command| {
            target.queue().submit(Some(command));
        },
        || target.window().pre_present_notify(),
        |output| target.queue().present(output),
    )
}

/// Resolves the external frame appearance, using opaque black for runtimes
/// without an external provider (such as the confirmation dialog).
pub(super) fn resolve_frame_appearance(runtime: &Runtime, backdrop_available: bool) -> [f32; 4] {
    runtime
        .frame_appearance(backdrop_available)
        .map(|appearance| appearance.rgba)
        .unwrap_or([0.0, 0.0, 0.0, 1.0])
}

/// Converts a straight-alpha RGBA clear value into the representation
/// required by the configured surface compositing mode.
pub(super) fn frame_clear_color(
    rgba: [f32; 4],
    alpha_mode: wgpu::CompositeAlphaMode,
) -> wgpu::Color {
    let a = rgba[3] as f64;
    let (r, g, b) = (rgba[0] as f64, rgba[1] as f64, rgba[2] as f64);
    if alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied {
        wgpu::Color {
            r: r * a,
            g: g * a,
            b: b * a,
            a,
        }
    } else {
        wgpu::Color { r, g, b, a }
    }
}

/// Borrowed host resources valid for one frame.
///
/// The separate `surface` lifetime describes the lifetime carried by the wgpu
/// surface itself, while `frame` describes the borrow of that surface and the
/// other host resources. Configuration mutation is framed as a Host-owned
/// callback so encode can share the same GpuContext via the temporary
/// CustomPaint GPU scope without aliasing a mutable configuration borrow.
/// This value is consumed by a frame call and cannot be retained by either
/// the runtime or adapter.
pub struct WinitFrameTarget<'frame, 'surface> {
    window: &'frame Window,
    surface: &'frame wgpu::Surface<'surface>,
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    configure: &'frame mut dyn FnMut(u32, u32),
    backdrop_available: bool,
    alpha_mode: wgpu::CompositeAlphaMode,
}

impl<'frame, 'surface> WinitFrameTarget<'frame, 'surface> {
    pub fn new(
        window: &'frame Window,
        surface: &'frame wgpu::Surface<'surface>,
        device: &'frame wgpu::Device,
        queue: &'frame wgpu::Queue,
        configure: &'frame mut dyn FnMut(u32, u32),
        backdrop_available: bool,
        alpha_mode: wgpu::CompositeAlphaMode,
    ) -> Self {
        Self {
            window,
            surface,
            device,
            queue,
            configure,
            backdrop_available,
            alpha_mode,
        }
    }

    pub fn reconfigure(&mut self, viewport: &Viewport) {
        assert!(
            viewport.is_drawable(),
            "refusing zero-sized surface configure"
        );
        (self.configure)(viewport.physical_size.0, viewport.physical_size.1);
    }

    pub fn window(&self) -> &'frame Window {
        self.window
    }

    pub fn surface(&self) -> &'frame wgpu::Surface<'surface> {
        self.surface
    }

    pub fn device(&self) -> &'frame wgpu::Device {
        self.device
    }

    pub fn queue(&self) -> &'frame wgpu::Queue {
        self.queue
    }

    pub const fn backdrop_available(&self) -> bool {
        self.backdrop_available
    }

    pub const fn alpha_mode(&self) -> wgpu::CompositeAlphaMode {
        self.alpha_mode
    }
}

/// The host-visible result of one attempted frame.
#[derive(Clone, Debug, PartialEq)]
pub enum FrameOutcome {
    Presented(RuntimeEffects),
    PresentedSuboptimal(RuntimeEffects),
    Skipped(RuntimeEffects),
    RecoveryRequired(RuntimeEffects),
    Fatal(FrameError, RuntimeEffects),
}

impl FrameOutcome {
    pub const fn presented(effects: RuntimeEffects) -> Self {
        Self::Presented(effects)
    }

    pub const fn presented_suboptimal(effects: RuntimeEffects) -> Self {
        Self::PresentedSuboptimal(effects)
    }

    pub const fn skipped(effects: RuntimeEffects) -> Self {
        Self::Skipped(effects)
    }

    pub const fn recovery_required(effects: RuntimeEffects) -> Self {
        Self::RecoveryRequired(effects)
    }

    pub const fn fatal(error: FrameError, effects: RuntimeEffects) -> Self {
        Self::Fatal(error, effects)
    }

    pub const fn effects(&self) -> &RuntimeEffects {
        match self {
            Self::Presented(effects)
            | Self::PresentedSuboptimal(effects)
            | Self::Skipped(effects)
            | Self::RecoveryRequired(effects)
            | Self::Fatal(_, effects) => effects,
        }
    }

    pub fn into_effects(self) -> RuntimeEffects {
        match self {
            Self::Presented(effects)
            | Self::PresentedSuboptimal(effects)
            | Self::Skipped(effects)
            | Self::RecoveryRequired(effects)
            | Self::Fatal(_, effects) => effects,
        }
    }

    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_, _))
    }

    pub const fn is_presented(&self) -> bool {
        matches!(self, Self::Presented(_) | Self::PresentedSuboptimal(_))
    }

    pub const fn is_suboptimal(&self) -> bool {
        matches!(self, Self::PresentedSuboptimal(_))
    }

    pub const fn is_recovery_required(&self) -> bool {
        matches!(self, Self::RecoveryRequired(_))
    }

    pub const fn fatal_error(&self) -> Option<&FrameError> {
        match self {
            Self::Fatal(error, _) => Some(error),
            _ => None,
        }
    }
}

/// A fatal presentation failure for the host to diagnose and handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    OutOfMemory,
    DeviceLost,
    Validation(String),
    Presentation(String),
    Other(String),
}

impl FrameError {
    pub const fn out_of_memory() -> Self {
        Self::OutOfMemory
    }

    pub const fn device_lost() -> Self {
        Self::DeviceLost
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn presentation(message: impl Into<String>) -> Self {
        Self::Presentation(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::{frame_clear_color, resolve_frame_appearance};
    use crate::runtime::Runtime;
    use wgpu::CompositeAlphaMode;

    #[test]
    fn should_use_opaque_black_when_runtime_has_no_appearance_provider() {
        let runtime = Runtime::new();

        assert_eq!(
            resolve_frame_appearance(&runtime, true),
            [0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn should_premultiply_external_appearance_for_premultiplied_surface() {
        let color = frame_clear_color([0.36, 0.20, 0.08, 0.25], CompositeAlphaMode::PreMultiplied);

        assert_eq!(color.r, 0.36_f32 as f64 * 0.25_f32 as f64);
        assert_eq!(color.g, 0.20_f32 as f64 * 0.25_f32 as f64);
        assert_eq!(color.b, 0.08_f32 as f64 * 0.25_f32 as f64);
        assert_eq!(color.a, 0.25);
    }

    #[test]
    fn should_preserve_external_appearance_for_postmultiplied_surface() {
        let color = frame_clear_color([0.36, 0.20, 0.08, 0.25], CompositeAlphaMode::PostMultiplied);

        assert_eq!(color.r, 0.36_f32 as f64);
        assert_eq!(color.g, 0.20_f32 as f64);
        assert_eq!(color.b, 0.08_f32 as f64);
        assert_eq!(color.a, 0.25_f32 as f64);
    }

    #[test]
    fn opaque_fallback_is_not_darkened_by_premultiplication() {
        let color = frame_clear_color([0.36, 0.20, 0.08, 1.0], CompositeAlphaMode::PreMultiplied);

        assert_eq!(color.r, 0.36_f32 as f64);
        assert_eq!(color.g, 0.20_f32 as f64);
        assert_eq!(color.b, 0.08_f32 as f64);
        assert_eq!(color.a, 1.0_f32 as f64);
    }
}
