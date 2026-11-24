//! Shared application confirmation composition for paste and clipboard-write requests.

use crate::appearance::{
    GlassAppearance,
    design_system::{self as design, ControlRole, TextRole},
};
use crate::command::{AppCommand, AppCommandRequest, ResolvedKeybindings};
use crate::workspace::WorkspaceUiController;
use harbor_terminal::safe_preview_line;
use harbor_widget::{
    ActionHandler, ActionOutcome, Align, Alignment2D, Border, BorderRadius, BoxDecoration, Button,
    ClipBehavior, Column, ComponentExt as _, ConstrainedBox, CrossAxisAlignment, DecoratedBox,
    Expanded, Flexible, FocusHandle, FocusScope, Icon, Insets, KeyChord, LucideIcon,
    MainAxisAlignment, Padding, Portal, Row, ScrollController, ScrollView, Separator, ShortcutMap,
    Stack, ThemeProvider,
    input::event::{Key, Modifiers, UiEvent},
    input::event_ctx::{EventCtx, EventHandled},
    layout::{BoxConstraints, Rect, Size},
    scene::primitive::{Color, Primitive},
    text::TextMetrics,
    view::{BuildCx, Component, PrimitiveLeaf, View},
    widgets::{sized_box::SizedBox, text_label::TextLabel},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

pub mod preview;
use preview::{PreviewContent, PreviewPane};

pub const CONFIRMATION_PREVIEW_VISIBLE_LINES: usize = 12;

/// Business kind of the active in-window confirmation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmationKind {
    Paste,
    ClipboardWrite,
}

/// Application-owned presentation data for an in-window confirmation overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmationRequest {
    pub kind: ConfirmationKind,
    pub title: String,
    pub preview_text: String,
    pub cancel_label: &'static str,
    pub confirm_label: &'static str,
}

impl ConfirmationRequest {
    pub fn paste(line_count: usize, preview_text: String) -> Self {
        Self {
            kind: ConfirmationKind::Paste,
            title: format!("Paste {line_count} lines?"),
            preview_text,
            cancel_label: "Cancel",
            confirm_label: "Paste",
        }
    }

    pub fn clipboard_write(title: String, preview_text: String) -> Self {
        Self {
            kind: ConfirmationKind::ClipboardWrite,
            title,
            preview_text,
            cancel_label: "Deny this request",
            confirm_label: "Allow this request",
        }
    }
}

#[derive(Clone)]
pub(crate) struct ConfirmationOverlayState {
    kind: ConfirmationKind,
    title: String,
    escaped_lines: Arc<Vec<String>>,
    cancel_label: &'static str,
    confirm_label: &'static str,
    scroll: ScrollController,
    cancel_focus: FocusHandle,
    confirm_focus: FocusHandle,
}

impl ConfirmationOverlayState {
    #[cfg(test)]
    pub(crate) fn scroll_controller(&self) -> ScrollController {
        self.scroll.clone()
    }
    pub(crate) fn new(request: ConfirmationRequest) -> Self {
        let escaped_lines: Vec<String> = request
            .preview_text
            .lines()
            .map(safe_preview_line)
            .collect();
        Self {
            kind: request.kind,
            title: request.title,
            escaped_lines: Arc::new(escaped_lines),
            cancel_label: request.cancel_label,
            confirm_label: request.confirm_label,
            scroll: ScrollController::new(),
            cancel_focus: FocusHandle::new(),
            confirm_focus: FocusHandle::new(),
        }
    }

    pub(crate) fn kind(&self) -> ConfirmationKind {
        self.kind
    }

