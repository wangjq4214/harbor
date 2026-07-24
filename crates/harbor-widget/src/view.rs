use crate::fiber::FiberId;
use crate::input::event::UiEvent;
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::Primitive;
use crate::signal::{Hook, Signal};
use std::any::TypeId;
use std::sync::Arc;

// ── Key ─────────────────────────────────────────────────────────────────────

/// A stable identity marker for list reconciliation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key(String);

impl Key {
    pub fn new(s: impl Into<String>) -> Self {
        Key(s.into())
    }
}

// ── BuildCx ─────────────────────────────────────────────────────────────────

/// Per-build context for hook creation and Signal tracking.
pub struct BuildCx {
    pub(crate) current_fiber: Option<FiberId>,
    pub(crate) hooks: Vec<Box<dyn Hook>>,
    pub(crate) hook_index: usize,
}

impl BuildCx {
    /// Creates a stub BuildCx for pre-building Views outside the Runtime.
    ///
    /// Views built with a stub context cannot use hooks.
    pub fn stub() -> Self {
        BuildCx {
            current_fiber: None,
            hooks: Vec::new(),
            hook_index: 0,
        }
    }

    /// Returns a Signal for state of type `T`.
    ///
    /// On first call (for a given hook index), creates a new Signal
    /// initialized with `init()`. On subsequent rebuilds, returns the
    /// existing Signal at the same hook index.
    ///
    /// # Panics
    ///
    /// Panics if called with a different `T` than the previous build for
    /// the same hook index.
    pub fn use_state<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> Signal<T> {
        let signal = if self.hook_index < self.hooks.len() {
            let hook = &self.hooks[self.hook_index];
            if let Some(s) = hook.as_any_ref().downcast_ref::<Signal<T>>() {
                s.clone()
            } else {
                panic!(
                    "hook type mismatch at index {} (expected {})",
                    self.hook_index,
                    std::any::type_name::<T>(),
                );
            }
        } else {
            let s = Signal::new(init());
            self.hooks.push(Box::new(s.clone()));
            s
        };
        self.hook_index += 1;
        // Subscribe current fiber to this signal so it is marked dirty on writes
        if let Some(fid) = self.current_fiber {
            signal.subscribe(fid);
        }
        signal
    }
}

// ── Component ────────────────────────────────────────────────────────────────

/// A user-facing builder of immutable Views.
///
/// Implementations produce a `View` tree describing the widget's appearance.
/// State is managed via `BuildCx::use_state` and stored in Fibers, not in
/// the Component struct itself.
pub trait Component {
    fn build(&self, cx: &mut BuildCx) -> View;
}

// ── AnyView ─────────────────────────────────────────────────────────────────

/// Internal type-erased View capability.
///
/// Each concrete widget type provides an AnyView implementation that stores
/// configuration and can be used for layout and rebuild.
#[allow(dead_code)]
pub(crate) trait AnyView: 'static {
    /// Optional key for list reconciliation.
    fn key(&self) -> Option<&Key>;

    /// The TypeId of the concrete implementation, used for reconciliation.
    fn widget_type(&self) -> TypeId;

    /// Rebuilds this view, returning a new View with potentially updated
    /// children. The returned children are reconciled against existing
    /// child Fibers by the runtime.
    fn build(self: Box<Self>, cx: &mut BuildCx) -> View;

    /// Computes the intrinsic size given layout constraints.
    fn intrinsic_size(&self, constraints: BoxConstraints) -> Size;

    /// Computes the layout of this widget given child intrinsic sizes.
    /// Returns own size and child origins (relative to self).
    /// Default: positions all children at origin with own size from intrinsic_size.
    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
    ) -> (Size, Vec<Point>) {
        let _ = child_sizes;
        let size = self.intrinsic_size(constraints);
        let positions = vec![Point::ZERO; child_sizes.len()];
        (size, positions)
    }

    /// Returns Primitives to draw for this widget given its layout rect.
    /// Default: no primitives (invisible).
    fn paint_primitives(&self, rect: Rect) -> Vec<Primitive> {
        let _ = rect;
        vec![]
    }

    /// Returns true if the point (in widget-local coordinates) is inside
    /// the widget's hit-testable region.
    /// Default: point-in-rect test.
    fn hit_test(&self, point: Point, rect: Rect) -> bool {
        rect.contains(point)
    }

    /// Handles an input event delivered to this widget.
    /// Returns `EventHandled::Handled` if consumed.
    /// Default: ignores all events.
    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        let _ = (event, ctx, rect);
        EventHandled::Ignored
    }

    /// Whether this widget can receive focus via Tab navigation.
    /// Default: false.
    fn is_focusable(&self) -> bool {
        false
    }

    /// Whether this widget is a modal scope — events targeting widgets outside
    /// its subtree should be blocked.
    /// Default: false.
    fn is_modal_scope(&self) -> bool {
        false
    }
}

