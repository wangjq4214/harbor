//! Integrated application workspace regressions.
#[cfg(test)]
pub(super) fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Modifiers::default()
    }
}
use super::*;
use crate::command::{AppCommand, AppCommandRequest};
use crate::tab_manager::{TabId, TabIndex};
use crate::tab_view::{
    COMPACT_RAIL_WIDTH, RailPresentation, TabCommand, TabFocusPolicy, ui::abbreviation,
};
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

fn controller(width: f64, snapshots: Vec<TabSnapshot>, active_draw: u64) -> WorkspaceUiController {
    WorkspaceUiController::new(snapshots, bridge(active_draw), width)
}

fn mount(controller: WorkspaceUiController, width: u32, height: u32) -> Runtime {
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
fn notification_preserves_terminal_focus_allocation_and_routes_dismissal_as_host_intent() {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        std::io::empty(),
        Writer(bytes.clone()),
        || true,
    )));
    let bridge = TerminalWidgetBridge::new(1, engine, Arc::new(AtomicBool::new(false)));
    let controller = WorkspaceUiController::new(vec![snapshot(1, true, false)], bridge, 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    let focused = runtime.input().focused();
    let allocation = controller.latest_terminal_allocation().unwrap();
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("Invalid settings; previously accepted configuration remains effective".into()),
    );
    runtime.update(Instant::now());
    assert_eq!(runtime.input().focused(), focused);
    assert_eq!(controller.latest_terminal_allocation(), Some(allocation));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('a'),
        modifiers: Modifiers::default(),
    }));
    assert_eq!(
        &*bytes.lock().unwrap(),
        b"a",
        "notification must not inject PTY bytes or gate typing"
    );
    // Empty overlay space still permits normal rail interaction.
    let row = tab_rows(&runtime)[0];
    let rect = runtime.arena().get(row).unwrap().layout_rect().unwrap();
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(rect.min.x + 8.0, rect.min.y + 8.0),
            phase,
            PointerButton::Left,
            7,
        )));
    }
    assert_eq!(
        controller.drain_actions()[0].command,
        AppCommand::Tab(TabCommand::Activate(TabId(1)))
    );
    runtime.request_focus(&controller.terminal_focus());
    for (width, height, scale) in [(1000, 500, 1.0), (1200, 800, 2.0)] {
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        let before = controller.latest_terminal_allocation();
        let focused = runtime.input().focused();
        let delta = runtime.pending_delta().unwrap();
        let origin = delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match &item.primitive {
                Primitive::Text { text, origin, .. } if text.as_ref() == "×" => Some(*origin),
                _ => None,
            })
            .expect("visible close control");
        assert!((origin.x - (width as f32 / scale - 38.0)).abs() < 1.0);
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(origin.x + 5.0, origin.y + 5.0),
                phase,
                PointerButton::Left,
                8,
            )));
        }
        let actions = controller.drain_actions();
        assert_eq!(actions.len(), 1);
        let AppCommand::DismissNotification(revision) = actions[0].command else {
            panic!("dismiss action");
        };
        assert_eq!(actions[0].focus, TabFocusPolicy::PreserveRail);
        assert!(
            controller.store.state().read().notices.message().is_some(),
            "routing never mutates Host state"
        );
        controller.dismiss_notification(revision);
        runtime.update(Instant::now());
        assert!(controller.store.state().read().notices.message().is_none());
        assert_eq!(runtime.input().focused(), focused);
        assert_eq!(controller.latest_terminal_allocation(), before);
        controller.publish_configuration_notice(
            ConfigurationNoticeSource::Reload,
            Some(format!("A different error after {scale}")),
        );
        runtime.update(Instant::now());
    }
}

