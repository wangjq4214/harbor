//! Common slot extension and editor-lifetime regressions through the mounted workspace.
use super::palette_support::*;
use super::*;
use crate::popups::Popup;
use std::{cell::Cell, rc::Rc};

#[derive(Clone)]
struct ProbePopup {
    input: TextEditingController,
    focus: FocusHandle,
    closes: Rc<Cell<usize>>,
}
impl Popup for ProbePopup {
    fn build(
        &self,
        cx: &mut BuildCx,
        _: &WorkspaceUiController,
        _: &ResolvedKeybindings,
        generation: u64,
    ) -> View {
        Portal::new()
            .child(TextField::new(self.input.clone()).focus_handle(self.focus))
            .keyed(format!("test-popup-{generation}"))
            .build(cx)
    }
    fn focus(&self) -> FocusHandle {
        self.focus
    }
    fn close(&self) {
        self.closes.set(self.closes.get() + 1);
        self.input.set_text("");
    }
}
fn probe(ui: &WorkspaceUiController, runtime: &mut Runtime, closes: Rc<Cell<usize>>) -> u64 {
    let mut state = ui.store.state().read().clone();
    let focus = state.popups.open(|_| ProbePopup {
        input: TextEditingController::new("third popup"),
        focus: FocusHandle::new(),
        closes,
    });
    let generation = state.popups.generation().unwrap();
    ui.store.set_state(state);
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    generation
}

#[test]
fn third_popup_uses_the_same_slot_cleanup_focus_and_notification_contract() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    let allocation = ui.latest_terminal_allocation();
    let closes = Rc::new(Cell::new(0));
    for iteration in 0..3 {
        let generation = probe(&ui, &mut runtime, closes.clone());
        assert!(!ui.palette_is_open());
        assert!(ui.search_query().is_none());
        let snapshot = ui.store.state().read().clone();
        let editor = snapshot.popups.get::<ProbePopup>().unwrap().input.clone();
        replace(&mut runtime, "focused third editor");
        assert_eq!(editor.text(), "focused third editor");
        let focus = runtime.input().focused();
        ui.report_configuration_result(format!("test notification {iteration}"));
        runtime.update(Instant::now());
        assert_eq!(runtime.input().focused(), focus);
        assert!(painted(&runtime).contains("test notification"));
        let deadline = ui.store.state().read().notices.deadline().unwrap();
        ui.advance_notifications(deadline);
        runtime.update(Instant::now());
        assert!(ui.popup_is_open());
        assert_eq!(runtime.input().focused(), focus);
        ui.open_search(tabs.active_id().unwrap());
        runtime.request_focus(&ui.search_focus());
        runtime.update(Instant::now());
        assert_eq!(editor.text(), "");
        assert!(!ui.store.state().read().popups.is_current(generation));
        let retired = closes.get();
        drop(snapshot);
        assert_eq!(
            closes.get(),
            retired,
            "dropping a publication snapshot is not another retirement"
        );
        open(&ui, &mut runtime);
        assert!(ui.search_query().is_none());
        assert_eq!(ui.latest_terminal_allocation(), allocation);
        probe(&ui, &mut runtime, closes.clone());
        assert!(!ui.palette_is_open());
        ui.open_palette();
    }
    assert_eq!(closes.get(), 6);
    let generation = ui.store.state().read().popups.generation().unwrap();
    assert!(ui.close_palette(generation));
    runtime.request_focus(&ui.terminal_focus());
    runtime.update(Instant::now());
    assert!(!ui.popup_is_open(), "no suspended popup is restored");
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn replacement_and_same_kind_reopen_reject_clipboard_ime_and_old_close() {
    for search_first in [false, true] {
        let (tabs, ui, mut runtime, bytes) = fixture();
        if search_first {
            ui.open_search(tabs.active_id().unwrap());
            runtime.request_focus(&ui.search_focus());
            runtime.update(Instant::now());
        } else {
            open(&ui, &mut runtime);
        }
        replace(&mut runtime, "old query");
        ui.drain_actions();
        let old_generation = ui.store.state().read().popups.generation().unwrap();
        let effects = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('v'),
            modifiers: ctrl(),
        }));
        let Some(ClipboardEffect::Editor(request)) = effects.clipboard else {
            panic!("paste request");
        };
        up(&mut runtime, Key::Character('v'), ctrl());
        runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
        // No reconciliation between these transitions: even same-kind reopening
        // must create a new editor lifetime, not reuse the retired Fiber.
        if search_first {
            ui.open_palette();
            ui.open_search(tabs.active_id().unwrap());
            runtime.request_focus(&ui.search_focus());
        } else {
            ui.open_search(tabs.active_id().unwrap());
            ui.open_palette();
            runtime.request_focus(&ui.palette_focus);
        }
        runtime.update(Instant::now());
        runtime.complete_clipboard(request.id, Ok("late paste".into()));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("late IME".into())));
        assert!(!ui.close_palette(old_generation));
        assert!(ui.popup_is_open());
        let current = if search_first {
            ui.search_query().unwrap().1
        } else {
            input(&ui).text()
        };
        assert_eq!(
            current, "",
            "retired composition/clipboard must not reach a new lifetime"
        );
        assert!(ui.drain_actions().is_empty());
        assert!(bytes.lock().unwrap().is_empty());
    }
}
