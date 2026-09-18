//! Declarative widget definitions for Harbor's main workspace and dialogs.

use harbor_widget::{
    ActionOutcome, Actions, Button, Column, ComponentExt as _, ConstrainedBox, Dispatcher,
    Expanded, Focus, FocusScope, IconButton, Row, ScrollArea, Separator, Shortcuts,
    layout::Size,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{
        padding::Padding, preview_pane::PreviewPane, sized_box::SizedBox, text_label::TextLabel,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use super::{RailPresentation, TabCommand, TabUiController, TabUiState};
use crate::{
    command::{AppCommand, AppCommandRequest, ResolvedKeybindings},
    tab_manager::TabSnapshot,
    terminal_view::{TerminalDecorationPreset, TerminalWidgetBridge, terminal_widget},
};
#[cfg(test)]
use harbor_widget::input::event::Modifiers;

pub const CONFIRMATION_PREVIEW_VISIBLE_LINES: usize = 12;

/// Stable application-owned inputs used to construct the main-window root.
#[derive(Clone)]
pub struct MainWindowRootInputs {
    pub controller: TabUiController,
    pub backdrop_available: bool,
    pub keybindings: ResolvedKeybindings,
    pub backdrop_fallback: [f32; 3],
}

impl MainWindowRootInputs {
    pub fn new(
        controller: TabUiController,
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
pub fn tab_workspace(controller: TabUiController, backdrop_available: bool) -> impl Component {
    main_window_root(MainWindowRootInputs::new(
        controller,
        backdrop_available,
        harbor_config::WindowBackdropStyle::default().fallback,
        crate::command::resolve_keybindings(&harbor_config::RawKeybindings::default()).keybindings,
    ))
}

pub fn tab_workspace_with_fallback(
    controller: TabUiController,
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
    let dispatcher = props.controller.store.dispatcher();
    let action_dispatcher = dispatcher.clone();
    let active_bridge = state
        .active_bridge
        .clone()
        .expect("workspace has an active terminal until window exit");
    let root = root_padding(props.backdrop_available, props.backdrop_fallback);
    let shortcuts = shortcuts(&props.keybindings);
    let command_bridge = active_bridge.clone();
    let actions = Actions::handler(move |command| {
        let outcome = command_outcome(command, &command_bridge);
        if outcome == ActionOutcome::Consumed {
            action_dispatcher.dispatch(AppCommandRequest::shortcut(command));
        }
        outcome
    });

    harbor_widget::view! { cx; root => {
        FocusScope::new() => {
            actions => {
                shortcuts => {
                    Row::new() => {
                        tab_rail(cx, &state, &props.controller, dispatcher);
                        Separator::vertical();
                        terminal_panel(cx, &props.controller, active_bridge);
                    }
                }
            }
        }
    } }
}

fn tab_rail(
    cx: &mut BuildCx,
    state: &TabUiState,
    controller: &TabUiController,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> View {
    let new_dispatcher = dispatcher.clone();
    harbor_widget::view! { cx;
        ConstrainedBox::new()
            .min_width(state.presentation.width())
            .max_width(state.presentation.width()) => {
            Column::new() => {
                Expanded::new() => {
                    ScrollArea::new().controller(controller.scroll.clone()) => {
                        Column::new() => {
                            for snapshot in state.snapshots.iter() {
                                tab_row(
                                    cx,
                                    snapshot,
                                    state.presentation,
                                    controller,
                                    dispatcher.clone(),
                                );
                            }
                        }
                    }
                }
                match state.presentation {
                    RailPresentation::Expanded => {
                        Button::new("New terminal").on_click(move |_| {
                            new_dispatcher.dispatch(AppCommandRequest::rail(TabCommand::New));
                        });
                    },
                    RailPresentation::Compact => {
                        IconButton::new("+", "New terminal").on_click(move |_| {
                            new_dispatcher.dispatch(AppCommandRequest::rail(TabCommand::New));
                        });
                    }
                }
            }
        }
    }
}

fn tab_row(
    cx: &mut BuildCx,
    snapshot: &TabSnapshot,
    presentation: RailPresentation,
    controller: &TabUiController,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> View {
    harbor_widget::view! { cx;
        Row::new().keyed(format!("terminal-tab-{}", snapshot.id)) => {
            Expanded::new() => {
                Focus::empty()
                    .handle(controller
                        .tab_focus(snapshot.id)
                        .expect("live tab has a focus handle")) => {
                    select_tab_button(snapshot, presentation, dispatcher.clone());
                }
            }
            close_tab_button(snapshot, dispatcher);
        }
    }
}

fn terminal_panel(
    cx: &mut BuildCx,
    controller: &TabUiController,
    active_bridge: TerminalWidgetBridge,
) -> View {
    harbor_widget::view! { cx; Expanded::new() => {
        controller.allocation.observer() => {
            TerminalDecorationPreset::container() => {
                Focus::empty().handle(controller.terminal_focus) => {
                    terminal_widget(active_bridge);
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

#[derive(Clone)]
struct ConfirmationDialogProps {
    header_text: String,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
}

fn confirmation_dialog(cx: &mut BuildCx, props: &ConfirmationDialogProps) -> View {
    let cancelled = Arc::clone(&props.cancelled);
    let confirmed = Arc::clone(&props.confirmed);
    harbor_widget::view! { cx; FocusScope::new() => {
        Padding::new(24.0, 16.0, 24.0, 16.0) => {
            Column::new() => {
                TextLabel::new(props.header_text.clone());
                SizedBox::new(Size::new(0.0, 8.0));
                PreviewPane::new(
                    props.wrapped_lines.clone(),
                    Arc::clone(&props.scroll_offset),
                    props.line_height,
                    CONFIRMATION_PREVIEW_VISIBLE_LINES,
                );
                SizedBox::new(Size::new(0.0, 12.0));
                Row::new() => {
                    Button::new("Cancel").on_click(move |_| {
                        cancelled.store(true, Ordering::SeqCst);
                    });
                    SizedBox::new(Size::new(12.0, 0.0));
                    Button::new("Paste").on_click(move |_| {
                        confirmed.store(true, Ordering::SeqCst);
                    });
                }
            }
        }
    } }
}

pub fn build_confirmation_root(
    line_count: usize,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
) -> impl Component {
    let props = ConfirmationDialogProps {
        header_text: format!("Paste {line_count} lines?"),
        wrapped_lines,
        scroll_offset,
        cancelled,
        confirmed,
        line_height,
    };
    move |cx: &mut BuildCx| confirmation_dialog(cx, &props)
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

#[cfg(test)]
pub(super) fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Modifiers::default()
    }
}

fn select_tab_button(
    snapshot: &TabSnapshot,
    presentation: RailPresentation,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> IconButton {
    let id = snapshot.id;
    let label = tab_label(snapshot);
    let glyph = match presentation {
        RailPresentation::Expanded => label.clone(),
        RailPresentation::Compact => abbreviation(snapshot),
    };
    IconButton::new(glyph, label)
        .selected(snapshot.active)
        .on_click(move |_| {
            dispatcher.dispatch(AppCommandRequest::rail(TabCommand::Activate(id)));
        })
}

fn close_tab_button(
    snapshot: &TabSnapshot,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> IconButton {
    let id = snapshot.id;
    IconButton::new("×", format!("Close {}", snapshot.title)).on_click(move |_| {
        dispatcher.dispatch(AppCommandRequest::close_rail(id));
    })
}

pub(super) fn tab_label(snapshot: &TabSnapshot) -> String {
    if snapshot.unread {
        format!("• {}", snapshot.title)
    } else {
        snapshot.title.clone()
    }
}

pub(super) fn abbreviation(snapshot: &TabSnapshot) -> String {
    if snapshot.unread {
        "•".to_owned()
    } else {
        (snapshot.id.get() % 10).to_string()
    }
}