#[test]
fn notification_dismissal_suppression_recovery_and_independent_sources() {
    use ConfigurationNoticeSource as Source;
    let mut state = ConfigurationNotices::default();
    state.publish(Source::Reload, Some("invalid TOML".into()));
    let old_revision = state.revision;
    state.dismiss(old_revision);
    state.publish(Source::Reload, Some("invalid TOML".into()));
    assert!(
        state.message().is_none(),
        "identical watcher failure stays dismissed"
    );
    state.publish(Source::Reload, Some("invalid color".into()));
    state.dismiss(old_revision);
    assert_eq!(
        state.message().as_deref(),
        Some("invalid color"),
        "stale dismissal cannot hide newer feedback"
    );
    state.publish(
        Source::Observation,
        Some("watching unavailable; manual reload remains available".into()),
    );
    state.publish(Source::Reload, None);
    assert!(
        state.message().unwrap().contains("manual reload"),
        "manual success cannot claim observation recovery"
    );
    state.publish(Source::Observation, None);
    assert!(
        state.message().is_none(),
        "recovery removes obsolete failures without a success toast"
    );
    state.publish(Source::Resource, Some("session 1 needs recovery".into()));
    state.publish(Source::Reload, None); // a later missing file is not resource recovery
    assert_eq!(state.message().as_deref(), Some("session 1 needs recovery"));
    state.publish(Source::Resource, None); // successful application transaction
    state.publish(Source::Reload, Some("invalid TOML".into()));
    assert_eq!(
        state.message().as_deref(),
        Some("invalid TOML"),
        "failure after recovery is a new episode"
    );
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
    controller.tabs().sync(
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
    controller
        .tabs()
        .sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
    assert!(controller.store.state().read().keybindings.is_some());
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
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
        let test_bridge = TerminalWidgetBridge::new(1, engine, Arc::new(AtomicBool::new(false)));
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        controller
            .tabs()
            .sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
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
    let controller = WorkspaceUiController::new(
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
    assert!(!controller.tabs().update_presentation(100.0));
    assert!(controller.tabs().update_presentation(900.0));
    assert!(!controller.tabs().update_presentation(1200.0));
    assert!(controller.tabs().update_presentation(f64::NAN));
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
                    if color.a > 0.0 && rect.min.x >= rail.min.x && rect.max.x <= rail.max.x =>
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

    runtime.request_focus(&controller.tabs().tab_focus(TabId(2)).unwrap());
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
        controller.tabs().sync(
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

    runtime.request_focus(&controller.tabs().tab_focus(TabId(2)).unwrap());
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
                .tabs
                .scroll
                .jump_to(controller.tabs.scroll.metrics().max_scroll_extent());
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
    long.title = "项目 / a very long terminal title that must not overlap the close button".into();
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
    assert_eq!(controller.tabs.state.read().snapshots[0].title, long.title);
    controller
        .tabs()
        .sync(vec![snapshot(1, true, false)], Some(bridge(1)), 1000.0);
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
    assert!(controller.tabs.scroll.metrics().max_scroll_extent() > 0.0);

    for _ in 0..24 {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        }));
        runtime.update(Instant::now());
    }
    assert!(controller.tabs.scroll.metrics().offset() > 0.0);

    controller
        .tabs()
        .sync(vec![snapshot(1, true, false)], Some(bridge(1)), 1000.0);
    runtime.update(Instant::now());
    assert_eq!(controller.tabs.scroll.metrics().max_scroll_extent(), 0.0);
    assert_eq!(controller.tabs.scroll.metrics().offset(), 0.0);
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

    controller.tabs().sync(
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

    controller.tabs().sync(
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
    let controller = WorkspaceUiController::new(
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
    let controller = WorkspaceUiController::new(
        vec![snapshot(1, true, false)],
        TerminalWidgetBridge::new(1, Arc::clone(&terminal), Arc::new(AtomicBool::new(false))),
        1000.0,
    );
    let mut runtime = mount(controller.clone(), 1000, 320);
    drop(terminal);

    controller.tabs().sync(Vec::new(), None, 1000.0);
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
        controller
            .tabs()
            .sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
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

#[test]
fn global_publication_survives_tab_mutations_responsive_changes_and_root_replacement() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut raw = harbor_config::RawKeybindings::default();
    raw.entries.insert(
        "app.reload-configuration".into(),
        vec![harbor_config::RawKeybinding {
            modifiers: vec![],
            key: "q".into(),
        }],
    );
    controller.publish_keybindings(crate::command::validate_keybindings(&raw).unwrap());
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    let mut runtime = mount(controller.clone(), 1000, 320);
    let revision = controller.store.state().read().notices.revision;
    controller.dismiss_notification(revision);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Observation,
        Some("watch unavailable".into()),
    );
    let global_version = controller.store.state().version();
    for (snapshots, active, width) in [
        (
            vec![snapshot(1, false, false), snapshot(2, true, false)],
            2,
            899.0,
        ),
        (vec![snapshot(2, true, false)], 2, 1000.0),
        (
            vec![snapshot(2, false, true), snapshot(3, true, false)],
            3,
            500.0,
        ),
    ] {
        controller
            .tabs()
            .sync(snapshots, Some(bridge(active)), width);
        controller.tabs().update_presentation(width + 1.0);
        runtime.update(Instant::now());
        assert_eq!(
            controller.store.state().version(),
            global_version,
            "tabs cannot republish global state"
        );
        assert_eq!(
            controller.store.state().read().notices.message().as_deref(),
            Some("watch unavailable")
        );
        runtime.request_focus(&controller.terminal_focus());
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('q'),
            modifiers: Modifiers::default(),
        }));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Character('q'),
            modifiers: Modifiers::default(),
        }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::shortcut(AppCommand::ReloadConfiguration)]
        );
        assert!(controller.drain_actions().is_empty());
    }
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
        Some("watch unavailable")
    );
    controller.publish_configuration_notice(ConfigurationNoticeSource::Observation, None);
    assert!(controller.store.state().read().notices.message().is_none());
    controller.publish_configuration_notice(ConfigurationNoticeSource::Reload, None);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
        Some("invalid TOML")
    );
    runtime.set_root(harbor_widget::widgets::sized_box::SizedBox::new(
        harbor_widget::layout::Size::ZERO,
    ));
    runtime.update(Instant::now());
    runtime.set_root(tab_workspace(controller.clone(), false));
    runtime.update(Instant::now());
    runtime.request_focus(&controller.terminal_focus());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('q'),
        modifiers: Modifiers::default(),
    }));
    assert_eq!(controller.drain_actions().len(), 1);
    assert_eq!(controller.tabs.state.read().snapshots.len(), 2);
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
        Some("invalid TOML")
    );
}

