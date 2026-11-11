//! Mounted two-level chooser, catalog and editor-lifetime regressions.
use super::palette_support::*;
use super::*;
use harbor_config::{ProfileId, SessionProfile, ShellSettings};
use std::collections::BTreeMap;

fn catalog() -> BTreeMap<ProfileId, SessionProfile> {
    [("alpha", "Alpha shell"), ("work", "工作 shell")]
        .into_iter()
        .map(|(id, name)| {
            (
                ProfileId::new(id).unwrap(),
                SessionProfile {
                    project: None,
                    name: name.into(),
                    launch: ShellSettings {
                        program: Some("synthetic.exe".into()),
                        ..Default::default()
                    },
                },
            )
        })
        .collect()
}
fn profiles(ui: &WorkspaceUiController, runtime: &mut Runtime) {
    ui.publish_profiles(catalog());
    let focus = ui.open_profile_chooser();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
}
fn generation(ui: &WorkspaceUiController) -> u64 {
    ui.store
        .state()
        .read()
        .popups
        .get::<palette::PaletteState>()
        .unwrap()
        .generation
}
fn keyed_rect(runtime: &Runtime, key: &str) -> Rect {
    fiber_ids(runtime)
        .into_iter()
        .find_map(|id| {
            let fiber = runtime.arena().get(id).unwrap();
            (fiber
                .key()
                .is_some_and(|candidate| candidate.text().starts_with(key)))
            .then(|| fiber.layout_rect().unwrap())
        })
        .unwrap_or_else(|| panic!("missing keyed view: {key}"))
}
fn click(runtime: &mut Runtime, rect: Rect) {
    let point = Point::new(
        (rect.min.x + rect.max.x) / 2.0,
        (rect.min.y + rect.max.y) / 2.0,
    );
    row_pointer(runtime, point, PointerPhase::Down);
    row_pointer(runtime, point, PointerPhase::Up);
}