    fn panel(&self, cx: &mut BuildCx, ui: &WorkspaceUiController, generation: u64) -> View {
        let dispatcher = ui.dispatcher();
        let cancel_dispatch = dispatcher.clone();
        let confirm_dispatch = dispatcher.clone();
        let row_height = PreviewContent::row_height(&harbor_widget::runtime::DEFAULT_TEXT_METRICS);

        let header = Row::new()
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(10.0)
            .unwrap()
            .push_child(
                Icon::new(LucideIcon::TriangleAlert)
                    .size(20.0)
                    .expect("valid warning icon size")
                    .foreground(design::WARNING)
                    .expect("valid warning color")
                    .keyed("confirmation-warning-icon")
                    .build(cx),
            )
            .push_child(
                Expanded::new()
                    .child(
                        design::text(self.title.clone(), TextRole::Title)
                            .keyed("confirmation-title"),
                    )
                    .build(cx),
            )
            .keyed("confirmation-header")
            .build(cx);

        let preview_content = PreviewContent::new(
            Arc::clone(&self.escaped_lines),
            self.scroll.clone(),
            design::FOREGROUND,
            Color {
                a: 0.55,
                ..design::SECONDARY
            },
            design::DIVIDER,
            Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.04,
            },
        );
        let preview_scroll = ScrollView::new()
            .controller(self.scroll.clone())
            .line_step(row_height)
            .shrink_wrap(true)
            .child(preview_content)
            .keyed("confirmation-preview-scroll")
            .build(cx);

        // Keep `ClipBehavior::None` on the outer border box so the glass panel
        // (clip 0) and `ScrollView` (clip 1) stay within the 2-clip exact GPU budget.
        let preview_frame = DecoratedBox::new(
            BoxDecoration::new()
                .try_color(Color {
                    a: 0.24,
                    ..Color::BLACK
                })
                .unwrap()
                .border(
                    Border::all(
                        Color {
                            a: 0.18,
                            ..Color::WHITE
                        },
                        1.0,
                    )
                    .unwrap(),
                )
                .border_radius(BorderRadius::all(7.0).unwrap()),
        )
        .clip_behavior(ClipBehavior::None)
        .child(
            Padding::new(Insets {
                top: 4.0,
                right: 1.0,
                bottom: 4.0,
                left: 1.0,
            })
            .unwrap()
            .child(preview_scroll)
            .build(cx),
        )
        .keyed("confirmation-preview-frame")
        .build(cx);

        let footer = Column::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(10.0)
            .unwrap()
            .push_child(
                Separator::horizontal()
                    .color(design::DIVIDER)
                    .unwrap()
                    .build(cx),
            )
            .push_child(
                Row::new()
                    .wrap(true)
                    .main_axis_alignment(MainAxisAlignment::End)
                    .gap(8.0)
                    .unwrap()
                    .push_child(View::deferred(
                        Button::text(self.cancel_label)
                            .style(design::control(cx, ControlRole::Secondary))
                            .unwrap()
                            .focus_handle(self.cancel_focus)
                            .on_click(move |_| {
                                cancel_dispatch.dispatch(AppCommandRequest::popup(
                                    AppCommand::CancelConfirmation,
                                    generation,
                                ));
                            })
                            .keyed("confirmation-cancel"),
                    ))
                    .push_child(View::deferred(
                        Button::text(self.confirm_label)
                            .style(design::control(cx, ControlRole::Primary))
                            .unwrap()
                            .focus_handle(self.confirm_focus)
                            .on_click(move |_| {
                                confirm_dispatch.dispatch(AppCommandRequest::popup(
                                    AppCommand::ConfirmConfirmation,
                                    generation,
                                ));
                            })
                            .keyed("confirmation-confirm"),
                    ))
                    .build(cx),
            )
            .keyed("confirmation-footer")
            .build(cx);

        let inner = Padding::new(Insets {
            left: design::PANEL_PADDING,
            right: design::PANEL_PADDING,
            top: 12.0,
            bottom: 12.0,
        })
        .unwrap()
        .child(
            Column::new()
                .shrink_wrap(true)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(10.0)
                .unwrap()
                .push_child(header)
                .push_child(Flexible::new().child(preview_frame))
                .push_child(footer)
                .build(cx),
        )
        .build(cx);

        let card = GlassAppearance::command_palette().panel(cx, inner);

