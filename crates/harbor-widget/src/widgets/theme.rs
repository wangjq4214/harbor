use crate::layout::{BoxConstraints, Point, Size};
use crate::signal::Signal;
use crate::theme::Theme;
use crate::view::{AnyView, BuildCx, Component, View};
use std::sync::Arc;

#[derive(Clone)]
enum ThemeSource {
    Static(Box<Theme>),
    Signal(Signal<Theme>),
}

/// Supplies an inherited semantic theme to its descendants.
#[derive(Clone)]
pub struct ThemeProvider {
    source: ThemeSource,
    child: Option<View>,
}

impl ThemeProvider {
    pub fn new(theme: Theme) -> Self {
        Self {
            source: ThemeSource::Static(Box::new(theme)),
            child: None,
        }
    }

    /// Uses a signal as the provider value. Writes rebuild the provider subtree.
    /// Use [`Signal::new_distinct`] to avoid rebuilding for equal theme values.
    pub fn from_signal(theme: Signal<Theme>) -> Self {
        Self {
            source: ThemeSource::Signal(theme),
            child: None,
        }
    }

    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }
}

impl Component for ThemeProvider {
    fn build(&self, cx: &mut BuildCx) -> View {
        let theme = match &self.source {
            ThemeSource::Static(theme) => (**theme).clone(),
            ThemeSource::Signal(signal) => {
                cx.track(signal);
                signal.read().clone()
            }
        };
        View::new(
            ThemeProviderView {
                theme: Arc::new(theme),
            },
            self.child.iter().cloned().collect(),
            None,
        )
    }
}

impl crate::WithChildren for ThemeProvider {
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("ThemeProvider", &mut self.child)?;
        Ok(self)
    }
}

#[derive(Clone)]
struct ThemeProviderView {
    theme: Arc<Theme>,
}

impl AnyView for ThemeProviderView {
    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &crate::text::TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(child_sizes.first().copied().unwrap_or(Size::ZERO));
        (size, vec![Point::ZERO; child_sizes.len()])
    }

    fn theme_override(&self) -> Option<Arc<Theme>> {
        Some(self.theme.clone())
    }
}
