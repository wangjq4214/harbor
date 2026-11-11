//! Mounted quick-save flow: real editor, modal routing, one-shot Host admission.
use super::palette_support::*;
use super::*;
use crate::{QuickSaveDraft, command::CommandSource};
use harbor_config::{ProfileId, SessionProfile, ShellSettings};
use harbor_widget::layout::Size;

fn draft() -> QuickSaveDraft {
    QuickSaveDraft {
        project: Some(harbor_config::ProjectRecipe {
            environment: harbor_config::ProjectEnvironment::LocalPwsh,
            directory: "C:/synthetic-project".into(),
            agent: None,
        }),
        source: TabId(1),
        launch: ShellSettings {
            program: Some("synthetic-source.exe".into()),
            args: vec!["".into(), "source arg".into()],
            directory: Some(std::env::current_dir().unwrap()),
            environment: [
                ("ONE".into(), "synthetic-secret-one".into()),
                ("TWO".into(), "synthetic-secret-two".into()),
            ]
            .into(),
        },
    }
}
fn popup(ui: &WorkspaceUiController, command: AppCommand) -> AppCommandRequest {
    AppCommandRequest::popup(
        command,
        ui.store.state().read().popups.generation().unwrap(),
    )
}
fn start(ui: &WorkspaceUiController, runtime: &mut Runtime, draft: QuickSaveDraft) {
    runtime.request_focus(&ui.open_quick_save(draft));
    runtime.update(Instant::now());
}
fn advance(ui: &WorkspaceUiController, runtime: &mut Runtime) {
    tap(runtime, Key::Enter);
    let request = take_one(ui);
    assert_eq!(request.command, AppCommand::AdvanceQuickSave);
    runtime.request_focus(&ui.navigate_quick_save(request).unwrap());
    runtime.update(Instant::now());
}
fn keyed_rect(runtime: &Runtime, key: &str) -> Rect {
    fiber_ids(runtime)
        .into_iter()
        .find_map(|id| {
            let fiber = runtime.arena().get(id).unwrap();
            (fiber.key().is_some_and(|candidate| candidate.text() == key))
                .then(|| fiber.layout_rect().unwrap())
        })
        .unwrap_or_else(|| panic!("missing view {key}"))
}
fn focus_control(runtime: &mut Runtime, key: &str) {
    for _ in 0..64 {
        let root = fiber_ids(runtime)
            .into_iter()
            .find(|id| {
                runtime
                    .arena()
                    .get(*id)
                    .unwrap()
                    .key()
                    .is_some_and(|k| k.text() == key)
            })
            .unwrap();
        let mut descendants = vec![root];
        while let Some(id) = descendants.pop() {
            if runtime.input().focused() == Some(id) {
                return;
            }
            descendants.extend(runtime.arena().get(id).unwrap().children());
        }
        tap(runtime, Key::Tab);
        runtime.update(Instant::now());
    }
    panic!("control did not become focused: {key}");
}
fn click(runtime: &mut Runtime, rect: Rect) {
    let point = Point::new(
        (rect.min.x + rect.max.x) / 2.0,
        (rect.min.y + rect.max.y) / 2.0,
    );
    row_pointer(runtime, point, PointerPhase::Down);
    row_pointer(runtime, point, PointerPhase::Up);
}
fn has_control(runtime: &Runtime, key: &str) -> bool {
    fiber_ids(runtime).into_iter().any(|id| {
        runtime
            .arena()
            .get(id)
            .unwrap()
            .key()
            .is_some_and(|k| k.text() == key)
    })
}
fn edit_location(runtime: &mut Runtime) {
    click_control(runtime, "project-edit");
    runtime.update(Instant::now());
}

fn click_control(runtime: &mut Runtime, key: &str) {
    focus_control(runtime, key);
    let rect = keyed_rect(runtime, key);
    click(runtime, rect);
}

// Wrapped TextLabel emits one primitive per visual line without changing source text.
fn review_text(runtime: &Runtime) -> String {
    painted(runtime).replace('\n', "")
}
fn panel_rect(runtime: &Runtime) -> Rect {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::BackdropMaterial { rect, .. } | Primitive::BackdropBlur { rect, .. } => {
                Some(rect)
            }
            _ => None,
        })
        .expect("quick-save panel")
}

fn assert_borderless(runtime: &Runtime, key: &str) {
    let rect = keyed_rect(runtime, key);
    let delta = runtime.pending_delta().unwrap();
    assert!(
        !delta.added.iter().chain(&delta.modified).any(|item| {
            matches!(item.primitive,
            Primitive::Border { rect: outline, .. }
            | Primitive::RoundedBorder { rect: outline, .. } if outline == rect)
        }),
        "{key} must look like text, not another input"
    );
}

