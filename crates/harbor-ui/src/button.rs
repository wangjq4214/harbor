use crate::{BoxConstraints, Key, PaintContext, Rect, Widget, WidgetEventResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Hover,
    Pressed,
    Focused,
    Disabled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button<A, W> {
    pub child: W,
    pub intent: A,
    pub enabled: bool,
    pub key: Option<Key>,
}

impl<A, W> Button<A, W> {
    pub fn new(child: W, intent: A) -> Self {
        Self {
            child,
            intent,
            enabled: true,
            key: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.enabled = !disabled;
        self
    }

    pub fn key(mut self, key: Key) -> Self {
        self.key = Some(key);
        self
    }
}

pub struct ButtonRuntime<S> {
    pub state: ButtonState,
    child: S,
    pointer: Option<(f32, f32)>,
}

impl<A, W> Widget<A> for Button<A, W>
where
    A: Clone,
    W: Widget<A>,
{
    type State = ButtonRuntime<W::State>;

    fn create_state(&self) -> Self::State {
        ButtonRuntime {
            state: if self.enabled {
                ButtonState::Normal
            } else {
                ButtonState::Disabled
            },
            child: self.child.create_state(),
            pointer: None,
        }
    }

    fn key(&self) -> Option<Key> {
        self.key
    }

    fn layout(&self, state: &mut Self::State, constraints: BoxConstraints) -> Rect {
        self.child.layout(&mut state.child, constraints)
    }

    fn event(
        &self,
        state: &mut Self::State,
        event: &winit::event::WindowEvent,
        bounds: Rect,
    ) -> WidgetEventResult<A> {
        if !self.enabled {
            state.state = ButtonState::Disabled;
            return WidgetEventResult::Ignored;
        }

        match event {
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let pointer = (position.x as f32, position.y as f32);
                state.pointer = Some(pointer);
                if state.state != ButtonState::Pressed {
                    state.state = if bounds.contains(pointer.0, pointer.1) {
                        ButtonState::Hover
                    } else {
                        ButtonState::Normal
                    };
                }
                WidgetEventResult::Ignored
            }
            winit::event::WindowEvent::MouseInput {
                state: button_state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                let inside = state
                    .pointer
                    .is_some_and(|(x, y)| bounds.contains(x, y));
                match button_state {
                    winit::event::ElementState::Pressed if inside => {
                        state.state = ButtonState::Pressed;
                        WidgetEventResult::Handled
                    }
                    winit::event::ElementState::Released if state.state == ButtonState::Pressed => {
                        state.state = if inside {
                            ButtonState::Hover
                        } else {
                            ButtonState::Normal
                        };
                        if inside {
                            WidgetEventResult::Intent(self.intent.clone())
                        } else {
                            WidgetEventResult::Handled
                        }
                    }
                    _ => WidgetEventResult::Ignored,
                }
            }
            _ => self.child.event(&mut state.child, event, bounds),
        }
    }

    fn paint<'pass>(
        &self,
        state: &mut Self::State,
        context: PaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        self.child.paint(&mut state.child, context, pass);
    }
}
