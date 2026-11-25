use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use harbor_terminal::safe_preview_line;
use harbor_widget::ScrollController;
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

pub(crate) fn wrap_escaped_lines_to_width(
    escaped_lines: &[String],
    max_width: f32,
    mut char_width: impl FnMut(char) -> f32,
) -> Vec<PreviewRow> {
    let max_width = if max_width.is_finite() {
        max_width.max(0.0)
    } else {
        f32::INFINITY
    };
    let mut rows = Vec::with_capacity(escaped_lines.len());
    for (index, escaped) in escaped_lines.iter().enumerate() {
        let logical_line = index + 1;
        let mut is_first = true;
        let mut current_line = String::new();
        let mut current_width = 0.0f32;
        for ch in escaped.chars() {
            let measured = char_width(ch);
            let ch_width = if measured.is_finite() && measured > 0.0 {
                measured
            } else {
                0.0
            };
            // Keep zero-width combining marks attached to their base glyph even at the edge,
            // and never emit an empty continuation row for an oversized single glyph.
            if ch_width > 0.0 && current_width + ch_width > max_width && !current_line.is_empty() {
                rows.push(PreviewRow {
                    line_number: is_first.then_some(logical_line),
                    text: std::mem::take(&mut current_line),
                });
                is_first = false;
                current_width = 0.0;
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

#[derive(Clone)]
struct AdvanceTable {
    ascii: [Option<f32>; 128],
    non_ascii: HashMap<char, f32>,
    fallback: f32,
    max_digit_width: f32,
}

impl AdvanceTable {
    fn measure(
        escaped_lines: &[String],
        fallback: f32,
        advance: &mut dyn FnMut(char) -> f32,
    ) -> Self {
        let fallback = if fallback.is_finite() && fallback > 0.0 {
            fallback
        } else {
            1.0
        };
        let sanitize = |value: f32| {
            if value.is_finite() && value >= 0.0 {
                value
            } else {
                fallback
            }
        };
        let mut ascii = [None; 128];
        let mut max_digit_width = 0.0f32;
        for digit in '0'..='9' {
            let width = sanitize(advance(digit));
            ascii[digit as usize] = Some(width);
            max_digit_width = max_digit_width.max(width);
        }
        if max_digit_width <= 0.0 {
            max_digit_width = fallback;
        }
        let mut non_ascii = HashMap::new();
        for line in escaped_lines {
            for ch in line.chars() {
                if ch.is_ascii() {
                    let slot = &mut ascii[ch as usize];
                    if slot.is_none() {
                        *slot = Some(sanitize(advance(ch)));
                    }
                } else {
                    non_ascii.entry(ch).or_insert_with(|| sanitize(advance(ch)));
                }
            }
        }
        Self {
            ascii,
            non_ascii,
            fallback,
            max_digit_width,
        }
    }

    fn width_of(&self, ch: char) -> f32 {
        if ch.is_ascii() {
            self.ascii[ch as usize].unwrap_or(self.fallback)
        } else {
            self.non_ascii.get(&ch).copied().unwrap_or(self.fallback)
        }
    }

    fn text_width(&self, text: &str) -> f32 {
        text.chars().map(|ch| self.width_of(ch)).sum()
    }
}

/// Scrollable confirmation preview content rendered inside `ScrollView`.
///
/// Draws a logical line-number gutter, subtle vertical divider, alternating row
/// tinting, and escaped preview lines wrapped by measured glyph advance without
/// altering raw payloads.
type WrapCache = Rc<RefCell<Option<(u32, u32, Vec<PreviewRow>)>>>;

#[derive(Clone)]
pub struct PreviewContent {
    escaped_lines: Arc<Vec<String>>,
    scroll: ScrollController,
    row_tint: Color,
    gutter_color: Color,
    text_color: Color,
    gutter_divider: Color,
    advances: Rc<RefCell<Option<AdvanceTable>>>,
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
            advances: Rc::new(RefCell::new(None)),
            cache: Rc::new(RefCell::new(None)),
        }
    }

    pub fn row_height(metrics: &TextMetrics) -> f32 {
        metrics.line_height + ROW_VERTICAL_PADDING * 2.0
    }

    fn fallback_width(metrics: &TextMetrics) -> f32 {
        if metrics.cell_width.is_finite() && metrics.cell_width > 0.0 {
            metrics.cell_width
        } else {
            1.0
        }
    }

    fn gutter_digits(&self) -> usize {
        let count = self.escaped_lines.len().max(1);
        ((count.ilog10() as usize) + 1).max(2)
    }

    fn digit_width(&self, metrics: &TextMetrics) -> f32 {
        self.advances
            .borrow()
            .as_ref()
            .map_or_else(|| Self::fallback_width(metrics), |t| t.max_digit_width)
    }

    fn gutter_width(&self, metrics: &TextMetrics) -> f32 {
        GUTTER_LEFT_PADDING
            + self.gutter_digits() as f32 * self.digit_width(metrics)
            + GUTTER_RIGHT_PADDING
    }

    fn text_width(&self, text: &str, metrics: &TextMetrics) -> f32 {
        if let Some(table) = self.advances.borrow().as_ref() {
            table.text_width(text)
        } else {
            text.chars().count() as f32 * Self::fallback_width(metrics)
        }
    }

    fn available_text_width(&self, width: f32, metrics: &TextMetrics) -> f32 {
        if !width.is_finite() {
            return DEFAULT_WRAP_COLUMNS as f32 * Self::fallback_width(metrics);
        }
        (width - self.gutter_width(metrics) - TEXT_LEFT_GAP - SCROLLBAR_RESERVED).max(0.0)
    }

    fn rows_for_width(
        &self,
        max_text_width: f32,
        metrics: &TextMetrics,
    ) -> std::cell::Ref<'_, Vec<PreviewRow>> {
        let max_width = if max_text_width.is_finite() {
            max_text_width.max(0.0)
        } else {
            f32::INFINITY
        };
        let fallback = Self::fallback_width(metrics);
        let width_bits = max_width.to_bits();
        let fallback_bits = fallback.to_bits();
        {
            let mut cache = self.cache.borrow_mut();
            let needs_update = cache.as_ref().is_none_or(|(cached_w, cached_fb, _)| {
                *cached_w != width_bits || *cached_fb != fallback_bits
            });
            if needs_update {
                let advances = self.advances.borrow();
                let rows = match advances.as_ref() {
                    Some(table) => {
                        wrap_escaped_lines_to_width(&self.escaped_lines, max_width, |ch| {
                            table.width_of(ch)
                        })
                    }
                    None => {
                        wrap_escaped_lines_to_width(&self.escaped_lines, max_width, |_| fallback)
                    }
                };
                *cache = Some((width_bits, fallback_bits, rows));
            }
        }
        std::cell::Ref::map(self.cache.borrow(), |entry| {
            &entry.as_ref().expect("populated wrap cache").2
        })
    }
}

impl Component for PreviewContent {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::leaf(self.clone())
    }
}