#[test]
fn quick_save_naming_matches_palette_input_geometry_across_widths_and_dpi() {
    for (width, scale) in [(1000, 1.0), (400, 1.0), (280, 1.5), (1000, 2.0)] {
        let (_, ui, mut runtime, bytes) = fixture();
        runtime.set_viewport(harbor_widget::renderer::Viewport::new(
            (width as f32 * scale) as u32,
            (600.0 * scale) as u32,
            scale,
        ));
        open(&ui, &mut runtime);
        let palette_panel = panel_rect(&runtime);
        let generation = ui.store.state().read().popups.generation().unwrap();
        let palette_input = keyed_rect(&runtime, &format!("palette-editor-{generation}"));
        let text_origin = |runtime: &Runtime, expected: &str| {
            let delta = runtime.pending_delta().unwrap();
            delta
                .added
                .iter()
                .chain(&delta.modified)
                .filter_map(|item| item.primitive.text_run())
                .find_map(|(text, origin, ..)| (text == expected).then_some(origin))
                .unwrap()
        };
        let palette_text = text_origin(&runtime, "Type a command label or ID…");
        start(&ui, &mut runtime, draft());
        let naming_panel = panel_rect(&runtime);
        let generation = ui.store.state().read().popups.generation().unwrap();
        let naming_input = keyed_rect(&runtime, &format!("save-editor-{generation}"));
        let advance = keyed_rect(&runtime, "save-advance");
        let naming_text = text_origin(&runtime, "Preset name…");
        assert_eq!(naming_panel.min, palette_panel.min);
        assert_eq!(naming_panel.size().width, palette_panel.size().width);
        assert_eq!(naming_input.min, palette_input.min);
        assert_eq!(naming_input.size().height, palette_input.size().height);
        assert_eq!(advance.size().height, palette_input.size().height);
        assert_eq!(advance.max.x, palette_input.max.x);
        assert_eq!(
            naming_text.x - naming_input.min.x,
            palette_text.x - palette_input.min.x
        );
        assert_eq!(
            naming_text.y - naming_input.min.y,
            palette_text.y - palette_input.min.y
        );
        assert_eq!(
            naming_panel.max.y - naming_input.max.y,
            palette_input.min.y - palette_panel.min.y
        );
        assert!(
            naming_panel.size().height < palette_panel.size().height,
            "no command-list space"
        );
        replace(&mut runtime, "Stable dimensions");
        assert_eq!(panel_rect(&runtime), naming_panel);
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn quick_save_environment_segments_have_spaced_neutral_glass_hover_and_no_extra_press() {
    let (_, ui, mut runtime, bytes) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Segment states");
    advance(&ui, &mut runtime);
    edit_location(&mut runtime);
    let local = keyed_rect(&runtime, "pwsh");
    let wsl = keyed_rect(&runtime, "wsl");
    let ssh = keyed_rect(&runtime, "ssh");
    assert!(
        local.max.x < wsl.min.x && wsl.max.x < ssh.min.x,
        "segment surfaces need visible gaps"
    );
    let surface = |runtime: &Runtime, key: &str| {
        let rect = keyed_rect(runtime, key);
        let delta = runtime.pending_delta().unwrap();
        let fill = delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match item.primitive {
                Primitive::Quad {
                    rect: actual,
                    color,
                    ..
                } if actual == rect => Some(color),
                _ => None,
            })
            .unwrap();
        let material =
            delta
                .added
                .iter()
                .chain(&delta.modified)
                .find_map(|item| match item.primitive {
                    Primitive::BackdropMaterial {
                        rect: actual,
                        sigma,
                        refraction,
                        material,
                        ..
                    } if actual == rect => Some((sigma, refraction, material)),
                    _ => None,
                });
        (fill, material)
    };
    let selected = surface(&runtime, "pwsh");
    assert!(selected.1.is_some());
    assert_ne!(selected.0.r, selected.0.b);
    assert!(surface(&runtime, "wsl").1.is_none());
    let center = |rect: Rect| {
        Point::new(
            (rect.min.x + rect.max.x) / 2.0,
            (rect.min.y + rect.max.y) / 2.0,
        )
    };
    row_pointer(&mut runtime, center(wsl), PointerPhase::Move);
    let hovered = surface(&runtime, "wsl");
    assert!(hovered.1.is_some(), "hover must sample real liquid glass");
    assert_eq!(hovered.0.r, hovered.0.g);
    assert_eq!(hovered.0.g, hovered.0.b);
    assert!(hovered.0.a < 0.2, "no opaque grey hover");
    assert_eq!(surface(&runtime, "pwsh"), selected);
    assert_borderless(&runtime, "wsl");
    row_pointer(&mut runtime, center(wsl), PointerPhase::Down);
    assert_eq!(
        surface(&runtime, "wsl"),
        hovered,
        "no added press treatment"
    );
    assert_eq!(
        surface(&runtime, "pwsh"),
        selected,
        "pointer down alone is not selection"
    );
    row_pointer(&mut runtime, center(wsl), PointerPhase::Up);
    assert_eq!(
        surface(&runtime, "wsl"),
        selected,
        "successful activation selects WSL"
    );
    assert!(has_control(&runtime, "project-target"));
    row_pointer(&mut runtime, center(wsl), PointerPhase::Down);
    assert_eq!(
        surface(&runtime, "wsl"),
        selected,
        "pressing selection retains its tint"
    );
    row_pointer(&mut runtime, center(wsl), PointerPhase::Up);
    row_pointer(&mut runtime, Point::ZERO, PointerPhase::Move);
    assert!(surface(&runtime, "pwsh").1.is_none());
    focus_control(&mut runtime, "ssh");
    let delta = runtime.pending_delta().unwrap();
    assert!(
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .any(|item| matches!(item.primitive,
        Primitive::Border { rect, .. } if rect == ssh)),
        "keyboard focus stays visible without hover"
    );
    tap(&mut runtime, Key::Enter);
    runtime.update(Instant::now());
    assert_eq!(surface(&runtime, "ssh"), selected);
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_naming_and_review_are_compact_borderless_and_readable() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    source.launch.args.clear();
    source.launch.directory = Some(std::path::PathBuf::from(r"D:\Code\harbor"));
    source.launch.environment.clear();
    start(&ui, &mut runtime, source);

    let naming = panel_rect(&runtime);
    assert!(
        naming.size().height < 100.0,
        "no empty results area: {naming:?}"
    );
    assert_eq!(naming.size().width, 620.0);
    assert!(!painted(&runtime).contains("Save current session as preset"));
    assert!(!painted(&runtime).contains("Enter a preset name"));

    replace(&mut runtime, "My preset");
    advance(&ui, &mut runtime);
    let review = panel_rect(&runtime);
    assert!(
        review.size().height < 520.0,
        "review should fit its content: {review:?}"
    );
    assert!(has_control(&runtime, "project-edit"));
    assert!(!has_control(&runtime, "project-directory"));
    assert!(!has_control(&runtime, "project-apply"));
    assert!(!has_control(&runtime, "overrides-heading"));
    assert!(!has_control(&runtime, "details-divider"));
    let text = painted(&runtime);
    for expected in [
        "Save preset",
        "Edit name",
        "My preset",
        "No environment overrides will be saved.",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text}");
    }
    assert!(
        !text.contains("synthetic-source.exe"),
        "provenance starts collapsed"
    );
    focus_control(&mut runtime, "quick-save-details");
    tap(&mut runtime, Key::Enter);
    runtime.update(Instant::now());
    let details = painted(&runtime);
    assert!(details.contains("Source program (provenance): synthetic-source.exe"));
    assert!(details.contains("Source arguments (not replayed): None"));
    assert!(
        details.contains(r"Initial directory (not current-directory evidence): D:\Code\harbor")
    );
    assert!(!text.contains("Some("));
    for key in [
        "review-name",
        "review-program",
        "review-args",
        "review-directory",
    ] {
        assert_borderless(&runtime, key);
    }

    // The footer is visible without scrolling the header or actions away.
    runtime.set_viewport(harbor_widget::renderer::Viewport::new(400, 180, 1.0));
    runtime.update(Instant::now());
    focus_control(&mut runtime, "quick-save-final");
    runtime.update(Instant::now());
    let save = keyed_rect(&runtime, "quick-save-final");
    assert!(
        save.min.y >= 12.0 && save.max.y <= 168.0,
        "Save must remain visible: {save:?}"
    );
    tap(&mut runtime, Key::Enter);
    assert_eq!(take_one(&ui).command, AppCommand::CommitQuickSave);
}

