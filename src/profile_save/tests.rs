use super::*;
fn profile() -> SessionProfile {
    SessionProfile {
        project: None,
        name: "Synthetic".into(),
        launch: ShellSettings {
            program: Some("cmd.exe".into()),
            args: vec!["".into(), "space 世界".into()],
            directory: Some(std::env::current_dir().unwrap()),
            ..Default::default()
        },
    }
}
#[test]
fn selected_file_is_durable_comments_survive_and_retry_cannot_duplicate() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    let original = "# original comment\n[shell]\nprogram='cmd.exe' # keep\n";
    fs::write(&path, original).unwrap();
    let mut transaction = SaveTransaction::begin(path.clone()).unwrap();
    let loaded = transaction.commit(&profile()).unwrap();
    assert_eq!(loaded.settings.profiles.len(), 1);
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with(original));
    let guard = pin_generation(&path, saved.as_bytes()).unwrap();
    assert!(fs::write(&path, "# may not race publication").is_err());
    assert!(fs::remove_file(&path).is_err());
    drop(guard);
    assert_eq!(
        transaction
            .commit(&profile())
            .unwrap()
            .settings
            .profiles
            .len(),
        1
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
#[test]
fn late_edit_invalid_file_collision_and_faults_do_not_overwrite() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    fs::write(&path, "# initial\n").unwrap();
    let mut transaction = SaveTransaction::begin(path.clone()).unwrap();
    let error = transaction
        .commit_with(&profile(), |path| {
            fs::write(path, "# external late edit\n").unwrap();
            Ok(())
        })
        .unwrap_err();
    assert!(error.contains("changed"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "# external late edit\n");
    transaction.refresh().unwrap();
    assert!(
        transaction
            .commit_with(&profile(), |_| Err("injected staged-write failure".into()))
            .is_err()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "# external late edit\n");
    fs::write(
        &path,
        "[profiles.other]\nname='Synthetic'\nprogram='cmd.exe'\n",
    )
    .unwrap();
    transaction.refresh().unwrap();
    assert!(
        transaction
            .commit(&profile())
            .unwrap_err()
            .contains("already exists")
    );
    fs::write(&path, "invalid [toml").unwrap();
    transaction.refresh().unwrap();
    assert!(transaction.commit(&profile()).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "invalid [toml");
}
#[test]
fn missing_selected_file_is_create_only_and_never_falls_back() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("portable.toml");
    let mut transaction = SaveTransaction::begin(path.clone()).unwrap();
    assert!(
        transaction
            .commit_with(&profile(), |path| {
                fs::write(path, "# concurrent creator").unwrap();
                Ok(())
            })
            .is_err()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "# concurrent creator");
    let mut transaction = SaveTransaction::begin(root.path().join("absent/config.toml")).unwrap();
    assert!(transaction.commit(&profile()).is_err());
    assert!(!root.path().join("absent").exists());
}

#[test]
fn replacement_fault_restores_original_and_late_creator_preserves_recovery() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    let original = b"# original";
    fs::write(&path, original).unwrap();
    let staged = tempfile::NamedTempFile::new_in(root.path()).unwrap();
    assert!(replace_selected_with(&path, Some(original), staged, |_, _| Err(())).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    let staged = tempfile::NamedTempFile::new_in(root.path()).unwrap();
    let error = replace_selected_with(&path, Some(original), staged, |_, path| {
        fs::write(path, b"# late creator").unwrap();
        Err(())
    })
    .unwrap_err();
    assert!(error.contains("Original retained"));
    assert_eq!(fs::read(&path).unwrap(), b"# late creator");
    let recovery = fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".harbor-save-recovery-")
        })
        .unwrap();
    assert_eq!(fs::read(recovery).unwrap(), original);
}

