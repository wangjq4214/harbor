//! Declarative GPU UI primitives for Harbor.
//!
//! Widgets are immutable configuration. [`DialogRuntime`] retains only transient
//! interaction state and emits host-owned intents.

mod button;
mod container;
mod custom_paint;
mod dialog;
mod primitives;
mod terminal;
mod text;

pub use button::{Button, ButtonState};
pub use container::{Alignment, Container};
pub use custom_paint::{CustomPaint, CustomPainter, PaintContext};
pub use dialog::{Dialog, DialogEvent, DialogRuntime, WindowSpec};
pub use primitives::{BoxConstraints, Color, EdgeInsets, Key, Rect};
pub use terminal::{
    AtlasGlyph, Component, FontBook, Terminal, TerminalIntent, TerminalOverlays, TerminalRenderer,
    TextMetrics, load_system_fonts,
};
pub use text::{Text, TextStyle};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_resize_intent_uses_assigned_bounds() {
        assert_eq!(
            Terminal::new(Key(9)).resize_intent(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 805.0,
                    height: 401.0,
                },
                8.0,
                20.0,
            ),
            TerminalIntent::Resize(harbor_types::TerminalSize {
                rows: 20,
                cols: 100,
            }),
        );
    }
}
