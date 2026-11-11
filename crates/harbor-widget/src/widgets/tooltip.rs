use super::{MouseRegion, Portal, text_label::LabelGeometry};
use crate::input::event::{PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

/// Immediate, non-interactive hover help for an arbitrary single child.
///
/// The child keeps its constraints, size, focus and activation behavior. Help is
/// painted through a [`Portal`], outside ancestor scroll clips, preferring the
/// target's right side and staying inside the window. Long help wraps and is
/// ellipsized when the window cannot accommodate all of it.
///
/// Help disappears on exit, press, scrolling, or a committed target move. A new
/// pointer move can reveal it again. This wrapper adds no focus stop or capture.
#[derive(Clone)]
pub struct Tooltip {
    text: String,
    child: Option<View>,
}

impl Tooltip {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            child: None,
        }
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl crate::WithChildren for Tooltip {
    type Capability = crate::SingleChild;

    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("Tooltip", &mut self.child)?;
        Ok(self)
    }
}

#[derive(Default)]
struct Hover {
    target: Cell<Option<Rect>>,
    visible: Cell<bool>,
}

impl Hover {
    fn show(&self, visible: bool, cx: &mut EventCtx) {
        if self.visible.replace(visible) != visible {
            cx.invalidate_paint();
        }
    }
}

impl Component for Tooltip {
    fn build(&self, cx: &mut BuildCx) -> View {
        let hover = cx.use_state(|| Rc::new(Hover::default())).read().clone();
        let enter = hover.clone();
        let exit = hover.clone();
        let mut region = MouseRegion::new()
            .on_enter(move |cx| enter.show(true, cx))
            .on_exit(move |cx| exit.show(false, cx));
        if let Some(child) = &self.child {
            region = region.child(child.clone());
        }
        // Control themes may intentionally use transparent surfaces; hover help
        // needs an opaque backing rather than inheriting that interaction recipe.
        let mut colors = cx.theme().button.normal;
        if colors.background.a < 1.0 || colors.foreground.a < 1.0 {
            colors = crate::Theme::default().button.normal;
        }
        let hint = View::new(
            Hint {
                text: self.text.clone(),
                hover: hover.clone(),
                geometry: RefCell::new(None),
                foreground: colors.foreground,
                background: colors.background,
            },
            vec![],
            None,
        );
        View::new(
            Target { hover },
            vec![region.build(cx), Portal::new().child(hint).build(cx)],
            None,
        )
    }
}

struct Target {
    hover: Rc<Hover>,
}

impl AnyView for Target {
    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        // Portal edges are excluded from ordinary child measurement by layout.
        let size = children.measure(0, constraints)?;
        Ok(ParentLayout {
            size,
            placements: vec![(0, Point::ZERO)],
            diagnostics: vec![],
        })
    }

    fn post_layout(&self, rect: Rect, _: &[Rect]) {
        // LayoutObserver is deliberately not used here: its public callbacks run
        // AFTER paint and signal writes wait for another runtime turn. Internal
        // committed geometry is available before portal paint, so scroll/move
        // invalidation cannot leave a frame of help at the old target location.
        if self.hover.target.replace(Some(rect)) != Some(rect) {
            self.hover.visible.set(false);
        }
    }

    fn hit_test(&self, _: Point, _: Rect) -> bool {
        false
    }

    fn handle_event(&self, event: &UiEvent, cx: &mut EventCtx, rect: Rect) -> EventHandled {
        if cx.is_capture_phase()
            && let UiEvent::Pointer(pointer) = event
        {
            match pointer.phase {
                PointerPhase::Move => self.hover.show(rect.contains(pointer.position), cx),
                PointerPhase::Down
                | PointerPhase::Cancel
                | PointerPhase::WheelLine { .. }
                | PointerPhase::WheelPixel { .. } => self.hover.show(false, cx),
                _ => {}
            }
        }
        EventHandled::Ignored
    }
}

struct Hint {
    text: String,
    hover: Rc<Hover>,
    geometry: RefCell<Option<(LabelGeometry, f32)>>,
    foreground: Color,
    background: Color,
}

