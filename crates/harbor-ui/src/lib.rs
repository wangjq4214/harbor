//! Declarative GPU UI primitives for Harbor.
//!
//! Widgets are immutable configuration. [`DialogRuntime`] retains only transient
//! interaction state and emits host-owned intents.

mod background;
mod decoration;
pub mod font;
pub mod metrics;
mod terminal;
mod text;

pub use font::{FontBook, load_system_fonts};
pub use metrics::TextMetrics;
pub use terminal::UiRoot;
pub use text::AtlasGlyph;

use harbor_gpu::GpuContext;
use harbor_types::RenderSnapshot;
use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key as WinitKey, NamedKey},
};

/// Every terminal renderer layer: prepare + draw (+ optional resize).
pub trait Component {
    /// Uploads dirty GPU resources. No-op when nothing changed.
    fn prepare(&mut self, gpu: &GpuContext, snap: Option<&RenderSnapshot>);
    /// Issues draw calls. Always lightweight, no GPU allocation.
    fn draw(&self, pass: &mut wgpu::RenderPass);

    /// Called when the window surface is resized.
    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {}
}

/// Shell-owned terminal overlays rendered after the UI terminal layers.
pub trait TerminalOverlays {
    fn prepare(&mut self, gpu: &GpuContext, snap: &RenderSnapshot);
    fn draw(&self, pass: &mut wgpu::RenderPass);
    fn resize(&mut self, gpu: &GpuContext, size: (u32, u32));
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Key(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxConstraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl BoxConstraints {
    pub const fn tight(width: f32, height: f32) -> Self {
        Self {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl EdgeInsets {
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color(pub [f32; 4]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub size: f32,
    pub line_height: f32,
    pub bold: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: Color([1.0, 1.0, 1.0, 1.0]),
            size: 14.0,
            line_height: 20.0,
            bold: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub content: String,
    pub style: TextStyle,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            style: TextStyle::default(),
        }
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Container<W> {
    pub child: W,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub padding: EdgeInsets,
    pub margin: EdgeInsets,
    pub alignment: Alignment,
    pub background: Option<Color>,
    pub corner_radius: f32,
}

impl<W> Container<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            width: None,
            height: None,
            padding: EdgeInsets::default(),
            margin: EdgeInsets::default(),
            alignment: Alignment::Start,
            background: None,
            corner_radius: 0.0,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
    pub fn padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }
    pub fn margin(mut self, margin: EdgeInsets) -> Self {
        self.margin = margin;
        self
    }
    pub fn align(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x <= self.x + self.width && y <= self.y + self.height
    }
}

pub struct PaintContext<'a> {
    pub gpu: &'a GpuContext,
    pub bounds: Rect,
}

/// A custom GPU painter. The UI runtime owns surface acquisition and presentation.
pub trait CustomPainter {
    fn paint<'pass>(&mut self, context: PaintContext<'_>, pass: &mut wgpu::RenderPass<'pass>);
}

pub struct CustomPaint<P> {
    pub painter: P,
    pub key: Option<Key>,
}

impl<P> CustomPaint<P> {
    pub fn new(painter: P) -> Self {
        Self { painter, key: None }
    }
    pub fn key(mut self, key: Key) -> Self {
        self.key = Some(key);
        self
    }
}

/// Special UI component whose painter renders a terminal viewport.
///
/// The host owns the terminal session and applies returned resize intents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terminal {
    pub key: Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalIntent {
    Resize(harbor_types::TerminalSize),
}

impl Terminal {
    pub const fn new(key: Key) -> Self {
        Self { key }
    }

    pub fn resize_intent(self, bounds: Rect, cell_width: f32, line_height: f32) -> TerminalIntent {
        let cols = (bounds.width / cell_width).floor().max(1.0) as usize;
        let rows = (bounds.height / line_height).floor().max(1.0) as usize;
        TerminalIntent::Resize(harbor_types::TerminalSize { rows, cols })
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

    #[test]
    fn terminal_resize_intent_uses_assigned_bounds() {
        assert_eq!(
            Terminal::new(Key(9)).resize_intent(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 805.0,
                    height: 401.0
                },
                8.0,
                20.0,
            ),
            TerminalIntent::Resize(harbor_types::TerminalSize {
                rows: 20,
                cols: 100
            }),
        );
    }
}
