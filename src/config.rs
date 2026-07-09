//! Centralized application constants.
//!
//! This module consolidates display and behavior parameters that would otherwise
//! be duplicated across independent modules.  It is intentionally not a config
//! file reader or hot-reload source — just a single point of definition so the
//! same constant is never hard-coded in two places.
//!
//! Extend this module when adding a new tunable that more than one file needs.
//! Leave module-private constants (e.g. atlas packing sizes, shader sources) in
//! their owning module.

// ── Font ─────────────────────────────────────────────────────────────────────

/// Primary terminal font size in points.
pub(crate) const FONT_SIZE: f32 = 24.0;

// ── Layout ───────────────────────────────────────────────────────────────────

/// Pixels of padding between the window edge and the terminal grid.
pub(crate) const TEXT_PADDING: f32 = 16.0;

// ── Colors ───────────────────────────────────────────────────────────────────

/// Terminal background color (displayed in the clear pass).
///
/// A warm brown tone chosen to reduce eye strain during development.
/// Add foreground color, selection color, and cursor color here as they are
/// introduced.
pub(crate) const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.36,
    g: 0.20,
    b: 0.08,
    a: 1.0,
};

/// Selection highlight color (semi-transparent blue).
pub(crate) const SELECTION_COLOR: [f32; 4] = [0.3, 0.5, 0.9, 0.4];

// ── Cursor ───────────────────────────────────────────────────────────────────

/// Cursor blink interval in milliseconds (on/off each half-cycle).
pub(crate) const BLINK_INTERVAL_MS: u64 = 530;

// ── Scrollbar ──────────────────────────────────────────────────────────────────

/// Scrollbar track/thumb width in pixels.
pub(crate) const SCROLLBAR_WIDTH: f32 = 6.0;
/// Spacing between scrollbar right edge and window right edge.
pub(crate) const SCROLLBAR_MARGIN: f32 = 2.0;
/// Scrollbar thumb color (semi-transparent white).
pub(crate) const SCROLLBAR_COLOR: [f32; 4] = [0.8, 0.8, 0.8, 0.4];
/// Mouse idle time in ms before auto-hiding the scrollbar.
pub(crate) const SCROLLBAR_HIDE_DELAY_MS: u64 = 1500;
/// Minimum thumb height in pixels; ensures the thumb is always visible and draggable.
pub(crate) const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;
/// Thumb border radius in pixels. Capsule shape when equal to SCROLLBAR_WIDTH/2.
pub(crate) const SCROLLBAR_BORDER_RADIUS: f32 = 3.0;
