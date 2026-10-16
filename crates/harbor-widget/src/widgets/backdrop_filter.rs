use crate::{
    BorderRadius, DecorationError,
    layout::{BoxConstraints, Point, Rect, Size},
    scene::primitive::Primitive,
    text::TextMetrics,
    view::{AnyView, BuildCx, Component, View},
};

/// Locally blurs pixels painted before this wrapper; its child stays sharp.
///
/// Use [`crate::runtime::Runtime::encode_frame`] to render scenes with filters.
/// Sigma is in logical pixels, capped at 64 to bound sampling support. Zero
/// disables filtering. This wrapper adds no layout, fill or input surface;
/// compose it with DecoratedBox for tint, borders, shadows and child clipping.
#[derive(Clone)]
pub struct BackdropFilter {
    sigma: f32,
    radius: BorderRadius,
    child: Option<View>,
}

impl BackdropFilter {
    pub fn new(sigma: f32) -> Result<Self, DecorationError> {
        if !sigma.is_finite() {
            return Err(DecorationError::NonFinite {
                field: "blur.sigma",
            });
        }
        if sigma < 0.0 {
            return Err(DecorationError::Negative {
                field: "blur.sigma",
            });
        }
        Ok(Self {
            sigma: sigma.min(64.0),
            radius: BorderRadius::default(),
            child: None,
        })
    }

    /// The filter's shape, independent of child decoration and clipping.
    pub fn border_radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for BackdropFilter {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl crate::WithChildren for BackdropFilter {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("BackdropFilter", &mut self.child)?;
        Ok(self)
    }
}

impl AnyView for BackdropFilter {
    fn layout_children(
        &self,
        constraints: BoxConstraints,
        children: &[Size],
        _: &TextMetrics,
    ) -> (Size, Vec<Point>) {
        (
            constraints.constrain(children.first().copied().unwrap_or(Size::ZERO)),
            vec![Point::ZERO; children.len()],
        )
    }

    fn hit_test(&self, _: Point, _: Rect) -> bool {
        false
    }

    fn paint_primitives(&self, rect: Rect, _: &TextMetrics) -> Vec<Primitive> {
        if self.sigma == 0.0 || rect.size().width <= 0.0 || rect.size().height <= 0.0 {
            return Vec::new();
        }
        let Ok(radius) = self.radius.normalize(rect.size()) else {
            return Vec::new();
        };
        vec![Primitive::BackdropBlur {
            rect,
            sigma: self.sigma,
            corner_radii: radius.as_array(),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::DEFAULT_TEXT_METRICS;

    #[test]
    fn validates_sigma_and_preserves_child_geometry_without_a_hit_surface() {
        for sigma in [f32::NAN, f32::INFINITY, -1.0] {
            assert!(BackdropFilter::new(sigma).is_err());
        }
        let filter = BackdropFilter::new(f32::MAX).unwrap();
        let size = Size::new(100.0, 40.0);
        let rect = Rect::from_min_size(Point::ZERO, size);
        assert_eq!(
            filter.layout_children(BoxConstraints::loose(size), &[size], &DEFAULT_TEXT_METRICS),
            (size, vec![Point::ZERO])
        );
        assert!(!filter.hit_test(Point::new(10.0, 10.0), rect));
        assert!(matches!(
            filter.paint_primitives(rect, &DEFAULT_TEXT_METRICS)[0],
            Primitive::BackdropBlur { sigma: 64.0, .. }
        ));
        assert!(
            BackdropFilter::new(0.0)
                .unwrap()
                .paint_primitives(rect, &DEFAULT_TEXT_METRICS)
                .is_empty()
        );
    }
}
