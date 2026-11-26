//! Mounted production workspace routing; not native desktop/IME acceptance.
use harbor_app::{
    command::{AppCommand, AppCommandRequest, PaneCommand},
    tab_manager::{PaneBinding, TabManager, TerminalTabResources},
    terminal_view::TerminalWidgetBridge,
    workspace::{WorkspaceUiController, tab_workspace},
};
use harbor_terminal::{Terminal, TextMetrics};
use harbor_widget::{
    SplitAxis,
    input::event::{
        ImePreedit, Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
        UiEvent,
    },
    layout::Point,
    renderer::Viewport,
    runtime::Runtime,
};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc},
    time::Instant,
};

struct Reader(mpsc::Receiver<()>);
impl Read for Reader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.0.recv();
        Ok(0)
    }
}
#[derive(Clone)]
struct Writer(Arc<Mutex<Vec<u8>>>);
impl Write for Writer {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Fixture {
    tabs: TabManager,
    ui: WorkspaceUiController,
    rt: Runtime,
    panes: Vec<PaneBinding>,
    bytes: Vec<Arc<Mutex<Vec<u8>>>>,
    readers: Vec<mpsc::Sender<()>>,
    gate: Arc<AtomicBool>,
    scale: f32,
}
fn metrics() -> TextMetrics {
    TextMetrics {
        cell_width: 8.0,
        line_height: 16.0,
        ..harbor_widget::runtime::DEFAULT_TEXT_METRICS
    }
}
impl Fixture {
    #[allow(clippy::arc_with_non_send_sync)]
    fn new(scale: f32) -> Self {
        let mut tabs = TabManager::new();
        let gate = Arc::new(AtomicBool::new(false));
        let mut bytes = Vec::new();
        let mut readers = Vec::new();
        let mut make = |draw| {
            let data = Arc::new(Mutex::new(Vec::new()));
            let (tx, rx) = mpsc::channel();
            readers.push(tx);
            bytes.push(data.clone());
            let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
                12,
                30,
                Reader(rx),
                Writer(data),
                || true,
            )));
            let bridge = TerminalWidgetBridge::new(draw, terminal.clone(), gate.clone());
            TerminalTabResources::new(terminal, bridge, "routing")
        };
        tabs.create_tab(|_, draw| Ok(make(draw))).unwrap();
        let a = tabs.active_binding().unwrap();
        let (b, draw) = tabs.reserve_split_ids(a).unwrap();
        tabs.attach_split(a, b, draw, make(draw), SplitAxis::Horizontal)
            .unwrap();
        let (c, draw) = tabs.reserve_split_ids(b).unwrap();
        tabs.attach_split(b, c, draw, make(draw), SplitAxis::Vertical)
            .unwrap();
        let ui =
            WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1200.0);
        ui.sync_panes(&tabs, 1200.0);
        let mut rt = Runtime::new();
        rt.set_viewport(Viewport::new(
            (1200.0 * scale) as u32,
            (700.0 * scale) as u32,
            scale,
        ));
        rt.set_root(tab_workspace(ui.clone(), false));
        rt.update(Instant::now());
        rt.request_focus(&ui.terminal_focus());
        let mut fixture = Self {
            tabs,
            ui,
            rt,
            panes: vec![a, b, c],
            bytes,
            readers,
            gate,
            scale,
        };
        fixture.geometry();
        fixture
    }
    fn geometry(&mut self) {
        self.rt.update(Instant::now());
        self.ui.publish_pane_input_metrics(
            self.scale,
            ((1200.0 * self.scale) as u32, (700.0 * self.scale) as u32),
            metrics(),
        );
        self.tabs.observe_pane_allocations(
            &self.ui.pane_allocations(),
            self.scale,
            ((1200.0 * self.scale) as u32, (700.0 * self.scale) as u32),
            &metrics(),
        );
    }
    fn terminal(&self, index: usize) -> Arc<Mutex<Terminal>> {
        self.tabs
            .session_terminal(self.panes[index].session)
            .unwrap()
    }
    fn point(&self, index: usize, col: usize, row: usize) -> Point {
        let rect = self
            .ui
            .pane_allocations()
            .into_iter()
            .find(|a| a.binding == self.panes[index])
            .unwrap()
            .rect;
        Point::new(
            rect.min.x + (16.0 + col as f32 * 8.0 + 1.0) / self.scale,
            rect.min.y + (16.0 + row as f32 * 16.0 + 1.0) / self.scale,
        )
    }
    fn pointer(&mut self, point: Point, phase: PointerPhase) {
        self.rt.dispatch(UiEvent::Pointer(PointerEvent::new(
            point,
            phase,
            PointerButton::Left,
            71,
        )));
    }
    fn reduce(&mut self) -> Vec<AppCommand> {
        let actions = self.ui.drain_actions();
        let mut seen = Vec::new();
        for request in actions {
            seen.push(request.command);
            if let AppCommand::Pane(command) = request.command {
                if !matches!(command, PaneCommand::Focus(_)) {
                    self.rt.cancel_active_input_ownership(Point::ZERO);
                }
                self.tabs.execute_pane_command(command);
                self.ui.sync_panes(&self.tabs, 1200.0);
                if !matches!(command, PaneCommand::Focus(_)) {
                    self.rt.request_focus(&self.ui.terminal_focus());
                }
            }
        }
        self.geometry();
        seen
    }
    fn chord(&mut self, key: char, shift: bool) {
        let modifiers = Modifiers {
            ctrl: true,
            shift,
            ..Default::default()
        };
        self.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character(key),
            modifiers,
        }));
        self.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Character(key),
            modifiers,
        }));
    }
    fn clear_bytes(&self) {
        for bytes in &self.bytes {
            bytes.lock().unwrap().clear();
        }
    }
}

