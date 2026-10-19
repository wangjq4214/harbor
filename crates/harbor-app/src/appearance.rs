//! Product appearance recipes. Generic paint/filter mechanisms live in harbor-widget.
use harbor_widget::{
    BackdropFilter, Border, BorderRadius, BoxDecoration, ClipBehavior, DecoratedBox,
    EdgeRefraction, GradientStop, LinearGradient, Paint,
    layout::Point,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
};

/// Shared application material settings. Consumers override only what differs via
/// struct update syntax; filtering, decoration and clipping stay composed together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlassAppearance {
    pub blur_sigma: f32,
    pub refraction: EdgeRefraction,
    pub radius: BorderRadius,
    pub panel_fill: Paint,
    pub panel_border: Border,
    pub input_fill: Color,
    pub input_border: Option<Border>,
    pub input_radius: BorderRadius,
}

impl Default for GlassAppearance {
    fn default() -> Self {
        // A faint neutral black veil supports light text without hiding backdrop
        // colors; the lighter top keeps a subtle surface-reflection variation.
        let surface = LinearGradient::new(
            Point::ZERO,
            Point::new(0.0, 1.0),
            &[
                GradientStop::new(
                    0.0,
                    Color {
                        a: 0.06,
                        ..Color::BLACK
                    },
                ),
                GradientStop::new(
                    0.48,
                    Color {
                        a: 0.10,
                        ..Color::BLACK
                    },
                ),
                GradientStop::new(
                    1.0,
                    Color {
                        a: 0.08,
                        ..Color::BLACK
                    },
                ),
            ],
        )
        .unwrap();
        let edge = LinearGradient::new(
            Point::ZERO,
            Point::new(0.45, 1.0),
            &[
                GradientStop::new(
                    0.0,
                    Color {
                        a: 0.32,
                        ..Color::WHITE
                    },
                ),
                GradientStop::new(
                    0.45,
                    Color {
                        a: 0.03,
                        ..Color::WHITE
                    },
                ),
                GradientStop::new(
                    1.0,
                    Color {
                        a: 0.18,
                        ..Color::WHITE
                    },
                ),
            ],
        )
        .unwrap();
        Self {
            blur_sigma: 8.0,
            refraction: EdgeRefraction::new(12.0, 10.0).unwrap(),
            radius: BorderRadius::all(12.0).unwrap(),
            panel_fill: surface.into(),
            panel_border: Border::with_paint(edge.into(), 1.25).unwrap(),
            input_fill: Color {
                a: 0.07,
                ..Color::BLACK
            },
            input_border: None,
            input_radius: BorderRadius::all(7.0).unwrap(),
        }
    }
}

impl GlassAppearance {
    pub fn command_palette() -> Self {
        Self {
            // The larger palette needs a stronger inset editor surface. A fine
            // outline keeps it identifiable even over a uniformly dark backdrop.
            input_fill: Color {
                a: 0.24,
                ..Color::BLACK
            },
            input_border: Some(
                Border::all(
                    Color {
                        a: 0.18,
                        ..Color::WHITE
                    },
                    1.0,
                )
                .unwrap(),
            ),
            ..Self::default()
        }
    }

    pub fn input(&self) -> BoxDecoration {
        let mut decoration = BoxDecoration::new()
            .try_color(self.input_fill)
            .unwrap()
            .border_radius(self.input_radius);
        if let Some(border) = self.input_border {
            decoration = decoration.border(border);
        }
        decoration
    }

    /// Keep real background filtering inseparable from the translucent panel
    /// recipe. The consumer supplies layout/content, which remain sharp.
    pub fn panel(&self, cx: &mut BuildCx, child: View) -> View {
        let decoration = BoxDecoration::new()
            .try_paint(self.panel_fill)
            .unwrap()
            .border(self.panel_border)
            .border_radius(self.radius);
        let card = DecoratedBox::new(decoration)
            .clip_behavior(ClipBehavior::AntiAlias)
            .child(child)
            .build(cx);
        BackdropFilter::new(self.blur_sigma)
            .unwrap()
            .border_radius(self.radius)
            .edge_refraction(self.refraction)
            .child(card)
            .build(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::{
        ConstrainedBox, renderer::Viewport, runtime::Runtime, scene::primitive::Primitive,
    };
    use std::time::Instant;

    #[test]
    fn palette_overrides_only_the_editor_surface() {
        let shared = GlassAppearance::default();
        let palette = GlassAppearance::command_palette();
        assert_eq!(
            GlassAppearance {
                input_fill: shared.input_fill,
                input_border: shared.input_border,
                ..palette
            },
            shared
        );
        assert_eq!(palette.input().color(), Some(palette.input_fill));
        assert_eq!(palette.input().border_value(), palette.input_border);
        assert_eq!(
            palette.input().border_radius_value(),
            Some(shared.input_radius)
        );
        assert_eq!(
            GlassAppearance::default(),
            shared,
            "local overrides do not mutate defaults"
        );
    }

    #[test]
    fn local_settings_reach_the_composed_filter_and_decoration() {
        let local = GlassAppearance {
            blur_sigma: 4.0,
            refraction: EdgeRefraction::default(),
            radius: BorderRadius::all(20.0).unwrap(),
            panel_fill: Paint::Solid(Color {
                a: 0.2,
                ..Color::BLACK
            }),
            panel_border: Border::all(Color::WHITE, 2.0).unwrap(),
            ..GlassAppearance::default()
        };
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(200, 100, 1.0));
        runtime.set_root(move |cx: &mut BuildCx| {
            let child = ConstrainedBox::new()
                .min_width(200.0)
                .min_height(100.0)
                .build(cx);
            local.panel(cx, child)
        });
        runtime.update(Instant::now());
        let items = &runtime.pending_delta().unwrap().added;
        assert!(items.iter().any(|item| matches!(
            item.primitive,
            Primitive::BackdropBlur {
                sigma: 4.0,
                corner_radii: [20.0, 20.0, 20.0, 20.0],
                ..
            }
        )));
        assert!(items.iter().any(|item| matches!(item.primitive,
            Primitive::RoundedQuad { color, corner_radii, .. }
                if Paint::Solid(color) == local.panel_fill && corner_radii == [20.0; 4])));
        assert!(items.iter().any(|item| matches!(item.primitive,
            Primitive::RoundedBorder { color: Color::WHITE, width: 2.0, corner_radii, .. }
                if corner_radii == [20.0; 4])));
    }
}
