mod background;
pub mod caps;
mod cursor;
mod decoration;
pub mod font;
pub mod gpu;
mod scrollbar;
pub mod selection;
mod text;

pub use background::Background;
pub use caps::{InteractionResult, UiRequest, WaitResult};
pub use cursor::Cursor;
pub use decoration::Decoration;
pub use font::{FontBook, load_system_fonts};
pub use gpu::{
    GpuContext, SurfaceDisposition, SurfaceStatus, UploadMode, UploadPlan, UploadPolicy,
    surface_disposition,
};
pub use scrollbar::Scrollbar;
pub use selection::Selection;
pub use text::{AtlasGlyph, Text, TextMetrics};

use harbor_types::TerminalSnapshot;

/// Result of an event handler — controls whether propagation continues.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventResult {
    Handled,
    Continue,
    /// Multi-line paste needs confirmation. Contains the raw clipboard text.
    ConfirmPaste(String),
}

/// Every UI element: prepare + draw (+ optional resize).
///
/// Interaction is handled through concrete snapshot inputs and typed results.
pub trait Component {
    /// Uploads dirty GPU resources. No-op when nothing changed.
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&TerminalSnapshot>);
    /// Issues draw calls. Always lightweight, no GPU allocation.
    fn draw(&self, pass: &mut wgpu::RenderPass);

    /// Called when the window surface is resized. Components that cache
    /// dimension-dependent GPU data (e.g. vertex buffers sized to the grid)
    /// should mark themselves dirty here.
    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {}
}
