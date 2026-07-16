//! Declarative GPU UI primitives for Harbor.
//!
//! Widgets are immutable configuration. [`DialogRuntime`] retains only transient
//! interaction state and emits host-owned intents.

mod button;
mod container;
mod custom_paint;
mod dialog;
mod layout;
mod primitives;
mod terminal;
mod text;
mod widget;

pub use button::{Button, ButtonState};
pub use container::{Alignment, Container};
pub use layout::{Column, Expanded, Row, ScrollView, Stack};
pub use custom_paint::{CustomPaint, CustomPainter, PaintContext};
pub use dialog::{Dialog, DialogEvent, DialogRuntime, WindowSpec};
pub use primitives::{BoxConstraints, Color, EdgeInsets, Key, Rect};
pub use terminal::{
    AtlasGlyph, FontBook, Terminal, TerminalIntent, TextMetrics, TextResources, load_system_fonts,
};
pub use text::{Text, TextStyle};
pub use widget::{EventResult as WidgetEventResult, Map, Widget, WidgetRuntime};

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

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Intent {
        Activate,
        Back,
        Front,
    }

    #[test]
    fn widget_runtime_emits_button_intent_after_release_inside_bounds() {
        let button = Button::new(Text::new("Confirm"), Intent::Activate);
        let mut runtime: WidgetRuntime<_, Intent> = WidgetRuntime::new(&button);
        let bounds = runtime.layout(&button, BoxConstraints::tight(100.0, 24.0));
        let cursor = winit::event::WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(10.0, 10.0),
        };
        assert_eq!(runtime.event(&button, &cursor, bounds), WidgetEventResult::Ignored);
        let pressed = winit::event::WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: winit::event::ElementState::Pressed,
            button: winit::event::MouseButton::Left,
        };
        assert_eq!(runtime.event(&button, &pressed, bounds), WidgetEventResult::Handled);
        let released = winit::event::WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: winit::event::ElementState::Released,
            button: winit::event::MouseButton::Left,
        };
        assert_eq!(
            runtime.event(&button, &released, bounds),
            WidgetEventResult::Intent(Intent::Activate)
        );
    }

    #[test]
    fn stack_routes_pointer_release_to_frontmost_widget() {
        let widget = Stack::new(
            Button::new(Text::new("Back"), Intent::Back),
            Button::new(Text::new("Front"), Intent::Front),
        );
        let mut runtime: WidgetRuntime<_, Intent> = WidgetRuntime::new(&widget);
        let bounds = runtime.layout(&widget, BoxConstraints::tight(100.0, 24.0));
        let cursor = winit::event::WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(10.0, 10.0),
        };
        runtime.event(&widget, &cursor, bounds);
        let pressed = winit::event::WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: winit::event::ElementState::Pressed,
            button: winit::event::MouseButton::Left,
        };
        assert_eq!(runtime.event(&widget, &pressed, bounds), WidgetEventResult::Handled);
        let released = winit::event::WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: winit::event::ElementState::Released,
            button: winit::event::MouseButton::Left,
        };
        assert_eq!(
            runtime.event(&widget, &released, bounds),
            WidgetEventResult::Intent(Intent::Front)
        );
    }

    #[test]
    fn expanded_widget_claims_remaining_column_height() {
        let widget = Column::new(Text::new("Title"), Expanded::new(Text::new("Body")));
        let mut runtime: WidgetRuntime<_, ()> = WidgetRuntime::new(&widget);
        assert_eq!(
            runtime.layout(&widget, BoxConstraints::tight(120.0, 100.0)),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 100.0,
            }
        );
    }
}
