use crate::layout::{BoxConstraints, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

// ── TextLabel ───────────────────────────────────────────────────────────────

/// A non-interactive widget that displays a single line of monospace text.
///
/// Uses Runtime-owned `TextMetrics` for intrinsic size computation.
/// Produces a [`Primitive::Text`] with its text payload during the paint pass.
#[derive(Clone)]
pub struct TextLabel {
    text: String,
    color: Color,
    ellipsis: bool,
}

impl TextLabel {
    pub fn new(text: impl Into<String>) -> Self {
        TextLabel {
            text: text.into(),
            color: Color::WHITE,
            ellipsis: false,
        }
    }

    /// Truncates overflowing text to the available monospace cell budget.
    /// The full source text is retained for subsequent wider layouts.
    pub fn ellipsis(mut self) -> Self {
        self.ellipsis = true;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Component for TextLabel {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), vec![], None)
    }
}

impl crate::WithChildren for TextLabel {
    fn with_children(
        self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.ensure_leaf("TextLabel")?;
        Ok(self)
    }
}

impl AnyView for TextLabel {
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let width = self.text.len() as f32 * metrics.cell_width + 4.0;
        let height = metrics.line_height;
        constraints.constrain(Size::new(width, height))
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let text = if self.ellipsis {
            ellipsized(&self.text, rect.size().width, metrics.cell_width)
        } else {
            self.text.clone()
        };
        if self.ellipsis && text.is_empty() {
            return Vec::new();
        }
        vec![Primitive::Text {
            text: Arc::from(text),
            origin: rect.min,
            color: self.color,
        }]
    }
}

fn ellipsized(text: &str, width: f32, cell_width: f32) -> String {
    if !width.is_finite() || !cell_width.is_finite() || cell_width <= 0.0 {
        return String::new();
    }
    let budget = (width.max(0.0) / cell_width).floor() as usize;
    let cells: usize = text.chars().map(|ch| ch.width().unwrap_or(0)).sum();
    if cells <= budget {
        return text.to_owned();
    }
    if budget == 0 {
        return String::new();
    }
    let mut used = 0;
    let mut result = String::new();
    for ch in text.chars() {
        let cells = ch.width().unwrap_or(0);
        if used + cells > budget - 1 {
            break;
        }
        result.push(ch);
        used += cells;
    }
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn ellipsis_respects_cell_budgets_and_keeps_source_for_wider_layouts() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        for (text, cells, expected) in [
            ("Terminal", 4, "Ter…"),
            ("Terminal", 1, "…"),
            ("Terminal", 0, ""),
            ("Terminal", 8, "Terminal"),
            ("项目标签", 5, "项目…"),
            ("项目标签", 2, "…"),
            ("e\u{0301}xyz", 3, "e\u{0301}x…"),
        ] {
            assert_eq!(
                ellipsized(text, cells as f32 * metrics.cell_width, metrics.cell_width),
                expected
            );
        }
        let label = TextLabel::new("Terminal").ellipsis();
        for (cells, expected) in [(4, "Ter…"), (8, "Terminal")] {
            let rect = Rect::from_min_size(
                Point::ZERO,
                Size::new(cells as f32 * metrics.cell_width, 20.0),
            );
            assert!(matches!(&label.paint_primitives(rect, &metrics)[0],
                Primitive::Text { text, .. } if text.as_ref() == expected));
        }
        assert_eq!(ellipsized("test", 10.0, 0.0), "");
        assert_eq!(ellipsized("test", f32::NAN, 10.0), "");
    }

    use super::*;
    use crate::input::event::UiEvent;
    use crate::input::event_ctx::{EventCtx, EventHandled};
    use crate::layout::Point;

    #[test]
    fn text_label_intrinsic_size_uses_cell_width() {
        let label = TextLabel::new("Hello");
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = label.intrinsic_size(constraints, &crate::runtime::DEFAULT_TEXT_METRICS);
        // Default metrics: cell_width=10.0, line_height=20.0
        // "Hello" = 5 chars * 10.0 + 4.0 padding = 54.0
        assert!((size.width - 54.0).abs() < 1.0);
        assert!((size.height - 20.0).abs() < 1.0);
    }

    #[test]
    fn text_label_empty_text_min_width() {
        let label = TextLabel::new("");
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let size = label.intrinsic_size(constraints, &crate::runtime::DEFAULT_TEXT_METRICS);
        // Empty text: 0 chars * 10.0 + 4.0 padding = 4.0
        assert!((size.width - 4.0).abs() < 1.0);
    }

    #[test]
    fn text_label_paint_produces_text_primitive() {
        let label = TextLabel::new("Hi");
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(100.0, 30.0));
        let prims = label.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::Text { origin, color, .. } => {
                assert_eq!(*origin, Point::new(10.0, 20.0));
                assert_eq!(*color, Color::WHITE);
            }
            _ => panic!("expected Text primitive"),
        }
    }

    #[test]
    fn text_label_ignores_events() {
        let label = TextLabel::new("test");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 30.0));
        let event = UiEvent::Keyboard(crate::input::event::KeyboardEvent::KeyDown {
            key: crate::input::event::Key::Enter,
            modifiers: Default::default(),
        });
        let mut ctx = EventCtx::new();
        assert_eq!(
            label.handle_event(&event, &mut ctx, rect),
            EventHandled::Ignored
        );
    }

    #[test]
    fn text_label_custom_color() {
        let color = Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let label = TextLabel::new("X").color(color);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(10.0, 20.0));
        let prims = label.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        match &prims[0] {
            Primitive::Text { color: c, .. } => {
                assert_eq!(*c, color);
            }
            _ => panic!("expected Text"),
        }
    }
}