impl PrimitiveLeaf for PreviewContent {
    fn prepare_text_geometry(&self, metrics: &TextMetrics, advance: &mut dyn FnMut(char) -> f32) {
        *self.advances.borrow_mut() = Some(AdvanceTable::measure(
            &self.escaped_lines,
            metrics.cell_width,
            advance,
        ));
        *self.cache.borrow_mut() = None;
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let max_text_width = self.available_text_width(constraints.max.width, metrics);
        let rows = self.rows_for_width(max_text_width, metrics);
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            let max_line_width = rows
                .iter()
                .map(|row| self.text_width(&row.text, metrics))
                .fold(0.0f32, f32::max);
            self.gutter_width(metrics) + TEXT_LEFT_GAP + max_line_width + SCROLLBAR_RESERVED
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
        let max_text_width = self.available_text_width(size.width, metrics);
        let rows = self.rows_for_width(max_text_width, metrics);
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
                let label_width = self.text_width(&label, metrics);
                let num_x = (rect.min.x + gutter_w - GUTTER_RIGHT_PADDING - label_width)
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

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::runtime::DEFAULT_TEXT_METRICS;

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
    fn preview_content_wraps_proportional_ascii_and_cjk_by_measured_width() {
        let escaped: Arc<Vec<String>> = Arc::new(
            ["x".repeat(100), "你".repeat(50)]
                .into_iter()
                .map(|line| safe_preview_line(&line))
                .collect(),
        );
        let content = PreviewContent::new(
            escaped,
            ScrollController::new(),
            Color::WHITE,
            Color::WHITE,
            Color::WHITE,
            Color::TRANSPARENT,
        );
        let metrics = TextMetrics {
            cell_width: 11.0,
            ..DEFAULT_TEXT_METRICS
        };
        content.prepare_text_geometry(&metrics, &mut |ch| match ch {
            '0'..='9' => 6.5,
            'x' => 5.5,
            '你' => 12.0,
            _ => 11.0,
        });

        // Gutter = 10 + 2*6.5 + 10 = 33; text_left_gap = 10; scrollbar = 14 -> available = 493.0.
        let size = content.intrinsic_size(BoxConstraints::loose(Size::new(550.0, 500.0)), &metrics);
        let prims = content.paint_primitives(Rect::from_min_size(Point::ZERO, size), &metrics);
        let body_texts: Vec<&str> = prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, origin, .. } if origin.x > 40.0 => Some(text.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(
            body_texts,
            vec![
                "x".repeat(89),
                "x".repeat(11),
                "你".repeat(41),
                "你".repeat(9),
            ]
        );
    }

    #[test]
    fn wrap_escaped_lines_to_width_keeps_combining_marks_and_oversized_glyphs() {
        let lines = ["a\u{0301}b".to_owned(), "Wx".to_owned()];
        let combining = wrap_escaped_lines_to_width(&lines[..1], 10.0, |ch| match ch {
            '\u{0301}' => 0.0,
            _ => 10.0,
        });
        assert_eq!(
            combining,
            vec![
                PreviewRow {
                    line_number: Some(1),
                    text: "a\u{0301}".into(),
                },
                PreviewRow {
                    line_number: None,
                    text: "b".into(),
                },
            ]
        );

        let oversized = wrap_escaped_lines_to_width(&lines[1..], 10.0, |ch| match ch {
            'W' => 15.0,
            _ => 5.0,
        });
        assert_eq!(
            oversized,
            vec![
                PreviewRow {
                    line_number: Some(1),
                    text: "W".into(),
                },
                PreviewRow {
                    line_number: None,
                    text: "x".into(),
                },
            ]
        );
    }

    #[test]
    fn prepare_text_geometry_invalidates_wrap_cache_and_aligns_gutter_with_digit_width() {
        let escaped: Arc<Vec<String>> = Arc::new(
            (1..=10)
                .map(|i| {
                    if i == 1 {
                        "x".repeat(20)
                    } else {
                        format!("line {i}")
                    }
                })
                .collect(),
        );
        let gutter = Color {
            r: 0.7,
            g: 0.8,
            b: 0.9,
            a: 1.0,
        };
        let divider = Color {
            r: 0.2,
            g: 0.2,
            b: 0.2,
            a: 1.0,
        };
        let content = PreviewContent::new(
            escaped,
            ScrollController::new(),
            Color::WHITE,
            gutter,
            divider,
            Color::TRANSPARENT,
        );
        let metrics = TextMetrics {
            cell_width: 11.0,
            ..DEFAULT_TEXT_METRICS
        };

        // First pass: digit width = 6.5 -> gutter_w = 33.0; available = 164 - 33 - 10 - 14 = 107.0.
        content.prepare_text_geometry(&metrics, &mut |ch| match ch {
            '0'..='9' => 6.5,
            'x' => 5.5,
            _ => 6.0,
        });
        let size = content.intrinsic_size(BoxConstraints::loose(Size::new(164.0, 500.0)), &metrics);
        let prims = content.paint_primitives(Rect::from_min_size(Point::ZERO, size), &metrics);

        let divider_x = prims
            .iter()
            .find_map(|p| match p {
                Primitive::Quad { rect, color, .. } if *color == divider => Some(rect.min.x),
                _ => None,
            })
            .unwrap();
        assert_eq!(divider_x, 33.0);

        let gutter_origins: Vec<(&str, f32)> = prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Text {
                    text,
                    origin,
                    color,
                } if *color == gutter => Some((text.as_ref(), origin.x)),
                _ => None,
            })
            .collect();
        assert_eq!(gutter_origins[0], ("1", 33.0 - 10.0 - 6.5));
        assert_eq!(gutter_origins[9], ("10", 33.0 - 10.0 - 13.0));

        let first_pass_x_rows: Vec<&str> = prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, color, .. }
                    if *color == Color::WHITE && text.starts_with('x') =>
                {
                    Some(text.as_ref())
                }
                _ => None,
            })
            .collect();
        assert_eq!(first_pass_x_rows, vec!["x".repeat(19), "x".to_owned()]);

        // Re-prepare with wider glyphs: cache must invalidate and wrap into 2 rows of 9/9/2.
        content.prepare_text_geometry(&metrics, &mut |ch| match ch {
            '0'..='9' => 8.0,
            'x' => 11.0,
            _ => 8.0,
        });
        let size = content.intrinsic_size(BoxConstraints::loose(Size::new(164.0, 500.0)), &metrics);
        let prims = content.paint_primitives(Rect::from_min_size(Point::ZERO, size), &metrics);
        let second_pass_x_rows: Vec<&str> = prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, color, .. }
                    if *color == Color::WHITE && text.starts_with('x') =>
                {
                    Some(text.as_ref())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            second_pass_x_rows,
            vec!["x".repeat(9), "x".repeat(9), "x".repeat(2)]
        );
    }
}
