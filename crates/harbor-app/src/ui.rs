//! Declarative widget definitions for Harbor's main workspace and dialogs.

use harbor_widget::{
    ActionOutcome, Actions, Alignment, BoxDecoration, Button, ClipBehavior, Column,
    ComponentExt as _, ConstrainedBox, ControlColors, ControlStyle, DecoratedBox, Dispatcher,
    Expanded, Focus, FocusScope, Icon, IconButton, InteractionState, InteractiveRegion, LucideIcon,
    Row, ScrollArea, Shortcuts,
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
    let shortcuts = shortcuts(state.keybindings.as_ref().unwrap_or(&props.keybindings));
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
                        terminal_panel(cx, &props.controller, active_bridge, state.configuration_result.clone());
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
    harbor_widget::view! { cx;
        ConstrainedBox::new()
            .min_width(state.presentation.width())
            .max_width(state.presentation.width()) => {
            ScrollArea::new().controller(controller.scroll.clone()) => {
                Padding::new(0.0, 6.0, 4.0, 6.0) => {
                    Column::new().cross_axis_alignment(Alignment::Stretch).gap(4.0) => {
                        for snapshot in state.snapshots.iter() {
                            tab_row(cx, snapshot, state.presentation, controller, dispatcher.clone());
                        }
                        new_tab_button(state.presentation, dispatcher).keyed("new-terminal");
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
    let id = snapshot.id;
    let title = snapshot.title.clone();
    let compact = presentation == RailPresentation::Compact;
    let unread = snapshot.unread;
    let icon = if unread {
        LucideIcon::Dot
    } else {
        LucideIcon::Terminal
    };
    let short = abbreviation(snapshot);
    let select_dispatcher = dispatcher.clone();
    let region = InteractiveRegion::new()
        .style(rail_style(cx, compact))
        .selected(snapshot.active)
        .focus_handle(controller.tab_focus(id).expect("live tab has a focus handle"))
        .on_activate(move |_| {
            select_dispatcher.dispatch(AppCommandRequest::rail(TabCommand::Activate(id)));
        })
        .content(move |cx, state| {
            let color = rail_style(cx, compact).resolve_state(&state).foreground;
            harbor_widget::view! { cx;
                Row::new().cross_axis_alignment(Alignment::Center).gap(if compact { 2.0 } else { 8.0 }) => {
                    if !compact {
                        ConstrainedBox::new().min_width(24.0).max_width(24.0) => {
                            Icon::new(icon).size(20.0).color(color);
                        }
                    }
                    if compact && unread {
                        Icon::new(LucideIcon::Dot).size(12.0).color(color);
                    }
                    Expanded::new() => {
                        DecoratedBox::new(BoxDecoration::new()).clip_behavior(ClipBehavior::HardEdge) => {
                            TextLabel::new(if compact { short.clone() } else { title.clone() })
                                .color(color).ellipsis();
                        }
                    }
                    if !compact {
                        close_tab_button(cx, id, state, dispatcher.clone());
                    }
                }
            }
        });
    // Defer the stateful region to its own keyed fiber, rather than borrowing
    // the workspace's positional hooks when tabs are inserted or removed.
    harbor_widget::view! { cx;
        Padding::new(0.0, 0.0, 0.0, 0.0).keyed(format!("terminal-tab-{id}")) => {
            region;
        }
    }
}

fn rail_style(cx: &BuildCx, compact: bool) -> ControlStyle {
    let mut style = cx.theme().button.clone();
    let foreground = style.normal.foreground;
    let colors = |alpha| ControlColors {
        background: Color {
            a: alpha,
            ..foreground
        },
        border: Color::TRANSPARENT,
        foreground,
    };
    style.min_height = 34.0;
    style.horizontal_padding = if compact { 3.0 } else { 6.0 };
    style.border_width = 0.0;
    style.radius = 6.0;
    style.normal = colors(0.0);
    style.hovered = colors(0.06);
    style.pressed = colors(0.12);
    style.selected = colors(0.10);
    style
}

fn new_tab_button(
    presentation: RailPresentation,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> impl Component {
    move |cx: &mut BuildCx| {
        let compact = presentation == RailPresentation::Compact;
        InteractiveRegion::new()
            .style(rail_style(cx, compact))
            .on_activate({
                let dispatcher = dispatcher.clone();
                move |_| dispatcher.dispatch(AppCommandRequest::rail(TabCommand::New))
            })
            .content(move |cx, state| {
                let color = rail_style(cx, compact).resolve_state(&state).foreground;
                harbor_widget::view! { cx;
                    Row::new().cross_axis_alignment(Alignment::Center).gap(if compact { 0.0 } else { 8.0 }) => {
                        ConstrainedBox::new().min_width(24.0).max_width(24.0) => {
                            Icon::new(LucideIcon::Plus).size(20.0).color(color);
                        }
                        if !compact {
                            Expanded::new() => {
                                TextLabel::new("New terminal").color(color).ellipsis();
                            }
                        }
                    }
                }
            })
            .build(cx)
    }
}

fn close_tab_button(
    cx: &mut BuildCx,
    id: crate::tab_manager::TabId,
    row_state: InteractionState,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> impl Component + use<> {
    let mut style = rail_style(cx, true);
    style.min_height = 24.0;
    style.horizontal_padding = 0.0;
    style.radius = 4.0;
    let foreground = style.normal.foreground;
    let visible = row_state.selected || row_state.hovered || row_state.focused;
    ConstrainedBox::new()
        .min_width(20.0)
        .max_width(20.0)
        .max_height(24.0)
        .child(
            IconButton::new(LucideIcon::X, "Close tab")
                .style(style)
                .icon_size(16.0)
                .color_with(move |state| {
                    if visible || state.focused {
                        foreground
                    } else {
                        Color::TRANSPARENT
                    }
                })
                .on_click(move |_| dispatcher.dispatch(AppCommandRequest::close_rail(id))),
        )
}

fn terminal_panel(
    cx: &mut BuildCx,
    controller: &TabUiController,
    active_bridge: TerminalWidgetBridge,
    configuration_result: Option<String>,
) -> View {
    harbor_widget::view! { cx; Expanded::new() => {
        Column::new().cross_axis_alignment(Alignment::Stretch) => {
            Expanded::new() => {
                controller.allocation.observer() => {
                    TerminalDecorationPreset::container() => {
                        Focus::empty().handle(controller.terminal_focus) => {
                            terminal_widget(active_bridge);
                        }
                    }
                }
            }
            if let Some(message) = configuration_result {
                TextLabel::new(message);
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
    deny_label: &'static str,
    allow_label: &'static str,
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
                    Button::new(props.deny_label).on_click(move |_| {
                        cancelled.store(true, Ordering::SeqCst);
                    });
                    SizedBox::new(Size::new(12.0, 0.0));
                    Button::new(props.allow_label).on_click(move |_| {
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
        deny_label: "Cancel",
        allow_label: "Paste",
        wrapped_lines,
        scroll_offset,
        cancelled,
        confirmed,
        line_height,
    };
    move |cx: &mut BuildCx| confirmation_dialog(cx, &props)
}
/// Independent, per-request clipboard-write confirmation; never grants paste permission.
pub fn build_clipboard_confirmation_root(
    header_text: String,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
) -> impl Component {
    let props = ConfirmationDialogProps {
        header_text,
        deny_label: "Deny this request",
        allow_label: "Allow this request",
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

pub(super) fn abbreviation(snapshot: &TabSnapshot) -> String {
    (snapshot.id.get() % 10).to_string()
}
