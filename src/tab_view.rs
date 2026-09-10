//! Product-owned responsive terminal tab workspace.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use harbor_widget::{
    Actions, Button, Column, ComponentExt as _, ConstrainedBox, Dispatcher, Expanded, Focus,
    FocusHandle, FocusScope, IconButton, KeyChord, LayoutObserver, Row, ScrollArea,
    ScrollController, Separator, Shortcuts, Store,
};
use harbor_widget::{
    input::event::{Key, Modifiers},
    layout::Rect,
    scene::primitive::Color,
    view::{BuildCx, Component, View},
    widgets::layout_observer::LayoutChangedCallback,
    widgets::padding::Padding,
};

use crate::{
    tab_manager::{TabId, TabIndex, TabSnapshot},
    terminal_view::{TerminalDecorationPreset, TerminalWidgetBridge},
};

pub(crate) const EXPANDED_BREAKPOINT_DP: f32 = 900.0;
const EXPANDED_RAIL_WIDTH: f32 = 200.0;
const COMPACT_RAIL_WIDTH: f32 = 56.0;

/// The only product-level responsive policy for the terminal rail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RailPresentation {
    Expanded,
    Compact,
}

impl RailPresentation {
    pub(crate) fn for_logical_width(width: f64) -> Self {
        if width.is_finite() && width >= f64::from(EXPANDED_BREAKPOINT_DP) {
            Self::Expanded
        } else {
            Self::Compact
        }
    }

    fn width(self) -> f32 {
        match self {
            Self::Expanded => EXPANDED_RAIL_WIDTH,
            Self::Compact => COMPACT_RAIL_WIDTH,
        }
    }
}

/// Requests are queued during widget event routing and reduced by the Host afterward.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TabCommand {
    New,
    Close(TabId),
    CloseActive,
    Activate(TabId),
    Next,
    Previous,
    Numeric(TabIndex),
}

/// Focus disposition attached at the event source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TabFocusPolicy {
    PreserveRail,
    RailTab(TabId),
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TabCommandRequest {
    pub(crate) command: TabCommand,
    pub(crate) focus: TabFocusPolicy,
}

impl TabCommandRequest {
    const fn rail(command: TabCommand) -> Self {
        Self {
            command,
            focus: TabFocusPolicy::PreserveRail,
        }
    }

