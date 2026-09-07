pub mod construction;
pub use construction::{
    ChildCardinality, ChildConstructionError, Children, ComponentExt, IntoChildView, Keyed,
    WithChildren,
};

/// Public construction helpers consumed by generated view code.
#[doc(hidden)]
pub mod __macro_support {
    pub use crate::construction::{
        ChildCardinality, ChildConstructionError, Children, ComponentExt, IntoChildView, Keyed,
        WithChildren,
    };
}

pub mod decoration;
pub use decoration::{
    Border, BorderRadius, BoxDecoration, BoxShadow, ClipBehavior, DecorationError,
    NormalizedBorderRadius,
};

pub mod effects;
pub use effects::{
    ClipboardEffect, ControlFlowEffect, CursorEffect, CursorShape, ExternalInvalidation, ImeEffect,
    RuntimeEffects,
};
pub mod fiber;
pub mod input;
pub mod layout;
pub mod renderer;
pub mod runtime;
pub mod scene;
#[cfg(any(feature = "winit", test))]
mod scheduler;
pub mod signal;
pub mod text;
pub mod view;
pub mod widgets;
pub use layout::{Alignment, FlexFit};
pub use widgets::decorated_box::DecoratedBox;
pub use widgets::{
    Axis, Column, ConstrainedBox, Expanded, Flex, Flexible, MainAxisAlignment, Row, Separator,
    Spacer,
};

#[cfg(feature = "winit")]
pub mod winit;
