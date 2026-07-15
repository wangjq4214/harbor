//! Modal UI primitives for the Harbor terminal emulator.
//!
//! Provides reusable dialog building blocks: button focus tracking,
//! event results, and a [`ModalContent`] trait for custom dialog content.

use harbor_render::{AtlasGlyph, GpuContext, TextMetrics};
use winit::event::WindowEvent;

/// Which button currently has keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogButton {
    Paste,
    Cancel,
}

/// Result of processing a dialog input event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogResult {
    /// No state change — continue processing.
    None,
    /// User confirmed the action.
    Confirmed,
    /// User cancelled the action.
    Cancelled,
}

/// Content displayed inside a modal dialog window.
///
/// Implementations handle their own GPU resource preparation,
/// rendering, and input event processing. The dialog shell owns
/// the window and surface lifecycle.
pub trait ModalContent {
    /// Prepare GPU resources before rendering (called once per redraw).
    fn prepare(
        &mut self,
        gpu: &GpuContext,
        metrics: &TextMetrics,
        glyph: impl Fn(char) -> Option<AtlasGlyph>,
        text_pipeline: &wgpu::RenderPipeline,
        text_bind_group: &wgpu::BindGroup,
    );

    /// Issue draw calls for this content.
    fn render(
        &self,
        gpu: &GpuContext,
        text_pipeline: &wgpu::RenderPipeline,
        text_bind_group: &wgpu::BindGroup,
    );

    /// Process a window event. Returns the dialog outcome or `None`.
    fn handle_event(&mut self, event: &WindowEvent) -> DialogResult;
}
