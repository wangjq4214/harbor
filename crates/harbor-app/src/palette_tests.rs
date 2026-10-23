//! Headless evidence for the T0001 increment, not native Windows acceptance.
use super::*;
use crate::{
    command::{CommandSource, command_availability, validate_keybindings},
    tab_manager::{TabManager, TerminalTabResources},
};
use harbor_widget::{
    TextEditingController,
    effects::ClipboardEffect,
    input::event::{FocusEvent, ImePreedit},
};

type Capture = Arc<Mutex<Vec<u8>>>;

#[allow(clippy::arc_with_non_send_sync)]
fn resources(draw: ExternalDrawId, bytes: &Capture) -> TerminalTabResources {
    let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        std::io::empty(),
        Writer(bytes.clone()),
        || true,
    )));
    let bridge = TerminalWidgetBridge::new(draw, engine.clone(), Arc::new(AtomicBool::new(false)));
    TerminalTabResources::new(engine, bridge, "test")
}

fn fixture() -> (TabManager, WorkspaceUiController, Runtime, Capture) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap();
    let controller =
        WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    (tabs, controller, runtime, bytes)
}

fn down(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
}
fn up(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
}
fn tap(runtime: &mut Runtime, key: Key) {
    down(runtime, key, Modifiers::default());
    up(runtime, key, Modifiers::default());
}
fn open(controller: &WorkspaceUiController, runtime: &mut Runtime) {
    let mods = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::default()
    };
    down(runtime, Key::Character('P'), mods);
    let actions = controller.drain_actions();
    assert_eq!(
        actions,
        [AppCommandRequest::shortcut(AppCommand::OpenCommandPalette)]
    );
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    let effects = runtime.update(Instant::now());
    assert_eq!(
        effects.ime.expect("query IME destination").allowed,
        Some(true)
    );
    up(runtime, Key::Character('P'), mods);
    assert!(runtime.input().focused().is_some());
}
fn input(controller: &WorkspaceUiController) -> TextEditingController {
    controller
        .store
        .state()
        .read()
        .popups
        .get::<palette::PaletteState>()
        .unwrap()
        .input
        .clone()
}
fn replace(runtime: &mut Runtime, text: &str) {
    down(runtime, Key::Character('a'), ctrl());
    up(runtime, Key::Character('a'), ctrl());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(text.into())));
    runtime.update(Instant::now());
}
fn painted(runtime: &Runtime) -> String {
    let delta = runtime.pending_delta().unwrap();
    // Added and modified items are separate lists, not a single paint-order stream.
    let mut items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    items.sort_by_key(|item| item.paint_order);
    items
        .iter()
        .filter_map(|item| match &item.primitive {
            Primitive::Text { text, .. } => Some(text.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn take_one(controller: &WorkspaceUiController) -> AppCommandRequest {
    let actions = controller.drain_actions();
    assert_eq!(actions.len(), 1, "exactly one typed request");
    actions[0]
}

fn palette_geometry(runtime: &Runtime) -> (Rect, Rect) {
    let delta = runtime.pending_delta().unwrap();
    let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    let panel = items
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::BackdropMaterial { rect, .. } => Some(rect),
            _ => None,
        })
        .expect("palette panel");
    let selected_row = items
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::RoundedQuad {
                rect,
                color,
                corner_radii,
            } if corner_radii == [12.0; 4] && color.a > 0.0 => Some(rect),
            _ => None,
        })
        .expect("selected command row");
    (panel, selected_row)
}
#[test]
fn palette_build_queries_context_once_for_visible_rows_and_never_for_no_matches() {
    let (tabs, controller, mut runtime, _) = fixture();
    let bridge = tabs.active_bridge().unwrap();
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    let before = bridge.alt_screen_queries();
    runtime.update(Instant::now());
    assert!(
        crate::command::command_registry()
            .iter()
            .filter(|entry| entry.palette_visible)
            .count()
            > 5
    );
    assert_eq!(
        bridge.alt_screen_queries() - before,
        1,
        "one context snapshot for all result rows"
    );

    let before = bridge.alt_screen_queries();
    assert!(input(&controller).set_text("definitely-not-a-command"));
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("No matching commands"));
    assert_eq!(
        bridge.alt_screen_queries(),
        before,
        "empty results do not drain the PTY"
    );
}

fn assert_palette_title_centered(runtime: &Runtime, row: Rect, title: &str, lines: usize) {
    let delta = runtime.pending_delta().unwrap();
    let origin = delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match &item.primitive {
            Primitive::Text { text, origin, .. }
                if text.as_ref() == title
                    || text
                        .strip_suffix('…')
                        .is_some_and(|prefix| !prefix.is_empty() && title.starts_with(prefix)) =>
            {
                Some(*origin)
            }
            _ => None,
        })
        .expect("command title is painted");
    let text_height = runtime.text_metrics().line_height * lines as f32;
    assert_eq!(
        origin.y,
        row.min.y + (row.size().height - text_height) / 2.0,
        "center the actual title/reason block, not a title plus an empty line: {title}",
    );
}

