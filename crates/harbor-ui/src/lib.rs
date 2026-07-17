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
pub use custom_paint::{CustomPaint, CustomPainter, PaintContext};
pub use dialog::{Dialog, DialogEvent, DialogRuntime, WindowSpec};
pub use harbor_render::RenderEnvironment;
pub use layout::{Column, Expanded, Row, ScrollView, Stack};
pub use primitives::{BoxConstraints, Color, EdgeInsets, Key, Rect};
pub use terminal::{Terminal, TerminalIntent, TerminalScroll, TerminalVisualState};
pub use text::{Text, TextStyle};
pub use widget::{EventResult as WidgetEventResult, Map, Widget, WidgetRuntime};

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_render::{PaintCommand, RenderEnvironment};

    struct EnvironmentAware;

    impl Widget<()> for EnvironmentAware {
        type State = ();

        fn create_state(&self) -> Self::State {}

        fn layout(
            &self,
            _state: &mut Self::State,
            environment: RenderEnvironment,
            _constraints: BoxConstraints,
        ) -> Rect {
            let (width, height) = environment.logical_size();
            Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            }
        }
    }

    struct ViewportProbe;

    impl Widget<()> for ViewportProbe {
        type State = ();

        fn create_state(&self) -> Self::State {}

        fn layout(
            &self,
            _state: &mut Self::State,
            _environment: RenderEnvironment,
            _constraints: BoxConstraints,
        ) -> Rect {
            Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 200.0,
            }
        }

        fn paint<'a>(&'a self, _state: &'a mut Self::State, context: &mut PaintContext<'a>) {
            context.fill_rect(context.bounds(), harbor_types::RgbaColor::WHITE);
        }
    }

    struct FillPainter;

    impl CustomPainter for FillPainter {
        fn paint<'a>(&'a self, context: &mut PaintContext<'a>) {
            context.fill_rect(context.bounds(), harbor_types::RgbaColor::WHITE);
        }
    }

    #[test]
    fn widget_runtime_injects_render_environment_and_records_text_commands() {
        let environment = RenderEnvironment::new(320.0, 240.0, 1.5);
        let widget = EnvironmentAware;
        let mut runtime = WidgetRuntime::new(&widget);

        assert_eq!(
            runtime.layout(&widget, environment, BoxConstraints::tight(1.0, 1.0)),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 320.0,
                height: 240.0,
            }
        );

        let text = Text::new("Renderer owned");
        let mut text_runtime: WidgetRuntime<_, ()> = WidgetRuntime::new(&text);
        let bounds = text_runtime.layout(&text, environment, BoxConstraints::tight(320.0, 24.0));
        let mut context = PaintContext::new(environment, bounds);
        text_runtime.paint(&text, &mut context);

        assert!(matches!(
            context.finish().as_slice(),
            [PaintCommand::Text {
                text: "Renderer owned",
                ..
            }]
        ));
    }

    #[test]
    fn scroll_view_scopes_commands_to_the_viewport() {
        let environment = RenderEnvironment::new(100.0, 100.0, 1.0);
        let widget = ScrollView::new(ViewportProbe);
        let mut runtime = WidgetRuntime::new(&widget);
        let bounds = runtime.layout(&widget, environment, BoxConstraints::tight(100.0, 100.0));
        let scroll = winit::event::WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: winit::event::MouseScrollDelta::LineDelta(0.0, -1.0),
            phase: winit::event::TouchPhase::Moved,
        };
        runtime.event(&widget, &scroll, bounds);
        let mut context = PaintContext::new(environment, bounds);

        runtime.paint(&widget, &mut context);

        assert!(matches!(
            context.finish().as_slice(),
            [PaintCommand::FillRect { rect, clip, .. }]
                if rect.y == -20.0 && *clip == bounds
        ));
    }

    #[test]
    fn custom_paint_records_renderer_commands() {
        let environment = RenderEnvironment::new(80.0, 40.0, 1.0);
        let widget = CustomPaint::new(FillPainter);
        let mut runtime: WidgetRuntime<_, ()> = WidgetRuntime::new(&widget);
        let bounds = runtime.layout(&widget, environment, BoxConstraints::tight(80.0, 40.0));
        let mut context = PaintContext::new(environment, bounds);

        runtime.paint(&widget, &mut context);

        assert!(matches!(
            context.finish().as_slice(),
            [PaintCommand::FillRect { rect, .. }] if *rect == bounds
        ));
    }
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

    #[test]
    fn terminal_widget_emits_semantic_wheel_scroll_intent() {
        let terminal = Terminal::new(Key(9));
        let mut runtime: WidgetRuntime<_, TerminalIntent> = WidgetRuntime::new(&terminal);
        let bounds = runtime.layout(
            &terminal,
            RenderEnvironment::new(100.0, 100.0, 1.0),
            BoxConstraints::tight(100.0, 100.0),
        );
        let event = winit::event::WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: winit::event::MouseScrollDelta::LineDelta(0.0, 1.0),
            phase: winit::event::TouchPhase::Moved,
        };
        assert_eq!(
            runtime.event(&terminal, &event, bounds),
            WidgetEventResult::Intent(TerminalIntent::Scroll(TerminalScroll::Lines(3)))
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
        let bounds = runtime.layout(
            &button,
            RenderEnvironment::new(100.0, 24.0, 1.0),
            BoxConstraints::tight(100.0, 24.0),
        );
        let cursor = winit::event::WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(10.0, 10.0),
        };
        assert_eq!(
            runtime.event(&button, &cursor, bounds),
            WidgetEventResult::Ignored
        );
        let pressed = winit::event::WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: winit::event::ElementState::Pressed,
            button: winit::event::MouseButton::Left,
        };
        assert_eq!(
            runtime.event(&button, &pressed, bounds),
            WidgetEventResult::Handled
        );
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
        let bounds = runtime.layout(
            &widget,
            RenderEnvironment::new(100.0, 24.0, 1.0),
            BoxConstraints::tight(100.0, 24.0),
        );
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
        assert_eq!(
            runtime.event(&widget, &pressed, bounds),
            WidgetEventResult::Handled
        );
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
            runtime.layout(
                &widget,
                RenderEnvironment::new(120.0, 100.0, 1.0),
                BoxConstraints::tight(120.0, 100.0),
            ),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 100.0,
            }
        );
    }
}
