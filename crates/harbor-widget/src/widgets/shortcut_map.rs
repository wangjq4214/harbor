use crate::input::event::{Key, Modifiers};
use crate::layout::{BoxConstraints, Point, Size};
use crate::view::{ActionProviderView, AnyView, BuildCx, Component, View};
use std::any::Any;

/// A keyboard key plus its complete modifier set.
///
/// Construct with [`KeyChord::new`]: ASCII character keys are normalized to
/// lowercase, without changing modifiers or merging ordinary and keypad keys.
/// Non-ASCII characters are preserved (no Unicode case folding).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    key: Key,
    modifiers: Modifiers,
}

impl KeyChord {
    /// Creates a normalized chord with the complete modifier set.
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        let key = match key {
            Key::Character(character) => Key::Character(character.to_ascii_lowercase()),
            Key::NumpadCharacter(character) => Key::NumpadCharacter(character.to_ascii_lowercase()),
            key => key,
        };
        Self { key, modifiers }
    }

    /// Returns the normalized key, preserving its ordinary/keypad variant.
    pub const fn key(&self) -> Key {
        self.key
    }

    /// Returns the complete, unchanged modifier set used for exact matching.
    pub const fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

/// Maps key chords to typed action values for one descendant subtree.
#[derive(Clone)]
pub struct ShortcutMap<A: Clone + 'static> {
    bindings: Vec<(KeyChord, A)>,
    child: Option<View>,
}

impl<A: Clone + 'static> Default for ShortcutMap<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Clone + 'static> ShortcutMap<A> {
    /// Creates an empty shortcut map for declarative `view!` composition.
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            child: None,
        }
    }

    /// Replaces the single child slot.
    pub fn child(mut self, child: impl crate::IntoChildView) -> Self {
        self.child = Some(child.into_child_view());
        self
    }

    /// Adds a binding. If equal normalized chords are bound more than once,
    /// the first matching binding in this instance wins; this does not replace it.
    pub fn bind(mut self, chord: KeyChord, action: A) -> Self {
        self.bindings.push((chord, action));
        self
    }
}

impl<A: Clone + 'static> Component for ShortcutMap<A> {
    fn build(&self, _cx: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}

impl<A: Clone + 'static> crate::WithChildren for ShortcutMap<A> {
    type Capability = crate::SingleChild;
    fn with_children(
        mut self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.into_single("ShortcutMap", &mut self.child)?;
        Ok(self)
    }
}

impl<A: Clone + 'static> AnyView for ShortcutMap<A> {
    fn layout_children(
        &self,
        constraints: BoxConstraints,
        child_sizes: &[Size],
        _metrics: &crate::text::TextMetrics,
    ) -> (Size, Vec<Point>) {
        let size = constraints.constrain(child_sizes.first().copied().unwrap_or(Size::ZERO));
        (size, vec![Point::ZERO; child_sizes.len()])
    }

    fn as_action_provider(&self) -> Option<&dyn ActionProviderView> {
        Some(self)
    }
}

impl<A: Clone + 'static> ActionProviderView for ShortcutMap<A> {
    fn shortcut_action(&self, chord: KeyChord) -> Option<Box<dyn Any>> {
        self.bindings
            .iter()
            .find(|(binding, _)| *binding == chord)
            .map(|(_, action)| Box::new(action.clone()) as Box<dyn Any>)
    }
}
