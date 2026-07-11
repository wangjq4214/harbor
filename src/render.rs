mod background;
mod caps;
mod cursor;
mod decoration;
pub(crate) mod font;
pub(crate) mod gpu;
pub(crate) mod metrics;
mod scrollbar;
mod selection;
mod text;

pub(crate) use background::Background;
pub(crate) use caps::{
    CursorContext, CursorInput, CursorWaitContext, ScrollbarContext, ScrollbarInput,
    ScrollbarWaitContext, SelectionContext, SelectionInput,
};
pub(crate) use cursor::Cursor;
pub(crate) use decoration::Decoration;
pub(crate) use font::{FontBook, load_system_fonts};
pub(crate) use gpu::GpuContext;
pub(crate) use metrics::TextMetrics;
pub(crate) use scrollbar::Scrollbar;
pub(crate) use selection::Selection;
pub(crate) use text::Text;

use crate::terminal::Screen;

/// Result of an event handler — controls whether propagation continues.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventResult {
    Handled,
    Continue,
}

/// Every UI element: prepare + draw (+ optional resize).
///
/// Interaction is not on this trait. Interactive layers implement the
/// capability traits in [`caps`] and receive only the rights they need.
pub(crate) trait Component {
    /// Uploads dirty GPU resources. No-op when nothing changed.
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>);
    /// Issues draw calls. Always lightweight, no GPU allocation.
    fn draw(&self, pass: &mut wgpu::RenderPass);

    /// Called when the window surface is resized. Components that cache
    /// dimension-dependent GPU data (e.g. vertex buffers sized to the grid)
    /// should mark themselves dirty here.
    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {}
}