#[test]
fn quick_save_review_uses_palette_glass_and_translucent_inputs_without_a_scrim() {
    let (_, ui, mut runtime, _) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Transparent review");
    advance(&ui, &mut runtime);
    edit_location(&mut runtime);
    let appearance = crate::appearance::GlassAppearance::command_palette();
    let panel = panel_rect(&runtime);
    let directory = keyed_rect(&runtime, "project-directory");
    let delta = runtime.pending_delta().unwrap();
    let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    assert!(
        items.iter().any(|item| matches!(item.primitive,
            Primitive::BackdropMaterial { rect, sigma, refraction, material, .. }
                if rect == panel && sigma == appearance.blur_sigma
                    && refraction == appearance.refraction && Some(material) == appearance.material
        )),
        "review must keep the palette's full glass material, not just a blur"
    );
    assert!(
        items.iter().any(|item| matches!(item.primitive,
            Primitive::RoundedQuad { rect, color, .. }
                if rect == directory && color == appearance.input_fill
        )),
        "project inputs must retain the palette's translucent editor surface"
    );
    assert!(
        !items.iter().any(|item| matches!(item.primitive,
            Primitive::Quad { rect, color, .. }
                if rect.min == Point::ZERO && rect.size() == Size::new(1000.0, 500.0)
                    && color.a > 0.0 && color.r == 0.0 && color.g == 0.0 && color.b == 0.0
        )),
        "the review must not darken the whole window behind its glass"
    );
}

#[test]
fn quick_save_final_actions_align_right_and_wrap_at_narrow_widths() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    source.launch.environment.clear();
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Actions");
    advance(&ui, &mut runtime);
    let cancel = keyed_rect(&runtime, "quick-save-cancel");
    let back = keyed_rect(&runtime, "quick-save-back");
    let save = keyed_rect(&runtime, "quick-save-final");
    let divider = keyed_rect(&runtime, "actions-divider");
    assert!(cancel.max.x <= save.min.x);
    assert!(
        back.max.y < cancel.min.y,
        "name editing belongs in the header"
    );
    assert_eq!(cancel.min.y, save.min.y);
    assert!((save.max.x - divider.max.x).abs() < 1.0);

    focus_control(&mut runtime, "quick-save-final");
    let focused = runtime.input().focused();
    runtime.set_viewport(harbor_widget::renderer::Viewport::new(180, 600, 1.0));
    runtime.update(Instant::now());
    let cancel = keyed_rect(&runtime, "quick-save-cancel");
    let save = keyed_rect(&runtime, "quick-save-final");
    let divider = keyed_rect(&runtime, "actions-divider");
    assert!(
        save.min.y > cancel.min.y,
        "actions should wrap: {cancel:?} {save:?}"
    );
    for rect in [cancel, save] {
        assert!(rect.min.x >= divider.min.x && rect.max.x <= divider.max.x);
        assert!(rect.size().width > 0.0);
    }
    assert_eq!(
        runtime.input().focused(),
        focused,
        "wrapping must retain keyboard focus"
    );
    tap(&mut runtime, Key::Enter);
    assert_eq!(take_one(&ui).command, AppCommand::CommitQuickSave);
}

#[test]
fn quick_save_entry_and_palette_reach_input_only_naming_and_equivalent_advance() {
    let (tabs, ui, mut runtime, bytes) = fixture();
    let entry = keyed_rect(&runtime, "save-profile");
    click(&mut runtime, entry);
    assert_eq!(take_one(&ui).command, AppCommand::SaveCurrentSession);
    open(&ui, &mut runtime);
    replace(&mut runtime, "app.save-current-session");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::SaveCurrentSession);
    ui.take_palette_request(request, &tabs, false).unwrap();
    start(&ui, &mut runtime, draft());
    assert!(!painted(&runtime).contains("No matching commands"));
    replace(&mut runtime, "Saved source");
    let input = keyed_rect(
        &runtime,
        &format!(
            "save-editor-{}",
            ui.store.state().read().popups.generation().unwrap()
        ),
    );
    let button = keyed_rect(&runtime, "save-advance");
    assert!(button.min.x >= input.max.x);
    assert!((button.size().height - input.size().height).abs() < 0.01);
    click(&mut runtime, button);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::AdvanceQuickSave);
    runtime.request_focus(&ui.navigate_quick_save(request).unwrap());
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("Saved source"));
    assert!(painted(&runtime).contains("Source details"));
    assert!(!painted(&runtime).contains("synthetic-secret"));
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_invalid_names_ime_and_repeated_enter_never_commit_or_leak() {
    let (_, ui, mut runtime, bytes) = fixture();
    ui.publish_profiles(
        [(
            ProfileId::new("old").unwrap(),
            SessionProfile {
                project: None,
                name: "Duplicate".into(),
                launch: ShellSettings {
                    program: Some("cmd.exe".into()),
                    ..Default::default()
                },
            },
        )]
        .into(),
    );
    start(&ui, &mut runtime, draft());
    let editor_focus = runtime.input().focused().unwrap();
    for name in ["", "  ", "Duplicate"] {
        replace(&mut runtime, name);
        tap(&mut runtime, Key::Enter);
        for request in ui.drain_actions() {
            assert!(ui.navigate_quick_save(request).is_none());
        }
        assert!(ui.quick_save_is_open());
        let button = keyed_rect(&runtime, "save-advance");
        click(&mut runtime, button);
        let request = take_one(&ui);
        assert_eq!(request.command, AppCommand::AdvanceQuickSave);
        assert!(ui.navigate_quick_save(request).is_none());
        runtime.set_focus(editor_focus);
        runtime.update(Instant::now());
    }
    replace(&mut runtime, "Fresh");
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("界", None)));
    tap(&mut runtime, Key::Enter);
    assert!(
        ui.drain_actions().is_empty(),
        "candidate confirmation is not submit"
    );
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("", None)));
    replace(&mut runtime, "Fresh");
    down(&mut runtime, Key::Enter, Modifiers::default());
    let old = take_one(&ui);
    runtime.request_focus(&ui.navigate_quick_save(old).unwrap());
    runtime.update(Instant::now());
    for _ in 0..5 {
        down(&mut runtime, Key::Enter, Modifiers::default());
    }
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert!(
        ui.drain_actions().is_empty(),
        "review initially focuses a non-saving sink"
    );
    assert!(ui.navigate_quick_save(old).is_none());
    down(&mut runtime, Key::Character('t'), ctrl());
    up(&mut runtime, Key::Character('t'), ctrl());
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_return_retains_capture_and_name_stale_delivery_cannot_replace_it() {
    let (_, ui, mut runtime, bytes) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Retained");
    let old = popup(&ui, AppCommand::AdvanceQuickSave);
    let effects = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    up(&mut runtime, Key::Character('v'), ctrl());
    let Some(ClipboardEffect::Editor(paste)) = effects.clipboard else {
        panic!("editor paste")
    };
    runtime.request_focus(&ui.navigate_quick_save(old).unwrap());
    runtime.update(Instant::now());
    runtime.complete_clipboard(paste.id, Ok("late clipboard".into()));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("late IME".into())));
    let back = popup(&ui, AppCommand::EditQuickSave);
    runtime.request_focus(&ui.navigate_quick_save(back).unwrap());
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("Retained"));
    assert!(!painted(&runtime).contains("late"));
    advance(&ui, &mut runtime);
    focus_control(&mut runtime, "quick-save-details");
    tap(&mut runtime, Key::Enter);
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("source arg"));
    let retired = popup(&ui, AppCommand::CancelQuickSave);
    start(&ui, &mut runtime, draft());
    assert!(!ui.cancel_quick_save(retired));
    assert!(
        ui.take_quick_save_submission(AppCommandRequest::shortcut(AppCommand::CommitQuickSave))
            .is_none()
    );
    tap(&mut runtime, Key::Escape);
    assert!(ui.cancel_quick_save(take_one(&ui)));
    runtime.request_focus(&ui.terminal_focus());
    runtime.update(Instant::now());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_only_selected_environment_is_admitted_after_acknowledgment_once() {
    let (_, ui, mut runtime, _) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "No env");
    advance(&ui, &mut runtime);
    // Reach Save through ordinary keyboard traversal, not initial/held Enter.
    focus_control(&mut runtime, "quick-save-final");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::CommitQuickSave);
    let submission = ui.take_quick_save_submission(request).unwrap();
    assert!(submission.profile.launch.environment.is_empty());
    assert!(ui.take_quick_save_submission(request).is_none());
    ui.finish_quick_save(submission.generation, Err("synthetic failure".into()));
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("synthetic failure"));
    click_control(&mut runtime, "environment-0");
    runtime.update(Instant::now());
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
    assert!(
        ui.drain_actions().is_empty(),
        "selection alone does not authorize secrets"
    );
    click_control(&mut runtime, "quick-save-ack");
    runtime.update(Instant::now());
    click_control(&mut runtime, "quick-save-final");
    let request = take_one(&ui);
    assert!(matches!(request.source, CommandSource::Popup { .. }));
    let submission = ui.take_quick_save_submission(request).unwrap();
    assert_eq!(
        submission.profile.launch.environment,
        [("ONE".into(), "synthetic-secret-one".into())].into()
    );
    assert!(!format!("{submission:?}").contains("synthetic-secret"));
}

