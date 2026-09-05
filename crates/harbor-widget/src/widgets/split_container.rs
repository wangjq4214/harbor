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

    fn partition_lengths(&self, total: f32) -> (f32, f32, f32) {
        let total = total.max(0.0);
        let sash = self.sash_thickness.min(total);
        let available = total - sash;
        if available <= 0.0 {
            return (sash, 0.0, 0.0);
        }

        let effective_min = self.min_pane_size.min(available / 2.0);
        let first = (available * self.ratio).clamp(effective_min, available - effective_min);
        (sash, first, available - first)
    }

    /// Computes the sash divider rect given total container bounds.
    pub fn sash_rect(&self, total_size: Size) -> Rect {
        let total = match self.direction {
            SplitDirection::Horizontal => total_size.width,
            SplitDirection::Vertical => total_size.height,
        };

        let (sash, first_len, _) = self.partition_lengths(total);

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

    fn children_constraints(
        &self,
        _child_count: usize,
        constraints: BoxConstraints,
        _metrics: &TextMetrics,
    ) -> Vec<BoxConstraints> {
        let size = constraints.constrain(constraints.max);
        let total = match self.direction {
            SplitDirection::Horizontal => size.width,
            SplitDirection::Vertical => size.height,
        };

        let (_, first_len, second_len) = self.partition_lengths(total);

        match self.direction {
            SplitDirection::Horizontal => vec![
                BoxConstraints::tight(Size::new(first_len, size.height)),
                BoxConstraints::tight(Size::new(second_len, size.height)),
            ],
            SplitDirection::Vertical => vec![
                BoxConstraints::tight(Size::new(size.width, first_len)),
                BoxConstraints::tight(Size::new(size.width, second_len)),
            ],
        }
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        _child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(constraints.max);
        let total = match self.direction {
            SplitDirection::Horizontal => size.width,
            SplitDirection::Vertical => size.height,
        };

        let (sash, first_len, _) = self.partition_lengths(total);

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

        let mut prims = Vec::with_capacity(7);

        // 1. Sash background quad
        prims.push(Primitive::Quad {
            rect: sash_world,
            color: self.sash_color,
            corner_radius: 0.0,
        });

        // 2. Sash boundary divider lines separating first and second panes
        let divider_color = Color {
            r: 0.32,
            g: 0.36,
            b: 0.42,
            a: 1.0,
        };
        match self.direction {
            SplitDirection::Horizontal => {
                // Left divider line of the sash (right edge of first pane)
                prims.push(Primitive::Quad {
                    rect: Rect::from_min_size(
                        Point::new(sash_world.min.x, sash_world.min.y),
                        Size::new(1.0, sash_world.size().height),
                    ),
                    color: divider_color,
                    corner_radius: 0.0,
                });
                // Right divider line of the sash (left edge of second pane)
                prims.push(Primitive::Quad {
                    rect: Rect::from_min_size(
                        Point::new(sash_world.max.x - 1.0, sash_world.min.y),
                        Size::new(1.0, sash_world.size().height),
                    ),
                    color: divider_color,
                    corner_radius: 0.0,
                });
            }
            SplitDirection::Vertical => {
                // Top divider line of the sash (bottom edge of first pane)
                prims.push(Primitive::Quad {
                    rect: Rect::from_min_size(
                        Point::new(sash_world.min.x, sash_world.min.y),
                        Size::new(sash_world.size().width, 1.0),
                    ),
                    color: divider_color,
                    corner_radius: 0.0,
                });
                // Bottom divider line of the sash (top edge of second pane)
                prims.push(Primitive::Quad {
                    rect: Rect::from_min_size(
                        Point::new(sash_world.min.x, sash_world.max.y - 1.0),
                        Size::new(sash_world.size().width, 1.0),
                    ),
                    color: divider_color,
                    corner_radius: 0.0,
                });
            }
        }

        // 3. Outer pane boundary borders to ensure each pane has crisp, visible borders
        let pane_border_color = Color {
            r: 0.22,
            g: 0.24,
            b: 0.28,
            a: 1.0,
        };
        // Top border
        prims.push(Primitive::Quad {
            rect: Rect::from_min_size(
                Point::new(rect.min.x, rect.min.y),
                Size::new(rect.size().width, 1.0),
            ),
            color: pane_border_color,
            corner_radius: 0.0,
        });
        // Bottom border
        prims.push(Primitive::Quad {
            rect: Rect::from_min_size(
                Point::new(rect.min.x, rect.max.y - 1.0),
                Size::new(rect.size().width, 1.0),
            ),
            color: pane_border_color,
            corner_radius: 0.0,
        });
        // Left border
        prims.push(Primitive::Quad {
            rect: Rect::from_min_size(
                Point::new(rect.min.x, rect.min.y),
                Size::new(1.0, rect.size().height),
            ),
            color: pane_border_color,
            corner_radius: 0.0,
        });
        // Right border
        prims.push(Primitive::Quad {
            rect: Rect::from_min_size(
                Point::new(rect.max.x - 1.0, rect.min.y),
                Size::new(1.0, rect.size().height),
            ),
            color: pane_border_color,
            corner_radius: 0.0,
        });

        prims
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
                PointerPhase::Cancel if self.dragging.swap(false, Ordering::Relaxed) => {
                    ctx.release_pointer(pe.pointer_id);
                    ctx.reset_cursor();
                    return EventHandled::Handled;
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
    fn split_container_shrinks_minimums_to_fit_tiny_horizontal_and_vertical_bounds() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        for (direction, size) in [
            (SplitDirection::Horizontal, Size::new(24.0, 80.0)),
            (SplitDirection::Vertical, Size::new(80.0, 24.0)),
        ] {
            let split = SplitContainer::new(direction, 0.95)
                .sash_thickness(4.0)
                .min_pane_size(32.0);
            let constraints = BoxConstraints::tight(size);
            let child_constraints = split.children_constraints(2, constraints, &metrics);
            let (_, positions) = split.layout_children(constraints, &[], &metrics);

            match direction {
                SplitDirection::Horizontal => {
                    assert_eq!(child_constraints[0].max.width, 10.0);
                    assert_eq!(child_constraints[1].max.width, 10.0);
                    assert_eq!(positions[1], Point::new(14.0, 0.0));
                    assert!(positions[1].x + child_constraints[1].max.width <= size.width);
                }
                SplitDirection::Vertical => {
                    assert_eq!(child_constraints[0].max.height, 10.0);
                    assert_eq!(child_constraints[1].max.height, 10.0);
                    assert_eq!(positions[1], Point::new(0.0, 14.0));
                    assert!(positions[1].y + child_constraints[1].max.height <= size.height);
                }
            }
        }
    }

    #[test]
    fn split_container_caps_sash_when_container_is_smaller_than_sash() {
        let split = SplitContainer::new(SplitDirection::Horizontal, 0.5)
            .sash_thickness(8.0)
            .min_pane_size(32.0);
        let size = Size::new(3.0, 40.0);
        let sash = split.sash_rect(size);

        assert_eq!(sash, Rect::from_min_size(Point::ZERO, Size::new(3.0, 40.0)));
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

    #[test]
    fn split_container_horizontal_paint_primitives_has_sash_and_boundaries() {
        let split = SplitContainer::new(SplitDirection::Horizontal, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(204.0, 100.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let prims = split.paint_primitives(rect, &metrics);
        // Expect 7 primitives: 1 sash bg + 2 sash divider lines + 4 outer borders
        assert_eq!(prims.len(), 7);

        // Sash quad (x = 100, width = 4, height = 100)
        match &prims[0] {
            Primitive::Quad { rect: r, color, .. } => {
                assert_eq!(r.min.x, 100.0);
                assert_eq!(r.size().width, 4.0);
                assert_eq!(r.size().height, 100.0);
                assert_eq!(*color, DEFAULT_SASH_COLOR);
            }
            _ => panic!("expected sash quad"),
        }

        // Left divider line (x = 100, width = 1)
        match &prims[1] {
            Primitive::Quad { rect: r, color, .. } => {
                assert_eq!(r.min.x, 100.0);
                assert_eq!(r.size().width, 1.0);
                assert_eq!(color.r, 0.32);
            }
            _ => panic!("expected left divider line"),
        }

        // Right divider line (x = 103, width = 1)
        match &prims[2] {
            Primitive::Quad { rect: r, color, .. } => {
                assert_eq!(r.min.x, 103.0);
                assert_eq!(r.size().width, 1.0);
                assert_eq!(color.r, 0.32);
            }
            _ => panic!("expected right divider line"),
        }
    }

    #[test]
    fn split_container_vertical_paint_primitives_has_sash_and_boundaries() {
        let split = SplitContainer::new(SplitDirection::Vertical, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 204.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let prims = split.paint_primitives(rect, &metrics);
        assert_eq!(prims.len(), 7);

        // Sash quad (y = 100, height = 4, width = 100)
        match &prims[0] {
            Primitive::Quad { rect: r, .. } => {
                assert_eq!(r.min.y, 100.0);
                assert_eq!(r.size().height, 4.0);
                assert_eq!(r.size().width, 100.0);
            }
            _ => panic!("expected sash quad"),
        }

        // Top divider line (y = 100, height = 1)
        match &prims[1] {
            Primitive::Quad { rect: r, .. } => {
                assert_eq!(r.min.y, 100.0);
                assert_eq!(r.size().height, 1.0);
            }
            _ => panic!("expected top divider line"),
        }

        // Bottom divider line (y = 103, height = 1)
        match &prims[2] {
            Primitive::Quad { rect: r, .. } => {
                assert_eq!(r.min.y, 103.0);
                assert_eq!(r.size().height, 1.0);
            }
            _ => panic!("expected bottom divider line"),
        }
    }

    #[test]
    fn split_container_children_constraints_match_partition() {
        let split = SplitContainer::new(SplitDirection::Horizontal, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0);
        let constraints = BoxConstraints::tight(Size::new(204.0, 100.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let cc = split.children_constraints(2, constraints, &metrics);
        assert_eq!(cc.len(), 2);
        // Total = 204, sash = 4, available = 200, 50% = 100
        assert_eq!(cc[0].min, Size::new(100.0, 100.0));
        assert_eq!(cc[0].max, Size::new(100.0, 100.0));
        assert_eq!(cc[1].min, Size::new(100.0, 100.0));
        assert_eq!(cc[1].max, Size::new(100.0, 100.0));
    }

    #[test]
    fn split_container_vertical_children_constraints_match_partition() {
        let split = SplitContainer::new(SplitDirection::Vertical, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0);
        let constraints = BoxConstraints::tight(Size::new(100.0, 204.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let cc = split.children_constraints(2, constraints, &metrics);
        assert_eq!(cc.len(), 2);
        // Total = 204, sash = 4, available = 200, 50% = 100
        assert_eq!(cc[0].min, Size::new(100.0, 100.0));
        assert_eq!(cc[0].max, Size::new(100.0, 100.0));
        assert_eq!(cc[1].min, Size::new(100.0, 100.0));
        assert_eq!(cc[1].max, Size::new(100.0, 100.0));
    }
}
