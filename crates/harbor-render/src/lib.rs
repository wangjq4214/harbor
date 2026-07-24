mod background;
pub mod caps;
mod cursor;
mod decoration;
pub mod gpu;
mod scrollbar;
pub mod selection;
mod text;

pub use background::Background;
pub use caps::{
    CursorContext, CursorInput, CursorWaitContext, ScrollbarContext, ScrollbarInput,
    ScrollbarWaitContext, SelectionContext, SelectionInput, SelectionWaitContext,
};
pub use cursor::Cursor;
pub use decoration::Decoration;
pub use gpu::GpuContext;
pub use harbor_text::{AtlasGlyph, FontBook, TextMetrics, load_system_fonts};
pub use scrollbar::Scrollbar;
pub use selection::Selection;
pub use text::Text;

use harbor_types::RenderSnapshot;

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
/// Interaction is not on this trait. Interactive layers implement the
/// capability traits in [`caps`] and receive only the rights they need.
pub trait Component {
    /// Uploads dirty GPU resources. No-op when nothing changed.
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&RenderSnapshot>);
    /// Issues draw calls. Always lightweight, no GPU allocation.
    fn draw(&self, pass: &mut wgpu::RenderPass);

    /// Called when the window surface is resized. Components that cache
    /// dimension-dependent GPU data (e.g. vertex buffers sized to the grid)
    /// should mark themselves dirty here.
    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {}
}
