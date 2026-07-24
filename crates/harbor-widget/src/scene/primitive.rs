use crate::layout::{Point, Rect};

// ── Color ───────────────────────────────────────────────────────────────────

/// RGBA color with linear f32 components.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const RED: Self = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const GREEN: Self = Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const BLUE: Self = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

// ── Primitive ────────────────────────────────────────────────────────────────

pub type TextRunId = u64;
pub type ExternalDrawId = u64;

/// Standardized draw input produced by widgets during the paint pass.
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Quad {
        rect: Rect,
        color: Color,
        corner_radius: f32,
    },
    Text {
        run: TextRunId,
        origin: Point,
        color: Color,
    },
    Border {
        rect: Rect,
        width: f32,
        color: Color,
        corner_radius: f32,
    },
    External {
        draw: ExternalDrawId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_constants() {
        assert_eq!(Color::WHITE.to_array(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(Color::BLACK.to_array(), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(Color::RED.to_array(), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(Color::GREEN.to_array(), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(Color::BLUE.to_array(), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn color_to_array() {
        let c = Color {
            r: 0.5,
            g: 0.25,
            b: 0.75,
            a: 0.9,
        };
        assert_eq!(c.to_array(), [0.5, 0.25, 0.75, 0.9]);
    }
}
