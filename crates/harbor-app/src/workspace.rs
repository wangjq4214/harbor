//! Application workspace composition and Host-published global UI state.

use crate::{
    appearance::SearchAppearance,
    command::{AppCommand, AppCommandRequest, ResolvedKeybindings},
    notifications::{ConfigurationNoticeSource, ConfigurationNotices, configuration_notification},
    tab_manager::TabSnapshot,
    tab_view::{TabFocusPolicy, TabUiController, ui::tab_rail},
    terminal_view::{TerminalDecorationPreset, TerminalWidgetBridge, terminal_widget},
};
use harbor_widget::{
    ActionOutcome, Actions, BackdropFilter, ClipBehavior, ComponentExt as _, DecoratedBox,
    Expanded, Focus, FocusHandle, FocusScope, IconButton, LayoutObserver, LucideIcon, Portal, Row,
    Shortcuts, Store, TextEditingController, TextField,
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
    open: bool,
    source: Option<crate::tab_manager::TabId>,
    input: TextEditingController,
    status: String,
}

impl Default for SearchUiState {
    fn default() -> Self {
        Self {
            open: false,
            source: None,
            input: TextEditingController::with_acceptance("", |text| {
                text.len() <= harbor_terminal::literal_search::MAX_QUERY_BYTES
            }),
            status: "No Result".into(),
        }
    }
}

fn search_bar(cx: &mut BuildCx, controller: &WorkspaceUiController) -> View {
    let input = controller.store.state().read().search.input.clone();
    let changed_ui = controller.clone();
    let submit = controller.store.dispatcher();
    let close = controller.store.dispatcher();
    let entry = TextField::new(input)
        .decoration(SearchAppearance::input())
        .padding(6.0)
        .placeholder("Find…")
        .focus_handle(controller.search_focus)
        .on_changed(move |_| {
            changed_ui.publish_search_status("No Result");
            changed_ui
                .store
                .dispatcher()
                .dispatch(AppCommandRequest::shortcut(AppCommand::SearchChanged));
        })
        .on_submitted(move |modifiers| {
            submit.dispatch(AppCommandRequest::shortcut(if modifiers.shift {
                AppCommand::SearchPrevious
            } else {
                AppCommand::SearchNext
            }))
        })
        .on_escape(move || close.dispatch(AppCommandRequest::shortcut(AppCommand::SearchClose)));
    let mut style = cx.theme().icon_button.clone();
    style.min_height = 32.0;
    style.icon_size = 18.0;
    style.horizontal_padding = 7.0;
    style.border_width = 0.0;
    for colors in [
        &mut style.normal,
        &mut style.hovered,
        &mut style.pressed,
        &mut style.selected,
        &mut style.disabled,
    ] {
        colors.background = Color::TRANSPARENT;
    }
    style.normal.foreground.a = 0.78;
    style.hovered.foreground.a = 1.0;
    style.pressed.foreground.a = 0.5;
    let button = |icon, label, command| {
        let dispatcher = controller.store.dispatcher();
        IconButton::new(icon, label)
            .style(style.clone())
            .on_click(move |_| dispatcher.dispatch(AppCommandRequest::shortcut(command)))
    };
    let status = controller.store.state().read().search.status.clone();
    let radius = SearchAppearance::radius();
    let decoration = SearchAppearance::panel();
    let card = harbor_widget::view! { &mut *cx;
        BackdropFilter::new(SearchAppearance::BLUR_SIGMA).unwrap()
            .border_radius(radius).edge_refraction(SearchAppearance::refraction()) => {
            DecoratedBox::new(decoration).clip_behavior(ClipBehavior::AntiAlias) => {
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
            }
        }
    };
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
        .keyed("session-search")
        .build(cx)
}

// Fixed chrome height also reserves space for configuration notices below search.
const SEARCH_PANEL_HEIGHT: f32 = 48.0;
const SEARCH_PANEL_INSET: f32 = 12.0;

#[derive(Clone, Default)]
struct WorkspaceUiState {
    keybindings: Option<ResolvedKeybindings>,
    notices: ConfigurationNotices,
    search: SearchUiState,
    palette: Option<palette::PaletteState>,
    palette_generation: u64,
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
        if state.search.source != Some(source) {
            state.search = SearchUiState::default();
        }
        state.search.open = true;
        state.search.source = Some(source);
        self.store.set_state(state);
    }

    pub fn close_search(&self) {
        let mut state = self.store.state().read().clone();
        state.search = SearchUiState::default();
        self.store.set_state(state);
    }

    pub fn search_focus(&self) -> FocusHandle {
        self.search_focus
    }

    pub fn search_query(&self) -> Option<(crate::tab_manager::TabId, String)> {
        let state = self.store.state().read();
        if !state.search.open {
            return None;
        }
        Some((state.search.source?, state.search.input.text()))
    }

    pub fn sync_search_source(&self, active: Option<crate::tab_manager::TabId>) {
        if self
            .store
            .state()
            .read()
            .search
            .source
            .is_some_and(|source| Some(source) != active)
        {
            self.close_search();
        }
    }

    pub fn publish_search_status(&self, status: &str) {
        let mut state = self.store.state().read().clone();
        if state.search.status != status {
            state.search.status = status.into();
            self.store.set_state(state);
        }
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
        let mut state = self.store.state().read().clone();
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
        let mut state = self.store.state().read().clone();
        state.notices.publish(source, message);
        self.store.set_state(state);
    }

    pub fn dismiss_notification(&self, revision: u64) {
        let mut state = self.store.state().read().clone();
        state.notices.dismiss(revision);
        self.store.set_state(state);
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
    main_window_root(MainWindowRootInputs::new(
        controller,
        backdrop_available,
        harbor_config::WindowBackdropStyle::default().fallback,
        crate::command::resolve_keybindings(&harbor_config::RawKeybindings::default()).keybindings,
    ))
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
    let shortcuts = if state.palette.is_some() {
        Shortcuts::empty()
    } else {
        shortcuts(state.keybindings.as_ref().unwrap_or(&props.keybindings))
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
        let top_offset = if state.search.open {
            SEARCH_PANEL_HEIGHT + SEARCH_PANEL_INSET
        } else {
            0.0
        };
        configuration_notification(message, top_offset, move || {
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
                            terminal_panel(cx, &props.controller, active_bridge, state.keybindings.as_ref().unwrap_or(&props.keybindings));
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
    let search = controller.store.state().read().search.clone();
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
            if search.open {
                search_bar(cx, controller);
            }
            if controller.palette_is_open() {
                palette::palette_panel(cx, controller, bindings);
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
