use crate::layout::{BoxConstraints, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};
use std::cell::RefCell;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

/// A non-interactive label measured with the renderer's glyph advances.
///
/// There is no implicit padding; compose with `Padding` when spacing is desired.
/// Without font resources, measurement uses the renderer's per-scalar fallback.
#[derive(Clone)]
pub struct TextLabel {
    text: String,
    color: Color,
    scale: f32,
    ellipsis: bool,
    wrap: bool,
    geometry: RefCell<Option<LabelGeometry>>,
}

impl TextLabel {
    pub fn new(text: impl Into<String>) -> Self {
        TextLabel {
            text: text.into(),
            color: Color::WHITE,
            scale: 1.0,
            ellipsis: false,
            wrap: false,
            geometry: RefCell::new(None),
        }
    }

    /// Truncates overflowing text at grapheme boundaries using measured advances,
    /// including the ellipsis glyph. Keeps the source for subsequent wider layouts.
    pub fn ellipsis(mut self) -> Self {
        self.ellipsis = true;
        self.wrap = false;
        self
    }

    /// Wraps at measured grapheme boundaries and explicit line breaks, without
    /// an ellipsis or line cap. Source text survives every re-layout.
    /// A single grapheme wider than the available width overflows intact rather
    /// than disappearing. Parents own clipping/scrolling for constrained heights.
    /// `wrap` and `ellipsis` are exclusive; the last builder call wins.
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self.ellipsis = false;
        self
    }

    /// Scales this label relative to the UI font, including wrapping and line height.
    /// Glyphs are rasterized at the local scale and display DPI, not stretched bitmaps.
    /// A finite, nonnegative factor is required; zero suppresses the label.
    pub fn scale(mut self, scale: f32) -> Result<Self, crate::LayoutConfigError> {
        crate::layout::configuration::validate_scalar("TextLabel.scale", scale)?;
        self.scale = scale;
        *self.geometry.borrow_mut() = None;
        Ok(self)
    }

    fn with_geometry<T>(&self, metrics: &TextMetrics, f: impl FnOnce(&LabelGeometry) -> T) -> T {
        let geometry = self.geometry.borrow();
        match geometry.as_ref() {
            Some(geometry) => f(geometry),
            None => f(&LabelGeometry::measure(&self.text, &mut |_| {
                metrics.cell_width * self.scale
            })),
        }
    }

    pub fn foreground(mut self, color: Color) -> Result<Self, crate::theme::ColorConfigError> {
        crate::theme::validate_color("TextLabel.foreground", color)?;
        self.color = color;
        Ok(self)
    }
}

impl Component for TextLabel {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), vec![], None)
    }
}

impl AnyView for TextLabel {
    fn text_scale(&self) -> f32 {
        self.scale
    }
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        self.with_geometry(metrics, |geometry| {
            let size = if self.wrap {
                let ranges = geometry.line_ranges(&self.text, constraints.max.width, true);
                let width = ranges
                    .iter()
                    .map(|&(start, end)| geometry.prefixes[end].1 - geometry.prefixes[start].1)
                    .fold(0.0, f32::max);
                Size::new(
                    width,
                    ranges.len() as f32 * metrics.line_height * self.scale,
                )
            } else {
                Size::new(geometry.width(), metrics.line_height * self.scale)
            };
            constraints.constrain(size)
        })
    }

    fn prepare_text_geometry(&self, _metrics: &TextMetrics, advance: &mut dyn FnMut(char) -> f32) {
        *self.geometry.borrow_mut() = Some(LabelGeometry::measure(&self.text, advance));
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        if self.scale == 0.0 {
            return Vec::new();
        }
        let lines = self.with_geometry(metrics, |geometry| {
            if self.wrap {
                geometry.wrapped(&self.text, rect.size().width, None)
            } else if self.ellipsis {
                vec![geometry.ellipsized(&self.text, rect.size().width)]
            } else {
                vec![self.text.clone()]
            }
        });
        lines
            .into_iter()
            .enumerate()
            .filter(|(_, text)| !text.is_empty() || (!self.ellipsis && !self.wrap))
            .map(|(index, text)| {
                let origin = crate::layout::Point::new(
                    rect.min.x,
                    rect.min.y + index as f32 * metrics.line_height * self.scale,
                );
                if self.scale == 1.0 {
                    Primitive::Text {
                        text: Arc::from(text),
                        origin,
                        color: self.color,
                    }
                } else {
                    Primitive::ScaledText {
                        text: Arc::from(text),
                        origin,
                        color: self.color,
                        scale: self.scale,
                    }
                }
            })
            .collect()
    }
}

