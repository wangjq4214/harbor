//! Application workspace composition and Host-published global UI state.

use crate::{
    appearance::{GlassAppearance, glass_icon_button_style},
    command::{AppCommand, AppCommandRequest, ResolvedKeybindings},
    notifications::{ConfigurationNoticeSource, ConfigurationNotices, configuration_notification},
    tab_manager::TabSnapshot,
    tab_view::{TabFocusPolicy, TabUiController, ui::tab_rail},
    terminal_view::{TerminalDecorationPreset, TerminalWidgetBridge, terminal_widget},
};
use harbor_widget::{
    ActionHandler, ActionOutcome, Align, Alignment2D, ComponentExt as _, CrossAxisAlignment,
    Expanded, Focus, FocusHandle, FocusScope, Insets, LayoutObserver, LucideIcon, Portal, Row,
    ShortcutMap, SizedBox, Store, TextEditingController, TextField,
    layout::Rect,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{layout_observer::LayoutChangedCallback, padding::Padding, stack::Stack},
};
mod palette;
pub(crate) mod quick_save;
mod search_bar;
mod split_authentication;
#[cfg(test)]
use search_bar::{SEARCH_PANEL_HEIGHT, SEARCH_PANEL_INSET, search_bar};
use search_bar::{SearchUiState, popup_icon_button};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct WorkspaceUiState {
    keybindings: Option<ResolvedKeybindings>,
    profiles: std::collections::BTreeMap<harbor_config::ProfileId, harbor_config::SessionProfile>,
    profiles_revision: u64,
    notices: ConfigurationNotices,
    popups: crate::popups::PopupSlot,
}

/// App-wide publication and the sole one-shot action inbox, independent of tab sync.
#[derive(Clone)]
pub struct WorkspaceUiController {
    store: Store<WorkspaceUiState, AppCommandRequest>,
    tabs: TabUiController,
    terminal_focus: FocusHandle,
    pane_focus: std::rc::Rc<
        std::cell::RefCell<std::collections::HashMap<crate::tab_manager::SessionId, FocusHandle>>,
    >,
    search_focus: FocusHandle,
    palette_focus: FocusHandle,
    allocation: TerminalAllocationMailbox,
    pub(crate) pane_allocations: crate::pane_layout::PaneAllocations,
}

impl WorkspaceUiController {
    pub fn new(
        snapshots: Vec<TabSnapshot>,
        active_bridge: TerminalWidgetBridge,
        logical_width: f64,
    ) -> Self {
        Self {
            store: Store::new(WorkspaceUiState::default()),
            tabs: TabUiController::new(snapshots, active_bridge, logical_width),
            terminal_focus: FocusHandle::new(),
            pane_focus: Default::default(),
            search_focus: FocusHandle::new(),
            palette_focus: FocusHandle::new(),
            allocation: TerminalAllocationMailbox::new(),
            pane_allocations: Default::default(),
        }
    }

    pub fn open_search(&self, source: crate::tab_manager::SessionId) {
        self.update_state(|state| {
            if state
                .popups
                .get::<SearchUiState>()
                .is_some_and(|search| search.source == source)
            {
                return false;
            }
            state.popups.open(|_| SearchUiState {
                source,
                input: TextEditingController::with_acceptance("", |text| {
                    text.len() <= harbor_terminal::literal_search::MAX_QUERY_BYTES
                }),
                status: "No Result".into(),
                focus: self.search_focus,
                cancellation: Default::default(),
            });
            true
        });
    }

    pub fn close_search(&self) {
        self.update_state(|state| state.popups.close_active_if::<SearchUiState>());
    }

    pub fn search_focus(&self) -> FocusHandle {
        self.search_focus
    }

    pub fn search_query(&self) -> Option<(crate::tab_manager::SessionId, String)> {
        let state = self.store.state().read();
        let search = state.popups.get::<SearchUiState>()?;
        Some((search.source, search.input.text()))
    }

    pub(crate) fn bind_search_cancellation(&self, cancellation: crate::search::SearchCancellation) {
        if let Some(search) = self.store.state().read().popups.get::<SearchUiState>() {
            *search.cancellation.borrow_mut() = Some(cancellation);
        }
    }

    pub fn sync_search_source(&self, active: Option<crate::tab_manager::SessionId>) {
        if self
            .search_query()
            .is_some_and(|(source, _)| Some(source) != active)
        {
            self.close_search();
        }
    }

    pub fn publish_search_status(&self, status: &str) {
        self.update_state(|state| {
            if state
                .popups
                .get::<SearchUiState>()
                .is_none_or(|search| search.status == status)
            {
                return false;
            }
            state
                .popups
                .update::<SearchUiState>(|search| search.status = status.into());
            true
        });
    }