#[test]
fn quick_save_unmounted_approval_and_focus_loss_cannot_commit_a_replacement() {
    let (_, ui, mut runtime, bytes) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Old draft");
    advance(&ui, &mut runtime);
    click_control(&mut runtime, "quick-save-final");
    let old = take_one(&ui);
    runtime.set_root(
        harbor_widget::SizedBox::new(harbor_widget::layout::Size::new(10.0, 10.0)).unwrap(),
    );
    runtime.update(Instant::now());
    assert!(!ui.quick_save_is_open());
    assert!(ui.take_quick_save_submission(old).is_none());
    runtime.set_root(tab_workspace(ui.clone(), false));
    runtime.update(Instant::now());
    start(&ui, &mut runtime, draft());
    assert!(ui.take_quick_save_submission(old).is_none());
    assert!(ui.cancel_quick_save_on_focus_loss());
    runtime.request_focus(&ui.terminal_focus());
    runtime.update(Instant::now());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_unknown_context_requires_explicit_correction_and_retains_it() {
    let (_, ui, mut runtime, bytes) = fixture();
    let mut source = draft();
    source.project = None;
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Corrected");
    advance(&ui, &mut runtime);
    assert!(painted(&runtime).contains("unknown"));
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
    assert!(ui.drain_actions().is_empty());
    focus_control(&mut runtime, "project-directory");
    replace(&mut runtime, "C:/corrected 名");
    focus_control(&mut runtime, "project-apply");
    tap(&mut runtime, Key::Enter);
    runtime.update(Instant::now());
    let back = popup(&ui, AppCommand::EditQuickSave);
    runtime.request_focus(&ui.navigate_quick_save(back).unwrap());
    runtime.update(Instant::now());
    advance(&ui, &mut runtime);
    focus_control(&mut runtime, "quick-save-final");
    tap(&mut runtime, Key::Enter);
    let submission = ui.take_quick_save_submission(take_one(&ui)).unwrap();
    assert_eq!(
        submission.profile.project.unwrap().directory,
        "C:/corrected 名"
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_suspected_credentials_block_save_until_visible_omission() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    source.project.as_mut().unwrap().agent = Some(harbor_config::AgentInvocation {
        program: "pi".into(),
        args: vec!["--token".into(), "synthetic-private-value".into()],
    });
    source.launch.args = vec!["--password=synthetic-source-private".into()];
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Safe omission");
    advance(&ui, &mut runtime);
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
    assert!(ui.drain_actions().is_empty());
    focus_control(&mut runtime, "project-agent-include");
    let text = painted(&runtime);
    assert!(!text.contains("synthetic-private-value"));
    assert!(!text.contains("synthetic-source-private"));
    tap(&mut runtime, Key::Enter);
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("explicitly omitted"));
    focus_control(&mut runtime, "quick-save-final");
    tap(&mut runtime, Key::Enter);
    let submission = ui.take_quick_save_submission(take_one(&ui)).unwrap();
    assert!(submission.profile.project.unwrap().agent.is_none());
    assert!(submission.profile.launch.args.is_empty());
}

#[test]
fn quick_save_original_agent_arguments_survive_review_and_require_independent_ack() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    let invocation = harbor_config::AgentInvocation {
        program: "codex".into(),
        args: vec!["".into(), "a b".into(), "q\"\\名".into(), "--resume".into()],
    };
    source.project.as_mut().unwrap().agent = Some(invocation.clone());
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Original args");
    advance(&ui, &mut runtime);
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
    assert!(ui.drain_actions().is_empty());
    focus_control(&mut runtime, "project-agent-ack");
    tap(&mut runtime, Key::Enter);
    runtime.update(Instant::now());
    focus_control(&mut runtime, "quick-save-final");
    tap(&mut runtime, Key::Enter);
    let submission = ui.take_quick_save_submission(take_one(&ui)).unwrap();
    assert_eq!(submission.profile.project.unwrap().agent, Some(invocation));
    assert!(submission.profile.launch.environment.is_empty());
}