impl AnyView for Hint {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        // Portal supplies the viewport, not the declaring parent's allocation.
        constraints.fill_bounded(Size::ZERO)
    }

    fn hit_test(&self, _: Point, _: Rect) -> bool {
        // No interactive descendants either: even an overlapping hint cannot
        // replace the target's hover path, cursor, click target or focus owner.
        false
    }

    fn prepare_text_geometry(&self, _: &TextMetrics, advance: &mut dyn FnMut(char) -> f32) {
        *self.geometry.borrow_mut() = Some(measure_hint(&self.text, advance));
    }

    fn paint_primitives(&self, viewport: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let Some(target) = self.hover.target.get().filter(|_| self.hover.visible.get()) else {
            return vec![];
        };
        if self.text.is_empty() || !viewport.is_valid() || metrics.line_height <= 0.0 {
            return vec![];
        }
        let margin = 6.0_f32.min(viewport.size().width / 4.0);
        let max_width = 320.0_f32.min((viewport.size().width - margin * 2.0).max(0.0));
        let padding = 8.0_f32.min(max_width / 4.0);
        let spare_height = (viewport.size().height - metrics.line_height).max(0.0);
        let vertical_margin = 6.0_f32.min(spare_height / 4.0);
        let vertical_padding = 8.0_f32.min((spare_height - vertical_margin * 2.0) / 2.0);
        let text_width = (max_width - padding * 2.0).max(0.0);
        let max_lines = ((viewport.size().height - vertical_margin * 2.0 - vertical_padding * 2.0)
            / metrics.line_height)
            .max(0.0)
            .floor() as usize;
        if text_width == 0.0 || max_lines == 0 {
            return vec![];
        }
        let geometry = self.geometry.borrow();
        let fallback;
        let geometry = match geometry.as_ref() {
            Some(geometry) => geometry,
            None => {
                fallback = measure_hint(&self.text, &mut |_| metrics.cell_width);
                &fallback
            }
        };
        let lines = geometry.0.wrapped(&self.text, text_width, Some(max_lines));
        if lines.is_empty() {
            return vec![];
        }
        let width = (geometry.1 + padding * 2.0).min(max_width);
        let height = lines.len() as f32 * metrics.line_height + vertical_padding * 2.0;
        let preferred_x = target.max.x + 6.0;
        let x = if preferred_x + width <= viewport.max.x - margin {
            preferred_x
        } else {
            target.min.x - 6.0 - width
        }
        .clamp(viewport.min.x + margin, viewport.max.x - margin - width);
        let y = (target.min.y + (target.size().height - height) / 2.0).clamp(
            viewport.min.y + vertical_margin,
            viewport.max.y - vertical_margin - height,
        );
        let rect = Rect::from_min_size(Point::new(x, y), Size::new(width, height));
        let mut primitives = vec![Primitive::Quad {
            rect,
            color: self.background,
            corner_radius: 4.0,
        }];
        primitives.extend(
            lines
                .into_iter()
                .enumerate()
                .map(|(i, text)| Primitive::Text {
                    text: Arc::from(text),
                    origin: Point::new(
                        x + padding,
                        y + vertical_padding + i as f32 * metrics.line_height,
                    ),
                    color: self.foreground,
                }),
        );
        primitives
    }
}

fn measure_hint(text: &str, advance: &mut dyn FnMut(char) -> f32) -> (LabelGeometry, f32) {
    let geometry = LabelGeometry::measure(text, advance);
    let width = text
        .lines()
        .map(|line| line.chars().map(&mut *advance).sum::<f32>())
        .fold(0.0, f32::max);
    (geometry, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_width_uses_ui_glyph_advances_not_terminal_cells() {
        let hover = Rc::new(Hover::default());
        hover.target.set(Some(Rect::from_min_size(
            Point::ZERO,
            Size::new(20.0, 20.0),
        )));
        hover.visible.set(true);
        let hint = Hint {
            text: "Wiii".into(),
            hover,
            geometry: RefCell::new(None),
            foreground: Color::WHITE,
            background: Color::BLACK,
        };
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        hint.prepare_text_geometry(&metrics, &mut |ch| if ch == 'W' { 12.0 } else { 2.0 });
        let painted = hint.paint_primitives(
            Rect::from_min_size(Point::ZERO, Size::new(160.0, 80.0)),
            &metrics,
        );
        assert!(matches!(painted[0], Primitive::Quad { rect, .. } if rect.size().width == 34.0));
        assert!(matches!(&painted[1], Primitive::Text { text, .. } if text.as_ref() == "Wiii"));
    }
}