        let scroll = self.scroll.clone();
        let action_dispatch = dispatcher;
        let actions = ActionHandler::new(move |action: OverlayShortcut| {
            match action {
                OverlayShortcut::Cancel => {
                    action_dispatch.dispatch(AppCommandRequest::popup(
                        AppCommand::CancelConfirmation,
                        generation,
                    ));
                }
                OverlayShortcut::Confirm => {
                    action_dispatch.dispatch(AppCommandRequest::popup(
                        AppCommand::ConfirmConfirmation,
                        generation,
                    ));
                }
                OverlayShortcut::ScrollLines(lines) => {
                    scroll.scroll_by(lines * row_height);
                }
                OverlayShortcut::ScrollPage(direction) => {
                    let viewport = scroll.metrics().viewport_extent();
                    let step = if viewport > 0.0 {
                        (viewport - row_height).max(row_height)
                    } else {
                        row_height * 8.0
                    };
                    scroll.scroll_by(direction * step);
                }
            }
            ActionOutcome::Consumed
        });

        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let shortcuts = actions
            .child(
                ShortcutMap::new()
                    .bind(
                        KeyChord::new(Key::Escape, Modifiers::default()),
                        OverlayShortcut::Cancel,
                    )
                    .bind(
                        KeyChord::new(Key::Character('n'), Modifiers::default()),
                        OverlayShortcut::Cancel,
                    )
                    .bind(
                        KeyChord::new(Key::Character('n'), shift),
                        OverlayShortcut::Cancel,
                    )
                    .bind(
                        KeyChord::new(Key::Character('y'), Modifiers::default()),
                        OverlayShortcut::Confirm,
                    )
                    .bind(
                        KeyChord::new(Key::Character('y'), shift),
                        OverlayShortcut::Confirm,
                    )
                    .bind(
                        KeyChord::new(Key::ArrowUp, Modifiers::default()),
                        OverlayShortcut::ScrollLines(-1.0),
                    )
                    .bind(
                        KeyChord::new(Key::ArrowDown, Modifiers::default()),
                        OverlayShortcut::ScrollLines(1.0),
                    )
                    .bind(
                        KeyChord::new(Key::PageUp, Modifiers::default()),
                        OverlayShortcut::ScrollPage(-1.0),
                    )
                    .bind(
                        KeyChord::new(Key::PageDown, Modifiers::default()),
                        OverlayShortcut::ScrollPage(1.0),
                    )
                    .child(card),
            )
            .build(cx);

        let aligned = Align::new(Alignment2D::CENTER)
            .insets(Insets::all(12.0))
            .unwrap()
            .child(
                ConstrainedBox::new()
                    .max_width(600.0)
                    .max_height(520.0)
                    .child(FocusScope::new().modal(true).child(shortcuts).build(cx))
                    .build(cx),
            )
            .build(cx);

        Portal::new()
            .child(
                Stack::new()
                    .push_child(View::leaf(ConfirmationBarrier))
                    .push_child(aligned)
                    .build(cx),
            )
            .keyed(format!("confirmation-{generation}"))
            .build(cx)
    }
}

#[derive(Clone, Copy)]
enum OverlayShortcut {
    Cancel,
    Confirm,
    ScrollLines(f32),
    ScrollPage(f32),
}

/// Transparent modal pointer barrier: leaves the terminal backdrop undimmed while
/// preventing outside clicks/scrolls from activating background panes or rails.
#[derive(Clone)]
struct ConfirmationBarrier;

impl PrimitiveLeaf for ConfirmationBarrier {
    fn intrinsic_size(&self, constraints: BoxConstraints, _metrics: &TextMetrics) -> Size {
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            0.0
        };
        let height = if constraints.max.height.is_finite() {
            constraints.max.height
        } else {
            0.0
        };
        constraints.constrain(Size::new(width, height))
    }

    fn paint_primitives(&self, _rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        Vec::new()
    }

    fn handle_event(&self, event: &UiEvent, _ctx: &mut EventCtx, _rect: Rect) -> EventHandled {
        match event {
            UiEvent::Pointer(_) | UiEvent::PointerBoundary(_) => EventHandled::Handled,
            _ => EventHandled::Ignored,
        }
    }
}

