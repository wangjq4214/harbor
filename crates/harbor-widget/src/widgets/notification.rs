//! Notification card presentation without application placement or eligibility policy.

/// Caller-chosen alignment and symmetric inset within the offered bounds.
/// Use directly in a layout, or place inside a caller-owned Portal to float.
#[derive(Clone, Copy, Debug)]
pub struct NotificationPlacement {
    pub horizontal: Alignment,
    pub vertical: Alignment,
    pub inset: f32,
}

/// Generic card dimensions in logical pixels and optional theme color overrides.
/// Dimensions must be finite and nonnegative; `max_width` must be positive.
#[derive(Clone, Debug)]
pub struct NotificationStyle {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub max_width: f32,
    pub corner_radius: f32,
    pub horizontal_padding: f32,
    pub header_height: f32,
    pub bottom_padding: f32,
    pub close_width: f32,
}

impl Default for NotificationStyle {
    fn default() -> Self {
        Self {
            foreground: None,
            background: None,
            max_width: 520.0,
            corner_radius: 6.0,
            horizontal_padding: 12.0,
            header_height: 32.0,
            bottom_padding: 12.0,
            close_width: 36.0,
        }
    }
}
struct Position(NotificationPlacement);
impl AnyView for Position {
    fn hit_test(&self, _: Point, _: Rect) -> bool {
        false
    }
    fn layout(
        &self,
        constraints: BoxConstraints,
        children: &mut dyn ChildMeasurer,
        _: &TextMetrics,
    ) -> Result<ParentLayout, LayoutError> {
        let size = constraints.fill_bounded(Size::ZERO);
        let inset = self.0.inset.min(size.width / 2.0).min(size.height / 2.0);
        let available = Size::new(
            (size.width - inset * 2.0).max(0.0),
            (size.height - inset * 2.0).max(0.0),
        );
        let card = children.measure(0, BoxConstraints::loose(available))?;
        Ok(ParentLayout {
            size,
            placements: vec![(
                0,
                Point::new(
                    inset + self.0.horizontal.position(card.width, available.width),
                    inset + self.0.vertical.position(card.height, available.height),
                ),
            )],
            diagnostics: Vec::new(),
        })
    }
}
use std::cell::Cell;
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

use crate::effects::CursorShape;
use crate::input::event::{PointerButton, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{
    Alignment, BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size,
};
use crate::scene::primitive::{Color, Primitive};
use crate::signal::Signal;
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// Reusable persistent notification content. Placement and Portal escape belong to the caller.
/// Eligibility and dismissal state belong to the caller. No timeout or focus
/// request is introduced. Long content wraps and can be scrolled with the wheel.
#[derive(Clone)]
pub struct Notification {
    message: String,
    on_dismiss: Arc<dyn Fn() + Send + Sync>,
    style: NotificationStyle,
    placement: Option<NotificationPlacement>,
}

impl Notification {
    pub fn new(message: impl Into<String>, on_dismiss: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            message: message.into(),
            on_dismiss: Arc::new(on_dismiss),
            style: NotificationStyle::default(),
            placement: None,
        }
    }

    /// Optional positioning within the offered bounds; does not create a Portal.
    pub fn placement(mut self, placement: NotificationPlacement) -> Self {
        self.placement = Some(placement);
        self
    }

    pub fn style(mut self, style: NotificationStyle) -> Self {
        self.style = style;
        self
    }
}

impl Component for Notification {
    fn build(&self, cx: &mut BuildCx) -> View {
        let pressed = cx.use_state(|| None::<u64>);
        let offset = cx.use_state(|| 0usize);
        let card = View::new(
            Card {
                message: self.message.clone(),
                on_dismiss: self.on_dismiss.clone(),
                foreground: self
                    .style
                    .foreground
                    .unwrap_or(cx.theme().button.normal.foreground),
                background: self
                    .style
                    .background
                    .unwrap_or(cx.theme().button.normal.background),
                style: self.style.clone(),
                pressed,
                offset,
                scroll_limit: Cell::new(0),
            },
            vec![],
            None,
        );
        match self.placement {
            Some(placement) => View::new(Position(placement), vec![card], None),
            None => card,
        }
    }
}

struct Card {
    message: String,
    on_dismiss: Arc<dyn Fn() + Send + Sync>,
    foreground: Color,
    background: Color,
    style: NotificationStyle,
    pressed: Signal<Option<u64>>,
    offset: Signal<usize>,
    scroll_limit: Cell<usize>,
}

