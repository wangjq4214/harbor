//! Product appearance recipes. Generic paint/filter mechanisms live in harbor-widget.
use harbor_widget::{
    BackdropFilter, BackdropMaterial, BackdropRim, BackdropTone, Border, BorderRadius,
    BoxDecoration, ClipBehavior, DecoratedBox, EdgeRefraction, GradientStop, InteractionState,
    LinearGradient, Paint,
    layout::Point,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
};

/// Transparent overlay controls share the search panel's hover/press feedback.
pub(crate) fn glass_icon_button_style(cx: &BuildCx) -> harbor_widget::theme::ControlStyle {
    let mut style = cx.theme().icon_button.clone();
    style.min_height = 32.0;
    style.icon_size = 18.0;
    style.horizontal_padding = 7.0;
    style.border_width = 0.0;
    for colors in [
        &mut style.normal,
        &mut style.hovered,
        &mut style.pressed,
        &mut style.selected,
        &mut style.disabled,
    ] {
        colors.background = Color::TRANSPARENT;
    }
    style.normal.foreground.a = 0.78;
    style.hovered.foreground.a = 1.0;
    style.pressed.foreground.a = 0.5;
    style
}

/// Shared application material settings. Consumers override only what differs via
/// struct update syntax; filtering, decoration and clipping stay composed together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlassAppearance {
    pub blur_sigma: f32,
    pub refraction: EdgeRefraction,
    pub material: Option<BackdropMaterial>,
    pub radius: BorderRadius,
    pub content_clip: ClipBehavior,
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
        Self {
            blur_sigma: 8.0,
            refraction: EdgeRefraction::new(12.0, 10.0).unwrap(),
            radius: BorderRadius::all(12.0).unwrap(),
            content_clip: ClipBehavior::AntiAlias,
            panel_fill: surface.into(),
            // The shader now paints the geometric rim; don't double it with a border.
            panel_border: Border::all(Color::TRANSPARENT, 0.0).unwrap(),
            // Adapt the reference dark Regular tone toward identity to retain the
            // existing faint/translucent panels. All three consumers share this.
            material: Some(
                BackdropMaterial::new(
                    21.8,
                    -0.25,
                    BackdropTone {
                        offset: 0.105 * 0.35,
                        slope: 1.0 + (0.83 - 1.0) * 0.35,
                        quadratic: -0.2091 * 0.35,
                        saturation: 1.0648,
                    },
                    BackdropRim {
                        highlight: 0.30,
                        shade: 0.22,
                    },
                )
                .unwrap(),
            ),
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

    /// Row-local glass, lighter than the panel and absent on idle suggestions.
    pub fn command_palette_row(foreground: Color, state: InteractionState) -> Option<Self> {
        let alpha = if !state.disabled && state.pressed {
            0.14
        } else if state.selected {
            if !state.disabled && state.hovered {
                0.12
            } else {
                0.10
            }
        } else if !state.disabled && state.hovered {
            0.06
        } else {
            return None;
        };
        Some(Self {
            blur_sigma: 2.0,
            refraction: EdgeRefraction::new(6.0, 2.0).unwrap(),
            material: Some(
                BackdropMaterial::new(
                    4.0,
                    -0.1,
                    BackdropTone {
                        offset: 0.0,
                        slope: 1.0,
                        quadratic: 0.0,
                        saturation: 1.02,
                    },
                    BackdropRim {
                        highlight: 0.30,
                        shade: 0.22,
                    },
                )
                .unwrap(),
            ),
            radius: BorderRadius::all(12.0).unwrap(),
            // Row content is inset and already scroll-clipped. Another rounded
            // child mask would consume a third exact clip slot and cut keycaps.
            content_clip: ClipBehavior::None,
            panel_fill: Paint::Solid(Color {
                a: alpha,
                ..foreground
            }),
            ..Self::default()
        })
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
            .clip_behavior(self.content_clip)
            .child(child)
            .build(cx);
        BackdropFilter::new(self.blur_sigma)
            .unwrap()
            .border_radius(self.radius)
            .edge_refraction(self.refraction)
            .material(self.material)
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
    fn named_material_migration_preserves_panel_and_row_recipes() {
        let shared = GlassAppearance::default();
        let material = shared.material.unwrap();
        assert_eq!(shared.blur_sigma, 8.0);
        assert_eq!(shared.refraction, EdgeRefraction::new(12.0, 10.0).unwrap());
        assert_eq!(material.frost(), [21.8, -0.25]);
        assert_eq!(
            material.tone(),
            [
                0.105 * 0.35,
                1.0 + (0.83 - 1.0) * 0.35,
                -0.2091 * 0.35,
                1.0648,
            ]
        );
        assert_eq!(material.rim(), [0.30, 0.22]);
        assert_eq!(GlassAppearance::command_palette().material, shared.material);
        for state in [
            InteractionState {
                hovered: true,
                ..Default::default()
            },
            InteractionState {
                pressed: true,
                ..Default::default()
            },
            InteractionState {
                selected: true,
                ..Default::default()
            },
        ] {
            let row = GlassAppearance::command_palette_row(Color::WHITE, state).unwrap();
            let material = row.material.unwrap();
            assert_eq!(row.blur_sigma, 2.0);
            assert_eq!(row.refraction, EdgeRefraction::new(6.0, 2.0).unwrap());
            assert_eq!(material.frost(), [4.0, -0.1]);
            assert_eq!(material.tone(), [0.0, 1.0, 0.0, 1.02]);
            assert_eq!(material.rim(), [0.30, 0.22]);
        }
        assert!(GlassAppearance::command_palette_row(Color::WHITE, Default::default()).is_none());
    }

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
            material: None,
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