fn search_panel_rect(runtime: &Runtime) -> Rect {
    fiber_ids(runtime)
        .into_iter()
        .find_map(|id| {
            let fiber = runtime.arena().get(id).unwrap();
            let rect = fiber.layout_rect()?;
            (fiber.widget_type() == TypeId::of::<DecoratedBox>()
                && rect.size().height == SEARCH_PANEL_HEIGHT)
                .then_some(rect)
        })
        .expect("floating search card")
}

#[test]
fn search_editor_publishes_initial_no_result_instead_of_an_immediate_busy_hint() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.open_search(TabId(1));
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.search_focus());
    controller.publish_search_status("1 / 3");
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("needle".into())));
    runtime.update(Instant::now());
    assert_eq!(controller.store.state().read().search.status, "No Result");
    assert_eq!(controller.search_query().unwrap().1, "needle");
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('a'),
        modifiers: ctrl(),
    }));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Backspace,
        modifiers: Modifiers::default(),
    }));
    runtime.update(Instant::now());
    assert_eq!(controller.store.state().read().search.status, "No Result");
    assert_eq!(controller.search_query().unwrap().1, "");
}

#[test]
fn search_status_changes_do_not_resize_input_or_shift_result_and_buttons() {
    for (width, height, scale) in [
        (1000, 500, 1.0),
        (1200, 800, 1.5),
        (1200, 800, 2.0),
        (320, 240, 1.0),
    ] {
        let controller = controller(
            width as f64 / scale as f64,
            vec![snapshot(1, true, false)],
            1,
        );
        controller.open_search(TabId(1));
        let mut runtime = mount(controller.clone(), width, height);
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.request_focus(&controller.search_focus());
        runtime.update(Instant::now());
        let focused = runtime.input().focused();
        let allocation = controller.latest_terminal_allocation();
        let geometry = |runtime: &Runtime| {
            let panel = search_panel_rect(runtime);
            let rect_for = |widget_type| {
                fiber_ids(runtime)
                    .into_iter()
                    .find_map(|id| {
                        let fiber = runtime.arena().get(id).unwrap();
                        let rect = fiber.layout_rect()?;
                        (fiber.widget_type() == widget_type
                            && panel.contains(rect.min)
                            && rect.size().height < panel.size().height)
                            .then_some(rect)
                    })
                    .expect("search field/status layout")
            };
            let input = rect_for(TypeId::of::<DecoratedBox>());
            let result = rect_for(TypeId::of::<harbor_widget::widgets::text_label::TextLabel>());
            // No GPU encode consumes the initial delta, so coalescing retains the icons
            // even if a status change leaves their primitives unchanged.
            let delta = runtime.pending_delta().unwrap();
            let mut buttons: Vec<_> = delta
                .added
                .iter()
                .chain(&delta.modified)
                .filter_map(|item| match item.primitive {
                    Primitive::Icon { icon, rect, .. }
                        if panel.contains(rect.min)
                            && matches!(
                                icon,
                                LucideIcon::ChevronUp | LucideIcon::ChevronDown | LucideIcon::X
                            ) =>
                    {
                        Some(rect)
                    }
                    _ => None,
                })
                .collect();
            buttons.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
            assert_eq!(buttons.len(), 3);
            (input, result, buttons)
        };
        let initial = geometry(&runtime);
        for _ in 0..3 {
            for status in [
                "Searching…",
                "— / 9",
                "1 / 9",
                "9 / 10",
                "1024 / 1024",
                "partial/limited · — / 1024",
                "Searching… · 1 / 1024",
                "No Result",
            ] {
                controller.publish_search_status(status);
                runtime.update(Instant::now());
                assert_eq!(
                    geometry(&runtime),
                    initial,
                    "status={status}, viewport={width}x{height}@{scale}"
                );
                assert_eq!(runtime.input().focused(), focused);
                assert_eq!(controller.latest_terminal_allocation(), allocation);
            }
        }
    }
}

