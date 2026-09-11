use crate::decoration::{BorderRadius, ClipBehavior};
use crate::input::event::{Key, KeyboardEvent, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size};
use crate::scene::clip::RoundedClip;
use crate::signal::Signal;
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, EnsureVisibleResult, View};
use std::cell::Cell;
use std::rc::Rc;

/// Default distance, in logical pixels, moved by one wheel line or arrow key.
pub const DEFAULT_SCROLL_LINE_STEP: f32 = 40.0;

/// Committed vertical geometry for a [`ScrollArea`].
///
/// Values are always finite and non-negative. Metrics change only after a
/// successful layout commit and do not themselves invalidate the widget tree.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollMetrics {
    offset: f32,
    viewport_extent: f32,
    content_extent: f32,
    max_scroll_extent: f32,
}

impl ScrollMetrics {
    /// Returns the effective committed vertical offset.
    pub fn offset(self) -> f32 {
        self.offset
    }

    /// Returns the committed viewport height.
    pub fn viewport_extent(self) -> f32 {
        self.viewport_extent
    }

    /// Returns the committed full content height.
    pub fn content_extent(self) -> f32 {
        self.content_extent
    }

    /// Returns `max(content_extent - viewport_extent, 0)`.
    pub fn max_scroll_extent(self) -> f32 {
        self.max_scroll_extent
    }
}

/// Cloneable state model for one vertical [`ScrollArea`].
///
/// `jump_to` stores a normalized request. The next successful layout clamps it
/// to the current content extent. Wheel and keyboard operations use committed
/// metrics and therefore report whether they could actually move the viewport.
#[derive(Clone)]
pub struct ScrollController {
    requested_offset: Signal<f32>,
    metrics: Rc<Cell<ScrollMetrics>>,
}

impl Default for ScrollController {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollController {
    /// Creates a controller at offset zero with no committed extents.
    pub fn new() -> Self {
        Self {
            requested_offset: Signal::new_distinct(0.0),
            metrics: Rc::new(Cell::new(ScrollMetrics::default())),
        }
    }

    /// Returns the normalized requested offset. After a successful commit this
    /// equals [`ScrollMetrics::offset`].
    pub fn offset(&self) -> f32 {
        *self.requested_offset.read()
    }

    /// Returns the latest successful layout snapshot.
    pub fn metrics(&self) -> ScrollMetrics {
        self.metrics.get()
    }

    /// Requests an absolute logical-pixel offset.
    ///
    /// Negative and NaN requests become zero; positive infinity saturates to
    /// `f32::MAX`. Returns whether the stored request changed.
    pub fn jump_to(&self, offset: f32) -> bool {
        self.set_requested(normalize_non_negative(offset))
    }

    /// Requests a relative logical-pixel movement using the last committed
    /// effective offset. The result is clamped to committed scroll bounds.
    pub fn scroll_by(&self, delta: f32) -> bool {
        if !delta.is_finite() || delta == 0.0 {
            return false;
        }
        let metrics = self.metrics.get();
        let requested = finite_add(metrics.offset, delta).clamp(0.0, metrics.max_scroll_extent);
        self.set_requested(requested)
    }

    fn set_requested(&self, offset: f32) -> bool {
        if *self.requested_offset.read() == offset {
            return false;
        }
        self.requested_offset.set(offset);
        true
    }

    fn requested(&self) -> f32 {
        normalize_non_negative(*self.requested_offset.read())
    }

    fn commit_metrics(&self, viewport_extent: f32, content_extent: f32, offset: f32) {
        let viewport_extent = normalize_non_negative(viewport_extent);
        let content_extent = normalize_non_negative(content_extent);
        let max_scroll_extent = max_scroll_extent(content_extent, viewport_extent);
        let offset = normalize_non_negative(offset).min(max_scroll_extent);
        self.metrics.set(ScrollMetrics {
            offset,
            viewport_extent,
            content_extent,
            max_scroll_extent,
        });
        self.set_requested(offset);
    }

