//! Single-line editing state. All public positions are UTF-8 byte offsets.
//! Invalid positions are clamped backwards to an extended grapheme boundary.
//! Each accepted replacement is one undo step (at most 100); programmatic
//! `set_value`/`set_text` clears history. Composition is transient, not history.

use crate::{input::event::ImePreedit, signal::Signal};
use std::{cell::RefCell, ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub extent: usize,
}

impl TextSelection {
    pub const fn collapsed(offset: usize) -> Self {
        Self {
            anchor: offset,
            extent: offset,
        }
    }

    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.extent)..self.anchor.max(self.extent)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextEditingValue {
    pub text: String,
    pub selection: TextSelection,
    /// Composition replaces the selection visually, without changing source text.
    pub composition: Option<ImePreedit>,
}

impl TextEditingValue {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            selection: TextSelection::collapsed(text.len()),
            text,
            composition: None,
        }
    }

    fn normalized(mut self) -> Self {
        self.selection.anchor = floor_boundary(&self.text, self.selection.anchor);
        self.selection.extent = floor_boundary(&self.text, self.selection.extent);
        if let Some(preedit) = &mut self.composition {
            preedit.cursor_range = preedit.cursor_range.map(|(a, b)| {
                (
                    floor_boundary(&preedit.text, a),
                    floor_boundary(&preedit.text, b),
                )
            });
            if preedit.text.is_empty() {
                self.composition = None;
            }
        }
        self
    }
}

#[derive(Default)]
struct History {
    undo: Vec<TextEditingValue>,
    redo: Vec<TextEditingValue>,
}

/// Cloneable UI-thread editing handle. Clones share state and history.
/// The acceptance predicate applies to resulting committed text, including
/// programmatic writes and undo/redo. Control characters are always rejected.
#[derive(Clone)]
pub struct TextEditingController {
    pub(crate) value: Signal<TextEditingValue>,
    history: Rc<RefCell<History>>,
    accepts: Rc<dyn Fn(&str) -> bool>,
}

impl Default for TextEditingController {
    fn default() -> Self {
        Self::new("")
    }
}

impl TextEditingController {
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_acceptance(text, |_| true)
    }

    /// Panics if the initial value is not acceptable single-line text.
    pub fn with_acceptance(
        text: impl Into<String>,
        accepts: impl Fn(&str) -> bool + 'static,
    ) -> Self {
        let text = text.into();
        assert!(
            single_line(&text) && accepts(&text),
            "invalid initial input text"
        );
        Self {
            value: Signal::new_distinct(TextEditingValue::new(text)),
            history: Rc::new(RefCell::new(History::default())),
            accepts: Rc::new(accepts),
        }
    }

    pub fn value(&self) -> TextEditingValue {
        self.value.read().clone()
    }
    pub fn text(&self) -> String {
        self.value.read().text.clone()
    }
    pub fn revision(&self) -> u64 {
        self.value.version()
    }

    pub(crate) fn same_controller(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.history, &other.history)
    }

    fn accepts(&self, text: &str) -> bool {
        single_line(text) && (self.accepts)(text)
    }

    /// Atomically replaces state, normalizes positions and clears edit history.
    /// Returns false without changing anything if committed text is rejected.
    pub fn set_value(&self, value: TextEditingValue) -> bool {
        if !self.accepts(&value.text)
            || value
                .composition
                .as_ref()
                .is_some_and(|p| !single_line(&p.text))
        {
            return false;
        }
        *self.history.borrow_mut() = History::default();
        self.value.set(value.normalized());
        true
    }

    pub fn set_text(&self, text: impl Into<String>) -> bool {
        self.set_value(TextEditingValue::new(text))
    }

    pub fn set_selection(&self, selection: TextSelection) {
        let mut value = self.value();
        value.selection = selection;
        value.composition = None;
        self.value.set(value.normalized());
    }

    pub fn selected_text(&self) -> String {
        let value = self.value.read();
        value.text[value.selection.range()].to_owned()
    }

    pub(crate) fn compose(&self, preedit: ImePreedit) {
        if !single_line(&preedit.text) {
            self.cancel_composition();
            return;
        }
        let mut value = self.value();
        value.composition = Some(preedit);
        self.value.set(value.normalized());
    }

    pub(crate) fn cancel_composition(&self) {
        let mut value = self.value();
        value.composition = None;
        self.value.set(value);
    }

    pub fn replace_selection(&self, inserted: &str) -> bool {
        self.replace_range(self.value().selection.range(), inserted)
    }

    fn replace_range(&self, range: Range<usize>, inserted: &str) -> bool {
        let old = self.value();
        let mut text = old.text.clone();
        text.replace_range(range.clone(), inserted);
        if !self.accepts(&text) {
            return false;
        }
        // An insertion can merge with following combining marks / ZWJ sequences.
        let end = ceil_boundary(&text, range.start + inserted.len());
        let next = TextEditingValue {
            text,
            selection: TextSelection::collapsed(end),
            composition: None,
        };
        if next.text != old.text {
            let mut history = self.history.borrow_mut();
            let mut snapshot = old;
            snapshot.composition = None;
            history.undo.push(snapshot);
            if history.undo.len() > 100 {
                history.undo.remove(0);
            }
            history.redo.clear();
        }
        self.value.set(next);
        true
    }

    pub fn undo(&self) -> bool {
        self.restore(false)
    }
    pub fn redo(&self) -> bool {
        self.restore(true)
    }

    fn restore(&self, redo: bool) -> bool {
        let mut history = self.history.borrow_mut();
        let History { undo, redo: redos } = &mut *history;
        let (source, destination) = if redo { (redos, undo) } else { (undo, redos) };
        if !source.last().is_some_and(|next| self.accepts(&next.text)) {
            return false;
        }
        let mut current = self.value();
        current.composition = None;
        let next = source.pop().expect("history entry was validated");
        destination.push(current);
        drop(history);
        self.value.set(next);
        true
    }

    pub(crate) fn move_to(&self, offset: usize, extend: bool) {
        let mut selection = self.value.read().selection;
        selection.extent = offset;
        if !extend {
            selection.anchor = offset;
        }
        self.set_selection(selection);
    }

    pub(crate) fn delete(&self, backward: bool) {
        let value = self.value();
        let mut range = value.selection.range();
        if range.is_empty() {
            let pos = value.selection.extent;
            let end = if backward {
                previous_boundary(&value.text, pos)
            } else {
                next_boundary(&value.text, pos)
            };
            range = pos.min(end)..pos.max(end);
        }
        self.replace_range(range, "");
    }
}

