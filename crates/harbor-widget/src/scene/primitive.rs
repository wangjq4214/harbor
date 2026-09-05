use crate::layout::{Point, Rect};
use crate::renderer::Viewport;
use std::sync::Arc;

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
    pub const TRANSPARENT: Self = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Returns true when every linear RGBA component is finite.
    pub fn is_finite(&self) -> bool {
        self.r.is_finite() && self.g.is_finite() && self.b.is_finite() && self.a.is_finite()
    }

    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

// ── Primitive ────────────────────────────────────────────────────────────────

pub type TextRunId = u64;
pub type ExternalDrawId = u64;

/// Immutable geometry context for one retained external draw invocation.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalDrawContext {
    /// Logical allocation in dp.
    pub logical_rect: Rect,
    /// Current frame viewport (physical size and scale).
    pub viewport: Viewport,
}

impl ExternalDrawContext {
    pub fn new(logical_rect: Rect, viewport: Viewport) -> Self {
        Self {
            logical_rect,
            viewport,
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.viewport.scale_factor
    }

    pub fn surface_size(&self) -> (u32, u32) {
        self.viewport.physical_size
    }

    /// Physical allocation origin and size within the full surface (pixels).
    pub fn physical_allocation(&self) -> (f32, f32, u32, u32) {
        let scale = self.scale_factor();
        let left = (self.logical_rect.min.x * scale).floor();
        let top = (self.logical_rect.min.y * scale).floor();
        let right = (self.logical_rect.max.x * scale).ceil();
        let bottom = (self.logical_rect.max.y * scale).ceil();
        let width = (right - left).max(0.0) as u32;
        let height = (bottom - top).max(0.0) as u32;
        (left.max(0.0), top.max(0.0), width, height)
    }

    /// Clamped physical scissor `(x, y, width, height)` for wgpu.
    pub fn scissor_rect(&self) -> (u32, u32, u32, u32) {
        Self::compute_scissor(self.logical_rect, self.scale_factor(), self.surface_size())
    }

    pub(crate) fn compute_scissor(
        logical_rect: Rect,
        scale: f32,
        (surf_w, surf_h): (u32, u32),
    ) -> (u32, u32, u32, u32) {
        if surf_w == 0 || surf_h == 0 {
            return (0, 0, 0, 0);
        }

        let left = (logical_rect.min.x * scale).floor() as i64;
        let top = (logical_rect.min.y * scale).floor() as i64;
        let right = (logical_rect.max.x * scale).ceil() as i64;
        let bottom = (logical_rect.max.y * scale).ceil() as i64;

        let clip_left = left.clamp(0, surf_w as i64);
        let clip_top = top.clamp(0, surf_h as i64);
        let clip_right = right.clamp(0, surf_w as i64);
        let clip_bottom = bottom.clamp(0, surf_h as i64);

        let width = (clip_right - clip_left).max(0) as u32;
        let height = (clip_bottom - clip_top).max(0) as u32;
        (clip_left as u32, clip_top as u32, width, height)
    }

    pub fn is_empty(&self) -> bool {
        let (_, _, w, h) = self.scissor_rect();
        w == 0 || h == 0
    }
}

/// Whether this external paint may upload live content or must replay last GPU state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalDrawMode {
    /// Provider prepares current content, then draws.
    Live,
    /// Provider draws last committed buffers only.
    Retain,
}

impl ExternalDrawMode {
    /// Live when this id is eligible or the pass is a deferred-external commit.
    pub const fn from_eligibility(eligible: bool, commit: bool) -> Self {
        if eligible || commit {
            Self::Live
        } else {
            Self::Retain
        }
    }
}

/// Signature for an external draw callback.
///
/// Called by [`crate::runtime::Runtime::encode`] when a [`Primitive::External`] is
/// encountered. The callback receives the draw ID, the geometry context, the
/// active RenderPass (with scissor already set), and whether this pass may
/// upload live content.
pub type ExternalDrawFn<'a> =
    dyn Fn(ExternalDrawId, &ExternalDrawContext, &mut wgpu::RenderPass<'_>, ExternalDrawMode) + 'a;

/// Widget-neutral scheduling snapshot from one external schedule provider.
///
/// Mirrors terminal Frame Demand shape without depending on harbor-terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalScheduleDemand {
    /// True when the host should request a frame before waiting on `deadline`.
    pub redraw_now: bool,
    /// Earliest next animation/deadline boundary, when one is active.
    pub deadline: Option<std::time::Instant>,
    /// False when an ordinary present should be deferred. Default empty demand is eligible.
    pub ordinary_present_eligible: bool,
}

impl ExternalScheduleDemand {
    /// Empty demand used when a provider has nothing to schedule.
    pub const fn empty() -> Self {
        Self {
            redraw_now: false,
            deadline: None,
            ordinary_present_eligible: true,
        }
    }
}

