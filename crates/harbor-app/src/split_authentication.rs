//! An unpublished split has its own terminal and focus, never the active source's input.
use super::*;
use crate::tab_manager::PaneBinding;

#[derive(Clone)]
struct SplitAuthentication {
    target: PaneBinding,
    bridge: TerminalWidgetBridge,
    focus: FocusHandle,
}
impl crate::popups::Popup for SplitAuthentication {
    fn build(
        &self,
        _cx: &mut BuildCx,
        ui: &WorkspaceUiController,
        _: &ResolvedKeybindings,
        generation: u64,
    ) -> View {
        let dispatcher = ui.store.dispatcher();
        let target = self.target;
        harbor_widget::view! { FocusScope::new().modal(true) => { harbor_widget::Column::new() => {
            harbor_widget::Row::new() => {
                harbor_widget::TextLabel::new("Starting split — authenticate in this reserved session");
                harbor_widget::Button::text("Cancel").on_click(move |_| {
                    dispatcher.dispatch(AppCommandRequest::popup(AppCommand::CancelSplit(target), generation));
                });
            }
            SizedBox::height(240.0).expect("valid authentication height") => {
                Focus::new().focus_handle(self.focus) => { terminal_widget(self.bridge.clone()); }
            }
        } } }
    }
    fn focus(&self) -> FocusHandle {
        self.focus
    }
    fn close(&self) {}
    fn blocks_shortcuts(&self) -> bool {
        true
    }
}
impl WorkspaceUiController {
    pub fn show_split_authentication(
        &self,
        target: PaneBinding,
        bridge: TerminalWidgetBridge,
    ) -> FocusHandle {
        let mut state = self.store.state().read().clone();
        let focus = state.popups.open(|_| SplitAuthentication {
            target,
            bridge,
            focus: FocusHandle::new(),
        });
        self.store.set_state(state);
        focus
    }
    pub fn close_split_authentication(&self, target: PaneBinding) {
        let mut state = self.store.state().read().clone();
        if state
            .popups
            .get::<SplitAuthentication>()
            .is_some_and(|auth| auth.target == target)
        {
            let generation = state.popups.generation().unwrap();
            state.popups.close(generation);
            self.store.set_state(state);
        }
    }
}
