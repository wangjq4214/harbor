use crate::{gpu::GpuContext, terminal::Screen};

/// Per-layer frame interface:
/// - `prepare`: uploads dirty GPU resources, no-op when nothing changed.
/// - `draw`: issues draw calls, no GPU allocation.
pub(crate) trait Layer {
    /// Uploads dirty GPU resources. No-op when nothing changed.
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>);
    /// Issues draw calls. Always lightweight, no GPU allocation.
    fn draw(&self, pass: &mut wgpu::RenderPass);
}
