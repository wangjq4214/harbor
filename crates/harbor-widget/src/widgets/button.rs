use crate::input::event::{Key, KeyboardEvent, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::signal::Signal;
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key as ViewKey, View};
use std::sync::Arc;

// ── Button Constants ────────────────────────────────────────────────────────

/// Horizontal padding added to both sides of the button label.
const HORIZONTAL_PADDING: f32 = 32.0;
/// Default button height in logical pixels.
const DEFAULT_HEIGHT: f32 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ButtonVisualState {
    Normal,
    Hovered,
    Pressed,
    Focused,
}

impl ButtonVisualState {
    fn background_color(&self) -> Color {
        match self {
            ButtonVisualState::Normal => Color {
                r: 0.25,
                g: 0.25,
                b: 0.25,
                a: 1.0,
            },
            ButtonVisualState::Hovered => Color {
                r: 0.35,
                g: 0.35,
                b: 0.35,
                a: 1.0,
            },
            ButtonVisualState::Pressed => Color {
                r: 0.15,
                g: 0.15,
                b: 0.15,
                a: 1.0,
            },
            ButtonVisualState::Focused => Color {
                r: 0.28,
                g: 0.28,
                b: 0.28,
                a: 1.0,
            },
        }
    }

    fn border_color(&self) -> Color {
        match self {
            ButtonVisualState::Focused => Color {
                r: 0.4,
                g: 0.6,
                b: 1.0,
                a: 1.0,
            },
            _ => Color {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 1.0,
            },
        }
    }
}

/// Button click callback type.
type OnClick = std::sync::Arc<dyn Fn(&mut EventCtx) + Send + Sync>;

// ── Button ──────────────────────────────────────────────────────────────────

/// A clickable, focusable button with label and onClick callback.
///
/// Visual states: Normal, Hovered, Pressed, Focused.
/// Activated by pointer click (Up after Down) or Enter/Space key.
#[derive(Clone)]
pub struct Button {
    label: String,
    // onClick is stored as Arc for Clone support.
    on_click: Option<OnClick>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Button {
            label: label.into(),
            on_click: None,
        }
    }

    pub fn on_click(mut self, handler: impl Fn(&mut EventCtx) + Send + Sync + 'static) -> Self {
        self.on_click = Some(std::sync::Arc::new(handler));
        self
    }

    fn render_view(&self, state: Signal<ButtonVisualState>) -> ButtonView {
        ButtonView {
            label: self.label.clone(),
            on_click: self.on_click.clone(),
            state,
        }
    }
}

impl Component for Button {
    fn build(&self, cx: &mut BuildCx) -> View {
        let state = cx.use_state(|| ButtonVisualState::Normal);
        View::new(self.render_view(state), vec![], None)
    }
}

/// The materialized button view. Configuration is copied from [`Button`] while
/// its visual state remains owned by the Fiber hook that built it.
#[derive(Clone)]
struct ButtonView {
    label: String,
    on_click: Option<OnClick>,
    state: Signal<ButtonVisualState>,
}

impl ButtonView {
    fn corner_radius() -> f32 {
        4.0
    }
}

impl AnyView for ButtonView {
    fn key(&self) -> Option<&ViewKey> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let label_width = self.label.len() as f32 * metrics.cell_width + HORIZONTAL_PADDING;
        let width = label_width.clamp(constraints.min.width, constraints.max.width);
        let height = DEFAULT_HEIGHT.clamp(constraints.min.height, constraints.max.height);
        constraints.constrain(Size::new(width, height))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        (
            self.intrinsic_size(constraints, metrics),
            vec![Point::ZERO; child_sizes.len()],
        )
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let state = *self.state.read();
        let bg = state.background_color();
        let border = state.border_color();

        let mut prims = vec![
            Primitive::Quad {
                rect,
                color: bg,
                corner_radius: Self::corner_radius(),
            },
            Primitive::Border {
                rect,
                width: 1.0,
                color: border,
                corner_radius: Self::corner_radius(),
            },
        ];

