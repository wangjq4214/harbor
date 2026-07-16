use crate::{BoxConstraints, Key, PaintContext, Rect, Widget, WidgetEventResult};
use harbor_gpu::gpu::{self, ColoredVertex};

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
    child_bounds: Rect,
    pointer: Option<(f32, f32)>,
    background_buffer: Option<wgpu::Buffer>,
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
            child_bounds: Rect::default(),
            pointer: None,
            background_buffer: None,
        }
    }

    fn key(&self) -> Option<Key> {
        self.key
    }

    fn layout(&self, state: &mut Self::State, constraints: BoxConstraints) -> Rect {
        let child = self.child.layout(&mut state.child, constraints.loosen());
        let (width, height) = constraints.constrain(child.width, child.height);
        state.child_bounds = Rect {
            x: (width - child.width).max(0.0) / 2.0,
            y: (height - child.height).max(0.0) / 2.0,
            ..child
        };
        Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
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
                let inside = state.pointer.is_some_and(|(x, y)| bounds.contains(x, y));
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
            _ => self.child.event(
                &mut state.child,
                event,
                Rect {
                    x: bounds.x + state.child_bounds.x,
                    y: bounds.y + state.child_bounds.y,
                    ..state.child_bounds
                },
            ),
        }
    }

    fn paint<'pass>(
        &self,
        state: &mut Self::State,
        context: PaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        let color = match state.state {
            ButtonState::Normal => [0.15, 0.15, 0.15, 1.0],
            ButtonState::Hover => [0.2, 0.2, 0.2, 1.0],
            ButtonState::Pressed => [0.1, 0.35, 0.1, 1.0],
            ButtonState::Focused => [0.15, 0.45, 0.15, 1.0],
            ButtonState::Disabled => [0.1, 0.1, 0.1, 1.0],
        };
        let vertices = ColoredVertex::from_pixel_rect(
            context.bounds.x,
            context.bounds.y,
            context.bounds.x + context.bounds.width,
            context.bounds.y + context.bounds.height,
            color,
            context.gpu.surface_size().0 as f32,
            context.gpu.surface_size().1 as f32,
        );
        let buffer = state.background_buffer.get_or_insert_with(|| {
            gpu::create_colored_vertex_buffer(context.gpu.device(), &[ColoredVertex::default(); 6])
        });
        context
            .gpu
            .queue()
            .write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
        pass.set_pipeline(&context.gpu.colored_quad_pipeline());
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..6, 0..1);
        self.child.paint(
            &mut state.child,
            PaintContext {
                gpu: context.gpu,
                text: context.text,
                bounds: Rect {
                    x: context.bounds.x + state.child_bounds.x,
                    y: context.bounds.y + state.child_bounds.y,
                    ..state.child_bounds
                },
            },
            pass,
        );
    }
}