    fn ensure_visible(&self, viewport: Rect, target: &mut Rect) -> bool {
        let metrics = self.metrics.get();
        if !valid_rect(viewport)
            || !valid_rect(*target)
            || viewport.size().is_empty()
            || target.size().is_empty()
        {
            return false;
        }

        let target_height = target.size().height;
        let viewport_height = viewport.size().height;
        let requested = if target_height > viewport_height || target.min.y < viewport.min.y {
            finite_add(metrics.offset, target.min.y - viewport.min.y)
        } else if target.max.y > viewport.max.y {
            finite_add(metrics.offset, target.max.y - viewport.max.y)
        } else {
            return false;
        };
        let requested = requested.clamp(0.0, metrics.max_scroll_extent);
        if !self.set_requested(requested) {
            return false;
        }
        let target_shift = metrics.offset - requested;
        target.min.y = finite_coordinate_add(target.min.y, target_shift);
        target.max.y = finite_coordinate_add(target.max.y, target_shift);
        true
    }
}

/// A hard-clipped, single-child vertical viewport.
///
/// Positive wheel deltas follow the platform convention and move content toward
/// the start (decreasing the offset); negative deltas move toward the end.
/// Line-wheel and arrow movement use `line_step`; pixel-wheel deltas are logical
/// pixels. Page keys move by one committed viewport extent. Input is consumed only
/// when the effective offset can change, allowing an outer scroll area to handle
/// boundary input.
#[derive(Clone)]
pub struct ScrollArea {
    controller: Option<ScrollController>,
    line_step: f32,
    child: Option<View>,
}

impl Default for ScrollArea {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollArea {
    /// Creates an empty vertical scroll area with the default line step.
    pub fn new() -> Self {
        Self {
            controller: None,
            line_step: DEFAULT_SCROLL_LINE_STEP,
            child: None,
        }
    }

    /// Uses an externally retained controller instead of internal hook state.
    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }

    /// Sets the logical-pixel distance for line-wheel and arrow movement.
    pub fn line_step(mut self, line_step: f32) -> Self {
        self.line_step = normalize_non_negative(line_step);
        self
    }

    /// Sets the single scrollable child.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for ScrollArea {
    fn build(&self, cx: &mut BuildCx) -> View {
        let controller = self.controller.clone().unwrap_or_else(|| {
            let state = cx.use_state(ScrollController::new);
            state.read().clone()
        });
        cx.track(&controller.requested_offset);
        View::new(
            ScrollAreaView {
                controller,
                line_step: self.line_step,
            },
            self.child.iter().cloned().collect(),
            None,
        )
    }
}

impl crate::WithChildren for ScrollArea {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("ScrollArea", &mut self.child)?;
        Ok(self)
    }
}

#[derive(Clone)]
struct ScrollAreaView {
    controller: ScrollController,
    line_step: f32,
}

impl AnyView for ScrollAreaView {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.fill_bounded(Size::ZERO)
    }

    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _metrics: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        constraints.validate()?;
        if children.len() > 1 {
            return Err(LayoutError::InvalidChildIndex);
        }

        let child_size = if children.len() == 1 {
            children.measure(
                0,
                BoxConstraints {
                    min: Size::new(constraints.min.width, 0.0),
                    max: Size::new(constraints.max.width, f32::INFINITY),
                },
            )?
        } else {
            Size::ZERO
        };
        let viewport = constraints.fill_bounded(child_size);
        let offset = self
            .controller
            .requested()
            .min(max_scroll_extent(child_size.height, viewport.height));
        let placements = if children.len() == 1 {
            vec![(0, Point::new(0.0, -offset))]
        } else {
            Vec::new()
        };
        Ok(ParentLayout {
            size: viewport,
            placements,
            diagnostics: Vec::new(),
        })
    }

    fn descendant_clip(&self, rect: Rect) -> Option<RoundedClip> {
        if !valid_rect(rect) {
            return None;
        }
        RoundedClip::new(
            rect,
            BorderRadius::all(0.0).expect("zero radius is valid"),
            ClipBehavior::HardEdge,
        )
        .ok()
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        if ctx.is_capture_phase() {
            return EventHandled::Ignored;
        }
        let delta_or_target = match event {
            UiEvent::Pointer(pointer)
                if pointer.modifiers == Default::default()
                    && matches!(
                        pointer.phase,
                        PointerPhase::WheelLine { .. } | PointerPhase::WheelPixel { .. }
                    ) =>
            {
                match pointer.phase {
                    PointerPhase::WheelLine { dx: _, dy } if dy != 0.0 => {
                        Some(ScrollRequest::Delta(-dy * self.line_step))
                    }
                    PointerPhase::WheelPixel { dx: _, dy } if dy != 0.0 => {
                        Some(ScrollRequest::Delta(-dy))
                    }
                    _ => None,
                }
            }
            UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers })
                if *modifiers == Default::default() =>
            {
                match key {
                    Key::ArrowUp => Some(ScrollRequest::Delta(-self.line_step)),
                    Key::ArrowDown => Some(ScrollRequest::Delta(self.line_step)),
                    Key::PageUp => Some(ScrollRequest::Delta(
                        -self.controller.metrics().viewport_extent,
                    )),
                    Key::PageDown => Some(ScrollRequest::Delta(
                        self.controller.metrics().viewport_extent,
                    )),
                    Key::Home => Some(ScrollRequest::Target(0.0)),
                    Key::End => Some(ScrollRequest::Target(
                        self.controller.metrics().max_scroll_extent,
                    )),
                    _ => None,
                }
            }
            _ => None,
        };

        let changed = match delta_or_target {
            Some(ScrollRequest::Delta(delta)) => self.controller.scroll_by(delta),
            Some(ScrollRequest::Target(target)) => self.controller.jump_to(target),
            None => false,
        };
        if changed {
            ctx.invalidate_paint();
            ctx.stop_propagation();
            EventHandled::Handled
        } else {
            EventHandled::Ignored
        }
    }

    fn post_layout(&self, rect: Rect, child_rects: &[Rect]) {
        let content_extent = child_rects
            .first()
            .filter(|rect| valid_rect(**rect))
            .map(|rect| rect.size().height)
            .unwrap_or(0.0);
        let offset = child_rects
            .first()
            .filter(|child| valid_rect(**child))
            .map(|child| rect.min.y - child.min.y)
            .unwrap_or(0.0);
        self.controller
            .commit_metrics(rect.size().height, content_extent, offset);
    }

    fn ensure_visible(&self, rect: Rect, target: &mut Rect) -> EnsureVisibleResult {
        if !valid_rect(rect) || rect.size().is_empty() {
            EnsureVisibleResult::Unavailable
        } else {
            EnsureVisibleResult::Resolved(self.controller.ensure_visible(rect, target))
        }
    }
}