#[test]
fn palette_row_height_is_uniform_across_navigation_and_filtered_result_counts() {
    for (width, height, scale, line_height) in [
        (1000, 900, 1.0, 20.0),
        (1500, 1350, 1.5, 20.0),
        (320, 1200, 1.0, 28.0),
    ] {
        for multiple_bindings in [false, true] {
            let (_, controller, mut runtime, bytes) = fixture();
            runtime.set_viewport(Viewport::new(width, height, scale));
            runtime.set_text_metrics(harbor_widget::text::TextMetrics {
                line_height,
                ..*runtime.text_metrics()
            });
            if multiple_bindings {
                let mut raw = harbor_config::RawKeybindings::default();
                raw.entries.insert(
                    "app.new-tab".into(),
                    ["ctrl", "alt", "shift"]
                        .into_iter()
                        .map(|modifier| harbor_config::RawKeybinding {
                            modifiers: vec![modifier.into()],
                            key: "n".into(),
                        })
                        .collect(),
                );
                raw.entries.insert("terminal.copy".into(), vec![]);
                controller.publish_keybindings(validate_keybindings(&raw).unwrap());
            }
            runtime.update(Instant::now());
            open(&controller, &mut runtime);
            let (full_panel, first_row) = palette_geometry(&runtime);
            let row_height = first_row.size().height;
            assert_palette_title_centered(&runtime, first_row, "Reload configuration", 1);
            let count = crate::command::command_registry()
                .iter()
                .filter(|entry| entry.palette_visible)
                .count();
            for index in 1..=count {
                tap(&mut runtime, Key::ArrowDown);
                runtime.update(Instant::now());
                let (panel, row) = palette_geometry(&runtime);
                assert_eq!(
                    panel, full_panel,
                    "the results viewport must not resize during navigation"
                );
                assert_eq!(
                    row.size().height,
                    row_height,
                    "all command rows have one height"
                );
                assert_eq!(
                    row.min.y,
                    first_row.min.y + (index % count).min(4) as f32 * (row_height + 4.0),
                    "selection advances in equal row steps, then keeps five rows visible",
                );
            }
            for (query, visible, title, lines) in [
                ("terminal.search", 4, "Search current session", 1),
                ("terminal.page-", 2, "Page up", 1),
                ("app.new-tab", 1, "New tab", 1),
                ("terminal.search-close", 1, "Close search", 2),
                ("terminal.copy", 1, "Copy", 1),
                ("", 5, "Reload configuration", 1),
            ] {
                replace(&mut runtime, query);
                let (panel, row) = palette_geometry(&runtime);
                assert_eq!(row.size().height, row_height, "query: {query}");
                assert_palette_title_centered(&runtime, row, title, lines);
                assert_eq!(
                    panel.size().height,
                    full_panel.size().height - (5 - visible) as f32 * (row_height + 4.0),
                    "fewer matches use their actual row count without empty slots: {query}",
                );
            }
            assert!(bytes.lock().unwrap().is_empty());
        }
    }
}

