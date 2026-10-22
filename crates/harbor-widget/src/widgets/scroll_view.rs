use crate::decoration::{BorderRadius, ClipBehavior};
use crate::input::event::{
    Key, KeyboardEvent, PointerBoundaryKind, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size};
use crate::scene::clip::RoundedClip;
use crate::scene::primitive::{Color, Primitive};
use crate::signal::Signal;
use crate::text::TextMetrics;
use crate::view::{
    AnyView, BuildCx, Component, EnsureVisibleResult, PaintPhase, ScrollableView, View,
};
use std::cell::Cell;
use std::rc::Rc;

/// Default distance, in logical pixels, moved by one wheel line or arrow key.
pub const DEFAULT_SCROLL_LINE_STEP: f32 = 40.0;

/// Committed vertical geometry for a [`ScrollView`].
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

/// Cloneable state model for one vertical [`ScrollView`].
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

    /// Minimally reveals a vertical range in content-local logical pixels.
    /// Oversized ranges align their leading edge. Uses the latest committed bounds.
    pub fn reveal_range(&self, start: f32, end: f32) -> bool {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            return false;
        }
        let metrics = self.metrics();
        if metrics.viewport_extent <= 0.0 {
            return false;
        }
        let offset = self.requested().min(metrics.max_scroll_extent);
        let requested = if end - start > metrics.viewport_extent || start < offset {
            start
        } else if end > finite_add(offset, metrics.viewport_extent) {
            end - metrics.viewport_extent
        } else {
            return false;
        };
        self.jump_to(requested.clamp(0.0, metrics.max_scroll_extent))
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

/// A hard-clipped, single-child vertical viewport with an overflow-only overlay scrollbar.
/// The thumb can be dragged; clicking its track pages the viewport without changing focus.
/// At rest only a thin thumb is painted; hovering the track or interacting expands it.
///
/// Positive wheel deltas follow the platform convention and move content toward
/// the start (decreasing the offset); negative deltas move toward the end.
/// Line-wheel and arrow movement use `line_step`; pixel-wheel deltas are logical
/// pixels. Page keys move by one committed viewport extent. Input is consumed only
/// when the effective offset can change, allowing an outer scroll area to handle
/// boundary input.
#[derive(Clone)]
pub struct ScrollView {
    controller: Option<ScrollController>,
    line_step: f32,
    child: Option<View>,
}

impl Default for ScrollView {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollView {
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

impl Component for ScrollView {
    fn build(&self, cx: &mut BuildCx) -> View {
        // Keep these hooks before the optional controller hook so switching controller
        // ownership never changes the type occupying an existing hook slot.
        let gesture = cx.use_state(|| Rc::new(Cell::new(None))).read().clone();
        let hovered = cx.use_state(|| Rc::new(Cell::new(false))).read().clone();
        let controller = self.controller.clone().unwrap_or_else(|| {
            let state = cx.use_state(ScrollController::new);
            state.read().clone()
        });
        cx.track(&controller.requested_offset);
        View::new(
            ScrollViewport {
                controller,
                line_step: self.line_step,
                scrollbar_color: cx.theme().button.normal.foreground,
                gesture,
                hovered,
            },
            self.child.iter().cloned().collect(),
            None,
        )
    }
}

impl crate::WithChildren for ScrollView {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("ScrollView", &mut self.child)?;
        Ok(self)
    }
}

#[derive(Clone)]
struct ScrollViewport {
    controller: ScrollController,
    line_step: f32,
    scrollbar_color: Color,
    gesture: Rc<Cell<Option<ScrollbarGesture>>>,
    hovered: Rc<Cell<bool>>,
}

impl AnyView for ScrollViewport {
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

    fn paint_primitives_for_phase(
        &self,
        phase: PaintPhase,
        rect: Rect,
        _metrics: &TextMetrics,
    ) -> Vec<Primitive> {
        if phase != PaintPhase::AfterChildren {
            return Vec::new();
        }
        let Some(geometry) = ScrollbarGeometry::new(rect, self.controller.metrics()) else {
            return Vec::new();
        };
        let expanded = self.hovered.get() || self.gesture.get().is_some();
        let mut primitives = Vec::with_capacity(2);
        if expanded {
            primitives.push(Primitive::Quad {
                rect: geometry.track,
                color: Color {
                    a: 0.06,
                    ..self.scrollbar_color
                },
                corner_radius: 3.0,
            });
        }
        let mut thumb = geometry.thumb;
        if !expanded {
            // Keep the hit target and content layout unchanged when the thumb narrows.
            thumb.min.x = thumb.max.x - thumb.size().width.min(3.0);
        }
        primitives.push(Primitive::Quad {
            rect: thumb,
            color: Color {
                a: if self.gesture.get().is_some() {
                    0.6
                } else {
                    0.35
                },
                ..self.scrollbar_color
            },
            corner_radius: 3.0,
        });
        primitives
    }

    fn accepts_pointer_boundaries(&self) -> bool {
        true
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

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        let position = match event {
            UiEvent::Pointer(pointer) => {
                Some((pointer.phase != PointerPhase::Cancel).then_some(pointer.position))
            }
            UiEvent::PointerBoundary(boundary) => Some(
                (boundary.kind == PointerBoundaryKind::Enter)
                    .then_some(boundary.position)
                    .flatten(),
            ),
            _ => None,
        };
        if let Some(position) = position {
            let hovered = position.is_some_and(|point| {
                ScrollbarGeometry::new(rect, self.controller.metrics())
                    .is_some_and(|geometry| geometry.track.contains(point))
            });
            if self.hovered.replace(hovered) != hovered {
                ctx.invalidate_paint();
            }
        }
        // The overlay owns its track even when a descendant control would consume the press.
        if let UiEvent::Pointer(pointer) = event
            && self.handle_scrollbar(pointer, ctx, rect)
        {
            ctx.invalidate_paint();
            ctx.stop_propagation();
            return EventHandled::Handled;
        }
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
        let (content_extent, offset) = child_rects
            .first()
            .filter(|child| valid_rect(**child))
            .map_or((0.0, 0.0), |child| {
                (child.size().height, rect.min.y - child.min.y)
            });
        self.controller
            .commit_metrics(rect.size().height, content_extent, offset);
    }

