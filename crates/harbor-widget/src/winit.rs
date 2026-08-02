//! Optional winit integration contracts.
//!
//! This module defines the per-window and per-frame boundary only. Event
//! conversion and frame acquisition/presentation are implemented by later
//! integration slices.

use crate::effects::RuntimeEffects;
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use winit::event::WindowEvent;
use winit::window::Window;

/// Per-window state for the winit integration.
#[derive(Default)]
pub struct WinitAdapter {
    // State such as modifiers, pointer position, scale, and IME composition
    // belongs here once event conversion is implemented.
    _state: (),
}

impl WinitAdapter {
    pub const fn new() -> Self {
        Self { _state: () }
    }

    /// Handles one window event and returns host effects.
    ///
    /// The event conversion and dispatch behavior is intentionally deferred;
    /// this foundation establishes the stable call boundary without changing
    /// application behavior.
    pub fn handle_event(&mut self, _runtime: &mut Runtime, _event: &WindowEvent) -> RuntimeEffects {
        RuntimeEffects::default()
    }

    /// Attempts one integration frame.
    ///
    /// Frame acquisition, encoding, submission, and presentation are deferred
    /// to the presentation slice. A deferred result makes that absence of
    /// behavior explicit rather than pretending a frame was presented.
    pub fn render<'frame, 'surface>(
        &mut self,
        _runtime: &mut Runtime,
        _target: WinitFrameTarget<'frame, 'surface>,
    ) -> FrameOutcome {
        FrameOutcome::Deferred
    }
}

/// Borrowed host resources valid for one frame.
///
/// The separate `surface` lifetime describes the lifetime carried by the wgpu
/// surface itself, while `frame` describes the borrow of that surface and the
/// other host resources. This value is consumed by a frame call and cannot be
/// retained by either the runtime or adapter.
pub struct WinitFrameTarget<'frame, 'surface> {
    window: &'frame Window,
    surface: &'frame wgpu::Surface<'surface>,
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    config: &'frame mut wgpu::SurfaceConfiguration,
    viewport: Viewport,
    clear_color: wgpu::Color,
}

impl<'frame, 'surface> WinitFrameTarget<'frame, 'surface> {
    pub fn new(
        window: &'frame Window,
        surface: &'frame wgpu::Surface<'surface>,
        device: &'frame wgpu::Device,
        queue: &'frame wgpu::Queue,
        config: &'frame mut wgpu::SurfaceConfiguration,
        viewport: Viewport,
        clear_color: wgpu::Color,
    ) -> Self {
        Self {
            window,
            surface,
            device,
            queue,
            config,
            viewport,
            clear_color,
        }
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

    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        self.config
    }

    pub fn config_mut(&mut self) -> &mut wgpu::SurfaceConfiguration {
        self.config
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn clear_color(&self) -> wgpu::Color {
        self.clear_color
    }
}

/// The host-visible result of one attempted frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameOutcome {
    Presented,
    Skipped,
    Deferred,
    RecoveryRequired,
    Fatal(FrameError),
}

impl FrameOutcome {
    pub const fn presented() -> Self {
        Self::Presented
    }

    pub const fn skipped() -> Self {
        Self::Skipped
    }

    pub const fn deferred() -> Self {
        Self::Deferred
    }

    pub const fn recovery_required() -> Self {
        Self::RecoveryRequired
    }

    pub const fn fatal(error: FrameError) -> Self {
        Self::Fatal(error)
    }

    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    pub const fn is_presented(&self) -> bool {
        matches!(self, Self::Presented)
    }

    pub const fn is_recovery_required(&self) -> bool {
        matches!(self, Self::RecoveryRequired)
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
