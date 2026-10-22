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
    ActionOutcome, Actions, ComponentExt as _, Expanded, Focus, FocusHandle, FocusScope,
    IconButton, LayoutObserver, LucideIcon, Portal, Row, Shortcuts, Store, TextEditingController,
    TextField,
    layout::Rect,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{layout_observer::LayoutChangedCallback, padding::Padding, stack::Stack},
};
#[path = "palette.rs"]
mod palette;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct SearchUiState {
    source: crate::tab_manager::TabId,
    input: TextEditingController,
    status: String,
    focus: FocusHandle,
    cancellation: std::rc::Rc<std::cell::RefCell<Option<crate::search::SearchCancellation>>>,
}

impl crate::popups::Popup for SearchUiState {
    fn build(
        &self,
        cx: &mut BuildCx,
        ui: &WorkspaceUiController,
        _: &ResolvedKeybindings,
        generation: u64,
    ) -> View {
        search_bar(cx, ui, self, generation)
    }
    fn focus(&self) -> FocusHandle {
        self.focus
    }
    fn close(&self) {
        self.input.set_text("");
        if let Some(cancellation) = self.cancellation.borrow_mut().take() {
            cancellation.cancel();
        }
    }
    fn notification_inset(&self) -> f32 {
        SEARCH_PANEL_HEIGHT + SEARCH_PANEL_INSET
    }
}

fn search_bar(
    cx: &mut BuildCx,
    controller: &WorkspaceUiController,
    search: &SearchUiState,
    generation: u64,
) -> View {
    let appearance = GlassAppearance::default();
    let input = search.input.clone();
    let changed_ui = controller.clone();
    let submit = controller.store.dispatcher();
    let close = controller.store.dispatcher();
    let entry = TextField::new(input)
        .decoration(appearance.input())
        .padding(6.0)
        .placeholder("Find…")
        .focus_handle(controller.search_focus)
        .on_changed(move |_| {
            if !changed_ui
                .store
                .state()
                .read()
                .popups
                .is_current(generation)
            {
                return;
            }
            changed_ui.publish_search_status("No Result");
            changed_ui
                .store
                .dispatcher()
                .dispatch(AppCommandRequest::popup(
                    AppCommand::SearchChanged,
                    generation,
                ));
        })
        .on_submitted(move |modifiers| {
            submit.dispatch(AppCommandRequest::popup(
                if modifiers.shift {
                    AppCommand::SearchPrevious
                } else {
                    AppCommand::SearchNext
                },
                generation,
            ))
        })
        .on_escape(move || {
            close.dispatch(AppCommandRequest::popup(
                AppCommand::SearchClose,
                generation,
            ))
        });
    let style = glass_icon_button_style(cx);
    let button = |icon, label, command| {
        let dispatcher = controller.store.dispatcher();
        IconButton::new(icon, label)
            .style(style.clone())
            .on_click(move |_| dispatcher.dispatch(AppCommandRequest::popup(command, generation)))
    };
    let status = search.status.clone();
    let content = harbor_widget::view! { &mut *cx;
        Padding::all(8.0) => {
            Row::new().gap(4.0).cross_axis_alignment(harbor_widget::Alignment::Center) => {
                Expanded::new() => { entry; }
                // Status text changes frequently; reserve its slot instead of resizing the editor.
                harbor_widget::ConstrainedBox::new().min_width(120.0).max_width(120.0) => {
                    harbor_widget::widgets::text_label::TextLabel::new(status)
                        .color(cx.theme().button.normal.foreground).ellipsis();
                }
                button(LucideIcon::ChevronUp, "Previous match", AppCommand::SearchPrevious);
                button(LucideIcon::ChevronDown, "Next match", AppCommand::SearchNext);
                button(LucideIcon::X, "Close search", AppCommand::SearchClose);
            }
        }
    };
    let card = appearance.panel(cx, content);
    Portal::new()
        .child(
            harbor_widget::widgets::align::Align::new(harbor_widget::Alignment::End)
                .vertical_alignment(harbor_widget::Alignment::Start)
                .insets(
                    SEARCH_PANEL_INSET,
                    SEARCH_PANEL_INSET,
                    SEARCH_PANEL_INSET,
                    SEARCH_PANEL_INSET,
                )
                .child(
                    harbor_widget::ConstrainedBox::new()
                        .min_width(440.0)
                        .max_width(440.0)
                        .min_height(SEARCH_PANEL_HEIGHT)
                        .max_height(SEARCH_PANEL_HEIGHT)
                        .child(card),
                ),
        )
        .keyed(format!("session-search-{generation}"))
        .build(cx)
}

// Fixed chrome height also reserves space for configuration notices below search.
const SEARCH_PANEL_HEIGHT: f32 = 48.0;
const SEARCH_PANEL_INSET: f32 = 12.0;

#[derive(Clone, Default)]
struct WorkspaceUiState {
    keybindings: Option<ResolvedKeybindings>,
    notices: ConfigurationNotices,
    popups: crate::popups::PopupSlot,
}

