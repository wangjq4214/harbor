use crate::layout::{BoxConstraints, Point, Size};
use crate::signal::Signal;
use crate::theme::Theme;
use crate::view::{AnyView, BuildCx, Component, ThemedView, View};
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
    /// Validates both control roles and spacing before accepting a static theme.
    pub fn new(theme: Theme) -> Result<Self, crate::theme::AppearanceError> {
        theme.validate()?;
        Ok(Self {
            source: ThemeSource::Static(Box::new(theme)),
            child: None,
        })
    }

    /// Uses a signal as the provider value. Writes rebuild the provider subtree.
    /// Use [`Signal::new_distinct`] to avoid rebuilding for equal theme values.
    /// Every consumed value is validated; invalid fields panic without fallback.
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
                let theme = signal.read().clone();
                theme
                    .validate()
                    .unwrap_or_else(|error| panic!("ThemeProvider::from_signal: {error}"));
                theme
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
    type Capability = crate::SingleChild;
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

    fn as_themed(&self) -> Option<&dyn ThemedView> {
        Some(self)
    }
}

impl ThemedView for ThemeProviderView {
    fn theme_override(&self) -> Option<Arc<Theme>> {
        Some(self.theme.clone())
    }
}