        // Render the button label as a Text primitive.
        if !self.label.is_empty() {
            let label_color = Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            };
            // Position the text centered within the button.
            let text_width = self.label.len() as f32 * metrics.cell_width;
            let origin = Point::new(
                rect.min.x + (rect.size().width - text_width).max(0.0) / 2.0,
                rect.min.y + (rect.size().height - DEFAULT_HEIGHT).max(0.0) / 2.0 + 2.0,
            );
            prims.push(Primitive::Text {
                text: Arc::from(self.label.as_str()),
                origin,
                color: label_color,
            });
        }

        prims
    }

    fn hit_test(&self, point: Point, rect: Rect) -> bool {
        rect.contains(point)
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        match event {
            UiEvent::Pointer(pe) => match pe.phase {
                PointerPhase::Down => {
                    self.state.set(ButtonVisualState::Pressed);
                    ctx.invalidate_paint();
                    ctx.capture_pointer(pe.pointer_id);
                    EventHandled::Handled
                }
                PointerPhase::Up => {
                    self.state.set(ButtonVisualState::Hovered);
                    ctx.invalidate_paint();
                    ctx.release_pointer(pe.pointer_id);
                    if let Some(ref cb) = self.on_click {
                        cb(ctx);
                    }
                    EventHandled::Handled
                }
                PointerPhase::Cancel => {
                    self.state.set(ButtonVisualState::Normal);
                    ctx.invalidate_paint();
                    ctx.release_pointer(pe.pointer_id);
                    EventHandled::Handled
                }
                PointerPhase::Move => {
                    let prev = *self.state.read();
                    // Stay Pressed during drag, otherwise track hover
                    if *self.state.read() != ButtonVisualState::Pressed {
                        self.state.set(ButtonVisualState::Hovered);
                    }
                    if *self.state.read() != prev {
                        ctx.invalidate_paint();
                    }
                    EventHandled::Handled
                }
                _ => EventHandled::Ignored,
            },
            UiEvent::Focus(fe) => match fe {
                crate::input::event::FocusEvent::Gained => {
                    self.state.set(ButtonVisualState::Focused);
                    ctx.invalidate_paint();
                    EventHandled::Handled
                }
                crate::input::event::FocusEvent::Lost => {
                    self.state.set(ButtonVisualState::Normal);
                    ctx.invalidate_paint();
                    EventHandled::Handled
                }
            },
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Enter | Key::Space,
                ..
            }) => {
                if let Some(ref cb) = self.on_click {
                    cb(ctx);
                }
                EventHandled::Handled
            }
            _ => EventHandled::Ignored,
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::input::event::{FocusEvent, PointerButton, PointerEvent};
    use crate::widgets::column::Column;
    use std::time::Instant;

    use super::*;

    fn test_view(button: Button) -> ButtonView {
        button.render_view(Signal::new(ButtonVisualState::Normal))
    }

    #[test]
    fn build_creates_visual_state_hook_and_private_view() {
        let mut cx = BuildCx::stub();
        let view = Button::new("OK").build(&mut cx);

        assert_eq!(cx.hooks.len(), 1);
        let (inner, children, _) = view.decompose();
        assert!(children.is_empty());
        assert!(inner.is_focusable());
    }

    #[test]
    fn private_view_handles_layout_hit_testing_and_painting() {
        let btn = test_view(Button::new("OK"));
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let constraints = BoxConstraints::tight(Size::new(50.0, 20.0));

        assert!(btn.is_focusable());
        assert!(btn.hit_test(Point::new(50.0, 16.0), rect));
        assert!(!btn.hit_test(Point::new(200.0, 16.0), rect));
        assert_eq!(
            btn.intrinsic_size(constraints, &crate::runtime::DEFAULT_TEXT_METRICS),
            Size::new(50.0, 20.0)
        );
        assert_eq!(
            btn.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS)
                .len(),
            3
        );
    }

    #[test]
    fn pointer_events_update_hook_state_and_invalidate_paint() {
        let btn = test_view(Button::new("OK"));
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let mut ctx = EventCtx::new();

        let down = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ));
        assert_eq!(
            btn.handle_event(&down, &mut ctx, rect),
            EventHandled::Handled
        );
        assert_eq!(*btn.state.read(), ButtonVisualState::Pressed);
        assert!(ctx.needs_paint());

        let move_event = UiEvent::Pointer(PointerEvent::new(
            Point::new(50.0, 16.0),
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ));
        btn.handle_event(&move_event, &mut ctx, rect);
        assert_eq!(*btn.state.read(), ButtonVisualState::Pressed);

        let cancel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Cancel,
            PointerButton::Left,
            0,
        ));
        btn.handle_event(&cancel, &mut ctx, rect);
        assert_eq!(*btn.state.read(), ButtonVisualState::Normal);
    }

    #[test]
    fn focus_and_hover_events_update_hook_state() {
        let btn = test_view(Button::new("OK"));
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let mut ctx = EventCtx::new();

        btn.handle_event(&UiEvent::Focus(FocusEvent::Gained), &mut ctx, rect);
        assert_eq!(*btn.state.read(), ButtonVisualState::Focused);
        btn.handle_event(&UiEvent::Focus(FocusEvent::Lost), &mut ctx, rect);
        assert_eq!(*btn.state.read(), ButtonVisualState::Normal);

        let move_event = UiEvent::Pointer(PointerEvent::new(
            Point::new(50.0, 16.0),
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ));
        btn.handle_event(&move_event, &mut ctx, rect);
        assert_eq!(*btn.state.read(), ButtonVisualState::Hovered);
    }

    #[test]
    fn button_hook_state_persists_across_parent_rebuild_and_subscribes_nested_fiber() {
        let mut runtime = crate::runtime::Runtime::new();
        runtime.set_root(Column::new().child(Button::new("OK")));
        runtime.update(Instant::now());

        let parent_id = runtime.root_id().unwrap();
        let button_id = runtime.arena().get(parent_id).unwrap().children[0];
        let state = runtime.arena().get(button_id).unwrap().hooks[0]
            .as_any_ref()
            .downcast_ref::<Signal<ButtonVisualState>>()
            .unwrap()
            .clone();

        state.set(ButtonVisualState::Pressed);
        let effects = runtime.update(Instant::now());

        assert!(effects.request_redraw, "the nested button Fiber subscribed");
        assert_eq!(
            runtime.arena().get(parent_id).unwrap().children[0],
            button_id
        );
        assert_eq!(runtime.arena().get(button_id).unwrap().hooks.len(), 1);
        assert_eq!(*state.read(), ButtonVisualState::Pressed);
    }

    #[test]
    fn activation_events_preserve_callback_api() {
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();
        let btn = test_view(Button::new("OK").on_click(move |_ctx| {
            clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let mut ctx = EventCtx::new();

        let up = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Up,
            PointerButton::Left,
            0,
        ));
        assert_eq!(btn.handle_event(&up, &mut ctx, rect), EventHandled::Handled);
        assert!(clicked.load(std::sync::atomic::Ordering::SeqCst));

        let escape = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Escape,
            modifiers: Default::default(),
        });
        assert_eq!(
            btn.handle_event(&escape, &mut ctx, rect),
            EventHandled::Ignored
        );
    }
}
