use crate::{BoxConstraints, LegacyPaintContext, PaintContext, Rect, Widget, WidgetEventResult};
use harbor_render::RenderEnvironment;

fn translated(parent: Rect, child: Rect) -> Rect {
    Rect {
        x: parent.x + child.x,
        y: parent.y + child.y,
        ..child
    }
}

pub struct Stack<Back, Front> {
    pub back: Back,
    pub front: Front,
}

impl<Back, Front> Stack<Back, Front> {
    pub fn new(back: Back, front: Front) -> Self {
        Self { back, front }
    }
}

pub struct StackState<Back, Front> {
    back: Back,
    front: Front,
    back_bounds: Rect,
    front_bounds: Rect,
}

impl<A, Back, Front> Widget<A> for Stack<Back, Front>
where
    Back: Widget<A>,
    Front: Widget<A>,
{
    type State = StackState<Back::State, Front::State>;

    fn create_state(&self) -> Self::State {
        StackState {
            back: self.back.create_state(),
            front: self.front.create_state(),
            back_bounds: Rect::default(),
            front_bounds: Rect::default(),
        }
    }

    fn layout(
        &self,
        state: &mut Self::State,
        environment: RenderEnvironment,
        constraints: BoxConstraints,
    ) -> Rect {
        let back = self.back.layout(&mut state.back, environment, constraints);
        let front = self
            .front
            .layout(&mut state.front, environment, constraints);
        let (width, height) =
            constraints.constrain(back.width.max(front.width), back.height.max(front.height));
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        state.back_bounds = bounds;
        state.front_bounds = bounds;
        bounds
    }

    fn event(
        &self,
        state: &mut Self::State,
        event: &winit::event::WindowEvent,
        bounds: Rect,
    ) -> WidgetEventResult<A> {
        let front = self.front.event(
            &mut state.front,
            event,
            translated(bounds, state.front_bounds),
        );
        if !matches!(front, WidgetEventResult::Ignored) {
            return front;
        }
        self.back.event(
            &mut state.back,
            event,
            translated(bounds, state.back_bounds),
        )
    }

    fn paint<'a>(&'a self, state: &'a mut Self::State, context: &mut PaintContext<'a>) {
        let bounds = context.bounds();
        context.with_bounds(translated(bounds, state.back_bounds), |context| {
            self.back.paint(&mut state.back, context);
        });
        context.with_bounds(translated(bounds, state.front_bounds), |context| {
            self.front.paint(&mut state.front, context);
        });
    }

    fn legacy_paint<'pass>(
        &self,
        state: &mut Self::State,
        context: LegacyPaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        self.back.legacy_paint(
            &mut state.back,
            LegacyPaintContext {
                gpu: context.gpu,
                text: &mut *context.text,
                bounds: translated(context.bounds, state.back_bounds),
            },
            pass,
        );
        self.front.legacy_paint(
            &mut state.front,
            LegacyPaintContext {
                gpu: context.gpu,
                text: &mut *context.text,
                bounds: translated(context.bounds, state.front_bounds),
            },
            pass,
        );
    }
}

pub struct Row<Start, End> {
    pub start: Start,
    pub end: End,
}

impl<Start, End> Row<Start, End> {
    pub fn new(start: Start, end: End) -> Self {
        Self { start, end }
    }
}

pub struct Column<Top, Bottom> {
    pub top: Top,
    pub bottom: Bottom,
}

impl<Top, Bottom> Column<Top, Bottom> {
    pub fn new(top: Top, bottom: Bottom) -> Self {
        Self { top, bottom }
    }
}

pub struct LinearState<First, Second> {
    first: First,
    second: Second,
    first_bounds: Rect,
    second_bounds: Rect,
}

fn child_constraints(constraints: BoxConstraints, horizontal: bool, main: f32) -> BoxConstraints {
    if horizontal {
        BoxConstraints {
            min_width: main,
            max_width: main,
            min_height: 0.0,
            max_height: constraints.max_height,
        }
    } else {
        BoxConstraints {
            min_width: 0.0,
            max_width: constraints.max_width,
            min_height: main,
            max_height: main,
        }
    }
}

fn natural_constraints(constraints: BoxConstraints, horizontal: bool) -> BoxConstraints {
    if horizontal {
        BoxConstraints {
            min_width: 0.0,
            max_width: constraints.max_width,
            min_height: 0.0,
            max_height: constraints.max_height,
        }
    } else {
        BoxConstraints {
            min_width: 0.0,
            max_width: constraints.max_width,
            min_height: 0.0,
            max_height: constraints.max_height,
        }
    }
}

fn main(bounds: Rect, horizontal: bool) -> f32 {
    if horizontal {
        bounds.width
    } else {
        bounds.height
    }
}

