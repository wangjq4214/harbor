use std::cell::Cell;
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

use crate::Portal;
use crate::effects::CursorShape;
use crate::input::event::{PointerButton, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, ChildMeasurer, LayoutError, ParentLayout, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::signal::Signal;
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// A persistent, non-modal, top-right notification presented through a Portal.
/// Eligibility and dismissal state belong to the caller. No timeout or focus
/// request is introduced. Long content wraps and can be scrolled with the wheel.
#[derive(Clone)]
pub struct Notification {
    message: String,
    on_dismiss: Arc<dyn Fn() + Send + Sync>,
}

impl Notification {
    pub fn new(message: impl Into<String>, on_dismiss: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            message: message.into(),
            on_dismiss: Arc::new(on_dismiss),
        }
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
                foreground: cx.theme().button.normal.foreground,
                background: cx.theme().button.normal.background,
                pressed,
                offset,
                scroll_limit: Cell::new(0),
            },
            vec![],
            None,
        );
        Portal::new()
            .child(View::new(Position, vec![card], None))
            .build(cx)
    }
}

struct Position;
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
        let inset = 12.0_f32.min(size.width / 2.0).min(size.height / 2.0);
        let available = Size::new(
            (size.width - inset * 2.0).min(520.0),
            (size.height - inset * 2.0).max(0.0),
        );
        let card = children.measure(0, BoxConstraints::loose(available))?;
        Ok(ParentLayout {
            size,
            placements: vec![(
                0,
                Point::new((size.width - inset - card.width).max(0.0), inset),
            )],
            diagnostics: Vec::new(),
        })
    }
}

struct Card {
    message: String,
    on_dismiss: Arc<dyn Fn() + Send + Sync>,
    foreground: Color,
    background: Color,
    pressed: Signal<Option<u64>>,
    offset: Signal<usize>,
    scroll_limit: Cell<usize>,
}

impl Card {
    fn lines(&self, width: f32, metrics: &TextMetrics) -> Vec<String> {
        let cells = ((width - 24.0).max(0.0) / metrics.cell_width)
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

    fn close_rect(rect: Rect) -> Rect {
        Rect::from_min_size(
            Point::new((rect.max.x - 36.0).max(rect.min.x), rect.min.y),
            Size::new(
                36.0_f32.min(rect.size().width),
                32.0_f32.min(rect.size().height),
            ),
        )
    }
}

impl AnyView for Card {
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let width = constraints.max.width.min(520.0);
        let height = 44.0 + self.lines(width, metrics).len() as f32 * metrics.line_height;
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
            color: Color {
                a: 1.0,
                ..self.background
            },
            corner_radius: 6.0,
        }];
        let close = Self::close_rect(rect);
        items.push(Primitive::Text {
            text: Arc::from("×"),
            origin: Point::new(close.min.x + 10.0, close.min.y + 6.0),
            color: self.foreground,
        });
        let lines = self.lines(rect.size().width, metrics);
        let visible = ((rect.size().height - 44.0).max(0.0) / metrics.line_height).floor() as usize;
        self.scroll_limit.set(lines.len().saturating_sub(visible));
        let start = (*self.offset.read()).min(lines.len().saturating_sub(visible));
        for (i, line) in lines.iter().skip(start).take(visible).enumerate() {
            items.push(Primitive::Text {
                text: Arc::from(line.as_str()),
                origin: Point::new(
                    rect.min.x + 12.0,
                    rect.min.y + 32.0 + i as f32 * metrics.line_height,
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
                    && Self::close_rect(rect).contains(pointer.position) =>
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
                if Self::close_rect(rect).contains(pointer.position) {
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
        // The card, unlike the empty Portal surface, prevents input leaking into
        // the terminal behind it. It never requests keyboard focus.
        ctx.stop_propagation();
        EventHandled::Handled
    }
}