fn single_line(text: &str) -> bool {
    !text
        .chars()
        .any(|ch| ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}')
}

pub(crate) fn boundaries(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
}

pub(crate) fn floor_boundary(text: &str, offset: usize) -> usize {
    boundaries(text)
        .take_while(|&i| i <= offset)
        .last()
        .unwrap_or(0)
}

fn ceil_boundary(text: &str, offset: usize) -> usize {
    boundaries(text)
        .find(|&i| i >= offset)
        .unwrap_or(text.len())
}

pub(crate) fn previous_boundary(text: &str, offset: usize) -> usize {
    boundaries(text)
        .take_while(|&i| i < offset)
        .last()
        .unwrap_or(0)
}

pub(crate) fn next_boundary(text: &str, offset: usize) -> usize {
    boundaries(text).find(|&i| i > offset).unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_history_restores_preserve_both_stacks_and_composition() {
        use std::cell::Cell;
        let limit = Rc::new(Cell::new(10));
        let policy = limit.clone();
        let c = TextEditingController::with_acceptance("a", move |s| s.len() <= policy.get());
        c.replace_selection("b");
        c.compose(ImePreedit::new("你", None));
        let composed = c.value();
        let revision = c.revision();

        limit.set(0);
        assert!(!c.undo());
        assert_eq!(c.value(), composed);
        assert_eq!(c.revision(), revision);
        limit.set(1);
        assert!(c.undo());
        assert_eq!(c.text(), "a");
        assert!(c.value().composition.is_none());
        let undone = c.value();
        assert!(!c.redo());
        assert_eq!(c.value(), undone);
        limit.set(2);
        assert!(c.redo());
        assert_eq!(c.text(), "ab");
        assert!(c.value().composition.is_none());
    }

    #[test]
    fn unicode_selection_replacement_and_history() {
        for unit in ["e\u{301}", "你", "😀", "🇨🇳", "👨‍👩‍👧‍👦"] {
            let c = TextEditingController::new(format!("a{unit}b"));
            c.move_to(1 + unit.len(), false);
            c.delete(true);
            assert_eq!(c.text(), "ab");
            assert!(c.undo());
            assert_eq!(c.text(), format!("a{unit}b"));
            assert!(c.redo());
            assert_eq!(c.text(), "ab");
        }
    }

    #[test]
    fn arbitrary_offsets_normalize_and_insertions_can_merge_graphemes() {
        let c = TextEditingController::new("你e\u{301}");
        c.set_selection(TextSelection {
            anchor: 2,
            extent: usize::MAX,
        });
        assert_eq!(c.selected_text(), "你e\u{301}");
        c.set_text("\u{301}x");
        c.move_to(0, false);
        c.replace_selection("e");
        assert_eq!(c.value().selection.extent, "e\u{301}".len());
        c.set_text("ab");
        assert!(!c.undo());
    }

    #[test]
    fn acceptance_uses_result_not_sum_and_rejections_are_atomic() {
        let c = TextEditingController::with_acceptance("abcd", |s| s.len() <= 4);
        c.set_selection(TextSelection {
            anchor: 1,
            extent: 4,
        });
        assert!(c.replace_selection("你"));
        assert_eq!(c.text(), "a你");
        let old = c.value();
        assert!(!c.replace_selection("x"));
        assert!(!c.replace_selection("\n"));
        assert!(!c.set_text("too long"));
        assert_eq!(c.value(), old);
    }

    #[test]
    fn composition_is_transient_and_not_an_undo_step() {
        let c = TextEditingController::new("ab");
        c.move_to(1, false);
        c.compose(ImePreedit::new("你", Some((2, 99))));
        assert_eq!(c.text(), "ab");
        assert!(!c.undo());
        assert_eq!(c.value().composition.unwrap().cursor_range, Some((0, 3)));
        c.replace_selection("你");
        assert_eq!(c.text(), "a你b");
        c.undo();
        assert_eq!(c.text(), "ab");
        assert!(c.value().composition.is_none());
    }
}
