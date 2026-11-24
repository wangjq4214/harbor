use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use harbor_terminal::safe_preview_line;
use harbor_widget::ScrollController;
use harbor_widget::input::event::{PointerPhase, UiEvent};
use harbor_widget::input::event_ctx::{EventCtx, EventHandled};
use harbor_widget::layout::{BoxConstraints, Point, Rect, Size};
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::text::TextMetrics;
use harbor_widget::view::{BuildCx, Component, PrimitiveLeaf, View};
use unicode_width::UnicodeWidthChar;

/// One visual row inside the in-window confirmation preview.
///
/// Wrapped continuation rows keep `line_number == None` so the left gutter
/// stays blank without incrementing the 1-based logical line counter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewRow {
    pub line_number: Option<usize>,
    pub text: String,
}

/// Escapes control characters via `safe_preview_line` and wraps logical lines
/// at `max_columns` visual columns while preserving 1-based logical line numbers
/// only on the first visual row of each logical line.
pub fn wrap_preview_rows(raw_text: &str, max_columns: usize) -> Vec<PreviewRow> {
    let escaped: Vec<String> = raw_text.lines().map(safe_preview_line).collect();
    wrap_escaped_lines(&escaped, max_columns)
}

pub(crate) fn wrap_escaped_lines(escaped_lines: &[String], max_columns: usize) -> Vec<PreviewRow> {
    let max_columns = max_columns.max(1);
    let mut rows = Vec::with_capacity(escaped_lines.len());
    for (index, escaped) in escaped_lines.iter().enumerate() {
        let logical_line = index + 1;
        let mut is_first = true;
        let mut current_line = String::new();
        let mut current_width = 0usize;
        for ch in escaped.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
            if current_width + ch_width > max_columns && !current_line.is_empty() {
                rows.push(PreviewRow {
                    line_number: is_first.then_some(logical_line),
                    text: std::mem::take(&mut current_line),
                });
                is_first = false;
                current_width = 0;
            }
            current_line.push(ch);
            current_width += ch_width;
        }
        rows.push(PreviewRow {
            line_number: is_first.then_some(logical_line),
            text: current_line,
        });
    }
    rows
}

const OUTER_VERTICAL_PADDING: f32 = 2.0;
const ROW_VERTICAL_PADDING: f32 = 2.0;
const GUTTER_LEFT_PADDING: f32 = 10.0;
const GUTTER_RIGHT_PADDING: f32 = 10.0;
const TEXT_LEFT_GAP: f32 = 10.0;
const SCROLLBAR_RESERVED: f32 = 14.0;
const DEFAULT_WRAP_COLUMNS: usize = 60;

/// Scrollable confirmation preview content rendered inside `ScrollView`.
///
/// Draws a logical line-number gutter, subtle vertical divider, alternating row
/// tinting, and escaped monospace preview lines without altering raw payloads.
type WrapCache = Rc<RefCell<Option<(usize, Vec<PreviewRow>)>>>;

#[derive(Clone)]
pub struct PreviewContent {
    escaped_lines: Arc<Vec<String>>,
    scroll: ScrollController,
    row_tint: Color,
    gutter_color: Color,
    text_color: Color,
    gutter_divider: Color,
    cache: WrapCache,
}

impl PreviewContent {
    pub fn new(
        escaped_lines: Arc<Vec<String>>,
        scroll: ScrollController,
        text_color: Color,
        gutter_color: Color,
        gutter_divider: Color,
        row_tint: Color,
    ) -> Self {
        Self {
            escaped_lines,
            scroll,
            row_tint,
            gutter_color,
            text_color,
            gutter_divider,
            cache: Rc::new(RefCell::new(None)),
        }
    }

    pub fn row_height(metrics: &TextMetrics) -> f32 {
        metrics.line_height + ROW_VERTICAL_PADDING * 2.0
    }

    fn gutter_digits(&self) -> usize {
        let count = self.escaped_lines.len().max(1);
        ((count.ilog10() as usize) + 1).max(2)
    }