#[test]
fn mixed_panes_focus_markers_commands_overrides_and_consumption() {
    for scale in [1.0, 1.5] {
        let mut f = Fixture::new(scale);
        for (index, marker) in ["ALPHA", "BETA", "GAMMA"].iter().enumerate() {
            let p = f.point(index, 0, 0);
            f.pointer(p, PointerPhase::Down);
            f.reduce();
            f.pointer(p, PointerPhase::Up);
            f.reduce();
            assert_eq!(f.tabs.active_binding(), Some(f.panes[index]));
            f.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime((*marker).into())));
            assert_eq!(*f.bytes[index].lock().unwrap(), marker.as_bytes());
        }
        f.clear_bytes();
        f.chord('{', true);
        assert_eq!(f.reduce(), [AppCommand::Pane(PaneCommand::Previous)]);
        assert_eq!(f.tabs.active_binding(), Some(f.panes[1]));
        f.chord('m', true);
        f.reduce();
        assert_eq!(f.tabs.visible_bindings(), [f.panes[1]]);
        f.chord('m', true);
        f.reduce();
        assert_eq!(f.tabs.visible_bindings().len(), 3);
        for (key, command) in [
            ('d', PaneCommand::SplitHorizontal),
            ('e', PaneCommand::SplitVertical),
            ('}', PaneCommand::Next),
        ] {
            f.chord(key, true);
            assert_eq!(f.ui.drain_actions()[0].command, AppCommand::Pane(command));
        }
        assert!(f.bytes.iter().all(|bytes| bytes.lock().unwrap().is_empty()));
        // Explicit close-tab override keeps tab meaning; removing pane-close avoids conflict.
        let settings = harbor_config::Settings {
            keybindings: harbor_config::RawKeybindings {
                entries: [
                    ("app.close-pane".into(), vec![]),
                    (
                        "app.close-active-tab".into(),
                        vec![harbor_config::RawKeybinding {
                            modifiers: vec!["ctrl".into()],
                            key: "w".into(),
                        }],
                    ),
                ]
                .into_iter()
                .collect(),
                valid: true,
            },
            ..Default::default()
        };
        f.ui.publish_keybindings(
            harbor_app::command::validate_keybindings(&settings.keybindings).unwrap(),
        );
        f.rt.update(Instant::now());
        f.chord('w', false);
        assert_eq!(
            f.ui.drain_actions(),
            [AppCommandRequest::shortcut(AppCommand::Tab(
                harbor_app::tab_view::TabCommand::CloseActive
            ))]
        );
        assert!(f.bytes.iter().all(|bytes| bytes.lock().unwrap().is_empty()));
    }
}