#[test]
fn palette_rows_reserve_a_gutter_with_right_aligned_keycaps_and_translucent_selection() {
    for (width, height, scale) in [(1000, 700, 1.0), (1500, 1000, 1.5), (320, 700, 1.0)] {
        let (_, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        let delta = runtime.pending_delta().unwrap();
        let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
        let panel = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::BackdropMaterial { rect, .. } => Some(rect),
                _ => None,
            })
            .unwrap();
        let selection = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::RoundedQuad {
                    rect,
                    color,
                    corner_radii,
                } if corner_radii == [12.0; 4] && color.a > 0.0 => {
                    assert!(color.a <= 0.15, "selection must reveal the glass below it");
                    Some(rect)
                }
                _ => None,
            })
            .expect("selected command has a translucent highlight");
        assert!(
            items.iter().any(|item| matches!(item.primitive,
            Primitive::BackdropMaterial { rect, corner_radii, .. }
                if rect == selection && corner_radii == [12.0; 4])),
            "selection uses real rounded liquid glass, not only a translucent fill"
        );
        assert_eq!(selection.min.x, panel.min.x + 8.0);
        assert_eq!(selection.max.x, panel.max.x - 8.0 - 14.0);

        let caps: Vec<_> = items
            .iter()
            .filter(|item| {
                let Primitive::RoundedBorder { rect, .. } = item.primitive else {
                    return false;
                };
                item.clips.iter().all(|clip| {
                    let clip = clip.rect();
                    rect.min.y >= clip.min.y && rect.max.y <= clip.max.y
                })
            })
            .filter_map(|item| match item.primitive {
                Primitive::RoundedBorder {
                    rect,
                    width: 1.0,
                    corner_radii,
                    ..
                } if corner_radii == [4.0; 4] => Some(rect),
                _ => None,
            })
            .collect();
        assert_eq!(
            caps.len(),
            12,
            "each key in the five default shortcuts has a frame"
        );
        let mut rows: Vec<f32> = caps.iter().map(|rect| rect.min.y).collect();
        rows.sort_by(f32::total_cmp);
        rows.dedup();
        assert_eq!(rows.len(), 5);
        for y in rows {
            let right = caps
                .iter()
                .filter(|rect| rect.min.y == y)
                .map(|rect| rect.max.x)
                .fold(0.0, f32::max);
            assert_eq!(
                right,
                selection.max.x - 8.0,
                "shortcut groups share the right inset"
            );
        }
        for cap in &caps {
            assert!(cap.min.x >= selection.min.x + 8.0);
            let text = items
                .iter()
                .find_map(|item| match &item.primitive {
                    Primitive::Text { origin, text, .. }
                        if *origin == Point::new(cap.min.x + 2.0, cap.min.y + 1.0) =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .expect("keycap text has only a small inset");
            let metrics = runtime.text_metrics();
            assert_eq!(cap.size().height, metrics.line_height + 2.0);
            assert_eq!(
                cap.size().width,
                text.len() as f32 * metrics.cell_width + 4.0,
                "keycap wraps the label's natural width plus two pixels per side",
            );
        }
        let text = painted(&runtime);
        assert!(text.contains("Ctrl\nShift\nR"));
        assert!(!text.contains('+'));
        assert!(!text.contains("app."));
        assert!(!text.contains("terminal."));
        assert!(
            text.contains("No other tab"),
            "disabled reason remains visible"
        );
        assert!(bytes.lock().unwrap().is_empty());
    }
}

fn row_surface(runtime: &Runtime, alpha: f32) -> Option<Rect> {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::RoundedQuad {
                rect,
                color,
                corner_radii,
            } if corner_radii == [12.0; 4] && color.a == alpha => Some(rect),
            _ => None,
        })
}

fn row_pointer(runtime: &mut Runtime, position: Point, phase: PointerPhase) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        position,
        phase,
        PointerButton::Left,
        42,
    )));
    runtime.update(Instant::now());
}

