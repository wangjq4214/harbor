//! Application workspace composition and Host-published global UI state.

use crate::{
    command::{AppCommand, AppCommandRequest, ResolvedKeybindings},
    notifications::{ConfigurationNoticeSource, ConfigurationNotices, configuration_notification},
    tab_manager::TabSnapshot,
    tab_view::{TabFocusPolicy, TabUiController, ui::tab_rail},
    terminal_view::{TerminalDecorationPreset, TerminalWidgetBridge, terminal_widget},
};
use harbor_widget::{
    ActionOutcome, Actions, ComponentExt as _, Expanded, Focus, FocusHandle, FocusScope,
    LayoutObserver, Row, Shortcuts, Store,
    layout::Rect,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{layout_observer::LayoutChangedCallback, padding::Padding, stack::Stack},
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct SearchUiState {
    open: bool,
    source: Option<crate::tab_manager::TabId>,
    query: String,
    preedit: String,
    status: String,
}

#[derive(Clone)]
struct SearchEntry(WorkspaceUiController);

impl harbor_widget::view::PrimitiveLeaf for SearchEntry {
    fn intrinsic_size(
        &self,
        constraints: harbor_widget::layout::BoxConstraints,
        metrics: &harbor_widget::text::TextMetrics,
    ) -> harbor_widget::layout::Size {
        constraints.constrain(harbor_widget::layout::Size::new(
            280.0,
            metrics.line_height + 8.0,
        ))
    }
    fn paint_primitives(
        &self,
        rect: Rect,
        _metrics: &harbor_widget::text::TextMetrics,
    ) -> Vec<harbor_widget::scene::primitive::Primitive> {
        let state = self.0.store.state().read();
        vec![harbor_widget::scene::primitive::Primitive::Text {
            text: Arc::from(format!(
                "Find: {}{}▏",
                state.search.query, state.search.preedit
            )),
            origin: rect.min,
            color: Color::WHITE,
        }]
    }
    fn handle_event(
        &self,
        event: &harbor_widget::input::event::UiEvent,
        ctx: &mut harbor_widget::input::event_ctx::EventCtx,
        _rect: Rect,
    ) -> harbor_widget::input::event_ctx::EventHandled {
        use harbor_widget::input::{
            event::{Key, KeyboardEvent, UiEvent},
            event_ctx::EventHandled,
        };
        let mut state = self.0.store.state().read().clone();
        let mut changed = false;
        let command = match event {
            UiEvent::Pointer(pointer)
                if pointer.phase == harbor_widget::input::event::PointerPhase::Down =>
            {
                Some(AppCommand::SearchOpen)
            }
            UiEvent::Focus(harbor_widget::input::event::FocusEvent::Lost) => {
                state.search.preedit.clear();
                self.0.store.set_state(state);
                return EventHandled::Handled;
            }
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Escape, ..
            }) => Some(AppCommand::SearchClose),
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Enter | Key::NumpadEnter,
                modifiers,
            }) => Some(if modifiers.shift {
                AppCommand::SearchPrevious
            } else {
                AppCommand::SearchNext
            }),
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Backspace,
                ..
            }) => {
                changed = state.search.query.pop().is_some();
                None
            }
            UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers })
                if !modifiers.ctrl && !modifiers.alt && !modifiers.meta =>
            {
                let ch = match key {
                    Key::Character(ch) | Key::NumpadCharacter(ch) => Some(*ch),
                    Key::Space => Some(' '),
                    _ => None,
                };
                if let Some(ch) = ch
                    && !ch.is_control()
                    && state.search.query.len() + ch.len_utf8()
                        <= harbor_terminal::literal_search::MAX_QUERY_BYTES
                {
                    state.search.query.push(ch);
                    changed = true;
                }
                None
            }
            UiEvent::Keyboard(KeyboardEvent::Ime(text)) => {
                if state.search.query.len() + text.len()
                    <= harbor_terminal::literal_search::MAX_QUERY_BYTES
                    && !text.chars().any(char::is_control)
                {
                    state.search.query.push_str(text);
                    state.search.preedit.clear();
                    changed = true;
                }
                None
            }
            UiEvent::ImePreedit(preedit) => {
                state.search.preedit = preedit.text.chars().take(32).collect();
                self.0.store.set_state(state);
                ctx.invalidate_paint();
                return EventHandled::Handled;
            }
            UiEvent::Keyboard(_) => None,
            _ => return EventHandled::Ignored,
        };
        if changed {
            state.search.preedit.clear();
            state.search.status = "Searching…".into();
            self.0.store.set_state(state);
            self.0
                .store
                .dispatcher()
                .dispatch(AppCommandRequest::shortcut(AppCommand::SearchChanged));
        }
        if let Some(command) = command {
            self.0
                .store
                .dispatcher()
                .dispatch(AppCommandRequest::shortcut(command));
        }
        ctx.invalidate_paint();
        EventHandled::Handled
    }
}

fn search_bar(cx: &mut BuildCx, controller: &WorkspaceUiController) -> View {
    let entry = View::leaf(SearchEntry(controller.clone()));
    let button = |label, command| {
        let dispatcher = controller.store.dispatcher();
        harbor_widget::Button::new(label)
            .on_click(move |_| dispatcher.dispatch(AppCommandRequest::shortcut(command)))
    };
    let status = controller.store.state().read().search.status.clone();
    harbor_widget::view! { cx; harbor_widget::Column::new() => {
        Row::new() => {
            Expanded::new() => { Focus::new(entry).handle(controller.search_focus); }
            button("Previous", AppCommand::SearchPrevious);
            button("Next", AppCommand::SearchNext);
            button("Close", AppCommand::SearchClose);
        }
        harbor_widget::widgets::text_label::TextLabel::new(status).ellipsis();
    } }
}

#[derive(Clone, Default)]
struct WorkspaceUiState {
    keybindings: Option<ResolvedKeybindings>,
    notices: ConfigurationNotices,
    search: SearchUiState,
}

/// App-wide publication and the sole one-shot action inbox, independent of tab sync.
#[derive(Clone)]
pub struct WorkspaceUiController {
    store: Store<WorkspaceUiState, AppCommandRequest>,
    tabs: TabUiController,
    terminal_focus: FocusHandle,
    search_focus: FocusHandle,
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
        Some((state.search.source?, state.search.query.clone()))
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
    let shortcuts = shortcuts(state.keybindings.as_ref().unwrap_or(&props.keybindings));
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
        configuration_notification(message, move || {
            dismiss_dispatcher.dispatch(AppCommandRequest {
                command: AppCommand::DismissNotification(revision),
                focus: TabFocusPolicy::PreserveRail,
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
                            terminal_panel(cx, &props.controller, active_bridge);
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
) -> View {
    let search = controller.store.state().read().search.clone();
    harbor_widget::view! { cx; Expanded::new() => {
        harbor_widget::Column::new() => {
            if search.open {
                search_bar(cx, controller);
            }
            Expanded::new().keyed("terminal-surface") => {
                controller.allocation.observer() => {
                    TerminalDecorationPreset::container() => {
                        Focus::empty().handle(controller.terminal_focus) => {
                            terminal_widget(active_bridge);
                        }
                    }
                }
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
