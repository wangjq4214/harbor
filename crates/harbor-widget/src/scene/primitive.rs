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

/// Signature for an external draw callback.
///
/// Called by [`Runtime::encode`] when a [`Primitive::External`] is encountered.
/// The callback receives the draw ID, the layout rect in logical dp, and the
/// active RenderPass (with scissor already set).
pub type ExternalDrawFn<'a> =
    dyn Fn(ExternalDrawId, crate::layout::Rect, &mut wgpu::RenderPass<'_>) + 'a;

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
        rect: crate::layout::Rect,
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
