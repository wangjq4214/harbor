//! Dynamic tab navigation bar supporting top, bottom, left, and right window placements with scroll support.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use harbor_types::{SessionId, TabBarPosition};

use crate::input::event::{PointerButton, PointerPhase, UiEvent};
use crate::input::event_ctx::{EventCtx, EventHandled};
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key, View};

/// Default height of top/bottom horizontal tab bar.
pub const DEFAULT_TAB_BAR_HEIGHT: f32 = 36.0;
/// Default width of left/right vertical sidebar tab bar.
pub const DEFAULT_TAB_BAR_SIDEBAR_WIDTH: f32 = 180.0;
/// Default width of a single horizontal tab item.
pub const DEFAULT_TAB_ITEM_WIDTH: f32 = 140.0;
/// Default height of a single vertical sidebar tab item.
pub const DEFAULT_SIDEBAR_ITEM_HEIGHT: f32 = 36.0;
/// Plus button dimension.
pub const ADD_BUTTON_SIZE: f32 = 28.0;
const MIN_TAB_ITEM_WIDTH_WITH_CLOSE: f32 = 48.0;

/// Callback invoked when a tab is selected.
pub type OnSelect = Arc<dyn Fn(usize) + Send + Sync>;
/// Callback invoked when a tab's close button is clicked.
pub type OnClose = Arc<dyn Fn(usize) + Send + Sync>;
/// Callback invoked when the new tab button is clicked.
pub type OnNew = Arc<dyn Fn() + Send + Sync>;

/// Metadata describing a single tab in the `TabBar`.
#[derive(Clone, Debug, PartialEq)]
pub struct TabItem {
    pub id: SessionId,
    pub title: String,
    pub active: bool,
}

impl TabItem {
    /// Creates a new tab item.
    pub fn new(id: SessionId, title: impl Into<String>, active: bool) -> Self {
        Self {
            id,
            title: title.into(),
            active,
        }
    }
}

/// Navigation bar widget displaying a list of tabs and action buttons.
#[derive(Clone)]
pub struct TabBar {
    pub position: TabBarPosition,
    pub tabs: Vec<TabItem>,
    pub active_index: usize,
    on_select: Option<OnSelect>,
    on_close: Option<OnClose>,
    on_new: Option<OnNew>,
    scroll_offset: Arc<AtomicU32>,
}

impl TabBar {
    /// Creates a new `TabBar` with the specified position and tab items.
    pub fn new(position: TabBarPosition, tabs: Vec<TabItem>, active_index: usize) -> Self {
        let active_index = if tabs.is_empty() {
            0
        } else {
            active_index.min(tabs.len().saturating_sub(1))
        };
        Self {
            position,
            tabs,
            active_index,
            on_select: None,
            on_close: None,
            on_new: None,
            scroll_offset: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        }
    }

    /// Sets the tab selection callback.
    pub fn on_select(mut self, callback: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(callback));
        self
    }

    /// Sets the tab close callback.
    pub fn on_close(mut self, callback: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Sets the new session callback.
    pub fn on_new(mut self, callback: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_new = Some(Arc::new(callback));
        self
    }

    /// Sets a shared persistent scroll offset state across rebuilds.
    pub fn with_scroll_offset(mut self, offset: Arc<AtomicU32>) -> Self {
        self.scroll_offset = offset;
        self
    }

    fn horizontal_item_width(rect: Rect) -> f32 {
        DEFAULT_TAB_ITEM_WIDTH.min((rect.size().width - 8.0).max(0.0))
    }

    fn content_extent(&self, rect: Rect) -> f32 {
        if self.position.is_horizontal() {
            self.tabs.len() as f32 * (Self::horizontal_item_width(rect) + 4.0)
                + ADD_BUTTON_SIZE
                + 8.0
        } else {
            (self.tabs.len() as f32 + 1.0) * (DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0) + 8.0
        }
    }

    fn clamped_scroll(&self, rect: Rect) -> f32 {
        let view_len = if self.position.is_horizontal() {
            rect.size().width
        } else {
            rect.size().height
        };
        let max_scroll = (self.content_extent(rect) - view_len).max(0.0);
        let stored = f32::from_bits(self.scroll_offset.load(Ordering::Relaxed));
        let scroll = if stored.is_finite() {
            stored.clamp(0.0, max_scroll)
        } else {
            0.0
        };
        if scroll.to_bits() != stored.to_bits() {
            self.scroll_offset
                .store(scroll.to_bits(), Ordering::Relaxed);
        }
        scroll
    }
}

fn intersect_rect(rect: Rect, bounds: Rect) -> Option<Rect> {
    let min = Point::new(rect.min.x.max(bounds.min.x), rect.min.y.max(bounds.min.y));
    let max = Point::new(rect.max.x.min(bounds.max.x), rect.max.y.min(bounds.max.y));
    (max.x > min.x && max.y > min.y)
        .then(|| Rect::from_min_size(min, Size::new(max.x - min.x, max.y - min.y)))
}

fn push_clipped_quad(
    prims: &mut Vec<Primitive>,
    rect: Rect,
    bounds: Rect,
    color: Color,
    corner_radius: f32,
) {
    let (rect, color, corner_radius) = intersect_rect(rect, bounds).map_or_else(
        || {
            (
                Rect::from_min_size(bounds.min, Size::ZERO),
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
                0.0,
            )
        },
        |rect| (rect, color, corner_radius),
    );
    prims.push(Primitive::Quad {
        rect,
        color,
        corner_radius,
    });
}

fn label_that_fits(text: &str, max_width: f32, metrics: &TextMetrics) -> Arc<str> {
    let max_chars = (max_width.max(0.0) / metrics.cell_width.max(1.0)).floor() as usize;
    Arc::from(text.chars().take(max_chars).collect::<String>())
}