#[test]
fn palette_hover_is_lighter_transient_glass_and_preserves_keyboard_selection_and_focus() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let focus = runtime.input().focused();
    let (panel, selected) = palette_geometry(&runtime);
    let second = Point::new(
        selected.max.x - 2.0,
        selected.max.y + 4.0 + selected.size().height / 2.0,
    );
    row_pointer(&mut runtime, second, PointerPhase::Move);
    let hovered = row_surface(&runtime, 0.06).expect("hover is lighter than selection");
    assert!(hovered.contains(second));
    assert_eq!(row_surface(&runtime, 0.10), Some(selected));
    let delta = runtime.pending_delta().unwrap();
    assert!(
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .any(|item| matches!(
                item.primitive, Primitive::BackdropMaterial { rect, corner_radii, .. }
                    if rect == hovered && corner_radii == [12.0; 4]
            ))
    );
    assert_eq!(runtime.input().focused(), focus);
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .popups
            .get::<palette::PaletteState>()
            .unwrap()
            .selected,
        0
    );
    assert!(
        controller.drain_actions().is_empty(),
        "hover never executes"
    );
    row_pointer(
        &mut runtime,
        Point::new(panel.min.x + 1.0, panel.min.y + 1.0),
        PointerPhase::Move,
    );
    assert!(
        row_surface(&runtime, 0.06).is_none(),
        "hover disappears on leave"
    );
    assert_eq!(row_surface(&runtime, 0.10), Some(selected));
    tap(&mut runtime, Key::ArrowDown);
    runtime.update(Instant::now());
    assert_eq!(runtime.input().focused(), focus);
    tap(&mut runtime, Key::Enter);
    assert_eq!(
        take_one(&controller).command,
        AppCommand::Tab(TabCommand::New)
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_single_click_runs_the_clicked_command_once_instead_of_the_keyboard_selection() {
    // Labels, keycaps and row padding are all part of the same click target.
    for fraction in [0.0, 0.5, 1.0] {
        let (mut tabs, controller, mut runtime, bytes) = fixture();
        open(&controller, &mut runtime);
        let focus = runtime.input().focused();
        let (_, first) = palette_geometry(&runtime);
        let point = Point::new(
            first.min.x + 2.0 + (first.size().width - 4.0) * fraction,
            first.max.y + 4.0 + first.size().height / 2.0,
        );
        row_pointer(&mut runtime, point, PointerPhase::Move);
        row_pointer(&mut runtime, point, PointerPhase::Down);
        assert!(
            controller.drain_actions().is_empty(),
            "execute on release, not press"
        );
        assert_eq!(runtime.input().focused(), focus);
        row_pointer(&mut runtime, point, PointerPhase::Up);
        // Repeated clicks and Enter before the Host drains cannot enqueue duplicates.
        row_pointer(&mut runtime, point, PointerPhase::Down);
        row_pointer(&mut runtime, point, PointerPhase::Up);
        tap(&mut runtime, Key::Enter);
        let request = take_one(&controller);
        assert_eq!(request.command, AppCommand::Tab(TabCommand::New));
        assert!(matches!(request.source, CommandSource::Palette { .. }));
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap();
        let mut executions = 0;
        tabs.execute_command(TabCommand::New, |_, draw| {
            executions += 1;
            Ok(resources(draw, &bytes))
        })
        .unwrap();
        assert_eq!(executions, 1);
        assert_eq!(tabs.snapshots().len(), 2);
        assert!(!controller.palette_is_open());
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn palette_cancelled_click_and_disabled_row_leave_the_editor_usable_without_executing() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let focus = runtime.input().focused();
    let (panel, first) = palette_geometry(&runtime);
    let point = Point::new(
        first.min.x + 30.0,
        first.max.y + 4.0 + first.size().height / 2.0,
    );
    let outside = Point::new(panel.min.x - 2.0, panel.min.y);
    row_pointer(&mut runtime, point, PointerPhase::Down);
    row_pointer(&mut runtime, outside, PointerPhase::Move);
    row_pointer(&mut runtime, outside, PointerPhase::Up);
    assert!(runtime.input().captor(42).is_none());
    assert_eq!(runtime.input().focused(), focus);
    assert!(controller.drain_actions().is_empty());
    replace(&mut runtime, "app.next-tab");
    assert!(painted(&runtime).contains("No other tab"));
    let (_, disabled) = palette_geometry(&runtime);
    let point = Point::new(disabled.min.x + 30.0, disabled.min.y + 10.0);
    row_pointer(&mut runtime, point, PointerPhase::Move);
    row_pointer(&mut runtime, point, PointerPhase::Down);
    row_pointer(&mut runtime, point, PointerPhase::Up);
    assert_eq!(runtime.input().focused(), focus);
    assert!(controller.drain_actions().is_empty());
    replace(&mut runtime, "app.new-tab");
    tap(&mut runtime, Key::Enter);
    assert_eq!(
        take_one(&controller).command,
        AppCommand::Tab(TabCommand::New)
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_filtering_during_a_press_never_executes_a_replacement_row() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let (_, first) = palette_geometry(&runtime);
    let point = Point::new(
        first.min.x + 30.0,
        first.max.y + 4.0 + first.size().height / 2.0,
    );
    row_pointer(&mut runtime, point, PointerPhase::Down);
    replace(&mut runtime, "terminal.copy");
    row_pointer(&mut runtime, point, PointerPhase::Up);
    assert!(controller.drain_actions().is_empty());
    assert!(runtime.input().captor(42).is_none());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_uses_shared_local_blur_with_a_distinct_inset_editor() {
    for (width, height, scale) in [(1000, 700, 1.0), (1500, 1000, 1.5), (320, 700, 1.0)] {
        let (_tabs, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        let allocation = controller.latest_terminal_allocation();
        open(&controller, &mut runtime);
        assert_eq!(controller.latest_terminal_allocation(), allocation);

        let delta = runtime.pending_delta().unwrap();
        let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
        let shared = GlassAppearance::default();
        let palette = GlassAppearance::command_palette();
        let panel = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::BackdropMaterial {
                    rect,
                    sigma,
                    refraction,
                    material,
                    corner_radii,
                } => {
                    assert_eq!(Some(material), shared.material);
                    assert_eq!(sigma, shared.blur_sigma);
                    assert_eq!(refraction, shared.refraction);
                    assert_eq!(corner_radii, [12.0; 4]);
                    Some(rect)
                }
                _ => None,
            })
            .expect("palette must filter the real backdrop, not just paint translucency");
        assert_eq!(
            panel.size().width,
            620.0_f32.min(width as f32 / scale - 24.0)
        );
        assert!(items.iter().any(|item| matches!(item.primitive,
            Primitive::PaintedQuad { rect, paint, .. }
                if rect == panel && paint == shared.panel_fill)));
        let editor = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::RoundedQuad {
                    rect,
                    color,
                    corner_radii,
                } if corner_radii == [7.0; 4] && color == palette.input_fill => Some(rect),
                _ => None,
            })
            .expect("editor has its own inset surface");
        assert!(palette.input_fill.a > shared.input_fill.a);
        assert!(palette.input_fill.a < 1.0, "editor remains translucent");
        assert_eq!(editor.min, Point::new(panel.min.x + 8.0, panel.min.y + 8.0));
        assert!(editor.max.x <= panel.max.x - 8.0);
        assert!(editor.size().width > 0.0);
        assert!(items.iter().any(|item| matches!(item.primitive,
            Primitive::RoundedBorder { rect, color, width: 1.0, corner_radii }
                if rect == editor && color == palette.input_border.unwrap().color()
                    && corner_radii == [7.0; 4])));
        assert!(painted(&runtime).contains("Type a command label or ID"));
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn palette_filters_navigates_and_executes_one_real_tab_reduction_without_pty_input() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    assert!(painted(&runtime).contains("Reload configuration"));
    assert!(painted(&runtime).contains("No other tab"));
    tap(&mut runtime, Key::ArrowDown);
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .popups
            .get::<palette::PaletteState>()
            .unwrap()
            .selected,
        1
    );
    tap(&mut runtime, Key::ArrowUp);
    replace(&mut runtime, "NeW TaB");
    assert!(painted(&runtime).contains("New tab"));
    replace(&mut runtime, "APP.NEW-TAB");
    down(&mut runtime, Key::Enter, Modifiers::default());
    down(&mut runtime, Key::Enter, Modifiers::default()); // repeat while Host has not drained
    let request = take_one(&controller);
    assert!(matches!(request.source, CommandSource::Palette { .. }));
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    let AppCommand::Tab(command) = request.command else {
        panic!("tab request")
    };
    let mut executions = 0;
    let resulting_bytes = Arc::new(Mutex::new(Vec::new()));
    let outcome = tabs
        .execute_command(command, |_, draw| {
            executions += 1;
            Ok(resources(draw, &resulting_bytes))
        })
        .unwrap();
    assert_eq!(executions, 1);
    assert!(outcome.active_bridge_changed);
    assert_eq!(tabs.snapshots().len(), 2);
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .is_err()
    );
    controller
        .tabs()
        .sync(tabs.snapshots(), tabs.active_bridge(), 1000.0);
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    down(&mut runtime, Key::Enter, Modifiers::default()); // held invocation after resulting-tab focus
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert!(controller.drain_actions().is_empty());
    assert!(
        bytes.lock().unwrap().is_empty(),
        "opener, filter, navigation, Enter and releases are consumed"
    );
    assert!(
        resulting_bytes.lock().unwrap().is_empty(),
        "invocation repeats/releases do not reach the new terminal"
    );
    tap(&mut runtime, Key::Character('x'));
    assert!(
        bytes.lock().unwrap().is_empty(),
        "old terminal never receives post-command input"
    );
    assert_eq!(
        &*resulting_bytes.lock().unwrap(),
        b"x",
        "new terminal receives resumed input only"
    );
}