#[test]
fn pointer_reporting_capture_and_wheel_are_pane_local_not_focus_local() {
    for scale in [1.0, 1.5] {
        let mut f = Fixture::new(scale);
        for index in 0..3 {
            f.terminal(index)
                .lock()
                .unwrap()
                .process_output(b"\x1b[?1002h\x1b[?1006h");
        }
        for index in 0..3 {
            f.clear_bytes();
            let p = f.point(index, 2, 1);
            f.pointer(p, PointerPhase::Down);
            f.reduce();
            assert_eq!(*f.bytes[index].lock().unwrap(), b"\x1b[<0;3;2M");
            assert!(f.rt.input().captor(71).is_some());
            let other = f.point((index + 1) % 3, 0, 0);
            f.pointer(other, PointerPhase::Move);
            assert!(
                f.bytes[(index + 1) % 3].lock().unwrap().is_empty(),
                "capture must beat hit test"
            );
            f.pointer(other, PointerPhase::Up);
            assert!(f.rt.input().captor(71).is_none());
            f.reduce();
            let before = f.bytes[index].lock().unwrap().len();
            f.pointer(p, PointerPhase::WheelLine { dx: 0.0, dy: 1.0 });
            assert!(f.bytes[index].lock().unwrap().len() > before);
            assert!(
                f.bytes
                    .iter()
                    .enumerate()
                    .all(|(i, b)| i == index || b.lock().unwrap().is_empty())
            );
        }
        // A committed allocation can move before the next render/Host resize
        // submission. The first hit must still use the new local origin.
        let harbor_app::pane_layout::PaneLayout::Split { ratio, .. } =
            f.tabs.active_layout().unwrap()
        else {
            panic!()
        };
        ratio.set_ratio(0.4);
        f.rt.update(Instant::now());
        f.clear_bytes();
        let p = f.point(1, 2, 1);
        f.pointer(p, PointerPhase::Down);
        assert_eq!(*f.bytes[1].lock().unwrap(), b"\x1b[<0;3;2M");
        f.pointer(p, PointerPhase::Up);
        f.reduce();
        // Divider capture may never report into either adjacent terminal.
        f.clear_bytes();
        let a =
            f.ui.pane_allocations()
                .into_iter()
                .find(|a| a.binding == f.panes[0])
                .unwrap()
                .rect;
        let p = Point::new(a.max.x + 3.0, a.min.y + 25.0);
        f.pointer(p, PointerPhase::Down);
        f.pointer(Point::new(p.x + 20.0, p.y), PointerPhase::Move);
        f.pointer(p, PointerPhase::Up);
        assert!(f.bytes.iter().all(|b| b.lock().unwrap().is_empty()));
    }
}