/// App-wide publication and the sole one-shot action inbox, independent of tab sync.
#[derive(Clone)]
pub struct WorkspaceUiController {
    store: Store<WorkspaceUiState, AppCommandRequest>,
    tabs: TabUiController,
    terminal_focus: FocusHandle,
    search_focus: FocusHandle,
    palette_focus: FocusHandle,
    allocation: TerminalAllocationMailbox,
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
            search_focus: FocusHandle::new(),
            palette_focus: FocusHandle::new(),
            allocation: TerminalAllocationMailbox::new(),
        }
    }

    pub fn open_search(&self, source: crate::tab_manager::TabId) {
        let mut state = self.store.state().read().clone();
        if state
            .popups
            .get::<SearchUiState>()
            .is_some_and(|search| search.source == source)
        {
            return;
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
        self.store.set_state(state);
    }

    pub fn close_search(&self) {
        let mut state = self.store.state().read().clone();
        if state.popups.get::<SearchUiState>().is_some() {
            let generation = state.popups.generation().unwrap();
            state.popups.close(generation);
            self.store.set_state(state);
        }
    }

    pub fn search_focus(&self) -> FocusHandle {
        self.search_focus
    }

    pub fn search_query(&self) -> Option<(crate::tab_manager::TabId, String)> {
        let state = self.store.state().read();
        let search = state.popups.get::<SearchUiState>()?;
        Some((search.source, search.input.text()))
    }

    pub(crate) fn bind_search_cancellation(&self, cancellation: crate::search::SearchCancellation) {
        if let Some(search) = self.store.state().read().popups.get::<SearchUiState>() {
            *search.cancellation.borrow_mut() = Some(cancellation);
        }
    }

    pub fn sync_search_source(&self, active: Option<crate::tab_manager::TabId>) {
        if self
            .search_query()
            .is_some_and(|(source, _)| Some(source) != active)
        {
            self.close_search();
        }
    }

    pub fn publish_search_status(&self, status: &str) {
        let mut state = self.store.state().read().clone();
        if state
            .popups
            .get::<SearchUiState>()
            .is_none_or(|search| search.status == status)
        {
            return;
        }
        state
            .popups
            .update::<SearchUiState>(|search| search.status = status.into());
        self.store.set_state(state);
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

    pub fn tabs(&self) -> &TabUiController {
        &self.tabs
    }
    pub fn drain_actions(&self) -> Vec<AppCommandRequest> {
        self.store.drain_actions()
    }
    pub fn terminal_focus(&self) -> FocusHandle {
        self.terminal_focus
    }
    pub fn latest_terminal_allocation(&self) -> Option<Rect> {
        self.allocation.latest()
    }
    pub fn publish_keybindings(&self, keybindings: crate::command::ResolvedKeybindings) {
        let current = self.store.state().read();
        if current.keybindings.as_ref() == Some(&keybindings) {
            return;
        }
        let mut state = current.clone();
        drop(current);
        state.keybindings = Some(keybindings);
        self.store.set_state(state);
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

    /// Keep unrelated workspace state and subscribers untouched for notice no-ops.
    fn update_notices(&self, update: impl FnOnce(&mut ConfigurationNotices) -> bool) -> bool {
        let current = self.store.state().read();
        let mut notices = current.notices.clone();
        if !update(&mut notices) {
            return false;
        }
        let mut state = current.clone();
        drop(current);
        state.notices = notices;
        self.store.set_state(state);
        true
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
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

#[allow(dead_code)]
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
        Shortcuts::empty()
    } else {
        shortcuts(bindings)
    };
    let command_bridge = active_bridge.clone();
    let actions = Actions::handler(move |command| {
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
    harbor_widget::view! { cx; Stack::new() => {
        root => {
            FocusScope::new() => {
                actions => {
                    shortcuts => {
                        Row::new() => {
                            tab_rail(cx, &tab_state, &props.controller.tabs, dispatcher);
                            terminal_panel(cx, &props.controller, active_bridge, bindings);
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
    bindings: &ResolvedKeybindings,
) -> View {
    let popup_state = controller.store.state().read().clone();
    harbor_widget::view! { cx; Expanded::new() => {
        harbor_widget::Column::new() => {
            Expanded::new().keyed("terminal-surface") => {
                controller.allocation.observer() => {
                    TerminalDecorationPreset::container() => {
                        Focus::empty().handle(controller.terminal_focus) => {
                            terminal_widget(active_bridge);
                        }
                    }
                }
            }
            if let Some(popup) = popup_state.popups.active() {
                popup.build(cx, controller, bindings, popup_state.popups.generation().unwrap());
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
        Padding::all(4.0)
    } else {
        Padding::all(4.0).background(Color {
            r: fallback[0],
            g: fallback[1],
            b: fallback[2],
            a: 1.0,
        })
    }
}

fn shortcuts(keybindings: &ResolvedKeybindings) -> Shortcuts<AppCommand> {
    keybindings
        .bindings()
        .fold(Shortcuts::empty(), |shortcuts, (chord, command)| {
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
        LayoutObserver::from_callback(Arc::clone(&self.callback))
    }
}
