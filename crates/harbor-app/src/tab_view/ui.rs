//! Tab rail presentation and tab-specific controls.

use super::{RailPresentation, TabCommand, TabUiController, TabUiState};
use crate::{
    command::{AppCommand, AppCommandRequest},
    tab_manager::TabSnapshot,
};
use harbor_widget::{
    Align, Alignment2D, Axis, BoxDecoration, ClipBehavior, Column, ComponentExt as _,
    ConstrainedBox, ControlColors, ControlStyle, CrossAxisAlignment, DecoratedBox, Dispatcher,
    Expanded, Flex, FlexOptions, Icon, IconButton, Insets, InteractionState, InteractiveRegion,
    LucideIcon, Row, ScrollView, Separator, SizedBox, Stack,
    layout::Size,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{Tooltip, padding::Padding, text_label::TextLabel},
};

pub(crate) fn tab_rail(
    cx: &mut BuildCx,
    state: &TabUiState,
    controller: &TabUiController,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> View {
    let compact = state.presentation == RailPresentation::Compact;
    let actions = [
        (
            LucideIcon::Plus,
            "New terminal",
            AppCommandRequest::rail(TabCommand::New),
            "new-terminal",
        ),
        (
            LucideIcon::ChevronDown,
            "New from profile",
            AppCommandRequest::shortcut(AppCommand::OpenProfileChooser),
            "new-profile",
        ),
        (
            LucideIcon::Check,
            "Save preset",
            AppCommandRequest::shortcut(AppCommand::SaveCurrentSession),
            "save-profile",
        ),
    ]
    .map(|(icon, label, request, key)| {
        rail_action(cx, icon, label, request, dispatcher.clone()).keyed(key)
    });
    let action_options = FlexOptions {
        gap: 4.0,
        cross_axis_alignment: CrossAxisAlignment::Center,
        ..Default::default()
    };
    let foreground = cx.theme().button.normal.foreground;
    harbor_widget::view! { SizedBox::width(state.presentation.width()) => {
        Column::with_options(FlexOptions { cross_axis_alignment: CrossAxisAlignment::Stretch, ..Default::default() }) => {
            Expanded::new() => {
                ScrollView::new().controller(controller.scroll.clone()) => {
                    Padding::new(Insets { top: 0.0, right: 6.0, bottom: 4.0, left: 6.0 }) => {
                        Column::with_options(FlexOptions { gap: 4.0, cross_axis_alignment: CrossAxisAlignment::Stretch, ..Default::default() }) => {
                            for snapshot in state.snapshots.iter() {
                                tab_row(cx, snapshot, state.presentation, controller, dispatcher.clone());
                            }
                        }
                    }
                }
            }
            Padding::new(Insets { top: 0.0, right: 6.0, bottom: 0.0, left: 6.0 }) => {
                Separator::horizontal().color(Color { a: 0.12, ..foreground }).expect("valid separator color");
            }
            Padding::all(6.0) => {
                // Keep the same keyed controls mounted when only orientation changes.
                Flex::with_options(if compact { Axis::Vertical } else { Axis::Horizontal }, action_options).expect("valid action layout") => {
                    for action in actions { action; }
                }
            }
        }
    } }
}

fn tab_row(
    cx: &mut BuildCx,
    snapshot: &TabSnapshot,
    presentation: RailPresentation,
    controller: &TabUiController,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> impl Component + use<> {
    let id = snapshot.id;
    let title = snapshot.title.clone();
    let compact = presentation == RailPresentation::Compact;
    let unread = snapshot.unread;
    let icon = if unread {
        LucideIcon::Dot
    } else {
        LucideIcon::Terminal
    };
    let select_dispatcher = dispatcher.clone();
    let region = InteractiveRegion::new()
        .style(rail_style(cx, compact)).expect("valid control style")
        .selected(snapshot.active)
        .focus_handle(controller.tab_focus(id).expect("live tab has a focus handle"))
        .on_activate(move |_| {
            select_dispatcher.dispatch(AppCommandRequest::rail(TabCommand::Activate(id)));
        })
        .child_with(move |cx, state| {
            let color = rail_style(cx, compact).resolve_state(&state).foreground;
            if compact {
                return harbor_widget::view! {
                    Align::new(Alignment2D::CENTER) => {
                        SizedBox::new(Size::new(24.0, 24.0)) => {
                            Stack::new() => {
                                Icon::new(LucideIcon::Terminal).size(20.0).expect("valid icon size").foreground(color).expect("valid color");
                                if unread {
                                    Padding::new(Insets { top: 0.0, right: 0.0, bottom: 12.0, left: 12.0 }) => {
                                        Icon::new(LucideIcon::Dot).size(12.0).expect("valid icon size").foreground(color).expect("valid color");
                                    }
                                }
                            }
                        }
                    }
                };
            }
            harbor_widget::view! {
                Row::with_options(FlexOptions { gap: 8.0, cross_axis_alignment: CrossAxisAlignment::Center, ..Default::default() }) => {
                    SizedBox::width(24.0) => {
                        Icon::new(icon).size(20.0).expect("valid icon size").foreground(color).expect("valid color");
                    }
                    Expanded::new() => {
                        DecoratedBox::new(BoxDecoration::new()).clip_behavior(ClipBehavior::HardEdge) => {
                            TextLabel::new(title.clone()).foreground(color).expect("valid color").ellipsis();
                        }
                    }
                    close_tab_button(cx, id, state, dispatcher.clone());
                }
            }
        });
    Tooltip::new(snapshot.title.clone())
        .child(region)
        .keyed(format!("terminal-tab-{id}"))
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

fn rail_action(
    cx: &BuildCx,
    icon: LucideIcon,
    label: &'static str,
    request: AppCommandRequest,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> impl Component + use<> {
    let mut style = rail_style(cx, true);
    style.min_height = 32.0;
    style.horizontal_padding = 0.0;
    Tooltip::new(label).child(
        SizedBox::new(Size::new(32.0, 32.0))
            .expect("valid action size")
            .child(
                IconButton::new(icon, label)
                    .style(style)
                    .expect("valid control style")
                    .icon_size(18.0)
                    .expect("valid icon size")
                    .on_click(move |_| dispatcher.dispatch(request)),
            ),
    )
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
    // Width is fixed; height remains a cap, not a second fixed dimension.
    ConstrainedBox::new()
        .min_width(20.0)
        .max_width(20.0)
        .max_height(24.0)
        .child(
            IconButton::new(LucideIcon::X, "Close tab")
                .style(style)
                .expect("valid control style")
                .icon_size(16.0)
                .expect("valid size")
                .foreground_with(move |state| {
                    if visible || state.focused {
                        foreground
                    } else {
                        Color::TRANSPARENT
                    }
                })
                .on_click(move |_| dispatcher.dispatch(AppCommandRequest::close_rail(id))),
        )
}