#[test]
fn palette_empty_results_disabled_rows_and_filter_changes_cannot_execute_phantom_selection() {
    let (tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    tap(&mut runtime, Key::ArrowUp); // selects last numeric row
    replace(&mut runtime, "does not match");
    assert!(painted(&runtime).contains("No matching commands"));
    for key in [Key::ArrowUp, Key::ArrowDown, Key::Enter] {
        tap(&mut runtime, key);
    }
    assert!(controller.drain_actions().is_empty());
    replace(&mut runtime, "app.select-tab-9");
    assert!(painted(&runtime).contains("Tab does not exist"));
    tap(&mut runtime, Key::Enter);
    assert!(controller.drain_actions().is_empty());
    assert_eq!(tabs.snapshots().len(), 1);
    assert!(
        controller
            .store
            .state()
            .read()
            .notices
            .message()
            .unwrap()
            .contains("Rejected")
    );
    replace(&mut runtime, "app.new-tab");
    tap(&mut runtime, Key::Enter);
    assert_eq!(
        take_one(&controller).command,
        AppCommand::Tab(TabCommand::New)
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_binding_overrides_multiple_unbinding_and_rejected_reload_display() {
    let (_, controller, mut runtime, _) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "app.new-tab");
    assert!(painted(&runtime).contains("Ctrl\nT"));
    let mut raw = harbor_config::RawKeybindings::default();
    raw.entries.insert(
        "app.new-tab".into(),
        vec![
            harbor_config::RawKeybinding {
                modifiers: vec!["ctrl".into()],
                key: "n".into(),
            },
            harbor_config::RawKeybinding {
                modifiers: vec!["alt".into()],
                key: "n".into(),
            },
        ],
    );
    controller.publish_keybindings(validate_keybindings(&raw).unwrap());
    runtime.update(Instant::now());
    let text = painted(&runtime);
    assert!(text.contains("Ctrl\nN\nAlt\nN"), "{text:?}");
    assert!(!text.contains("Ctrl\nT"));
    raw.entries.insert("app.new-tab".into(), vec![]);
    controller.publish_keybindings(validate_keybindings(&raw).unwrap());
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("Unbound"));
    let accepted = controller.store.state().read().keybindings.clone();
    raw.entries.insert("unknown".into(), vec![]);
    assert!(validate_keybindings(&raw).is_err());
    // Failed candidates never reach Host publication.
    assert_eq!(controller.store.state().read().keybindings, accepted);
    controller.report_palette_result(
        AppCommand::ReloadConfiguration,
        Err("Failed: invalid candidate"),
    );
    assert_eq!(controller.store.state().read().keybindings, accepted);
}

#[test]
fn palette_ime_preedit_confirmation_escape_and_clipboard_are_editor_owned() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "app.new-tab");
    let editor = input(&controller);
    let initial_selected = controller
        .store
        .state()
        .read()
        .popups
        .get::<palette::PaletteState>()
        .unwrap()
        .selected;
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
    tap(&mut runtime, Key::ArrowDown);
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .popups
            .get::<palette::PaletteState>()
            .unwrap()
            .selected,
        initial_selected
    );
    assert_eq!(editor.text(), "app.new-tab");
    down(&mut runtime, Key::Enter, Modifiers::default());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    down(&mut runtime, Key::Enter, Modifiers::default());
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert_eq!(editor.text(), "app.new-tab界");
    assert!(
        controller.drain_actions().is_empty(),
        "IME confirmation never executes a candidate"
    );
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("文", None)));
    tap(&mut runtime, Key::Escape);
    assert!(editor.value().composition.is_none());
    assert!(controller.drain_actions().is_empty());
    assert!(controller.palette_is_open());
    down(&mut runtime, Key::Character('a'), ctrl());
    let copied = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('c'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(copied)) = copied.clipboard else {
        panic!("editor copy")
    };
    runtime.complete_clipboard(copied.id, Ok(String::new()));
    let cut = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('x'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(cut)) = cut.clipboard else {
        panic!("editor cut")
    };
    runtime.complete_clipboard(cut.id, Ok(String::new()));
    assert_eq!(editor.text(), "");
    let pasted = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(pasted)) = pasted.clipboard else {
        panic!("editor paste")
    };
    runtime.complete_clipboard(pasted.id, Ok("New tab".into()));
    assert_eq!(editor.text(), "New tab");
    down(&mut runtime, Key::Character('z'), ctrl());
    assert_eq!(editor.text(), "");
    assert!(
        controller.drain_actions().is_empty(),
        "query clipboard never queues terminal actions"
    );
    tap(&mut runtime, Key::Escape);
    let request = take_one(&controller);
    let AppCommand::CloseCommandPalette(generation) = request.command else {
        panic!("close")
    };
    assert!(controller.close_palette(generation));
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    up(&mut runtime, Key::Escape, Modifiers::default());
    assert!(bytes.lock().unwrap().is_empty());
    tap(&mut runtime, Key::Character('q'));
    assert_eq!(&*bytes.lock().unwrap(), b"q");
}

