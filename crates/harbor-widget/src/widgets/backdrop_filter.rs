use crate::{
    BorderRadius, DecorationError,
    layout::{BoxConstraints, Point, Rect, Size},
    scene::primitive::Primitive,
    text::TextMetrics,
    view::{AnyView, BuildCx, Component, View},
};

/// Bounded background displacement near a rounded rectangle's edge.
///
/// Values are logical pixels. Width is capped at 64 and displacement at 32;
/// either zero disables refraction. The effective width is also limited to half
/// the shape's shortest side, so its center never moves. This is a visual lens
/// approximation, not a physical index-of-refraction model.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeRefraction {
    width: f32,
    displacement: f32,
}

impl EdgeRefraction {
    pub fn new(width: f32, displacement: f32) -> Result<Self, DecorationError> {
        for (field, value) in [
            ("refraction.width", width),
            ("refraction.displacement", displacement),
        ] {
            if !value.is_finite() {
                return Err(DecorationError::NonFinite { field });
            }
            if value < 0.0 {
                return Err(DecorationError::Negative { field });
            }
        }
        Ok(Self {
            width: width.min(64.0),
            displacement: displacement.min(32.0),
        })
    }

    pub fn width(self) -> f32 {
        self.width
    }
    pub fn displacement(self) -> f32 {
        self.displacement
    }
    pub(crate) fn enabled(self) -> bool {
        self.width > 0.0 && self.displacement > 0.0
    }
}

/// Raw tone inputs on straight RGB, validated by [`BackdropMaterial::new`].
///
/// The polynomial uses offset + slope * channel + quadratic * channel².
/// Coefficients may be negative; saturation must be nonnegative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropTone {
    pub offset: f32,
    pub slope: f32,
    pub quadratic: f32,
    pub saturation: f32,
}

/// Raw geometric rim intensities, validated by [`BackdropMaterial::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropRim {
    pub highlight: f32,
    pub shade: f32,
}

/// Opt-in background material, independent of product controls or layout.
/// Frost mixes a wider Gaussian with the base blur: negative mix darkens,
/// positive mix lightens. All inputs must be finite.
/// Sigma is capped at 64dp, mix at `[-1,1]`, tone coefficients at `[-2,2]`,
/// saturation at `[0,2]`, and rim intensities at `[0,1]`.
///
/// Named inputs are checked at consumption, including values changed through
/// their public fields. Legal finite values above a cap are clamped, not errors.
///
/// ```
/// use harbor_widget::{BackdropMaterial, BackdropTone, BackdropRim};
/// let material = BackdropMaterial::new(
///     4.0, -0.1,
///     BackdropTone { offset: 0.0, slope: 1.0, quadratic: 0.0, saturation: 1.02 },
///     BackdropRim { highlight: 0.30, shade: 0.22 },
/// )?;
/// # Ok::<(), harbor_widget::DecorationError>(())
/// ```
///
/// Positional tone/rim configuration is no longer supported:
///
/// ```compile_fail
/// use harbor_widget::BackdropMaterial;
/// let _ = BackdropMaterial::new(4.0, -0.1, [0.0, 1.0, 0.0, 1.02], [0.30, 0.22]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropMaterial {
    frost: [f32; 2],
    tone: [f32; 4],
    rim: [f32; 2],
}

impl BackdropMaterial {
    pub fn new(
        fill_sigma: f32,
        fill_mix: f32,
        tone: BackdropTone,
        rim: BackdropRim,
    ) -> Result<Self, DecorationError> {
        for (field, value, nonnegative) in [
            ("material.fill_sigma", fill_sigma, true),
            ("material.fill_mix", fill_mix, false),
            ("material.tone.offset", tone.offset, false),
            ("material.tone.slope", tone.slope, false),
            ("material.tone.quadratic", tone.quadratic, false),
            ("material.tone.saturation", tone.saturation, true),
            ("material.rim.highlight", rim.highlight, true),
            ("material.rim.shade", rim.shade, true),
        ] {
            if !value.is_finite() {
                return Err(DecorationError::NonFinite { field });
            }
            if nonnegative && value < 0.0 {
                return Err(DecorationError::Negative { field });
            }
        }
        Ok(Self {
            frost: [fill_sigma.min(64.0), fill_mix.clamp(-1.0, 1.0)],
            tone: [
                tone.offset.clamp(-2.0, 2.0),
                tone.slope.clamp(-2.0, 2.0),
                tone.quadratic.clamp(-2.0, 2.0),
                tone.saturation.min(2.0),
            ],
            rim: [rim.highlight.min(1.0), rim.shade.min(1.0)],
        })
    }

    /// Effective fill sigma and mix, in renderer packing order.
    pub fn frost(self) -> [f32; 2] {
        self.frost
    }
    /// Effective offset, slope, quadratic and saturation, in renderer packing order.
    pub fn tone(self) -> [f32; 4] {
        self.tone
    }
    /// Effective highlight and shade, in renderer packing order.
    pub fn rim(self) -> [f32; 2] {
        self.rim
    }
}

