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
pub const FONT_SIZE: f32 = 14.0;

// ── Layout ────────────────────────────────────────────────────────────────────

/// Pixels of padding between the window edge and the terminal grid.
pub const TEXT_PADDING: f32 = 16.0;

// ── Colors ────────────────────────────────────────────────────────────────────

/// Terminal background color (displayed in the clear pass).
///
/// A warm brown tone chosen to reduce eye strain during development.
/// Linear-light values; convert to sRGB or `wgpu::Color` at the rendering boundary.
pub const BACKGROUND: [f32; 4] = [0.36, 0.20, 0.08, 1.0];

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