#[test]
fn palette_rechecks_host_context_disposes_stale_requests_and_preserves_new_editor() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "app.new-tab");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&controller);
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap(); // authoritative context changes before drain
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap_err()
            .contains("changed")
    );
    assert!(!controller.palette_is_open());
    controller
        .tabs()
        .sync(tabs.snapshots(), tabs.active_bridge(), 1000.0);
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    let new_editor = input(&controller);
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .is_err()
    );
    assert!(
        controller.palette_is_open(),
        "old requests cannot close the replacement editor"
    );
    assert_eq!(new_editor.text(), "");
    assert_eq!(tabs.snapshots().len(), 2);
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_close_focus_loss_and_root_replacement_reject_late_clipboard_and_ime() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    replace(&mut runtime, "query");
    let old = input(&controller);
    let pasted = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(pasted)) = pasted.clipboard else {
        panic!("paste")
    };
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
    runtime.dispatch(UiEvent::Focus(FocusEvent::Lost));
    runtime.complete_clipboard(pasted.id, Ok("late".into()));
    assert_eq!(old.text(), "query");
    assert!(old.value().composition.is_none());
    runtime.dispatch(UiEvent::Focus(FocusEvent::Gained));
    runtime.request_focus(&controller.open_palette());
    let late_close = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(late_close)) = late_close.clipboard else {
        panic!("pending close paste")
    };
    let generation = controller
        .store
        .state()
        .read()
        .popups
        .get::<palette::PaletteState>()
        .unwrap()
        .generation;
    controller.close_palette(generation);
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    runtime.complete_clipboard(late_close.id, Ok("late close".into()));
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("文", None)));
    let editor = input(&controller);
    let pending = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    // Composition consumes Ctrl+V rather than starting clipboard I/O.
    assert!(pending.clipboard.is_none());
    runtime.set_root(tab_workspace(controller.clone(), false));
    runtime.update(Instant::now());
    assert!(editor.value().composition.is_none());
    runtime.request_focus(&controller.open_palette());
    let late_root = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(late_root)) = late_root.clipboard else {
        panic!("pending replacement paste")
    };
    runtime.set_root(tab_workspace(controller.clone(), false));
    runtime.update(Instant::now());
    runtime.request_focus(&controller.open_palette());
    runtime.complete_clipboard(late_root.id, Ok("wrong lifetime".into()));
    assert_eq!(editor.text(), "");
    assert!(controller.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_tab_navigation_numeric_bounds_and_final_close_use_existing_model_semantics() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap();
    for query in [
        "app.previous-tab",
        "app.next-tab",
        "app.select-tab-1",
        "app.close-active-tab",
        "app.close-active-tab",
    ] {
        controller
            .tabs()
            .sync(tabs.snapshots(), tabs.active_bridge(), 1000.0);
        runtime.request_focus(&controller.terminal_focus());
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        replace(&mut runtime, query);
        tap(&mut runtime, Key::Enter);
        let request = take_one(&controller);
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap();
        let AppCommand::Tab(command) = request.command else {
            panic!("tab")
        };
        let outcome = tabs
            .execute_command(command, |_, _| panic!("no creation"))
            .unwrap();
        if tabs.snapshots().is_empty() {
            assert!(outcome.close_window);
            break;
        }
        assert!(!outcome.close_window);
        assert!(tabs.active_id().is_some());
    }
    assert!(tabs.snapshots().is_empty());
    assert!(
        command_availability(
            AppCommand::Tab(TabCommand::Numeric(TabIndex::from_valid_u8(9))),
            &tabs.snapshots(),
            crate::command::CommandContext::default(),
        )
        .is_err()
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_native_ime_confirmation_and_close_gestures_do_not_replay_into_terminal() {
    use harbor_widget::winit::WinitAdapter;
    use winit::{
        event::{ElementState, Ime, WindowEvent},
        keyboard::{Key as NativeKey, KeyLocation, NamedKey},
    };
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Preedit("New tab".into(), None)),
    );
    let enter = NativeKey::Named(NamedKey::Enter);
    adapter.handle_keyboard_input(
        &mut runtime,
        &enter,
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Commit("New tab".into())),
    );
    adapter.handle_keyboard_input_with_repeat(
        &mut runtime,
        &enter,
        ElementState::Pressed,
        KeyLocation::Standard,
        true,
    );
    adapter.handle_keyboard_input(
        &mut runtime,
        &enter,
        ElementState::Released,
        KeyLocation::Standard,
    );
    assert!(
        controller.drain_actions().is_empty(),
        "native IME confirmation repeat cannot execute a candidate"
    );
    assert_eq!(input(&controller).text(), "New tab");
    let escape = NativeKey::Named(NamedKey::Escape);
    adapter.handle_keyboard_input(
        &mut runtime,
        &escape,
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    let AppCommand::CloseCommandPalette(generation) = take_one(&controller).command else {
        panic!("close")
    };
    controller.close_palette(generation);
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    adapter.handle_keyboard_input_with_repeat(
        &mut runtime,
        &escape,
        ElementState::Pressed,
        KeyLocation::Standard,
        true,
    );
    adapter.handle_keyboard_input(
        &mut runtime,
        &escape,
        ElementState::Released,
        KeyLocation::Standard,
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "held close gesture never becomes terminal Escape"
    );
    adapter.handle_keyboard_input(
        &mut runtime,
        &NativeKey::Character("z".into()),
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    assert_eq!(&*bytes.lock().unwrap(), b"z");
}

#[path = "palette_terminal_tests.rs"]
mod terminal_actions;

#[path = "palette_render_tests.rs"]
mod rendered;

#[path = "popup_tests.rs"]
mod popups;

fn selected_result_viewport(runtime: &Runtime) -> (Rect, Rect) {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::RoundedQuad {
                rect,
                corner_radii: [12.0, 12.0, 12.0, 12.0],
                color,
            } if color.a > 0.0 => Some((
                rect,
                item.clips.last().expect("result viewport clip").rect(),
            )),
            _ => None,
        })
        .expect("selected result")
}

