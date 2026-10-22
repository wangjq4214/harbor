//! Application popup lifecycle: a single slot, not a catalog of panel kinds.
use crate::{command::ResolvedKeybindings, workspace::WorkspaceUiController};
use harbor_widget::{
    FocusHandle,
    view::{BuildCx, View},
};
use std::{any::Any, rc::Rc};

/// Application consumers supply their own content, focus and retirement behavior.
/// No terminal/session/command effects are executed by the slot itself.
pub(crate) trait Popup: Any {
    fn build(
        &self,
        cx: &mut BuildCx,
        ui: &WorkspaceUiController,
        bindings: &ResolvedKeybindings,
        generation: u64,
    ) -> View;
    fn focus(&self) -> FocusHandle;
    fn close(&self);
    fn blocks_shortcuts(&self) -> bool {
        false
    }
    fn notification_inset(&self) -> f32 {
        0.0
    }
}

#[derive(Clone)]
struct ActivePopup {
    generation: u64,
    content: Rc<dyn Popup>,
}

/// Clones are publication snapshots, not independently owned popup lifetimes.
/// Only explicit close/replacement retires a lifetime, never dropping a snapshot.
#[derive(Clone, Default)]
pub(crate) struct PopupSlot {
    active: Option<ActivePopup>,
    generation: u64,
}

impl PopupSlot {
    pub fn get<T: Popup>(&self) -> Option<&T> {
        let content: &dyn Any = self.active.as_ref()?.content.as_ref();
        content.downcast_ref()
    }

    pub fn generation(&self) -> Option<u64> {
        self.active.as_ref().map(|active| active.generation)
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation() == Some(generation)
    }

    pub fn active(&self) -> Option<&dyn Popup> {
        self.active.as_ref().map(|active| active.content.as_ref())
    }

    pub fn open<T: Popup>(&mut self, make: impl FnOnce(u64) -> T) -> FocusHandle {
        if let Some(old) = self.active.take() {
            old.content.close();
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("popup lifetime exhausted");
        let content = Rc::new(make(self.generation));
        let focus = content.focus();
        self.active = Some(ActivePopup {
            generation: self.generation,
            content,
        });
        focus
    }

    pub fn close(&mut self, generation: u64) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        self.active.take().unwrap().content.close();
        true
    }

    /// Publish a content edit without replacing or retiring its lifetime.
    pub fn update<T: Popup + Clone>(&mut self, edit: impl FnOnce(&mut T)) {
        let Some(mut content) = self.get::<T>().cloned() else {
            return;
        };
        edit(&mut content);
        self.active.as_mut().unwrap().content = Rc::new(content);
    }
}