fn cross(bounds: Rect, horizontal: bool) -> f32 {
    if horizontal {
        bounds.height
    } else {
        bounds.width
    }
}

fn rect(width: f32, height: f32) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    }
}

macro_rules! impl_linear {
    ($type:ident, $First:ident, $Second:ident, $first:ident, $second:ident, $horizontal:expr) => {
        impl<A, $First, $Second> Widget<A> for $type<$First, $Second>
        where
            $First: Widget<A>,
            $Second: Widget<A>,
        {
            type State = LinearState<$First::State, $Second::State>;

            fn create_state(&self) -> Self::State {
                LinearState {
                    first: self.$first.create_state(),
                    second: self.$second.create_state(),
                    first_bounds: Rect::default(),
                    second_bounds: Rect::default(),
                }
            }

            fn layout(
                &self,
                state: &mut Self::State,
                environment: RenderEnvironment,
                constraints: BoxConstraints,
            ) -> Rect {
                let max_main = if $horizontal {
                    constraints.max_width
                } else {
                    constraints.max_height
                };
                let first_expands = self.$first.expands();
                let second_expands = self.$second.expands();
                let natural = natural_constraints(constraints, $horizontal);
                let mut first = if first_expands {
                    Rect::default()
                } else {
                    self.$first.layout(&mut state.first, environment, natural)
                };
                let mut second = if second_expands {
                    Rect::default()
                } else {
                    self.$second.layout(&mut state.second, environment, natural)
                };
                let remaining =
                    (max_main - main(first, $horizontal) - main(second, $horizontal)).max(0.0);
                let expanded = first_expands as u8 + second_expands as u8;
                if first_expands {
                    first = self.$first.layout(
                        &mut state.first,
                        environment,
                        child_constraints(constraints, $horizontal, remaining / expanded as f32),
                    );
                }
                if second_expands {
                    second = self.$second.layout(
                        &mut state.second,
                        environment,
                        child_constraints(constraints, $horizontal, remaining / expanded as f32),
                    );
                }
                let total_main = main(first, $horizontal) + main(second, $horizontal);
                let max_cross = cross(first, $horizontal).max(cross(second, $horizontal));
                let (width, height) = if $horizontal {
                    constraints.constrain(total_main, max_cross)
                } else {
                    constraints.constrain(max_cross, total_main)
                };
                state.first_bounds = rect(first.width, first.height);
                state.second_bounds = if $horizontal {
                    Rect {
                        x: first.width,
                        y: 0.0,
                        width: second.width,
                        height: second.height,
                    }
                } else {
                    Rect {
                        x: 0.0,
                        y: first.height,
                        width: second.width,
                        height: second.height,
                    }
                };
                rect(width, height)
            }

            fn event(
                &self,
                state: &mut Self::State,
                event: &winit::event::WindowEvent,
                bounds: Rect,
            ) -> WidgetEventResult<A> {
                let second = self.$second.event(
                    &mut state.second,
                    event,
                    translated(bounds, state.second_bounds),
                );
                if !matches!(second, WidgetEventResult::Ignored) {
                    return second;
                }
                self.$first.event(
                    &mut state.first,
                    event,
                    translated(bounds, state.first_bounds),
                )
            }

            fn paint<'a>(&'a self, state: &'a mut Self::State, context: &mut PaintContext<'a>) {
                let bounds = context.bounds();
                context.with_bounds(translated(bounds, state.first_bounds), |context| {
                    self.$first.paint(&mut state.first, context);
                });
                context.with_bounds(translated(bounds, state.second_bounds), |context| {
                    self.$second.paint(&mut state.second, context);
                });
            }

            fn legacy_paint<'pass>(
                &self,
                state: &mut Self::State,
                context: LegacyPaintContext<'_>,
                pass: &mut wgpu::RenderPass<'pass>,
            ) {
                self.$first.legacy_paint(
                    &mut state.first,
                    LegacyPaintContext {
                        gpu: context.gpu,
                        text: &mut *context.text,
                        bounds: translated(context.bounds, state.first_bounds),
                    },
                    pass,
                );
                self.$second.legacy_paint(
                    &mut state.second,
                    LegacyPaintContext {
                        gpu: context.gpu,
                        text: &mut *context.text,
                        bounds: translated(context.bounds, state.second_bounds),
                    },
                    pass,
                );
            }
        }
    };
}

impl_linear!(Row, Start, End, start, end, true);
impl_linear!(Column, Top, Bottom, top, bottom, false);

pub struct Expanded<W> {
    pub child: W,
}

impl<W> Expanded<W> {
    pub fn new(child: W) -> Self {
        Self { child }
    }
}

pub struct ExpandedState<S> {
    child: S,
}