#[test]
fn ime_capture_gate_focus_and_unmount_cancel_without_retargeting_late_commit() {
    let mut f = Fixture::new(1.0);
    let p = f.point(0, 0, 0);
    f.pointer(p, PointerPhase::Down);
    f.reduce();
    assert!(f.rt.input().captor(71).is_some());
    f.rt.dispatch(UiEvent::ImePreedit(ImePreedit::new(
        "compose",
        Some((7, 7)),
    )));
    assert!(f.terminal(0).lock().unwrap().preedit().is_some());
    f.rt.request_focus(&f.ui.pane_focus(f.panes[1]));
    f.reduce();
    assert!(f.rt.input().captor(71).is_none());
    assert!(f.terminal(0).lock().unwrap().preedit().is_none());
    f.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("late".into())));
    assert!(f.bytes.iter().all(|b| b.lock().unwrap().is_empty()));
    let p = f.point(1, 0, 0);
    f.pointer(p, PointerPhase::Down);
    f.reduce();
    f.rt.dispatch(UiEvent::ImePreedit(ImePreedit::new("pending", None)));
    f.gate.store(true, std::sync::atomic::Ordering::Release);
    f.rt.cancel_active_input_ownership(Point::ZERO);
    f.rt.dispatch(UiEvent::Focus(
        harbor_widget::input::event::FocusEvent::Lost,
    ));
    assert!(f.rt.input().captor(71).is_none());
    assert!(
        f.terminal(1).lock().unwrap().preedit().is_none(),
        "gate cannot suppress lifecycle cancellation"
    );
    f.rt.set_root(harbor_widget::SizedBox::width(1.0).unwrap());
    assert!(f.terminal(1).lock().unwrap().preedit().is_none());
    assert!(f.bytes.iter().all(|b| b.lock().unwrap().is_empty()));
}

#[test]
fn pane_close_survivor_and_whole_tab_final_window_semantics() {
    let mut f = Fixture::new(1.0);
    let survivor = f.terminal(0);
    let identity = survivor.lock().unwrap().content_version().session;
    f.tabs.focus_pane(f.panes[1]);
    f.ui.sync_panes(&f.tabs, 1200.0);
    f.rt.request_focus(&f.ui.terminal_focus());
    f.geometry();
    f.chord('w', false);
    assert_eq!(f.reduce(), [AppCommand::Pane(PaneCommand::Close)]);
    assert_eq!(f.tabs.active_binding(), Some(f.panes[0]));
    assert_eq!(survivor.lock().unwrap().content_version().session, identity);
    assert!(!survivor.lock().unwrap().is_session_closed());
    f.chord('w', true);
    let requests = f.ui.drain_actions();
    assert_eq!(
        requests[0].command,
        AppCommand::Tab(harbor_app::tab_view::TabCommand::CloseActive)
    );
    assert!(f.tabs.close_active().close_window);
    assert!(f.tabs.snapshots().is_empty());
    assert!(f.bytes.iter().all(|b| b.lock().unwrap().is_empty()));
    assert_eq!(f.readers.len(), 3);
}

