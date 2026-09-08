use crate::effects::CursorShape;
use crate::input::event::{PointerBoundaryKind, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::view::{AnyView, BuildCx, Component, View};
use std::sync::Arc;

type BoundaryCallback = Arc<dyn Fn(&mut EventCtx) + Send + Sync>;

/// A pointer-boundary wrapper. It deliberately has no click or capture semantics.
#[derive(Clone)]
pub struct MouseRegion {
    cursor: Option<CursorShape>,
    on_enter: Option<BoundaryCallback>,
    on_exit: Option<BoundaryCallback>,
    child: Option<View>,
}

impl MouseRegion {
    pub fn new() -> Self {
        Self {
            cursor: None,
            on_enter: None,
            on_exit: None,
            child: None,
        }
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }

    pub fn cursor(mut self, cursor: CursorShape) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn on_enter(mut self, callback: impl Fn(&mut EventCtx) + Send + Sync + 'static) -> Self {
        self.on_enter = Some(Arc::new(callback));
        self
    }

    pub fn on_exit(mut self, callback: impl Fn(&mut EventCtx) + Send + Sync + 'static) -> Self {
        self.on_exit = Some(Arc::new(callback));
        self
    }
}

impl Default for MouseRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for MouseRegion {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for MouseRegion {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("MouseRegion", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for MouseRegion {
    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &crate::text::TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(child_sizes.first().copied().unwrap_or(Size::ZERO));
        (size, vec![Point::ZERO; child_sizes.len()])
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        match event {
            UiEvent::PointerBoundary(boundary) => match boundary.kind {
                PointerBoundaryKind::Enter => {
                    if let Some(callback) = &self.on_enter {
                        callback(ctx);
                    }
                    EventHandled::Handled
                }
                PointerBoundaryKind::Leave => {
                    if let Some(callback) = &self.on_exit {
                        callback(ctx);
                    }
                    EventHandled::Handled
                }
            },
            _ => EventHandled::Ignored,
        }
    }

    fn pointer_cursor(&self) -> Option<CursorShape> {
        self.cursor
    }

    fn accepts_pointer_boundaries(&self) -> bool {
        true
    }
}
