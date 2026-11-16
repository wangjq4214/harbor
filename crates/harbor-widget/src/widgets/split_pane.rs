//! Two-child allocation and captured divider interaction, independent of application policy.
use std::{cell::Cell, rc::Rc};

use crate::{
    effects::CursorShape,
    input::{
        event::{PointerButton, PointerPhase, UiEvent},
        event_ctx::{EventCtx, EventHandled},
    },
    layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size},
    scene::primitive::{Color, Primitive},
    signal::Signal,
    text::TextMetrics,
    view::{AnyView, BuildCx, Component, View},
};

/// Direction of child placement (horizontal means left/right).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

/// Retained ratio; cloning shares intent without owning either child's lifetime.
#[derive(Clone)]
pub struct SplitController {
    ratio: Signal<f32>,
    gesture: Rc<Cell<Option<(u64, f32)>>>,
    hovered: Rc<Cell<bool>>,
}
impl SplitController {
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio: Signal::new(normalize(ratio)),
            gesture: Rc::default(),
            hovered: Rc::default(),
        }
    }
    pub fn ratio(&self) -> f32 {
        *self.ratio.read()
    }
    /// Ignore invalid values; finite intent is clamped to 5%..95%.
    pub fn set_ratio(&self, ratio: f32) {
        if ratio.is_finite() && self.ratio() != normalize(ratio) {
            self.ratio.set(normalize(ratio));
        }
    }
}
fn normalize(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(0.05, 0.95)
    } else {
        0.5
    }
}

/// Bounded two-child split. The 6dp divider shrinks with the parent; each side
/// receives at least 24dp along the split when possible, otherwise half the
/// remaining space. Compression never changes the stored ratio. Not a focus stop.
#[derive(Clone)]
pub struct SplitPane {
    axis: SplitAxis,
    controller: SplitController,
    children: [View; 2],
}
impl SplitPane {
    pub fn new(
        axis: SplitAxis,
        controller: SplitController,
        first: impl crate::IntoChildView,
        second: impl crate::IntoChildView,
    ) -> Self {
        Self {
            axis,
            controller,
            children: [first.into_child_view(), second.into_child_view()],
        }
    }
    fn coordinate(&self, point: Point) -> f32 {
        match self.axis {
            SplitAxis::Horizontal => point.x,
            SplitAxis::Vertical => point.y,
        }
    }
    fn size(&self, main: f32, cross: f32) -> Size {
        match self.axis {
            SplitAxis::Horizontal => Size::new(main, cross),
            SplitAxis::Vertical => Size::new(cross, main),
        }
    }
    fn parts(&self, rect: Rect) -> (f32, f32, f32) {
        let extent = self.coordinate(rect.max) - self.coordinate(rect.min);
        let gutter = extent.clamp(0.0, 6.0);
        let available = (extent - gutter).max(0.0);
        let minimum = 24.0_f32.min(available / 2.0);
        let first = (available * self.controller.ratio()).clamp(minimum, available - minimum);
        (first, gutter, available)
    }
    fn divider(&self, rect: Rect) -> Rect {
        let (first, gutter, _) = self.parts(rect);
        match self.axis {
            SplitAxis::Horizontal => Rect::from_min_size(
                Point::new(rect.min.x + first, rect.min.y),
                Size::new(gutter, rect.size().height),
            ),
            SplitAxis::Vertical => Rect::from_min_size(
                Point::new(rect.min.x, rect.min.y + first),
                Size::new(rect.size().width, gutter),
            ),
        }
    }
}
impl Component for SplitPane {
    fn build(&self, cx: &mut BuildCx) -> View {
        cx.track(&self.controller.ratio);
        View::new(self.clone(), self.children.to_vec(), None)
    }
}
impl AnyView for SplitPane {
    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        constraints.validate()?;
        if children.len() != 2 {
            return Err(LayoutError::InvalidChildIndex);
        }
        let size = constraints.fill_bounded(Size::ZERO);
        let rect = Rect::from_min_size(Point::ZERO, size);
        let (first, gutter, available) = self.parts(rect);
        let cross = match self.axis {
            SplitAxis::Horizontal => size.height,
            SplitAxis::Vertical => size.width,
        };
        children.measure(0, BoxConstraints::tight(self.size(first, cross)))?;
        children.measure(
            1,
            BoxConstraints::tight(self.size(available - first, cross)),
        )?;
        let origin = match self.axis {
            SplitAxis::Horizontal => Point::new(first + gutter, 0.0),
            SplitAxis::Vertical => Point::new(0.0, first + gutter),
        };
        Ok(ParentLayout {
            size,
            placements: vec![(0, Point::ZERO), (1, origin)],
            diagnostics: Vec::new(),
        })
    }
    fn paint_primitives(&self, rect: Rect, _: &TextMetrics) -> Vec<Primitive> {
        vec![Primitive::Quad {
            rect: self.divider(rect),
            color: Color {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 0.15,
            },
            corner_radius: 0.0,
        }]
    }
    fn pointer_cursor(&self) -> Option<CursorShape> {
        (self.controller.hovered.get() || self.controller.gesture.get().is_some()).then_some(
            match self.axis {
                SplitAxis::Horizontal => CursorShape::ResizeHorizontal,
                SplitAxis::Vertical => CursorShape::ResizeVertical,
            },
        )
    }
    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        let UiEvent::Pointer(pointer) = event else {
            return EventHandled::Ignored;
        };
        self.controller
            .hovered
            .set(self.divider(rect).contains(pointer.position));
        if let Some((id, grab)) = self.controller.gesture.get() {
            if id != pointer.pointer_id {
                return EventHandled::Ignored;
            }
            match pointer.phase {
                PointerPhase::Move => {
                    let (_, _, available) = self.parts(rect);
                    if available > 0.0 {
                        self.controller.set_ratio(
                            (self.coordinate(pointer.position) - self.coordinate(rect.min) - grab)
                                / available,
                        );
                    }
                    ctx.invalidate_paint();
                }
                PointerPhase::Up if pointer.button == PointerButton::Left => {
                    self.controller.gesture.set(None);
                    ctx.release_pointer(id);
                }
                PointerPhase::Cancel => {
                    self.controller.gesture.set(None);
                    ctx.release_pointer(id);
                }
                _ => {}
            }
            return EventHandled::Handled;
        }
        if pointer.phase == PointerPhase::Down
            && pointer.button == PointerButton::Left
            && self.divider(rect).contains(pointer.position)
        {
            let grab = self.coordinate(pointer.position) - self.coordinate(self.divider(rect).min);
            self.controller
                .gesture
                .set(Some((pointer.pointer_id, grab)));
            ctx.capture_pointer(pointer.pointer_id);
            return EventHandled::Handled;
        }
        EventHandled::Ignored
    }
    fn unmount(&self) {
        self.controller.gesture.set(None);
        self.controller.hovered.set(false);
    }
}
