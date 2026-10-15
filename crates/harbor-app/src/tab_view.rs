//! Product-owned responsive terminal tab workspace.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use harbor_widget::{
    FocusHandle, LayoutObserver, ScrollController, Store, layout::Rect,
    widgets::layout_observer::LayoutChangedCallback,
};

use crate::{
    command::AppCommandRequest,
    tab_manager::{TabId, TabIndex, TabSnapshot},
    terminal_view::TerminalWidgetBridge,
};

#[path = "ui.rs"]
pub mod ui;

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
pub enum TabCommand {
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
pub enum TabFocusPolicy {
    PreserveRail,
    RailTab(TabId),
    Terminal,
}

#[derive(Clone)]
pub(crate) struct TabUiState {
    snapshots: Vec<TabSnapshot>,
    active_bridge: Option<TerminalWidgetBridge>,
    presentation: RailPresentation,
    keybindings: Option<crate::command::ResolvedKeybindings>,
    configuration_result: Option<String>,
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
pub struct TabUiController {
    store: Store<TabUiState, AppCommandRequest>,
    tab_focus: Arc<Mutex<HashMap<TabId, FocusHandle>>>,
    scroll: ScrollController,
    terminal_focus: FocusHandle,
    allocation: TerminalAllocationMailbox,
}

impl TabUiController {
    pub fn new(
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
                keybindings: None,
                configuration_result: None,
            }),
            tab_focus: Arc::new(Mutex::new(tab_focus)),
            scroll: ScrollController::new(),
            terminal_focus: FocusHandle::new(),
            allocation: TerminalAllocationMailbox::new(),
        }
    }

    pub fn publish_keybindings(&self, keybindings: crate::command::ResolvedKeybindings) {
        let mut state = self.store.state().read().clone();
        state.keybindings = Some(keybindings);
        self.store.set_state(state);
    }

    pub fn report_configuration_result(&self, message: String) {
        let mut state = self.store.state().read().clone();
        state.configuration_result = Some(message);
        self.store.set_state(state);
    }
    pub fn sync(
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
        let configuration = {
            let state = self.store.state().read();
            (
                state.keybindings.clone(),
                state.configuration_result.clone(),
            )
        };
        self.store.set_state(TabUiState {
            snapshots,
            active_bridge,
            presentation: RailPresentation::for_logical_width(logical_width),
            keybindings: configuration.0,
            configuration_result: configuration.1,
        });
    }

    pub fn update_presentation(&self, logical_width: f64) -> bool {
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

    pub fn drain_actions(&self) -> Vec<AppCommandRequest> {
        self.store.drain_actions()
    }

    pub fn terminal_focus(&self) -> FocusHandle {
        self.terminal_focus
    }

    pub fn latest_terminal_allocation(&self) -> Option<Rect> {
        self.allocation.latest()
    }

    pub fn tab_focus(&self, id: TabId) -> Option<FocusHandle> {
        self.tab_focus.lock().ok()?.get(&id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::ui::*;
    use super::*;
    use crate::command::{AppCommand, AppCommandRequest};
    use harbor_terminal::Terminal;
    use harbor_widget::{
        ConstrainedBox, Row,
        fiber::FiberId,
        input::event::{
            Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
        },
        layout::Point,
        renderer::Viewport,
        runtime::Runtime,
        scene::primitive::{ExternalDrawId, Primitive},
    };
    use std::{any::TypeId, sync::atomic::AtomicBool, time::Instant};

    fn snapshot(id: u64, active: bool, unread: bool) -> TabSnapshot {
        TabSnapshot {
            id: TabId(id),
            title: format!("Terminal {id}"),
            unread,
            draw_id: ExternalDrawId::new(id),
            working_directory: None,
            shell_integration: None,
            active,
        }
    }

    #[allow(clippy::arc_with_non_send_sync)]
    fn bridge(draw_id: u64) -> TerminalWidgetBridge {
        TerminalWidgetBridge::new(
            ExternalDrawId::new(draw_id),
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
        runtime.set_root(tab_workspace(controller, false));
        runtime.update(Instant::now());
        runtime
    }
    #[derive(Clone)]
    struct Writer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn reload_rebinding_keeps_consumed_press_repeat_and_release_out_of_terminal() {
        let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let engine = std::sync::Arc::new(std::sync::Mutex::new(
            harbor_terminal::Terminal::new_headless_with_io(
                4,
                20,
                std::io::empty(),
                Writer(bytes.clone()),
                || true,
            ),
        ));
        let test_bridge = crate::terminal_view::TerminalWidgetBridge::new(
            1,
            engine,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        controller.sync(
            vec![snapshot(1, true, false)],
            Some(test_bridge.clone()),
            1000.0,
        );
        runtime.update(Instant::now());
        runtime.request_focus(&controller.terminal_focus());
        let modifiers = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        };
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('R'),
            modifiers,
        });
        assert!(runtime.dispatch(event.clone()).request_redraw);
        assert!(bytes.lock().unwrap().is_empty());
        assert_eq!(
            controller.drain_actions()[0].command,
            AppCommand::ReloadConfiguration
        );
        let mut raw = harbor_config::RawKeybindings::default();
        raw.entries.insert(
            "app.reload-configuration".to_owned(),
            vec![harbor_config::RawKeybinding {
                modifiers: vec!["ctrl".to_owned(), "shift".to_owned()],
                key: "q".to_owned(),
            }],
        );
        controller.publish_keybindings(crate::command::validate_keybindings(&raw).unwrap());
        controller.report_configuration_result("Configuration reloaded".to_owned());
        runtime.update(Instant::now());
        assert!(
            runtime
                .dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                    key: Key::Character('r'),
                    modifiers: Modifiers::default()
                }))
                .request_redraw
        );
        assert!(bytes.lock().unwrap().is_empty());
        assert_eq!(
            controller.drain_actions()[0].command,
            AppCommand::ReloadConfiguration
        );
        assert!(
            runtime
                .dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
                    key: Key::Character('r'),
                    modifiers: Modifiers::default()
                }))
                .request_redraw
        );
        assert!(bytes.lock().unwrap().is_empty());
        assert!(controller.drain_actions().is_empty());
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('q'),
            modifiers,
        }));
        assert_eq!(
            controller.drain_actions()[0].command,
            AppCommand::ReloadConfiguration
        );
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Character('q'),
            modifiers,
        }));
        controller.sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
        assert!(controller.store.state().read().keybindings.is_some());
        assert_eq!(
            controller
                .store
                .state()
                .read()
                .configuration_result
                .as_deref(),
            Some("Configuration reloaded")
        );
        runtime.update(Instant::now());
        runtime.request_focus(&controller.terminal_focus());
        assert!(bytes.lock().unwrap().is_empty());
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('r'),
            modifiers,
        }));
        assert!(controller.drain_actions().is_empty());
        assert_eq!(
            *bytes.lock().unwrap(),
            b"\x12",
            "released old chord must return to the PTY writer"
        );
    }

    #[test]
    #[allow(clippy::arc_with_non_send_sync)]
    fn canceled_paste_press_cannot_replay_after_gate_focus_loss_or_surface_suspend() {
        use harbor_widget::winit::{NativeKeyboardMetadata, WindowPresenter, WinitAdapter};
        use winit::{
            event::{ElementState, WindowEvent},
            keyboard::{Key as NativeKey, KeyLocation, ModifiersState},
        };
        for (boundary, release_dropped) in [
            ("gate", true),
            ("focus", true),
            ("focus", false),
            ("surface", true),
        ] {
            let bytes = Arc::new(Mutex::new(Vec::new()));
            let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                4,
                20,
                std::io::empty(),
                Writer(bytes.clone()),
                || true,
            )));
            let test_bridge =
                TerminalWidgetBridge::new(1, engine, Arc::new(AtomicBool::new(false)));
            let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
            let mut runtime = mount(controller.clone(), 1000, 320);
            controller.sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
            runtime.update(Instant::now());
            runtime.request_focus(&controller.terminal_focus());
            let mut adapter = WinitAdapter::new();
            adapter.handle_event(
                &mut runtime,
                &WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
            );
            let key = NativeKey::Character("v".into());
            adapter.handle_keyboard_input(
                &mut runtime,
                &key,
                ElementState::Pressed,
                KeyLocation::Standard,
            );
            assert_eq!(controller.drain_actions()[0].command, AppCommand::Paste);
            assert!(bytes.lock().unwrap().is_empty());
            match boundary {
                "gate" => {
                    runtime.cancel_active_input_ownership(Point::ZERO);
                }
                "focus" => {
                    adapter.handle_event(&mut runtime, &WindowEvent::Focused(false));
                }
                "surface" => {
                    let mut presenter = WindowPresenter::new(1000, 320, 1.0);
                    presenter.handle_event_with_size(
                        &mut adapter,
                        &mut runtime,
                        &WindowEvent::Resized(winit::dpi::PhysicalSize::new(0, 0)),
                        Some((0, 0)),
                    );
                    presenter.handle_event_with_size(
                        &mut adapter,
                        &mut runtime,
                        &WindowEvent::Resized(winit::dpi::PhysicalSize::new(1000, 320)),
                        Some((1000, 320)),
                    );
                }
                _ => unreachable!(),
            }
            // Windows focus transitions synthesize release/gain-press events
            // with repeat=false. Neither may clear canceled ownership or replay
            // the chord; exercise the exact metadata route used by WindowEvent.
            adapter.handle_keyboard_input_with_metadata(
                &mut runtime,
                &key,
                ElementState::Released,
                KeyLocation::Standard,
                NativeKeyboardMetadata {
                    repeat: false,
                    is_synthetic: true,
                },
            );
            adapter.handle_event(&mut runtime, &WindowEvent::Focused(true));
            adapter.handle_event(
                &mut runtime,
                &WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
            );
            adapter.handle_keyboard_input_with_metadata(
                &mut runtime,
                &key,
                ElementState::Pressed,
                KeyLocation::Standard,
                NativeKeyboardMetadata {
                    repeat: false,
                    is_synthetic: true,
                },
            );
            adapter.handle_keyboard_input_with_repeat(
                &mut runtime,
                &key,
                ElementState::Pressed,
                KeyLocation::Standard,
                true,
            );
            assert!(
                controller.drain_actions().is_empty(),
                "synthetic lifecycle: {boundary}"
            );
            assert!(
                bytes.lock().unwrap().is_empty(),
                "synthetic lifecycle: {boundary}"
            );
            adapter.handle_event(
                &mut runtime,
                &WindowEvent::ModifiersChanged(ModifiersState::empty().into()),
            );
            // A still-held repeat remains consumed, but the canceled action must
            // not execute. The confirmation gate then drops the physical key-up.
            adapter.handle_keyboard_input_with_repeat(
                &mut runtime,
                &key,
                ElementState::Pressed,
                KeyLocation::Standard,
                true,
            );
            assert!(controller.drain_actions().is_empty(), "{boundary}");
            assert!(bytes.lock().unwrap().is_empty(), "{boundary}");
            adapter.handle_event(&mut runtime, &WindowEvent::Focused(true));
            if !release_dropped {
                adapter.handle_keyboard_input(
                    &mut runtime,
                    &key,
                    ElementState::Released,
                    KeyLocation::Standard,
                );
                assert!(controller.drain_actions().is_empty());
                assert!(bytes.lock().unwrap().is_empty());
            }
            // A genuinely new native press must be ordinary input, with or without a lost release.
            adapter.handle_keyboard_input(
                &mut runtime,
                &key,
                ElementState::Pressed,
                KeyLocation::Standard,
            );
            adapter.handle_keyboard_input(
                &mut runtime,
                &key,
                ElementState::Released,
                KeyLocation::Standard,
            );
            assert!(controller.drain_actions().is_empty(), "{boundary}");
            assert_eq!(*bytes.lock().unwrap(), b"v", "{boundary}");
        }
    }

    #[test]
    #[allow(clippy::arc_with_non_send_sync)]
    fn programmatic_terminal_focus_reaches_ime_provider_and_preedit_input() {
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
        let controller = TabUiController::new(
            vec![snapshot(1, true, false)],
            TerminalWidgetBridge::new(1, Arc::clone(&terminal), Arc::new(AtomicBool::new(false))),
            1000.0,
        );
        let mut runtime = mount(controller.clone(), 1000, 320);

        let focus = runtime.request_focus(&controller.terminal_focus());
        assert!(focus.request_redraw);
        let effects = runtime.update(Instant::now());
        let ime = effects.ime.expect("focused terminal IME effect");
        assert_eq!(ime.allowed, Some(true));

        runtime.dispatch(UiEvent::ImePreedit(
            harbor_widget::input::event::ImePreedit::new("draft", Some((0, 5))),
        ));
        assert_eq!(
            terminal
                .lock()
                .unwrap()
                .preedit()
                .map(|value| value.text.as_str()),
            Some("draft")
        );
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
                let children = fiber.children();
                fiber.widget_type() == TypeId::of::<Row>()
                    && children.last().is_some_and(|child| {
                        let kind = runtime.arena().get(*child).unwrap().widget_type();
                        (kind == TypeId::of::<ConstrainedBox>() && children.len() == 3)
                            || (kind == TypeId::of::<harbor_widget::Expanded>()
                                && runtime
                                    .arena()
                                    .get(*child)
                                    .unwrap()
                                    .children()
                                    .first()
                                    .is_some_and(|label| {
                                        runtime.arena().get(*label).unwrap().widget_type()
                                            == TypeId::of::<harbor_widget::DecoratedBox>()
                                    }))
                    })
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
    fn declarative_workspace_binds_all_tab_shortcuts() {
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
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
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
            assert_eq!(
                controller.drain_actions(),
                [AppCommandRequest::shortcut(AppCommand::Tab(command))]
            );
        }
    }

    #[test]
    fn terminal_shortcuts_enqueue_only_when_context_consumes() {
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        };

        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('C'),
            modifiers: ctrl_shift,
        }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::shortcut(AppCommand::Copy)]
        );
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Character('C'),
            modifiers: ctrl_shift,
        }));

        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('c'),
            modifiers: ctrl(),
        }));
        assert!(controller.drain_actions().is_empty());

        for (key, command) in [
            (Key::Character('v'), AppCommand::Paste),
            (Key::PageUp, AppCommand::PageUp),
            (Key::Home, AppCommand::ScrollToTop),
        ] {
            let modifiers = if key == Key::Character('v') {
                ctrl()
            } else {
                Modifiers::default()
            };
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
            assert_eq!(
                controller.drain_actions(),
                [AppCommandRequest::shortcut(command)]
            );
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
                Primitive::External { rect, draw } if draw == 1 => Some(rect),
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
                    Primitive::External { rect, draw } if draw == 1 => Some(rect),
                    _ => None,
                })
                .expect("active terminal primitive");
            let first_tab = runtime
                .pending_delta()
                .unwrap()
                .added
                .iter()
                .find_map(|item| match item.primitive {
                    Primitive::Quad { rect, color, .. }
                    | Primitive::RoundedQuad { rect, color, .. }
                        if color.a > 0.0
                            && rect.min.x >= rail.min.x
                            && rect.max.x <= rail.max.x =>
                    {
                        Some(rect)
                    }
                    _ => None,
                })
                .expect("selected first tab background");
            assert_eq!(
                first_tab.min.y, terminal.min.y,
                "first tab and terminal panel must share the same top edge"
            );
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
            [AppCommandRequest::shortcut(AppCommand::Tab(
                TabCommand::New
            ))]
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
            [AppCommandRequest::rail(TabCommand::Activate(TabId(1)))]
        );
    }

    fn move_pointer(runtime: &mut Runtime, position: Point) {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            position,
            PointerPhase::Move,
            PointerButton::Left,
            7,
        )));
        runtime.update(Instant::now());
    }

    fn close_is_visible(runtime: &Runtime, row: FiberId) -> bool {
        let rect = runtime.arena().get(row).unwrap().layout_rect().unwrap();
        let delta = runtime.pending_delta().unwrap();
        delta.added.iter().chain(&delta.modified).any(|item| {
            matches!(&item.primitive, Primitive::Icon { icon: harbor_widget::LucideIcon::X, rect: icon_rect, color }
                if rect.contains(icon_rect.min) && color.a > 0.0)
        })
    }

    #[test]
    fn compact_rail_has_no_close_control_or_close_focus_stop() {
        let controller = controller(
            899.0,
            vec![snapshot(1, true, false), snapshot(2, false, false)],
            1,
        );
        let mut runtime = mount(controller.clone(), 899, 320);
        let rows = tab_rows(&runtime);
        assert_eq!(rows.len(), 2);
        for row in &rows {
            let children = runtime.arena().get(*row).unwrap().children();
            assert_eq!(children.len(), 1, "compact row contains only its label");
            assert_eq!(
                runtime.arena().get(children[0]).unwrap().widget_type(),
                TypeId::of::<harbor_widget::Expanded>()
            );
            assert!(!close_is_visible(&runtime, *row));
        }

        let rect = runtime.arena().get(rows[1]).unwrap().layout_rect().unwrap();
        let position = Point::new(rect.max.x - 4.0, rect.min.y + 8.0);
        move_pointer(&mut runtime, position);
        assert!(!close_is_visible(&runtime, rows[1]));
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                position,
                phase,
                PointerButton::Left,
                7,
            )));
            runtime.update(Instant::now());
        }
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::rail(TabCommand::Activate(TabId(2)))]
        );

        runtime.request_focus(&controller.tab_focus(TabId(2)).unwrap());
        runtime.update(Instant::now());
        assert!(!close_is_visible(&runtime, rows[1]));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        }));
        runtime.update(Instant::now());
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::rail(TabCommand::New)]
        );
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('w'),
            modifiers: ctrl(),
        }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::shortcut(AppCommand::Tab(
                TabCommand::CloseActive
            ))]
        );
    }

    #[test]
    fn resizing_removes_and_restores_close_controls() {
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        for width in [1000, 899, 900, 899] {
            runtime.set_viewport(Viewport::new(width, 320, 1.0));
            controller.sync(
                vec![snapshot(1, true, false)],
                Some(bridge(1)),
                f64::from(width),
            );
            runtime.update(Instant::now());
            let row = tab_rows(&runtime)[0];
            let compact = width < 900;
            assert_eq!(
                runtime.arena().get(row).unwrap().children().len(),
                if compact { 1 } else { 3 }
            );
            assert_eq!(close_is_visible(&runtime, row), !compact);
        }
    }

    #[test]
    fn close_affordance_follows_selection_hover_and_both_keyboard_stops() {
        let controller = controller(
            1000.0,
            vec![snapshot(1, true, false), snapshot(2, false, false)],
            1,
        );
        let mut runtime = mount(controller.clone(), 1000, 320);
        let rows = tab_rows(&runtime);
        assert!(close_is_visible(&runtime, rows[0]));
        assert!(!close_is_visible(&runtime, rows[1]));
        let rect = runtime.arena().get(rows[1]).unwrap().layout_rect().unwrap();
        move_pointer(&mut runtime, Point::new(rect.min.x + 8.0, rect.min.y + 8.0));
        assert!(close_is_visible(&runtime, rows[1]));
        move_pointer(&mut runtime, Point::new(500.0, 200.0));
        assert!(!close_is_visible(&runtime, rows[1]));

        runtime.request_focus(&controller.tab_focus(TabId(2)).unwrap());
        runtime.update(Instant::now());
        assert!(close_is_visible(&runtime, rows[1]));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        }));
        runtime.update(Instant::now());
        assert!(close_is_visible(&runtime, rows[1]));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::close_rail(TabId(2))]
        );
        runtime.request_focus(&controller.terminal_focus());
        runtime.update(Instant::now());
        assert!(!close_is_visible(&runtime, rows[1]));
    }

    #[test]
    fn nested_close_click_never_activates_the_tab_and_survives_pressed_rebuilds() {
        for width in [900, 1000] {
            let controller = controller(
                f64::from(width),
                vec![snapshot(1, true, false), snapshot(2, false, false)],
                1,
            );
            let mut runtime = mount(controller.clone(), width, 320);
            let row = tab_rows(&runtime)[1];
            let close = *runtime.arena().get(row).unwrap().children().last().unwrap();
            let rect = runtime.arena().get(close).unwrap().layout_rect().unwrap();
            let position = Point::new(
                rect.min.x + rect.size().width / 2.0,
                rect.min.y + rect.size().height / 2.0,
            );
            move_pointer(&mut runtime, position);
            assert!(close_is_visible(&runtime, row));
            for phase in [PointerPhase::Down, PointerPhase::Up] {
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    position,
                    phase,
                    PointerButton::Left,
                    7,
                )));
                runtime.update(Instant::now());
            }
            assert_eq!(
                controller.drain_actions(),
                [AppCommandRequest::close_rail(TabId(2))]
            );
            assert!(runtime.input().captor(7).is_none());
        }
    }

    #[test]
    fn new_terminal_follows_tabs_and_remains_reachable_at_the_end_of_scroll() {
        for count in [2, 20] {
            let controller = controller(
                1000.0,
                (1..=count).map(|id| snapshot(id, id == 1, false)).collect(),
                1,
            );
            let mut runtime = mount(controller.clone(), 1000, 320);
            if count == 20 {
                controller
                    .scroll
                    .jump_to(controller.scroll.metrics().max_scroll_extent());
                runtime.update(Instant::now());
            }
            let last = *tab_rows(&runtime).last().unwrap();
            let last_rect = runtime.arena().get(last).unwrap().layout_rect().unwrap();
            let delta = runtime.pending_delta().unwrap();
            let origin = delta
                .added
                .iter()
                .chain(&delta.modified)
                .find_map(|item| match &item.primitive {
                    Primitive::Text { text, origin, .. } if text.as_ref() == "New terminal" => {
                        Some(*origin)
                    }
                    _ => None,
                })
                .expect("new terminal label");
            assert!(origin.y > last_rect.max.y);
            assert!(origin.y < 300.0, "new control must be in the viewport");
            if count == 2 {
                assert!(
                    origin.y < 120.0,
                    "new control must not be pinned to the bottom"
                );
            }
            for phase in [PointerPhase::Down, PointerPhase::Up] {
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    Point::new(origin.x + 4.0, origin.y + 4.0),
                    phase,
                    PointerButton::Left,
                    7,
                )));
            }
            assert_eq!(
                controller.drain_actions(),
                [AppCommandRequest::rail(TabCommand::New)]
            );
        }
    }

    #[test]
    fn long_unicode_titles_are_ellipsized_clipped_and_never_resize_close_controls() {
        let mut long = snapshot(1, true, false);
        long.title =
            "项目 / a very long terminal title that must not overlap the close button".into();
        let controller = controller(1000.0, vec![long.clone()], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        let row = tab_rows(&runtime)[0];
        let close = *runtime.arena().get(row).unwrap().children().last().unwrap();
        let close_rect = runtime.arena().get(close).unwrap().layout_rect().unwrap();
        let title = runtime.pending_delta().unwrap().added.iter().find(|item| {
            matches!(&item.primitive, Primitive::Text { text, .. } if text.starts_with("项目"))
        }).expect("truncated title");
        assert!(matches!(&title.primitive, Primitive::Text { text, .. } if text.ends_with('…')));
        assert!(
            title
                .clips
                .iter()
                .any(|clip| clip.rect().max.x <= close_rect.min.x)
        );
        assert_eq!(
            controller.store.state().read().snapshots[0].title,
            long.title
        );
        controller.sync(vec![snapshot(1, true, false)], Some(bridge(1)), 1000.0);
        runtime.update(Instant::now());
        assert_eq!(
            runtime.arena().get(close).unwrap().layout_rect().unwrap(),
            close_rect
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
    fn rail_icon_artwork_replaces_glyphs_in_expanded_and_compact_modes() {
        use harbor_widget::LucideIcon;
        for width in [899, 1000] {
            let controller = controller(
                f64::from(width),
                vec![snapshot(1, true, false), snapshot(2, false, true)],
                1,
            );
            let runtime = mount(controller, width, 320);
            let items = &runtime.pending_delta().unwrap().added;
            let icons: Vec<_> = items
                .iter()
                .filter_map(|item| match &item.primitive {
                    Primitive::Icon { icon, .. } => Some(*icon),
                    _ => None,
                })
                .collect();
            assert!(icons.contains(&LucideIcon::Plus));
            assert!(icons.contains(&LucideIcon::Dot));
            assert_eq!(icons.contains(&LucideIcon::Terminal), width >= 900);
            assert_eq!(icons.contains(&LucideIcon::X), width >= 900);
            let labels: Vec<_> = items
                .iter()
                .filter_map(|item| match &item.primitive {
                    Primitive::Text { text, .. } => Some(text.as_ref()),
                    _ => None,
                })
                .collect();
            assert!(
                !labels
                    .iter()
                    .any(|text| [">_", "•", "+", "×", "+  New terminal"].contains(text))
            );
            if width < 900 {
                assert!(labels.contains(&"1") && labels.contains(&"2"));
            } else {
                assert!(labels.contains(&"Terminal 1") && labels.contains(&"Terminal 2"));
                assert!(labels.contains(&"New terminal"));
            }
            for item in items {
                if let Primitive::Icon { rect, color, .. } = &item.primitive {
                    assert!(rect.is_valid());
                    assert!(color.is_finite());
                }
            }
        }
    }

    #[test]
    fn compact_abbreviations_keep_numbers_even_when_unread() {
        assert_eq!(abbreviation(&snapshot(123, false, true)), "3");
        assert_eq!(abbreviation(&snapshot(123, false, false)), "3");
    }

    #[test]
    #[allow(clippy::arc_with_non_send_sync)]
    fn unmounting_terminal_widget_clears_transient_preedit() {
        use harbor_widget::widgets::sized_box::SizedBox;

        let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
        terminal
            .lock()
            .unwrap()
            .handle_event(harbor_terminal::TerminalEvent::Preedit(
                harbor_terminal::Preedit::new("draft", Some((0, 5))),
            ))
            .unwrap();
        let controller = TabUiController::new(
            vec![snapshot(1, true, false)],
            TerminalWidgetBridge::new(1, Arc::clone(&terminal), Arc::new(AtomicBool::new(false))),
            1000.0,
        );
        let mut runtime = mount(controller, 1000, 320);

        runtime.set_root(SizedBox::new(harbor_widget::layout::Size::ZERO));
        runtime.update(Instant::now());

        assert!(terminal.lock().unwrap().preedit().is_none());
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
    #[allow(clippy::arc_with_non_send_sync)]
    fn native_escape_reaches_terminal_writer_in_primary_and_alternate_modes() {
        use harbor_widget::winit::WinitAdapter;
        use winit::{
            event::ElementState,
            keyboard::{Key as NativeKey, KeyLocation, NamedKey},
        };
        for alternate in [false, true] {
            let bytes = Arc::new(Mutex::new(Vec::new()));
            let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                4,
                20,
                std::io::empty(),
                Writer(bytes.clone()),
                || true,
            )));
            if alternate {
                engine.lock().unwrap().put_bytes(b"\x1b[?1049h");
            }
            let test_bridge =
                TerminalWidgetBridge::new(1, engine.clone(), Arc::new(AtomicBool::new(false)));
            let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
            let mut runtime = mount(controller.clone(), 1000, 320);
            controller.sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
            runtime.update(Instant::now());
            runtime.request_focus(&controller.terminal_focus());
            let mut adapter = WinitAdapter::new();
            let key = NativeKey::Named(NamedKey::Escape);
            adapter.handle_keyboard_input(
                &mut runtime,
                &key,
                ElementState::Pressed,
                KeyLocation::Standard,
            );
            adapter.handle_keyboard_input_with_repeat(
                &mut runtime,
                &key,
                ElementState::Pressed,
                KeyLocation::Standard,
                true,
            );
            adapter.handle_keyboard_input(
                &mut runtime,
                &key,
                ElementState::Released,
                KeyLocation::Standard,
            );
            assert!(controller.drain_actions().is_empty());
            assert_eq!(*bytes.lock().unwrap(), b"\x1b\x1b", "alternate={alternate}");
            assert_eq!(engine.lock().unwrap().is_alt_screen(), alternate);
        }
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
