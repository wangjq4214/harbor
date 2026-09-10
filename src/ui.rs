//! Declarative widget definitions for Harbor's main workspace and dialogs.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use harbor_widget::{
    Actions, Button, Column, ComponentExt as _, ConstrainedBox, Dispatcher, Expanded, Focus,
    FocusScope, IconButton, KeyChord, Row, ScrollArea, Separator, Shortcuts,
    input::event::{Key, Modifiers},
    layout::Size,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::{
        padding::Padding, preview_pane::PreviewPane, sized_box::SizedBox, text_label::TextLabel,
    },
};

use super::{RailPresentation, TabCommand, TabCommandRequest, TabUiController};
use crate::{
    tab_manager::{TabIndex, TabSnapshot},
    terminal_view::TerminalDecorationPreset,
};

pub(crate) const CONFIRMATION_PREVIEW_VISIBLE_LINES: usize = 12;

#[derive(Clone)]
pub(crate) struct TabWorkspace {
    controller: TabUiController,
    backdrop_available: bool,
    backdrop_fallback: [f32; 3],
}

impl TabWorkspace {
    #[allow(dead_code)]
    pub(crate) fn new(controller: TabUiController, backdrop_available: bool) -> Self {
        Self::with_fallback(
            controller,
            backdrop_available,
            harbor_config::WindowBackdropStyle::default().fallback,
        )
    }

    pub(crate) fn with_fallback(
        controller: TabUiController,
        backdrop_available: bool,
        backdrop_fallback: [f32; 3],
    ) -> Self {
        Self {
            controller,
            backdrop_available,
            backdrop_fallback,
        }
    }
}

impl Component for TabWorkspace {
    fn build(&self, cx: &mut BuildCx) -> View {
        let state = self.controller.store.watch(cx).clone();
        let dispatcher = self.controller.store.dispatcher();
        let action_dispatcher = dispatcher.clone();
        let new_dispatcher = dispatcher.clone();
        let active_bridge = state
            .active_bridge
            .expect("workspace has an active terminal until window exit");
        let root = root_padding(self.backdrop_available, self.backdrop_fallback);
        let shortcuts = shortcuts();
        let actions = Actions::handler(move |request| action_dispatcher.dispatch(request));

        harbor_widget::view!(cx, root => {
            FocusScope::new() => {
                actions => {
                    shortcuts => {
                        Row::new() => {
                            ConstrainedBox::new()
                                .min_width(state.presentation.width())
                                .max_width(state.presentation.width()) => {
                                Column::new() => {
                                    Expanded::new() => {
                                        ScrollArea::new()
                                            .controller(self.controller.scroll.clone()) => {
                                            Column::new() => {
                                                for snapshot in state.snapshots.iter() {
                                                    Row::new()
                                                        .keyed(format!("terminal-tab-{}", snapshot.id)) => {
                                                        Expanded::new() => {
                                                            Focus::empty()
                                                                .handle(self.controller
                                                                    .tab_focus(snapshot.id)
                                                                    .expect("live tab has a focus handle")) => {
                                                                select_tab_button(
                                                                    snapshot,
                                                                    state.presentation,
                                                                    dispatcher.clone(),
                                                                ) => {}
                                                            }
                                                        }
                                                        close_tab_button(snapshot, dispatcher.clone()) => {}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    match state.presentation {
                                        RailPresentation::Expanded => {
                                            Button::new("New terminal").on_click(move |_| {
                                                new_dispatcher.dispatch(
                                                    TabCommandRequest::rail(TabCommand::New)
                                                );
                                            }) => {}
                                        },
                                        RailPresentation::Compact => {
                                            IconButton::new("+", "New terminal").on_click(move |_| {
                                                new_dispatcher.dispatch(
                                                    TabCommandRequest::rail(TabCommand::New)
                                                );
                                            }) => {}
                                        }
                                    }
                                }
                            }
                            Separator::vertical() => {}
                            Expanded::new() => {
                                self.controller.allocation.observer() => {
                                    Focus::empty().handle(self.controller.terminal_focus) => {
                                        TerminalDecorationPreset::container() => {
                                            { active_bridge }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }
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
pub(crate) struct ConfirmationDialog {
    header_text: String,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
}

impl Component for ConfirmationDialog {
    fn build(&self, cx: &mut BuildCx) -> View {
        let cancelled = Arc::clone(&self.cancelled);
        let confirmed = Arc::clone(&self.confirmed);
        harbor_widget::view!(cx, FocusScope::new() => {
            Padding::new(24.0, 16.0, 24.0, 16.0) => {
                Column::new() => {
                    TextLabel::new(self.header_text.clone()) => {}
                    SizedBox::new(Size::new(0.0, 8.0)) => {}
                    PreviewPane::new(
                        self.wrapped_lines.clone(),
                        Arc::clone(&self.scroll_offset),
                        self.line_height,
                        CONFIRMATION_PREVIEW_VISIBLE_LINES,
                    ) => {}
                    SizedBox::new(Size::new(0.0, 12.0)) => {}
                    Row::new() => {
                        Button::new("Cancel").on_click(move |_| {
                            cancelled.store(true, Ordering::SeqCst);
                        }) => {}
                        SizedBox::new(Size::new(12.0, 0.0)) => {}
                        Button::new("Paste").on_click(move |_| {
                            confirmed.store(true, Ordering::SeqCst);
                        }) => {}
                    }
                }
            }
        })
    }
}

pub(crate) fn build_confirmation_root(
    line_count: usize,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
) -> ConfirmationDialog {
    ConfirmationDialog {
        header_text: format!("Paste {line_count} lines?"),
        wrapped_lines,
        scroll_offset,
        cancelled,
        confirmed,
        line_height,
    }
}

fn shortcuts() -> Shortcuts<TabCommandRequest> {
    let shortcuts = Shortcuts::empty()
        .bind(
            KeyChord::new(Key::Character('t'), ctrl()),
            TabCommandRequest::shortcut(TabCommand::New),
        )
        .bind(
            KeyChord::new(Key::Character('w'), ctrl()),
            TabCommandRequest::shortcut(TabCommand::CloseActive),
        )
        .bind(
            KeyChord::new(Key::Tab, ctrl()),
            TabCommandRequest::shortcut(TabCommand::Next),
        )
        .bind(
            KeyChord::new(
                Key::Tab,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Modifiers::default()
                },
            ),
            TabCommandRequest::shortcut(TabCommand::Previous),
        );
    (1..=9).fold(shortcuts, |shortcuts, index| {
        let digit = char::from_digit(index as u32, 10).expect("numeric shortcut is a digit");
        shortcuts.bind(
            KeyChord::new(Key::Character(digit), ctrl()),
            TabCommandRequest::shortcut(TabCommand::Numeric(TabIndex::from_valid_u8(index as u8))),
        )
    })
}

pub(super) fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Modifiers::default()
    }
}

fn select_tab_button(
    snapshot: &TabSnapshot,
    presentation: RailPresentation,
    dispatcher: Dispatcher<TabCommandRequest>,
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
            dispatcher.dispatch(TabCommandRequest::rail(TabCommand::Activate(id)));
        })
}

fn close_tab_button(
    snapshot: &TabSnapshot,
    dispatcher: Dispatcher<TabCommandRequest>,
) -> IconButton {
    let id = snapshot.id;
    IconButton::new("×", format!("Close {}", snapshot.title)).on_click(move |_| {
        dispatcher.dispatch(TabCommandRequest::rail(TabCommand::Close(id)));
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