#[test]
fn search_overlay_preserves_terminal_geometry_and_uses_equal_icon_controls() {
    use harbor_widget::layout::Size;
    for (width, height, scale) in [(1000, 500, 1.0), (1200, 800, 2.0), (320, 240, 1.0)] {
        let controller = controller(
            width as f64 / scale as f64,
            vec![snapshot(1, true, false)],
            1,
        );
        let mut runtime = mount(controller.clone(), width, height);
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        runtime.request_focus(&controller.terminal_focus());
        let terminal_focus = runtime.input().focused();
        let allocation = controller.latest_terminal_allocation();
        controller.open_search(TabId(1));
        runtime.request_focus(&controller.search_focus());
        runtime.update(Instant::now());
        assert_eq!(controller.latest_terminal_allocation(), allocation);
        let panel = search_panel_rect(&runtime);
        assert_eq!(panel.min.y, SEARCH_PANEL_INSET);
        assert_eq!(panel.max.x, width as f32 / scale - SEARCH_PANEL_INSET);
        assert_eq!(
            panel.size().width,
            440.0_f32.min(width as f32 / scale - 24.0)
        );
        assert_ne!(runtime.input().focused(), terminal_focus);

        let delta = runtime.pending_delta().unwrap();
        let mut icons: Vec<_> = delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| match item.primitive {
                Primitive::Icon { icon, rect, .. }
                    if panel.contains(rect.min)
                        && matches!(
                            icon,
                            LucideIcon::ChevronUp | LucideIcon::ChevronDown | LucideIcon::X
                        ) =>
                {
                    Some((icon, rect))
                }
                _ => None,
            })
            .collect();
        icons.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));
        assert_eq!(
            icons.iter().map(|(icon, _)| *icon).collect::<Vec<_>>(),
            [
                LucideIcon::ChevronUp,
                LucideIcon::ChevronDown,
                LucideIcon::X
            ]
        );
        assert!(
            icons
                .iter()
                .all(|(_, rect)| rect.size() == Size::new(18.0, 18.0))
        );
        let mut buttons: Vec<_> = delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| match item.primitive {
                Primitive::Quad { rect, .. }
                    if panel.contains(rect.min) && rect.size() == Size::new(32.0, 32.0) =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .collect();
        buttons.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[1].min.x - buttons[0].max.x, 4.0);
        assert_eq!(buttons[2].min.x - buttons[1].max.x, 4.0);
        assert!(!delta.added.iter().chain(&delta.modified).any(|item| {
            matches!(&item.primitive, Primitive::Text { text, .. }
                if ["Previous", "Next", "Close"].contains(&text.as_ref()))
        }));
        let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
        assert!(items.iter().any(|item| matches!(&item.primitive,
            Primitive::BackdropBlur { rect, sigma: 8.0, .. } if *rect == panel)));
        let surface = items
            .iter()
            .find_map(|item| match &item.primitive {
                Primitive::PaintedQuad {
                    rect,
                    paint: harbor_widget::Paint::LinearGradient(gradient),
                    corner_radii,
                } if *rect == panel => {
                    assert_eq!(
                        *corner_radii, [12.0; 4],
                        "material does not reshape the panel"
                    );
                    Some(gradient)
                }
                _ => None,
            })
            .expect("panel uses generic gradient fill");
        assert!(
            surface
                .stops()
                .iter()
                .all(|stop| stop.color().a > 0.0 && stop.color().a < 0.2),
            "surface preserves backdrop colors rather than covering them with gray"
        );
        assert!(items.iter().any(|item| matches!(&item.primitive,
            Primitive::PaintedBorder { rect, paint: harbor_widget::Paint::LinearGradient(gradient), corner_radii, .. }
                if *rect == panel && *corner_radii == [12.0; 4]
                && gradient.stops()[0].color().a > gradient.stops()[1].color().a)));
        assert!(items.iter().any(|item| matches!(&item.primitive,
            Primitive::OuterShadow { occluder_rect, color, .. } if *occluder_rect == panel && color.a > 0.0)));
        let (status_origin, _) = items
            .iter()
            .find_map(|item| match &item.primitive {
                Primitive::Text { text, origin, .. } if text.as_ref() == "No Result" => {
                    Some((*origin, text))
                }
                _ => None,
            })
            .expect("default result appears on the same row");
        assert!(status_origin.x < icons[0].1.min.x);
        assert!((status_origin.y - (panel.min.y + panel.size().height / 2.0)).abs() < 20.0);
        assert!(
            items.iter().any(|item| matches!(&item.primitive,
            Primitive::RoundedQuad { rect, color, corner_radii }
                if panel.contains(rect.min) && rect.size().width < panel.size().width
                && color.r == 0.0 && color.a > 0.0 && color.a < 0.24 && *corner_radii == [7.0; 4]))
        );
        assert!(
            items
                .iter()
                .filter(|item| matches!(&item.primitive,
            Primitive::Quad { rect, .. } if buttons.contains(rect)))
                .all(|item| matches!(&item.primitive,
                Primitive::Quad { color, .. } if color.a == 0.0))
        );

        for ((_, rect), command) in icons.into_iter().zip([
            AppCommand::SearchPrevious,
            AppCommand::SearchNext,
            AppCommand::SearchClose,
        ]) {
            for phase in [PointerPhase::Down, PointerPhase::Up] {
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    Point::new(rect.min.x + 9.0, rect.min.y + 9.0),
                    phase,
                    PointerButton::Left,
                    42,
                )));
            }
            assert_eq!(
                controller.drain_actions(),
                [AppCommandRequest::shortcut(command)]
            );
        }
        // Blank overlay space still permits normal terminal interaction.
        let terminal = allocation.unwrap();
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(
                    terminal.min.x + 8.0,
                    terminal.min.y + terminal.size().height / 2.0,
                ),
                phase,
                PointerButton::Left,
                43,
            )));
        }
        assert!(
            runtime
                .arena()
                .get(terminal_focus.unwrap())
                .unwrap()
                .children()
                .contains(&runtime.input().focused().unwrap()),
            "click reaches the terminal under its focus wrapper"
        );
        controller.close_search();
        runtime.request_focus(&controller.terminal_focus());
        runtime.update(Instant::now());
        assert_eq!(
            runtime.input().focused(),
            terminal_focus,
            "terminal fiber survives open/close"
        );
        assert_eq!(controller.latest_terminal_allocation(), allocation);
    }
}