    fn gutter_width(&self, metrics: &TextMetrics) -> f32 {
        GUTTER_LEFT_PADDING
            + self.gutter_digits() as f32 * metrics.cell_width
            + GUTTER_RIGHT_PADDING
    }

    fn columns_for_width(&self, width: f32, metrics: &TextMetrics) -> usize {
        if !width.is_finite() || metrics.cell_width <= 0.0 || !metrics.cell_width.is_finite() {
            return DEFAULT_WRAP_COLUMNS;
        }
        let available = width - self.gutter_width(metrics) - TEXT_LEFT_GAP - SCROLLBAR_RESERVED;
        (available / metrics.cell_width).floor().max(1.0) as usize
    }

    fn rows_for_columns(&self, cols: usize) -> std::cell::Ref<'_, Vec<PreviewRow>> {
        {
            let mut cache = self.cache.borrow_mut();
            let needs_update = cache
                .as_ref()
                .is_none_or(|(cached_cols, _)| *cached_cols != cols);
            if needs_update {
                *cache = Some((cols, wrap_escaped_lines(&self.escaped_lines, cols)));
            }
        }
        std::cell::Ref::map(self.cache.borrow(), |entry| {
            &entry.as_ref().expect("populated wrap cache").1
        })
    }
}

impl Component for PreviewContent {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::leaf(self.clone())
    }
}

impl PrimitiveLeaf for PreviewContent {
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let cols = self.columns_for_width(constraints.max.width, metrics);
        let rows = self.rows_for_columns(cols);
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            let max_chars = rows
                .iter()
                .map(|row| {
                    row.text
                        .chars()
                        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(1).max(1))
                        .sum::<usize>()
                })
                .max()
                .unwrap_or(0);
            self.gutter_width(metrics)
                + TEXT_LEFT_GAP
                + max_chars as f32 * metrics.cell_width
                + SCROLLBAR_RESERVED
        };
        let row_count = rows.len().max(1);
        let height = row_count as f32 * Self::row_height(metrics) + OUTER_VERTICAL_PADDING * 2.0;
        constraints.constrain(Size::new(width, height))
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let size = rect.size();
        if !size.is_valid() || size.width <= 0.0 || size.height <= 0.0 {
            return Vec::new();
        }
        let cols = self.columns_for_width(size.width, metrics);
        let rows = self.rows_for_columns(cols);
        let row_h = Self::row_height(metrics);
        let gutter_w = self.gutter_width(metrics);
        let text_x = rect.min.x + gutter_w + TEXT_LEFT_GAP;

        let scroll_metrics = self.scroll.metrics();
        let (start_idx, end_idx) = if scroll_metrics.viewport_extent() > 0.0 {
            let offset = scroll_metrics.offset();
            let vis_end = offset + scroll_metrics.viewport_extent();
            let start = ((offset - OUTER_VERTICAL_PADDING) / row_h).floor().max(0.0) as usize;
            let end = (((vis_end - OUTER_VERTICAL_PADDING) / row_h).ceil().max(0.0) as usize + 1)
                .min(rows.len());
            (start.min(rows.len()), end)
        } else {
            (0, rows.len())
        };

        let mut prims = Vec::with_capacity((end_idx.saturating_sub(start_idx)) * 3 + 1);
        if self.gutter_divider.a > 0.0 {
            prims.push(Primitive::Quad {
                rect: Rect::from_min_size(
                    Point::new(rect.min.x + gutter_w, rect.min.y),
                    Size::new(1.0, size.height),
                ),
                color: self.gutter_divider,
                corner_radius: 0.0,
            });
        }

        for (i, row) in rows.iter().enumerate().take(end_idx).skip(start_idx) {
            let row_y = rect.min.y + OUTER_VERTICAL_PADDING + i as f32 * row_h;
            if i % 2 == 1 && self.row_tint.a > 0.0 {
                prims.push(Primitive::Quad {
                    rect: Rect::from_min_size(
                        Point::new(rect.min.x, row_y),
                        Size::new(size.width, row_h),
                    ),
                    color: self.row_tint,
                    corner_radius: 0.0,
                });
            }
            if let Some(line_number) = row.line_number {
                let label = line_number.to_string();
                let num_x = (rect.min.x + gutter_w
                    - GUTTER_RIGHT_PADDING
                    - label.len() as f32 * metrics.cell_width)
                    .max(rect.min.x + 2.0);
                prims.push(Primitive::Text {
                    text: Arc::from(label),
                    origin: Point::new(num_x, row_y + ROW_VERTICAL_PADDING),
                    color: self.gutter_color,
                });
            }
            if !row.text.is_empty() {
                prims.push(Primitive::Text {
                    text: Arc::from(row.text.as_str()),
                    origin: Point::new(text_x, row_y + ROW_VERTICAL_PADDING),
                    color: self.text_color,
                });
            }
        }
        prims
    }
}

