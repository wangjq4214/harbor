mod background;
mod cursor;
mod decoration;
pub(crate) mod font;
pub(crate) mod gpu;
pub(crate) mod metrics;
mod scrollbar;
mod selection;
mod text;

pub(crate) use background::Background;
pub(crate) use cursor::Cursor;
pub(crate) use decoration::Decoration;
pub(crate) use font::{FontBook, load_system_fonts};
pub(crate) use gpu::GpuContext;
pub(crate) use metrics::TextMetrics;
pub(crate) use scrollbar::Scrollbar;
pub(crate) use selection::Selection;
pub(crate) use text::Text;

use crate::{pty::Pty, terminal::Screen, terminal::Terminal};
use std::time::Instant;
use winit::event::WindowEvent;
use winit::keyboard::ModifiersState;
use winit::window::Window;

/// Result of an event handler — controls whether propagation continues.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventResult {
    Handled,
    Continue,
}

/// Shared mutable context passed to component event handlers.
pub(crate) struct EventContext<'a> {
    pub(crate) gpu: &'a mut GpuContext,
    pub(crate) terminal: &'a mut Terminal,
    pub(crate) window: &'a Window,
    pub(crate) pty: &'a mut Pty,
    pub(crate) modifiers: ModifiersState,
    pub(crate) deadline: &'a mut Option<Instant>,
}

/// Every UI element: rendering + optional interaction.
/// Pure-rendering components use the default no-op event handlers.
pub(crate) trait Component {
    /// Uploads dirty GPU resources. No-op when nothing changed.
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>);
    /// Issues draw calls. Always lightweight, no GPU allocation.
    fn draw(&self, pass: &mut wgpu::RenderPass);

    /// Called when the window surface is resized. Components that cache
    /// dimension-dependent GPU data (e.g. vertex buffers sized to the grid)
    /// should mark themselves dirty here.
    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {}

    /// Handle a window event. Return `Handled` to stop propagation.
    fn handle_event(&mut self, _event: &WindowEvent, _ctx: &mut EventContext<'_>) -> EventResult {
        EventResult::Continue
    }

    /// Called before the event loop blocks; returns the next wake deadline or `None`.
    fn on_about_to_wait(&mut self, _ctx: &mut EventContext<'_>) -> Option<Instant> {
        None
    }
}
