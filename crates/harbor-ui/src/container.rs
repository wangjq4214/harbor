use crate::{BoxConstraints, Color, EdgeInsets, PaintContext, Rect, Widget, WidgetEventResult};
use harbor_gpu::gpu::{self, ColoredVertex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Container<W> {
    pub child: W,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub padding: EdgeInsets,
    pub margin: EdgeInsets,
    pub alignment: Alignment,
    pub background: Option<Color>,
    pub corner_radius: f32,
}

impl<W> Container<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            width: None,
            height: None,
            padding: EdgeInsets::default(),
            margin: EdgeInsets::default(),
            alignment: Alignment::Start,
            background: None,
            corner_radius: 0.0,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn margin(mut self, margin: EdgeInsets) -> Self {
        self.margin = margin;
        self
    }

    pub fn align(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }
}

pub struct ContainerState<S> {
    child: S,
    child_bounds: Rect,
    background_buffer: Option<wgpu::Buffer>,
}

impl<A, W> Widget<A> for Container<W>
where
    W: Widget<A>,
{
    type State = ContainerState<W::State>;

    fn create_state(&self) -> Self::State {
        ContainerState {
            child: self.child.create_state(),
            child_bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            background_buffer: None,
        }
    }

    fn layout(&self, state: &mut Self::State, constraints: BoxConstraints) -> Rect {
        let horizontal =
            self.margin.left + self.margin.right + self.padding.left + self.padding.right;
        let vertical =
            self.margin.top + self.margin.bottom + self.padding.top + self.padding.bottom;
        let child_constraints = BoxConstraints {
            min_width: 0.0,
            max_width: (constraints.max_width - horizontal).max(0.0),
            min_height: 0.0,
            max_height: (constraints.max_height - vertical).max(0.0),
        };
        let child = self.child.layout(&mut state.child, child_constraints);
        let requested_width = self.width.unwrap_or(child.width + horizontal);
        let requested_height = self.height.unwrap_or(child.height + vertical);
        let (width, height) = constraints.constrain(requested_width, requested_height);
        let available_width = (width - horizontal).max(0.0);
        let available_height = (height - vertical).max(0.0);
        let offset_x = match self.alignment {
            Alignment::Start => 0.0,
            Alignment::Center => (available_width - child.width).max(0.0) / 2.0,
            Alignment::End => (available_width - child.width).max(0.0),
        };
        let offset_y = match self.alignment {
            Alignment::Start => 0.0,
            Alignment::Center => (available_height - child.height).max(0.0) / 2.0,
            Alignment::End => (available_height - child.height).max(0.0),
        };
        state.child_bounds = Rect {
            x: self.margin.left + self.padding.left + offset_x,
            y: self.margin.top + self.padding.top + offset_y,
            width: child.width,
            height: child.height,
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
        let child_bounds = Rect {
            x: bounds.x + state.child_bounds.x,
            y: bounds.y + state.child_bounds.y,
            ..state.child_bounds
        };
        self.child.event(&mut state.child, event, child_bounds)
    }

    fn paint<'pass>(
        &self,
        state: &mut Self::State,
        context: PaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        if let Some(color) = self.background {
            let vertices = ColoredVertex::from_pixel_rect(
                context.bounds.x,
                context.bounds.y,
                context.bounds.x + context.bounds.width,
                context.bounds.y + context.bounds.height,
                color.0,
                context.gpu.surface_size().0 as f32,
                context.gpu.surface_size().1 as f32,
            );
            let buffer = state.background_buffer.get_or_insert_with(|| {
                gpu::create_colored_vertex_buffer(
                    context.gpu.device(),
                    &[ColoredVertex::default(); 6],
                )
            });
            context
                .gpu
                .queue()
                .write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
            pass.set_pipeline(&context.gpu.colored_quad_pipeline());
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..6, 0..1);
        }
        let child_bounds = Rect {
            x: context.bounds.x + state.child_bounds.x,
            y: context.bounds.y + state.child_bounds.y,
            ..state.child_bounds
        };
        self.child.paint(
            &mut state.child,
            PaintContext {
                gpu: context.gpu,
                text: context.text,
                bounds: child_bounds,
            },
            pass,
        );
    }
}