/// A read-only monospace text preview widget (legacy secondary-window preview;
/// retained during T0001/T0002 until T0003 retires `ConfirmationWindow`).
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
        View::leaf(self.clone())
    }
}

impl PrimitiveLeaf for PreviewPane {
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
    use harbor_widget::runtime::DEFAULT_TEXT_METRICS;

    fn pointer_event(phase: PointerPhase) -> UiEvent {
        UiEvent::Pointer(harbor_widget::input::event::PointerEvent::new(
            Point::ZERO,
            phase,
            harbor_widget::input::event::PointerButton::Left,
            0,
        ))
    }

    #[test]
    fn wrap_preview_rows_escapes_controls_and_keeps_logical_line_numbers() {
        let rows = wrap_preview_rows("short\n1234567890\na\tb\x01c\n\nend", 5);
        assert_eq!(
            rows,
            vec![
                PreviewRow {
                    line_number: Some(1),
                    text: "short".into()
                },
                PreviewRow {
                    line_number: Some(2),
                    text: "12345".into()
                },
                PreviewRow {
                    line_number: None,
                    text: "67890".into()
                },
                PreviewRow {
                    line_number: Some(3),
                    text: "a\u{2192}b\u{2401}c".into()
                },
                PreviewRow {
                    line_number: Some(4),
                    text: "".into()
                },
                PreviewRow {
                    line_number: Some(5),
                    text: "end".into()
                },
            ]
        );

        let cjk = wrap_preview_rows("你好世界\nnext", 4);
        assert_eq!(
            cjk,
            vec![
                PreviewRow {
                    line_number: Some(1),
                    text: "你好".into()
                },
                PreviewRow {
                    line_number: None,
                    text: "世界".into()
                },
                PreviewRow {
                    line_number: Some(2),
                    text: "next".into()
                },
            ]
        );
    }