#[test]
fn profile_entry_and_registered_command_open_the_same_second_level_without_changing_ctrl_t() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    ui.publish_profiles(catalog());
    let entry = keyed_rect(&runtime, "new-profile");
    click(&mut runtime, entry);
    assert_eq!(take_one(&ui).command, AppCommand::OpenProfileChooser);
    runtime.request_focus(&ui.open_profile_chooser());
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("Filter profiles by name or ID…"));
    assert!(painted(&runtime).contains("工作 shell"));
    assert!(ui.cancel_palette());
    runtime.request_focus(&ui.terminal_focus());
    runtime.update(Instant::now());
    open(&ui, &mut runtime);
    replace(&mut runtime, "app.open-profile");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::OpenProfileChooser);
    ui.take_palette_request(request, &tabs, false).unwrap();
    runtime.request_focus(&ui.open_profile_chooser());
    runtime.update(Instant::now());
    assert_eq!(input(&ui).text(), "");
    assert!(painted(&runtime).contains("Alpha shell"));
    assert!(ui.cancel_palette());
    runtime.request_focus(&ui.terminal_focus());
    runtime.update(Instant::now());
    down(&mut runtime, Key::Character('t'), ctrl());
    up(&mut runtime, Key::Character('t'), ctrl());
    assert_eq!(take_one(&ui).command, AppCommand::Tab(TabCommand::New));
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn profile_filter_keyboard_mouse_and_empty_results_never_submit_a_phantom() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    // This scenario admits a launch; an EOF reader fixture is intentionally not a live source.
    *tabs.active_terminal().unwrap().lock().unwrap() = Terminal::new_headless(4, 20);
    profiles(&ui, &mut runtime);
    replace(&mut runtime, "NO MATCH");
    assert!(painted(&runtime).contains("No matching profiles"));
    tap(&mut runtime, Key::ArrowDown);
    tap(&mut runtime, Key::Enter);
    assert!(ui.drain_actions().is_empty());
    replace(&mut runtime, "WoRK");
    tap(&mut runtime, Key::Enter);
    tap(&mut runtime, Key::Enter);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::LaunchProfile);
    assert_eq!(
        ui.palette_profile_intent(request).unwrap().profile.as_str(),
        "work"
    );
    ui.take_palette_request(request, &tabs, false).unwrap();
    assert!(ui.take_palette_request(request, &tabs, false).is_err());
    profiles(&ui, &mut runtime);
    // Click a non-keyboard-selected row; its stable identity must be used.
    let rect = keyed_rect(&runtime, &format!("palette-row-{}-work", generation(&ui)));
    click(&mut runtime, rect);
    let request = take_one(&ui);
    assert_eq!(
        ui.palette_profile_intent(request).unwrap().profile.as_str(),
        "work"
    );
    ui.take_palette_request(request, &tabs, false).unwrap();
    ui.publish_profiles(BTreeMap::new());
    runtime.request_focus(&ui.open_profile_chooser());
    runtime.update(Instant::now());
    tap(&mut runtime, Key::Enter);
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn accepted_catalog_updates_preserve_identity_and_removed_selection_cannot_retarget() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    profiles(&ui, &mut runtime);
    tap(&mut runtime, Key::ArrowDown); // work
    let mut changed = catalog();
    changed.insert(ProfileId::new("before").unwrap(), changed["alpha"].clone());
    ui.publish_profiles(changed.clone());
    runtime.update(Instant::now());
    tap(&mut runtime, Key::Enter);
    let request = take_one(&ui);
    assert_eq!(
        ui.palette_profile_intent(request).unwrap().profile.as_str(),
        "work"
    );
    // A queued recipe edit fails admission, not a launch of either the old or new recipe.
    changed
        .get_mut("work")
        .unwrap()
        .launch
        .args
        .push("edited".into());
    ui.publish_profiles(changed);
    assert!(ui.take_palette_request(request, &tabs, false).is_err());
    profiles(&ui, &mut runtime);
    let mut removed = catalog();
    removed.remove("alpha");
    ui.publish_profiles(removed);
    runtime.update(Instant::now());
    tap(&mut runtime, Key::Enter);
    assert!(
        ui.drain_actions().is_empty(),
        "removed alpha cannot turn into work"
    );
    tap(&mut runtime, Key::ArrowDown);
    tap(&mut runtime, Key::Enter);
    let request = take_one(&ui);
    assert_eq!(
        ui.palette_profile_intent(request).unwrap().profile.as_str(),
        "work"
    );
    ui.publish_profiles(BTreeMap::new());
    assert!(ui.take_palette_request(request, &tabs, false).is_err());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn back_retires_editor_clipboard_ime_and_old_close_without_resurrecting_search() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    ui.open_search(tabs.active_id().unwrap());
    ui.publish_search_status("1 / 2");
    profiles(&ui, &mut runtime);
    assert!(ui.search_query().is_none());
    let old_generation = generation(&ui);
    let old = input(&ui);
    replace(&mut runtime, "work");
    let effects = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    up(&mut runtime, Key::Character('v'), ctrl());
    let Some(ClipboardEffect::Editor(paste)) = effects.clipboard else {
        panic!("editor paste")
    };
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
    tap(&mut runtime, Key::Escape);
    assert!(
        ui.drain_actions().is_empty(),
        "composition Escape is not Back"
    );
    assert!(old.value().composition.is_none());
    tap(&mut runtime, Key::Escape);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::BackToCommands(old_generation));
    runtime.request_focus(&ui.back_to_commands(old_generation).unwrap());
    runtime.update(Instant::now());
    runtime.complete_clipboard(paste.id, Ok("retired".into()));
    assert_eq!(old.text(), "");
    assert_eq!(input(&ui).text(), "");
    assert!(ui.back_to_commands(old_generation).is_none());
    assert!(!ui.close_palette(old_generation));
    assert!(ui.search_query().is_none());
    assert!(runtime.input().focused().is_some());
    tap(&mut runtime, Key::Escape);
    let request = take_one(&ui);
    let AppCommand::CloseCommandPalette(current) = request.command else {
        panic!("close commands")
    };
    assert!(ui.close_palette(current));
    runtime.request_focus(&ui.terminal_focus());
    runtime.update(Instant::now());
    assert!(ui.search_query().is_none());
    assert!(bytes.lock().unwrap().is_empty());
    tap(&mut runtime, Key::Character('q'));
    assert_eq!(&*bytes.lock().unwrap(), b"q");
}