impl<A, W> Widget<A> for Expanded<W>
where
    W: Widget<A>,
{
    type State = ExpandedState<W::State>;

    fn create_state(&self) -> Self::State {
        ExpandedState {
            child: self.child.create_state(),
        }
    }

    fn expands(&self) -> bool {
        true
    }

    fn layout(
        &self,
        state: &mut Self::State,
        environment: RenderEnvironment,
        constraints: BoxConstraints,
    ) -> Rect {
        let child = self
            .child
            .layout(&mut state.child, environment, constraints);
        rect(
            constraints.max_width.max(child.width),
            constraints.max_height.max(child.height),
        )
    }

    fn event(
        &self,
        state: &mut Self::State,
        event: &winit::event::WindowEvent,
        bounds: Rect,
    ) -> WidgetEventResult<A> {
        self.child.event(&mut state.child, event, bounds)
    }

    fn paint<'a>(&'a self, state: &'a mut Self::State, context: &mut PaintContext<'a>) {
        self.child.paint(&mut state.child, context);
    }

    fn legacy_paint<'pass>(
        &self,
        state: &mut Self::State,
        context: LegacyPaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        self.child.legacy_paint(&mut state.child, context, pass);
    }
}

pub struct ScrollView<W> {
    pub child: W,
}

impl<W> ScrollView<W> {
    pub fn new(child: W) -> Self {
        Self { child }
    }
}

pub struct ScrollViewState<S> {
    child: S,
    child_bounds: Rect,
    pub offset: f32,
}

impl<A, W> Widget<A> for ScrollView<W>
where
    W: Widget<A>,
{
    type State = ScrollViewState<W::State>;

    fn create_state(&self) -> Self::State {
        ScrollViewState {
            child: self.child.create_state(),
            child_bounds: Rect::default(),
            offset: 0.0,
        }
    }

    fn layout(
        &self,
        state: &mut Self::State,
        environment: RenderEnvironment,
        constraints: BoxConstraints,
    ) -> Rect {
        let child = self.child.layout(
            &mut state.child,
            environment,
            BoxConstraints {
                min_width: 0.0,
                max_width: constraints.max_width,
                min_height: 0.0,
                max_height: f32::INFINITY,
            },
        );
        let viewport = rect(
            child
                .width
                .clamp(constraints.min_width, constraints.max_width),
            child
                .height
                .clamp(constraints.min_height, constraints.max_height),
        );
        state.child_bounds = child;
        state.offset = state
            .offset
            .clamp(0.0, (child.height - viewport.height).max(0.0));
        viewport
    }

    fn event(
        &self,
        state: &mut Self::State,
        event: &winit::event::WindowEvent,
        bounds: Rect,
    ) -> WidgetEventResult<A> {
        if let winit::event::WindowEvent::MouseWheel { delta, .. } = event {
            let distance = match delta {
                winit::event::MouseScrollDelta::LineDelta(_, y) => *y * 20.0,
                winit::event::MouseScrollDelta::PixelDelta(position) => position.y as f32,
            };
            state.offset = (state.offset - distance).max(0.0);
            return WidgetEventResult::Handled;
        }
        self.child.event(
            &mut state.child,
            event,
            Rect {
                x: bounds.x,
                y: bounds.y - state.offset,
                ..state.child_bounds
            },
        )
    }

    fn paint<'a>(&'a self, state: &'a mut Self::State, context: &mut PaintContext<'a>) {
        let viewport = context.bounds();
        let child_bounds = Rect {
            x: viewport.x,
            y: viewport.y - state.offset,
            ..state.child_bounds
        };
        context.with_clip(viewport, |context| {
            context.with_bounds(child_bounds, |context| {
                self.child.paint(&mut state.child, context);
            });
        });
    }

    fn legacy_paint<'pass>(
        &self,
        state: &mut Self::State,
        context: LegacyPaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        let surface = context.gpu.surface_size();
        let left = context.bounds.x.max(0.0).floor() as u32;
        let top = context.bounds.y.max(0.0).floor() as u32;
        let width = context
            .bounds
            .width
            .min(surface.0.saturating_sub(left) as f32)
            .max(0.0)
            .floor() as u32;
        let height = context
            .bounds
            .height
            .min(surface.1.saturating_sub(top) as f32)
            .max(0.0)
            .floor() as u32;
        if width == 0 || height == 0 {
            return;
        }
        pass.set_scissor_rect(left, top, width, height);
        self.child.legacy_paint(
            &mut state.child,
            LegacyPaintContext {
                gpu: context.gpu,
                text: context.text,
                bounds: Rect {
                    x: context.bounds.x,
                    y: context.bounds.y - state.offset,
                    ..state.child_bounds
                },
            },
            pass,
        );
        pass.set_scissor_rect(0, 0, surface.0, surface.1);
    }
}