    #[test]
    fn preview_content_paints_gutter_divider_alternating_row_tint_and_line_numbers() {
        let escaped: Arc<Vec<String>> = Arc::new(
            ["first", "abcdefghij1234567890", "third"]
                .into_iter()
                .map(safe_preview_line)
                .collect(),
        );
        let scroll = ScrollController::new();
        let tint = Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.04,
        };
        let gutter = Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.55,
        };
        let divider = Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.12,
        };
        let content = PreviewContent::new(escaped, scroll, Color::WHITE, gutter, divider, tint);
        let metrics = DEFAULT_TEXT_METRICS;
        // Gutter = 10 + 2*10 + 10 = 40; text_left = 50; scrollbar = 14 -> 10 cols at width = 164.
        let size = content.intrinsic_size(BoxConstraints::loose(Size::new(164.0, 500.0)), &metrics);
        let rect = Rect::from_min_size(Point::ZERO, size);
        let prims = content.paint_primitives(rect, &metrics);

        let row_tints: Vec<_> = prims
            .iter()
            .filter(|p| matches!(p, Primitive::Quad { color, .. } if *color == tint))
            .collect();
        // 4 visual rows (0: "first", 1: "abcdefghij", 2: "1234567890" [cont], 3: "third")
        // Odd visual rows are indices 1 and 3 -> 2 alternating row tints.
        assert_eq!(row_tints.len(), 2);

        let gutter_nums: Vec<&str> = prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, color, .. } if *color == gutter => Some(text.as_ref()),
                _ => None,
            })
            .collect();
        // Continuation row (visual row 2) has no line number; line 3 stays "3".
        assert_eq!(gutter_nums, vec!["1", "2", "3"]);

        let body_texts: Vec<&str> = prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, color, .. } if *color == Color::WHITE => {
                    Some(text.as_ref())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            body_texts,
            vec!["first", "abcdefghij", "1234567890", "third"]
        );
    }

    #[test]
    fn intrinsic_size_reflects_widest_line() {
        let metrics = DEFAULT_TEXT_METRICS;
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
        let metrics = DEFAULT_TEXT_METRICS;
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
        let prims = pane.paint_primitives(rect, &harbor_widget::runtime::DEFAULT_TEXT_METRICS);
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
        let prims = pane.paint_primitives(rect, &harbor_widget::runtime::DEFAULT_TEXT_METRICS);
        // Only 1 visible line (index 4)
        assert_eq!(prims.len(), 1);
    }

    #[test]
    fn paint_beyond_end_empty() {
        let lines: Vec<String> = (0..5).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(10));
        let pane = PreviewPane::new(lines, offset, 20.0, 3);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 60.0));
        let prims = pane.paint_primitives(rect, &harbor_widget::runtime::DEFAULT_TEXT_METRICS);
        assert!(prims.is_empty());
    }

    #[test]
    fn wheel_down_increments_offset() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(5));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = pointer_event(PointerPhase::WheelLine { dx: 0.0, dy: 3.0 });
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        let result = pane.handle_event(&event, &mut ctx, rect);
        assert_eq!(result, EventHandled::Handled);
        assert_eq!(offset.load(Ordering::Relaxed), 8);
        assert!(ctx.needs_paint());
    }

    #[test]
    fn should_scroll_when_wheel_pixel_arrives() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(4));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = pointer_event(PointerPhase::WheelPixel { dx: 0.0, dy: 2.0 });
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));

        let result = pane.handle_event(&event, &mut ctx, rect);

        assert_eq!(result, EventHandled::Handled);
        assert_eq!(offset.load(Ordering::Relaxed), 6);
        assert!(ctx.needs_paint());
    }

    #[test]
    fn wheel_up_decrements_offset() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let offset = Arc::new(AtomicUsize::new(5));
        let pane = PreviewPane::new(lines, offset.clone(), 20.0, 10);
        let event = pointer_event(PointerPhase::WheelLine { dx: 0.0, dy: -2.0 });
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
        let event = pointer_event(PointerPhase::WheelLine { dx: 0.0, dy: -5.0 });
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
        let event = pointer_event(PointerPhase::WheelLine { dx: 0.0, dy: 10.0 });
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

        let move_event = pointer_event(PointerPhase::Move);
        let mut ctx = EventCtx::new();
        assert_eq!(
            pane.handle_event(&move_event, &mut ctx, rect),
            EventHandled::Ignored
        );

        let kb_event = UiEvent::Keyboard(harbor_widget::input::event::KeyboardEvent::KeyDown {
            key: harbor_widget::input::event::Key::ArrowDown,
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
        let event = pointer_event(PointerPhase::WheelLine { dx: 0.0, dy: 5.0 });
        let mut ctx = EventCtx::new();
        let rect = Rect::from_min_size(Point::ZERO, Size::new(200.0, 200.0));
        pane.handle_event(&event, &mut ctx, rect);
        assert_eq!(offset.load(Ordering::Relaxed), 0);
        assert!(!ctx.needs_paint());
    }
}
