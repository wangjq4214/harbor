use crate::input::event::UiEvent;
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key as ViewKey, View};
use std::sync::Arc;

// ── TextLabel ───────────────────────────────────────────────────────────────

/// A non-interactive widget that displays a single line of monospace text.
///
/// Uses Runtime-owned `TextMetrics` for intrinsic size computation.
/// Produces a [`Primitive::Text`] with its text payload during the paint pass.
#[derive(Clone)]
pub struct TextLabel {
    text: String,
    color: Color,
    children: Vec<View>,
}

impl TextLabel {
    pub fn new(text: impl Into<String>) -> Self {
        TextLabel {
            text: text.into(),
            color: Color::WHITE,
            children: vec![],
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Component for TextLabel {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.children.clone(), None)
    }
}

impl AnyView for TextLabel {
    fn key(&self) -> Option<&ViewKey> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let width = self.text.len() as f32 * metrics.cell_width + 4.0;
        let height = metrics.line_height;
        constraints.constrain(Size::new(width, height))
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let own = self.intrinsic_size(constraints, metrics);
        let positions = vec![Point::ZERO; child_sizes.len()];
        (own, positions)
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        vec![Primitive::Text {
            text: Arc::from(self.text.as_str()),
            origin: rect.min,
            color: self.color,
        }]
    }

    fn handle_event(&self, _event: &UiEvent, _ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        EventHandled::Ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
