//! Product appearance recipes. Generic paint/filter mechanisms live in harbor-widget.
use harbor_widget::{
    Border, BorderRadius, BoxDecoration, EdgeRefraction, GradientStop, LinearGradient,
    layout::Point, scene::primitive::Color,
};

pub(crate) struct SearchAppearance;

impl SearchAppearance {
    pub const BLUR_SIGMA: f32 = 8.0;

    pub fn refraction() -> EdgeRefraction {
        EdgeRefraction::new(12.0, 10.0).unwrap()
    }
    pub fn radius() -> BorderRadius {
        BorderRadius::all(12.0).unwrap()
    }

    pub fn input() -> BoxDecoration {
        BoxDecoration::new()
            .try_color(Color {
                a: 0.07,
                ..Color::BLACK
            })
            .unwrap()
            .border_radius(BorderRadius::all(7.0).unwrap())
    }

    pub fn panel() -> BoxDecoration {
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
        BoxDecoration::new()
            .try_paint(surface.into())
            .unwrap()
            .border(Border::with_paint(edge.into(), 1.25).unwrap())
            .border_radius(Self::radius())
    }
}