fn palette_thumb(runtime: &Runtime) -> Option<Rect> {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::Quad {
                rect,
                color,
                corner_radius: 3.0,
            } if color.a == 0.35 || color.a == 0.6 => Some(rect),
            _ => None,
        })
}

#[test]
fn palette_results_leave_a_stable_gutter_outside_row_surfaces() {
    for (width, height, scale) in [(1000, 500, 1.0), (1500, 750, 1.5), (320, 500, 1.0)] {
        let (_, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        let focus = runtime.input().focused();
        let (row, viewport) = selected_result_viewport(&runtime);
        let thumb = palette_thumb(&runtime).expect("overflow scrollbar");
        assert!(row.max.x < thumb.min.x, "idle thumb must not cover a row");

        row_pointer(
            &mut runtime,
            Point::new(viewport.max.x - 5.0, row.min.y + 10.0),
            PointerPhase::Move,
        );
        let delta = runtime.pending_delta().unwrap();
        let track = delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match item.primitive {
                Primitive::Quad {
                    rect,
                    color,
                    corner_radius: 3.0,
                } if color.a == 0.06 => Some(rect),
                _ => None,
            })
            .expect("hover expands the scrollbar track");
        assert!(
            row.max.x + 4.0 <= track.min.x,
            "the entire scrollbar hit target stays beyond the row background and its contents"
        );
        assert!(
            row_surface(&runtime, 0.06).is_none(),
            "gutter is not a row hover target"
        );
        assert_eq!(runtime.input().focused(), focus);

        replace(&mut runtime, "app.new-tab");
        let (filtered_row, filtered_viewport) = selected_result_viewport(&runtime);
        assert_eq!(filtered_row.size().width, row.size().width);
        assert_eq!(filtered_viewport.size().width, viewport.size().width);
        assert!(palette_thumb(&runtime).is_none());
        assert!(controller.drain_actions().is_empty());
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn palette_real_results_scroll_drag_and_keyboard_reveal_keep_query_focus_and_pty_quiet() {
    for height in [500, 180] {
        let (_, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(1000, height, 1.0));
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        let focus = runtime.input().focused();
        let (first, viewport) = selected_result_viewport(&runtime);
        let text = painted(&runtime);
        assert!(text.contains("Reload configuration"));
        assert!(
            text.contains("Previous search match"),
            "offscreen results exist, not a five-item slice"
        );
        assert!(palette_thumb(&runtime).is_some());
        assert!(first.min.y >= viewport.min.y && first.max.y <= viewport.max.y);

        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(viewport.min.x + 20.0, viewport.min.y + 10.0),
            PointerPhase::WheelPixel {
                dx: 0.0,
                dy: -100.0,
            },
            PointerButton::Left,
            10,
        )));
        runtime.update(Instant::now());
        let (scrolled, _) = selected_result_viewport(&runtime);
        assert_eq!(
            scrolled.min.y,
            first.min.y - 100.0,
            "continuous pixel scrolling"
        );
        assert_eq!(runtime.input().focused(), focus);
        let thumb = palette_thumb(&runtime).unwrap();
        for (phase, y) in [
            (PointerPhase::Down, thumb.min.y + 2.0),
            (PointerPhase::Move, viewport.max.y + 100.0),
            (PointerPhase::Up, viewport.max.y + 100.0),
        ] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(thumb.min.x + 2.0, y),
                phase,
                PointerButton::Left,
                11,
            )));
            runtime.update(Instant::now());
        }
        assert!(runtime.input().captor(11).is_none());
        assert_eq!(runtime.input().focused(), focus);
        tap(&mut runtime, Key::ArrowDown);
        runtime.update(Instant::now());
        let (selected, clipped) = selected_result_viewport(&runtime);
        assert!(selected.min.y >= clipped.min.y && selected.max.y <= clipped.max.y);
        assert_eq!(runtime.input().focused(), focus);
        assert!(controller.drain_actions().is_empty());
        assert!(bytes.lock().unwrap().is_empty());

        replace(&mut runtime, "app.new-tab");
        let (selected, clipped) = selected_result_viewport(&runtime);
        assert_eq!(
            selected.min.y, clipped.min.y,
            "filter resets to the first result"
        );
        assert!(
            palette_thumb(&runtime).is_none(),
            "one result has no scrollbar"
        );
        assert_eq!(runtime.input().focused(), focus);
        replace(&mut runtime, "not-a-real-command");
        assert!(painted(&runtime).contains("No matching commands"));
        assert!(palette_thumb(&runtime).is_none());
        assert!(controller.drain_actions().is_empty());
        assert!(bytes.lock().unwrap().is_empty());
    }
}