    const fn shortcut(command: TabCommand) -> Self {
        Self {
            command,
            focus: TabFocusPolicy::Terminal,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TabUiState {
    snapshots: Vec<TabSnapshot>,
    active_bridge: Option<TerminalWidgetBridge>,
    presentation: RailPresentation,
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

/// Stable boundary between the Host-owned tab model and the declarative widget tree.
#[derive(Clone)]
pub(crate) struct TabUiController {
    store: Store<TabUiState, TabCommandRequest>,
    tab_focus: Arc<Mutex<HashMap<TabId, FocusHandle>>>,
    scroll: ScrollController,
    terminal_focus: FocusHandle,
    allocation: TerminalAllocationMailbox,
}

impl TabUiController {
    pub(crate) fn new(
        snapshots: Vec<TabSnapshot>,
        active_bridge: TerminalWidgetBridge,
        logical_width: f64,
    ) -> Self {
        let tab_focus = snapshots
            .iter()
            .map(|snapshot| (snapshot.id, FocusHandle::new()))
            .collect();
        Self {
            store: Store::new(TabUiState {
                snapshots,
                active_bridge: Some(active_bridge),
                presentation: RailPresentation::for_logical_width(logical_width),
            }),
            tab_focus: Arc::new(Mutex::new(tab_focus)),
            scroll: ScrollController::new(),
            terminal_focus: FocusHandle::new(),
            allocation: TerminalAllocationMailbox::new(),
        }
    }

    pub(crate) fn sync(
        &self,
        snapshots: Vec<TabSnapshot>,
        active_bridge: Option<TerminalWidgetBridge>,
        logical_width: f64,
    ) {
        if let Ok(mut handles) = self.tab_focus.lock() {
            handles.retain(|id, _| snapshots.iter().any(|snapshot| snapshot.id == *id));
            for snapshot in &snapshots {
                handles.entry(snapshot.id).or_insert_with(FocusHandle::new);
            }
        }
        self.store.set_state(TabUiState {
            snapshots,
            active_bridge,
            presentation: RailPresentation::for_logical_width(logical_width),
        });
    }

    pub(crate) fn update_presentation(&self, logical_width: f64) -> bool {
        let current = self.store.state().read().clone();
        let presentation = RailPresentation::for_logical_width(logical_width);
        if current.presentation == presentation {
            return false;
        }
        self.store.set_state(TabUiState {
            presentation,
            ..current
        });
        true
    }

    pub(crate) fn drain_actions(&self) -> Vec<TabCommandRequest> {
        self.store.drain_actions()
    }

    pub(crate) fn terminal_focus(&self) -> FocusHandle {
        self.terminal_focus
    }

    pub(crate) fn latest_terminal_allocation(&self) -> Option<Rect> {
        self.allocation.latest()
    }

    pub(crate) fn tab_focus(&self, id: TabId) -> Option<FocusHandle> {
        self.tab_focus.lock().ok()?.get(&id).copied()
    }
}

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

/// Main-window root: a 4dp backdrop-aware inset around product content.
pub(crate) fn build_main_root(
    backdrop_available: bool,
    fallback_rgb: [f32; 3],
    child: impl Component + 'static,
) -> Padding {
    // Product/native colors are sRGB; widget colors feed a linear-light shader.
    let fallback = fallback_rgb.map(|channel| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    let root = Padding::all(4.0);
    let root = if backdrop_available {
        root
    } else {
        root.background(Color {
            r: fallback[0],
            g: fallback[1],
            b: fallback[2],
            a: 1.0,
        })
    };
    root.child(child)
}

impl Component for TabWorkspace {
    fn build(&self, cx: &mut BuildCx) -> View {
        let state = self.controller.store.watch(cx).clone();
        let dispatcher = self.controller.store.dispatcher();
        let action_dispatcher = dispatcher.clone();
        let new_dispatcher = dispatcher.clone();
        let terminal = self.controller.allocation.observer().child(
            Focus::new(TerminalDecorationPreset::wrap(
                state
                    .active_bridge
                    .expect("workspace has an active terminal until window exit"),
            ))
            .handle(self.controller.terminal_focus),
        );
        let workspace = harbor_widget::view!(&mut *cx, Row::new() => {
            ConstrainedBox::new()
                .min_width(state.presentation.width())
                .max_width(state.presentation.width()) => {
                Column::new() => {
                    Expanded::new() => {
                        ScrollArea::new().controller(self.controller.scroll.clone()) => {
                            Column::new() => {
                                for snapshot in state.snapshots.iter() {
                                    tab_item(
                                        snapshot,
                                        state.presentation,
                                        dispatcher.clone(),
                                        self.controller
                                            .tab_focus(snapshot.id)
                                            .expect("live tab has a focus handle"),
                                    )
                                    .keyed(format!("terminal-tab-{}", snapshot.id)) => {}
                                }
                            }
                        }
                    }
                    { new_tab_button(state.presentation, new_dispatcher) }
                }
            }
            Separator::vertical() => {}
            Expanded::new() => { terminal => {} }
        });
        let shortcuts = Shortcuts::new(workspace)
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
        let shortcuts = (1..=9).fold(shortcuts, |shortcuts, index| {
            let digit = char::from_digit(index as u32, 10).expect("numeric shortcut is a digit");
            let tab_index = TabIndex::from_valid_u8(index as u8);
            shortcuts.bind(
                KeyChord::new(Key::Character(digit), ctrl()),
                TabCommandRequest::shortcut(TabCommand::Numeric(tab_index)),
            )
        });
        build_main_root(
            self.backdrop_available,
            self.backdrop_fallback,
            FocusScope::new().child(Actions::new(shortcuts, move |request| {
                action_dispatcher.dispatch(request);
            })),
        )
        .build(cx)
    }
}

fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Modifiers::default()
    }
}

fn new_tab_button(
    presentation: RailPresentation,
    dispatcher: Dispatcher<TabCommandRequest>,
) -> View {
    match presentation {
        RailPresentation::Expanded => {
            View::deferred(Button::new("New terminal").on_click(move |_| {
                dispatcher.dispatch(TabCommandRequest::rail(TabCommand::New));
            }))
        }
        RailPresentation::Compact => {
            View::deferred(IconButton::new("+", "New terminal").on_click(move |_| {
                dispatcher.dispatch(TabCommandRequest::rail(TabCommand::New));
            }))
        }
    }
}

fn tab_item(
    snapshot: &TabSnapshot,
    presentation: RailPresentation,
    dispatcher: Dispatcher<TabCommandRequest>,
    focus: FocusHandle,
) -> Row {
    let id = snapshot.id;
    let active = snapshot.active;
    let select_dispatcher = dispatcher.clone();
    let close_dispatcher = dispatcher;
    let label = tab_label(snapshot);
    let glyph = match presentation {
        RailPresentation::Expanded => label.clone(),
        RailPresentation::Compact => abbreviation(snapshot),
    };
    Row::new()
        .child(
            Expanded::new().child(
                Focus::new(
                    IconButton::new(glyph, label)
                        .selected(active)
                        .on_click(move |_| {
                            select_dispatcher
                                .dispatch(TabCommandRequest::rail(TabCommand::Activate(id)));
                        }),
                )
                .handle(focus),
            ),
        )
        .child(
            IconButton::new("×", format!("Close {}", snapshot.title)).on_click(move |_| {
                close_dispatcher.dispatch(TabCommandRequest::rail(TabCommand::Close(id)));
            }),
        )
}

fn tab_label(snapshot: &TabSnapshot) -> String {
    if snapshot.unread {
        format!("• {}", snapshot.title)
    } else {
        snapshot.title.clone()
    }
}

fn abbreviation(snapshot: &TabSnapshot) -> String {
    if snapshot.unread {
        "•".to_owned()
    } else {
        (snapshot.id.get() % 10).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_terminal::Terminal;
    use harbor_widget::{
        fiber::FiberId,
        input::event::{KeyboardEvent, PointerButton, PointerEvent, PointerPhase, UiEvent},
        layout::Point,
        renderer::Viewport,
        runtime::Runtime,
        scene::primitive::Primitive,
    };
    use std::{any::TypeId, sync::atomic::AtomicBool, time::Instant};

    fn snapshot(id: u64, active: bool, unread: bool) -> TabSnapshot {
        TabSnapshot {
            id: TabId(id),
            title: format!("Terminal {id}"),
            unread,
            draw_id: id,
            active,
        }
    }

    #[allow(clippy::arc_with_non_send_sync)]
    fn bridge(draw_id: u64) -> TerminalWidgetBridge {
        TerminalWidgetBridge::new(
            draw_id,
            Arc::new(Mutex::new(Terminal::new_headless(4, 20))),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn controller(width: f64, snapshots: Vec<TabSnapshot>, active_draw: u64) -> TabUiController {
        TabUiController::new(snapshots, bridge(active_draw), width)
    }

    fn mount(controller: TabUiController, width: u32, height: u32) -> Runtime {
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(width, height, 1.0));
        runtime.set_root(TabWorkspace::new(controller, false));
        runtime.update(Instant::now());
        runtime
    }

    fn fiber_ids(runtime: &Runtime) -> Vec<FiberId> {
        let mut pending = vec![runtime.root_id().expect("workspace root")];
        let mut ids = Vec::new();
        while let Some(id) = pending.pop() {
            let fiber = runtime.arena().get(id).expect("live fiber");
            pending.extend_from_slice(fiber.children());
            ids.push(id);
        }
        ids
    }

    fn tab_rows(runtime: &Runtime) -> Vec<FiberId> {
        let mut rows: Vec<_> = fiber_ids(runtime)
            .into_iter()
            .filter(|id| {
                let fiber = runtime.arena().get(*id).expect("live fiber");
                fiber.widget_type() == TypeId::of::<harbor_widget::Keyed<Row>>()
                    && fiber.children().len() == 2
            })
            .collect();
        rows.sort_by(|left, right| {
            let left_y = runtime
                .arena()
                .get(*left)
                .unwrap()
                .layout_rect()
                .unwrap()
                .min
                .y;
            let right_y = runtime
                .arena()
                .get(*right)
                .unwrap()
                .layout_rect()
                .unwrap()
                .min
                .y;
            left_y.total_cmp(&right_y)
        });
        rows
    }

    #[derive(Clone)]
    struct HandwrittenWorkspace {
        controller: TabUiController,
    }

    impl Component for HandwrittenWorkspace {
        fn build(&self, cx: &mut BuildCx) -> View {
            let state = self.controller.store.watch(cx).clone();
            let dispatcher = self.controller.store.dispatcher();
            let action_dispatcher = dispatcher.clone();
            let mut items = Column::new();
            for snapshot in &state.snapshots {
                items = items.child(
                    tab_item(
                        snapshot,
                        state.presentation,
                        dispatcher.clone(),
                        self.controller.tab_focus(snapshot.id).unwrap(),
                    )
                    .keyed(format!("terminal-tab-{}", snapshot.id.0)),
                );
            }
            let new_dispatcher = dispatcher.clone();
            let rail = Column::new().child(
                Expanded::new().child(
                    ScrollArea::new()
                        .controller(self.controller.scroll.clone())
                        .child(items),
                ),
            );
            let rail = rail.child(new_tab_button(state.presentation, new_dispatcher));
            let terminal = self.controller.allocation.observer().child(
                Focus::new(TerminalDecorationPreset::wrap(
                    state.active_bridge.expect("handwritten active bridge"),
                ))
                .handle(self.controller.terminal_focus),
            );
            let workspace = Row::new()
                .child(
                    ConstrainedBox::new()
                        .min_width(state.presentation.width())
                        .max_width(state.presentation.width())
                        .child(rail),
                )
                .child(Separator::vertical())
                .child(Expanded::new().child(terminal));
            let shortcuts = Shortcuts::new(workspace)
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
            let shortcuts = (1..=9).fold(shortcuts, |shortcuts, index| {
                let digit =
                    char::from_digit(index as u32, 10).expect("numeric shortcut is a digit");
                let tab_index = TabIndex::from_valid_u8(index as u8);
                shortcuts.bind(
                    KeyChord::new(Key::Character(digit), ctrl()),
                    TabCommandRequest::shortcut(TabCommand::Numeric(tab_index)),
                )
            });
            build_main_root(
                false,
                harbor_config::WindowBackdropStyle::default().fallback,
                FocusScope::new().child(Actions::new(shortcuts, move |request| {
                    action_dispatcher.dispatch(request);
                })),
            )
            .build(cx)
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct FiberSignature {
        widget_type: TypeId,
        rect: Option<harbor_widget::layout::Rect>,
        children: Vec<FiberSignature>,
    }

    fn fiber_signature(runtime: &Runtime, id: FiberId) -> FiberSignature {
        let fiber = runtime.arena().get(id).unwrap();
        FiberSignature {
            widget_type: fiber.widget_type(),
            rect: fiber.layout_rect(),
            children: fiber
                .children()
                .iter()
                .copied()
                .map(|child| fiber_signature(runtime, child))
                .collect(),
        }
    }

    fn scene_signature(runtime: &Runtime) -> Vec<(Primitive, u32)> {
        runtime
            .pending_delta()
            .unwrap()
            .added
            .iter()
            .map(|item| (item.primitive.clone(), item.paint_order))
            .collect()
    }

    #[test]
    fn breakpoint_is_finite_and_inclusive() {
        assert_eq!(
            RailPresentation::for_logical_width(899.999),
            RailPresentation::Compact
        );
        assert_eq!(
            RailPresentation::for_logical_width(900.0),
            RailPresentation::Expanded
        );
        assert_eq!(
            RailPresentation::for_logical_width(f64::NAN),
            RailPresentation::Compact
        );
        assert_eq!(
            RailPresentation::for_logical_width(f64::INFINITY),
            RailPresentation::Compact
        );
        assert_eq!(
            RailPresentation::for_logical_width(-1.0),
            RailPresentation::Compact
        );
    }

    #[test]
    fn product_macro_matches_handwritten_workspace_fibers_scene_and_event_target() {
        for logical_width in [800.0, 1000.0] {
            let snapshots = vec![snapshot(1, true, false), snapshot(2, false, true)];
            let macro_controller = controller(logical_width, snapshots.clone(), 1);
            let mut macro_runtime = mount(macro_controller.clone(), logical_width as u32, 320);

            let handwritten_controller = controller(logical_width, snapshots, 1);
            let mut handwritten_runtime = Runtime::new();
            handwritten_runtime.set_viewport(Viewport::new(logical_width as u32, 320, 1.0));
            handwritten_runtime.set_root(HandwrittenWorkspace {
                controller: handwritten_controller.clone(),
            });
            handwritten_runtime.update(Instant::now());

            assert_eq!(
                fiber_signature(&macro_runtime, macro_runtime.root_id().unwrap()),
                fiber_signature(&handwritten_runtime, handwritten_runtime.root_id().unwrap()),
            );
            assert_eq!(
                scene_signature(&macro_runtime),
                scene_signature(&handwritten_runtime)
            );

            let macro_row = tab_rows(&macro_runtime)[0];
            let rect = macro_runtime
                .arena()
                .get(macro_row)
                .unwrap()
                .layout_rect()
                .unwrap();
            let position = Point::new(rect.min.x + 8.0, rect.min.y + rect.size().height / 2.0);
            for runtime in [&mut macro_runtime, &mut handwritten_runtime] {
                for phase in [PointerPhase::Down, PointerPhase::Up] {
                    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                        position,
                        phase,
                        PointerButton::Left,
                        1,
                    )));
                }
            }
            let macro_request = macro_controller.drain_actions();
            let handwritten_request = handwritten_controller.drain_actions();
            assert_eq!(
                macro_request,
                [TabCommandRequest::rail(TabCommand::Activate(TabId(1)))]
            );
            assert_eq!(macro_request, handwritten_request);

            let close_position = Point::new(rect.max.x - 4.0, position.y);
            for runtime in [&mut macro_runtime, &mut handwritten_runtime] {
                for phase in [PointerPhase::Down, PointerPhase::Up] {
                    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                        close_position,
                        phase,
                        PointerButton::Left,
                        2,
                    )));
                }
            }
            let macro_request = macro_controller.drain_actions();
            let handwritten_request = handwritten_controller.drain_actions();
            assert_eq!(
                macro_request,
                [TabCommandRequest::rail(TabCommand::Close(TabId(1)))]
            );
            assert_eq!(macro_request, handwritten_request);

            let rail = fiber_ids(&macro_runtime)
                .into_iter()
                .find_map(|id| {
                    let fiber = macro_runtime.arena().get(id).unwrap();
                    (fiber.widget_type() == TypeId::of::<ConstrainedBox>())
                        .then(|| fiber.layout_rect().unwrap())
                })
                .unwrap();
            let new_rect = fiber_ids(&macro_runtime)
                .into_iter()
                .filter_map(|id| {
                    let fiber = macro_runtime.arena().get(id).unwrap();
                    let rect = fiber.layout_rect()?;
                    (fiber.is_focusable() && rect.max.x <= rail.max.x).then_some(rect)
                })
                .max_by(|left, right| left.min.y.total_cmp(&right.min.y))
                .expect("new-tab control");
            let new_position = Point::new(
                new_rect.min.x + new_rect.size().width / 2.0,
                new_rect.min.y + new_rect.size().height / 2.0,
            );
            for runtime in [&mut macro_runtime, &mut handwritten_runtime] {
                for phase in [PointerPhase::Down, PointerPhase::Up] {
                    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                        new_position,
                        phase,
                        PointerButton::Left,
                        3,
                    )));
                }
            }
            let macro_request = macro_controller.drain_actions();
            let handwritten_request = handwritten_controller.drain_actions();
            assert_eq!(macro_request, [TabCommandRequest::rail(TabCommand::New)]);
            assert_eq!(macro_request, handwritten_request);