fn text_line_fits(origin: Point, width: f32, bounds: Rect, metrics: &TextMetrics) -> bool {
    origin.x >= bounds.min.x
        && origin.y >= bounds.min.y
        && origin.x + width <= bounds.max.x
        && origin.y + metrics.line_height <= bounds.max.y
}

fn push_contained_text(
    prims: &mut Vec<Primitive>,
    text: Arc<str>,
    origin: Point,
    bounds: Rect,
    metrics: &TextMetrics,
    color: Color,
) {
    let width = text.chars().count() as f32 * metrics.cell_width;
    let (text, origin) = if !text.is_empty() && text_line_fits(origin, width, bounds, metrics) {
        (text, origin)
    } else {
        (Arc::from(""), bounds.min)
    };
    prims.push(Primitive::Text {
        text,
        origin,
        color,
    });
}

impl Component for TabBar {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), Vec::new(), None)
    }
}

impl AnyView for TabBar {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<TabBar>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        let desired = if self.position.is_horizontal() {
            Size::new(constraints.max.width, DEFAULT_TAB_BAR_HEIGHT)
        } else {
            Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, constraints.max.height)
        };
        Size::new(
            desired.width.min(constraints.max.width).max(0.0),
            desired.height.min(constraints.max.height).max(0.0),
        )
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        _child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = self.intrinsic_size(constraints, _metrics);
        (size, Vec::new())
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let mut prims = Vec::new();
        // Background strip
        let bg_color = Color {
            r: 0.12,
            g: 0.13,
            b: 0.15,
            a: 1.0,
        };
        prims.push(Primitive::Quad {
            rect,
            color: bg_color,
            corner_radius: 0.0,
        });

        // Bar boundary border separating it from the adjacent pane / content area
        let bar_border_color = Color {
            r: 0.25,
            g: 0.28,
            b: 0.32,
            a: 1.0,
        };
        let bar_border_rect = match self.position {
            TabBarPosition::Top => Rect::from_min_size(
                Point::new(rect.min.x, rect.max.y - 1.0),
                Size::new(rect.size().width, 1.0),
            ),
            TabBarPosition::Bottom => Rect::from_min_size(
                Point::new(rect.min.x, rect.min.y),
                Size::new(rect.size().width, 1.0),
            ),
            TabBarPosition::Left => Rect::from_min_size(
                Point::new(rect.max.x - 1.0, rect.min.y),
                Size::new(1.0, rect.size().height),
            ),
            TabBarPosition::Right => Rect::from_min_size(
                Point::new(rect.min.x, rect.min.y),
                Size::new(1.0, rect.size().height),
            ),
        };
        prims.push(Primitive::Quad {
            rect: bar_border_rect,
            color: bar_border_color,
            corner_radius: 0.0,
        });

        let scroll = self.clamped_scroll(rect);
        let effective_active = if self.tabs.is_empty() {
            0
        } else {
            self.active_index.min(self.tabs.len() - 1)
        };

        // Render each tab with stable primitive slot indices
        if self.position.is_horizontal() {
            let tab_item_width = Self::horizontal_item_width(rect);
            let mut x_offset = rect.min.x + 4.0 - scroll;
            let tab_y = rect.min.y + 4.0;
            let tab_h = (rect.size().height - 8.0).max(0.0);

            for (i, tab) in self.tabs.iter().enumerate() {
                let is_active = i == effective_active || tab.active;
                let has_close = tab_item_width >= MIN_TAB_ITEM_WIDTH_WITH_CLOSE;
                let tab_rect = Rect::from_min_size(
                    Point::new(x_offset, tab_y),
                    Size::new(tab_item_width, tab_h),
                );

                let (tab_border_color, tab_bg) = if is_active {
                    (
                        Color {
                            r: 0.35,
                            g: 0.45,
                            b: 0.60,
                            a: 1.0,
                        },
                        Color {
                            r: 0.20,
                            g: 0.23,
                            b: 0.27,
                            a: 1.0,
                        },
                    )
                } else {
                    (
                        Color {
                            r: 0.20,
                            g: 0.22,
                            b: 0.25,
                            a: 1.0,
                        },
                        Color {
                            r: 0.14,
                            g: 0.15,
                            b: 0.17,
                            a: 1.0,
                        },
                    )
                };

                // 1. Tab border
                push_clipped_quad(&mut prims, tab_rect, rect, tab_border_color, 2.0);

                // 2. Tab background (inset by 1px)
                let tab_inner = Rect::from_min_size(
                    Point::new(tab_rect.min.x + 1.0, tab_rect.min.y + 1.0),
                    Size::new(
                        (tab_rect.size().width - 2.0).max(0.0),
                        (tab_rect.size().height - 2.0).max(0.0),
                    ),
                );
                push_clipped_quad(&mut prims, tab_inner, rect, tab_bg, 1.0);

                // 3. Indicator
                let indicator_color = if is_active {
                    Color {
                        r: 0.2,
                        g: 0.6,
                        b: 1.0,
                        a: 1.0,
                    }
                } else {
                    Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }
                };
                let indicator = Rect::from_min_size(
                    Point::new(x_offset, rect.max.y - 2.0),
                    Size::new(tab_item_width, 2.0),
                );
                push_clipped_quad(&mut prims, indicator, rect, indicator_color, 0.0);

                // 4. Tab title text
                let text_color = if is_active {
                    Color::WHITE
                } else {
                    Color {
                        r: 0.7,
                        g: 0.7,
                        b: 0.7,
                        a: 1.0,
                    }
                };
                let title_origin = Point::new(x_offset + 8.0, tab_y + 6.0);
                let trailing_space = if has_close { 36.0 } else { 16.0 };
                let title_width = (tab_rect.max.x.min(rect.max.x) - title_origin.x)
                    .min(tab_item_width - trailing_space);
                let title = label_that_fits(&tab.title, title_width, metrics);
                push_contained_text(&mut prims, title, title_origin, rect, metrics, text_color);

                // 5. Close button "x"
                let close_origin = Point::new(x_offset + tab_item_width - 18.0, tab_y + 6.0);
                push_contained_text(
                    &mut prims,
                    Arc::from(if has_close { "x" } else { "" }),
                    close_origin,
                    rect,
                    metrics,
                    Color {
                        r: 0.6,
                        g: 0.6,
                        b: 0.6,
                        a: 1.0,
                    },
                );

                x_offset += tab_item_width + 4.0;
            }

            // "+" Button: border + inner background + text
            let add_rect = Rect::from_min_size(
                Point::new(x_offset, tab_y + (tab_h - ADD_BUTTON_SIZE) / 2.0),
                Size::new(ADD_BUTTON_SIZE, ADD_BUTTON_SIZE),
            );
            push_clipped_quad(
                &mut prims,
                add_rect,
                rect,
                Color {
                    r: 0.24,
                    g: 0.26,
                    b: 0.30,
                    a: 1.0,
                },
                2.0,
            );
            let add_inner = Rect::from_min_size(
                Point::new(add_rect.min.x + 1.0, add_rect.min.y + 1.0),
                Size::new(
                    (add_rect.size().width - 2.0).max(0.0),
                    (add_rect.size().height - 2.0).max(0.0),
                ),
            );
            push_clipped_quad(
                &mut prims,
                add_inner,
                rect,
                Color {
                    r: 0.18,
                    g: 0.20,
                    b: 0.22,
                    a: 1.0,
                },
                1.0,
            );
            let add_origin = Point::new(add_rect.min.x + 8.0, add_rect.min.y + 4.0);
            push_contained_text(
                &mut prims,
                Arc::from("+"),
                add_origin,
                rect,
                metrics,
                Color::WHITE,
            );

            let total_w = x_offset + ADD_BUTTON_SIZE + 4.0 + scroll - rect.min.x;
            let max_scroll = (total_w - rect.size().width).max(0.0);
            if max_scroll > 0.0 {
                let thumb_ratio = (rect.size().width / total_w).clamp(0.1, 1.0);
                let thumb_w = rect.size().width * thumb_ratio;
                let scroll_ratio = scroll / max_scroll;
                let thumb_x = rect.min.x + scroll_ratio * (rect.size().width - thumb_w);
                let thumb_rect = Rect::from_min_size(
                    Point::new(thumb_x, rect.max.y - 3.0),
                    Size::new(thumb_w, 2.0),
                );
                push_clipped_quad(
                    &mut prims,
                    thumb_rect,
                    rect,
                    Color {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 0.8,
                    },
                    1.0,
                );
            }
        } else {
            // Vertical sidebar with stable [Quad(border), Quad(inner), Quad(indicator), Text, Text] slots
            let mut y_offset = rect.min.y + 4.0 - scroll;
            let tab_x = rect.min.x + 4.0;
            let tab_w = (rect.size().width - 8.0).max(0.0);

            for (i, tab) in self.tabs.iter().enumerate() {
                let is_active = i == effective_active || tab.active;
                let has_close = tab_w >= MIN_TAB_ITEM_WIDTH_WITH_CLOSE;
                let tab_rect = Rect::from_min_size(
                    Point::new(tab_x, y_offset),
                    Size::new(tab_w, DEFAULT_SIDEBAR_ITEM_HEIGHT),
                );

                let (tab_border_color, tab_bg) = if is_active {
                    (
                        Color {
                            r: 0.35,
                            g: 0.45,
                            b: 0.60,
                            a: 1.0,
                        },
                        Color {
                            r: 0.20,
                            g: 0.23,
                            b: 0.27,
                            a: 1.0,
                        },
                    )
                } else {
                    (
                        Color {
                            r: 0.20,
                            g: 0.22,
                            b: 0.25,
                            a: 1.0,
                        },
                        Color {
                            r: 0.14,
                            g: 0.15,
                            b: 0.17,
                            a: 1.0,
                        },
                    )
                };

                // 1. Tab border
                push_clipped_quad(&mut prims, tab_rect, rect, tab_border_color, 2.0);

                // 2. Tab background (inset by 1px)
                let tab_inner = Rect::from_min_size(
                    Point::new(tab_rect.min.x + 1.0, tab_rect.min.y + 1.0),
                    Size::new(
                        (tab_rect.size().width - 2.0).max(0.0),
                        (tab_rect.size().height - 2.0).max(0.0),
                    ),
                );
                push_clipped_quad(&mut prims, tab_inner, rect, tab_bg, 1.0);

                // 3. Indicator
                let indicator_color = if is_active {
                    Color {
                        r: 0.2,
                        g: 0.6,
                        b: 1.0,
                        a: 1.0,
                    }
                } else {
                    Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }
                };
                let indicator = Rect::from_min_size(
                    Point::new(rect.min.x, y_offset),
                    Size::new(3.0, DEFAULT_SIDEBAR_ITEM_HEIGHT),
                );
                push_clipped_quad(&mut prims, indicator, rect, indicator_color, 0.0);

                // 4. Tab title text
                let text_color = if is_active {
                    Color::WHITE
                } else {
                    Color {
                        r: 0.7,
                        g: 0.7,
                        b: 0.7,
                        a: 1.0,
                    }
                };
                let title_origin = Point::new(tab_x + 10.0, y_offset + 8.0);
                let trailing_space = if has_close { 38.0 } else { 18.0 };
                let title_width =
                    (tab_rect.max.x.min(rect.max.x) - title_origin.x).min(tab_w - trailing_space);
                let title = label_that_fits(&tab.title, title_width, metrics);
                push_contained_text(&mut prims, title, title_origin, rect, metrics, text_color);

                // 5. Close button "x"
                let close_origin = Point::new(tab_x + tab_w - 20.0, y_offset + 8.0);
                push_contained_text(
                    &mut prims,
                    Arc::from(if has_close { "x" } else { "" }),
                    close_origin,
                    rect,
                    metrics,
                    Color {
                        r: 0.6,
                        g: 0.6,
                        b: 0.6,
                        a: 1.0,
                    },
                );

                y_offset += DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0;
            }

            // "+" Button at bottom of list: border + inner bg + text
            let add_rect = Rect::from_min_size(
                Point::new(tab_x, y_offset),
                Size::new(tab_w, DEFAULT_SIDEBAR_ITEM_HEIGHT),
            );
            push_clipped_quad(
                &mut prims,
                add_rect,
                rect,
                Color {
                    r: 0.24,
                    g: 0.26,
                    b: 0.30,
                    a: 1.0,
                },
                2.0,
            );
            let add_inner = Rect::from_min_size(
                Point::new(add_rect.min.x + 1.0, add_rect.min.y + 1.0),
                Size::new(
                    (add_rect.size().width - 2.0).max(0.0),
                    (add_rect.size().height - 2.0).max(0.0),
                ),
            );
            push_clipped_quad(
                &mut prims,
                add_inner,
                rect,
                Color {
                    r: 0.18,
                    g: 0.20,
                    b: 0.22,
                    a: 1.0,
                },
                1.0,
            );
            let add_origin = Point::new(tab_x + tab_w / 2.0 - 4.0, y_offset + 8.0);
            push_contained_text(
                &mut prims,
                Arc::from("+"),
                add_origin,
                rect,
                metrics,
                Color::WHITE,
            );

            let total_h = y_offset + DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0 + scroll - rect.min.y;
            let max_scroll = (total_h - rect.size().height).max(0.0);
            if max_scroll > 0.0 {
                let thumb_ratio = (rect.size().height / total_h).clamp(0.1, 1.0);
                let thumb_h = rect.size().height * thumb_ratio;
                let scroll_ratio = scroll / max_scroll;
                let thumb_y = rect.min.y + scroll_ratio * (rect.size().height - thumb_h);
                let thumb_rect = Rect::from_min_size(
                    Point::new(rect.max.x - 4.0, thumb_y),
                    Size::new(3.0, thumb_h),
                );
                push_clipped_quad(
                    &mut prims,
                    thumb_rect,
                    rect,
                    Color {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 0.8,
                    },
                    1.5,
                );
            }
        }

        prims
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        if let UiEvent::Pointer(pe) = event {
            let scroll = self.clamped_scroll(rect);
            match pe.phase {
                PointerPhase::WheelLine { dy, .. } => {
                    let total = self.content_extent(rect);
                    let view_len = if self.position.is_horizontal() {
                        rect.size().width
                    } else {
                        rect.size().height
                    };
                    let max_scroll = (total - view_len).max(0.0);
                    let next = (scroll - dy * 24.0).clamp(0.0, max_scroll);
                    if next.to_bits() != scroll.to_bits() {
                        self.scroll_offset.store(next.to_bits(), Ordering::Relaxed);
                        ctx.invalidate_paint();
                    }
                    return EventHandled::Handled;
                }
                PointerPhase::WheelPixel { dy, .. } => {
                    let total = self.content_extent(rect);
                    let view_len = if self.position.is_horizontal() {
                        rect.size().width
                    } else {
                        rect.size().height
                    };
                    let max_scroll = (total - view_len).max(0.0);
                    let next = (scroll - dy).clamp(0.0, max_scroll);
                    if next.to_bits() != scroll.to_bits() {
                        self.scroll_offset.store(next.to_bits(), Ordering::Relaxed);
                        ctx.invalidate_paint();
                    }
                    return EventHandled::Handled;
                }
                PointerPhase::Down if pe.button == PointerButton::Left => {
                    let point = Point::new(pe.position.x - rect.min.x, pe.position.y - rect.min.y);
                    if self.position.is_horizontal() {
                        let tab_y = 4.0;
                        let tab_h = rect.size().height - 8.0;
                        if point.y >= tab_y && point.y <= tab_y + tab_h {
                            let mut x = 4.0 - scroll;
                            for (i, _) in self.tabs.iter().enumerate() {
                                let tab_end = x + Self::horizontal_item_width(rect);
                                if point.x >= x && point.x <= tab_end {
                                    if tab_end - x >= MIN_TAB_ITEM_WIDTH_WITH_CLOSE
                                        && point.x >= tab_end - 24.0
                                    {
                                        if let Some(on_close) = &self.on_close {
                                            on_close(i);
                                        }
                                    } else if let Some(on_select) = &self.on_select {
                                        on_select(i);
                                    }
                                    ctx.invalidate_paint();
                                    return EventHandled::Handled;
                                }
                                x += Self::horizontal_item_width(rect) + 4.0;
                            }

                            if point.x >= x && point.x <= x + ADD_BUTTON_SIZE {
                                if let Some(on_new) = &self.on_new {
                                    on_new();
                                }
                                ctx.invalidate_paint();
                                return EventHandled::Handled;
                            }
                        }
                    } else {
                        let tab_x = 4.0;
                        let tab_w = rect.size().width - 8.0;
                        let mut y = 4.0 - scroll;
                        for (i, _) in self.tabs.iter().enumerate() {
                            let tab_end = y + DEFAULT_SIDEBAR_ITEM_HEIGHT;
                            if point.y >= y
                                && point.y <= tab_end
                                && point.x >= tab_x
                                && point.x <= tab_x + tab_w
                            {
                                if tab_w >= MIN_TAB_ITEM_WIDTH_WITH_CLOSE
                                    && point.x >= tab_x + tab_w - 24.0
                                {
                                    if let Some(on_close) = &self.on_close {
                                        on_close(i);
                                    }
                                } else if let Some(on_select) = &self.on_select {
                                    on_select(i);
                                }
                                ctx.invalidate_paint();
                                return EventHandled::Handled;
                            }
                            y += DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0;
                        }

                        if point.y >= y
                            && point.y <= y + DEFAULT_SIDEBAR_ITEM_HEIGHT
                            && point.x >= tab_x
                            && point.x <= tab_x + tab_w
                        {
                            if let Some(on_new) = &self.on_new {
                                on_new();
                            }
                            ctx.invalidate_paint();
                            return EventHandled::Handled;
                        }
                    }
                }
                _ => {}
            }
        }

        EventHandled::Ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::PointerEvent;

    fn assert_primitive_inside(primitive: &Primitive, bounds: Rect, metrics: &TextMetrics) {
        match primitive {
            Primitive::Quad { rect, .. } | Primitive::Border { rect, .. } => {
                assert!(
                    rect.min.x >= bounds.min.x,
                    "primitive starts left of bar: {rect:?}"
                );
                assert!(
                    rect.min.y >= bounds.min.y,
                    "primitive starts above bar: {rect:?}"
                );
                assert!(
                    rect.max.x <= bounds.max.x,
                    "primitive ends right of bar: {rect:?}"
                );
                assert!(
                    rect.max.y <= bounds.max.y,
                    "primitive ends below bar: {rect:?}"
                );
            }
            Primitive::Text { text, origin, .. } => {
                let width = text.chars().count() as f32 * metrics.cell_width;
                assert!(
                    text_line_fits(*origin, width, bounds, metrics),
                    "text line is outside bar: {origin:?}, text={text:?}"
                );
            }
            Primitive::External { rect, .. } => {
                assert!(rect.min.x >= bounds.min.x && rect.max.x <= bounds.max.x);
                assert!(rect.min.y >= bounds.min.y && rect.max.y <= bounds.max.y);
            }
        }
    }

    #[test]
    fn tab_bar_intrinsic_size_respects_tight_bounds_when_window_is_smaller_than_bar() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let tabs = vec![TabItem::new(SessionId(1), "Terminal 1", true)];

        let left = TabBar::new(TabBarPosition::Left, tabs.clone(), 0);
        assert_eq!(
            left.intrinsic_size(BoxConstraints::tight(Size::new(96.0, 500.0)), &metrics),
            Size::new(96.0, 500.0)
        );

        let top = TabBar::new(TabBarPosition::Top, tabs, 0);
        assert_eq!(
            top.intrinsic_size(BoxConstraints::tight(Size::new(500.0, 24.0)), &metrics),
            Size::new(500.0, 24.0)
        );
    }

    #[test]
    fn tab_bar_clamps_persistent_scroll_and_paints_only_inside_resized_bar() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let scroll = Arc::new(AtomicU32::new(10_000.0f32.to_bits()));
        let tabs = (1..=9)
            .map(|id| TabItem::new(SessionId(id), format!("Terminal {id}"), id == 9))
            .collect();
        let bar =
            TabBar::new(TabBarPosition::Left, tabs, 8).with_scroll_offset(Arc::clone(&scroll));
        let resized = Rect::from_min_size(Point::new(7.0, 11.0), Size::new(96.0, 132.0));

        let primitives = bar.paint_primitives(resized, &metrics);

        let total = (9.0 + 1.0) * (DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0) + 8.0;
        let expected_max = total - resized.size().height;
        assert_eq!(f32::from_bits(scroll.load(Ordering::Relaxed)), expected_max);
        for primitive in &primitives {
            assert_primitive_inside(primitive, resized, &metrics);
        }

        scroll.store(0.0f32.to_bits(), Ordering::Relaxed);
        let primitives_at_start = bar.paint_primitives(resized, &metrics);
        assert_eq!(primitives_at_start.len(), primitives.len());
        for primitive in &primitives_at_start {
            assert_primitive_inside(primitive, resized, &metrics);
        }
    }

    #[test]
    fn horizontal_tab_bar_truncates_titles_and_stays_inside_narrow_bounds() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let scroll = Arc::new(AtomicU32::new(10_000.0f32.to_bits()));
        let tabs = (1..=6)
            .map(|id| {
                TabItem::new(
                    SessionId(id),
                    format!("Terminal with a deliberately long title {id}"),
                    id == 6,
                )
            })
            .collect();
        let bar = TabBar::new(TabBarPosition::Top, tabs, 5).with_scroll_offset(Arc::clone(&scroll));
        let resized = Rect::from_min_size(Point::new(13.0, 17.0), Size::new(132.0, 36.0));

        let primitives = bar.paint_primitives(resized, &metrics);

        let expected_item_width = TabBar::horizontal_item_width(resized);
        let total = 6.0 * (expected_item_width + 4.0) + ADD_BUTTON_SIZE + 8.0;
        assert_eq!(
            f32::from_bits(scroll.load(Ordering::Relaxed)),
            total - resized.size().width
        );
        for primitive in &primitives {
            assert_primitive_inside(primitive, resized, &metrics);
        }
    }

    #[test]
    fn horizontal_tab_bar_intrinsic_size() {
        let tabs = vec![
            TabItem::new(SessionId(1), "Tab 1", true),
            TabItem::new(SessionId(2), "Tab 2", false),
        ];
        let bar = TabBar::new(TabBarPosition::Top, tabs, 0);
        let constraints = BoxConstraints::tight(Size::new(800.0, 600.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let size = bar.intrinsic_size(constraints, &metrics);
        assert_eq!(size, Size::new(800.0, DEFAULT_TAB_BAR_HEIGHT));
    }

    #[test]
    fn vertical_tab_bar_intrinsic_size() {
        let tabs = vec![
            TabItem::new(SessionId(1), "Tab 1", true),
            TabItem::new(SessionId(2), "Tab 2", false),
        ];
        let bar = TabBar::new(TabBarPosition::Left, tabs, 0);
        let constraints = BoxConstraints::tight(Size::new(800.0, 600.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        let size = bar.intrinsic_size(constraints, &metrics);
        assert_eq!(size, Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, 600.0));
    }

    #[test]
    fn tab_bar_intrinsic_size_matches_all_positions() {
        let tabs = vec![
            TabItem::new(SessionId(1), "Tab 1", true),
            TabItem::new(SessionId(2), "Tab 2", false),
        ];
        let constraints = BoxConstraints::tight(Size::new(800.0, 600.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;

        for (position, expected) in [
            (
                TabBarPosition::Top,
                Size::new(800.0, DEFAULT_TAB_BAR_HEIGHT),
            ),
            (
                TabBarPosition::Bottom,
                Size::new(800.0, DEFAULT_TAB_BAR_HEIGHT),
            ),
            (
                TabBarPosition::Left,
                Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, 600.0),
            ),
            (
                TabBarPosition::Right,
                Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, 600.0),
            ),
        ] {
            let bar = TabBar::new(position, tabs.clone(), 0);
            assert_eq!(bar.intrinsic_size(constraints, &metrics), expected);
        }
    }

    #[test]
    fn tab_bar_handles_horizontal_close_select_and_new_events() {
        let selects = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let closes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let new_sessions = std::sync::Arc::new(std::sync::Mutex::new(0usize));

        let bar = TabBar::new(
            TabBarPosition::Top,
            vec![
                TabItem::new(SessionId(1), "Tab 1", true),
                TabItem::new(SessionId(2), "Tab 2", false),
            ],
            1,
        )
        .on_select({
            let selects = std::sync::Arc::clone(&selects);
            move |index| selects.lock().expect("select log").push(index)
        })
        .on_close({
            let closes = std::sync::Arc::clone(&closes);
            move |index| closes.lock().expect("close log").push(index)
        })
        .on_new({
            let new_sessions = std::sync::Arc::clone(&new_sessions);
            move || *new_sessions.lock().expect("new session count") += 1
        });

        let rect = Rect::from_min_size(Point::ZERO, Size::new(400.0, DEFAULT_TAB_BAR_HEIGHT));
        let mut ctx = EventCtx::new();

        assert_eq!(
            bar.handle_event(
                &UiEvent::Pointer(PointerEvent {
                    position: Point::new(20.0, 10.0),
                    phase: PointerPhase::Down,
                    button: PointerButton::Left,
                    pointer_id: 1,
                    modifiers: crate::input::event::Modifiers::default(),
                }),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert_eq!(&*selects.lock().expect("select log"), &[0]);

        assert_eq!(
            bar.handle_event(
                &UiEvent::Pointer(PointerEvent {
                    position: Point::new(276.0, 10.0),
                    phase: PointerPhase::Down,
                    button: PointerButton::Left,
                    pointer_id: 2,
                    modifiers: crate::input::event::Modifiers::default(),
                }),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert_eq!(&*closes.lock().expect("close log"), &[1]);

        assert_eq!(
            bar.handle_event(
                &UiEvent::Pointer(PointerEvent {
                    position: Point::new(302.0, 10.0),
                    phase: PointerPhase::Down,
                    button: PointerButton::Left,
                    pointer_id: 3,
                    modifiers: crate::input::event::Modifiers::default(),
                }),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert_eq!(*new_sessions.lock().expect("new session count"), 1);
    }

    #[test]
    fn narrow_horizontal_tab_treats_its_visible_body_as_select_not_close() {
        let selects = Arc::new(std::sync::Mutex::new(Vec::new()));
        let closes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let bar = TabBar::new(
            TabBarPosition::Top,
            vec![TabItem::new(SessionId(1), "Tab 1", true)],
            0,
        )
        .on_select({
            let selects = Arc::clone(&selects);
            move |index| selects.lock().expect("select log").push(index)
        })
        .on_close({
            let closes = Arc::clone(&closes);
            move |index| closes.lock().expect("close log").push(index)
        });
        let rect = Rect::from_min_size(Point::ZERO, Size::new(40.0, DEFAULT_TAB_BAR_HEIGHT));

        let handled = bar.handle_event(
            &UiEvent::Pointer(PointerEvent::new(
                Point::new(30.0, 10.0),
                PointerPhase::Down,
                PointerButton::Left,
                1,
            )),
            &mut EventCtx::new(),
            rect,
        );

        assert_eq!(handled, EventHandled::Handled);
        assert_eq!(&*selects.lock().expect("select log"), &[0]);
        assert!(closes.lock().expect("close log").is_empty());
    }

    #[test]
    fn tab_bar_handles_vertical_close_select_and_new_events() {
        let selects = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let closes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let new_sessions = std::sync::Arc::new(std::sync::Mutex::new(0usize));

        let bar = TabBar::new(
            TabBarPosition::Left,
            vec![TabItem::new(SessionId(1), "Tab 1", false)],
            0,
        )
        .on_select({
            let selects = std::sync::Arc::clone(&selects);
            move |index| selects.lock().expect("select log").push(index)
        })
        .on_close({
            let closes = std::sync::Arc::clone(&closes);
            move |index| closes.lock().expect("close log").push(index)
        })
        .on_new({
            let new_sessions = std::sync::Arc::clone(&new_sessions);
            move || *new_sessions.lock().expect("new session count") += 1
        });

        let rect =
            Rect::from_min_size(Point::ZERO, Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, 200.0));
        let mut ctx = EventCtx::new();

        // Click select on tab
        assert_eq!(
            bar.handle_event(
                &UiEvent::Pointer(PointerEvent {
                    position: Point::new(12.0, 12.0),
                    phase: PointerPhase::Down,
                    button: PointerButton::Left,
                    pointer_id: 4,
                    modifiers: crate::input::event::Modifiers::default(),
                }),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert_eq!(&*selects.lock().expect("select log"), &[0]);

        // Click close on tab (right edge)
        assert_eq!(
            bar.handle_event(
                &UiEvent::Pointer(PointerEvent {
                    position: Point::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH - 10.0, 12.0),
                    phase: PointerPhase::Down,
                    button: PointerButton::Left,
                    pointer_id: 5,
                    modifiers: crate::input::event::Modifiers::default(),
                }),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert_eq!(&*closes.lock().expect("close log"), &[0]);

        // Click "+" button
        assert_eq!(
            bar.handle_event(
                &UiEvent::Pointer(PointerEvent {
                    position: Point::new(12.0, 48.0),
                    phase: PointerPhase::Down,
                    button: PointerButton::Left,
                    pointer_id: 6,
                    modifiers: crate::input::event::Modifiers::default(),
                }),
                &mut ctx,
                rect,
            ),
            EventHandled::Handled
        );
        assert_eq!(*new_sessions.lock().expect("new session count"), 1);
    }

    #[test]
    fn tab_bar_multi_step_lifecycle_maintains_valid_scene_items_and_quad_mapping() {
        use crate::scene::SceneGraph;

        let mut scene = SceneGraph::new();
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let rect =
            Rect::from_min_size(Point::ZERO, Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, 600.0));

        // Step 1: 4 tabs
        let mut tabs = vec![
            TabItem::new(SessionId(1), "Terminal 1", false),
            TabItem::new(SessionId(2), "Terminal 2", false),
            TabItem::new(SessionId(3), "Terminal 3", false),
            TabItem::new(SessionId(4), "Terminal 4", true),
        ];
        let bar1 = TabBar::new(TabBarPosition::Left, tabs.clone(), 3);
        let prims1 = bar1.paint_primitives(rect, &metrics);
        let items1: Vec<crate::scene::SceneItem> = prims1
            .into_iter()
            .enumerate()
            .map(|(i, p)| crate::scene::SceneItem {
                id: i as u64 + 1,
                primitive: p,
                paint_order: i as u32,
            })
            .collect();
        let delta1 = scene.diff(items1);
        assert_eq!(delta1.added.len(), 2 + 4 * 5 + 3);

        // Step 2: Add 5th tab (Terminal 5 becomes active)
        tabs.push(TabItem::new(SessionId(5), "Terminal 5", true));
        tabs[3].active = false;
        let bar2 = TabBar::new(TabBarPosition::Left, tabs.clone(), 4);
        let prims2 = bar2.paint_primitives(rect, &metrics);
        let items2: Vec<crate::scene::SceneItem> = prims2
            .into_iter()
            .enumerate()
            .map(|(i, p)| crate::scene::SceneItem {
                id: i as u64 + 1,
                primitive: p,
                paint_order: i as u32,
            })
            .collect();
        let delta2 = scene.diff(items2);
        assert!(!delta2.added.is_empty() || !delta2.modified.is_empty());

        // Step 3: Switch active tab to Terminal 2
        tabs[4].active = false;
        tabs[1].active = true;
        let bar3 = TabBar::new(TabBarPosition::Left, tabs.clone(), 1);
        let prims3 = bar3.paint_primitives(rect, &metrics);
        let items3: Vec<crate::scene::SceneItem> = prims3
            .into_iter()
            .enumerate()
            .map(|(i, p)| crate::scene::SceneItem {
                id: i as u64 + 1,
                primitive: p,
                paint_order: i as u32,
            })
            .collect();
        let delta3 = scene.diff(items3);
        assert!(!delta3.modified.is_empty());

        // Step 4: Remove Terminal 3
        tabs.remove(2);
        let bar4 = TabBar::new(TabBarPosition::Left, tabs.clone(), 1);
        let prims4 = bar4.paint_primitives(rect, &metrics);
        let items4: Vec<crate::scene::SceneItem> = prims4
            .into_iter()
            .enumerate()
            .map(|(i, p)| crate::scene::SceneItem {
                id: i as u64 + 1,
                primitive: p,
                paint_order: i as u32,
            })
            .collect();
        let delta4 = scene.diff(items4);
        assert!(!delta4.removed.is_empty());
    }

    #[test]
    fn tab_bar_paint_primitives_includes_bar_and_tab_borders() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let rect = Rect::from_min_size(Point::ZERO, Size::new(800.0, DEFAULT_TAB_BAR_HEIGHT));
        let tabs = vec![
            TabItem::new(SessionId(1), "Tab 1", true),
            TabItem::new(SessionId(2), "Tab 2", false),
        ];
        let bar = TabBar::new(TabBarPosition::Top, tabs, 0);
        let prims = bar.paint_primitives(rect, &metrics);

        // Expect: 1 (bg) + 1 (bar border) + 2 tabs * 5 + 3 (add button) = 15 primitives
        assert_eq!(prims.len(), 2 + 2 * 5 + 3);

        // First quad: bar background
        match &prims[0] {
            Primitive::Quad { rect: r, color, .. } => {
                assert_eq!(*r, rect);
                assert_eq!(color.r, 0.12);
            }
            _ => panic!("expected bar background quad"),
        }

        // Second quad: bar bottom border separating bar from content
        match &prims[1] {
            Primitive::Quad { rect: r, .. } => {
                assert_eq!(r.min.y, rect.max.y - 1.0);
                assert_eq!(r.size().height, 1.0);
                assert_eq!(r.size().width, rect.size().width);
            }
            _ => panic!("expected bar border quad"),
        }

        // Tab 0 border (active)
        match &prims[2] {
            Primitive::Quad { color, .. } => {
                assert_eq!(color.r, 0.35); // active tab border
            }
            _ => panic!("expected tab 0 border quad"),
        }

        // Tab 0 inner background (active)
        match &prims[3] {
            Primitive::Quad { color, .. } => {
                assert_eq!(color.r, 0.20); // active tab background
            }
            _ => panic!("expected tab 0 inner bg quad"),
        }

        // Tab 1 border (inactive)
        match &prims[7] {
            Primitive::Quad { color, .. } => {
                assert_eq!(color.r, 0.20); // inactive tab border
            }
            _ => panic!("expected tab 1 border quad"),
        }

        // Tab 1 inner background (inactive)
        match &prims[8] {
            Primitive::Quad { color, .. } => {
                assert_eq!(color.r, 0.14); // inactive tab background
            }
            _ => panic!("expected tab 1 inner bg quad"),
        }
    }

    #[test]
    fn tab_bar_close_maintains_clamped_active_index_and_valid_colors() {
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let rect =
            Rect::from_min_size(Point::ZERO, Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, 400.0));

        // Initial: 2 tabs, tab 1 is active
        let mut tabs = vec![
            TabItem::new(SessionId(1), "Tab 1", false),
            TabItem::new(SessionId(2), "Tab 2", true),
        ];
        let bar = TabBar::new(TabBarPosition::Left, tabs.clone(), 1);
        assert_eq!(bar.active_index, 1);

        // Now close tab 2 (the active one)
        tabs.remove(1);
        // When active_index was 1, constructing TabBar clamps it to 0
        tabs[0].active = true;
        let bar_after_close = TabBar::new(TabBarPosition::Left, tabs.clone(), 1);
        assert_eq!(bar_after_close.active_index, 0);

        let prims = bar_after_close.paint_primitives(rect, &metrics);
        // 1 bar bg + 1 bar border + 1 tab * 5 + 3 add button = 10 primitives
        assert_eq!(prims.len(), 2 + 5 + 3);

        // Remaining tab should have active border and active bg
        match &prims[2] {
            Primitive::Quad { color, .. } => {
                assert_eq!(color.r, 0.35); // active border
            }
            _ => panic!("expected active tab border"),
        }
        match &prims[3] {
            Primitive::Quad { color, .. } => {
                assert_eq!(color.r, 0.20); // active bg
            }
            _ => panic!("expected active tab bg"),
        }
    }

    #[test]
    fn tab_bar_hit_test_bounds_check_x_and_y_for_add_and_close() {
        let new_sessions = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let bar = TabBar::new(
            TabBarPosition::Left,
            vec![TabItem::new(SessionId(1), "Tab 1", false)],
            0,
        )
        .on_new({
            let new_sessions = std::sync::Arc::clone(&new_sessions);
            move || *new_sessions.lock().expect("new session count") += 1
        });

        let rect =
            Rect::from_min_size(Point::ZERO, Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, 400.0));
        let mut ctx = EventCtx::new();

        // Click at y=48 (add button height), but x = 250 (outside sidebar width): should be ignored
        let handled = bar.handle_event(
            &UiEvent::Pointer(PointerEvent {
                position: Point::new(250.0, 48.0),
                phase: PointerPhase::Down,
                button: PointerButton::Left,
                pointer_id: 10,
                modifiers: crate::input::event::Modifiers::default(),
            }),
            &mut ctx,
            rect,
        );
        assert_eq!(handled, EventHandled::Ignored);
        assert_eq!(*new_sessions.lock().expect("new session count"), 0);

        // Click at y=48, x = 12 (inside sidebar width): should be handled
        let handled = bar.handle_event(
            &UiEvent::Pointer(PointerEvent {
                position: Point::new(12.0, 48.0),
                phase: PointerPhase::Down,
                button: PointerButton::Left,
                pointer_id: 11,
                modifiers: crate::input::event::Modifiers::default(),
            }),
            &mut ctx,
            rect,
        );
        assert_eq!(handled, EventHandled::Handled);
        assert_eq!(*new_sessions.lock().expect("new session count"), 1);

        // 3. Horizontal bar: click with y out of bounds (y = 50 in a 36px bar): should be ignored
        let closes = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let h_bar = TabBar::new(
            TabBarPosition::Top,
            vec![TabItem::new(SessionId(1), "Tab 1", false)],
            0,
        )
        .on_close({
            let closes = std::sync::Arc::clone(&closes);
            move |_| *closes.lock().expect("close count") += 1
        });
        let h_rect = Rect::from_min_size(Point::ZERO, Size::new(800.0, DEFAULT_TAB_BAR_HEIGHT));
        let handled = h_bar.handle_event(
            &UiEvent::Pointer(PointerEvent {
                position: Point::new(130.0, 50.0), // close button x, but y > 36
                phase: PointerPhase::Down,
                button: PointerButton::Left,
                pointer_id: 12,
                modifiers: crate::input::event::Modifiers::default(),
            }),
            &mut ctx,
            h_rect,
        );
        assert_eq!(handled, EventHandled::Ignored);
        assert_eq!(*closes.lock().expect("close count"), 0);

        // 4. Horizontal bar: click close button within y bounds: should be handled
        let handled = h_bar.handle_event(
            &UiEvent::Pointer(PointerEvent {
                position: Point::new(130.0, 18.0),
                phase: PointerPhase::Down,
                button: PointerButton::Left,
                pointer_id: 13,
                modifiers: crate::input::event::Modifiers::default(),
            }),
            &mut ctx,
            h_rect,
        );
        assert_eq!(handled, EventHandled::Handled);
        assert_eq!(*closes.lock().expect("close count"), 1);
    }
}
