//! The bounded, built-in Lucide artwork used by Harbor controls.
//!
//! These icons are precompiled from the bundled SVGs into the quad renderer's
//! signed-distance functions. This is not a general SVG loader. The original
//! 24-unit viewBox, 2-unit strokes, round caps and joins are preserved; `Dot`
//! uses the equivalent filled 2-unit-radius disk for the unread indicator.
//! See `assets/lucide/LICENSE` for Lucide and Feather license notices.

/// Built-in vector icons, independent of system fonts and text metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LucideIcon {
    Terminal,
    Plus,
    X,
    Dot,
    ChevronUp,
    ChevronDown,
}

impl LucideIcon {
    /// Original bundled SVG artwork. No network or installed font is needed.
    pub const fn svg_source(self) -> &'static str {
        match self {
            Self::Terminal => include_str!("../assets/lucide/terminal.svg"),
            Self::Plus => include_str!("../assets/lucide/plus.svg"),
            Self::X => include_str!("../assets/lucide/x.svg"),
            Self::Dot => include_str!("../assets/lucide/dot.svg"),
            Self::ChevronUp => include_str!("../assets/lucide/chevron-up.svg"),
            Self::ChevronDown => include_str!("../assets/lucide/chevron-down.svg"),
        }
    }

    // Modes 0..=2 are reserved for boxes, shadows and clip masks.
    pub(crate) const fn render_mode(self) -> u32 {
        match self {
            Self::Terminal => 3,
            Self::Plus => 4,
            Self::X => 5,
            Self::Dot => 6,
            Self::ChevronUp => 7,
            Self::ChevronDown => 8,
        }
    }
}