/// Locally blurs pixels painted before this wrapper; its child stays sharp.
///
/// Use [`crate::runtime::Runtime::encode_frame`] to render scenes with filters.
/// Sigma is in logical pixels, capped at 64 to bound sampling support. Zero
/// disables blur, but optional edge refraction can still run. This wrapper adds
/// no layout, fill or input surface; compose it with DecoratedBox for styling.
#[derive(Clone)]
pub struct BackdropFilter {
    sigma: f32,
    radius: BorderRadius,
    refraction: EdgeRefraction,
    material: Option<BackdropMaterial>,
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
            refraction: EdgeRefraction::default(),
            material: None,
            child: None,
        })
    }

    /// Opt-in edge/corner refraction of the backdrop, never of the child.
    pub fn edge_refraction(mut self, refraction: EdgeRefraction) -> Self {
        self.refraction = refraction;
        self
    }

    /// Opt into nonlinear edge refraction, geometric rim and frost/tone.
    /// None retains the legacy blur/refraction path.
    pub fn material(mut self, material: Option<BackdropMaterial>) -> Self {
        self.material = material;
        self
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
    type Capability = crate::SingleChild;
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
        if (self.material.is_none() && self.sigma == 0.0 && !self.refraction.enabled())
            || rect.size().width <= 0.0
            || rect.size().height <= 0.0
        {
            return Vec::new();
        }
        let Ok(radius) = self.radius.normalize(rect.size()) else {
            return Vec::new();
        };
        if let Some(material) = self.material {
            return vec![Primitive::BackdropMaterial {
                rect,
                sigma: self.sigma,
                corner_radii: radius.as_array(),
                refraction: self.refraction,
                material,
            }];
        }
        if self.refraction.enabled() {
            return vec![Primitive::BackdropRefract {
                rect,
                sigma: self.sigma,
                corner_radii: radius.as_array(),
                refraction: self.refraction,
            }];
        }
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
    fn material_is_validated_bounded_and_opt_in_without_layout_or_hit_changes() {
        let bounded = BackdropMaterial::new(
            f32::MAX,
            -f32::MAX,
            BackdropTone {
                offset: f32::MAX,
                slope: -f32::MAX,
                quadratic: f32::MAX,
                saturation: f32::MAX,
            },
            BackdropRim {
                highlight: f32::MAX,
                shade: f32::MAX,
            },
        )
        .unwrap();
        assert_eq!(bounded.frost(), [64.0, -1.0]);
        assert_eq!(bounded.tone(), [2.0, -2.0, 2.0, 2.0]);
        assert_eq!(bounded.rim(), [1.0; 2]);
        let size = Size::new(40.0, 20.0);
        let rect = Rect::from_min_size(Point::ZERO, size);
        let material = BackdropFilter::new(0.0).unwrap().material(Some(bounded));
        assert!(matches!(
            material.paint_primitives(rect, &DEFAULT_TEXT_METRICS)[0],
            Primitive::BackdropMaterial { .. }
        ));
        assert_eq!(
            material.layout_children(BoxConstraints::loose(size), &[size], &DEFAULT_TEXT_METRICS),
            (size, vec![Point::ZERO])
        );
        assert!(!material.hit_test(Point::ZERO, rect));
        assert!(
            material
                .material(None)
                .paint_primitives(rect, &DEFAULT_TEXT_METRICS)
                .is_empty()
        );
    }

    #[test]
    fn refraction_validates_bounds_and_is_opt_in_without_layout_or_input_changes() {
        for invalid in [f32::NAN, f32::INFINITY, -1.0] {
            assert!(EdgeRefraction::new(invalid, 1.0).is_err());
            assert!(EdgeRefraction::new(1.0, invalid).is_err());
        }
        let bounded = EdgeRefraction::new(f32::MAX, f32::MAX).unwrap();
        assert_eq!(bounded.width(), 64.0);
        assert_eq!(bounded.displacement(), 32.0);
        let size = Size::new(40.0, 20.0);
        let rect = Rect::from_min_size(Point::ZERO, size);
        let refracted = BackdropFilter::new(0.0).unwrap().edge_refraction(bounded);
        assert!(matches!(
            refracted.paint_primitives(rect, &DEFAULT_TEXT_METRICS)[0],
            Primitive::BackdropRefract { sigma: 0.0, .. }
        ));
        assert_eq!(
            refracted.layout_children(BoxConstraints::loose(size), &[size], &DEFAULT_TEXT_METRICS),
            (size, vec![Point::ZERO])
        );
        assert!(!refracted.hit_test(Point::ZERO, rect));
        assert!(
            refracted
                .paint_primitives(
                    Rect::from_min_size(Point::ZERO, Size::ZERO),
                    &DEFAULT_TEXT_METRICS
                )
                .is_empty()
        );
        for (width, strength) in [(0.0, 5.0), (5.0, 0.0)] {
            let filter = BackdropFilter::new(2.0)
                .unwrap()
                .edge_refraction(EdgeRefraction::new(width, strength).unwrap());
            assert!(matches!(
                filter.paint_primitives(rect, &DEFAULT_TEXT_METRICS)[0],
                Primitive::BackdropBlur { sigma: 2.0, .. }
            ));
        }
    }

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