#[test]
fn real_flow_to_writer_to_accepted_catalog_retains_disk_only_failure_and_retries_once() {
    use harbor_app::{
        command::{AppCommand, AppCommandRequest},
        tab_manager::TerminalTabResources,
        terminal_view::TerminalWidgetBridge,
        workspace::{WorkspaceUiController, tab_workspace},
    };
    use harbor_widget::{
        ComponentExt as _,
        input::event::{Key, KeyboardEvent, Modifiers, UiEvent},
        renderer::Viewport,
        runtime::Runtime,
        scene::primitive::Primitive,
    };
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use std::time::Instant;
    let mut tabs = TabManager::new();
    #[allow(clippy::arc_with_non_send_sync)]
    tabs.create_tab(|_, draw| {
        let terminal = Arc::new(Mutex::new(harbor_terminal::Terminal::new_headless(4, 20)));
        let bridge =
            TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
        Ok(TerminalTabResources::new(terminal, bridge, "synthetic"))
    })
    .unwrap();
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(1000, 700, 1.0));
    runtime.set_root(tab_workspace(ui.clone(), false).keyed("save-integration"));
    runtime.update(Instant::now());
    let source = tabs.active_id().unwrap();
    let provenance = harbor_pty::LaunchProvenance {
        program: "cmd.exe".into(),
        args: vec!["".into(), "source arg".into()],
        initial_directory: std::env::current_dir().unwrap(),
        configured_environment: [("SYNTHETIC".into(), "not-in-file".into())].into(),
    };
    let unknown = capture_recipe(source, &provenance, None).unwrap();
    assert!(unknown.launch.directory.is_none());
    assert!(unknown.project.is_none());
    let directory = tempfile::tempdir().unwrap();
    let native = directory.path().to_string_lossy().replace('\\', "/");
    let native = native.strip_prefix("//?/").unwrap_or(&native);
    let mut metadata_source = harbor_terminal::Terminal::new_headless(4, 20);
    metadata_source.process_output(b"\x1b]7;file://remote.invalid/C:/unrelated\x07");
    metadata_source.drain_output_events();
    assert!(
        metadata_source.project_snapshot().is_none(),
        "OSC 7 is not project capture evidence"
    );
    let hex = |s: &str| {
        s.as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    metadata_source.process_output(
        format!(
            "\x1b]777;harbor;1;context;{};;{}\x07",
            hex("pwsh"),
            hex(native)
        )
        .as_bytes(),
    );
    metadata_source.drain_output_events();
    let captured = capture_recipe(source, &provenance, metadata_source.project_snapshot()).unwrap();
    let osc_capture = captured.clone();
    metadata_source.process_output(b"\x1b]777;harbor;1;busy\x07");
    metadata_source.drain_output_events();
    assert!(metadata_source.project_snapshot().is_none());
    // Later metadata and recipe changes cannot rewrite the already-owned draft.
    let mut later = provenance.clone();
    later.args.push("later profile edit".into());
    later
        .configured_environment
        .insert("LATER".into(), "excluded".into());
    assert_ne!(osc_capture.launch.args, later.args);
    assert!(!osc_capture.launch.environment.contains_key("LATER"));
    assert_eq!(osc_capture.project.as_ref().unwrap().directory, native);
    assert!(!format!("{provenance:?}").contains("not-in-file"));
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("portable.toml");
    fs::write(&path, "# preserved\n").unwrap();
    let mut transaction = SaveTransaction::begin(path.clone()).unwrap();
    runtime.request_focus(&ui.open_quick_save(captured));
    runtime.update(Instant::now());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("Flow preset".into())));
    let tap = |runtime: &mut Runtime, key| {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
        }));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key,
            modifiers: Modifiers::default(),
        }));
    };
    tap(&mut runtime, Key::Enter);
    let advance = ui.drain_actions().pop().unwrap();
    assert_eq!(advance.command, AppCommand::AdvanceQuickSave);
    assert_eq!(fs::read_to_string(&path).unwrap(), "# preserved\n");
    runtime.request_focus(&ui.navigate_quick_save(advance).unwrap());
    runtime.update(Instant::now());
    let click_save = |runtime: &mut Runtime| {
        use harbor_widget::input::event::{PointerButton, PointerEvent, PointerPhase};
        let mut visible = None;
        for _ in 0..64 {
            let mut todo = vec![runtime.root_id().unwrap()];
            let rect = loop {
                let id = todo.pop().expect("final save button");
                let fiber = runtime.arena().get(id).unwrap();
                if fiber
                    .key()
                    .is_some_and(|key| key.text() == "quick-save-final")
                {
                    break fiber.layout_rect().unwrap();
                }
                todo.extend(fiber.children().iter().copied());
            };
            if rect.min.y >= 12.0 && rect.max.y <= 688.0 {
                visible = Some(rect);
                break;
            }
            // Keyboard traversal reveals actions inside the scrollable review.
            tap(runtime, Key::Tab);
            runtime.update(Instant::now());
        }
        let rect = visible.expect("final Save must scroll into the viewport");
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent {
                pointer_id: 0,
                phase,
                button: PointerButton::Left,
                position: harbor_widget::layout::Point::new(
                    (rect.min.x + rect.max.x) / 2.0,
                    (rect.min.y + rect.max.y) / 2.0,
                ),
                modifiers: Modifiers::default(),
            }));
        }
    };
    click_save(&mut runtime);
    let request = ui.drain_actions().pop().unwrap();
    let submission = ui.take_quick_save_submission(request).unwrap();
    let loaded = transaction.commit(&submission.profile).unwrap();
    let mut accepted = harbor_config::Settings::default();
    let effective = harbor_config::ClipboardSettings::default();
    assert!(
        crate::shell::accept_configuration(
            &mut accepted,
            &effective,
            loaded.settings,
            |_, _| anyhow::bail!("synthetic publication failure")
        )
        .is_err()
    );
    assert!(accepted.profiles.is_empty());
    runtime.request_focus(
        &ui.finish_quick_save(
            submission.generation,
            Err("Saved on disk; retry publication".into()),
        )
        .unwrap(),
    );
    runtime.update(Instant::now());
    assert!(ui.quick_save_is_open());
    click_save(&mut runtime);
    let retry = ui.drain_actions().pop().unwrap();
    let submission = ui.take_quick_save_submission(retry).unwrap();
    assert!(ui.take_quick_save_submission(retry).is_none());
    let loaded = transaction.commit(&submission.profile).unwrap();
    crate::shell::accept_configuration(
        &mut accepted,
        &effective,
        loaded.settings,
        |settings, bindings| {
            ui.publish_profiles(settings.profiles.clone());
            ui.publish_keybindings(bindings);
            Ok(Vec::new())
        },
    )
    .unwrap();
    assert_eq!(accepted.profiles.len(), 1);
    assert!(
        accepted
            .profiles
            .values()
            .next()
            .unwrap()
            .launch
            .environment
            .is_empty()
    );
    assert!(
        accepted
            .profiles
            .values()
            .next()
            .unwrap()
            .launch
            .args
            .is_empty()
    );
    assert_eq!(
        accepted
            .profiles
            .values()
            .next()
            .unwrap()
            .project
            .as_ref()
            .unwrap()
            .directory,
        native
    );
    let intent = harbor_app::profiles::ProfileLaunchIntent {
        profile: accepted.profiles.keys().next().unwrap().clone(),
        source: None,
        directory: None,
    };
    let mut launches = 0;
    crate::profile_launch::create_tab(
        &mut tabs,
        &accepted.profiles,
        &intent,
        |_, draw, command| {
            launches += 1;
            assert_eq!(command.program.as_deref(), Some("pwsh"));
            assert_eq!(
                command
                    .directory
                    .as_ref()
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                native
            );
            #[allow(clippy::arc_with_non_send_sync)]
            let terminal = Arc::new(Mutex::new(harbor_terminal::Terminal::new_headless(4, 20)));
            let bridge =
                TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
            Ok(TerminalTabResources::new(terminal, bridge, "project"))
        },
    )
    .unwrap();
    assert_eq!(launches, 1);
    let removed = std::collections::BTreeMap::new();
    assert!(
        crate::profile_launch::create_tab(&mut tabs, &removed, &intent, |_, _, _| {
            launches += 1;
            anyhow::bail!("removed profile must not reach the factory")
        })
        .is_err()
    );
    assert_eq!(launches, 1);
    runtime.request_focus(&ui.finish_quick_save(submission.generation, Ok(())).unwrap());
    runtime.update(Instant::now());
    assert!(!ui.quick_save_is_open());
    runtime.request_focus(&ui.open_profile_chooser());
    runtime.update(Instant::now());
    let delta = runtime.pending_delta().unwrap();
    assert!(delta.added.iter().chain(&delta.modified).any(|item| matches!(&item.primitive, Primitive::Text { text, .. } if text.as_ref() == "Flow preset")));
    assert_eq!(
        harbor_config::load_candidate_from_path(&path)
            .unwrap()
            .settings
            .profiles
            .len(),
        1
    );
    assert!(!fs::read_to_string(&path).unwrap().contains("not-in-file"));
    assert!(
        ui.take_quick_save_submission(AppCommandRequest::shortcut(AppCommand::CommitQuickSave))
            .is_none()
    );
}

#[test]
fn readonly_target_and_unpublished_external_changes_keep_existing_content() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    fs::write(&path, "# original").unwrap();
    let mut transaction = SaveTransaction::begin(path.clone()).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    let original_permissions = permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).unwrap();
    assert!(
        transaction
            .commit(&profile())
            .unwrap_err()
            .contains("read-only")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "# original");
    fs::set_permissions(&path, original_permissions).unwrap();
    transaction.commit(&profile()).unwrap();
    fs::write(&path, "# changed after disk save").unwrap();
    assert!(transaction.commit(&profile()).is_err());
    assert!(pin_generation(&path, &transaction.committed_bytes().unwrap()).is_err());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# changed after disk save"
    );
}