// ── View ────────────────────────────────────────────────────────────────────

/// An opaque, discardable UI description with optional Key.
///
/// Produced by `Component::build` and consumed by the reconciliation system.
#[derive(Clone)]
pub struct View {
    pub(crate) inner: Arc<dyn AnyView>,
    explicit_key: Option<Key>,
    pub(crate) children: Vec<View>,
}

impl View {
    /// Creates a new View wrapping the given AnyView implementation.
    // AnyView is pub(crate); private_bounds is intentional — this is the
    // public API surface for crate-internal widget types.
    #[allow(private_bounds)]
    pub fn new(inner: impl AnyView, children: Vec<View>, key: Option<Key>) -> Self {
        View {
            inner: Arc::new(inner),
            explicit_key: key,
            children,
        }
    }

    /// Returns the key for this view, checking the explicit key first,
    /// then falling back to the inner view's key.
    pub fn key(&self) -> Option<&Key> {
        self.explicit_key.as_ref().or_else(|| self.inner.key())
    }

    /// Returns the TypeId of the widget that produced this view.
    pub fn widget_type(&self) -> TypeId {
        self.inner.widget_type()
    }

    /// Consumes the View and returns its components.
    pub(crate) fn decompose(self) -> (Arc<dyn AnyView>, Vec<View>, Option<Key>) {
        (self.inner, self.children, self.explicit_key)
    }
}

