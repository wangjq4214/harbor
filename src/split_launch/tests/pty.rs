//! Real local Windows/ConPTY observations; no remote fixtures are implied.
use super::*;
use std::path::{Path, PathBuf};

fn local_path(path: &Path) -> PathBuf {
    let path = std::fs::canonicalize(path).unwrap();
    PathBuf::from(path.to_str().unwrap().trim_start_matches(r"\\?\"))
}
fn native_attempt(
    command: &ShellCommand,
    probe: Option<&StartupProbe>,
    draw: ExternalDrawId,
) -> AttemptLaunch {
    let mut cleanup = None;
    let result = (|| {
        let mut endpoints = harbor_pty::PtyEndpoints::spawn_shell(
            harbor_pty::TerminalSize {
                rows: 24,
                cols: 120,
            },
            command,
        )?;
        cleanup = Some(Cleanup::Native(endpoints.shutdown_receipt()));
        if let Some(p) = probe {
            endpoints.observe_output(p.observer());
        }
        let terminal =
            Terminal::new_headless(24, 120).start_session_from_endpoints(endpoints, || true)?;
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(terminal));
        let bridge =
            TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
        Ok(TerminalTabResources::new(terminal, bridge, "native-child"))
    })();
    AttemptLaunch {
        resources: result,
        cleanup,
    }
}
fn poll_until(
    op: &mut SplitOperation,
    tabs: &TabManager,
    mut launch: impl FnMut(&ShellCommand, Option<&StartupProbe>) -> AttemptLaunch,
) -> SplitPoll {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let poll = op.poll(tabs, Instant::now(), &mut launch);
        if !matches!(poll, SplitPoll::Pending) {
            return poll;
        }
        assert!(
            Instant::now() < deadline,
            "native split startup/cleanup deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn report(terminal: &mut Terminal, path: &Path, expected: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        terminal.drain_pty();
        if let Ok(text) = std::fs::read_to_string(path) {
            let text = text.trim_start_matches('\u{feff}').trim();
            if text == expected {
                return text.into();
            }
        }
        assert!(Instant::now() < deadline, "shell report not written");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
fn check_cwd(terminal: &mut Terminal, file: &Path, expected: &Path) {
    let script = format!(
        "[IO.File]::WriteAllText({}, (Get-Location).ProviderPath)\r",
        ps_quote(file.to_str().unwrap())
    );
    terminal.write_pty(script.as_bytes()).unwrap();
    assert_eq!(
        PathBuf::from(report(terminal, file, expected.to_str().unwrap())),
        expected
    );
}
fn wait_receipt(receipt: &Cleanup) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !receipt.complete() {
        assert!(
            Instant::now() < deadline,
            "native resources were not released"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "pty: Windows ConPTY and local PowerShell 7"]
fn ready_immediate_exit_before_waiting_poll_never_launches_default() {
    let root = tempfile::tempdir().unwrap();
    let current = local_path(root.path());
    let (mut tabs, source) = fixture();
    context(
        &mut tabs,
        source,
        &ProjectEnvironment::LocalPwsh,
        current.to_str().unwrap(),
    );
    let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let (_, draw) = op.target();
    let mut probe = None;
    let mut receipt = None;
    op.poll(&tabs, Instant::now(), |command, p| {
        probe = p.cloned();
        let mut command = command.clone();
        // Keep real cwd entry and attempt marker, then exit without any Host poll.
        command.args.last_mut().unwrap().push_str("; exit 0");
        let attempt = native_attempt(&command, p, draw);
        if let Some(Cleanup::Native(native)) = &attempt.cleanup {
            receipt = Some(Cleanup::Native(native.clone()));
        }
        attempt
    });
    let child = op.pending_resources().unwrap().terminal().clone();
    let p = probe.unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    // Observe only process state/raw readiness, not startup polling or parser progress.
    while p.status.load(Ordering::Acquire) != READY || !child.lock().unwrap().shell_process_exited()
    {
        assert!(Instant::now() < deadline, "READY plus immediate shell exit");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(child.lock().unwrap().parsed_output_bytes(), 0);
    match poll_until(&mut op, &tabs, no_launch) {
        SplitPoll::Failed(error) => {
            assert_eq!(error.to_string(), "split session exited before attachment");
        }
        _ => panic!("dead successful child must not publish or restart"),
    }
    assert!(child.lock().unwrap().is_session_closed());
    assert!(receipt.unwrap().complete());
    assert!(tabs.contains_binding(source));
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Finished
    ));
}

#[test]
#[ignore = "pty: Windows ConPTY and local PowerShell 7"]
fn local_current_not_initial_quoted_directory_independent_child_and_later_natural_exit() {
    let root = tempfile::tempdir().unwrap();
    let initial = root.path().join("initial");
    let current = root.path().join("current ' $ ; & [literal] 世界");
    std::fs::create_dir(&initial).unwrap();
    std::fs::create_dir(&current).unwrap();
    let initial = local_path(&initial);
    let current = local_path(&current);
    let mut tabs = TabManager::new();
    let mut source_cleanup = None;
    tabs.create_tab(|_, draw| {
        let command = ShellCommand {
            program: Some("pwsh".into()),
            args: vec!["-NoLogo".into(), "-NoProfile".into(), "-NoExit".into()],
            directory: Some(initial.clone()),
            ..Default::default()
        };
        let attempt = native_attempt(&command, None, draw);
        source_cleanup = attempt.cleanup;
        attempt.resources
    })
    .unwrap();
    let binding = tabs.active_binding().unwrap();
    let source_terminal = tabs.session_terminal(binding.session).unwrap();
    {
        let mut t = source_terminal.lock().unwrap();
        assert_eq!(t.launch_provenance().unwrap().initial_directory, initial);
        // Use the project's actual opt-in PowerShell integration in the live source.
        let integration = local_path(Path::new("scripts/shell/harbor-project.ps1"));
        t.write_pty(
            format!(
                ". {}; Set-Location -LiteralPath {}\r",
                ps_quote(integration.to_str().unwrap()),
                ps_quote(current.to_str().unwrap())
            )
            .as_bytes(),
        )
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        tabs.process_output(binding.session);
        if source_terminal
            .lock()
            .unwrap()
            .project_snapshot()
            .is_some_and(|r| Path::new(&r.directory) == current)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "real pwsh integration did not report changed cwd"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut operation = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    assert_eq!(
        PathBuf::from(&operation.context.as_ref().unwrap().directory),
        current
    );
    let (_, draw) = operation.target();
    let mut child_cleanup = None;
    let complete = ready(poll_until(&mut operation, &tabs, |command, probe| {
        assert!(probe.is_some());
        let attempt = native_attempt(command, probe, draw);
        if let Some(Cleanup::Native(receipt)) = &attempt.cleanup {
            child_cleanup = Some(Cleanup::Native(receipt.clone()));
        }
        attempt
    }));
    let prepared = complete.claim(&tabs, operation.id()).unwrap();
    assert_eq!(prepared.kind, LaunchKind::Inherited);
    assert_ne!(
        source_terminal.lock().unwrap().content_version().session,
        prepared
            .resources
            .terminal()
            .lock()
            .unwrap()
            .content_version()
            .session
    );
    check_cwd(
        &mut prepared.resources.terminal().lock().unwrap(),
        &root.path().join("child-cwd.txt"),
        &current,
    );
    check_cwd(
        &mut source_terminal.lock().unwrap(),
        &root.path().join("source-cwd.txt"),
        &current,
    );
    {
        let mut child = prepared.resources.terminal().lock().unwrap();
        child.write_pty(b"exit\r").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !child.shell_process_exited() {
            child.drain_pty();
            assert!(Instant::now() < deadline, "shell-process exit not observed");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(matches!(
        operation.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Finished
    ));
    assert!(!source_terminal.lock().unwrap().is_session_closed());
    drop(prepared);
    wait_receipt(&child_cleanup.unwrap());
    tabs.close(binding.workspace);
    wait_receipt(&source_cleanup.unwrap());
}

#[test]
#[ignore = "pty: Windows ConPTY and local PowerShell 7"]
fn removed_directory_under_noexit_falls_back_after_native_cleanup_with_default_policy() {
    let root = tempfile::tempdir().unwrap();
    let inherited = root.path().join("removed ' 世界");
    let default_dir = root.path().join("default");
    std::fs::create_dir(&inherited).unwrap();
    std::fs::create_dir(&default_dir).unwrap();
    let inherited = local_path(&inherited);
    let default_dir = local_path(&default_dir);
    let (mut tabs, a) = fixture();
    context(
        &mut tabs,
        a,
        &ProjectEnvironment::LocalPwsh,
        inherited.to_str().unwrap(),
    );
    let report_file = root.path().join("default-policy.txt");
    let default = crate::tab_coordinator::configured_shell(&harbor_config::ShellSettings {
        program: Some("pwsh".into()),
        args: vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NoExit".into(),
            "-Command".into(),
            format!(
                "[IO.File]::WriteAllText({}, ($env:HARBOR_SPLIT_POLICY + '|' + (Get-Location).ProviderPath))",
                ps_quote(report_file.to_str().unwrap())
            ),
        ],
        directory: Some(default_dir.clone()),
        environment: [("HARBOR_SPLIT_POLICY".into(), "default ' $ literal".into())].into(),
    });
    let mut op = SplitOperation::begin(&mut tabs, default.clone()).unwrap();
    std::fs::remove_dir(&inherited).unwrap(); // Disappears after capture, before Set-Location.
    let (_, draw) = op.target();
    let mut first_cleanup = None;
    let mut second_cleanup = None;
    let mut first_probe = None;
    let mut calls = 0;
    let complete = ready(poll_until(&mut op, &tabs, |command, probe| {
        calls += 1;
        if calls == 1 {
            assert!(command.args.iter().any(|s| s == "-NoExit"));
            first_probe = probe.cloned();
        } else {
            assert_eq!(calls, 2);
            assert!(probe.is_none());
            assert_eq!(command, &default);
            assert!(
                first_cleanup.as_ref().is_some_and(Cleanup::complete),
                "replacement spawned before native teardown"
            );
            assert_eq!(
                first_probe.as_ref().unwrap().status.load(Ordering::Acquire),
                FAILED,
                "-NoExit cwd error was not observed"
            );
        }
        let attempt = native_attempt(command, probe, draw);
        if let Some(Cleanup::Native(receipt)) = &attempt.cleanup {
            if calls == 1 {
                first_cleanup = Some(Cleanup::Native(receipt.clone()));
            } else {
                second_cleanup = Some(Cleanup::Native(receipt.clone()));
            }
        }
        attempt
    }));
    assert_eq!(calls, 2);
    let prepared = complete.claim(&tabs, op.id()).unwrap();
    assert_eq!(prepared.kind, LaunchKind::DefaultStartupFailure);
    assert_eq!(
        report(
            &mut prepared.resources.terminal().lock().unwrap(),
            &report_file,
            &format!("default ' $ literal|{}", default_dir.display())
        ),
        format!("default ' $ literal|{}", default_dir.display())
    );
    assert_eq!(
        prepared
            .resources
            .terminal()
            .lock()
            .unwrap()
            .launch_provenance()
            .unwrap()
            .args,
        default.args
    );
    drop(prepared);
    wait_receipt(&second_cleanup.unwrap());
    assert!(
        !tabs
            .session_terminal(a.session)
            .unwrap()
            .lock()
            .unwrap()
            .is_session_closed()
    );
}

#[test]
#[ignore = "pty: Windows ConPTY and local PowerShell 7"]
fn native_inherited_failure_then_default_failure_is_honest_and_cleans_up() {
    let (mut tabs, a) = fixture();
    let root = tempfile::tempdir().unwrap();
    context(
        &mut tabs,
        a,
        &ProjectEnvironment::LocalPwsh,
        local_path(root.path()).join("absent").to_str().unwrap(),
    );
    let default = ShellCommand {
        program: Some("pwsh".into()),
        directory: Some(root.path().join("also absent")),
        ..ShellCommand::default()
    };
    let mut op = SplitOperation::begin(&mut tabs, default).unwrap();
    let (_, draw) = op.target();
    let mut receipts = Vec::new();
    let mut calls = 0;
    let result = poll_until(&mut op, &tabs, |command, probe| {
        calls += 1;
        if calls == 2 {
            assert!(receipts.iter().all(Cleanup::complete));
        }
        let attempt = native_attempt(command, probe, draw);
        if let Some(Cleanup::Native(receipt)) = &attempt.cleanup {
            receipts.push(Cleanup::Native(receipt.clone()));
        }
        attempt
    });
    assert!(matches!(result, SplitPoll::Failed(_)));
    assert_eq!(calls, 2);
    assert!(receipts.iter().all(Cleanup::complete));
    assert_eq!(tabs.snapshots().len(), 1);
    assert!(source_live(&tabs, a));
}

#[test]
#[ignore = "pty: Windows ConPTY and local PowerShell 7"]
fn native_delayed_readiness_after_source_close_is_cancelled_and_fully_retired() {
    let (mut tabs, a) = fixture();
    let root = tempfile::tempdir().unwrap();
    context(
        &mut tabs,
        a,
        &ProjectEnvironment::LocalPwsh,
        local_path(root.path()).to_str().unwrap(),
    );
    let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let (_, draw) = op.target();
    let mut receipt = None;
    let mut probe = None;
    let mut child = None;
    op.poll(&tabs, Instant::now(), |command, p| {
        let mut command = command.clone();
        command.args[3].insert_str(0, "Start-Sleep -Milliseconds 200; ");
        probe = p.cloned();
        let attempt = native_attempt(&command, p, draw);
        if let Some(Cleanup::Native(r)) = &attempt.cleanup {
            receipt = Some(Cleanup::Native(r.clone()));
        }
        child = attempt
            .resources
            .as_ref()
            .ok()
            .map(|r| r.terminal().clone());
        attempt
    });
    assert_eq!(op.pending_resources().unwrap().bridge().draw_id(), draw);
    tabs.close(a.workspace);
    let b = source(&mut tabs);
    assert!(matches!(
        poll_until(&mut op, &tabs, no_launch),
        SplitPoll::Cancelled
    ));
    let p = probe.unwrap();
    (p.observer())(&p.ready);
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Finished
    ));
    assert!(receipt.unwrap().complete());
    assert!(child.unwrap().lock().unwrap().is_session_closed());
    assert_eq!(tabs.active_binding(), Some(b));
}