#[test]
fn selection_copy_scroll_and_scrollbar_stay_with_hit_pane() {
    let mut f = Fixture::new(1.5);
    for index in 0..3 {
        f.terminal(index)
            .lock()
            .unwrap()
            .process_output(b"\x1b[HABCDEFGHIJ");
    }
    let start = f.point(2, 0, 0);
    let end = f.point(2, 3, 0);
    f.pointer(start, PointerPhase::Down);
    f.reduce();
    f.pointer(end, PointerPhase::Move);
    f.pointer(end, PointerPhase::Up);
    f.reduce();
    assert_eq!(
        f.tabs
            .dispatch_terminal_command(AppCommand::Copy)
            .unwrap()
            .clipboard_text
            .as_deref(),
        Some("ABCD")
    );
    assert!(!f.terminal(0).lock().unwrap().has_non_empty_selection());
    assert!(!f.terminal(1).lock().unwrap().has_non_empty_selection());
    for index in 0..3 {
        for _ in 0..90 {
            f.terminal(index)
                .lock()
                .unwrap()
                .process_output(b"line\r\n");
        }
    }
    let p = f.point(1, 1, 1);
    f.pointer(p, PointerPhase::WheelLine { dx: 0.0, dy: 2.0 });
    assert!(f.terminal(1).lock().unwrap().screen().view_offset() > 0);
    assert_eq!(f.terminal(0).lock().unwrap().screen().view_offset(), 0);
    assert_eq!(f.terminal(2).lock().unwrap().screen().view_offset(), 0);
    let rect =
        f.ui.pane_allocations()
            .into_iter()
            .find(|a| a.binding == f.panes[0])
            .unwrap()
            .rect;
    let track = Point::new(rect.max.x - 3.0 / f.scale, rect.min.y + 40.0 / f.scale);
    f.pointer(track, PointerPhase::Down);
    f.pointer(track, PointerPhase::Up);
    f.reduce();
    assert!(f.terminal(0).lock().unwrap().screen().view_offset() > 0);
    assert_eq!(f.terminal(2).lock().unwrap().screen().view_offset(), 0);
    assert!(f.bytes.iter().all(|bytes| bytes.lock().unwrap().is_empty()));
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn unpublished_authentication_routes_to_reserved_session_across_source_changes() {
    let mut f = Fixture::new(1.0);
    let (target, draw) = f.tabs.reserve_split_ids(f.panes[0]).unwrap();
    let data = Arc::new(Mutex::new(Vec::new()));
    let (_tx, rx) = mpsc::channel();
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        12,
        30,
        Reader(rx),
        Writer(data.clone()),
        || true,
    )));
    let bridge = TerminalWidgetBridge::new(draw, terminal.clone(), f.gate.clone());
    let resources = TerminalTabResources::new(terminal.clone(), bridge.clone(), "unpublished");
    let focus = f.ui.show_split_authentication(target, bridge);
    f.rt.request_focus(&focus);
    f.geometry();
    // The source can cease to be the active workspace; no active-source lookup
    // participates in authentication input.
    f.tabs
        .create_tab(|_, draw| {
            let terminal = Arc::new(Mutex::new(Terminal::new_headless(12, 30)));
            Ok(TerminalTabResources::new(
                terminal.clone(),
                TerminalWidgetBridge::new(draw, terminal, f.gate.clone()),
                "other",
            ))
        })
        .unwrap();
    f.ui.sync_panes(&f.tabs, 1200.0);
    f.rt.request_focus(&focus);
    f.geometry();
    f.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(
        "RESERVED_ONLY".into(),
    )));
    assert_eq!(*data.lock().unwrap(), b"RESERVED_ONLY");
    assert!(f.bytes.iter().all(|bytes| bytes.lock().unwrap().is_empty()));
    assert!(f.ui.drain_actions().is_empty());
    f.rt.dispatch(UiEvent::ImePreedit(ImePreedit::new("pending", None)));
    assert!(terminal.lock().unwrap().preedit().is_some());
    f.ui.close_split_authentication(target);
    drop(resources);
    f.rt.request_focus(&f.ui.terminal_focus());
    f.geometry();
    f.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("late".into())));
    assert!(terminal.lock().unwrap().is_session_closed());
    assert!(terminal.lock().unwrap().preedit().is_none());
    assert_eq!(*data.lock().unwrap(), b"RESERVED_ONLY");
}

