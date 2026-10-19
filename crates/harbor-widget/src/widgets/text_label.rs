use crate::layout::{BoxConstraints, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};
use std::cell::RefCell;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

/// A non-interactive, single-line label measured with the renderer's glyph advances.
///
/// There is no implicit padding; compose with `Padding` when spacing is desired.
/// Without font resources, measurement uses the renderer's per-scalar fallback.
#[derive(Clone)]
pub struct TextLabel {
    text: String,
    color: Color,
    ellipsis: bool,
    geometry: RefCell<Option<LabelGeometry>>,
}

impl TextLabel {
    pub fn new(text: impl Into<String>) -> Self {
        TextLabel {
            text: text.into(),
            color: Color::WHITE,
            ellipsis: false,
            geometry: RefCell::new(None),
        }
    }

    /// Truncates overflowing text at grapheme boundaries using measured advances,
    /// including the ellipsis glyph. Keeps the source for subsequent wider layouts.
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
        let width = self.geometry.borrow().as_ref().map_or_else(
            || self.text.chars().count() as f32 * metrics.cell_width,
            LabelGeometry::width,
        );
        constraints.constrain(Size::new(width, metrics.line_height))
    }

    fn prepare_text_geometry(&self, _metrics: &TextMetrics, advance: &mut dyn FnMut(char) -> f32) {
        *self.geometry.borrow_mut() = Some(LabelGeometry::measure(&self.text, advance));
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let text = if self.ellipsis {
            let geometry = self.geometry.borrow();
            let fallback;
            let geometry = match geometry.as_ref() {
                Some(geometry) => geometry,
                None => {
                    fallback = LabelGeometry::measure(&self.text, &mut |_| metrics.cell_width);
                    &fallback
                }
            };
            geometry.ellipsized(&self.text, rect.size().width)
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

/// Cumulative renderer advances at grapheme ends, including the empty prefix.
/// These widths serve both intrinsic layout and truncation; no cell-budget path.
#[derive(Clone)]
struct LabelGeometry {
    prefixes: Vec<(usize, f32)>,
    ellipsis_width: f32,
}

impl LabelGeometry {
    fn measure(text: &str, advance: &mut dyn FnMut(char) -> f32) -> Self {
        let mut prefixes = vec![(0, 0.0)];
        let mut width = 0.0;
        for (start, grapheme) in text.grapheme_indices(true) {
            for ch in grapheme.chars() {
                width += advance(ch);
            }
            prefixes.push((start + grapheme.len(), width));
        }
        Self {
            prefixes,
            ellipsis_width: advance('…'),
        }
    }

    fn width(&self) -> f32 {
        self.prefixes.last().unwrap().1
    }

    fn ellipsized(&self, text: &str, width: f32) -> String {
        if !width.is_finite() || width <= 0.0 {
            return String::new();
        }
        if self.width() <= width {
            return text.to_owned();
        }
        if self.ellipsis_width > width {
            return String::new();
        }
        let end = self
            .prefixes
            .iter()
            .take_while(|(_, advance)| advance + self.ellipsis_width <= width)
            .last()
            .map_or(0, |(end, _)| *end);
        format!("{}…", &text[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::UiEvent;
    use crate::input::event_ctx::{EventCtx, EventHandled};
    use crate::layout::Point;

    fn painted_text(label: &TextLabel, width: f32) -> String {
        let rect = Rect::from_min_size(Point::ZERO, Size::new(width, 20.0));
        label
            .paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS)
            .iter()
            .filter_map(|primitive| match primitive {
                Primitive::Text { text, .. } => Some(text.as_ref()),
                _ => None,
            })
            .collect::<String>()
    }

    #[test]
    fn default_width_uses_glyph_advances_without_hidden_padding() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        let label = TextLabel::new("Ctrl");
        label.prepare_text_geometry(&metrics, &mut |ch| if ch == 'l' { 2.0 } else { 6.0 });
        assert_eq!(label.intrinsic_size(constraints, &metrics).width, 20.0);
        label.prepare_text_geometry(&metrics, &mut |_| 5.0 / 1.5);
        assert!((label.intrinsic_size(constraints, &metrics).width - 20.0 / 1.5).abs() < 0.001);
        assert_eq!(
            label
                .intrinsic_size(BoxConstraints::loose(Size::new(7.0, 20.0)), &metrics)
                .width,
            7.0
        );
        let empty = TextLabel::new("");
        empty.prepare_text_geometry(&metrics, &mut |_| 6.0);
        assert_eq!(empty.intrinsic_size(constraints, &metrics).width, 0.0);
    }

    #[test]
    fn ellipsis_uses_the_same_proportional_advances_and_preserves_graphemes() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let advance = |ch| match ch {
            'i' => 1.0,
            'W' | '👩' | '💻' => 8.0,
            'e' => 5.0,
            '\u{0301}' | '\u{200d}' => 0.0,
            '…' => 3.0,
            _ => 10.0,
        };
        for (text, width, expected) in [
            ("iiiiWW", 4.0, "i…"),
            ("iiiiWW", 7.0, "iiii…"),
            ("iiiiWW", 20.0, "iiiiWW"),
            ("iiii", 4.0, "iiii"),
            ("WWii", 10.0, "…"),
            ("项目标签", 23.0, "项目…"),
            ("e\u{0301}WW", 8.0, "e\u{0301}…"),
            ("👩\u{200d}💻WW", 19.0, "👩\u{200d}💻…"),
            ("👩\u{200d}💻WW", 12.0, "…"),
            ("WW", 2.0, ""),
            ("WW", 0.0, ""),
            ("WW", f32::NAN, ""),
            ("", 10.0, ""),
        ] {
            let label = TextLabel::new(text).ellipsis();
            label.prepare_text_geometry(&metrics, &mut |ch| advance(ch));
            assert_eq!(painted_text(&label, width), expected, "{text:?} at {width}");
            // Narrowing never destroys the source needed by a later wider layout.
            assert_eq!(painted_text(&label, 800.0), text);
        }
    }

    #[test]
    fn without_fonts_fallback_counts_scalars_not_utf8_bytes_or_padding() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let constraints = BoxConstraints::loose(Size::new(800.0, 600.0));
        for (text, width) in [("Hello", 50.0), ("项目标签", 40.0), ("", 0.0)] {
            assert_eq!(
                TextLabel::new(text)
                    .intrinsic_size(constraints, &metrics)
                    .width,
                width
            );
        }
        let label = TextLabel::new("Terminal").ellipsis();
        assert_eq!(painted_text(&label, 40.0), "Ter…");
        assert_eq!(painted_text(&label, 80.0), "Terminal");
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