impl crate::popups::Popup for ConfirmationOverlayState {
    fn build(
        &self,
        cx: &mut BuildCx,
        ui: &WorkspaceUiController,
        _: &ResolvedKeybindings,
        generation: u64,
    ) -> View {
        let overlay = self.clone();
        let ui = ui.clone();
        ThemeProvider::new(design::theme(cx.theme()))
            .expect("valid application theme")
            .child(
                (move |cx: &mut BuildCx| overlay.panel(cx, &ui, generation))
                    .keyed(format!("confirmation-owner-{generation}")),
            )
            .build(cx)
    }

    fn focus(&self) -> FocusHandle {
        self.cancel_focus
    }

    fn close(&self) {}

    fn blocks_shortcuts(&self) -> bool {
        true
    }
}

// ── Legacy secondary-window helpers (retired in T0003) ──────────────────────

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

impl Component for ConfirmationDialogProps {
    fn build(&self, _cx: &mut BuildCx) -> View {
        let cancelled = Arc::clone(&self.cancelled);
        let confirmed = Arc::clone(&self.confirmed);
        harbor_widget::view! { FocusScope::new() => {
            Padding::new(harbor_widget::Insets { top: 24.0, right: 16.0, bottom: 24.0, left: 16.0 }) => {
                Column::new() => {
                    TextLabel::new(self.header_text.clone());
                    SizedBox::new(Size::new(0.0, 8.0));
                    PreviewPane::new(
                        self.wrapped_lines.clone(),
                        Arc::clone(&self.scroll_offset),
                        self.line_height,
                        CONFIRMATION_PREVIEW_VISIBLE_LINES,
                    );
                    SizedBox::new(Size::new(0.0, 12.0));
                    Row::new() => {
                        Button::text(self.deny_label).on_click(move |_| {
                            cancelled.store(true, Ordering::SeqCst);
                        });
                        SizedBox::new(Size::new(12.0, 0.0));
                        Button::text(self.allow_label).on_click(move |_| {
                            confirmed.store(true, Ordering::SeqCst);
                        });
                    }
                }
            }
        } }
    }
}

