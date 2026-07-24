use crate::input::event::{Key, KeyboardEvent, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::view::{AnyView, BuildCx, Component, Key as ViewKey, View};

/// A container that manages Tab/Shift+Tab focus traversal within its subtree.
///
/// When `modal` is true, events targeting widgets outside this scope are
/// blocked during the capture phase.
#[derive(Clone)]
pub struct FocusScope {
    pub modal: bool,
    children: Vec<View>,
}

impl Default for FocusScope {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusScope {
    pub fn new() -> Self {
        FocusScope {
            modal: false,
            children: vec![],
        }
    }

    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    pub fn child(mut self, child: impl Component + 'static) -> Self {
        let mut cx = BuildCx::stub();
        self.children.push(child.build(&mut cx));
        self
    }
}

impl Component for FocusScope {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for FocusScope {
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
        // FocusScope delegates to children — it's a passthrough container
        let child_size = self
            .children
            .first()
            .map(|_| Size::new(constraints.max.width, constraints.max.height))
            .unwrap_or(Size::ZERO);
        constraints.constrain(child_size)
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        let own = constraints.constrain(Size::new(constraints.max.width, constraints.max.height));
        let positions = vec![Point::ZERO; child_sizes.len()];
        (own, positions)
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        match event {
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Tab,
                modifiers,
            }) => {
                ctx.navigate_focus(!modifiers.shift);
                EventHandled::Handled
            }
            _ => EventHandled::Ignored,
        }
    }

    fn is_modal_scope(&self) -> bool {
        self.modal
    }
}

#[cfg(test)]
mod tests {
    use crate::input::event::Modifiers;

    use super::*;

    #[test]
    fn focus_scope_not_modal_by_default() {
        let scope = FocusScope::new();
        assert!(!scope.is_modal_scope());
    }

    #[test]
    fn focus_scope_modal_flag() {
        let scope = FocusScope::new().modal(true);
        assert!(scope.is_modal_scope());
    }

    #[test]
    fn focus_scope_build() {
        let scope = FocusScope::new().modal(true);
        let mut cx = BuildCx::stub();
        let view = scope.build(&mut cx);
        // Verify the view can be decomposed
        let (inner, children, _key) = view.decompose();
        assert!(children.is_empty());
        assert!(inner.is_modal_scope());
    }

    #[test]
    fn focus_scope_handles_tab() {
        let scope = FocusScope::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Default::default(),
        });
        let mut ctx = EventCtx::new();
        let result = scope.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
    }

    #[test]
    fn focus_scope_ignores_other_keys() {
        let scope = FocusScope::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Default::default(),
        });
        let mut ctx = EventCtx::new();
        let result = scope.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Ignored);
    }

    #[test]
    fn focus_scope_handles_shift_tab_for_backward_navigation() {
        let scope = FocusScope::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        });
        let mut ctx = EventCtx::new();
        ctx.set_current_fiber(dummy_fiber_id());
        let result = scope.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        // Command recorded as NavigateFocus with forward=false
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            crate::input::event_ctx::EventCommand::NavigateFocus { forward, .. } => {
                assert!(!forward, "Shift+Tab should navigate backward");
            }
            _ => panic!("expected NavigateFocus command"),
        }
    }

    #[test]
    fn focus_scope_tab_forward_navigation() {
        let scope = FocusScope::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0));
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        });
        let mut ctx = EventCtx::new();
        ctx.set_current_fiber(dummy_fiber_id());
        let result = scope.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        let cmds = ctx.take_commands();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            crate::input::event_ctx::EventCommand::NavigateFocus { forward, .. } => {
                assert!(*forward, "Tab should navigate forward");
            }
            _ => panic!("expected NavigateFocus command"),
        }
    }

    #[test]
    fn focus_scope_intrinsic_size_with_child() {
        use crate::widgets::sized_box::SizedBox;
        let scope = FocusScope::new().child(SizedBox::new(Size::new(100.0, 50.0)));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = scope.intrinsic_size(constraints);
        // FocusScope fills available max space
        assert_eq!(size, Size::new(800.0, 600.0));
    }

    #[test]
    fn focus_scope_intrinsic_size_without_child() {
        let scope = FocusScope::new();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = scope.intrinsic_size(constraints);
        // Without a child, FocusScope reports zero
        assert_eq!(size, Size::ZERO);
    }

    #[test]
    fn focus_scope_layout_children_with_child() {
        let scope = FocusScope::new();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let child_sizes = vec![Size::new(100.0, 50.0)];
        let (own, positions) = scope.layout_children(constraints, &child_sizes);
        assert_eq!(own, Size::new(800.0, 600.0));
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], Point::ZERO);
    }

    #[test]
    fn focus_scope_layout_children_empty() {
        let scope = FocusScope::new();
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let (own, positions) = scope.layout_children(constraints, &[]);
        assert_eq!(own, Size::new(800.0, 600.0));
        assert!(positions.is_empty());
    }

    fn dummy_fiber_id() -> crate::fiber::FiberId {
        use crate::fiber::{Fiber, FiberArena};
        let mut arena = FiberArena::new();
        arena.insert(Fiber::new(None, std::any::TypeId::of::<()>(), None))
    }
}