#[test]
fn configuration_notice_stays_below_search_and_returns_when_search_closes() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("Invalid settings".into()),
    );
    let mut runtime = mount(controller.clone(), 1000, 500);
    let notice_close_y = |runtime: &Runtime| {
        let delta = runtime.pending_delta().unwrap();
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match &item.primitive {
                Primitive::Text { text, origin, .. } if text.as_ref() == "×" => Some(origin.y),
                _ => None,
            })
            .unwrap()
    };
    let original_y = notice_close_y(&runtime);
    let allocation = controller.latest_terminal_allocation();
    controller.open_search(TabId(1));
    runtime.update(Instant::now());
    let panel = search_panel_rect(&runtime);
    assert!(notice_close_y(&runtime) > panel.max.y);
    assert_eq!(controller.latest_terminal_allocation(), allocation);
    controller.close_search();
    runtime.update(Instant::now());
    assert_eq!(notice_close_y(&runtime), original_y);
    assert!(controller.store.state().read().notices.message().is_some());
}

#[test]
fn current_session_search_entry_routes_typed_commands_without_pty_leakage() {
    use crate::tab_manager::{TabManager, TerminalTabResources};
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| {
        #[allow(clippy::arc_with_non_send_sync)]
        let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            4,
            20,
            std::io::empty(),
            Writer(bytes.clone()),
            || true,
        )));
        engine
            .lock()
            .unwrap()
            .put_str("needle\r\nneedle\r\nother\r\nlive\r\n");
        let bridge =
            TerminalWidgetBridge::new(draw, engine.clone(), Arc::new(AtomicBool::new(false)));
        Ok(TerminalTabResources::new(engine, bridge, "test"))
    })
    .unwrap();
    let controller =
        WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    let key = |key, modifiers| UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers });
    runtime.dispatch(key(
        Key::Character('f'),
        Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        },
    ));
    let actions = controller.drain_actions();
    assert_eq!(
        actions,
        [AppCommandRequest::shortcut(AppCommand::SearchOpen)]
    );
    let focus = tabs
        .dispatch_search_command(actions[0].command, &controller)
        .unwrap();
    runtime.request_focus(&focus); // requests before mount must survive reconciliation
    runtime.update(Instant::now());
    for ch in "needle".chars() {
        runtime.dispatch(key(Key::Character(ch), Modifiers::default()));
    }
    runtime.dispatch(UiEvent::ImePreedit(
        harbor_widget::input::event::ImePreedit::new("界", None),
    ));
    assert_eq!(controller.search_query().unwrap().1, "needle");
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    runtime.dispatch(key(Key::Backspace, Modifiers::default()));
    assert_eq!(controller.search_query().unwrap().1, "needle");
    for action in controller.drain_actions() {
        tabs.dispatch_search_command(action.command, &controller);
    }
    for _ in 0..100 {
        if !tabs.advance_search().pending {
            break;
        }
    }
    assert_eq!(tabs.active_bridge().unwrap().decoration_ranges().len(), 2);
    for event in [
        key(Key::Enter, Modifiers::default()),
        key(
            Key::Enter,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ),
        key(Key::F3, Modifiers::default()),
    ] {
        runtime.dispatch(event);
        let actions = controller.drain_actions();
        assert_eq!(actions.len(), 1);
        tabs.dispatch_search_command(actions[0].command, &controller);
    }
    assert!(
        tabs.active_terminal()
            .unwrap()
            .lock()
            .unwrap()
            .screen()
            .view_offset()
            > 0
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "all consumed query/IME/navigation input stays out of PTY"
    );
    runtime.dispatch(key(Key::Escape, Modifiers::default()));
    let actions = controller.drain_actions();
    assert_eq!(actions[0].command, AppCommand::SearchClose);
    let focus = tabs
        .dispatch_search_command(actions[0].command, &controller)
        .unwrap();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    assert!(tabs.active_bridge().unwrap().decoration_ranges().is_empty());
    runtime.dispatch(key(Key::Character('x'), Modifiers::default()));
    assert_eq!(
        &*bytes.lock().unwrap(),
        b"x",
        "normal pass-through resumes after close"
    );
}