            let mut shortcuts = vec![
                (Key::Character('t'), ctrl(), TabCommand::New),
                (Key::Character('w'), ctrl(), TabCommand::CloseActive),
                (Key::Tab, ctrl(), TabCommand::Next),
                (
                    Key::Tab,
                    Modifiers {
                        ctrl: true,
                        shift: true,
                        ..Modifiers::default()
                    },
                    TabCommand::Previous,
                ),
            ];
            shortcuts.extend((1..=9).map(|index| {
                (
                    Key::Character(char::from_digit(index as u32, 10).unwrap()),
                    ctrl(),
                    TabCommand::Numeric(TabIndex::from_valid_u8(index as u8)),
                )
            }));
            for (key, modifiers, command) in shortcuts {
                for runtime in [&mut macro_runtime, &mut handwritten_runtime] {
                    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
                }
                let macro_request = macro_controller.drain_actions();
                let handwritten_request = handwritten_controller.drain_actions();
                assert_eq!(macro_request, [TabCommandRequest::shortcut(command)]);
                assert_eq!(macro_request, handwritten_request);
            }
        }
    }

    #[test]
    fn workspace_publishes_the_final_terminal_panel_allocation() {
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        let external_rect = runtime
            .pending_delta()
            .unwrap()
            .added
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::External { rect, draw: 1 } => Some(rect),
                _ => None,
            })
            .unwrap();
        assert_eq!(controller.latest_terminal_allocation(), Some(external_rect));

        runtime.update(Instant::now());
        assert_eq!(controller.latest_terminal_allocation(), Some(external_rect));
    }

    #[test]
    fn presentation_changes_only_when_crossing_the_breakpoint() {
        let controller = controller(899.0, vec![snapshot(1, true, false)], 1);
        assert!(!controller.update_presentation(100.0));
        assert!(controller.update_presentation(900.0));
        assert!(!controller.update_presentation(1200.0));
        assert!(controller.update_presentation(f64::NAN));
    }

    #[test]
    fn workspace_allocates_fixed_rail_and_finite_terminal_geometry() {
        for (logical_width, expected_rail) in [(899.0, 56.0), (900.0, 200.0)] {
            let controller = controller(
                logical_width,
                vec![snapshot(1, true, false), snapshot(2, false, true)],
                1,
            );
            let runtime = mount(controller, logical_width as u32, 320);
            let rail = fiber_ids(&runtime)
                .into_iter()
                .find_map(|id| {
                    let fiber = runtime.arena().get(id).unwrap();
                    (fiber.widget_type() == TypeId::of::<ConstrainedBox>())
                        .then(|| fiber.layout_rect().unwrap())
                })
                .expect("bounded rail");
            assert_eq!(rail.size().width, expected_rail);
            let terminal = runtime
                .pending_delta()
                .unwrap()
                .added
                .iter()
                .find_map(|item| match item.primitive {
                    Primitive::External { rect, draw: 1 } => Some(rect),
                    _ => None,
                })
                .expect("active terminal primitive");
            assert!(
                [
                    terminal.min.x,
                    terminal.min.y,
                    terminal.max.x,
                    terminal.max.y
                ]
                .into_iter()
                .all(f32::is_finite)
            );
            assert!(terminal.size().width >= 0.0);
            assert!(terminal.size().height >= 0.0);
            if expected_rail == COMPACT_RAIL_WIDTH {
                let cell_width = runtime.text_metrics().cell_width;
                for item in &runtime.pending_delta().unwrap().added {
                    if let Primitive::Text { text, origin, .. } = &item.primitive {
                        let text_width = text.chars().count() as f32 * cell_width;
                        assert!(origin.x >= rail.min.x);
                        assert!(origin.x + text_width <= rail.max.x);
                    }
                }
            }
        }

        let narrow = mount(controller(0.0, vec![snapshot(1, true, false)], 1), 0, 0);
        for id in fiber_ids(&narrow) {
            let rect = narrow.arena().get(id).unwrap().layout_rect().unwrap();
            assert!(
                [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
                    .into_iter()
                    .all(f32::is_finite)
            );
            assert!(rect.size().width >= 0.0 && rect.size().height >= 0.0);
        }
    }

    #[test]
    fn shortcuts_and_pointer_controls_enqueue_distinct_focus_policies() {
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);

        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('t'),
            modifiers: ctrl(),
        }));
        assert_eq!(
            controller.drain_actions(),
            [TabCommandRequest::shortcut(TabCommand::New)]
        );

        let row = tab_rows(&runtime)[0];
        let rect = runtime.arena().get(row).unwrap().layout_rect().unwrap();
        let position = Point::new(rect.min.x + 8.0, rect.min.y + rect.size().height / 2.0);
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                position,
                phase,
                PointerButton::Left,
                1,
            )));
        }
        assert_eq!(
            controller.drain_actions(),
            [TabCommandRequest::rail(TabCommand::Activate(TabId(1)))]
        );
    }

    #[test]
    fn many_tabs_scroll_and_offset_clamps_after_content_shrinks() {
        let snapshots = (1..=20).map(|id| snapshot(id, id == 1, false)).collect();
        let controller = controller(1000.0, snapshots, 1);
        let mut runtime = mount(controller.clone(), 1000, 120);
        assert!(controller.scroll.metrics().max_scroll_extent() > 0.0);

        for _ in 0..24 {
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Tab,
                modifiers: Modifiers::default(),
            }));
            runtime.update(Instant::now());
        }
        assert!(controller.scroll.metrics().offset() > 0.0);

        controller.sync(vec![snapshot(1, true, false)], Some(bridge(1)), 1000.0);
        runtime.update(Instant::now());
        assert_eq!(controller.scroll.metrics().max_scroll_extent(), 0.0);
        assert_eq!(controller.scroll.metrics().offset(), 0.0);
    }

    #[test]
    fn signal_updates_preserve_surviving_keyed_tab_fibers() {
        let controller = controller(
            1000.0,
            vec![
                snapshot(1, true, false),
                snapshot(2, false, false),
                snapshot(3, false, false),
            ],
            1,
        );
        let mut runtime = mount(controller.clone(), 1000, 320);
        let before = tab_rows(&runtime);
        assert_eq!(before.len(), 3);

        controller.sync(
            vec![
                snapshot(1, true, false),
                snapshot(3, false, true),
                snapshot(4, false, false),
            ],
            Some(bridge(1)),
            1000.0,
        );
        runtime.update(Instant::now());
        let after = tab_rows(&runtime);
        assert_eq!(after.len(), 3);
        assert_eq!(after[0], before[0]);
        assert_eq!(after[1], before[2]);
        assert_ne!(after[2], before[1]);
    }

    #[test]
    fn signal_switch_replaces_the_only_mounted_terminal_bridge() {
        let controller = controller(
            1000.0,
            vec![snapshot(1, true, false), snapshot(2, false, false)],
            1,
        );
        let mut runtime = mount(controller.clone(), 1000, 320);
        assert!(runtime.has_external_draws());

        controller.sync(
            vec![snapshot(1, false, false), snapshot(2, true, false)],
            Some(bridge(2)),
            1000.0,
        );
        runtime.update(Instant::now());
        let delta = runtime.pending_delta().expect("bridge switch scene delta");
        let mounted: Vec<_> = delta
            .added
            .iter()
            .filter_map(|item| match item.primitive {
                Primitive::External { draw, .. } => Some(draw),
                _ => None,
            })
            .collect();
        assert_eq!(mounted, [2]);
        assert!(runtime.has_external_draws());
    }

    #[test]
    fn labels_are_deterministic_and_preserve_full_compact_metadata() {
        let unread = snapshot(123, false, true);
        assert_eq!(tab_label(&unread), "• Terminal 123");
        assert_eq!(abbreviation(&unread), "•");
        assert_eq!(abbreviation(&snapshot(123, false, false)), "3");
        let button = IconButton::new(abbreviation(&unread), tab_label(&unread));
        assert_eq!(button.label(), "• Terminal 123");
    }

    #[test]
    fn clearing_workspace_releases_the_last_terminal_bridge() {
        use harbor_widget::widgets::sized_box::SizedBox;

        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
        let weak = Arc::downgrade(&terminal);
        let controller = TabUiController::new(
            vec![snapshot(1, true, false)],
            TerminalWidgetBridge::new(1, Arc::clone(&terminal), Arc::new(AtomicBool::new(false))),
            1000.0,
        );
        let mut runtime = mount(controller.clone(), 1000, 320);
        drop(terminal);

        controller.sync(Vec::new(), None, 1000.0);
        runtime.set_root(SizedBox::new(harbor_widget::layout::Size::ZERO));
        runtime.update(Instant::now());

        assert!(weak.upgrade().is_none());
        assert!(!runtime.has_external_draws());
    }

    #[test]
    fn tab_index_domain_type_validates_range() {
        assert_eq!(TabIndex::new(0), None);
        assert_eq!(TabIndex::new(10), None);
        assert_eq!(TabIndex::new(255), None);

        let first = TabIndex::new(1).unwrap();
        assert_eq!(first.get(), 1);
        assert_eq!(first.to_zero_based(), 0);

        let ninth = TabIndex::new(9).unwrap();
        assert_eq!(ninth.get(), 9);
        assert_eq!(ninth.to_zero_based(), 8);
    }
}