pub fn build_confirmation_root(
    line_count: usize,
    wrapped_lines: Vec<String>,
    scroll_offset: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    confirmed: Arc<AtomicBool>,
    line_height: f32,
) -> impl Component {
    ConfirmationDialogProps {
        header_text: format!("Paste {line_count} lines?"),
        deny_label: "Cancel",
        allow_label: "Paste",
        wrapped_lines,
        scroll_offset,
        cancelled,
        confirmed,
        line_height,
    }
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
    ConfirmationDialogProps {
        header_text,
        deny_label: "Deny this request",
        allow_label: "Allow this request",
        wrapped_lines,
        scroll_offset,
        cancelled,
        confirmed,
        line_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab_manager::{PaneBinding, PaneId, SessionId, TabId, TabSnapshot};
    use crate::terminal_view::TerminalWidgetBridge;
    use crate::workspace::tab_workspace;
    use harbor_terminal::Terminal;
    use harbor_widget::{
        fiber::FiberId,
        input::event::{Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase},
        layout::Point,
        renderer::Viewport,
        runtime::Runtime,
        scene::primitive::ExternalDrawId,
    };
    use std::{sync::Mutex, time::Instant};

    #[allow(clippy::arc_with_non_send_sync)]
    fn mounted_workspace(width: u32, height: u32) -> (WorkspaceUiController, Runtime) {
        let snapshots = vec![TabSnapshot {
            id: TabId(1),
            binding: PaneBinding {
                workspace: TabId(1),
                pane: PaneId(1),
                session: SessionId(1),
            },
            title: "Terminal 1".into(),
            unread: false,
            draw_id: ExternalDrawId::new(1),
            working_directory: None,
            shell_integration: None,
            active: true,
        }];
        let bridge = TerminalWidgetBridge::new(
            ExternalDrawId::new(1),
            Arc::new(Mutex::new(Terminal::new_headless(4, 20))),
            Arc::new(AtomicBool::new(false)),
        );
        let ui = WorkspaceUiController::new(snapshots, bridge, width as f64);
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(width, height, 1.0));
        runtime.set_root(tab_workspace(ui.clone(), false));
        runtime.update(Instant::now());
        (ui, runtime)
    }

    fn press(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
        runtime.update(Instant::now());
    }

    fn fiber_ids(runtime: &Runtime) -> Vec<FiberId> {
        let mut pending = vec![runtime.root_id().unwrap()];
        let mut ids = Vec::new();
        while let Some(id) = pending.pop() {
            let fiber = runtime.arena().get(id).unwrap();
            pending.extend_from_slice(fiber.children());
            ids.push(id);
        }
        ids
    }

    fn keyed_rect(runtime: &Runtime, key: &str) -> Rect {
        fiber_ids(runtime)
            .into_iter()
            .find_map(|id| {
                let fiber = runtime.arena().get(id).unwrap();
                fiber
                    .key()
                    .is_some_and(|candidate| candidate.text() == key)
                    .then(|| fiber.layout_rect().unwrap())
            })
            .unwrap_or_else(|| panic!("missing keyed node {key}"))
    }

    #[test]
    fn overlay_defaults_focus_to_cancel_or_deny_and_preserves_shortcuts_and_generation() {
        for request in [
            ConfirmationRequest::paste(3, "one\ntwo\nthree".into()),
            ConfirmationRequest::clipboard_write(
                "Tab 1 requests clipboard write (5 bytes)".into(),
                "hello".into(),
            ),
        ] {
            let expected_kind = request.kind;
            let (ui, mut runtime) = mounted_workspace(900, 600);
            let (generation, focus) = ui.open_confirmation(request);
            assert_eq!(ui.confirmation_kind(), Some(expected_kind));
            assert_eq!(ui.confirmation_generation(), Some(generation));
            runtime.request_focus(&focus);
            runtime.update(Instant::now());

            // Enter initially triggers Cancel/Deny, never Confirm/Allow.
            press(&mut runtime, Key::Enter, Modifiers::default());
            let actions = ui.drain_actions();
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].command, AppCommand::CancelConfirmation);
            assert!(ui.popup_request_is_current(actions[0]));

            // Space initially triggers Cancel/Deny too.
            press(&mut runtime, Key::Space, Modifiers::default());
            let actions = ui.drain_actions();
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].command, AppCommand::CancelConfirmation);

            // Tab moves focus to Paste/Allow; Enter triggers Confirm.
            press(&mut runtime, Key::Tab, Modifiers::default());
            press(&mut runtime, Key::Enter, Modifiers::default());
            let actions = ui.drain_actions();
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].command, AppCommand::ConfirmConfirmation);

            // Shift+Tab returns to Cancel/Deny.
            press(
                &mut runtime,
                Key::Tab,
                Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            );
            press(&mut runtime, Key::Enter, Modifiers::default());
            assert_eq!(
                ui.drain_actions()[0].command,
                AppCommand::CancelConfirmation
            );

            // Escape, n, N cancel; y, Y confirm.
            for (key, shift, expected) in [
                (Key::Escape, false, AppCommand::CancelConfirmation),
                (Key::Character('n'), false, AppCommand::CancelConfirmation),
                (Key::Character('n'), true, AppCommand::CancelConfirmation),
                (Key::Character('y'), false, AppCommand::ConfirmConfirmation),
                (Key::Character('y'), true, AppCommand::ConfirmConfirmation),
            ] {
                press(
                    &mut runtime,
                    key,
                    Modifiers {
                        shift,
                        ..Modifiers::default()
                    },
                );
                let actions = ui.drain_actions();
                assert_eq!(
                    actions.len(),
                    1,
                    "expected action for {key:?} shift={shift}"
                );
                assert_eq!(actions[0].command, expected);
            }

            // Workspace shortcuts are suppressed while confirmation is active.
            press(
                &mut runtime,
                Key::Character('t'),
                Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            );
            assert!(
                ui.drain_actions().is_empty(),
                "blocks_shortcuts must suppress workspace bindings"
            );

            // Stale generation actions are rejected after replacement.
            let stale_action =
                AppCommandRequest::popup(AppCommand::ConfirmConfirmation, generation);
            let (next_gen, next_focus) =
                ui.open_confirmation(ConfirmationRequest::paste(1, "x".into()));
            assert_ne!(next_gen, generation);
            assert!(!ui.popup_request_is_current(stale_action));
            assert!(!ui.close_confirmation(generation));
            assert!(ui.is_confirmation_current(next_gen));
            runtime.request_focus(&next_focus);
            runtime.update(Instant::now());
            assert!(ui.close_confirmation(next_gen));
            assert!(!ui.confirmation_is_open());
        }
    }

    #[test]
    fn overlay_displaces_search_and_scrolls_long_wrapped_preview_via_keys_and_wheel() {
        let (ui, mut runtime) = mounted_workspace(800, 480);
        ui.open_search(SessionId(1));
        runtime.request_focus(&ui.search_focus());
        runtime.update(Instant::now());
        assert!(ui.search_query().is_some());

        let long_text = (1..=40)
            .map(|i| format!("line-{i:02}-{}", "x".repeat(90)))
            .collect::<Vec<_>>()
            .join("\n");
        let (generation, focus) = ui.open_confirmation(ConfirmationRequest::paste(40, long_text));
        assert!(
            ui.search_query().is_none(),
            "opening confirmation closes Search"
        );
        runtime.request_focus(&focus);
        runtime.update(Instant::now());

        let scroll_rect = keyed_rect(&runtime, "confirmation-preview-scroll");
        let frame_rect = keyed_rect(&runtime, "confirmation-preview-frame");
        let header_rect = keyed_rect(&runtime, "confirmation-header");
        let footer_rect = keyed_rect(&runtime, "confirmation-footer");
        assert!(header_rect.max.y <= frame_rect.min.y);
        assert!(frame_rect.max.y <= footer_rect.min.y);

        // Verify ScrollView scrollbar thumb appears and wheel / keyboard scroll change offset.
        let scroll_controller = {
            let delta = runtime.pending_delta().unwrap();
            assert!(
                delta.added.iter().chain(&delta.modified).any(|item| {
                    matches!(
                        item.primitive,
                        Primitive::Icon {
                            icon: LucideIcon::TriangleAlert,
                            ..
                        }
                    )
                }),
                "warning icon must render in header"
            );
            assert!(
                delta.added.iter().chain(&delta.modified).any(|item| {
                    matches!(item.primitive, Primitive::RoundedBorder { rect, .. } if rect == frame_rect)
                }),
                "preview must render inside an inset rounded border"
            );
            ui.store_confirmation_scroll_for_test().unwrap()
        };
        assert!(
            scroll_controller.metrics().max_scroll_extent() > 0.0,
            "40 wrapped lines must overflow preview viewport"
        );
        assert_eq!(scroll_controller.metrics().offset(), 0.0);

        press(&mut runtime, Key::ArrowDown, Modifiers::default());
        assert!(scroll_controller.metrics().offset() > 0.0);
        let after_down = scroll_controller.metrics().offset();

        press(&mut runtime, Key::PageDown, Modifiers::default());
        assert!(scroll_controller.metrics().offset() > after_down);

        press(&mut runtime, Key::PageUp, Modifiers::default());
        press(&mut runtime, Key::ArrowUp, Modifiers::default());

        // Pointer wheel inside preview scrolls; outside click does not steal focus or hit terminal.
        let before_wheel = scroll_controller.metrics().offset();
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(
                (scroll_rect.min.x + scroll_rect.max.x) / 2.0,
                (scroll_rect.min.y + scroll_rect.max.y) / 2.0,
            ),
            PointerPhase::WheelLine { dx: 0.0, dy: -2.0 },
            PointerButton::Left,
            1,
        )));
        runtime.update(Instant::now());
        assert!(scroll_controller.metrics().offset() > before_wheel);

        let focused_before = runtime.input().focused();
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(5.0, 5.0),
            PointerPhase::Down,
            PointerButton::Left,
            1,
        )));
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(5.0, 5.0),
            PointerPhase::Up,
            PointerButton::Left,
            1,
        )));
        runtime.update(Instant::now());
        assert_eq!(runtime.input().focused(), focused_before);
        assert!(ui.close_confirmation(generation));
    }

    #[test]
    fn overlay_fits_constrained_viewports_without_clipping_header_or_overlapping_buttons() {
        let long_text = (1..=20)
            .map(|i| format!("payload line {i} with enough text to wrap"))
            .collect::<Vec<_>>()
            .join("\n");
        let cases = [
            (320, 240, ConfirmationRequest::paste(20, long_text.clone())),
            (
                420,
                280,
                ConfirmationRequest::clipboard_write(
                    "Tab 1 requests clipboard write (500 bytes)".into(),
                    long_text.clone(),
                ),
            ),
            (
                280,
                320,
                ConfirmationRequest::clipboard_write(
                    "Tab 1 requests clipboard write (500 bytes)".into(),
                    long_text,
                ),
            ),
        ];
        for (width, height, request) in cases {
            let (ui, mut runtime) = mounted_workspace(width, height);
            let (_gen, focus) = ui.open_confirmation(request);
            runtime.request_focus(&focus);
            runtime.update(Instant::now());

            let header = keyed_rect(&runtime, "confirmation-header");
            let frame = keyed_rect(&runtime, "confirmation-preview-frame");
            let footer = keyed_rect(&runtime, "confirmation-footer");
            let cancel = keyed_rect(&runtime, "confirmation-cancel");
            let confirm = keyed_rect(&runtime, "confirmation-confirm");
            let viewport = Rect::from_min_size(Point::ZERO, Size::new(width as f32, height as f32));

            assert!(viewport.contains(header.min) && viewport.contains(header.max));
            assert!(viewport.contains(footer.min) && viewport.contains(footer.max));
            assert!(header.max.y <= frame.min.y);
            assert!(frame.max.y <= footer.min.y);
            assert!(
                cancel.max.x <= confirm.min.x || cancel.max.y <= confirm.min.y,
                "wrapped footer buttons must not overlap at {width}x{height}: cancel={cancel:?} confirm={confirm:?}"
            );
            // Buttons remain focusable and actionable under constrained viewports.
            assert!(cancel.size().width > 0.0 && cancel.size().height > 0.0);
            assert!(confirm.size().width > 0.0 && confirm.size().height > 0.0);
            press(&mut runtime, Key::Enter, Modifiers::default());
            assert_eq!(
                ui.drain_actions()[0].command,
                AppCommand::CancelConfirmation
            );
            press(&mut runtime, Key::Tab, Modifiers::default());
            press(&mut runtime, Key::Enter, Modifiers::default());
            assert_eq!(
                ui.drain_actions()[0].command,
                AppCommand::ConfirmConfirmation
            );
        }
    }

    #[test]
    fn confirmation_buttons_preserve_host_coordination_and_independent_permissions() {
        for clipboard_write in [false, true] {
            let cancelled = Arc::new(AtomicBool::new(false));
            let confirmed = Arc::new(AtomicBool::new(false));
            let scroll = Arc::new(AtomicUsize::new(0));
            let mut runtime = Runtime::new();
            if clipboard_write {
                runtime.set_root(build_clipboard_confirmation_root(
                    "Clipboard request".into(),
                    vec!["preview".into()],
                    scroll,
                    cancelled.clone(),
                    confirmed.clone(),
                    18.0,
                ));
            } else {
                runtime.set_root(build_confirmation_root(
                    1,
                    vec!["preview".into()],
                    scroll,
                    cancelled.clone(),
                    confirmed.clone(),
                    18.0,
                ));
            }
            runtime.update(Instant::now());
            assert!(runtime.focus_first_focusable());
            let press = |runtime: &mut Runtime, key| {
                runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                    key,
                    modifiers: Modifiers::default(),
                }));
                runtime.update(Instant::now());
            };
            press(&mut runtime, Key::Enter);
            assert!(cancelled.load(Ordering::SeqCst));
            assert!(
                !confirmed.load(Ordering::SeqCst),
                "deny never grants permission"
            );
            cancelled.store(false, Ordering::SeqCst);
            press(&mut runtime, Key::Tab);
            press(&mut runtime, Key::Space);
            assert!(confirmed.load(Ordering::SeqCst));
            assert!(!cancelled.load(Ordering::SeqCst));
            drop(runtime);
            assert_eq!(Arc::strong_count(&confirmed), 1);
            assert_eq!(Arc::strong_count(&cancelled), 1);
        }
    }
}