impl Default for ExternalScheduleDemand {
    fn default() -> Self {
        Self::empty()
    }
}

/// Signature for an external schedule callback.
///
/// Invoked by [`crate::runtime::Runtime::update`] before idle wait selection.
/// Providers may only report demand — they must not acquire, submit, or present.
pub type ExternalScheduleFn = dyn Fn(ExternalDrawId, std::time::Instant) -> ExternalScheduleDemand;

/// Standardized draw input produced by widgets during the paint pass.
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Quad {
        rect: Rect,
        color: Color,
        corner_radius: f32,
    },
    Text {
        text: Arc<str>,
        origin: Point,
        color: Color,
    },
    Border {
        rect: Rect,
        width: f32,
        color: Color,
        corner_radius: f32,
    },
    /// A bounded outer-only rounded shadow in logical pixels.
    ///
    /// `rect` bounds raster coverage; `shape_rect` is the unblurred rounded
    /// shadow shape after offset and spread. `occluder_rect` is the original
    /// decorated box whose interior must remain free of outer-shadow coverage.
    OuterShadow {
        rect: Rect,
        shape_rect: Rect,
        occluder_rect: Rect,
        color: Color,
        corner_radii: [f32; 4],
        occluder_radii: [f32; 4],
        blur_radius: f32,
    },
    /// A fill with independently resolved corner radii.
    RoundedQuad {
        rect: Rect,
        color: Color,
        corner_radii: [f32; 4],
    },
    /// A uniform-width outline with independently resolved corner radii.
    RoundedBorder {
        rect: Rect,
        width: f32,
        color: Color,
        corner_radii: [f32; 4],
    },
    External {
        draw: ExternalDrawId,
        rect: crate::layout::Rect,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Point, Rect};
    use crate::renderer::Viewport;

    #[test]
    fn color_constants() {
        assert_eq!(Color::WHITE.to_array(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(Color::BLACK.to_array(), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(Color::RED.to_array(), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(Color::GREEN.to_array(), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(Color::BLUE.to_array(), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(Color::TRANSPARENT.to_array(), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn color_finiteness_checks_every_component() {
        assert!(Color::TRANSPARENT.is_finite());
        for color in [
            Color {
                r: f32::NAN,
                ..Color::WHITE
            },
            Color {
                g: f32::NAN,
                ..Color::WHITE
            },
            Color {
                b: f32::NAN,
                ..Color::WHITE
            },
            Color {
                a: f32::NAN,
                ..Color::WHITE
            },
            Color {
                r: f32::INFINITY,
                ..Color::WHITE
            },
            Color {
                g: f32::INFINITY,
                ..Color::WHITE
            },
            Color {
                b: f32::INFINITY,
                ..Color::WHITE
            },
            Color {
                a: f32::INFINITY,
                ..Color::WHITE
            },
            Color {
                r: f32::NEG_INFINITY,
                ..Color::WHITE
            },
            Color {
                g: f32::NEG_INFINITY,
                ..Color::WHITE
            },
            Color {
                b: f32::NEG_INFINITY,
                ..Color::WHITE
            },
            Color {
                a: f32::NEG_INFINITY,
                ..Color::WHITE
            },
        ] {
            assert!(!color.is_finite());
        }
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

    #[test]
    fn should_report_physical_allocation_from_logical_rect_at_scale() {
        // Arrange
        let rect = Rect::from_min_size(
            Point::new(10.0, 5.0),
            crate::layout::Size::new(200.0, 100.0),
        );
        let context = ExternalDrawContext::new(rect, Viewport::new(800, 600, 2.0));

        // Act
        let (origin_x, origin_y, width, height) = context.physical_allocation();

        // Assert: logical dp is multiplied by scale and floored/ceiled to pixels.
        assert_eq!((origin_x, origin_y), (20.0, 10.0));
        assert_eq!((width, height), (400, 200));
        assert_eq!(context.scale_factor(), 2.0);
        assert_eq!(context.surface_size(), (800, 600));
    }

    #[test]
    fn should_clamp_scissor_rect_to_surface_bounds() {
        // Arrange: rect extends past the right/bottom edge of the surface.
        let rect = Rect::from_min_size(
            Point::new(750.0, 550.0),
            crate::layout::Size::new(100.0, 100.0),
        );
        let context = ExternalDrawContext::new(rect, Viewport::new(800, 600, 1.0));

        // Act
        let (x, y, width, height) = context.scissor_rect();

        // Assert
        assert_eq!((x, y, width, height), (750, 550, 50, 50));
        assert!(!context.is_empty());
    }

    #[test]
    fn should_report_empty_when_surface_or_scissor_has_zero_extent() {
        // Arrange
        let rect =
            Rect::from_min_size(Point::new(10.0, 10.0), crate::layout::Size::new(50.0, 50.0));
        let zero_surface = ExternalDrawContext::new(rect, Viewport::new(0, 600, 1.0));
        let zero_rect = ExternalDrawContext::new(
            Rect::from_min_size(Point::ZERO, crate::layout::Size::ZERO),
            Viewport::new(800, 600, 1.0),
        );

        // Assert
        assert!(zero_surface.is_empty());
        assert_eq!(zero_surface.scissor_rect(), (0, 0, 0, 0));
        assert!(zero_rect.is_empty());
    }

    #[test]
    fn should_clamp_negative_logical_origin_to_zero_in_physical_allocation() {
        // Arrange: logical rect starts off-surface.
        let rect = Rect::from_min_size(
            Point::new(-10.0, -5.0),
            crate::layout::Size::new(40.0, 30.0),
        );
        let context = ExternalDrawContext::new(rect, Viewport::new(800, 600, 1.0));

        // Act
        let (origin_x, origin_y, width, height) = context.physical_allocation();

        // Assert: origin is clamped; size still spans the unclamped logical extent.
        assert_eq!((origin_x, origin_y), (0.0, 0.0));
        assert_eq!((width, height), (40, 30));
    }

    #[test]
    fn should_round_fractional_logical_rect_outward_to_physical_pixels() {
        // Arrange: fractional dp edges must floor origin and ceil extent.
        let rect = Rect::from_min_size(
            Point::new(10.25, 5.75),
            crate::layout::Size::new(20.5, 15.25),
        );
        let context = ExternalDrawContext::new(rect, Viewport::new(800, 600, 2.0));

        // Act
        let (origin_x, origin_y, width, height) = context.physical_allocation();
        let scissor = context.scissor_rect();

        // Assert: physical left/top floor, right/bottom ceil.
        // 10.25×2=20.5→20, 5.75×2=11.5→11; 30.75×2=61.5→62, 21×2=42→42
        assert_eq!((origin_x, origin_y), (20.0, 11.0));
        assert_eq!((width, height), (42, 31));
        assert_eq!(scissor, (20, 11, 42, 31));
    }

    #[test]
    fn should_compute_scissor_at_2x_scale_within_surface() {
        // Arrange
        let rect = Rect::from_min_size(
            Point::new(100.0, 50.0),
            crate::layout::Size::new(200.0, 100.0),
        );
        let context = ExternalDrawContext::new(rect, Viewport::new(800, 600, 2.0));

        // Act
        let scissor = context.scissor_rect();

        // Assert
        assert_eq!(scissor, (200, 100, 400, 200));
        assert!(!context.is_empty());
    }

    #[test]
    fn should_report_no_redraw_and_no_deadline_when_schedule_demand_is_empty() {
        // Arrange / Act
        let empty = ExternalScheduleDemand::empty();

        // Assert
        assert_eq!(empty, ExternalScheduleDemand::default());
        assert!(!empty.redraw_now);
        assert!(empty.deadline.is_none());
        assert!(empty.ordinary_present_eligible);
    }

    #[test]
    fn should_preserve_redraw_and_deadline_fields_when_schedule_demand_is_constructed() {
        // Arrange
        let deadline = std::time::Instant::now();

        // Act
        let demand = ExternalScheduleDemand {
            redraw_now: true,
            deadline: Some(deadline),
            ..ExternalScheduleDemand::empty()
        };

        // Assert
        assert!(demand.redraw_now);
        assert_eq!(demand.deadline, Some(deadline));
        assert!(demand.ordinary_present_eligible);
        assert_ne!(demand, ExternalScheduleDemand::empty());
    }

    #[test]
    fn should_preserve_deferred_eligibility_when_schedule_demand_is_constructed() {
        // Arrange
        let deadline = std::time::Instant::now();

        // Act
        let demand = ExternalScheduleDemand {
            redraw_now: true,
            deadline: Some(deadline),
            ordinary_present_eligible: false,
        };

        // Assert
        assert!(demand.redraw_now);
        assert_eq!(demand.deadline, Some(deadline));
        assert!(!demand.ordinary_present_eligible);
        assert_ne!(demand, ExternalScheduleDemand::empty());
    }

    #[test]
    fn should_select_live_when_eligible_or_commit() {
        assert_eq!(
            ExternalDrawMode::from_eligibility(true, false),
            ExternalDrawMode::Live
        );
        assert_eq!(
            ExternalDrawMode::from_eligibility(false, true),
            ExternalDrawMode::Live
        );
        assert_eq!(
            ExternalDrawMode::from_eligibility(true, true),
            ExternalDrawMode::Live
        );
    }

    #[test]
    fn should_select_retain_when_ineligible_without_commit() {
        assert_eq!(
            ExternalDrawMode::from_eligibility(false, false),
            ExternalDrawMode::Retain
        );
        assert_ne!(ExternalDrawMode::Live, ExternalDrawMode::Retain);
    }
}
