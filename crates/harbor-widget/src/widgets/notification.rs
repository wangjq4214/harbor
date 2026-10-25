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
use super::text_label::LabelGeometry;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use crate::LucideIcon;
use crate::effects::CursorShape;
use crate::input::event::{PointerBoundaryKind, PointerButton, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{
    Alignment, BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size,
};
use crate::scene::primitive::{Color, Primitive};
use crate::signal::Signal;
use crate::text::TextMetrics;
use crate::theme::ControlStyle;
use crate::view::{AnyView, BuildCx, Component, View};
use crate::widgets::{icon::Icon, interactive_region::InteractionState};

/// Reusable persistent notification content. Placement and Portal escape belong to the caller.
/// Eligibility and dismissal state belong to the caller. No timeout or focus
/// request is introduced. Long detail wraps and either scrolls or uses a caller-selected line cap.
#[derive(Clone)]
pub struct Notification {
    message: String,
    title: Option<String>,
    detail_color: Option<Color>,
    max_lines: Option<usize>,
    close_alignment: Alignment,
    on_dismiss: Rc<dyn Fn()>,
    style: NotificationStyle,
    close_style: Option<ControlStyle>,
    placement: Option<NotificationPlacement>,
}

impl Notification {
    /// Creates persistent content with a UI-thread-local dismissal callback.
    pub fn new(message: impl Into<String>, on_dismiss: impl Fn() + 'static) -> Self {
        Self {
            message: message.into(),
            title: None,
            detail_color: None,
            max_lines: None,
            close_alignment: Alignment::Center,
            on_dismiss: Rc::new(on_dismiss),
            style: NotificationStyle::default(),
            close_style: None,
            placement: None,
        }
    }

    /// Optional single-line heading, ellipsized independently from the detail.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into().replace(['\r', '\n'], " "));
        self
    }

    pub fn detail_color(mut self, color: Color) -> Self {
        self.detail_color = Some(color);
        self
    }

    /// Cap visible detail lines (at least one); overflow ends in an ellipsis instead of scrolling.
    pub fn max_lines(mut self, lines: usize) -> Self {
        self.max_lines = Some(lines.max(1));
        self
    }

    /// Vertical alignment of the close control within the card.
    pub fn close_alignment(mut self, alignment: Alignment) -> Self {
        self.close_alignment = alignment;
        self
    }

    /// Optional positioning within the offered bounds; does not create a Portal.
    pub fn placement(mut self, placement: NotificationPlacement) -> Self {
        self.placement = Some(placement);
        self
    }

    /// Appearance of the vector close control. It remains non-focus-stealing.
    pub fn close_button_style(mut self, style: ControlStyle) -> Self {
        self.close_style = Some(style);
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
        let hovered = cx.use_state(|| false);
        let card = View::new(
            Card {
                message: self.message.clone(),
                title: self.title.clone(),
                detail_color: self.detail_color,
                max_lines: self.max_lines,
                close_alignment: self.close_alignment,
                geometry: RefCell::new(None),
                title_geometry: RefCell::new(None),
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
                hovered,
                close_style: self
                    .close_style
                    .clone()
                    .unwrap_or_else(|| cx.theme().icon_button.clone()),
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
    title: Option<String>,
    detail_color: Option<Color>,
    max_lines: Option<usize>,
    close_alignment: Alignment,
    geometry: RefCell<Option<LabelGeometry>>,
    title_geometry: RefCell<Option<LabelGeometry>>,
    on_dismiss: Rc<dyn Fn()>,
    foreground: Color,
    background: Color,
    style: NotificationStyle,
    pressed: Signal<Option<u64>>,
    hovered: Signal<bool>,
    close_style: ControlStyle,
    offset: Signal<usize>,
    scroll_limit: Cell<usize>,
}

impl Card {
    fn text_width(&self, width: f32) -> f32 {
        (width - self.style.horizontal_padding * 2.0 - self.style.close_width).max(0.0)
    }

    fn lines(&self, width: f32, metrics: &TextMetrics, limit: Option<usize>) -> Vec<String> {
        let geometry = self.geometry.borrow();
        let fallback;
        let geometry = match geometry.as_ref() {
            Some(geometry) => geometry,
            None => {
                fallback = LabelGeometry::measure(&self.message, &mut |_| metrics.cell_width);
                &fallback
            }
        };
        geometry.wrapped(&self.message, self.text_width(width), limit)
    }

    fn close_rect(&self, rect: Rect) -> Rect {
        let height = self
            .style
            .header_height
            .max(self.close_style.min_height)
            .min(rect.size().height);
        Rect::from_min_size(
            Point::new(
                (rect.max.x - self.style.close_width).max(rect.min.x),
                rect.min.y + self.close_alignment.position(height, rect.size().height),
            ),
            Size::new(self.style.close_width.min(rect.size().width), height),
        )
    }

    fn update_hover(&self, hovered: bool, ctx: &mut EventCtx) {
        if *self.hovered.read() != hovered {
            self.hovered.set(hovered);
            ctx.invalidate_paint();
        }
    }
}

impl AnyView for Card {
    fn prepare_text_geometry(&self, _metrics: &TextMetrics, advance: &mut dyn FnMut(char) -> f32) {
        *self.geometry.borrow_mut() = Some(LabelGeometry::measure(&self.message, advance));
        *self.title_geometry.borrow_mut() = self
            .title
            .as_ref()
            .map(|title| LabelGeometry::measure(title, advance));
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let width = constraints.max.width.min(self.style.max_width);
        let height = self.style.header_height
            + self.style.bottom_padding
            + self.lines(width, metrics, self.max_lines).len() as f32 * metrics.line_height;
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
        let colors = self.close_style.resolve_state(&InteractionState {
            hovered: *self.hovered.read(),
            pressed: self.pressed.read().is_some() && *self.hovered.read(),
            ..Default::default()
        });
        if colors.background.a > 0.0 {
            items.push(Primitive::Quad {
                rect: close,
                color: colors.background,
                corner_radius: self.close_style.radius,
            });
        }
        if self.close_style.border_width > 0.0 && colors.border.a > 0.0 {
            items.push(Primitive::Border {
                rect: close,
                width: self.close_style.border_width,
                color: colors.border,
                corner_radius: self.close_style.radius,
            });
        }
        items.extend(
            Icon::new(LucideIcon::X)
                .size(self.close_style.icon_size)
                .color(colors.foreground)
                .paint_primitives(close, metrics),
        );
        if let Some(title) = &self.title {
            let geometry = self.title_geometry.borrow();
            let fallback;
            let geometry = match geometry.as_ref() {
                Some(geometry) => geometry,
                None => {
                    fallback = LabelGeometry::measure(title, &mut |_| metrics.cell_width);
                    &fallback
                }
            };
            let title = geometry.ellipsized(title, self.text_width(rect.size().width));
            if !title.is_empty() {
                items.push(Primitive::Text {
                    text: Arc::from(title),
                    origin: Point::new(
                        rect.min.x + self.style.horizontal_padding,
                        rect.min.y
                            + ((self.style.header_height - metrics.line_height) / 2.0).max(0.0),
                    ),
                    color: self.foreground,
                });
            }
        }
        let visible = ((rect.size().height - self.style.header_height - self.style.bottom_padding)
            .max(0.0)
            / metrics.line_height)
            .floor() as usize;
        let lines = self.lines(
            rect.size().width,
            metrics,
            self.max_lines.map(|limit| limit.min(visible)),
        );
        self.scroll_limit.set(if self.max_lines.is_some() {
            0
        } else {
            lines.len().saturating_sub(visible)
        });
        let start = (*self.offset.read()).min(self.scroll_limit.get());
        for (i, line) in lines.iter().skip(start).take(visible).enumerate() {
            items.push(Primitive::Text {
                text: Arc::from(line.as_str()),
                origin: Point::new(
                    rect.min.x + self.style.horizontal_padding,
                    rect.min.y + self.style.header_height + i as f32 * metrics.line_height,
                ),
                color: self.detail_color.unwrap_or(self.foreground),
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

    fn accepts_pointer_boundaries(&self) -> bool {
        true
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        if ctx.is_capture_phase() {
            return EventHandled::Ignored;
        }
        if let UiEvent::PointerBoundary(boundary) = event {
            self.update_hover(
                boundary.kind == PointerBoundaryKind::Enter
                    && boundary
                        .position
                        .is_some_and(|p| self.close_rect(rect).contains(p)),
                ctx,
            );
            return EventHandled::Handled;
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
                self.update_hover(true, ctx);
                ctx.invalidate_paint();
                ctx.capture_pointer(pointer.pointer_id);
            }
            PointerPhase::Up
                if pointer.button == PointerButton::Left
                    && *self.pressed.read() == Some(pointer.pointer_id) =>
            {
                self.pressed.set(None);
                self.update_hover(self.close_rect(rect).contains(pointer.position), ctx);
                ctx.invalidate_paint();
                ctx.release_pointer(pointer.pointer_id);
                if self.close_rect(rect).contains(pointer.position) {
                    (self.on_dismiss)();
                }
            }
            PointerPhase::Move => {
                self.update_hover(self.close_rect(rect).contains(pointer.position), ctx);
            }
            PointerPhase::Cancel => {
                self.pressed.set(None);
                self.update_hover(false, ctx);
                ctx.invalidate_paint();
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
        EventHandled::Handled
    }
}