    fn as_scrollable(&self) -> Option<&dyn ScrollableView> {
        Some(self)
    }
}

#[derive(Clone, Copy)]
struct ScrollbarGesture {
    pointer_id: u64,
    // None captures a track click until release, without turning it into a drag.
    grab_fraction: Option<f32>,
}

struct ScrollbarGeometry {
    track: Rect,
    thumb: Rect,
}

impl ScrollbarGeometry {
    fn new(rect: Rect, metrics: ScrollMetrics) -> Option<Self> {
        if !valid_rect(rect) || rect.size().is_empty() || metrics.max_scroll_extent <= 0.0 {
            return None;
        }
        let height = rect.size().height;
        let width = rect.size().width.min(10.0);
        let inset = (width / 4.0).min(2.0);
        let thumb_height = ((f64::from(height) * f64::from(metrics.viewport_extent)
            / f64::from(metrics.content_extent)) as f32)
            .clamp(24.0_f32.min(height / 2.0), height);
        let travel = height - thumb_height;
        let top = rect.min.y + travel * (metrics.offset / metrics.max_scroll_extent);
        Some(Self {
            track: Rect::from_min_size(
                Point::new(rect.max.x - width, rect.min.y),
                Size::new(width, height),
            ),
            thumb: Rect::from_min_size(
                Point::new(rect.max.x - width + inset, top),
                Size::new(width - inset * 2.0, thumb_height),
            ),
        })
    }
}

impl ScrollViewport {
    fn handle_scrollbar(&self, pointer: &PointerEvent, ctx: &mut EventCtx, rect: Rect) -> bool {
        let geometry = ScrollbarGeometry::new(rect, self.controller.metrics());
        if let Some(gesture) = self.gesture.get() {
            if gesture.pointer_id != pointer.pointer_id {
                return false;
            }
            match pointer.phase {
                PointerPhase::Move => {
                    if let (Some(grab), Some(geometry)) = (gesture.grab_fraction, geometry) {
                        let travel = geometry.track.size().height - geometry.thumb.size().height;
                        if travel > 0.0 && pointer.position.y.is_finite() {
                            let position = pointer.position.y
                                - geometry.track.min.y
                                - grab * geometry.thumb.size().height;
                            self.controller.jump_to(
                                (position / travel).clamp(0.0, 1.0)
                                    * self.controller.metrics().max_scroll_extent,
                            );
                        }
                    }
                    return true;
                }
                PointerPhase::Up if pointer.button == PointerButton::Left => {}
                PointerPhase::Cancel => {}
                _ => return false,
            }
            self.gesture.set(None);
            ctx.release_pointer(pointer.pointer_id);
            return true;
        }
        let Some(geometry) = geometry else {
            return false;
        };
        if pointer.phase != PointerPhase::Down
            || pointer.button != PointerButton::Left
            || pointer.modifiers != Default::default()
            || !geometry.track.contains(pointer.position)
        {
            return false;
        }
        // The entire horizontal thumb band is draggable, including its visual inset.
        let in_thumb =
            pointer.position.y >= geometry.thumb.min.y && pointer.position.y < geometry.thumb.max.y;
        let grab_fraction = if in_thumb {
            Some((pointer.position.y - geometry.thumb.min.y) / geometry.thumb.size().height)
        } else {
            let direction = if pointer.position.y < geometry.thumb.min.y {
                -1.0
            } else {
                1.0
            };
            self.controller
                .scroll_by(direction * self.controller.metrics().viewport_extent);
            None
        };
        self.gesture.set(Some(ScrollbarGesture {
            pointer_id: pointer.pointer_id,
            grab_fraction,
        }));
        ctx.capture_pointer(pointer.pointer_id);
        true
    }
}
impl ScrollableView for ScrollViewport {
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
    fn post_layout_uses_one_valid_child_for_both_committed_metrics() {
        let controller = ScrollController::new();
        let view = ScrollViewport {
            controller: controller.clone(),
            line_step: DEFAULT_SCROLL_LINE_STEP,
            scrollbar_color: Color::WHITE,
            gesture: Rc::new(Cell::new(None)),
            hovered: Rc::new(Cell::new(false)),
        };
        let viewport = Rect::from_min_size(Point::new(5.0, 100.0), Size::new(50.0, 40.0));
        let child = Rect::from_min_size(Point::new(5.0, 80.0), Size::new(50.0, 100.0));
        view.post_layout(viewport, &[child]);
        assert_eq!(controller.metrics().content_extent(), 100.0);
        assert_eq!(controller.metrics().offset(), 20.0);
        assert_eq!(controller.metrics().max_scroll_extent(), 60.0);

        let invalid = Rect {
            max: Point::new(f32::NAN, 180.0),
            ..child
        };
        let inverted = Rect {
            max: Point::new(4.0, 180.0),
            ..child
        };
        let empty = Rect::from_min_size(viewport.min, Size::ZERO);
        for children in [vec![], vec![invalid], vec![inverted], vec![empty]] {
            view.post_layout(viewport, &children);
            assert_eq!(controller.metrics().viewport_extent(), 40.0);
            assert_eq!(controller.metrics().content_extent(), 0.0);
            assert_eq!(controller.metrics().offset(), 0.0);
            assert_eq!(controller.offset(), 0.0);
        }
    }

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
