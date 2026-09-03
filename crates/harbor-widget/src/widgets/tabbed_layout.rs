//! Dedicated two-child container that places a TabBar and content area without overlap.

use harbor_types::TabBarPosition;

use crate::layout::{BoxConstraints, Point, Size};
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

    fn layout_children(
        &self,
        constraints: BoxConstraints,
        _child_sizes: &[Size],
        _metrics: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.max;

        let positions = match self.position {
            TabBarPosition::Top => {
                let thickness = DEFAULT_TAB_BAR_HEIGHT;
                vec![Point::ZERO, Point::new(0.0, thickness)]
            }
            TabBarPosition::Bottom => {
                let thickness = DEFAULT_TAB_BAR_HEIGHT;
                let top = (size.height - thickness).max(0.0);
                vec![Point::new(0.0, top), Point::ZERO]
            }
            TabBarPosition::Left => {
                let thickness = DEFAULT_TAB_BAR_SIDEBAR_WIDTH;
                vec![Point::ZERO, Point::new(thickness, 0.0)]
            }
            TabBarPosition::Right => {
                let thickness = DEFAULT_TAB_BAR_SIDEBAR_WIDTH;
                let left = (size.width - thickness).max(0.0);
                vec![Point::new(left, 0.0), Point::ZERO]
            }
        };

        (size, positions)
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
}
