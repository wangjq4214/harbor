//! Mounted quick-save flow: real editor, modal routing, one-shot Host admission.
use super::palette_support::*;
use super::*;
use crate::{QuickSaveDraft, command::CommandSource};
use harbor_config::{ProfileId, SessionProfile, ShellSettings};

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

fn panel_rect(runtime: &Runtime) -> Rect {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::BackdropMaterial { rect, .. } => Some(rect),
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
    assert!(naming.size().width <= 480.0);
    assert!(!painted(&runtime).contains("Save current session as preset"));
    assert_borderless(&runtime, "save-error");

    replace(&mut runtime, "My preset");
    advance(&ui, &mut runtime);
    let review = panel_rect(&runtime);
    assert!(
        review.size().height < 520.0,
        "review should fit its content: {review:?}"
    );
    let text = painted(&runtime);
    for expected in [
        "Save this preset?",
        "Name: My preset",
        "Source program (provenance): synthetic-source.exe",
        "Source arguments (not replayed): None",
        r"Directory: D:\Code\harbor",
        "No environment overrides will be saved.",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text}");
    }
    assert!(!text.contains("Some("));
    for key in [
        "review-name",
        "review-program",
        "review-args",
        "review-directory",
    ] {
        assert_borderless(&runtime, key);
    }

    // A constrained window must still scroll to actions rather than clip them away.
    runtime.set_viewport(harbor_widget::renderer::Viewport::new(400, 180, 1.0));
    runtime.update(Instant::now());
    focus_control(&mut runtime, "quick-save-final");
    runtime.update(Instant::now());
    let save = keyed_rect(&runtime, "quick-save-final");
    assert!(
        save.min.y >= 12.0 && save.max.y <= 168.0,
        "Save must scroll into view: {save:?}"
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
    assert!(painted(&runtime).contains("synthetic-source.exe"));
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
    for name in ["", "  ", "Duplicate"] {
        replace(&mut runtime, name);
        tap(&mut runtime, Key::Enter);
        for request in ui.drain_actions() {
            assert!(ui.navigate_quick_save(request).is_none());
        }
        assert!(ui.quick_save_is_open());
        let button = keyed_rect(&runtime, "save-advance");
        click(&mut runtime, button);
        assert!(ui.drain_actions().is_empty());
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
    let one = keyed_rect(&runtime, "environment-0");
    click(&mut runtime, one);
    runtime.update(Instant::now());
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
    assert!(
        ui.drain_actions().is_empty(),
        "selection alone does not authorize secrets"
    );
    let ack = keyed_rect(&runtime, "quick-save-ack");
    click(&mut runtime, ack);
    runtime.update(Instant::now());
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
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
    let save = keyed_rect(&runtime, "quick-save-final");
    click(&mut runtime, save);
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