    /// Scoped editor/button intents may only act on the still-live popup lifetime.
    pub fn popup_request_is_current(&self, request: AppCommandRequest) -> bool {
        match request.source {
            crate::command::CommandSource::Popup { generation } => {
                self.store.state().read().popups.is_current(generation)
            }
            _ => true,
        }
    }

    pub fn popup_is_open(&self) -> bool {
        self.store.state().read().popups.active().is_some()
    }

    pub(crate) fn dispatcher(&self) -> harbor_widget::Dispatcher<AppCommandRequest> {
        self.store.dispatcher()
    }

    pub fn open_confirmation(
        &self,
        request: crate::confirmation::ConfirmationRequest,
    ) -> (u64, FocusHandle) {
        let mut opened = None;
        self.update_state(|state| {
            let focus = state
                .popups
                .open(|_| crate::confirmation::ConfirmationOverlayState::new(request));
            let generation = state.popups.generation().expect("opened confirmation");
            opened = Some((generation, focus));
            true
        });
        opened.expect("confirmation opened")
    }

    pub fn confirmation_is_open(&self) -> bool {
        self.store
            .state()
            .read()
            .popups
            .get::<crate::confirmation::ConfirmationOverlayState>()
            .is_some()
    }

    pub fn confirmation_kind(&self) -> Option<crate::confirmation::ConfirmationKind> {
        self.store
            .state()
            .read()
            .popups
            .get::<crate::confirmation::ConfirmationOverlayState>()
            .map(crate::confirmation::ConfirmationOverlayState::kind)
    }

    pub fn confirmation_generation(&self) -> Option<u64> {
        let state = self.store.state().read();
        state
            .popups
            .get::<crate::confirmation::ConfirmationOverlayState>()?;
        state.popups.generation()
    }

    pub fn is_confirmation_current(&self, generation: u64) -> bool {
        let state = self.store.state().read();
        state.popups.is_current(generation)
            && state
                .popups
                .get::<crate::confirmation::ConfirmationOverlayState>()
                .is_some()
    }

    pub fn close_confirmation(&self, generation: u64) -> bool {
        self.update_state(|state| {
            state
                .popups
                .close_if::<crate::confirmation::ConfirmationOverlayState>(generation)
        })
    }

    pub fn close_active_confirmation(&self) -> bool {
        let Some(generation) = self.confirmation_generation() else {
            return false;
        };
        self.close_confirmation(generation)
    }

    #[cfg(test)]
    pub(crate) fn store_confirmation_scroll_for_test(
        &self,
    ) -> Option<harbor_widget::ScrollController> {
        self.store
            .state()
            .read()
            .popups
            .get::<crate::confirmation::ConfirmationOverlayState>()
            .map(crate::confirmation::ConfirmationOverlayState::scroll_controller)
    }

    pub fn tabs(&self) -> &TabUiController {
        &self.tabs
    }
    pub fn drain_actions(&self) -> Vec<AppCommandRequest> {
        self.store.drain_actions()
    }
    pub fn terminal_focus(&self) -> FocusHandle {
        self.tabs
            .state
            .read()
            .snapshots
            .iter()
            .find(|tab| tab.active)
            .map_or(self.terminal_focus, |tab| self.pane_focus(tab.binding))
    }
    pub fn pane_focus(&self, binding: crate::tab_manager::PaneBinding) -> FocusHandle {
        *self
            .pane_focus
            .borrow_mut()
            .entry(binding.session)
            .or_default()
    }
    pub(crate) fn pane_focused(&self, binding: crate::tab_manager::PaneBinding) {
        if self.tabs.state.read().snapshots.iter().any(|tab| {
            tab.active && tab.binding.workspace == binding.workspace && tab.binding != binding
        }) {
            self.store
                .dispatcher()
                .dispatch(AppCommandRequest::shortcut(AppCommand::Pane(
                    crate::command::PaneCommand::Focus(binding),
                )));
        }
    }
    /// Publish model visibility and registration identity before mounting its projection.
    pub fn sync_panes(&self, tabs: &crate::tab_manager::TabManager, logical_width: f64) {
        self.pane_focus
            .borrow_mut()
            .retain(|session, _| tabs.binding_for_session(*session).is_some());
        let layout = tabs.active_layout();
        self.pane_allocations.publish(layout.as_ref());
        self.tabs.sync_with_layout(
            tabs.snapshots(),
            tabs.active_bridge(),
            layout,
            logical_width,
        );
    }
    pub fn publish_panes(&self, tabs: &crate::tab_manager::TabManager) {
        self.pane_focus
            .borrow_mut()
            .retain(|session, _| tabs.binding_for_session(*session).is_some());
        let layout = tabs.active_layout();
        self.pane_allocations.publish(layout.as_ref());
        self.tabs.publish_layout(layout);
    }
    pub fn publish_pane_input_metrics(
        &self,
        scale: f32,
        surface: (u32, u32),
        metrics: harbor_terminal::TextMetrics,
    ) {
        self.pane_allocations
            .set_input_metrics(scale, surface, metrics);
    }
    pub fn pane_allocations(&self) -> Vec<crate::pane_layout::PaneAllocation> {
        self.pane_allocations.latest()
    }
    pub fn latest_terminal_allocation(&self) -> Option<Rect> {
        self.allocation.latest()
    }
    pub fn publish_keybindings(&self, keybindings: crate::command::ResolvedKeybindings) {
        self.update_state(|state| {
            if state.keybindings.as_ref() == Some(&keybindings) {
                return false;
            }
            state.keybindings = Some(keybindings);
            true
        });
    }