#[test]
fn quick_save_environment_choices_have_persistent_environment_specific_fields_and_explicit_apply() {
    let (_, ui, mut runtime, bytes) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Environment review");
    advance(&ui, &mut runtime);
    edit_location(&mut runtime);
    assert!(!review_text(&runtime).contains("WSL distribution"));
    assert!(!review_text(&runtime).contains("SSH destination"));
    assert!(!fiber_ids(&runtime).iter().any(|id| {
        runtime
            .arena()
            .get(*id)
            .unwrap()
            .key()
            .is_some_and(|k| k.text() == "project-target")
    }));

    click_control(&mut runtime, "wsl");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("WSL distribution"));
    assert!(review_text(&runtime).contains("confirm the launch location before saving"));
    click_control(&mut runtime, "project-apply");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("Enter a valid environment target"));
    focus_control(&mut runtime, "project-target");
    replace(&mut runtime, "Ubuntu");
    focus_control(&mut runtime, "project-directory");
    replace(&mut runtime, "/home/synthetic/项目");
    // Merely filling the form does not approve it; the explicit gate remains.
    assert!(review_text(&runtime).contains("Unknown or edited context"));
    click_control(&mut runtime, "project-apply");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("Context explicitly corrected"));
    assert!(!review_text(&runtime).contains("Enter a valid environment target"));

    edit_location(&mut runtime);
    // Keyboard traversal + activation uses the same caller-owned selection.
    focus_control(&mut runtime, "ssh");
    tap(&mut runtime, Key::Space);
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("SSH destination"));
    assert!(!review_text(&runtime).contains("WSL distribution"));
    focus_control(&mut runtime, "pwsh");
    tap(&mut runtime, Key::Space);
    runtime.update(Instant::now());
    assert!(!fiber_ids(&runtime).iter().any(|id| {
        runtime
            .arena()
            .get(*id)
            .unwrap()
            .key()
            .is_some_and(|k| k.text() == "project-target")
    }));
    assert!(ui.drain_actions().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_agent_and_override_checkboxes_have_independent_acknowledgements_and_pending_state() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    source.project.as_mut().unwrap().agent = Some(harbor_config::AgentInvocation {
        program: "pi".into(),
        args: vec!["--resume".into(), "synthetic-session".into()],
    });
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Independent choices");
    advance(&ui, &mut runtime);
    click_control(&mut runtime, "environment-0");
    runtime.update(Instant::now());
    click_control(&mut runtime, "project-agent-ack");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("confirm above before saving"));
    assert!(ui.drain_actions().is_empty());
    click_control(&mut runtime, "quick-save-ack");
    runtime.update(Instant::now());
    click_control(&mut runtime, "project-agent-include");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("explicitly omitted"));
    click_control(&mut runtime, "project-agent-include");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("Review agent arguments"));
    assert!(
        !review_text(&runtime).contains("confirm above before saving"),
        "agent choice must not reset the independent override acknowledgement"
    );
    click_control(&mut runtime, "project-agent-ack");
    runtime.update(Instant::now());
    click_control(&mut runtime, "quick-save-final");
    let request = take_one(&ui);
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("Saving…"));
    // Disabled controls cannot alter the approved snapshot while Host admission waits.
    let include = keyed_rect(&runtime, "project-agent-include");
    click(&mut runtime, include);
    let submission = ui.take_quick_save_submission(request).unwrap();
    assert!(submission.profile.project.unwrap().agent.is_some());
    assert_eq!(submission.profile.launch.environment.len(), 1);
    assert!(ui.drain_actions().is_empty());
}

#[test]
fn quick_save_long_values_remain_selectable_and_copyable_across_resize_and_disclosure() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    let argument = format!(
        "{}项目 e\u{0301} 👩\u{200d}💻",
        "synthetic-long-argument/".repeat(40)
    );
    source.launch.args = vec![argument.clone()];
    source.project.as_mut().unwrap().directory =
        format!("C:/{}项目", "synthetic-long-path/".repeat(35));
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Long values");
    advance(&ui, &mut runtime);
    click_control(&mut runtime, "quick-save-details");
    runtime.update(Instant::now());
    let expected = format!("Source arguments (not replayed): {argument:?}");
    for (width, height) in [(1000, 500), (280, 180), (800, 600)] {
        runtime.set_viewport(harbor_widget::renderer::Viewport::new(width, height, 1.0));
        runtime.update(Instant::now());
        focus_control(&mut runtime, "review-args");
        down(&mut runtime, Key::Character('a'), ctrl());
        up(&mut runtime, Key::Character('a'), ctrl());
        let effect = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('c'),
            modifiers: ctrl(),
        }));
        up(&mut runtime, Key::Character('c'), ctrl());
        let Some(ClipboardEffect::Editor(request)) = effect.clipboard else {
            panic!("copy request")
        };
        assert_eq!(request.write.as_deref(), Some(expected.as_str()));
    }
    assert!(ui.drain_actions().is_empty());
}

#[test]
fn quick_save_summary_has_ordered_content_and_fixed_chrome_while_body_scrolls() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    source.project.as_mut().unwrap().agent = Some(harbor_config::AgentInvocation {
        program: "pi".into(),
        args: vec!["--resume".into(), "synthetic-session".into()],
    });
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Grouped review");
    advance(&ui, &mut runtime);
    let heading_scale = |runtime: &Runtime, title: &str| {
        let delta = runtime.pending_delta().unwrap();
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| item.primitive.text_run())
            .find_map(|(text, _, _, scale)| (text == title).then_some(scale))
            .unwrap_or_else(|| panic!("missing heading {title}"))
    };
    assert!(heading_scale(&runtime, "Save preset") > heading_scale(&runtime, "Launch location"));
    assert_eq!(
        heading_scale(&runtime, "Launch contents"),
        heading_scale(&runtime, "Environment variables")
    );
    let keys = [
        "project-heading",
        "launch-heading",
        "overrides-heading",
        "quick-save-details",
    ];
    for pair in keys.windows(2) {
        assert!(keyed_rect(&runtime, pair[0]).max.y < keyed_rect(&runtime, pair[1]).min.y);
    }
    for key in [
        "project-divider",
        "agent-divider",
        "overrides-divider",
        "details-divider",
        "project-directory",
    ] {
        assert!(!has_control(&runtime, key), "summary must not retain {key}");
    }
    assert_borderless(&runtime, "project-recipe");
    assert_borderless(&runtime, "project-agent");
    click_control(&mut runtime, "quick-save-details");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("Source program (provenance)"));
    for (width, height) in [(280, 180), (640, 360), (1000, 600)] {
        runtime.set_viewport(harbor_widget::renderer::Viewport::new(width, height, 1.0));
        runtime.update(Instant::now());
        let header = keyed_rect(&runtime, "review-header");
        let footer = keyed_rect(&runtime, "review-footer");
        let body = keyed_rect(&runtime, "review-body");
        assert!(header.max.y <= body.min.y && body.max.y <= footer.min.y);
        assert!(
            body.size().height > 0.0,
            "body must remain reachable: {body:?}"
        );
        assert!(header.min.y >= 12.0 && footer.max.y <= height as f32 - 12.0);
        focus_control(&mut runtime, "review-args");
        runtime.update(Instant::now());
        let args = keyed_rect(&runtime, "review-args");
        assert!(
            args.min.y >= body.min.y && args.min.y < body.max.y,
            "keyboard traversal reveals body values: {args:?} in {body:?}"
        );
        assert_eq!(keyed_rect(&runtime, "review-header"), header);
        assert_eq!(keyed_rect(&runtime, "review-footer"), footer);
        focus_control(&mut runtime, "project-agent-ack");
        runtime.update(Instant::now());
        assert_eq!(keyed_rect(&runtime, "review-header"), header);
        assert_eq!(keyed_rect(&runtime, "review-footer"), footer);
    }
    assert!(ui.drain_actions().is_empty());
}

