use crate::icon::LucideIcon;
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::{Color, Primitive};
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};

/// A non-interactive, square vector icon in logical pixels.
///
/// Defaults to 20dp and the inherited theme's normal icon-button foreground.
/// Wrap it in an `InteractiveRegion` for activation and state-specific colors.
#[derive(Clone, Debug)]
pub struct Icon {
    icon: LucideIcon,
    size: f32,
    color: Option<Color>,
}

impl Icon {
    pub fn new(icon: LucideIcon) -> Self {
        Self {
            icon,
            size: 20.0,
            color: None,
        }
    }

    /// Sets a finite, nonnegative square size in dp. Zero hides the artwork.
    pub fn size(mut self, size: f32) -> Result<Self, crate::LayoutConfigError> {
        crate::layout::configuration::validate_scalar("Icon.size", size)?;
        self.size = size;
        Ok(self)
    }

    /// Sets a tint including opacity; finite components outside `[0,1]` are legal.
    pub fn foreground(mut self, color: Color) -> Result<Self, crate::theme::ColorConfigError> {
        crate::theme::validate_color("Icon.foreground", color)?;
        self.color = Some(color);
        Ok(self)
    }
}

impl Component for Icon {
    fn build(&self, cx: &mut BuildCx) -> View {
        let mut icon = self.clone();
        icon.color = Some(
            self.color
                .unwrap_or(cx.theme().icon_button.normal.foreground),
        );
        View::new(icon, vec![], None)
    }
}

impl AnyView for Icon {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        constraints.constrain(Size::new(self.size, self.size))
    }

    fn paint_primitives(&self, rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        let allocation = rect.size();
        let size = self.size.min(allocation.width).min(allocation.height);
        let color = self.color.unwrap_or(Color::WHITE);
        if !allocation.is_valid() || !rect.min.is_valid() || size <= 0.0 || color.a <= 0.0 {
            return Vec::new();
        }
        let origin = Point::new(
            rect.min.x + (allocation.width - size) * 0.5,
            rect.min.y + (allocation.height - size) * 0.5,
        );
        vec![Primitive::Icon {
            icon: self.icon,
            rect: Rect::from_min_size(origin, Size::new(size, size)),
            color,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::DEFAULT_TEXT_METRICS;

    #[test]
    fn icon_is_square_centered_and_independent_of_text_metrics() {
        let icon = Icon::new(LucideIcon::Terminal)
            .size(18.0)
            .expect("valid icon size")
            .foreground(Color::RED)
            .expect("valid color");
        let mut metrics = DEFAULT_TEXT_METRICS;
        metrics.cell_width = 80.0;
        metrics.line_height = 120.0;
        assert_eq!(
            icon.intrinsic_size(BoxConstraints::loose(Size::new(100.0, 100.0)), &metrics),
            Size::new(18.0, 18.0)
        );
        let allocation = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(30.0, 24.0));
        assert_eq!(
            icon.paint_primitives(allocation, &metrics),
            vec![Primitive::Icon {
                icon: LucideIcon::Terminal,
                rect: Rect::from_min_size(Point::new(16.0, 23.0), Size::new(18.0, 18.0)),
                color: Color::RED,
            }]
        );
    }

    #[test]
    fn icon_shrinks_without_distortion_and_bounds_invalid_inputs() {
        let rect = Rect::from_min_size(Point::ZERO, Size::new(12.0, 8.0));
        for value in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(Icon::new(LucideIcon::Plus).size(value).is_err());
        }
        let icon = Icon::new(LucideIcon::Plus);
        let primitives = icon.paint_primitives(rect, &DEFAULT_TEXT_METRICS);
        assert!(matches!(&primitives[0], Primitive::Icon { rect, .. }
            if rect.size() == Size::new(8.0, 8.0) && rect.min == Point::new(2.0, 0.0)));
        for icon in [
            icon.clone().size(0.0).expect("valid icon size"),
            icon.foreground(Color::TRANSPARENT).expect("valid color"),
        ] {
            assert!(
                icon.paint_primitives(rect, &DEFAULT_TEXT_METRICS)
                    .is_empty()
            );
        }
        for size in [Size::ZERO, Size::new(0.0, 30.0)] {
            assert!(
                Icon::new(LucideIcon::X)
                    .paint_primitives(
                        Rect::from_min_size(Point::ZERO, size),
                        &DEFAULT_TEXT_METRICS
                    )
                    .is_empty()
            );
        }
    }
}