enum ScrollRequest {
    Delta(f32),
    Target(f32),
}

fn normalize_non_negative(value: f32) -> f32 {
    if value.is_nan() || value <= 0.0 {
        0.0
    } else if value.is_infinite() {
        f32::MAX
    } else {
        value
    }
}

fn finite_add(left: f32, right: f32) -> f32 {
    (f64::from(left) + f64::from(right)).clamp(0.0, f64::from(f32::MAX)) as f32
}

fn finite_coordinate_add(left: f32, right: f32) -> f32 {
    (f64::from(left) + f64::from(right)).clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32
}

fn max_scroll_extent(content: f32, viewport: f32) -> f32 {
    let content = normalize_non_negative(content);
    let viewport = normalize_non_negative(viewport);
    (content - viewport).max(0.0)
}

fn valid_rect(rect: Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
        && rect.max.x >= rect.min.x
        && rect.max.y >= rect.min.y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_normalizes_requests_and_clamps_after_commit() {
        let controller = ScrollController::new();
        assert!(!controller.jump_to(f32::NAN));
        assert!(!controller.jump_to(-10.0));
        assert!(controller.jump_to(f32::INFINITY));
        controller.commit_metrics(40.0, 100.0, 60.0);
        assert_eq!(controller.offset(), 60.0);
        assert_eq!(controller.metrics().max_scroll_extent(), 60.0);
    }

    #[test]
    fn ensure_visible_uses_minimal_and_leading_edge_movement() {
        let controller = ScrollController::new();
        controller.commit_metrics(40.0, 200.0, 20.0);
        let viewport = Rect::from_min_size(Point::new(0.0, 100.0), Size::new(50.0, 40.0));

        let mut visible = Rect::from_min_size(Point::new(0.0, 110.0), Size::new(20.0, 10.0));
        assert!(!controller.ensure_visible(viewport, &mut visible));
        let mut below = Rect::from_min_size(Point::new(0.0, 135.0), Size::new(20.0, 20.0));
        assert!(controller.ensure_visible(viewport, &mut below));
        assert_eq!(controller.offset(), 35.0);

        controller.commit_metrics(40.0, 200.0, 35.0);
        let mut oversized = Rect::from_min_size(Point::new(0.0, 80.0), Size::new(20.0, 80.0));
        assert!(controller.ensure_visible(viewport, &mut oversized));
        assert_eq!(controller.offset(), 15.0);
    }
}