#[test]
fn profile_mouse_back_and_stale_press_do_not_run_replacement_rows() {
    let (_, ui, mut runtime, bytes) = fixture();
    profiles(&ui, &mut runtime);
    let old_generation = generation(&ui);
    let rect = keyed_rect(&runtime, &format!("palette-row-{old_generation}-alpha"));
    let point = Point::new(rect.min.x + 20.0, rect.min.y + 10.0);
    row_pointer(&mut runtime, point, PointerPhase::Down);
    let mut changed = catalog();
    changed
        .get_mut("alpha")
        .unwrap()
        .launch
        .args
        .push("new recipe".into());
    ui.publish_profiles(changed);
    runtime.update(Instant::now());
    row_pointer(&mut runtime, point, PointerPhase::Up);
    assert!(
        ui.drain_actions().is_empty(),
        "press started on an obsolete recipe"
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn profile_adapter_ime_and_held_opening_enter_cannot_submit_the_next_level() {
    use harbor_widget::winit::WinitAdapter;
    use winit::{
        event::{ElementState, Ime, WindowEvent},
        keyboard::{Key as NativeKey, KeyLocation, NamedKey},
    };
    let (tabs, ui, mut runtime, bytes) = fixture();
    ui.publish_profiles(catalog());
    open(&ui, &mut runtime);
    replace(&mut runtime, "app.open-profile");
    let mut adapter = WinitAdapter::new();
    let enter = NativeKey::Named(NamedKey::Enter);
    adapter.handle_keyboard_input(
        &mut runtime,
        &enter,
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    let request = take_one(&ui);
    ui.take_palette_request(request, &tabs, false).unwrap();
    runtime.request_focus(&ui.open_profile_chooser());
    runtime.update(Instant::now());
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
        ui.drain_actions().is_empty(),
        "held opening Enter cannot select the first profile"
    );
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Preedit("work".into(), None)),
    );
    adapter.handle_keyboard_input(
        &mut runtime,
        &enter,
        ElementState::Pressed,
        KeyLocation::Standard,
    );
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Commit("work".into())));
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
        ui.drain_actions().is_empty(),
        "IME candidate confirmation cannot launch"
    );
    assert_eq!(input(&ui).text(), "work");
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn mouse_return_retires_preedit_before_late_native_commit() {
    use harbor_widget::winit::WinitAdapter;
    use winit::event::{Ime, WindowEvent};
    let (_, ui, mut runtime, bytes) = fixture();
    profiles(&ui, &mut runtime);
    let old = generation(&ui);
    let mut adapter = WinitAdapter::new();
    adapter.handle_event(&mut runtime, &WindowEvent::Ime(Ime::Enabled));
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Preedit("work".into(), None)),
    );
    let back = keyed_rect(&runtime, "profile-back");
    click(&mut runtime, back);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::BackToCommands(old));
    runtime.request_focus(&ui.back_to_commands(old).unwrap());
    runtime.update(Instant::now());
    adapter.handle_event(
        &mut runtime,
        &WindowEvent::Ime(Ime::Commit("late retired commit".into())),
    );
    assert_eq!(input(&ui).text(), "");
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn profile_input_is_flanked_by_search_style_icons_with_shared_confirm_semantics() {
    let (_, ui, mut runtime, bytes) = fixture();
    profiles(&ui, &mut runtime);
    let generation = generation(&ui);
    let back = keyed_rect(&runtime, "profile-back");
    let editor = keyed_rect(&runtime, &format!("palette-editor-{generation}"));
    let confirm = keyed_rect(&runtime, "profile-confirm-");
    assert_eq!(back.size(), harbor_widget::layout::Size::new(32.0, 32.0));
    assert_eq!(confirm.size(), back.size());
    assert_eq!(
        back.min.y, editor.min.y,
        "button and field top edges must match"
    );
    assert_eq!(
        back.max.y, editor.max.y,
        "button and field bottom edges must match"
    );
    assert_eq!(confirm.min.y, editor.min.y);
    assert_eq!(confirm.max.y, editor.max.y);
    assert!(back.max.x < editor.min.x && editor.max.x < confirm.min.x);
    let center = |rect: Rect| (rect.min.y + rect.max.y) / 2.0;
    assert_eq!(center(back), center(editor));
    assert_eq!(center(confirm), center(editor));
    assert!(back.min.y >= 0.0 && confirm.max.x <= 1000.0);
    assert!(!painted(&runtime).contains("← Back"));
    let icons: Vec<_> = runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .filter_map(|item| match item.primitive {
            Primitive::Icon { icon, rect, .. } => Some((icon, rect)),
            _ => None,
        })
        .collect();
    assert!(
        icons
            .iter()
            .any(|(icon, rect)| *icon == LucideIcon::ArrowLeft && back.contains(rect.min))
    );
    assert!(
        icons
            .iter()
            .any(|(icon, rect)| *icon == LucideIcon::Check && confirm.contains(rect.min))
    );
    let focused = runtime.input().focused();
    let point = Point::new(confirm.min.x + 16.0, confirm.min.y + 16.0);
    row_pointer(&mut runtime, point, PointerPhase::Move);
    let delta = runtime.pending_delta().unwrap();
    assert!(
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .any(|item| matches!(
                item.primitive,
                Primitive::RoundedQuad { rect, color, .. } if rect == confirm && color.a == 0.12
            )),
        "same glass feedback as search buttons"
    );
    assert_eq!(runtime.input().focused(), focused);
    // Arrow navigation updates which profile the right-hand button confirms.
    tap(&mut runtime, Key::ArrowDown);
    runtime.update(Instant::now());
    let confirm = keyed_rect(&runtime, "profile-confirm-");
    click(&mut runtime, confirm);
    click(&mut runtime, confirm);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::LaunchProfile);
    assert_eq!(
        ui.palette_profile_intent(request).unwrap().profile.as_str(),
        "work"
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn profile_confirm_cannot_submit_empty_stale_or_composing_selection() {
    let (_, ui, mut runtime, bytes) = fixture();
    profiles(&ui, &mut runtime);
    replace(&mut runtime, "no match");
    let confirm = keyed_rect(&runtime, "profile-confirm-");
    click(&mut runtime, confirm);
    assert!(ui.drain_actions().is_empty());
    replace(&mut runtime, "work");
    let confirm = keyed_rect(&runtime, "profile-confirm-");
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
    click(&mut runtime, confirm);
    assert!(
        ui.drain_actions().is_empty(),
        "confirmation must not bypass IME ownership"
    );
    tap(&mut runtime, Key::Escape);
    // The pressed button is invalidated by a changed catalog, just like a result row.
    let point = Point::new(confirm.min.x + 16.0, confirm.min.y + 16.0);
    row_pointer(&mut runtime, point, PointerPhase::Down);
    let mut changed = catalog();
    changed
        .get_mut("work")
        .unwrap()
        .launch
        .args
        .push("edited".into());
    ui.publish_profiles(changed);
    runtime.update(Instant::now());
    row_pointer(&mut runtime, point, PointerPhase::Up);
    assert!(ui.drain_actions().is_empty());
    ui.publish_profiles(Default::default());
    runtime.update(Instant::now());
    let confirm = keyed_rect(&runtime, "profile-confirm-");
    click(&mut runtime, confirm);
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn profile_input_controls_share_edges_across_text_metrics_without_a_header() {
    for line_height in [16.0, 20.0, 24.0, 31.5] {
        let (_, ui, mut runtime, _) = fixture();
        let mut metrics = *runtime.text_metrics();
        metrics.line_height = line_height;
        metrics.ascent = line_height * 0.75;
        runtime.set_text_metrics(metrics);
        runtime.request_focus(&ui.open_palette());
        runtime.update(Instant::now());
        let commands = keyed_rect(&runtime, &format!("palette-editor-{}", generation(&ui)));
        assert_eq!(commands.size().height, line_height + 12.0);
        profiles(&ui, &mut runtime);
        let back = keyed_rect(&runtime, "profile-back");
        let editor = keyed_rect(&runtime, &format!("palette-editor-{}", generation(&ui)));
        let confirm = keyed_rect(&runtime, "profile-confirm-");
        assert_eq!(
            editor.size().height,
            commands.size().height,
            "both palette levels must use the same field height"
        );
        let delta = runtime.pending_delta().unwrap();
        for (icon, bounds) in [(LucideIcon::ArrowLeft, back), (LucideIcon::Check, confirm)] {
            let glyph = delta
                .added
                .iter()
                .chain(&delta.modified)
                .find_map(|item| match item.primitive {
                    Primitive::Icon {
                        icon: candidate,
                        rect,
                        ..
                    } if candidate == icon && bounds.contains(rect.min) => Some(rect),
                    _ => None,
                })
                .expect("button glyph");
            assert_eq!(
                glyph.size(),
                harbor_widget::layout::Size::new(18.0, 18.0),
                "parent height must not squeeze icon artwork"
            );
            assert_eq!(
                (glyph.min.y + glyph.max.y) / 2.0,
                (bounds.min.y + bounds.max.y) / 2.0
            );
        }
        let caret = delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match item.primitive {
                Primitive::Quad { rect, .. }
                    if rect.size().width == 1.0
                        && rect.size().height == line_height
                        && editor.contains(rect.min) =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .expect("focused field caret");
        assert_eq!(
            (caret.min.y + caret.max.y) / 2.0,
            (editor.min.y + editor.max.y) / 2.0,
            "caret must remain vertically centered at {line_height}dp"
        );
        for button in [back, confirm] {
            assert_eq!(
                button.min.y, editor.min.y,
                "top edge at line height {line_height}"
            );
            assert_eq!(
                button.max.y, editor.max.y,
                "bottom edge at line height {line_height}"
            );
        }
        let (panel, _) = palette_geometry(&runtime);
        assert_eq!(
            editor.min.y,
            panel.min.y + 8.0,
            "no header or reserved header space"
        );
        assert!(
            !painted(&runtime)
                .lines()
                .any(|line| line == "New session from profile")
        );
        let point = Point::new(back.min.x + 16.0, (back.min.y + back.max.y) / 2.0);
        row_pointer(&mut runtime, point, PointerPhase::Move);
        let delta = runtime.pending_delta().unwrap();
        assert!(
            delta
                .added
                .iter()
                .chain(&delta.modified)
                .any(|item| matches!(
                    item.primitive,
                    Primitive::RoundedQuad { rect, color, .. } if rect == back && color.a == 0.12
                )),
            "visible button glass must fill its entire height at {line_height}dp"
        );
    }
}
