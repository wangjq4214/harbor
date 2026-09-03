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
        if self.position.is_horizontal() {
            Size::new(constraints.max.width, DEFAULT_TAB_BAR_HEIGHT)
        } else {
            Size::new(DEFAULT_TAB_BAR_SIDEBAR_WIDTH, constraints.max.height)
        }
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

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
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

        let scroll = f32::from_bits(self.scroll_offset.load(Ordering::Relaxed));

        // Render each tab with stable primitive slot indices
        if self.position.is_horizontal() {
            let mut x_offset = rect.min.x + 4.0 - scroll;
            let tab_y = rect.min.y + 4.0;
            let tab_h = rect.size().height - 8.0;

            for (i, tab) in self.tabs.iter().enumerate() {
                let is_active = i == self.active_index || tab.active;
                let tab_rect = Rect::from_min_size(
                    Point::new(x_offset, tab_y),
                    Size::new(DEFAULT_TAB_ITEM_WIDTH, tab_h),
                );

                let tab_bg = if is_active {
                    Color {
                        r: 0.22,
                        g: 0.25,
                        b: 0.28,
                        a: 1.0,
                    }
                } else {
                    Color {
                        r: 0.16,
                        g: 0.17,
                        b: 0.19,
                        a: 1.0,
                    }
                };
                prims.push(Primitive::Quad {
                    rect: tab_rect,
                    color: tab_bg,
                    corner_radius: 2.0,
                });

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
                    Size::new(DEFAULT_TAB_ITEM_WIDTH, 2.0),
                );
                prims.push(Primitive::Quad {
                    rect: indicator,
                    color: indicator_color,
                    corner_radius: 0.0,
                });

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
                prims.push(Primitive::Text {
                    text: Arc::from(tab.title.as_str()),
                    origin: Point::new(x_offset + 8.0, tab_y + 6.0),
                    color: text_color,
                });
                prims.push(Primitive::Text {
                    text: Arc::from("x"),
                    origin: Point::new(x_offset + DEFAULT_TAB_ITEM_WIDTH - 18.0, tab_y + 6.0),
                    color: Color {
                        r: 0.6,
                        g: 0.6,
                        b: 0.6,
                        a: 1.0,
                    },
                });

                x_offset += DEFAULT_TAB_ITEM_WIDTH + 4.0;
            }

            // "+" Button (Quad + Text)
            let add_rect = Rect::from_min_size(
                Point::new(x_offset, tab_y + (tab_h - ADD_BUTTON_SIZE) / 2.0),
                Size::new(ADD_BUTTON_SIZE, ADD_BUTTON_SIZE),
            );
            prims.push(Primitive::Quad {
                rect: add_rect,
                color: Color {
                    r: 0.18,
                    g: 0.20,
                    b: 0.22,
                    a: 1.0,
                },
                corner_radius: 2.0,
            });
            prims.push(Primitive::Text {
                text: Arc::from("+"),
                origin: Point::new(add_rect.min.x + 8.0, add_rect.min.y + 4.0),
                color: Color::WHITE,
            });

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
                prims.push(Primitive::Quad {
                    rect: thumb_rect,
                    color: Color {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 0.8,
                    },
                    corner_radius: 1.0,
                });
            }
        } else {
            // Vertical sidebar with stable [Quad, Quad, Text, Text] slots
            let mut y_offset = rect.min.y + 4.0 - scroll;
            let tab_x = rect.min.x + 4.0;
            let tab_w = rect.size().width - 8.0;

            for (i, tab) in self.tabs.iter().enumerate() {
                let is_active = i == self.active_index || tab.active;
                let tab_rect = Rect::from_min_size(
                    Point::new(tab_x, y_offset),
                    Size::new(tab_w, DEFAULT_SIDEBAR_ITEM_HEIGHT),
                );

                let tab_bg = if is_active {
                    Color {
                        r: 0.22,
                        g: 0.25,
                        b: 0.28,
                        a: 1.0,
                    }
                } else {
                    Color {
                        r: 0.16,
                        g: 0.17,
                        b: 0.19,
                        a: 1.0,
                    }
                };
                prims.push(Primitive::Quad {
                    rect: tab_rect,
                    color: tab_bg,
                    corner_radius: 2.0,
                });

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
                prims.push(Primitive::Quad {
                    rect: indicator,
                    color: indicator_color,
                    corner_radius: 0.0,
                });

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
                prims.push(Primitive::Text {
                    text: Arc::from(tab.title.as_str()),
                    origin: Point::new(tab_x + 10.0, y_offset + 8.0),
                    color: text_color,
                });
                prims.push(Primitive::Text {
                    text: Arc::from("x"),
                    origin: Point::new(tab_x + tab_w - 20.0, y_offset + 8.0),
                    color: Color {
                        r: 0.6,
                        g: 0.6,
                        b: 0.6,
                        a: 1.0,
                    },
                });

                y_offset += DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0;
            }

            // "+" Button at bottom of list (Quad + Text)
            let add_rect = Rect::from_min_size(
                Point::new(tab_x, y_offset),
                Size::new(tab_w, DEFAULT_SIDEBAR_ITEM_HEIGHT),
            );
            prims.push(Primitive::Quad {
                rect: add_rect,
                color: Color {
                    r: 0.18,
                    g: 0.20,
                    b: 0.22,
                    a: 1.0,
                },
                corner_radius: 2.0,
            });
            prims.push(Primitive::Text {
                text: Arc::from("+"),
                origin: Point::new(tab_x + tab_w / 2.0 - 4.0, y_offset + 8.0),
                color: Color::WHITE,
            });

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
                prims.push(Primitive::Quad {
                    rect: thumb_rect,
                    color: Color {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 0.8,
                    },
                    corner_radius: 1.5,
                });
            }
        }

        prims
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        if let UiEvent::Pointer(pe) = event {
            let scroll = f32::from_bits(self.scroll_offset.load(Ordering::Relaxed));
            match pe.phase {
                PointerPhase::WheelLine { dy, .. } => {
                    let total = if self.position.is_horizontal() {
                        (self.tabs.len() as f32) * (DEFAULT_TAB_ITEM_WIDTH + 4.0)
                            + ADD_BUTTON_SIZE
                            + 8.0
                    } else {
                        (self.tabs.len() as f32 + 1.0) * (DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0) + 8.0
                    };
                    let view_len = if self.position.is_horizontal() {
                        rect.size().width
                    } else {
                        rect.size().height
                    };
                    let max_scroll = (total - view_len).max(0.0);
                    let next = (scroll - dy * 24.0).clamp(0.0, max_scroll);
                    self.scroll_offset.store(next.to_bits(), Ordering::Relaxed);
                    ctx.invalidate_paint();
                    return EventHandled::Handled;
                }
                PointerPhase::WheelPixel { dy, .. } => {
                    let total = if self.position.is_horizontal() {
                        (self.tabs.len() as f32) * (DEFAULT_TAB_ITEM_WIDTH + 4.0)
                            + ADD_BUTTON_SIZE
                            + 8.0
                    } else {
                        (self.tabs.len() as f32 + 1.0) * (DEFAULT_SIDEBAR_ITEM_HEIGHT + 4.0) + 8.0
                    };
                    let view_len = if self.position.is_horizontal() {
                        rect.size().width
                    } else {
                        rect.size().height
                    };
                    let max_scroll = (total - view_len).max(0.0);
                    let next = (scroll - dy).clamp(0.0, max_scroll);
                    self.scroll_offset.store(next.to_bits(), Ordering::Relaxed);
                    ctx.invalidate_paint();
                    return EventHandled::Handled;
                }
                PointerPhase::Down if pe.button == PointerButton::Left => {
                    let point = Point::new(pe.position.x - rect.min.x, pe.position.y - rect.min.y);
                    if self.position.is_horizontal() {
                        let mut x = 4.0 - scroll;
                        for (i, _) in self.tabs.iter().enumerate() {
                            let tab_end = x + DEFAULT_TAB_ITEM_WIDTH;
                            if point.x >= x && point.x <= tab_end {
                                if point.x >= tab_end - 24.0 {
                                    if let Some(on_close) = &self.on_close {
                                        on_close(i);
                                    }
                                } else if let Some(on_select) = &self.on_select {
                                    on_select(i);
                                }
                                ctx.invalidate_paint();
                                return EventHandled::Handled;
                            }
                            x += DEFAULT_TAB_ITEM_WIDTH + 4.0;
                        }

                        if point.x >= x && point.x <= x + ADD_BUTTON_SIZE {
                            if let Some(on_new) = &self.on_new {
                                on_new();
                            }
                            ctx.invalidate_paint();
                            return EventHandled::Handled;
                        }
                    } else {
                        let mut y = 4.0 - scroll;
                        for (i, _) in self.tabs.iter().enumerate() {
                            let tab_end = y + DEFAULT_SIDEBAR_ITEM_HEIGHT;
                            if point.y >= y && point.y <= tab_end {
                                if point.x >= rect.size().width - 28.0 {
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

                        if point.y >= y && point.y <= y + DEFAULT_SIDEBAR_ITEM_HEIGHT {
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
        assert_eq!(delta1.added.len(), 1 + 4 * 4 + 2);

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
        assert!(delta2.added.len() > 0 || delta2.modified.len() > 0);

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
        assert!(delta3.modified.len() > 0);

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
        assert!(delta4.removed.len() > 0);
    }
}
