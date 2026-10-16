//! Product appearance recipes. Generic paint/filter mechanisms live in harbor-widget.
use harbor_widget::{
    Border, BorderRadius, BoxDecoration, BoxShadow, GradientStop, LinearGradient, layout::Point,
    scene::primitive::Color,
};

pub(crate) struct SearchAppearance;

impl SearchAppearance {
    pub const BLUR_SIGMA: f32 = 8.0;

    pub fn radius() -> BorderRadius {
        BorderRadius::all(12.0).unwrap()
    }

    pub fn input() -> BoxDecoration {
        BoxDecoration::new()
            .try_color(Color {
                a: 0.14,
                ..Color::BLACK
            })
            .unwrap()
            .border_radius(BorderRadius::all(7.0).unwrap())
    }

    pub fn panel() -> BoxDecoration {
        // Weak coverage preserves the colors of the real, locally blurred
        // background; the ramp adds a surface reflection, not an opaque card.
        let surface = LinearGradient::new(
            Point::ZERO,
            Point::new(0.0, 1.0),
            &[
                GradientStop::new(
                    0.0,
                    Color {
                        a: 0.12,
                        ..Color::WHITE
                    },
                ),
                GradientStop::new(
                    0.48,
                    Color {
                        r: 0.20,
                        g: 0.24,
                        b: 0.30,
                        a: 0.045,
                    },
                ),
                GradientStop::new(
                    1.0,
                    Color {
                        r: 0.50,
                        g: 0.56,
                        b: 0.64,
                        a: 0.075,
                    },
                ),
            ],
        )
        .unwrap();
        let edge = LinearGradient::new(
            Point::ZERO,
            Point::new(0.15, 1.0),
            &[
                GradientStop::new(
                    0.0,
                    Color {
                        a: 0.46,
                        ..Color::WHITE
                    },
                ),
                GradientStop::new(
                    0.45,
                    Color {
                        a: 0.08,
                        ..Color::WHITE
                    },
                ),
                GradientStop::new(
                    1.0,
                    Color {
                        a: 0.22,
                        ..Color::WHITE
                    },
                ),
            ],
        )
        .unwrap();
        BoxDecoration::new()
            .try_paint(surface.into())
            .unwrap()
            .border(Border::with_paint(edge.into(), 1.0).unwrap())
            .border_radius(Self::radius())
            .shadow(
                BoxShadow::new()
                    .try_color(Color {
                        a: 0.18,
                        ..Color::BLACK
                    })
                    .unwrap()
                    .try_offset(Point::new(0.0, 2.0))
                    .unwrap()
                    .try_blur_radius(6.0)
                    .unwrap(),
            )
    }
}