// Re-export SizedBox from widgets module
pub use crate::widgets::sized_box::SizedBox;

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[derive(Clone)]
    struct KeyedTestView {
        inner_key: Option<Key>,
    }

    impl AnyView for KeyedTestView {
        fn key(&self) -> Option<&Key> {
            self.inner_key.as_ref()
        }

        fn widget_type(&self) -> TypeId {
            TypeId::of::<Self>()
        }

        fn build(self: Box<Self>, _cx: &mut BuildCx) -> View {
            View::new(
                KeyedTestView {
                    inner_key: self.inner_key.clone(),
                },
                vec![],
                None,
            )
        }

        fn intrinsic_size(&self, constraints: BoxConstraints) -> Size {
            constraints.constrain(Size::new(1.0, 1.0))
        }
    }

    #[test]
    fn key_equality() {
        let k1 = Key::new("foo");
        let k2 = Key::new("foo");
        let k3 = Key::new("bar");
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn view_key_explicit() {
        use crate::widgets::sized_box::SizedBox;
        let sb = SizedBox::new(Size::new(10.0, 10.0));
        let view = View::new(sb, vec![], Some(Key::new("my_key")));
        assert_eq!(view.key(), Some(&Key::new("my_key")));
    }

    #[test]
    fn view_key_fallback_to_inner() {
        use crate::widgets::sized_box::SizedBox;
        let sb = SizedBox::new(Size::new(10.0, 10.0));
        // SizedBox has no key, so no fallback
        let view = View::new(sb, vec![], None);
        assert_eq!(view.key(), None);
    }

    #[test]
    fn view_key_prefers_explicit_over_inner_key() {
        let explicit = Key::new("explicit");
        let inner = Key::new("inner");
        let view = View::new(
            KeyedTestView {
                inner_key: Some(inner),
            },
            vec![],
            Some(explicit.clone()),
        );
        assert_eq!(view.key(), Some(&explicit));
    }

    #[test]
    fn view_key_falls_back_to_inner_key() {
        let inner = Key::new("inner");
        let view = View::new(
            KeyedTestView {
                inner_key: Some(inner.clone()),
            },
            vec![],
            None,
        );
        assert_eq!(view.key(), Some(&inner));
    }

    #[test]
    fn view_widget_type() {
        use crate::widgets::sized_box::SizedBox;
        let sb1 = SizedBox::new(Size::new(10.0, 10.0));
        let sb2 = SizedBox::new(Size::new(20.0, 20.0));
        let view1 = View::new(sb1, vec![], None);
        let view2 = View::new(sb2, vec![], None);
        // Same widget type
        assert_eq!(view1.widget_type(), view2.widget_type());
    }

    #[test]
    fn use_state_persists_across_rebuilds() {
        let hooks: Vec<Box<dyn Hook>> = vec![];
        let mut cx = BuildCx {
            current_fiber: None,
            hooks,
            hook_index: 0,
        };

        // First build: creates a new signal
        let s1 = cx.use_state(|| 42u32);
        assert_eq!(*s1.read(), 42);
        s1.set(100);

        // Second build (simulated): hooks are preserved
        let mut cx2 = BuildCx {
            current_fiber: None,
            hooks: cx.hooks,
            hook_index: 0,
        };
        let s2 = cx2.use_state(|| 0u32); // init is ignored — existing signal used
        assert_eq!(*s2.read(), 100); // preserved value
        assert_eq!(cx2.hook_index, 1);
    }

    #[test]
    fn use_state_multiple_hooks() {
        let mut cx = BuildCx {
            current_fiber: None,
            hooks: vec![],
            hook_index: 0,
        };

        let s1 = cx.use_state(|| "hello".to_string());
        let s2 = cx.use_state(|| 42u32);
        assert_eq!(*s1.read(), "hello");
        assert_eq!(*s2.read(), 42);
        assert_eq!(cx.hook_index, 2);
    }

    #[test]
    #[should_panic(expected = "hook type mismatch")]
    fn use_state_type_mismatch_panics() {
        let mut cx = BuildCx {
            current_fiber: None,
            hooks: vec![],
            hook_index: 0,
        };

        // First build with u32
        let _s1 = cx.use_state(|| 42u32);

        // Second build tries to read it as String
        let mut cx2 = BuildCx {
            current_fiber: None,
            hooks: cx.hooks,
            hook_index: 0,
        };
        let _s2 = cx2.use_state(|| "oops".to_string());
    }

    #[test]
    fn sized_box_build() {
        let sized_box = SizedBox::new(Size::new(100.0, 50.0));
        let mut cx = BuildCx::stub();
        let view = sized_box.build(&mut cx);
        assert_eq!(view.children.len(), 0);
    }

    #[test]
    fn anyview_intrinsic_size() {
        use crate::widgets::sized_box::SizedBox;
        let sb = SizedBox::new(Size::new(100.0, 50.0));
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = sb.intrinsic_size(constraints);
        assert_eq!(size, Size::new(100.0, 50.0));
    }

    #[test]
    fn anyview_intrinsic_size_clamped() {
        use crate::widgets::sized_box::SizedBox;
        let sb = SizedBox::new(Size::new(1000.0, 50.0));
        let constraints = BoxConstraints::tight(Size::new(500.0, 500.0));
        let size = sb.intrinsic_size(constraints);
        // Both dimensions clamped to tight 500x500 constraint
        assert_eq!(size, Size::new(500.0, 500.0));
    }
}
