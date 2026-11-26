//! In-window session search bar popup and shared translucent popup icon button.

use super::*;
use harbor_widget::{BorderRadius, FlexOptions, Icon, InteractionState, InteractiveRegion};

// Fixed chrome height also reserves space for configuration notices below search.
pub(super) const SEARCH_PANEL_HEIGHT: f32 = 48.0;
pub(super) const SEARCH_PANEL_INSET: f32 = 12.0;

#[derive(Clone)]
pub(super) struct SearchUiState {
    pub(super) source: crate::tab_manager::SessionId,
    pub(super) input: TextEditingController,
    pub(super) status: String,
    pub(super) focus: FocusHandle,
    pub(super) cancellation:
        std::rc::Rc<std::cell::RefCell<Option<crate::search::SearchCancellation>>>,
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

pub(super) fn search_bar(
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
        .expect("valid field padding")
        .placeholder("Find…")
        .focus_handle(controller.search_focus)
        .on_change(move |_| {
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
        .on_submit(move |modifiers| {
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
    let button = |icon, command| {
        let dispatcher = controller.store.dispatcher();
        popup_icon_button(icon, style.clone()).on_activate(move |_| {
            dispatcher.dispatch(AppCommandRequest::popup(command, generation))
        })
    };
    let status = search.status.clone();
    let content = harbor_widget::view! {
        Padding::all(8.0) => {
            Row::with_options(FlexOptions { gap: 4.0, cross_axis_alignment: CrossAxisAlignment::Center, ..Default::default() }) => {
                Expanded::new() => { entry; }
                // Status text changes frequently; reserve its slot instead of resizing the editor.
                SizedBox::width(120.0) => {
                    harbor_widget::widgets::text_label::TextLabel::new(status)
                        .foreground(cx.theme().button.normal.foreground).expect("valid color").ellipsis();
                }
                button(LucideIcon::ChevronUp, AppCommand::SearchPrevious);
                button(LucideIcon::ChevronDown, AppCommand::SearchNext);
                button(LucideIcon::X, AppCommand::SearchClose);
            }
        }
    };
    let card = appearance.panel(cx, content);
    Portal::new()
        .child(harbor_widget::view! {
            Align::new(Alignment2D::TOP_RIGHT).insets(Insets::all(SEARCH_PANEL_INSET)) => {
                SizedBox::new(harbor_widget::layout::Size::new(440.0, SEARCH_PANEL_HEIGHT)) => {
                    card;
                }
            }
        })
        .keyed(format!("session-search-{generation}"))
        .build(cx)
}

// Keep the IconButton hit box and focus behavior, but include its padding inside
// the material so hover feedback covers the entire control, not just the glyph.
pub(super) fn popup_icon_button(
    icon: LucideIcon,
    mut style: harbor_widget::theme::ControlStyle,
) -> InteractiveRegion {
    let width = style.icon_size + style.horizontal_padding * 2.0;
    style.horizontal_padding = 0.0;
    style.radius = 12.0; // Match the search panel's outer corners.
    InteractiveRegion::new()
        .style(style.clone())
        .expect("valid control style")
        .child_with(move |cx, state| {
            let content = Icon::new(icon)
                .size(style.icon_size)
                .expect("valid icon size")
                .foreground(style.resolve_state(&state).foreground)
                .expect("valid color")
                .build(cx);
            // Center Icon's fixed-size artwork in the actual control allocation.
            // A shorter parent (e.g. a palette field) must not squeeze the glyph
            // with padding computed from the search button's 32dp minimum.
            let content = SizedBox::width(width)
                .unwrap()
                .child(
                    harbor_widget::ConstrainedBox::new()
                        .min_height(style.min_height)
                        .child(content)
                        .build(cx),
                )
                .build(cx);
            if state.disabled || !(state.hovered || state.pressed) {
                return content;
            }
            // Use the palette's selected-row treatment rather than its faint
            // hover-only tint. This is a visual state, not a selected command.
            let glass = GlassAppearance::command_palette_row(
                style.normal.foreground,
                InteractionState {
                    selected: true,
                    ..state
                },
            )
            .unwrap();
            GlassAppearance {
                radius: BorderRadius::all(style.radius).unwrap(),
                ..glass
            }
            .panel(cx, content)
        })
}
