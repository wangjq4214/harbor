use crate::input::event::{Key, KeyboardEvent, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::view::{AnyView, BuildCx, Component, Key as ViewKey, View};
use std::cell::Cell;

// ── Button Constants ────────────────────────────────────────────────────────

/// Estimated pixel width per character for button sizing.
const CHAR_WIDTH_ESTIMATE: f32 = 10.0;
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
    state: Cell<ButtonVisualState>,
    children: Vec<View>,
    // onClick is stored as Arc for Clone support.
    // During build, the closure is wrapped and stored in View.
    on_click: Option<OnClick>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Button {
            label: label.into(),
            state: Cell::new(ButtonVisualState::Normal),
            children: vec![],
            on_click: None,
        }
    }

    pub fn on_click(mut self, handler: impl Fn(&mut EventCtx) + Send + Sync + 'static) -> Self {
        self.on_click = Some(std::sync::Arc::new(handler));
        self
    }

    fn corner_radius() -> f32 {
        4.0
    }
}

impl Component for Button {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for Button {
    fn key(&self) -> Option<&ViewKey> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn build(self: Box<Self>, _cx: &mut BuildCx) -> View {
        View::new(*self, vec![], None)
    }

    fn intrinsic_size(&self, constraints: BoxConstraints) -> Size {
        let label_width = self.label.len() as f32 * CHAR_WIDTH_ESTIMATE + HORIZONTAL_PADDING;
        let width = label_width.clamp(constraints.min.width, constraints.max.width);
        let height = DEFAULT_HEIGHT.clamp(constraints.min.height, constraints.max.height);
        constraints.constrain(Size::new(width, height))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        (
            self.intrinsic_size(constraints),
            vec![Point::ZERO; child_sizes.len()],
        )
    }

    fn paint_primitives(&self, rect: Rect) -> Vec<Primitive> {
        let state = self.state.get();
        let bg = state.background_color();
        let border = state.border_color();
        vec![
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
        ]
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
                    let prev = self.state.get();
                    // Stay Pressed during drag, otherwise track hover
                    if self.state.get() != ButtonVisualState::Pressed {
                        self.state.set(ButtonVisualState::Hovered);
                    }
                    if self.state.get() != prev {
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
    use crate::input::event::{PointerButton, PointerEvent};

    use super::*;

    #[test]
    fn button_is_focusable() {
        let btn = Button::new("OK");
        assert!(btn.is_focusable());
    }

    #[test]
    fn button_hit_test() {
        let btn = Button::new("OK");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        assert!(btn.hit_test(Point::new(50.0, 16.0), rect));
        assert!(!btn.hit_test(Point::new(200.0, 16.0), rect));
    }

    #[test]
    fn button_intrinsic_size() {
        let btn = Button::new("Hello");
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = btn.intrinsic_size(constraints);
        assert!(size.width > 0.0);
        assert_eq!(size.height, 32.0);
    }

    #[test]
    fn button_paint_produces_quad_and_border() {
        let btn = Button::new("OK");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let prims = btn.paint_primitives(rect);
        assert_eq!(prims.len(), 2);
        match &prims[0] {
            Primitive::Quad { .. } => {}
            _ => panic!("expected Quad"),
        }
        match &prims[1] {
            Primitive::Border { .. } => {}
            _ => panic!("expected Border"),
        }
    }

    #[test]
    fn button_click_fires_on_pointer_up() {
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();
        let btn = Button::new("OK").on_click(move |_ctx| {
            clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::Up,
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        let result = btn.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        assert!(clicked.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn button_enter_key_activates() {
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();
        let btn = Button::new("OK").on_click(move |_ctx| {
            clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Default::default(),
        });
        let mut ctx = EventCtx::new();
        let result = btn.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        assert!(clicked.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn button_space_key_activates() {
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();
        let btn = Button::new("OK").on_click(move |_ctx| {
            clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Space,
            modifiers: Default::default(),
        });
        let mut ctx = EventCtx::new();
        let result = btn.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        assert!(clicked.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn button_ignores_random_key() {
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();
        let btn = Button::new("OK").on_click(move |_ctx| {
            clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Escape,
            modifiers: Default::default(),
        });
        let mut ctx = EventCtx::new();
        let result = btn.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Ignored);
        assert!(!clicked.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn button_drag_maintains_pressed_state() {
        let btn = Button::new("OK");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));

        // Down sets Pressed
        let event_down = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        btn.handle_event(&event_down, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Pressed);
        ctx.take_commands(); // consume commands

        // Move while Pressed — stays Pressed
        let event_move = UiEvent::Pointer(PointerEvent::new(
            Point::new(50.0, 16.0),
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ));
        btn.handle_event(&event_move, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Pressed);
    }

    #[test]
    fn button_focus_gained_sets_focused_state() {
        let btn = Button::new("OK");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));

        let event = UiEvent::Focus(crate::input::event::FocusEvent::Gained);
        let mut ctx = EventCtx::new();
        btn.handle_event(&event, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Focused);
    }

    #[test]
    fn button_focus_lost_resets_to_normal() {
        let btn = Button::new("OK");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));

        // First gain focus
        let event_gain = UiEvent::Focus(crate::input::event::FocusEvent::Gained);
        let mut ctx = EventCtx::new();
        btn.handle_event(&event_gain, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Focused);
        ctx.take_commands();

        // Then lose focus
        let event_lost = UiEvent::Focus(crate::input::event::FocusEvent::Lost);
        btn.handle_event(&event_lost, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Normal);
    }

    #[test]
    fn button_move_sets_hovered_when_not_pressed() {
        let btn = Button::new("OK");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));

        // Move sets Hovered when not pressed
        let event = UiEvent::Pointer(PointerEvent::new(
            Point::new(50.0, 16.0),
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        btn.handle_event(&event, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Hovered);
    }

    #[test]
    fn button_pointer_cancel_resets_to_normal() {
        let btn = Button::new("OK");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 32.0));

        // Down -> Pressed
        let event_down = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Down,
            PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        btn.handle_event(&event_down, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Pressed);
        ctx.take_commands();

        // Cancel -> Normal
        let event_cancel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Cancel,
            PointerButton::Left,
            0,
        ));
        btn.handle_event(&event_cancel, &mut ctx, rect);
        assert_eq!(btn.state.get(), ButtonVisualState::Normal);
    }

    #[test]
    fn button_intrinsic_size_under_tight_constraints() {
        let btn = Button::new("Hello");
        let constraints = BoxConstraints::tight(Size::new(50.0, 20.0));
        let size = btn.intrinsic_size(constraints);
        // Button intrinsic size is clamped to constraints
        assert_eq!(size.width, 50.0);
        assert_eq!(size.height, 20.0);
    }

    #[test]
    fn button_layout_children_delegates_to_intrinsic_size() {
        let btn = Button::new("Hello");
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (own, positions) = btn.layout_children(constraints, &[]);
        let intrinsic = btn.intrinsic_size(constraints);
        assert_eq!(own, intrinsic);
        assert!(positions.is_empty());
    }
}
