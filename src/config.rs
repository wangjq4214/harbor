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

// ── Cursor ───────────────────────────────────────────────────────────────────

/// Cursor blink interval in milliseconds (on/off each half-cycle).
pub(crate) const BLINK_INTERVAL_MS: u64 = 530;
