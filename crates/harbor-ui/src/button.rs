use crate::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Hover,
    Pressed,
    Focused,
    Disabled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button<A, W> {
    pub child: W,
    pub intent: A,
    pub enabled: bool,
    pub key: Option<Key>,
}

impl<A, W> Button<A, W> {
    pub fn new(child: W, intent: A) -> Self {
        Self {
            child,
            intent,
            enabled: true,
            key: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.enabled = !disabled;
        self
    }

    pub fn key(mut self, key: Key) -> Self {
        self.key = Some(key);
        self
    }
}