#[test]
fn quick_save_grouped_feedback_stays_with_context_agent_overrides_and_actions() {
    let (_, ui, mut runtime, bytes) = fixture();
    let mut source = draft();
    source.launch.args = vec!["--token".into(), "synthetic-secret".into()];
    source.project.as_mut().unwrap().agent = Some(harbor_config::AgentInvocation {
        program: "pi".into(),
        args: vec!["--resume".into(), "synthetic-session".into()],
    });
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Nearby feedback");
    advance(&ui, &mut runtime);
    click_control(&mut runtime, "environment-0");
    runtime.update(Instant::now());
    assert!(
        keyed_rect(&runtime, "project-error").min.y > keyed_rect(&runtime, "launch-heading").max.y
    );
    assert!(
        keyed_rect(&runtime, "project-error").max.y
            < keyed_rect(&runtime, "overrides-heading").min.y
    );
    assert!(
        keyed_rect(&runtime, "override-error").min.y
            > keyed_rect(&runtime, "overrides-heading").max.y
    );
    assert!(
        keyed_rect(&runtime, "override-error").max.y
            < keyed_rect(&runtime, "quick-save-details").min.y
    );
    assert!(
        keyed_rect(&runtime, "source-secret-warning").max.y
            < keyed_rect(&runtime, "quick-save-details").min.y
    );
    assert!(!review_text(&runtime).contains("synthetic-secret"));
    edit_location(&mut runtime);
    click_control(&mut runtime, "wsl");
    runtime.update(Instant::now());
    assert!(
        keyed_rect(&runtime, "project-error").max.y < keyed_rect(&runtime, "launch-heading").min.y,
        "context error must not be misplaced under the retained agent"
    );
    assert!(ui.drain_actions().is_empty());
    // Exercise the real failure presentation after passing the independent gates.
    click_control(&mut runtime, "pwsh");
    runtime.update(Instant::now());
    click_control(&mut runtime, "project-agent-ack");
    runtime.update(Instant::now());
    click_control(&mut runtime, "quick-save-ack");
    runtime.update(Instant::now());
    click_control(&mut runtime, "quick-save-final");
    let request = take_one(&ui);
    assert!(ui.take_quick_save_submission(request).is_some());
    let generation = ui.store.state().read().popups.generation().unwrap();
    ui.finish_quick_save(
        generation,
        Err("Synthetic write conflict; draft retained".into()),
    );
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("Synthetic write conflict"));
    assert!(
        keyed_rect(&runtime, "save-error").max.y <= keyed_rect(&runtime, "actions-divider").min.y
    );
    assert!(
        keyed_rect(&runtime, "save-error").max.y < keyed_rect(&runtime, "quick-save-final").min.y
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_design_roles_separate_checks_actions_and_field_groups() {
    let (_, ui, mut runtime, _) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Design roles");
    advance(&ui, &mut runtime);
    edit_location(&mut runtime);
    // Use a tall viewport so geometry assertions do not depend on scroll position.
    runtime.set_viewport(harbor_widget::renderer::Viewport::new(1000, 1000, 1.0));
    runtime.update(Instant::now());
    let fill = |runtime: &Runtime, key: &str| {
        let rect = keyed_rect(runtime, key);
        let delta = runtime.pending_delta().unwrap();
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match item.primitive {
                Primitive::Quad {
                    rect: surface,
                    color,
                    ..
                } if surface == rect => Some(color),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing surface {key}"))
    };
    assert_eq!(fill(&runtime, "quick-save-final").a, 1.0);
    let theme = crate::appearance::design_system::theme(&harbor_widget::Theme::default());
    assert_eq!(
        fill(&runtime, "quick-save-final"),
        theme.button.normal.background
    );
    assert_eq!(fill(&runtime, "pwsh"), theme.button.selected.background);
    assert!(fill(&runtime, "pwsh").a < 1.0);
    let selected_rect = keyed_rect(&runtime, "pwsh");
    let delta = runtime.pending_delta().unwrap();
    let materials: Vec<_> = delta
        .added
        .iter()
        .chain(&delta.modified)
        .filter_map(|item| match item.primitive {
            Primitive::BackdropMaterial {
                rect,
                refraction,
                material,
                ..
            } if rect == selected_rect => Some((refraction, material)),
            _ => None,
        })
        .collect();
    assert_eq!(
        materials.len(),
        1,
        "selection itself must be real liquid glass"
    );
    assert!(materials[0].0.displacement() > 0.0);
    assert!(materials[0].1.rim()[0] > 0.0);
    assert_borderless(&runtime, "pwsh");
    assert!(fill(&runtime, "quick-save-back").a < 0.2);
    assert_eq!(fill(&runtime, "quick-save-cancel").a, 0.0);
    assert_eq!(fill(&runtime, "environment-0").a, 0.0);
    assert_borderless(&runtime, "environment-0");
    assert_borderless(&runtime, "quick-save-details");

    // Checkbox selection must not turn its entire row into another primary action.
    click_control(&mut runtime, "environment-0");
    runtime.update(Instant::now());
    // Move off the hovered row to inspect selected rather than hover feedback.
    row_pointer(&mut runtime, Point::new(5.0, 5.0), PointerPhase::Move);
    runtime.update(Instant::now());
    assert_eq!(fill(&runtime, "environment-0").a, 0.0);
    assert!(
        fill(&runtime, "quick-save-final").a < 1.0,
        "independent acknowledgement still gates Save"
    );
    click_control(&mut runtime, "quick-save-ack");
    runtime.update(Instant::now());
    assert_eq!(fill(&runtime, "quick-save-final").a, 1.0);

    let label = |text: &str| {
        let delta = runtime.pending_delta().unwrap();
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| item.primitive.text_run())
            .find_map(|(value, origin, _, scale)| (value == text).then_some((origin, scale)))
            .unwrap_or_else(|| panic!("missing label {text}"))
    };
    let (environment_label, label_scale) = label("Execution environment");
    let (directory_label, _) = label("Project directory");
    let environment = keyed_rect(&runtime, "project-environment");
    let directory = keyed_rect(&runtime, "project-directory");
    assert!(environment.min.y > environment_label.y);
    assert!(directory.min.y > directory_label.y);
    assert!(
        directory_label.y - environment.max.y > 6.0,
        "related label/control should be grouped, not evenly spaced with other fields"
    );
    let (_, heading_scale) = label("Launch location");
    assert!(heading_scale > label_scale);

    // Naming advance is a normal keyboard-focusable control in the same style family.
    ui.cancel_quick_save(popup(&ui, AppCommand::CancelQuickSave));
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Keyboard naming");
    focus_control(&mut runtime, "save-advance");
    tap(&mut runtime, Key::Enter);
    assert_eq!(take_one(&ui).command, AppCommand::AdvanceQuickSave);
    assert!(ui.drain_actions().is_empty());
}

#[test]
fn quick_save_pending_context_has_one_gate_explanation() {
    let (_, ui, mut runtime, _) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "One correction warning");
    advance(&ui, &mut runtime);
    edit_location(&mut runtime);
    click_control(&mut runtime, "wsl");
    runtime.update(Instant::now());
    assert_eq!(
        review_text(&runtime)
            .matches("Unknown or edited context:")
            .count(),
        1,
        "do not repeat the same gate beside the form"
    );
    assert!(ui.drain_actions().is_empty());
}

