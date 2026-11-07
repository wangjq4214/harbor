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
    assert!(
        keyed_rect(&runtime, "project-apply").size().width
            < keyed_rect(&runtime, "project-directory").size().width,
        "context correction is a content-sized secondary action"
    );
    let text = painted(&runtime);
    for expected in [
        "Save this preset?",
        "Name: My preset",
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
fn quick_save_review_uses_palette_glass_and_translucent_inputs_without_a_scrim() {
    let (_, ui, mut runtime, _) = fixture();
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Transparent review");
    advance(&ui, &mut runtime);
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
    assert!(cancel.max.x <= back.min.x && back.max.x <= save.min.x);
    assert_eq!(cancel.min.y, save.min.y);
    assert!((save.max.x - divider.max.x).abs() < 1.0);

    focus_control(&mut runtime, "quick-save-final");
    let focused = runtime.input().focused();
    runtime.set_viewport(harbor_widget::renderer::Viewport::new(230, 600, 1.0));
    runtime.update(Instant::now());
    let cancel = keyed_rect(&runtime, "quick-save-cancel");
    let back = keyed_rect(&runtime, "quick-save-back");
    let save = keyed_rect(&runtime, "quick-save-final");
    let divider = keyed_rect(&runtime, "actions-divider");
    assert!(
        save.min.y > cancel.min.y,
        "actions should wrap: {cancel:?} {back:?} {save:?}"
    );
    for rect in [cancel, back, save] {
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
    assert!(painted(&runtime).contains("Show supporting details"));
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
    assert!(review_text(&runtime).contains("apply the environment/directory correction first"));
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
fn quick_save_review_has_ordered_blocks_larger_headings_and_restrained_dividers() {
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
    let title = heading_scale(&runtime, "Save this preset?");
    let section = heading_scale(&runtime, "Project and execution environment");
    let field = heading_scale(&runtime, "Execution environment");
    assert!(title > section && section > field);
    for title in ["Agent", "Environment variables", "Supporting details"] {
        assert_eq!(heading_scale(&runtime, title), section);
    }
    let keys = [
        "project-divider",
        "agent-divider",
        "overrides-divider",
        "details-divider",
        "actions-divider",
    ];
    let dividers: Vec<_> = keys.iter().map(|key| keyed_rect(&runtime, key)).collect();
    for pair in dividers.windows(2) {
        assert!(pair[0].max.y < pair[1].min.y);
        assert_eq!(pair[0].size().width, pair[1].size().width);
    }
    assert!(dividers.iter().all(|rect| rect.size().height <= 1.0));
    let name = keyed_rect(&runtime, "review-name");
    let first_field = keyed_rect(&runtime, "project-environment");
    assert!(dividers[0].min.y - name.max.y > 6.0);
    assert!(first_field.min.y > dividers[0].max.y);
    assert_borderless(&runtime, "review-name");
    assert_borderless(&runtime, "project-agent");
    click_control(&mut runtime, "quick-save-details");
    runtime.update(Instant::now());
    assert!(review_text(&runtime).contains("Source program (provenance)"));
    assert!(
        keyed_rect(&runtime, "review-program").min.y
            > keyed_rect(&runtime, "details-divider").max.y
    );
    assert!(
        keyed_rect(&runtime, "review-directory").max.y
            < keyed_rect(&runtime, "actions-divider").min.y
    );
    click_control(&mut runtime, "project-agent-ack");
    runtime.update(Instant::now());
    for (width, height) in [(280, 180), (640, 360), (1000, 600)] {
        runtime.set_viewport(harbor_widget::renderer::Viewport::new(width, height, 1.0));
        runtime.update(Instant::now());
        focus_control(&mut runtime, "quick-save-final");
        let save = keyed_rect(&runtime, "quick-save-final");
        let panel = panel_rect(&runtime);
        assert!(
            save.min.y >= panel.min.y && save.max.y <= panel.max.y,
            "final action must stay reachable after scrolling: {save:?} in {panel:?}"
        );
    }
    assert!(ui.drain_actions().is_empty());

    // A draft with no recognized agent does not acquire an empty Agent block.
    ui.cancel_quick_save(popup(&ui, AppCommand::CancelQuickSave));
    start(&ui, &mut runtime, draft());
    replace(&mut runtime, "Shell only");
    advance(&ui, &mut runtime);
    assert!(!fiber_ids(&runtime).iter().any(|id| {
        runtime
            .arena()
            .get(*id)
            .unwrap()
            .key()
            .is_some_and(|key| key.text() == "agent-divider")
    }));
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
        keyed_rect(&runtime, "project-error").min.y > keyed_rect(&runtime, "agent-divider").max.y
    );
    assert!(
        keyed_rect(&runtime, "project-error").max.y
            < keyed_rect(&runtime, "overrides-divider").min.y
    );
    assert!(
        keyed_rect(&runtime, "override-error").min.y
            > keyed_rect(&runtime, "overrides-divider").max.y
    );
    assert!(
        keyed_rect(&runtime, "override-error").max.y
            < keyed_rect(&runtime, "details-divider").min.y
    );
    assert!(
        keyed_rect(&runtime, "source-secret-warning").min.y
            > keyed_rect(&runtime, "details-divider").max.y
    );
    assert!(!review_text(&runtime).contains("synthetic-secret"));
    click_control(&mut runtime, "wsl");
    runtime.update(Instant::now());
    assert!(
        keyed_rect(&runtime, "project-error").max.y < keyed_rect(&runtime, "agent-divider").min.y,
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
        keyed_rect(&runtime, "save-error").min.y > keyed_rect(&runtime, "actions-divider").max.y
    );
    assert!(
        keyed_rect(&runtime, "save-error").max.y < keyed_rect(&runtime, "quick-save-final").min.y
    );
    assert!(bytes.lock().unwrap().is_empty());
}
