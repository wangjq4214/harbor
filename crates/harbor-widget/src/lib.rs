pub mod construction;
pub use construction::{
    ChildCardinality, ChildConstructionError, Children, ComponentExt, IntoChildView, IntoChildren,
    Keyed, WithChildren,
};
pub use harbor_widget_macros::view;

/// Public construction helpers consumed by generated view code.
#[doc(hidden)]
pub mod __macro_support {
    pub use crate::construction::{
        ChildCardinality, ChildConstructionError, Children, ComponentExt, IntoChildView,
        IntoChildren, Keyed, WithChildren,
    };
    pub use crate::view::Component;
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
pub mod store;
pub use store::{Dispatcher, Store};
pub mod signal;
pub mod text;
pub mod theme;
pub mod view;
pub mod widgets;
pub use layout::{Alignment, FlexFit};
pub use theme::{ControlColors, ControlStyle, ControlVisualState, Theme};
pub use widgets::decorated_box::DecoratedBox;
pub use widgets::{
    Actions, Axis, Button, Column, ConstrainedBox, DEFAULT_SCROLL_LINE_STEP, Expanded, Flex,
    Flexible, Focus, FocusHandle, FocusScope, IconButton, InteractionState, InteractiveRegion,
    KeyChord, LayoutChangedCallback, LayoutObserver, MainAxisAlignment, MouseRegion, Row,
    ScrollArea, ScrollController, ScrollMetrics, Separator, Shortcuts, Spacer, ThemeProvider,
};

#[cfg(test)]
mod macro_tests;

#[cfg(feature = "winit")]
pub mod winit;
