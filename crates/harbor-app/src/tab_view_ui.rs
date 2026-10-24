//! Tab rail presentation and tab-specific controls.

use super::{RailPresentation, TabCommand, TabUiController, TabUiState};
use crate::{command::AppCommandRequest, tab_manager::TabSnapshot};
use harbor_widget::{
    Alignment, BoxDecoration, ClipBehavior, Column, ComponentExt as _, ConstrainedBox,
    ControlColors, ControlStyle, DecoratedBox, Dispatcher, Expanded, FlexOptions, Icon, IconButton,
    Insets, InteractionState, InteractiveRegion, LucideIcon, Row, ScrollView, SizedBox,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{padding::Padding, text_label::TextLabel},
};
pub(crate) fn abbreviation(snapshot: &TabSnapshot) -> String {
    (snapshot.id.get() % 10).to_string()
}
pub(crate) fn tab_rail(
    cx: &mut BuildCx,
    state: &TabUiState,
    controller: &TabUiController,
    dispatcher: Dispatcher<AppCommandRequest>,
) -> View {
    harbor_widget::view! { SizedBox::width(state.presentation.width()) => {
            ScrollView::new().controller(controller.scroll.clone()) => {
                Padding::new(Insets { top: 0.0, right: 6.0, bottom: 4.0, left: 6.0 }) => {
                    Column::with_options(FlexOptions { gap: 4.0, cross_axis_alignment: Alignment::Stretch, ..Default::default() }) => {
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
            harbor_widget::view! { Row::with_options(FlexOptions { gap: if compact { 2.0 } else { 8.0 }, cross_axis_alignment: Alignment::Center, ..Default::default() }) => {
                    if !compact {
                        SizedBox::width(24.0) => {
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
    // The region is a deferred component of its own keyed fiber; the padding
    // wrapper that used to isolate it from the workspace's positional hooks
    // is no longer needed (ADR 0079).
    region.keyed(format!("terminal-tab-{id}"))
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
                harbor_widget::view! { Row::with_options(FlexOptions { gap: if compact { 0.0 } else { 8.0 }, cross_axis_alignment: Alignment::Center, ..Default::default() }) => {
                        SizedBox::width(24.0) => {
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
    // Width is fixed; height remains a cap, not a second fixed dimension.
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
