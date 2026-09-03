//! Two-child split layout container with interactive sash divider.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use harbor_types::SplitDirection;

use crate::effects::CursorShape;
use crate::input::event::{PointerButton, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// Default sash divider thickness in logical pixels.
pub const DEFAULT_SASH_THICKNESS: f32 = 4.0;
/// Default minimum pane size in logical pixels to prevent complete collapse.
pub const DEFAULT_MIN_PANE_SIZE: f32 = 32.0;
/// Default sash background color (subtle border gray).
pub const DEFAULT_SASH_COLOR: Color = Color {
    r: 0.2,
    g: 0.22,
    b: 0.25,
    a: 1.0,
};
/// Hit test padding around the sash to make dragging easy.
const SASH_HIT_MARGIN: f32 = 4.0;

/// Callback invoked when the user drags the sash divider to adjust the split ratio.
pub type OnResize = Arc<dyn Fn(f32) + Send + Sync>;

/// Two-child split layout container supporting horizontal and vertical division.
#[derive(Clone)]
pub struct SplitContainer {
    pub direction: SplitDirection,
    pub ratio: f32,
    pub sash_thickness: f32,
    pub min_pane_size: f32,
    pub sash_color: Color,
    first: Option<View>,
    second: Option<View>,
    on_resize: Option<OnResize>,
    dragging: Arc<AtomicBool>,
}

impl SplitContainer {
    /// Creates a new split container with the given orientation and fractional ratio (0.0..1.0).
    pub fn new(direction: SplitDirection, ratio: f32) -> Self {
        Self {
            direction,
            ratio: ratio.clamp(0.05, 0.95),
            sash_thickness: DEFAULT_SASH_THICKNESS,
            min_pane_size: DEFAULT_MIN_PANE_SIZE,
            sash_color: DEFAULT_SASH_COLOR,
            first: None,
            second: None,
            on_resize: None,
            dragging: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Sets the first (left or top) child view.
    pub fn first(mut self, child: impl Component + 'static) -> Self {
        let mut cx = BuildCx::stub();
        self.first = Some(child.build(&mut cx));
        self
    }

    /// Sets the second (right or bottom) child view.
    pub fn second(mut self, child: impl Component + 'static) -> Self {
        let mut cx = BuildCx::stub();
        self.second = Some(child.build(&mut cx));
        self
    }

    /// Sets raw `View` children directly.
    pub fn views(mut self, first: View, second: View) -> Self {
        self.first = Some(first);
        self.second = Some(second);
        self
    }

    /// Sets the sash divider thickness.
    pub fn sash_thickness(mut self, thickness: f32) -> Self {
        self.sash_thickness = thickness.max(1.0);
        self
    }

    /// Sets the minimum child pane size.
    pub fn min_pane_size(mut self, size: f32) -> Self {
        self.min_pane_size = size.max(1.0);
        self
    }

    /// Sets the sash divider color.
    pub fn sash_color(mut self, color: Color) -> Self {
        self.sash_color = color;
        self
    }

    /// Sets a callback for split ratio adjustments.
    pub fn on_resize(mut self, callback: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_resize = Some(Arc::new(callback));
        self
    }

    /// Computes the sash divider rect given total container bounds.
    pub fn sash_rect(&self, total_size: Size) -> Rect {
        let total = match self.direction {
            SplitDirection::Horizontal => total_size.width,
            SplitDirection::Vertical => total_size.height,
        };

        let sash = self.sash_thickness;
        let available = (total - sash).max(0.0);
        let min_s = self.min_pane_size;

        let max_first = (available - min_s).max(min_s);
        let first_len = (available * self.ratio).clamp(min_s.min(max_first), max_first);

        match self.direction {
            SplitDirection::Horizontal => Rect::from_min_size(
                Point::new(first_len, 0.0),
                Size::new(sash, total_size.height),
            ),
            SplitDirection::Vertical => Rect::from_min_size(
                Point::new(0.0, first_len),
                Size::new(total_size.width, sash),
            ),
        }
    }

    /// Checks if a point is within the sash's hit-testable area.
    pub fn is_in_sash_hit(&self, point: Point, total_size: Size) -> bool {
        let base = self.sash_rect(total_size);
        let hit_rect = match self.direction {
            SplitDirection::Horizontal => Rect::from_min_size(
                Point::new(base.min.x - SASH_HIT_MARGIN, base.min.y),
                Size::new(
                    base.size().width + 2.0 * SASH_HIT_MARGIN,
                    base.size().height,
                ),
            ),
            SplitDirection::Vertical => Rect::from_min_size(
                Point::new(base.min.x, base.min.y - SASH_HIT_MARGIN),
                Size::new(
                    base.size().width,
                    base.size().height + 2.0 * SASH_HIT_MARGIN,
                ),
            ),
        };
        hit_rect.contains(point)
    }
}

impl Component for SplitContainer {
    fn build(&self, _cx: &mut BuildCx) -> View {
        let mut children = Vec::with_capacity(2);
        if let Some(first) = &self.first {
            children.push(first.clone());
        }
        if let Some(second) = &self.second {
            children.push(second.clone());
        }

        View::new(self.clone(), children, None)
    }
}

impl AnyView for SplitContainer {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<SplitContainer>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.max
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        _child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.max;
        let total = match self.direction {
            SplitDirection::Horizontal => size.width,
            SplitDirection::Vertical => size.height,
        };

        let sash = self.sash_thickness;
        let available = (total - sash).max(0.0);
        let min_s = self.min_pane_size;
        let max_first = (available - min_s).max(min_s);
        let first_len = (available * self.ratio).clamp(min_s.min(max_first), max_first);

        let positions = match self.direction {
            SplitDirection::Horizontal => vec![Point::ZERO, Point::new(first_len + sash, 0.0)],
            SplitDirection::Vertical => vec![Point::ZERO, Point::new(0.0, first_len + sash)],
        };

        (size, positions)
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        let sash_local = self.sash_rect(rect.size());
        let sash_world = Rect::from_min_size(
            Point::new(rect.min.x + sash_local.min.x, rect.min.y + sash_local.min.y),
            sash_local.size(),
        );

        vec![Primitive::Quad {
            rect: sash_world,
            color: self.sash_color,
            corner_radius: 0.0,
        }]
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        if let UiEvent::Pointer(pe) = event {
            let cursor_shape = match self.direction {
                SplitDirection::Horizontal => CursorShape::ResizeHorizontal,
                SplitDirection::Vertical => CursorShape::ResizeVertical,
            };

            let local_pos = Point::new(pe.position.x - rect.min.x, pe.position.y - rect.min.y);

            match pe.phase {
                PointerPhase::Down if pe.button == PointerButton::Left => {
                    if self.is_in_sash_hit(local_pos, rect.size()) {
                        self.dragging.store(true, Ordering::Relaxed);
                        ctx.capture_pointer(pe.pointer_id);
                        ctx.set_cursor(cursor_shape);
                        ctx.stop_propagation();
                        return EventHandled::Handled;
                    }
                }
                PointerPhase::Move => {
                    if self.dragging.load(Ordering::Relaxed) {
                        ctx.set_cursor(cursor_shape);
                        if let Some(on_resize) = &self.on_resize {
                            let total = match self.direction {
                                SplitDirection::Horizontal => rect.size().width,
                                SplitDirection::Vertical => rect.size().height,
                            };
                            let pos = match self.direction {
                                SplitDirection::Horizontal => local_pos.x,
                                SplitDirection::Vertical => local_pos.y,
                            };
                            if total > 0.0 {
                                let new_ratio = (pos / total).clamp(0.05, 0.95);
                                on_resize(new_ratio);
                            }
                        }
                        ctx.invalidate_paint();
                        return EventHandled::Handled;
                    } else if self.is_in_sash_hit(local_pos, rect.size()) {
                        ctx.set_cursor(cursor_shape);
                        return EventHandled::Handled;
                    }
                }
                PointerPhase::Up if pe.button == PointerButton::Left => {
                    if self.dragging.swap(false, Ordering::Relaxed) {
                        ctx.release_pointer(pe.pointer_id);
                        ctx.reset_cursor();
                        ctx.stop_propagation();
                        return EventHandled::Handled;
                    }
                }
                PointerPhase::Cancel => {
                    if self.dragging.swap(false, Ordering::Relaxed) {
                        ctx.release_pointer(pe.pointer_id);
                        ctx.reset_cursor();
                        return EventHandled::Handled;
                    }
                }
                _ => {}
            }
        }

        EventHandled::Ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::PointerEvent;
    use crate::widgets::sized_box::SizedBox;
    #[test]
    fn split_container_horizontal_layout() {
        let split = SplitContainer::new(SplitDirection::Horizontal, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0)
            .first(SizedBox::new(Size::new(50.0, 100.0)))
            .second(SizedBox::new(Size::new(50.0, 100.0)));

        let constraints = BoxConstraints::tight(Size::new(204.0, 100.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let (size, positions) = split.layout_children(constraints, &[], &metrics);
        assert_eq!(size, Size::new(204.0, 100.0));
        assert_eq!(positions.len(), 2);
        // Total = 204, sash = 4, available = 200, first = 100
        assert_eq!(positions[0], Point::ZERO);
        assert_eq!(positions[1], Point::new(104.0, 0.0));
    }

    #[test]
    fn split_container_vertical_layout() {
        let split = SplitContainer::new(SplitDirection::Vertical, 0.25)
            .sash_thickness(4.0)
            .min_pane_size(10.0)
            .first(SizedBox::new(Size::new(100.0, 50.0)))
            .second(SizedBox::new(Size::new(100.0, 50.0)));

        let constraints = BoxConstraints::tight(Size::new(100.0, 204.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let (size, positions) = split.layout_children(constraints, &[], &metrics);
        assert_eq!(size, Size::new(100.0, 204.0));
        assert_eq!(positions.len(), 2);
        // Total = 204, sash = 4, available = 200, first = 50
        assert_eq!(positions[0], Point::ZERO);
        assert_eq!(positions[1], Point::new(0.0, 54.0));
    }

    #[test]
    fn split_container_min_size_clamping() {
        let split = SplitContainer::new(SplitDirection::Horizontal, 0.01)
            .sash_thickness(4.0)
            .min_pane_size(30.0);

        let constraints = BoxConstraints::tight(Size::new(104.0, 100.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        // Total = 104, sash = 4, available = 100, ratio 0.01 -> 1.0 clamped to min 30.0
        let (_size, positions) = split.layout_children(constraints, &[], &metrics);
        assert_eq!(positions[1], Point::new(34.0, 0.0));
    }
    #[test]
    fn split_container_clamps_ratio_and_reports_sash_hits() {
        let clamped_low = SplitContainer::new(SplitDirection::Horizontal, 0.0);
        let clamped_high = SplitContainer::new(SplitDirection::Vertical, 1.0);

        assert_eq!(clamped_low.ratio, 0.05);
        assert_eq!(clamped_high.ratio, 0.95);

        let split = SplitContainer::new(SplitDirection::Horizontal, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0);
        let size = Size::new(200.0, 100.0);
        let sash = split.sash_rect(size);

        assert!(split.is_in_sash_hit(Point::new(sash.min.x - 4.0, 10.0), size));
        assert!(split.is_in_sash_hit(Point::new(sash.max.x + 3.0, 10.0), size));
        assert!(!split.is_in_sash_hit(Point::new(sash.min.x - 4.1, 10.0), size));
        assert!(!split.is_in_sash_hit(Point::new(sash.max.x + 4.0, 10.0), size));
    }

    #[test]
    fn split_container_dragging_emits_clamped_resize_ratios() {
        let resize_values = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_values = std::sync::Arc::clone(&resize_values);
        let split = SplitContainer::new(SplitDirection::Horizontal, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0)
            .views(
                SizedBox::new(Size::new(100.0, 100.0)).build(&mut BuildCx::stub()),
                SizedBox::new(Size::new(100.0, 100.0)).build(&mut BuildCx::stub()),
            )
            .on_resize(move |ratio| {
                captured_values.lock().expect("resize log").push(ratio);
            });

        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 100.0));
        let mut ctx = EventCtx::new();

        assert_eq!(
            split.handle_event(
                &UiEvent::Pointer(PointerEvent::new(
                    Point::new(100.0, 10.0),
                    PointerPhase::Down,
                    PointerButton::Left,
                    7,
                )),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert!(ctx.is_propagation_stopped());

        assert_eq!(
            split.handle_event(
                &UiEvent::Pointer(PointerEvent::new(
                    Point::new(1.0, 10.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    7,
                )),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert!(ctx.needs_paint());
        assert_eq!(&*resize_values.lock().expect("resize log"), &[0.05]);

        assert_eq!(
            split.handle_event(
                &UiEvent::Pointer(PointerEvent::new(
                    Point::new(1.0, 10.0),
                    PointerPhase::Up,
                    PointerButton::Left,
                    7,
                )),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
    }
}