#[test]
fn native_shifted_pane_cycle_is_consumed_once() {
    use harbor_widget::winit::WinitAdapter;
    use winit::{
        event::{ElementState, WindowEvent},
        keyboard::{Key as NativeKey, KeyLocation, ModifiersState},
    };
    let mut f = Fixture::new(1.0);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(
        &mut f.rt,
        &WindowEvent::ModifiersChanged((ModifiersState::CONTROL | ModifiersState::SHIFT).into()),
    );
    for (logical, command, index) in [("{", PaneCommand::Previous, 2), ("}", PaneCommand::Next, 0)]
    {
        for state in [ElementState::Pressed, ElementState::Released] {
            adapter.handle_keyboard_input(
                &mut f.rt,
                &NativeKey::Character(logical.into()),
                state,
                KeyLocation::Standard,
            );
        }
        assert_eq!(f.reduce(), [AppCommand::Pane(command)]);
        assert_eq!(f.tabs.active_binding(), Some(f.panes[index]));
        assert!(f.ui.drain_actions().is_empty());
        assert!(f.bytes.iter().all(|bytes| bytes.lock().unwrap().is_empty()));
    }
    // Defaults use shifted logical symbols, without changing explicit key semantics.
    let settings = harbor_config::RawKeybindings {
        entries: [(
            "app.next-pane".into(),
            vec![harbor_config::RawKeybinding {
                modifiers: vec!["ctrl".into(), "shift".into()],
                key: "]".into(),
            }],
        )]
        .into(),
        valid: true,
    };
    f.ui.publish_keybindings(harbor_app::command::validate_keybindings(&settings).unwrap());
    f.rt.update(Instant::now());
    for logical in ["}", "]"] {
        for state in [ElementState::Pressed, ElementState::Released] {
            adapter.handle_keyboard_input(
                &mut f.rt,
                &NativeKey::Character(logical.into()),
                state,
                KeyLocation::Standard,
            );
        }
        let actions = f.ui.drain_actions();
        if logical == "}" {
            assert!(actions.is_empty());
        } else {
            assert_eq!(
                actions,
                [AppCommandRequest::shortcut(AppCommand::Pane(
                    PaneCommand::Next
                ))]
            );
        }
    }
    f.clear_bytes();
    adapter.handle_event(
        &mut f.rt,
        &WindowEvent::ModifiersChanged(ModifiersState::SHIFT.into()),
    );
    for state in [ElementState::Pressed, ElementState::Released] {
        adapter.handle_keyboard_input(
            &mut f.rt,
            &NativeKey::Character("{".into()),
            state,
            KeyLocation::Standard,
        );
    }
    assert_eq!(*f.bytes[0].lock().unwrap(), b"{");
    assert!(f.ui.drain_actions().is_empty());
}

#[test]
fn unfocused_pane_exit_preserves_actual_runtime_focus_without_request() {
    for focus in ["terminal-handle", "terminal-click", "rail", "editor"] {
        let mut f = Fixture::new(1.0);
        match focus {
            "terminal-click" => {
                let p = f.point(0, 0, 0);
                f.pointer(p, PointerPhase::Down);
                f.reduce();
                f.pointer(p, PointerPhase::Up);
                f.reduce();
            }
            "rail" => {
                f.rt.request_focus(&f.ui.tabs().tab_focus(f.panes[2].workspace).unwrap());
            }
            "editor" => {
                f.ui.open_search(f.panes[2].session);
                f.rt.request_focus(&f.ui.search_focus());
            }
            _ => {}
        }
        f.geometry();
        f.ui.drain_actions();
        let before = f.rt.input().focused();
        let outcome = f.tabs.close_pane(f.panes[1]);
        assert!(!outcome.active_bridge_changed);
        // Production PreserveRail path: publish the new projection, reconcile, no focus request.
        f.ui.sync_panes(&f.tabs, 1200.0);
        f.geometry();
        if focus.starts_with("terminal") {
            for ch in "SURVIVOR".chars() {
                f.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                    key: Key::Character(ch),
                    modifiers: Modifiers::default(),
                }));
                f.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
                    key: Key::Character(ch),
                    modifiers: Modifiers::default(),
                }));
            }
        } else {
            f.rt.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("SURVIVOR".into())));
        }
        if focus.starts_with("terminal") {
            assert_eq!(*f.bytes[0].lock().unwrap(), b"SURVIVOR");
        } else {
            assert_eq!(f.rt.input().focused(), before, "{focus} focus moved");
            assert!(
                f.bytes.iter().all(|bytes| bytes.lock().unwrap().is_empty()),
                "{focus} focus stolen"
            );
            if focus == "editor" {
                assert_eq!(f.ui.search_query().unwrap().1, "SURVIVOR");
            }
        }
    }
}
