//! Dedicated two-child container that places a TabBar and content area without overlap.

use harbor_types::TabBarPosition;

use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, Key, View};
use crate::widgets::tab_bar::{DEFAULT_TAB_BAR_HEIGHT, DEFAULT_TAB_BAR_SIDEBAR_WIDTH};

/// Layout container managing non-overlapping placement of a `TabBar` and content `View`.
#[derive(Clone)]
pub struct TabbedLayout {
    pub position: TabBarPosition,
    pub tab_bar: View,
    pub content: View,
}

impl TabbedLayout {
    /// Creates a new `TabbedLayout` with the given position, tab bar view, and content view.
    pub fn new(position: TabBarPosition, tab_bar: View, content: View) -> Self {
        Self {
            position,
            tab_bar,
            content,
        }
    }
}

impl Component for TabbedLayout {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(
            self.clone(),
            vec![self.tab_bar.clone(), self.content.clone()],
            None,
        )
    }
}

impl AnyView for TabbedLayout {
    fn key(&self) -> Option<&Key> {
        None
    }

    fn widget_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<TabbedLayout>()
    }

    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.max
    }

    fn children_constraints(
        &self,
        _child_count: usize,
        constraints: BoxConstraints,
        _metrics: &TextMetrics,
    ) -> Vec<BoxConstraints> {
        let size = constraints.constrain(constraints.max);
        match self.position {
            TabBarPosition::Top | TabBarPosition::Bottom => {
                let bar_h = DEFAULT_TAB_BAR_HEIGHT.min(size.height);
                let content_h = (size.height - bar_h).max(0.0);
                vec![
                    BoxConstraints::tight(Size::new(size.width, bar_h)),
                    BoxConstraints::tight(Size::new(size.width, content_h)),
                ]
            }
            TabBarPosition::Left | TabBarPosition::Right => {
                let bar_w = DEFAULT_TAB_BAR_SIDEBAR_WIDTH.min(size.width);
                let content_w = (size.width - bar_w).max(0.0);
                vec![
                    BoxConstraints::tight(Size::new(bar_w, size.height)),
                    BoxConstraints::tight(Size::new(content_w, size.height)),
                ]
            }
        }
    }

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        _child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(constraints.max);

        let positions = match self.position {
            TabBarPosition::Top => {
                let thickness = DEFAULT_TAB_BAR_HEIGHT.min(size.height);
                vec![Point::ZERO, Point::new(0.0, thickness)]
            }
            TabBarPosition::Bottom => {
                let thickness = DEFAULT_TAB_BAR_HEIGHT.min(size.height);
                let top = (size.height - thickness).max(0.0);
                vec![Point::new(0.0, top), Point::ZERO]
            }
            TabBarPosition::Left => {
                let thickness = DEFAULT_TAB_BAR_SIDEBAR_WIDTH.min(size.width);
                vec![Point::ZERO, Point::new(thickness, 0.0)]
            }
            TabBarPosition::Right => {
                let thickness = DEFAULT_TAB_BAR_SIDEBAR_WIDTH.min(size.width);
                let left = (size.width - thickness).max(0.0);
                vec![Point::new(left, 0.0), Point::ZERO]
            }
        };

        (size, positions)
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        let divider_color = Color {
            r: 0.25,
            g: 0.28,
            b: 0.32,
            a: 1.0,
        };
        let divider_rect = match self.position {
            TabBarPosition::Top => {
                let thickness = DEFAULT_TAB_BAR_HEIGHT.min(rect.size().height);
                Rect::from_min_size(
                    Point::new(rect.min.x, rect.min.y + thickness - 1.0),
                    Size::new(rect.size().width, 1.0),
                )
            }
            TabBarPosition::Bottom => {
                let thickness = DEFAULT_TAB_BAR_HEIGHT.min(rect.size().height);
                Rect::from_min_size(
                    Point::new(rect.min.x, rect.max.y - thickness),
                    Size::new(rect.size().width, 1.0),
                )
            }
            TabBarPosition::Left => {
                let thickness = DEFAULT_TAB_BAR_SIDEBAR_WIDTH.min(rect.size().width);
                Rect::from_min_size(
                    Point::new(rect.min.x + thickness - 1.0, rect.min.y),
                    Size::new(1.0, rect.size().height),
                )
            }
            TabBarPosition::Right => {
                let thickness = DEFAULT_TAB_BAR_SIDEBAR_WIDTH.min(rect.size().width);
                Rect::from_min_size(
                    Point::new(rect.max.x - thickness, rect.min.y),
                    Size::new(1.0, rect.size().height),
                )
            }
        };
        vec![Primitive::Quad {
            rect: divider_rect,
            color: divider_color,
            corner_radius: 0.0,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::sized_box::SizedBox;

    #[test]
    fn tabbed_layout_left_positions() {
        let tab_bar = SizedBox::new(Size::new(180.0, 600.0)).build(&mut BuildCx::stub());
        let content = SizedBox::new(Size::new(620.0, 600.0)).build(&mut BuildCx::stub());
        let layout = TabbedLayout::new(TabBarPosition::Left, tab_bar, content);

        let constraints = BoxConstraints::tight(Size::new(800.0, 600.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let (size, positions) = layout.layout_children(constraints, &[], &metrics);

        assert_eq!(size, Size::new(800.0, 600.0));
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], Point::ZERO);
        assert_eq!(positions[1], Point::new(180.0, 0.0));
    }

    #[test]
    fn tabbed_layout_top_positions() {
        let tab_bar = SizedBox::new(Size::new(800.0, 36.0)).build(&mut BuildCx::stub());
        let content = SizedBox::new(Size::new(800.0, 564.0)).build(&mut BuildCx::stub());
        let layout = TabbedLayout::new(TabBarPosition::Top, tab_bar, content);

        let constraints = BoxConstraints::tight(Size::new(800.0, 600.0));
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let (size, positions) = layout.layout_children(constraints, &[], &metrics);

        assert_eq!(size, Size::new(800.0, 600.0));
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], Point::ZERO);
        assert_eq!(positions[1], Point::new(0.0, 36.0));
    }

    #[test]
    fn tabbed_layout_paint_primitives_has_divider_line() {
        let tab_bar = SizedBox::new(Size::new(180.0, 600.0)).build(&mut BuildCx::stub());
        let content = SizedBox::new(Size::new(620.0, 600.0)).build(&mut BuildCx::stub());
        let layout_left = TabbedLayout::new(TabBarPosition::Left, tab_bar.clone(), content.clone());
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let rect = Rect::from_min_size(Point::ZERO, Size::new(800.0, 600.0));

        let prims = layout_left.paint_primitives(rect, &metrics);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            Primitive::Quad { rect: r, color, .. } => {
                assert_eq!(r.min.x, DEFAULT_TAB_BAR_SIDEBAR_WIDTH - 1.0);
                assert_eq!(r.size().width, 1.0);
                assert_eq!(r.size().height, 600.0);
                assert_eq!(color.a, 1.0);
            }
            _ => panic!("expected divider quad"),
        }

        let layout_top = TabbedLayout::new(TabBarPosition::Top, tab_bar, content);
        let prims_top = layout_top.paint_primitives(rect, &metrics);
        assert_eq!(prims_top.len(), 1);
        match &prims_top[0] {
            Primitive::Quad { rect: r, .. } => {
                assert_eq!(r.min.y, DEFAULT_TAB_BAR_HEIGHT - 1.0);
                assert_eq!(r.size().height, 1.0);
                assert_eq!(r.size().width, 800.0);
            }
            _ => panic!("expected top divider quad"),
        }
    }

    #[test]
    fn tabbed_layout_runtime_measures_and_allocates_fullscreen_children() {
        use crate::renderer::Viewport;
        use crate::runtime::Runtime;
        use crate::widgets::split_container::SplitContainer;
        use crate::widgets::tab_bar::{TabBar, TabItem};
        use harbor_types::{SessionId, SplitDirection};

        let mut runtime = Runtime::new();
        let fullscreen_viewport = Viewport::new(1568, 862, 1.0);
        runtime.set_viewport(fullscreen_viewport);

        let tabs = vec![
            TabItem::new(SessionId(1), "Terminal 1", true),
            TabItem::new(SessionId(2), "Terminal 2", false),
        ];
        let bar = TabBar::new(TabBarPosition::Left, tabs, 0).build(&mut BuildCx::stub());
        let split = SplitContainer::new(SplitDirection::Horizontal, 0.5)
            .sash_thickness(4.0)
            .min_pane_size(10.0)
            .first(SizedBox::new(Size::new(100.0, 100.0)))
            .second(SizedBox::new(Size::new(100.0, 100.0)))
            .build(&mut BuildCx::stub());

        let layout = TabbedLayout::new(TabBarPosition::Left, bar, split);
        runtime.set_root(layout);
        runtime.update(std::time::Instant::now());

        let arena = runtime.arena();
        let root_id = runtime.root_id().unwrap();
        let root_fiber = arena.get(root_id).unwrap();
        assert_eq!(
            root_fiber.layout_rect.unwrap().size(),
            Size::new(1568.0, 862.0)
        );

        let children = &root_fiber.children;
        assert_eq!(children.len(), 2);

        // Child 0: TabBar stretched to full window height
        let bar_fiber = arena.get(children[0]).unwrap();
        let bar_rect = bar_fiber.layout_rect.unwrap();
        assert_eq!(
            bar_rect,
            Rect::from_min_size(Point::ZERO, Size::new(180.0, 862.0))
        );

        // Child 1: Content (SplitContainer) fills remaining width (1568 - 180 = 1388) and full height
        let split_fiber = arena.get(children[1]).unwrap();
        let split_rect = split_fiber.layout_rect.unwrap();
        assert_eq!(
            split_rect,
            Rect::from_min_size(Point::new(180.0, 0.0), Size::new(1388.0, 862.0))
        );

        // Children of SplitContainer:
        let split_children = &split_fiber.children;
        assert_eq!(split_children.len(), 2);
        // Available width: 1388 - 4 = 1384; 50% = 692.
        let first_pane = arena.get(split_children[0]).unwrap();
        let first_rect = first_pane.layout_rect.unwrap();
        assert_eq!(
            first_rect,
            Rect::from_min_size(Point::new(180.0, 0.0), Size::new(692.0, 862.0))
        );

        let second_pane = arena.get(split_children[1]).unwrap();
        let second_rect = second_pane.layout_rect.unwrap();
        assert_eq!(
            second_rect,
            Rect::from_min_size(
                Point::new(180.0 + 692.0 + 4.0, 0.0),
                Size::new(692.0, 862.0)
            )
        );

        // Horizontal-only resize keeps the sidebar thickness and reallocates content width.
        runtime.set_viewport(Viewport::new(1000, 862, 1.0));
        runtime.update(std::time::Instant::now());
        let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
        let bar = runtime.arena().get(root.children[0]).unwrap();
        let content = runtime.arena().get(root.children[1]).unwrap();
        assert_eq!(bar.layout_rect.unwrap().size(), Size::new(180.0, 862.0));
        assert_eq!(
            content.layout_rect.unwrap(),
            Rect::from_min_size(Point::new(180.0, 0.0), Size::new(820.0, 862.0))
        );

        // Vertical-only resize updates both children without changing their x partition.
        runtime.set_viewport(Viewport::new(1000, 500, 1.0));
        runtime.update(std::time::Instant::now());
        let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
        let bar = runtime.arena().get(root.children[0]).unwrap();
        let content = runtime.arena().get(root.children[1]).unwrap();
        assert_eq!(bar.layout_rect.unwrap().size(), Size::new(180.0, 500.0));
        assert_eq!(
            content.layout_rect.unwrap(),
            Rect::from_min_size(Point::new(180.0, 0.0), Size::new(820.0, 500.0))
        );

        // A window narrower than the configured sidebar shrinks the bar instead of overflowing.
        runtime.set_viewport(Viewport::new(120, 500, 1.0));
        runtime.update(std::time::Instant::now());
        let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
        let bar = runtime.arena().get(root.children[0]).unwrap();
        let content = runtime.arena().get(root.children[1]).unwrap();
        assert_eq!(bar.layout_rect.unwrap().size(), Size::new(120.0, 500.0));
        assert_eq!(content.layout_rect.unwrap().size(), Size::new(0.0, 500.0));
    }
}
