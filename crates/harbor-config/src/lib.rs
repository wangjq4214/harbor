//! Centralized application constants.
//!
//! This crate consolidates display and behavior parameters that would otherwise
//! be duplicated across independent modules.  It is intentionally not a config
//! file reader or hot-reload source — just a single point of definition so the
//! same constant is never hard-coded in two places.
//!
//! All types are native Rust primitives — no dependency on wgpu or any other
//! graphics library.

// ── Font ──────────────────────────────────────────────────────────────────────

/// Primary terminal font size in points.
pub const FONT_SIZE: f32 = 24.0;

// ── Layout ────────────────────────────────────────────────────────────────────

/// Pixels of padding between the window edge and the terminal grid.
pub const TEXT_PADDING: f32 = 16.0;

// ── Colors ────────────────────────────────────────────────────────────────────

/// Terminal background color (displayed in the clear pass).
///
/// A warm brown tone chosen to reduce eye strain during development.
/// Linear-light values; convert to sRGB or `wgpu::Color` at the rendering boundary.
pub const BACKGROUND: [f32; 4] = [0.36, 0.20, 0.08, 0.25];

/// Unified compositor-level backdrop tint applied to the whole main window
/// including the caption strip (ADR 0026).
pub struct WindowBackdropStyle {
    /// Tint RGB in sRGB space.
    pub tint_rgb: [f32; 3],
    /// Tint opacity applied by the compositor backdrop.
    pub tint_opacity: f32,
    /// Luminosity opacity of the compositor acrylic.
    pub luminosity_opacity: f32,
    /// Opaque fallback RGB in sRGB space when no compositor backdrop exists.
    pub fallback: [f32; 3],
}

impl Default for WindowBackdropStyle {
    fn default() -> Self {
        Self {
            tint_rgb: [1.0, 1.0, 1.0],
            tint_opacity: 0.06,
            luminosity_opacity: 1.0,
            fallback: [0.1176, 0.1176, 0.1176], // #1E1E1E
        }
    }
}
/// Selection highlight color (semi-transparent blue).
pub const SELECTION_COLOR: [f32; 4] = [0.3, 0.5, 0.9, 0.4];

// ── Cursor ────────────────────────────────────────────────────────────────────

/// Cursor blink interval in milliseconds (on/off each half-cycle).
pub const BLINK_INTERVAL_MS: u64 = 530;

// ── Scrollbar ─────────────────────────────────────────────────────────────────

/// Scrollbar track/thumb width in pixels.
pub const SCROLLBAR_WIDTH: f32 = 6.0;
/// Spacing between scrollbar right edge and window right edge.
pub const SCROLLBAR_MARGIN: f32 = 2.0;
/// Scrollbar thumb color (semi-transparent white).
pub const SCROLLBAR_COLOR: [f32; 4] = [0.8, 0.8, 0.8, 0.4];
/// Mouse idle time in ms before auto-hiding the scrollbar.
pub const SCROLLBAR_HIDE_DELAY_MS: u64 = 1500;
/// Minimum thumb height in pixels; ensures the thumb is always visible and draggable.
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;
/// Thumb border radius in pixels. Capsule shape when equal to SCROLLBAR_WIDTH/2.
pub const SCROLLBAR_BORDER_RADIUS: f32 = 3.0;

#[cfg(test)]
mod tests {
    #[test]
    fn should_expose_translucent_warm_brown_when_background_is_read() {
        // Arrange / Act
        let background = super::BACKGROUND;

        // Assert — RGB unchanged from prior warm brown; alpha is the agreed 25% tint.
        assert_eq!(background, [0.36, 0.20, 0.08, 0.25]);
        assert_ne!(background[3], 1.0);
    }

    #[test]
    fn should_expose_unified_backdrop_tint_defaults() {
        // Arrange / Act
        let style = super::WindowBackdropStyle::default();

        // Assert — white 6% tint, default luminosity, #1E1E1E fallback
        assert_eq!(style.tint_rgb, [1.0, 1.0, 1.0]);
        assert_eq!(style.tint_opacity, 0.06);
        assert_eq!(style.luminosity_opacity, 1.0);
        assert_eq!(style.fallback, [0.1176, 0.1176, 0.1176]);
    }

    #[test]
    fn should_preserve_custom_backdrop_values_when_constructed() {
        // Arrange
        let tint_rgb = [0.12, 0.34, 0.56];
        let tint_opacity = 0.78;
        let luminosity_opacity = 0.9;
        let fallback = [0.11, 0.22, 0.33];

        // Act
        let style = super::WindowBackdropStyle {
            tint_rgb,
            tint_opacity,
            luminosity_opacity,
            fallback,
        };

        // Assert
        assert_eq!(style.tint_rgb, tint_rgb);
        assert_eq!(style.tint_opacity, tint_opacity);
        assert_eq!(style.luminosity_opacity, luminosity_opacity);
        assert_eq!(style.fallback, fallback);
    }
}
