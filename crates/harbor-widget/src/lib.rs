pub mod construction;
pub use construction::{
    AcceptsChildCount, ChildCardinality, ChildConstructionError, Children, ComponentExt,
    IntoChildView, Keyed, MultiChild, SingleChild, WithChildren, with_static_children,
};
pub use harbor_widget_macros::view;

/// Public construction helpers consumed by generated view code.
#[doc(hidden)]
pub mod __macro_support {
    pub use crate::construction::resolve_node;
    pub use crate::construction::{
        AcceptsChildCount, ChildCardinality, ChildConstructionError, Children, ComponentExt,
        IntoChildView, Keyed, MultiChild, SingleChild, WithChildren, with_static_children,
    };
    pub use crate::view::Component;
}

pub mod icon;
pub use icon::LucideIcon;
pub mod paint;
pub use paint::{GradientError, GradientStop, LinearGradient, Paint};
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
pub mod store;
pub use store::{Dispatcher, Store};
pub mod signal;
pub mod text;
pub mod text_editing;
pub use text_editing::{TextEditingController, TextEditingValue, TextSelection};
pub mod theme;
pub mod view;
pub mod widgets;
pub use layout::{Alignment, Alignment2D, FlexFit, Insets, LayoutConfigError, PositionAlignment};
pub use theme::{ControlColors, ControlStyle, ControlVisualState, Theme};
pub use widgets::backdrop_filter::{BackdropFilter, BackdropMaterial, EdgeRefraction};
pub use widgets::decorated_box::DecoratedBox;
pub use widgets::{
    ActionOutcome, Actions, Axis, Button, CONTENT_SLOT_KEY, Column, ConstrainedBox,
    DEFAULT_SCROLL_LINE_STEP, Expanded, Flex, FlexOptions, Flexible, Focus, FocusHandle,
    FocusScope, Icon, IconButton, InteractionState, InteractiveRegion, KeyChord,
    LayoutChangedCallback, LayoutObserver, MainAxisAlignment, MouseRegion, Notification, Portal,
    Row, ScrollController, ScrollMetrics, ScrollView, Separator, Shortcuts, Spacer, ThemeProvider,
};
pub use widgets::{EditableText, TextField};
pub use widgets::{align::Align, padding::Padding, sized_box::SizedBox};

#[cfg(test)]
mod layout_component_tests;

#[cfg(test)]
mod macro_tests;

#[cfg(feature = "winit")]
pub mod winit;
