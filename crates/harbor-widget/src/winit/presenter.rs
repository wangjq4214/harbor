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

        match acquisition {
            FrameAcquisition::Presented(output) => {
                let commit = effects.force_present;
                let outcome = self.finish_presentable(effects, output, false, |output| {
                    execute_wgpu_frame(runtime, &target, output, commit)
                });
                if outcome.is_presented() {
                    self.surface_state.reset_after_success();
                }
                outcome
            }
            FrameAcquisition::Suboptimal(output) => {
                let commit = effects.force_present;
                let outcome = self.finish_presentable(effects, output, true, |output| {
                    execute_wgpu_frame(runtime, &target, output, commit)
                });
                target.reconfigure(self.surface_state.viewport());
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
                target.reconfigure(self.surface_state.viewport());
                let mut recovery_effects = effects;
                if self.surface_state.allow_recovery_retry() {
                    recovery_effects.merge(self.request_frame());
                }
                FrameOutcome::recovery_required(recovery_effects)
            }
            FrameAcquisition::Skipped => FrameOutcome::skipped(effects),
        }
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
        match (acquisition.kind(), acquisition) {
            (FrameAcquisitionKind::Presented, FrameAcquisition::Presented(output)) => {
                let outcome = self.finish_presentable(effects, output, false, present);
                if outcome.is_presented() {
                    self.surface_state.reset_after_success();
                }
                outcome
            }
            (FrameAcquisitionKind::Suboptimal, FrameAcquisition::Suboptimal(output)) => {
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
            (FrameAcquisitionKind::RecoveryRequired, FrameAcquisition::RecoveryRequired) => {
                let mut recovery_effects = effects;
                if self.surface_state.allow_recovery_retry() {
                    recovery_effects.merge(self.request_frame());
                }
                FrameOutcome::recovery_required(recovery_effects)
            }
            (FrameAcquisitionKind::Skipped, FrameAcquisition::Skipped) => {
                FrameOutcome::skipped(effects)
            }
            _ => unreachable!("acquisition kind must match its classification"),
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum FrameAcquisitionKind {
    Presented,
    Suboptimal,
    RecoveryRequired,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PresentableKind {
    Presented,
    Suboptimal,
}

impl<T> FrameAcquisition<T> {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn kind(&self) -> FrameAcquisitionKind {
        match self {
            Self::Presented(_) => FrameAcquisitionKind::Presented,
            Self::Suboptimal(_) => FrameAcquisitionKind::Suboptimal,
            Self::RecoveryRequired => FrameAcquisitionKind::RecoveryRequired,
            Self::Skipped => FrameAcquisitionKind::Skipped,
        }
    }
}

pub(super) fn classify_presentable<T>(output: T, kind: PresentableKind) -> FrameAcquisition<T> {
    match kind {
        PresentableKind::Presented => FrameAcquisition::Presented(output),
        PresentableKind::Suboptimal => FrameAcquisition::Suboptimal(output),
    }
}

pub(super) fn classify_surface_texture(
    texture: wgpu::CurrentSurfaceTexture,
) -> FrameAcquisition<wgpu::SurfaceTexture> {
    match texture {
        wgpu::CurrentSurfaceTexture::Success(output) => {
            classify_presentable(output, PresentableKind::Presented)
        }
        wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
            classify_presentable(output, PresentableKind::Suboptimal)
        }
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            FrameAcquisition::RecoveryRequired
        }
        wgpu::CurrentSurfaceTexture::Timeout
        | wgpu::CurrentSurfaceTexture::Occluded
        | wgpu::CurrentSurfaceTexture::Validation => FrameAcquisition::Skipped,
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
                            load: wgpu::LoadOp::Clear(target.clear_color()),
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
    clear_color: wgpu::Color,
}

impl<'frame, 'surface> WinitFrameTarget<'frame, 'surface> {
    pub fn new(
        window: &'frame Window,
        surface: &'frame wgpu::Surface<'surface>,
        device: &'frame wgpu::Device,
        queue: &'frame wgpu::Queue,
        configure: &'frame mut dyn FnMut(u32, u32),
        clear_color: wgpu::Color,
    ) -> Self {
        Self {
            window,
            surface,
            device,
            queue,
            configure,
            clear_color,
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

    pub fn clear_color(&self) -> wgpu::Color {
        self.clear_color
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