impl Card {
    fn lines(&self, width: f32, metrics: &TextMetrics) -> Vec<String> {
        let cells = ((width - self.style.horizontal_padding * 2.0).max(0.0) / metrics.cell_width)
            .floor()
            .max(1.0) as usize;
        let mut lines = Vec::new();
        for source in self.message.split('\n') {
            let mut line = String::new();
            let mut used = 0;
            for ch in source.chars() {
                let width = ch.width().unwrap_or(0);
                if used + width > cells && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push(ch);
                used += width;
            }
            lines.push(line);
        }
        lines
    }

    fn close_rect(&self, rect: Rect) -> Rect {
        Rect::from_min_size(
            Point::new(
                (rect.max.x - self.style.close_width).max(rect.min.x),
                rect.min.y,
            ),
            Size::new(
                self.style.close_width.min(rect.size().width),
                self.style.header_height.min(rect.size().height),
            ),
        )
    }
}

impl AnyView for Card {
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let width = constraints.max.width.min(self.style.max_width);
        let height = self.style.header_height
            + self.style.bottom_padding
            + self.lines(width, metrics).len() as f32 * metrics.line_height;
        constraints.constrain(Size::new(width, height))
    }

    fn descendant_clip(&self, rect: Rect) -> Option<crate::scene::clip::RoundedClip> {
        crate::scene::clip::RoundedClip::new(
            rect,
            crate::BorderRadius::default(),
            crate::ClipBehavior::HardEdge,
        )
        .ok()
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let mut items = vec![Primitive::Quad {
            rect,
            color: self.background,
            corner_radius: self.style.corner_radius,
        }];
        let close = self.close_rect(rect);
        items.push(Primitive::Text {
            text: Arc::from("×"),
            origin: Point::new(close.min.x + 10.0, close.min.y + 6.0),
            color: self.foreground,
        });
        let lines = self.lines(rect.size().width, metrics);
        let visible = ((rect.size().height - self.style.header_height - self.style.bottom_padding)
            .max(0.0)
            / metrics.line_height)
            .floor() as usize;
        self.scroll_limit.set(lines.len().saturating_sub(visible));
        let start = (*self.offset.read()).min(lines.len().saturating_sub(visible));
        for (i, line) in lines.iter().skip(start).take(visible).enumerate() {
            items.push(Primitive::Text {
                text: Arc::from(line.as_str()),
                origin: Point::new(
                    rect.min.x + self.style.horizontal_padding,
                    rect.min.y + self.style.header_height + i as f32 * metrics.line_height,
                ),
                color: self.foreground,
            });
        }
        items
    }

    fn hit_test(&self, point: Point, rect: Rect) -> bool {
        rect.contains(point)
    }
    fn pointer_cursor(&self) -> Option<CursorShape> {
        Some(CursorShape::Default)
    }
    fn permits_pointer_capture(&self) -> bool {
        true
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        if ctx.is_capture_phase() {
            return EventHandled::Ignored;
        }
        let UiEvent::Pointer(pointer) = event else {
            return EventHandled::Ignored;
        };
        match pointer.phase {
            PointerPhase::Down
                if pointer.button == PointerButton::Left
                    && self.close_rect(rect).contains(pointer.position) =>
            {
                self.pressed.set(Some(pointer.pointer_id));
                ctx.capture_pointer(pointer.pointer_id);
            }
            PointerPhase::Up
                if pointer.button == PointerButton::Left
                    && *self.pressed.read() == Some(pointer.pointer_id) =>
            {
                self.pressed.set(None);
                ctx.release_pointer(pointer.pointer_id);
                if self.close_rect(rect).contains(pointer.position) {
                    (self.on_dismiss)();
                }
            }
            PointerPhase::Cancel => {
                self.pressed.set(None);
            }
            PointerPhase::WheelLine { dy, .. } | PointerPhase::WheelPixel { dy, .. } => {
                let current = *self.offset.read();
                let next = (current as isize + dy.signum() as isize)
                    .clamp(0, self.scroll_limit.get() as isize) as usize;
                self.offset.set(next);
                ctx.invalidate_paint();
            }
            _ => {}
        }
        // The card, unlike empty composition space, prevents input leaking into
        // the terminal behind it. It never requests keyboard focus.
        ctx.stop_propagation();
        EventHandled::Handled
    }
}