/// Cumulative renderer advances at grapheme ends, including the empty prefix.
/// These widths serve both intrinsic layout and truncation; no cell-budget path.
#[derive(Clone)]
pub(super) struct LabelGeometry {
    prefixes: Vec<(usize, f32)>,
    ellipsis_width: f32,
}

impl LabelGeometry {
    pub(super) fn measure(text: &str, advance: &mut dyn FnMut(char) -> f32) -> Self {
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

    pub(super) fn ellipsized(&self, text: &str, width: f32) -> String {
        self.display_range(text, 0, self.prefixes.len() - 1, width, false)
    }

    /// Wrap with the exact advances used by UI rendering, never terminal cells.
    /// Uncapped text always retains oversized graphemes; only capped displays truncate.
    pub(super) fn wrapped(&self, text: &str, width: f32, max_lines: Option<usize>) -> Vec<String> {
        if width.is_nan()
            || width < 0.0
            || max_lines == Some(0)
            || (max_lines.is_some() && (!width.is_finite() || width == 0.0))
        {
            return Vec::new();
        }
        let ranges = self.line_ranges(text, width, max_lines.is_none());
        let count = max_lines.unwrap_or(ranges.len()).min(ranges.len());
        ranges
            .iter()
            .take(count)
            .enumerate()
            .map(|(i, &(start, end))| {
                if max_lines.is_none() {
                    text[self.prefixes[start].0..self.prefixes[end].0].to_owned()
                } else {
                    self.display_range(
                        text,
                        start,
                        end,
                        width,
                        i + 1 == count && count < ranges.len(),
                    )
                }
            })
            .collect()
    }

    fn line_ranges(&self, text: &str, width: f32, word_wrap: bool) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut start = 0;
        let end = self.prefixes.len() - 1;
        let mut last_break = None;
        let mut i = 0;
        while i < end {
            let grapheme = &text[self.prefixes[i].0..self.prefixes[i + 1].0];
            if matches!(grapheme, "\n" | "\r\n" | "\r") {
                ranges.push((start, i));
                start = i + 1;
                last_break = None;
            } else {
                if i > start && self.prefixes[i + 1].1 - self.prefixes[start].1 > width {
                    let split = last_break
                        .filter(|&boundary| word_wrap && boundary > start)
                        .unwrap_or(i);
                    ranges.push((start, split));
                    start = split;
                    last_break = None;
                    if split < i {
                        // Revisit the carried word; it may itself need grapheme wrapping.
                        i = split;
                        continue;
                    }
                }
                if grapheme.chars().all(char::is_whitespace) {
                    last_break = Some(i + 1);
                }
            }
            i += 1;
        }
        ranges.push((start, end));
        ranges
    }
    fn display_range(
        &self,
        text: &str,
        start: usize,
        end: usize,
        width: f32,
        force_ellipsis: bool,
    ) -> String {
        if !width.is_finite() || width <= 0.0 {
            return String::new();
        }
        let (start_byte, base) = self.prefixes[start];
        if !force_ellipsis && self.prefixes[end].1 - base <= width {
            return text[start_byte..self.prefixes[end].0].to_owned();
        }
        if self.ellipsis_width > width {
            return String::new();
        }
        let end_byte = self.prefixes[start..=end]
            .iter()
            .take_while(|(_, advance)| advance - base + self.ellipsis_width <= width)
            .last()
            .map_or(start_byte, |(end, _)| *end);
        format!("{}…", &text[start_byte..end_byte])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::UiEvent;
    use crate::input::event_ctx::{EventCtx, EventHandled};
    use crate::layout::Point;

    #[test]
    fn uncapped_wrapping_prefers_words_without_losing_whitespace_or_long_tokens() {
        let text = "a word longtoken\n项目 text";
        let geometry = LabelGeometry::measure(text, &mut |_| 1.0);
        assert_eq!(
            geometry.wrapped("a word longtoken\n项目 text", 5.0, None),
            ["a ", "word ", "longt", "oken", "项目 ", "text"]
        );
        let lines = geometry.wrapped(text, 5.0, None);
        assert_eq!(lines.concat(), text.replace('\n', ""));
        assert_eq!(
            geometry.wrapped(text, 100.0, None),
            ["a word longtoken", "项目 text"]
        );
    }
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
    fn wrapped_lines_use_proportional_advances_and_mark_capped_overflow() {
        let advance = |ch| match ch {
            'i' => 1.0,
            'W' => 8.0,
            '…' => 3.0,
            '\r' | '\n' => 0.0,
            _ => 2.0,
        };
        for (text, width, limit, expected) in [
            ("iiiiWW", 10.0, None, vec!["iiii", "W", "W"]),
            ("iiiiWW", 10.0, Some(2), vec!["iiii", "…"]),
            ("iiiiWW", 12.0, Some(2), vec!["iiiiW", "W"]),
            ("A\r\nB\nC\nD\nE", 50.0, Some(4), vec!["A", "B", "C", "D…"]),
            ("a\n", 50.0, None, vec!["a", ""]),
            ("", 50.0, None, vec![""]),
            ("text", 0.0, Some(4), vec![]),
            ("text", 50.0, Some(0), vec![]),
        ] {
            let geometry = LabelGeometry::measure(text, &mut |ch| advance(ch));
            assert_eq!(geometry.wrapped(text, width, limit), expected, "{text:?}");
        }
        let text = "e\u{0301}👩\u{200d}💻项目";
        let geometry = LabelGeometry::measure(text, &mut |ch| match ch {
            '\u{0301}' | '\u{200d}' => 0.0,
            '…' => 1.0,
            _ => 2.0,
        });
        assert_eq!(
            geometry.wrapped(text, 4.0, None),
            ["e\u{0301}", "👩\u{200d}💻", "项目"]
        );
        assert_eq!(geometry.wrapped(text, 4.0, Some(2)), ["e\u{0301}", "…"]);
        // Truncation never mutates the source/geometry used by wider layouts.
        assert_eq!(geometry.wrapped(text, 50.0, Some(4)), [text]);
    }

    #[test]
    fn uncapped_wrap_preserves_oversized_graphemes_and_remeasures_height() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let source = "iiiiWe\u{0301}👩\u{200d}💻项目\r\n\nlast\n";
        let label = TextLabel::new(source).ellipsis().wrap();
        label.prepare_text_geometry(&metrics, &mut |ch| match ch {
            'i' => 1.0,
            'W' => 8.0,
            '\u{0301}' | '\u{200d}' | '\r' | '\n' => 0.0,
            _ => 2.0,
        });
        for width in [4.0, 1.0, 0.0, 80.0, f32::INFINITY, 4.0] {
            let size = label.intrinsic_size(
                BoxConstraints::loose(Size::new(width, f32::INFINITY)),
                &metrics,
            );
            let geometry = label.geometry.borrow();
            let lines = geometry.as_ref().unwrap().wrapped(source, width, None);
            assert_eq!(lines.concat(), source.replace(['\r', '\n'], ""));
            assert_eq!(size.height, lines.len() as f32 * metrics.line_height);
            assert!(lines.iter().all(|line| !line.contains('…')));
            let painted =
                label.paint_primitives(Rect::from_min_size(Point::new(3.0, 7.0), size), &metrics);
            assert_eq!(
                painted_text(&label, width),
                source.replace(['\r', '\n'], "")
            );
            assert!(painted.iter().all(|p| matches!(p,
                Primitive::Text { origin, .. } if origin.x == 3.0 && origin.y >= 7.0
            )));
        }
        assert_eq!(
            label
                .geometry
                .borrow()
                .as_ref()
                .unwrap()
                .wrapped(source, 4.0, None),
            [
                "iiii",
                "W",
                "e\u{0301}",
                "👩\u{200d}💻",
                "项目",
                "",
                "la",
                "st",
                ""
            ]
        );
        // The explicit single-line mode remains available (last builder wins).
        let single = label.ellipsis();
        assert_eq!(
            single
                .intrinsic_size(
                    BoxConstraints::loose(Size::new(4.0, f32::INFINITY)),
                    &metrics,
                )
                .height,
            metrics.line_height
        );
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
        let label = TextLabel::new("X").foreground(color).expect("valid color");
        let rect = Rect::from_min_size(Point::ZERO, Size::new(10.0, 20.0));
        let prims = label.paint_primitives(rect, &crate::runtime::DEFAULT_TEXT_METRICS);
        match &prims[0] {
            Primitive::Text { color: c, .. } => {
                assert_eq!(*c, color);
            }
            _ => panic!("expected Text"),
        }
    }
    #[test]
    fn scaled_labels_wrap_at_local_advances_and_keep_scaled_line_height() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let label = TextLabel::new("WiWi").wrap().scale(1.5).unwrap();
        // Runtime supplies actual advances at the requested local font scale.
        label.prepare_text_geometry(&metrics, &mut |ch| if ch == 'W' { 12.0 } else { 3.0 });
        assert_eq!(label.text_scale(), 1.5);
        let narrow = label.intrinsic_size(BoxConstraints::loose(Size::new(15.0, 500.0)), &metrics);
        assert_eq!(narrow, Size::new(15.0, metrics.line_height * 3.0));
        let primitives = label.paint_primitives(Rect::from_min_size(Point::ZERO, narrow), &metrics);
        let runs: Vec<_> = primitives.iter().filter_map(Primitive::text_run).collect();
        assert_eq!(
            runs.iter().map(|run| run.0).collect::<Vec<_>>(),
            ["Wi", "Wi"]
        );
        assert!(runs.iter().all(|run| run.3 == 1.5));
        assert_eq!(runs[1].1.y - runs[0].1.y, metrics.line_height * 1.5);
        let wide = label.intrinsic_size(BoxConstraints::loose(Size::new(100.0, 500.0)), &metrics);
        assert_eq!(wide, Size::new(30.0, metrics.line_height * 1.5));
    }

    #[test]
    fn label_scale_validates_and_resets_prepared_geometry_without_affecting_other_labels() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let bounds = BoxConstraints::loose(Size::new(1000.0, 1000.0));
        for scale in [-1.0, f32::INFINITY, f32::NAN] {
            let error = TextLabel::new("test").scale(scale).err().unwrap();
            assert_eq!(error.field, "TextLabel.scale");
        }
        let label = TextLabel::new("WW");
        label.prepare_text_geometry(&metrics, &mut |_| 2.0);
        let scaled = label.clone().scale(2.0).unwrap();
        assert_eq!(
            scaled.intrinsic_size(bounds, &metrics).width,
            4.0 * metrics.cell_width
        );
        assert_eq!(label.intrinsic_size(bounds, &metrics).width, 4.0);
        let hidden = scaled.scale(0.0).unwrap();
        assert_eq!(hidden.intrinsic_size(bounds, &metrics), Size::ZERO);
        assert!(
            hidden
                .paint_primitives(
                    Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0)),
                    &metrics
                )
                .is_empty()
        );
    }
}