#[test]
fn search_entry_limits_and_query_cancellation_are_visible_and_coalesced() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.open_search(TabId(1));
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.search_focus());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("a".repeat(128))));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    assert_eq!(controller.search_query().unwrap().1.len(), 128);
    assert_eq!(controller.drain_actions().len(), 1);
    for _ in 0..128 {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Backspace,
            modifiers: Modifiers::default(),
        }));
    }
    assert_eq!(controller.search_query().unwrap().1, "");
    controller.sync_search_source(Some(TabId(2)));
    assert!(controller.search_query().is_none());
    runtime.update(Instant::now());
}

#[test]
fn reusable_search_editor_replaces_at_limit_and_keeps_clipboard_out_of_terminal_commands() {
    use harbor_widget::{TextSelection, effects::ClipboardEffect};
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.open_search(TabId(1));
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.search_focus());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("a".repeat(128))));
    controller.drain_actions();
    let input = controller.store.state().read().search.input.clone();
    input.set_selection(TextSelection {
        anchor: 125,
        extent: 128,
    });
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    assert_eq!(
        controller.search_query().unwrap().1,
        format!("{}界", "a".repeat(125))
    );
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::shortcut(AppCommand::SearchChanged)]
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('a'),
        modifiers: ctrl(),
    }));
    let copied = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('c'),
        modifiers: ctrl(),
    }));
    assert!(matches!(copied.clipboard, Some(ClipboardEffect::Editor(_))));
    assert!(
        controller.drain_actions().is_empty(),
        "copy must not enqueue the terminal copy command"
    );
    let paste = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(request)) = paste.clipboard else {
        panic!("expected editor paste");
    };
    runtime.complete_clipboard(request.id, Ok("needle".into()));
    assert_eq!(controller.search_query().unwrap().1, "needle");
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::shortcut(AppCommand::SearchChanged)]
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('z'),
        modifiers: ctrl(),
    }));
    assert_eq!(controller.search_query().unwrap().1.len(), 128);
    assert_eq!(controller.drain_actions().len(), 1);
    assert!(runtime.drain_external_input().is_empty());
    let late = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(request)) = late.clipboard else {
        panic!("expected editor paste");
    };
    controller.sync_search_source(Some(TabId(2)));
    controller.open_search(TabId(2));
    runtime.update(Instant::now());
    runtime.request_focus(&controller.search_focus());
    runtime.complete_clipboard(request.id, Ok("wrong session".into()));
    assert_eq!(controller.search_query().unwrap().1, "");
}