    pub fn report_configuration_result(&self, message: String) {
        self.publish_configuration_notice(ConfigurationNoticeSource::Operation, Some(message));
    }

    pub fn publish_configuration_notice(
        &self,
        source: ConfigurationNoticeSource,
        message: Option<String>,
    ) {
        self.update_notices(|notices| notices.publish(source, message));
    }

    /// Called by the application idle turn, independently of terminal activity.
    /// The deadline participates in the host wait policy, not a polling thread.
    pub fn advance_notifications(
        &self,
        now: std::time::Instant,
    ) -> (bool, Option<std::time::Instant>) {
        let deadline = self.store.state().read().notices.deadline();
        if !deadline.is_some_and(|deadline| now >= deadline) {
            return (false, deadline);
        }
        let expired = self.update_notices(|notices| notices.expire(now));
        (expired, self.store.state().read().notices.deadline())
    }

    pub fn dismiss_notification(&self, revision: u64) {
        self.update_notices(|notices| notices.dismiss(revision));
    }

    /// Keep unrelated workspace state and subscribers untouched for no-op updates.
    fn update_state(&self, update: impl FnOnce(&mut WorkspaceUiState) -> bool) -> bool {
        let current = self.store.state().read();
        let mut state = current.clone();
        drop(current);
        if !update(&mut state) {
            return false;
        }
        self.store.set_state(state);
        true
    }

    fn update_notices(&self, update: impl FnOnce(&mut ConfigurationNotices) -> bool) -> bool {
        self.update_state(|state| update(&mut state.notices))
    }
}

#[cfg(test)]
mod tests;
/// Stable application-owned inputs used to construct the main-window root.
#[derive(Clone)]
pub struct MainWindowRootInputs {
    pub controller: WorkspaceUiController,
    pub backdrop_available: bool,
    pub keybindings: ResolvedKeybindings,
    pub backdrop_fallback: [f32; 3],
}

impl MainWindowRootInputs {
    pub fn new(
        controller: WorkspaceUiController,
        backdrop_available: bool,
        backdrop_fallback: [f32; 3],
        keybindings: ResolvedKeybindings,
    ) -> Self {
        Self {
            controller,
            backdrop_available,
            backdrop_fallback,
            keybindings,
        }
    }
}

pub fn tab_workspace(
    controller: WorkspaceUiController,
    backdrop_available: bool,
) -> impl Component {
    tab_workspace_with_fallback(
        controller,
        backdrop_available,
        harbor_config::WindowBackdropStyle::default().fallback,
    )
}

pub fn tab_workspace_with_fallback(
    controller: WorkspaceUiController,
    backdrop_available: bool,
    backdrop_fallback: [f32; 3],
) -> impl Component {
    main_window_root(MainWindowRootInputs::new(
        controller,
        backdrop_available,
        backdrop_fallback,
        crate::command::resolve_keybindings(&harbor_config::RawKeybindings::default()).keybindings,
    ))
}

/// Builds the static main-window root from the same stable inputs used by HMR.
pub fn main_window_root(inputs: MainWindowRootInputs) -> impl Component {
    move |cx: &mut BuildCx| render_tab_workspace(cx, &inputs)
}

