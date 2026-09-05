use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::input::event::{PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key as ViewKey, View};

/// A read-only monospace text preview widget.
///
/// Displays a scrollable viewport over wrapped text lines, emitting Text
/// primitives only for visible lines. Scroll offset is shared with the
/// host via `Arc<AtomicUsize>` — the host owns the authoritative state.
///
/// Wheel scrolling is handled by the widget; keyboard scrolling is
/// handled by the host at window level.
#[derive(Clone)]
pub struct PreviewPane {
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    line_height: f32,
    visible_lines: usize,
    color: Color,
}

impl PreviewPane {
    pub fn new(
        wrapped_lines: Vec<String>,
        scroll_offset: Arc<AtomicUsize>,
        line_height: f32,
        visible_lines: usize,
    ) -> Self {
        PreviewPane {
            wrapped_lines,
            scroll_offset,
            line_height,
            visible_lines,
            color: Color::WHITE,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Component for PreviewPane {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), vec![], None)
    }
}

impl AnyView for PreviewPane {
    fn key(&self) -> Option<&ViewKey> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let cell_width = metrics.cell_width;
        let max_line_chars = self
            .wrapped_lines
            .iter()
            .map(|l| l.len())
            .max()
            .unwrap_or(0);
        let width = max_line_chars as f32 * cell_width + 4.0;
        let height = self.visible_lines as f32 * self.line_height;
        constraints.constrain(Size::new(width, height))
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        let offset = self.scroll_offset.load(Ordering::Relaxed);
        let end = (offset + self.visible_lines).min(self.wrapped_lines.len());
        let capacity = end.saturating_sub(offset);
        let mut prims = Vec::with_capacity(capacity);
        for i in offset..end {
            let line = &self.wrapped_lines[i];
            let y = rect.min.y + (i - offset) as f32 * self.line_height;
            prims.push(Primitive::Text {
                text: Arc::from(line.as_str()),
                origin: Point::new(rect.min.x, y),
                color: self.color,
            });
        }
        prims
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        match event {
            UiEvent::Pointer(ptr) => match ptr.phase {
                PointerPhase::WheelLine { dy, .. } | PointerPhase::WheelPixel { dy, .. } => {
                    let current = self.scroll_offset.load(Ordering::Relaxed) as isize;
                    let max_offset =
                        self.wrapped_lines.len().saturating_sub(self.visible_lines) as isize;
                    let new = (current + dy as isize).clamp(0, max_offset.max(0));
                    if new != current {
                        self.scroll_offset.store(new as usize, Ordering::Relaxed);
                        ctx.invalidate_paint();
                    }
                    EventHandled::Handled
                }
                _ => EventHandled::Ignored,
            },
            _ => EventHandled::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_metrics() -> TextMetrics {
        crate::runtime::DEFAULT_TEXT_METRICS
    }

    #[test]
    fn intrinsic_size_reflects_widest_line() {
        let metrics = test_metrics();
        let lines = vec!["hello".to_string(), "longer line!".to_string()];
        let offset = Arc::new(AtomicUsize::new(0));
        let pane = PreviewPane::new(lines, offset, 20.0, 10);
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = pane.intrinsic_size(constraints, &metrics);
        // "longer line!" = 12 chars * 10.0 + 4.0 = 124.0
        assert!((size.width - 124.0).abs() < 1.0);
        assert!((size.height - 200.0).abs() < 1.0); // 10 * 20.0
    }

    #[test]
    fn intrinsic_size_empty_lines() {
        let metrics = test_metrics();
        let lines: Vec<String> = vec![];
        let offset = Arc::new(AtomicUsize::new(0));
        let pane = PreviewPane::new(lines, offset, 20.0, 10);
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = pane.intrinsic_size(constraints, &metrics);
        // Empty: 0 chars * 10.0 + 4.0 = 4.0
        assert!((size.width - 4.0).abs() < 1.0);
        assert!((size.height - 200.0).abs() < 1.0);
    }

    #[test]
    fn paint_only_visible_lines() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(2));
        let pane = PreviewPane::new(lines, offset, 20.0, 3);
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(200.0, 60.0));
        let prims = pane.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        // visible_lines=3, offset=2 → lines 2, 3, 4
        assert_eq!(prims.len(), 3);
        for (i, prim) in prims.iter().enumerate() {
            match prim {
                Primitive::Text { origin, .. } => {
                    assert_eq!(origin.x, 10.0);
                    assert!((origin.y - (20.0 + i as f32 * 20.0)).abs() < 0.01);
                }
                _ => panic!("expected Text primitive"),
            }
        }
    }

    #[test]
    fn paint_near_end_clamps() {
        let lines: Vec<String> = (0..5).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(4));
        let pane = PreviewPane::new(lines, offset, 20.0, 3);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 60.0));
        let prims = pane.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        // Only 1 visible line (index 4)
        assert_eq!(prims.len(), 1);
    }

    #[test]
    fn paint_beyond_end_empty() {
        let lines: Vec<String> = (0..5).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(10));
        let pane = PreviewPane::new(lines, offset, 20.0, 3);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 60.0));
        let prims = pane.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert!(prims.is_empty());
    }

    #[test]
    fn wheel_down_increments_offset() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(5));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: 3.0 },
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        let result = pane.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        assert_eq!(offset.load(Ordering::Relaxed), 8);
        assert!(ctx.needs_paint());
    }

    #[test]
    fn should_scroll_when_wheel_pixel_arrives() {
        // Arrange
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(4));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelPixel { dx: 0.0, dy: 2.0 },
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));

        // Act
        let result = pane.handle_event(&event, &mut ctx, rect);

        // Assert
        assert_eq!(result, EventHandled::Handled);
        assert_eq!(offset.load(Ordering::Relaxed), 6);
        assert!(ctx.needs_paint());
    }

    #[test]
    fn wheel_up_decrements_offset() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(5));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: -2.0 },
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        let result = pane.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        assert_eq!(offset.load(Ordering::Relaxed), 3);
        assert!(ctx.needs_paint());
    }

    #[test]
    fn wheel_clamped_at_zero() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(0));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: -5.0 },
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        pane.handle_event(&event, &mut ctx, rect);
        assert_eq!(offset.load(Ordering::Relaxed), 0);
        assert!(!ctx.needs_paint());
    }

    #[test]
    fn wheel_clamped_at_max() {
        let lines: Vec<String> = (0..15).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(5)); // max = 15-10 = 5
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: 10.0 },
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        pane.handle_event(&event, &mut ctx, rect);
        assert_eq!(offset.load(Ordering::Relaxed), 5);
        assert!(!ctx.needs_paint());
    }

    #[test]
    fn non_scroll_events_ignored() {
        let lines: Vec<String> = vec!["hello".to_string()];
        let offset = Arc::new(AtomicUsize::new(0));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);

        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));

        // Pointer Move ignored
        let move_event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::Move,
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        assert_eq!(
            pane.handle_event(&move_event, &mut ctx, rect),
            EventHandled::Ignored
        );

        // Keyboard ignored
        let kb_event = UiEvent::Keyboard(crate::input::event::KeyboardEvent::KeyDown {
            key: crate::input::event::Key::ArrowDown,
            modifiers: Default::default(),
        });
        let mut ctx = EventCtx::new();
        assert_eq!(
            pane.handle_event(&kb_event, &mut ctx, rect),
            EventHandled::Ignored
        );
    }

    #[test]
    fn fewer_lines_than_visible_max_scroll_is_zero() {
        let lines: Vec<String> = vec!["a".to_string(), "b".to_string()];
        let offset = Arc::new(AtomicUsize::new(0));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = UiEvent::Pointer(crate::input::event::PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: 5.0 },
            crate::input::event::PointerButton::Left,
            0,
        ));
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        pane.handle_event(&event, &mut ctx, rect);
        // total_lines=2, visible_lines=10 → max=0
        assert_eq!(offset.load(Ordering::Relaxed), 0);
        assert!(!ctx.needs_paint());
    }
}