#[test]
fn quick_save_directory_editor_identity_survives_conditional_target_fields() {
    let (_, ui, mut runtime, _) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Stable field groups");
    advance(&ui, &mut runtime);
    edit_location(&mut runtime);
    let directory_id = |runtime: &Runtime| {
        fiber_ids(runtime)
            .into_iter()
            .find(|id| {
                runtime
                    .arena()
                    .get(*id)
                    .unwrap()
                    .key()
                    .is_some_and(|key| key.text() == "project-directory")
            })
            .unwrap()
    };
    let original = directory_id(&runtime);
    for environment in ["wsl", "ssh", "pwsh"] {
        click_control(&mut runtime, environment);
        runtime.update(Instant::now());
        assert_eq!(
            directory_id(&runtime),
            original,
            "adding/removing a target field must not retire the directory editor"
        );
        assert!(review_text(&runtime).contains("C:/synthetic-project"));
    }
    assert!(ui.drain_actions().is_empty());
}

#[test]
fn quick_save_naming_is_stable_and_uses_shared_notifications_and_search_icon_feedback() {
    let (_, ui, mut runtime, bytes) = fixture();
    start(&ui, &mut runtime, draft());
    let naming = panel_rect(&runtime);
    let editor_focus = runtime.input().focused();
    assert!(ui.store.state().read().notices.message().is_none());
    assert!(!painted(&runtime).contains("Enter a preset name"));
    assert!(!painted(&runtime).contains("Save…"));
    let button = keyed_rect(&runtime, "save-advance");
    let has_button_material = |runtime: &Runtime| {
        let delta = runtime.pending_delta().unwrap();
        delta.added.iter().chain(&delta.modified).any(|item| {
            matches!(item.primitive, Primitive::BackdropMaterial { rect, .. } if rect == button)
        })
    };
    assert!(!has_button_material(&runtime));
    let delta = runtime.pending_delta().unwrap();
    assert!(delta.added.iter().chain(&delta.modified).any(|item| {
        matches!(item.primitive, Primitive::Icon { icon: LucideIcon::Check, rect, .. }
            if button.contains(rect.min) && button.contains(Point::new(rect.max.x - 0.1, rect.max.y - 0.1)))
    }));
    let center = Point::new(
        (button.min.x + button.max.x) / 2.0,
        (button.min.y + button.max.y) / 2.0,
    );
    row_pointer(&mut runtime, center, PointerPhase::Move);
    runtime.update(Instant::now());
    assert!(
        has_button_material(&runtime),
        "reuse search hover glass over the whole button"
    );
    assert_eq!(panel_rect(&runtime), naming);
    row_pointer(&mut runtime, Point::ZERO, PointerPhase::Move);
    runtime.update(Instant::now());
    assert!(!has_button_material(&runtime));

    // Validation is explicit, not a warning shown while merely typing or opening.
    replace(&mut runtime, "  ");
    assert!(ui.store.state().read().notices.message().is_none());
    assert_eq!(panel_rect(&runtime), naming);
    tap(&mut runtime, Key::Enter);
    assert!(ui.navigate_quick_save(take_one(&ui)).is_none());
    runtime.update(Instant::now());
    assert_eq!(
        runtime.input().focused(),
        editor_focus,
        "notification must not take focus"
    );
    assert_eq!(
        panel_rect(&runtime),
        naming,
        "validation cannot resize naming"
    );
    assert_eq!(ui.store.state().read().notices.title(), "Save preset");
    assert!(review_text(&runtime).contains("Enter a preset name"));
    let revision = ui.store.state().read().notices.revision;
    let notice = keyed_rect(&runtime, &format!("configuration-notification-{revision}"));
    // The root-escape wrapper can fill the window; inspect the actual notification material.
    let delta = runtime.pending_delta().unwrap();
    assert!(
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .any(|item| matches!(
                item.primitive, Primitive::BackdropMaterial { rect, .. }
                    if rect.min.y > naming.max.y && notice.contains(rect.min)
            ))
    );
    ui.advance_notifications(Instant::now() + std::time::Duration::from_secs(4));
    runtime.update(Instant::now());
    assert!(ui.store.state().read().notices.message().is_none());
    assert_eq!(panel_rect(&runtime), naming);

    // A new editing episode may report the same error; unrelated notices remain owned separately.
    replace(&mut runtime, "   ");
    ui.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("Unrelated reload".into()),
    );
    click_control(&mut runtime, "save-advance");
    assert!(ui.navigate_quick_save(take_one(&ui)).is_none());
    runtime.update(Instant::now());
    assert!(
        ui.store
            .state()
            .read()
            .notices
            .message()
            .unwrap()
            .contains("Enter a preset name")
    );
    let stale = popup(&ui, AppCommand::AdvanceQuickSave);
    assert!(ui.cancel_quick_save(popup(&ui, AppCommand::CancelQuickSave)));
    assert_eq!(
        ui.store.state().read().notices.message().as_deref(),
        Some("Unrelated reload")
    );
    assert!(ui.navigate_quick_save(stale).is_none());
    ui.publish_configuration_notice(ConfigurationNoticeSource::Reload, None);
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Valid name");
    assert_eq!(panel_rect(&runtime), naming);
    focus_control(&mut runtime, "save-advance");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&ui);
    assert_eq!(request.command, AppCommand::AdvanceQuickSave);
    assert!(ui.navigate_quick_save(request).is_some());
    assert!(
        ui.drain_actions().is_empty(),
        "the checkmark never commits a preset"
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_summary_correction_and_local_name_edit_preserve_the_draft() {
    let (_, ui, mut runtime, bytes) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Summary");
    advance(&ui, &mut runtime);
    assert!(review_text(&runtime).contains("Local · PowerShell"));
    assert!(!has_control(&runtime, "project-directory"));
    assert!(!review_text(&runtime).contains("Approved relaunch context"));
    click_control(&mut runtime, "environment-0");
    runtime.update(Instant::now());
    click_control(&mut runtime, "quick-save-ack");
    runtime.update(Instant::now());
    edit_location(&mut runtime);
    assert!(!has_control(&runtime, "project-recipe"));
    focus_control(&mut runtime, "project-directory");
    replace(&mut runtime, "relative");
    click_control(&mut runtime, "project-apply");
    runtime.update(Instant::now());
    assert!(has_control(&runtime, "context-error"));
    assert!(has_control(&runtime, "project-directory"));
    focus_control(&mut runtime, "project-directory");
    replace(&mut runtime, "C:/updated 项目");
    // Returning to naming cannot approve pending location edits.
    click_control(&mut runtime, "quick-save-back");
    runtime.request_focus(&ui.navigate_quick_save(take_one(&ui)).unwrap());
    runtime.update(Instant::now());
    replace(&mut runtime, "Renamed");
    advance(&ui, &mut runtime);
    assert!(has_control(&runtime, "project-directory"));
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
    assert!(ui.drain_actions().is_empty());
    click_control(&mut runtime, "project-apply");
    runtime.update(Instant::now());
    assert!(!has_control(&runtime, "project-directory"));
    assert!(!has_control(&runtime, "project-apply"));
    assert!(review_text(&runtime).contains("C:/updated 项目"));
    assert!(
        !has_control(&runtime, "override-error"),
        "correction must not reset override approval"
    );
    click_control(&mut runtime, "quick-save-final");
    let saved = ui.take_quick_save_submission(take_one(&ui)).unwrap();
    assert_eq!(saved.profile.name, "Renamed");
    assert_eq!(saved.profile.project.unwrap().directory, "C:/updated 项目");
    assert_eq!(saved.profile.launch.environment.len(), 1);
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn quick_save_unusable_recognition_opens_correction_without_hiding_agent_safety() {
    let (_, ui, mut runtime, _) = fixture();
    let mut source = draft();
    source.project.as_mut().unwrap().directory = "relative".into();
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Invalid recognition");
    advance(&ui, &mut runtime);
    assert!(has_control(&runtime, "project-directory"));
    assert!(!has_control(&runtime, "project-recipe"));
    assert!(has_control(&runtime, "project-error"));
    assert!(
        keyed_rect(&runtime, "project-error").max.y < keyed_rect(&runtime, "launch-heading").min.y
    );
    assert!(ui.drain_actions().is_empty());
}

#[test]
fn quick_save_small_window_write_error_does_not_starve_the_body() {
    let (_, ui, mut runtime, _) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Retry");
    advance(&ui, &mut runtime);
    click_control(&mut runtime, "quick-save-final");
    let submission = ui.take_quick_save_submission(take_one(&ui)).unwrap();
    ui.finish_quick_save(
        submission.generation,
        Err("Synthetic write conflict. ".repeat(12)),
    );
    runtime.set_viewport(harbor_widget::renderer::Viewport::new(280, 180, 1.0));
    runtime.update(Instant::now());
    let body = keyed_rect(&runtime, "review-body");
    let footer = keyed_rect(&runtime, "review-footer");
    assert!(
        body.size().height > 0.0,
        "failure must not starve the body: {body:?}"
    );
    assert!(
        footer.max.y <= 168.0,
        "footer must stay in the viewport: {footer:?}"
    );
    let error = keyed_rect(&runtime, "save-error");
    assert!(
        error.max.y > body.min.y && error.max.y <= body.max.y,
        "failure should be revealed beside the footer: {error:?} in {body:?}"
    );
    focus_control(&mut runtime, "environment-0");
    let entry = keyed_rect(&runtime, "environment-0");
    assert!(entry.min.y >= body.min.y && entry.min.y < body.max.y);
}

#[test]
fn quick_save_correction_retires_clipboard_and_keeps_agent_approval_independent() {
    let (_, ui, mut runtime, bytes) = fixture();
    let mut source = draft();
    source.project.as_mut().unwrap().agent = Some(harbor_config::AgentInvocation {
        program: "pi".into(),
        args: vec!["--resume".into()],
    });
    start(&ui, &mut runtime, source);
    replace(&mut runtime, "Safe correction");
    advance(&ui, &mut runtime);
    click_control(&mut runtime, "project-agent-ack");
    runtime.update(Instant::now());
    edit_location(&mut runtime);
    assert!(
        !has_control(&runtime, "project-error"),
        "opening the editor does not clear agent approval"
    );
    focus_control(&mut runtime, "project-directory");
    replace(&mut runtime, "C:/approved");
    let effects = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    up(&mut runtime, Key::Character('v'), ctrl());
    let Some(ClipboardEffect::Editor(paste)) = effects.clipboard else {
        panic!("paste request");
    };
    click_control(&mut runtime, "project-apply");
    runtime.update(Instant::now());
    assert!(!has_control(&runtime, "project-directory"));
    assert!(review_text(&runtime).contains("Review agent arguments"));
    runtime.complete_clipboard(paste.id, Ok("C:/late-clipboard".into()));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("late IME".into())));
    runtime.update(Instant::now());
    assert!(!review_text(&runtime).contains("late"));
    edit_location(&mut runtime);
    assert!(review_text(&runtime).contains("C:/approved"));
    click_control(&mut runtime, "project-apply");
    runtime.update(Instant::now());
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
    assert!(
        ui.drain_actions().is_empty(),
        "correction is not argument approval"
    );
    click_control(&mut runtime, "project-agent-ack");
    runtime.update(Instant::now());
    click_control(&mut runtime, "quick-save-final");
    let saved = ui.take_quick_save_submission(take_one(&ui)).unwrap();
    assert_eq!(saved.profile.project.unwrap().directory, "C:/approved");
    assert!(bytes.lock().unwrap().is_empty());
}