fn render_tab_workspace(cx: &mut BuildCx, props: &MainWindowRootInputs) -> View {
    let state = props.controller.store.watch(cx).clone();
    cx.track(&props.controller.tabs.state);
    let tab_state = props.controller.tabs.state.read().clone();
    let dispatcher = props.controller.store.dispatcher();
    let action_dispatcher = dispatcher.clone();
    let active_bridge = tab_state
        .active_bridge
        .clone()
        .expect("workspace has an active terminal until window exit");
    let root = root_padding(props.backdrop_available, props.backdrop_fallback);
    let bindings = state.keybindings.as_ref().unwrap_or(&props.keybindings);
    let shortcuts = if state
        .popups
        .active()
        .is_some_and(|popup| popup.blocks_shortcuts())
    {
        ShortcutMap::new()
    } else {
        shortcuts(bindings)
    };
    let command_bridge = active_bridge.clone();
    let actions = ActionHandler::new(move |command| {
        let outcome = command_outcome(command, &command_bridge);
        if outcome == ActionOutcome::Consumed {
            action_dispatcher.dispatch(AppCommandRequest::shortcut(command));
        }
        outcome
    });

    let dismiss_dispatcher = dispatcher.clone();
    let notification = state.notices.message().map(|message| {
        let revision = state.notices.revision;
        let top_offset = state
            .popups
            .active()
            .map_or(0.0, |popup| popup.notification_inset());
        configuration_notification(state.notices.title(), message, top_offset, move || {
            dismiss_dispatcher.dispatch(AppCommandRequest {
                command: AppCommand::DismissNotification(revision),
                focus: TabFocusPolicy::PreserveRail,
                source: crate::command::CommandSource::Direct,
            })
        })
    });
    harbor_widget::view! { Stack::new() => {
        root => {
            FocusScope::new() => {
                actions => {
                    shortcuts => {
                        Row::new() => {
                            tab_rail(cx, &tab_state, &props.controller.tabs, dispatcher);
                            terminal_panel(
                                cx,
                                &props.controller,
                                active_bridge,
                                tab_state.layout.as_ref(),
                                &state.popups,
                                bindings,
                            );
                        }
                    }
                }
            }
        }
        if let Some(notification) = notification {
            notification.keyed(format!("configuration-notification-{}", state.notices.revision));
        }
    } }
}

fn terminal_panel(
    cx: &mut BuildCx,
    controller: &WorkspaceUiController,
    active_bridge: TerminalWidgetBridge,
    layout: Option<&crate::pane_layout::PaneLayout>,
    popups: &crate::popups::PopupSlot,
    bindings: &ResolvedKeybindings,
) -> View {
    harbor_widget::view! { Expanded::new() => {
        harbor_widget::Column::new() => {
            Expanded::new().keyed("terminal-surface") => {
                controller.allocation.observer() => {
                    if let Some(layout) = layout {
                        layout.build(cx, controller);
                    } else {
                        TerminalDecorationPreset::container() => {
                            Focus::new().focus_handle(controller.terminal_focus()) => { terminal_widget(active_bridge); }
                        }
                    }
                }
            }
            if let Some(popup) = popups.active() {
                popup.build(cx, controller, bindings, popups.generation().unwrap());
            }
        }
    } }
}

/// Main-window root styling without attaching widget children by hand.
pub(crate) fn root_padding(backdrop_available: bool, fallback_rgb: [f32; 3]) -> Padding {
    // Product/native colors are sRGB; widget colors feed a linear-light shader.
    let fallback = fallback_rgb.map(|channel| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    if backdrop_available {
        Padding::all(4.0).unwrap()
    } else {
        Padding::all(4.0)
            .unwrap()
            .background(Color {
                r: fallback[0],
                g: fallback[1],
                b: fallback[2],
                a: 1.0,
            })
            .expect("finite background")
    }
}

fn shortcuts(keybindings: &ResolvedKeybindings) -> ShortcutMap<AppCommand> {
    keybindings
        .bindings()
        .fold(ShortcutMap::new(), |shortcuts, (chord, command)| {
            shortcuts.bind(chord, command)
        })
}

fn command_outcome(command: AppCommand, bridge: &TerminalWidgetBridge) -> ActionOutcome {
    match command {
        AppCommand::CopyOrInterrupt if !bridge.has_non_empty_selection() => {
            ActionOutcome::PassThrough
        }
        AppCommand::PageUp
        | AppCommand::PageDown
        | AppCommand::ScrollToTop
        | AppCommand::ScrollToBottom
            if bridge.is_alt_screen() =>
        {
            ActionOutcome::PassThrough
        }
        _ => ActionOutcome::Consumed,
    }
}
#[derive(Clone)]
pub(crate) struct TerminalAllocationMailbox {
    latest: Arc<Mutex<Option<Rect>>>,
    callback: LayoutChangedCallback,
}

impl TerminalAllocationMailbox {
    fn new() -> Self {
        let latest = Arc::new(Mutex::new(None));
        let published = Arc::clone(&latest);
        let callback: LayoutChangedCallback = Arc::new(move |rect| {
            if let Ok(mut latest) = published.lock() {
                *latest = Some(rect);
            }
        });
        Self { latest, callback }
    }

    pub(crate) fn latest(&self) -> Option<Rect> {
        *self.latest.lock().ok()?
    }

    fn observer(&self) -> LayoutObserver {
        LayoutObserver::from_shared_callback(Arc::clone(&self.callback))
    }
}
