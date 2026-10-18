//! Decoration-free, single-line desktop editing. See `TextField` for decoration.
use crate::{
    BoxDecoration, ClipBehavior, TextEditingController, TextSelection,
    effects::{CursorShape, ImeEffect},
    input::{
        event::{FocusEvent, Key, KeyboardEvent, Modifiers, PointerButton, PointerPhase, UiEvent},
        event_ctx::{EventCtx, EventHandled},
    },
    layout::{BoxConstraints, Point, Rect, Size},
    scene::primitive::{Color, Primitive},
    text::TextMetrics,
    text_editing::{boundaries, next_boundary, previous_boundary},
    view::{AnyView, BuildCx, Component, FocusMetadata, View},
    widgets::{FocusHandle, decorated_box::DecoratedBox},
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

type Changed = Rc<dyn Fn(&str)>;
type Submitted = Rc<dyn Fn(Modifiers)>;

/// Controller-driven editor. User text changes invoke `on_changed`; selection,
/// composition and programmatic controller writes do not invoke it.
#[derive(Clone)]
pub struct EditableText {
    controller: TextEditingController,
    enabled: bool,
    read_only: bool,
    focus: Option<FocusHandle>,
    color: Color,
    placeholder: String,
    changed: Option<Changed>,
    submitted: Option<Submitted>,
    escaped: Option<Rc<dyn Fn()>>,
    vertical_navigation: Option<Rc<dyn Fn(bool)>>,
}

impl EditableText {
    pub fn new(controller: TextEditingController) -> Self {
        Self {
            controller,
            enabled: true,
            read_only: false,
            focus: None,
            color: Color::WHITE,
            placeholder: String::new(),
            changed: None,
            submitted: None,
            escaped: None,
            vertical_navigation: None,
        }
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.focus = Some(handle);
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }
    pub fn on_changed(mut self, callback: impl Fn(&str) + 'static) -> Self {
        self.changed = Some(Rc::new(callback));
        self
    }
    pub fn on_submitted(mut self, callback: impl Fn(Modifiers) + 'static) -> Self {
        self.submitted = Some(Rc::new(callback));
        self
    }
    pub fn on_escape(mut self, callback: impl Fn() + 'static) -> Self {
        self.escaped = Some(Rc::new(callback));
        self
    }
    /// Observe unmodified vertical navigation outside IME composition.
    /// `true` means down. Selection policy belongs to the consumer.
    pub fn on_vertical_navigation(mut self, callback: impl Fn(bool) + 'static) -> Self {
        self.vertical_navigation = Some(Rc::new(callback));
        self
    }
}

#[derive(Clone, Copy)]
struct PendingClipboard {
    id: u64,
    revision: u64,
    cut: bool,
}

struct EditingRuntime {
    controller: TextEditingController,
    focused: bool,
    pointer: Option<u64>,
    clipboard: Option<PendingClipboard>,
    scroll: f32,
    positions: Vec<(usize, f32)>,
    display: String,
    caret: f32,
    caret_visible: bool,
    selection: Option<(f32, f32)>,
    preedit: Option<(f32, f32)>,
    line_height: f32,
}

impl EditingRuntime {
    fn new(controller: TextEditingController) -> Self {
        Self {
            controller,
            focused: false,
            pointer: None,
            clipboard: None,
            scroll: 0.0,
            positions: vec![(0, 0.0)],
            display: String::new(),
            caret: 0.0,
            caret_visible: true,
            selection: None,
            preedit: None,
            line_height: 20.0,
        }
    }

    fn hit(&self, x: f32) -> usize {
        let x = x + self.scroll;
        self.positions
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map_or(0, |p| p.0)
    }

    fn reveal(&mut self, width: f32) {
        let available = (width - 2.0).max(0.0);
        if self.caret < self.scroll {
            self.scroll = self.caret;
        }
        if self.caret > self.scroll + available {
            self.scroll = self.caret - available;
        }
        self.scroll = self.scroll.max(0.0);
    }
}

impl Component for EditableText {
    fn build(&self, cx: &mut BuildCx) -> View {
        cx.track(&self.controller.value);
        let state =
            cx.use_state(|| Rc::new(RefCell::new(EditingRuntime::new(self.controller.clone()))));
        let runtime = state.read().clone();
        {
            let mut state = runtime.borrow_mut();
            if !state.controller.same_controller(&self.controller) {
                state.controller.cancel_composition();
                let focused = state.focused;
                *state = EditingRuntime::new(self.controller.clone());
                state.focused = focused;
            }
            if !self.enabled || self.read_only {
                state.clipboard = None;
                self.controller.cancel_composition();
            }
            if !self.enabled {
                state.focused = false;
                state.pointer = None;
            }
        }
        let leaf = View::new(
            EditorView {
                config: self.clone(),
                runtime,
            },
            vec![],
            None,
        );
        DecoratedBox::new(BoxDecoration::new())
            .clip_behavior(ClipBehavior::HardEdge)
            .child(leaf)
            .build(cx)
    }
}

impl crate::WithChildren for EditableText {
    fn with_children(
        self,
        children: crate::Children,
    ) -> Result<Self, crate::ChildConstructionError> {
        children.ensure_leaf("EditableText")?;
        Ok(self)
    }
}

struct EditorView {
    config: EditableText,
    runtime: Rc<RefCell<EditingRuntime>>,
}

impl EditorView {
    fn changed(&self, old: &str) {
        let text = self.config.controller.text();
        if text != old
            && let Some(callback) = &self.config.changed
        {
            callback(&text);
        }
    }

    fn clipboard(&self, ctx: &mut EventCtx, read: bool, cut: bool) {
        let c = &self.config.controller;
        if !read && c.selected_text().is_empty() {
            return;
        }
        let id = ctx.editor_clipboard(if read { None } else { Some(c.selected_text()) });
        self.runtime.borrow_mut().clipboard = Some(PendingClipboard {
            id,
            revision: c.revision(),
            cut,
        });
    }

    fn key(&self, key: Key, modifiers: Modifiers, ctx: &mut EventCtx) -> bool {
        let c = &self.config.controller;
        let value = c.value();
        if value.composition.is_some() {
            if matches!(key, Key::Enter | Key::NumpadEnter) {
                ctx.consume_key_press(key, false);
            }
            // Native IME owns editing/confirmation during composition.
            if key == Key::Escape {
                ctx.consume_key_press(key, false);
                c.cancel_composition();
            }
            return true;
        }
        let writable = !self.config.read_only;
        if modifiers.ctrl && !modifiers.alt && !modifiers.meta {
            match key {
                Key::Character(ch) => match ch.to_ascii_lowercase() {
                    'a' => c.set_selection(TextSelection {
                        anchor: 0,
                        extent: value.text.len(),
                    }),
                    'c' => self.clipboard(ctx, false, false),
                    'x' if writable => self.clipboard(ctx, false, true),
                    'v' if writable => self.clipboard(ctx, true, false),
                    'z' if writable => {
                        if modifiers.shift {
                            c.redo();
                        } else {
                            c.undo();
                        }
                    }
                    'y' if writable => {
                        c.redo();
                    }
                    'x' | 'v' | 'z' | 'y' => {}
                    _ => return false,
                },
                _ => return false,
            }
            return true;
        }
        if modifiers.ctrl || modifiers.alt || modifiers.meta {
            return false;
        }
        match key {
            Key::ArrowUp | Key::ArrowDown => {
                if !modifiers.shift
                    && let Some(callback) = &self.config.vertical_navigation
                {
                    callback(key == Key::ArrowDown);
                }
            }
            Key::ArrowLeft | Key::ArrowRight => {
                let range = value.selection.range();
                let offset = if !modifiers.shift && !range.is_empty() {
                    if key == Key::ArrowLeft {
                        range.start
                    } else {
                        range.end
                    }
                } else if key == Key::ArrowLeft {
                    previous_boundary(&value.text, value.selection.extent)
                } else {
                    next_boundary(&value.text, value.selection.extent)
                };
                c.move_to(offset, modifiers.shift);
            }
            Key::Home => c.move_to(0, modifiers.shift),
            Key::End => c.move_to(value.text.len(), modifiers.shift),
            Key::Backspace if writable => c.delete(true),
            Key::Delete if writable => c.delete(false),
            Key::Enter | Key::NumpadEnter => {
                if let Some(callback) = &self.config.submitted {
                    ctx.consume_key_press(key, true);
                    callback(modifiers);
                }
            }
            Key::Escape => {
                if let Some(callback) = &self.config.escaped {
                    ctx.consume_key_press(key, true);
                    callback();
                }
            }
            Key::Character(ch) | Key::NumpadCharacter(ch) if writable => {
                c.replace_selection(&ch.to_string());
            }
            Key::Space if writable => {
                c.replace_selection(" ");
            }
            Key::Tab => return false,
            _ => {}
        }
        true
    }
}

impl AnyView for EditorView {
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        constraints.constrain(Size::new(280.0, metrics.line_height))
    }

    fn focus_metadata(&self) -> Option<FocusMetadata> {
        Some(FocusMetadata {
            enabled: self.config.enabled,
            order: 0,
            participates_in_tab_traversal: true,
            handle: self.config.focus.map(|f| f.0),
        })
    }
    fn pointer_cursor(&self) -> Option<CursorShape> {
        self.config.enabled.then_some(CursorShape::Text)
    }
    fn permits_pointer_capture(&self) -> bool {
        self.config.enabled
    }

    fn owns_editing_key(&self, key: Key, modifiers: Modifiers) -> bool {
        self.config.enabled
            && (self.config.controller.value().composition.is_some()
                || (modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.meta
                    && matches!(key, Key::Character(ch) if matches!(ch.to_ascii_lowercase(), 'a' | 'c' | 'x' | 'v' | 'z' | 'y'))))
    }

    fn prepare_text_geometry(&self, metrics: &TextMetrics, advance: &mut dyn FnMut(char) -> f32) {
        let value = self.config.controller.value();
        let range = value.selection.range();
        let mut display = value.text.clone();
        if let Some(preedit) = &value.composition {
            display.replace_range(range.clone(), &preedit.text);
        }
        let mut widths = vec![(0, 0.0)];
        let mut x = 0.0;
        for (offset, ch) in display.char_indices() {
            x += advance(ch);
            widths.push((offset + ch.len_utf8(), x));
        }
        let at = |offset| {
            widths
                .binary_search_by_key(&offset, |p| p.0)
                .ok()
                .map_or(x, |index| widths[index].1)
        };
        let source_x = |offset| {
            let mapped = if let Some(preedit) = &value.composition {
                if offset <= range.start {
                    offset
                } else if offset < range.end {
                    range.start
                } else {
                    offset - range.len() + preedit.text.len()
                }
            } else {
                offset
            };
            at(mapped)
        };
        let mut runtime = self.runtime.borrow_mut();
        runtime.positions = boundaries(&value.text)
            .map(|offset| (offset, source_x(offset)))
            .collect();
        runtime.caret = if let Some(preedit) = &value.composition {
            at(range.start
                + preedit
                    .cursor_range
                    .map_or(preedit.text.len(), |(_, end)| end))
        } else {
            source_x(value.selection.extent)
        };
        runtime.caret_visible = value
            .composition
            .as_ref()
            .is_none_or(|preedit| preedit.cursor_range.is_some());
        runtime.selection = if let Some(preedit) = &value.composition {
            preedit
                .cursor_range
                .filter(|(a, b)| a != b)
                .map(|(a, b)| (at(range.start + a.min(b)), at(range.start + a.max(b))))
        } else if !range.is_empty() {
            Some((source_x(range.start), source_x(range.end)))
        } else {
            None
        };
        runtime.preedit = value
            .composition
            .as_ref()
            .map(|p| (at(range.start), at(range.start + p.text.len())));
        runtime.display = display;
        runtime.line_height = metrics.line_height;
    }

    fn focused_ime(&self, rect: Rect) -> Option<ImeEffect> {
        let mut runtime = self.runtime.borrow_mut();
        runtime.reveal(rect.size().width);
        Some(ImeEffect {
            allowed: Some(self.config.enabled && !self.config.read_only && runtime.focused),
            position: Some(Point::new(
                rect.min.x + runtime.caret - runtime.scroll,
                rect.min.y + runtime.line_height,
            )),
        })
    }

    fn paint_primitives(&self, rect: Rect, metrics: &TextMetrics) -> Vec<Primitive> {
        let mut runtime = self.runtime.borrow_mut();
        runtime.reveal(rect.size().width);
        let origin = Point::new(rect.min.x - runtime.scroll, rect.min.y);
        let mut out = Vec::new();
        let quad = |left: f32, right: f32, y: f32, height: f32, color| Primitive::Quad {
            rect: Rect::from_min_size(
                Point::new(origin.x + left, origin.y + y),
                Size::new((right - left).max(0.0), height),
            ),
            color,
            corner_radius: 0.0,
        };
        if let Some((left, right)) = runtime.selection {
            out.push(quad(
                left,
                right,
                0.0,
                metrics.line_height,
                Color {
                    r: 0.2,
                    g: 0.4,
                    b: 0.8,
                    a: 0.5,
                },
            ));
        }
        let empty = runtime.display.is_empty();
        out.push(Primitive::Text {
            text: Arc::from(if empty {
                self.config.placeholder.as_str()
            } else {
                &runtime.display
            }),
            origin,
            color: if empty {
                Color {
                    r: 0.5,
                    g: 0.5,
                    b: 0.5,
                    a: 1.0,
                }
            } else {
                self.config.color
            },
        });
        if let Some((left, right)) = runtime.preedit {
            out.push(quad(
                left,
                right,
                metrics.line_height - 1.0,
                1.0,
                self.config.color,
            ));
        }
        if runtime.focused && runtime.caret_visible && self.config.enabled {
            out.push(quad(
                runtime.caret,
                runtime.caret + 1.0,
                0.0,
                metrics.line_height,
                self.config.color,
            ));
        }
        out
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, rect: Rect) -> EventHandled {
        if !self.config.enabled || ctx.is_capture_phase() {
            return EventHandled::Ignored;
        }
        let c = &self.config.controller;
        let old = c.text();
        let handled = match event {
            UiEvent::Focus(focus) => {
                let focused = !matches!(focus, FocusEvent::Lost);
                self.runtime.borrow_mut().focused = focused;
                if !focused {
                    let mut runtime = self.runtime.borrow_mut();
                    if let Some(pointer) = runtime.pointer.take() {
                        ctx.release_pointer(pointer);
                    }
                    runtime.clipboard = None;
                    c.cancel_composition();
                }
                true
            }
            UiEvent::Pointer(pointer) if pointer.button == PointerButton::Left => {
                match pointer.phase {
                    PointerPhase::Down => {
                        ctx.request_focus_from_pointer();
                        ctx.capture_pointer(pointer.pointer_id);
                        let mut runtime = self.runtime.borrow_mut();
                        runtime.pointer = Some(pointer.pointer_id);
                        c.move_to(
                            runtime.hit(pointer.position.x - rect.min.x),
                            pointer.modifiers.shift,
                        );
                    }
                    PointerPhase::Move
                        if self.runtime.borrow().pointer == Some(pointer.pointer_id) =>
                    {
                        let offset = self.runtime.borrow().hit(pointer.position.x - rect.min.x);
                        c.move_to(offset, true);
                    }
                    PointerPhase::Up | PointerPhase::Cancel => {
                        let mut runtime = self.runtime.borrow_mut();
                        if runtime.pointer == Some(pointer.pointer_id) {
                            runtime.pointer = None;
                            ctx.release_pointer(pointer.pointer_id);
                        }
                    }
                    _ => return EventHandled::Ignored,
                }
                true
            }
            UiEvent::Keyboard(KeyboardEvent::KeyUp { .. }) => true,
            UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }) => {
                self.key(*key, *modifiers, ctx)
            }
            UiEvent::Keyboard(KeyboardEvent::Ime(text)) => {
                if !self.config.read_only {
                    c.replace_selection(text);
                    c.cancel_composition();
                }
                true
            }
            UiEvent::ImePreedit(preedit) => {
                if !self.config.read_only {
                    c.compose(preedit.clone());
                }
                true
            }
            UiEvent::Clipboard { request, result } => {
                let pending = self.runtime.borrow_mut().clipboard.take();
                if let Some(pending) = pending
                    && pending.id == *request
                    && pending.revision == c.revision()
                    && self.runtime.borrow().focused
                    && !self.config.read_only
                    && let Ok(text) = result
                {
                    if pending.cut {
                        c.replace_selection("");
                    } else if !text.is_empty() {
                        c.replace_selection(text);
                    }
                }
                true
            }
            _ => false,
        };
        if !handled {
            return EventHandled::Ignored;
        }
        self.changed(&old);
        ctx.invalidate_paint();
        ctx.stop_propagation();
        EventHandled::Handled
    }

    fn unmount(&self) {
        let mut runtime = self.runtime.borrow_mut();
        runtime.focused = false;
        runtime.pointer = None;
        runtime.clipboard = None;
        self.config.controller.cancel_composition();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_follow_variable_render_advances_and_preedit_selection() {
        let controller = TextEditingController::new("Wi你");
        controller.set_selection(TextSelection {
            anchor: 1,
            extent: 2,
        });
        let runtime = Rc::new(RefCell::new(EditingRuntime::new(controller.clone())));
        let view = EditorView {
            config: EditableText::new(controller.clone()),
            runtime: runtime.clone(),
        };
        let metrics = crate::runtime::DEFAULT_TEXT_METRICS;
        let mut measure = |ch| match ch {
            'W' => 20.0,
            'i' => 4.0,
            '你' => 18.0,
            _ => 7.0,
        };
        view.prepare_text_geometry(&metrics, &mut measure);
        assert_eq!(
            runtime.borrow().positions,
            [(0, 0.0), (1, 20.0), (2, 24.0), (5, 42.0)]
        );
        assert_eq!(runtime.borrow().selection, Some((20.0, 24.0)));
        assert_eq!(runtime.borrow().hit(23.0), 2);
        controller.compose(crate::input::event::ImePreedit::new("ni", Some((1, 2))));
        view.prepare_text_geometry(&metrics, &mut measure);
        assert_eq!(runtime.borrow().display, "Wni你");
        assert_eq!(runtime.borrow().caret, 31.0);
        assert_eq!(runtime.borrow().preedit, Some((20.0, 31.0)));
        assert_eq!(runtime.borrow().selection, Some((27.0, 31.0)));
        controller.compose(crate::input::event::ImePreedit::new("draft", None));
        view.prepare_text_geometry(&metrics, &mut measure);
        assert!(!runtime.borrow().caret_visible);
    }
}
