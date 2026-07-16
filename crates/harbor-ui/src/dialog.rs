use crate::{Button, ButtonState, Key, Rect, Text};
use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key as WinitKey, NamedKey},
};

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSpec {
    pub title: String,
    pub preferred_width: f32,
    pub preferred_height: f32,
    pub resizable: bool,
}

impl WindowSpec {
    pub fn fixed(title: impl Into<String>, width: f32, height: f32) -> Self {
        Self {
            title: title.into(),
            preferred_width: width,
            preferred_height: height,
            resizable: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dialog<A, W = Text> {
    pub window: WindowSpec,
    pub title: Option<Text>,
    pub body: W,
    pub actions: Vec<Button<A, Text>>,
    pub initial_focus: Option<Key>,
}

impl<A, W> Dialog<A, W> {
    pub fn new(window: WindowSpec, body: W) -> Self {
        Self {
            window,
            title: None,
            body,
            actions: Vec::new(),
            initial_focus: None,
        }
    }

    pub fn title(mut self, title: Text) -> Self {
        self.title = Some(title);
        self
    }

    pub fn actions(mut self, actions: Vec<Button<A, Text>>) -> Self {
        self.actions = actions;
        self
    }

    pub fn initial_focus(mut self, key: Key) -> Self {
        self.initial_focus = Some(key);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogEvent {
    None,
    ScrollChanged,
}

/// Retained dialog interaction state. The host owns the dialog's business data.
pub struct DialogRuntime<A> {
    focused: Option<usize>,
    pressed: Option<usize>,
    pointer: Option<(f32, f32)>,
    pub scroll_offset: usize,
    _intent: std::marker::PhantomData<A>,
}

impl<A> Default for DialogRuntime<A> {
    fn default() -> Self {
        Self {
            focused: None,
            pressed: None,
            pointer: None,
            scroll_offset: 0,
            _intent: std::marker::PhantomData,
        }
    }
}

impl<A> DialogRuntime<A> {
    pub fn sync<W>(&mut self, dialog: &Dialog<A, W>) {
        if let Some(key) = dialog.initial_focus
            && let Some(index) = dialog
                .actions
                .iter()
                .position(|button| button.enabled && button.key == Some(key))
        {
            self.focused = Some(index);
        } else if self.focused.is_none() {
            self.focused = dialog.actions.iter().position(|button| button.enabled);
        }
    }

    pub fn focused_state<W>(&self, dialog: &Dialog<A, W>, index: usize) -> ButtonState {
        let button = &dialog.actions[index];
        if !button.enabled {
            ButtonState::Disabled
        } else if self.pressed == Some(index) {
            ButtonState::Pressed
        } else if self.focused == Some(index) {
            ButtonState::Focused
        } else {
            ButtonState::Normal
        }
    }

    pub fn handle_key<'a, W>(
        &mut self,
        dialog: &'a Dialog<A, W>,
        event: &WindowEvent,
    ) -> Option<&'a A> {
        self.handle_key_with_shift(dialog, event, false)
    }

    pub fn handle_key_with_shift<'a, W>(
        &mut self,
        dialog: &'a Dialog<A, W>,
        event: &WindowEvent,
        shift: bool,
    ) -> Option<&'a A> {
        let WindowEvent::KeyboardInput { event, .. } = event else {
            return None;
        };
        if event.state != ElementState::Pressed {
            return None;
        }
        match &event.logical_key {
            WinitKey::Named(NamedKey::Tab) => {
                self.advance_focus(dialog, shift);
                None
            }
            WinitKey::Named(NamedKey::Enter | NamedKey::Space) => self.focused_intent(dialog),
            WinitKey::Named(NamedKey::ArrowUp) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                None
            }
            WinitKey::Named(NamedKey::ArrowDown) => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                None
            }
            WinitKey::Named(NamedKey::PageUp) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                None
            }
            WinitKey::Named(NamedKey::PageDown) => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                None
            }
            _ => None,
        }
    }

    pub fn handle_pointer<'a, W>(
        &mut self,
        dialog: &'a Dialog<A, W>,
        event: &WindowEvent,
        action_bounds: &[Rect],
    ) -> Option<&'a A> {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some((position.x as f32, position.y as f32));
                if self.pressed.is_none() {
                    self.focused = action_bounds
                        .iter()
                        .enumerate()
                        .find_map(|(index, bounds)| {
                            dialog
                                .actions
                                .get(index)
                                .filter(|button| {
                                    button.enabled
                                        && bounds.contains(position.x as f32, position.y as f32)
                                })
                                .map(|_| index)
                        })
                        .or(self.focused);
                }
                None
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.pointer?;
                self.pressed = action_bounds
                    .iter()
                    .enumerate()
                    .find_map(|(index, bounds)| {
                        dialog
                            .actions
                            .get(index)
                            .filter(|button| button.enabled && bounds.contains(x, y))
                            .map(|_| index)
                    });
                self.focused = self.pressed.or(self.focused);
                None
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                let pressed = self.pressed.take()?;
                let (x, y) = self.pointer?;
                action_bounds
                    .get(pressed)
                    .filter(|bounds| bounds.contains(x, y))?;
                dialog
                    .actions
                    .get(pressed)
                    .filter(|button| button.enabled)
                    .map(|button| &button.intent)
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(_, lines) if *lines > 0.0 => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(*lines as usize)
                    }
                    MouseScrollDelta::LineDelta(_, lines) if *lines < 0.0 => {
                        self.scroll_offset = self.scroll_offset.saturating_add((-*lines) as usize)
                    }
                    MouseScrollDelta::PixelDelta(position) if position.y > 0.0 => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(1)
                    }
                    MouseScrollDelta::PixelDelta(position) if position.y < 0.0 => {
                        self.scroll_offset = self.scroll_offset.saturating_add(1)
                    }
                    _ => {}
                }
                None
            }
            _ => None,
        }
    }

    fn advance_focus<W>(&mut self, dialog: &Dialog<A, W>, reverse: bool) {
        let enabled: Vec<usize> = dialog
            .actions
            .iter()
            .enumerate()
            .filter_map(|(index, button)| button.enabled.then_some(index))
            .collect();
        if enabled.is_empty() {
            self.focused = None;
            return;
        }
        let current = self
            .focused
            .and_then(|focus| enabled.iter().position(|index| *index == focus));
        let next = match (current, reverse) {
            (Some(index), false) => (index + 1) % enabled.len(),
            (Some(0), true) => enabled.len() - 1,
            (Some(index), true) => index - 1,
            (None, _) => 0,
        };
        self.focused = Some(enabled[next]);
    }

    fn focused_intent<'a, W>(&self, dialog: &'a Dialog<A, W>) -> Option<&'a A> {
        self.focused
            .and_then(|index| dialog.actions.get(index))
            .filter(|button| button.enabled)
            .map(|button| &button.intent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Intent {
        Confirm,
        Cancel,
    }

    fn dialog() -> Dialog<Intent> {
        Dialog::new(
            WindowSpec::fixed("Paste confirmation", 600.0, 400.0),
            Text::new("preview"),
        )
        .actions(vec![
            Button::new(Text::new("Paste"), Intent::Confirm).key(Key(1)),
            Button::new(Text::new("Cancel"), Intent::Cancel).key(Key(2)),
        ])
        .initial_focus(Key(2))
    }

    #[test]
    fn default_focus_activates_cancel() {
        let dialog = dialog();
        let mut runtime = DialogRuntime::default();
        runtime.sync(&dialog);
        assert_eq!(runtime.focused_intent(&dialog), Some(&Intent::Cancel));
    }

    #[test]
    fn reverse_focus_wraps_to_paste() {
        let dialog = dialog();
        let mut runtime = DialogRuntime::default();
        runtime.sync(&dialog);
        runtime.advance_focus(&dialog, true);
        assert_eq!(runtime.focused_intent(&dialog), Some(&Intent::Confirm));
    }

    #[test]
    fn pointer_requires_release_inside_pressed_button() {
        let dialog = dialog();
        let mut runtime = DialogRuntime::default();
        runtime.sync(&dialog);
        let bounds = [
            Rect {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 20.0,
            },
            Rect {
                x: 60.0,
                y: 0.0,
                width: 50.0,
                height: 20.0,
            },
        ];
        runtime.pointer = Some((10.0, 10.0));
        runtime.pressed = Some(0);
        runtime.pointer = Some((55.0, 10.0));
        let event = WindowEvent::MouseInput {
            device_id: unsafe { winit::event::DeviceId::dummy() },
            state: ElementState::Released,
            button: MouseButton::Left,
        };
        assert_eq!(runtime.handle_pointer(&dialog, &event, &bounds), None);
    }
}
