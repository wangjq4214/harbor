use super::*;
use harbor_app::terminal_view::TerminalWidgetBridge;
use harbor_config::{ProjectEnvironment, ProjectRecipe};
use harbor_terminal::Terminal;
use std::sync::Mutex;

fn resources(draw: ExternalDrawId) -> TerminalTabResources {
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(Terminal::new_headless(6, 80)));
    let bridge =
        TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
    TerminalTabResources::new(terminal, bridge, "synthetic-shell")
}
fn source(tabs: &mut TabManager) -> PaneBinding {
    tabs.create_tab(|_, draw| Ok(resources(draw))).unwrap();
    tabs.active_binding().unwrap()
}
fn project_event(tabs: &mut TabManager, binding: PaneBinding, action: &str, values: &[&str]) {
    let fields = values
        .iter()
        .map(|s| {
            s.as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(";");
    let text = format!("\x1b]777;harbor;1;{action};{fields}\x07");
    tabs.session_terminal(binding.session)
        .unwrap()
        .lock()
        .unwrap()
        .put_bytes(text.as_bytes());
    tabs.process_output(binding.session);
}
fn context(tabs: &mut TabManager, binding: PaneBinding, env: &ProjectEnvironment, directory: &str) {
    project_event(
        tabs,
        binding,
        "context",
        &[env.kind(), env.target(), directory],
    );
}
fn fixture() -> (TabManager, PaneBinding) {
    let mut tabs = TabManager::new();
    let source = source(&mut tabs);
    context(
        &mut tabs,
        source,
        &ProjectEnvironment::LocalPwsh,
        "C:/current",
    );
    (tabs, source)
}
fn no_launch(_: &ShellCommand, _: Option<&StartupProbe>) -> AttemptLaunch {
    panic!("unexpected launch")
}
fn success(draw: ExternalDrawId) -> AttemptLaunch {
    AttemptLaunch {
        resources: Ok(resources(draw)),
        cleanup: None,
    }
}
fn ready(poll: SplitPoll) -> SplitReady {
    match poll {
        SplitPoll::Ready(r) => *r,
        _ => panic!("expected Ready"),
    }
}

#[test]
fn recognition_captures_current_environment_not_initial_or_later_focus_or_agent() {
    for (env, initial, current, later) in [
        (
            ProjectEnvironment::LocalPwsh,
            "C:/initial",
            "C:/current ' 世界",
            "C:/later",
        ),
        (
            ProjectEnvironment::Wsl {
                distribution: "Ubuntu-24.04".into(),
            },
            "/initial",
            "/current ' 世界",
            "/later",
        ),
        (
            ProjectEnvironment::Ssh {
                destination: "dev@controlled-alias".into(),
            },
            "/initial",
            "/current ' 世界",
            "/later",
        ),
    ] {
        let mut tabs = TabManager::new();
        let a = source(&mut tabs);
        context(&mut tabs, a, &env, initial);
        context(&mut tabs, a, &env, current);
        let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
        assert_eq!(op.context.as_ref().unwrap().directory, current);
        assert_eq!(op.context.as_ref().unwrap().environment, env);
        assert!(op.context.as_ref().unwrap().agent.is_none());
        context(&mut tabs, a, &env, later);
        let b = source(&mut tabs);
        let mut probe = None;
        assert!(matches!(
            op.poll(&tabs, Instant::now(), |command, p| {
                assert_eq!(command.program.as_deref(), Some(env.program()));
                assert!(command.directory.is_none() && command.environment.is_empty());
                assert!(!command.args.join(" ").contains(initial));
                assert!(!command.args.join(" ").contains(later));
                probe = p.cloned();
                success(op_draw())
            }),
            SplitPoll::Pending
        ));
        // Process creation alone does not report Ready.
        assert!(matches!(
            op.poll(&tabs, Instant::now(), no_launch),
            SplitPoll::Pending
        ));
        let p = probe.unwrap();
        op.pending_resources()
            .unwrap()
            .terminal()
            .lock()
            .unwrap()
            .put_bytes(&p.ready);
        (p.observer())(&p.ready);
        let complete = ready(op.poll(&tabs, Instant::now(), no_launch));
        let prepared = complete.claim(&tabs, op.id()).unwrap();
        assert_eq!(prepared.source, a);
        assert_eq!(prepared.target.workspace, a.workspace);
        assert_ne!(prepared.target.pane, a.pane);
        assert_ne!(prepared.target.session, a.session);
        assert_eq!(tabs.active_binding(), Some(b));
        assert_eq!(tabs.snapshots().len(), 2); // Nothing attached by T0002.
    }
}
fn op_draw() -> ExternalDrawId {
    ExternalDrawId::new(9999)
}

#[test]
fn handshake_is_streaming_operation_qualified_and_terminal() {
    let a = StartupProbe::new(OperationId(101)).unwrap();
    let b = StartupProbe::new(OperationId(102)).unwrap();
    let mut observe = a.observer();
    let different_attempt = StartupProbe::new(OperationId(101)).unwrap();
    observe(&different_attempt.ready);
    assert_eq!(a.status.load(Ordering::Acquire), PENDING);
    observe(&vec![b'x'; 8192]);
    observe(&b.ready);
    assert_eq!(a.status.load(Ordering::Acquire), PENDING);
    for byte in &a.ready {
        observe(&[*byte]);
    }
    assert_eq!(a.status.load(Ordering::Acquire), READY);
    observe(&a.failed);
    observe(&[]); // Later natural EOF cannot undo readiness.
    assert_eq!(a.status.load(Ordering::Acquire), READY);
    let mut observe_b = b.observer();
    observe_b(&b.failed);
    observe_b(&b.ready);
    assert_eq!(b.status.load(Ordering::Acquire), FAILED);
}

#[test]
fn delayed_failure_retires_partial_resources_then_waits_for_cleanup_before_exact_default() {
    let (mut tabs, binding) = fixture();
    let default = ShellCommand {
        program: Some("default.exe".into()),
        args: vec!["".into(), "a b".into(), "--flag".into()],
        directory: Some("C:/default".into()),
        environment: [("ONLY_DEFAULT".into(), "literal value".into())].into(),
        ..ShellCommand::default()
    };
    let mut op = SplitOperation::begin(&mut tabs, default.clone()).unwrap();
    let clean = Arc::new(AtomicBool::new(false));
    let mut probe = None;
    let mut retained = None;
    op.poll(&tabs, Instant::now(), |_, p| {
        probe = p.cloned();
        let res = resources(op_draw());
        retained = Some(res.terminal().clone());
        AttemptLaunch {
            resources: Ok(res),
            cleanup: Some(Cleanup::Controlled(clean.clone())),
        }
    });
    let p = probe.unwrap();
    (p.observer())(&p.failed);
    source(&mut tabs); // Switching source does not cancel the original operation.
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Pending
    ));
    assert!(retained.unwrap().lock().unwrap().is_session_closed());
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Pending
    ));
    (p.observer())(&p.ready); // Stale completion from the failed attempt is terminally ignored.
    clean.store(true, Ordering::Release);
    let complete = ready(op.poll(&tabs, Instant::now(), |command, probe| {
        assert!(probe.is_none());
        assert_eq!(command, &default); // Includes executable recovery policy, args, cwd and environment.
        success(op_draw())
    }));
    let prepared = complete.claim(&tabs, op.id()).unwrap();
    assert_eq!(prepared.kind, LaunchKind::DefaultStartupFailure);
    assert_eq!(prepared.source, binding);
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Finished
    ));
}

#[test]
fn unknown_and_invalid_context_use_default_without_inheritance_or_connection() {
    for directory in [None, Some("not absolute")] {
        let (mut tabs, _) = fixture();
        let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
        op.context = directory.map(|d| ProjectRecipe {
            environment: ProjectEnvironment::Ssh {
                destination: "-bad".into(),
            },
            directory: d.into(),
            agent: None,
        });
        let r = ready(op.poll(&tabs, Instant::now(), |command, p| {
            assert_eq!(command, &ShellCommand::default());
            assert!(p.is_none());
            success(op_draw())
        }));
        assert_eq!(
            r.claim(&tabs, op.id()).unwrap().kind,
            LaunchKind::DefaultUnknownContext
        );
    }
}

#[test]
fn both_stages_failing_wait_for_each_cleanup_and_report_only_once() {
    let (mut tabs, binding) = fixture();
    let original = tabs.session_terminal(binding.session).unwrap();
    let version = original.lock().unwrap().content_version();
    let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let clean_first = Arc::new(AtomicBool::new(false));
    let clean_second = Arc::new(AtomicBool::new(false));
    op.poll(&tabs, Instant::now(), |_, p| {
        assert!(p.is_some());
        AttemptLaunch {
            resources: Err(anyhow::anyhow!("post-spawn resource failure")),
            cleanup: Some(Cleanup::Controlled(clean_first.clone())),
        }
    });
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Pending
    ));
    clean_first.store(true, Ordering::Release);
    assert!(matches!(
        op.poll(&tabs, Instant::now(), |_, p| {
            assert!(p.is_none());
            AttemptLaunch {
                resources: Err(anyhow::anyhow!("default failed")),
                cleanup: Some(Cleanup::Controlled(clean_second.clone())),
            }
        }),
        SplitPoll::Pending
    ));
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Pending
    ));
    clean_second.store(true, Ordering::Release);
    match op.poll(&tabs, Instant::now(), no_launch) {
        SplitPoll::Failed(e) => assert_eq!(e.to_string(), "default failed"),
        _ => panic!("expected honest failure"),
    }
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Finished
    ));
    assert_eq!(tabs.snapshots().len(), 1);
    assert_eq!(original.lock().unwrap().content_version(), version);
    assert!(!original.lock().unwrap().is_session_closed());
}

#[test]
fn closure_cancellation_and_stale_ready_never_retarget_replacement() {
    for stage in [
        "before",
        "waiting",
        "ready",
        "cancel-ready",
        "drop-ready",
        "wrong-operation",
        "source-exit",
    ] {
        let (mut tabs, a) = fixture();
        let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
        let id = op.id();
        if stage == "before" {
            tabs.close(a.workspace);
            source(&mut tabs);
            assert!(matches!(
                op.poll(&tabs, Instant::now(), no_launch),
                SplitPoll::Cancelled
            ));
            continue;
        }
        let mut probe = None;
        let mut child = None;
        op.poll(&tabs, Instant::now(), |_, p| {
            probe = p.cloned();
            let res = resources(op_draw());
            child = Some(res.terminal().clone());
            AttemptLaunch {
                resources: Ok(res),
                cleanup: None,
            }
        });
        if stage == "waiting" || stage == "source-exit" {
            if stage == "waiting" {
                tabs.close(a.workspace);
            } else {
                tabs.session_terminal(a.session)
                    .unwrap()
                    .lock()
                    .unwrap()
                    .retire_session();
            }
            source(&mut tabs);
            op.pending_resources()
                .unwrap()
                .terminal()
                .lock()
                .unwrap()
                .put_bytes(&probe.as_ref().unwrap().ready);
            (probe.as_ref().unwrap().observer())(&probe.as_ref().unwrap().ready);
            assert!(matches!(
                op.poll(&tabs, Instant::now(), no_launch),
                SplitPoll::Pending
            ));
            assert!(matches!(
                op.poll(&tabs, Instant::now(), no_launch),
                SplitPoll::Cancelled
            ));
        } else {
            let p = probe.unwrap();
            op.pending_resources()
                .unwrap()
                .terminal()
                .lock()
                .unwrap()
                .put_bytes(&p.ready);
            (p.observer())(&p.ready);
            let r = ready(op.poll(&tabs, Instant::now(), no_launch));
            match stage {
                "cancel-ready" => op.cancel(),
                "drop-ready" => drop(op),
                "wrong-operation" => {}
                _ => {
                    tabs.close(a.workspace);
                    source(&mut tabs);
                }
            }
            let expected = if stage == "wrong-operation" {
                OperationId(u64::MAX)
            } else {
                id
            };
            assert!(r.claim(&tabs, expected).is_err());
        }
        assert!(child.unwrap().lock().unwrap().is_session_closed());
    }
}

#[test]
fn timeout_and_pre_ready_exit_fallback_but_post_ready_exit_never_restarts() {
    for reason in ["timeout", "exit", "natural-after-ready"] {
        let (mut tabs, _) = fixture();
        let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
        let now = Instant::now();
        let mut probe = None;
        let mut child = None;
        op.poll(&tabs, now, |_, p| {
            probe = p.cloned();
            let r = resources(op_draw());
            child = Some(r.terminal().clone());
            AttemptLaunch {
                resources: Ok(r),
                cleanup: None,
            }
        });
        let child = child.unwrap();
        assert!(op.next_poll(now).unwrap() <= now + POLL_INTERVAL);
        if reason == "natural-after-ready" {
            let p = probe.unwrap();
            op.pending_resources()
                .unwrap()
                .terminal()
                .lock()
                .unwrap()
                .put_bytes(&p.ready);
            (p.observer())(&p.ready);
            let prepared = ready(op.poll(&tabs, now, no_launch))
                .claim(&tabs, op.id())
                .unwrap();
            assert_eq!(prepared.kind, LaunchKind::Inherited);
            child.lock().unwrap().retire_session();
            assert!(matches!(
                op.poll(&tabs, now + STARTUP_TIMEOUT, no_launch),
                SplitPoll::Finished
            ));
            assert!(op.next_poll(now).is_none());
        } else {
            if reason == "exit" {
                child.lock().unwrap().retire_session();
            }
            let later = if reason == "timeout" {
                now + STARTUP_TIMEOUT
            } else {
                now
            };
            assert!(matches!(
                op.poll(&tabs, later, no_launch),
                SplitPoll::Pending
            ));
            let r = ready(op.poll(&tabs, later, |_, p| {
                assert!(p.is_none());
                success(op_draw())
            }));
            assert_eq!(
                r.claim(&tabs, op.id()).unwrap().kind,
                LaunchKind::DefaultStartupFailure
            );
        }
    }
}

#[test]
fn split_reservations_burn_ids_without_publishing_or_reusing_on_cancellation() {
    let (mut tabs, a) = fixture();
    let op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let first = op.target();
    drop(op);
    let next = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let second = next.target();
    assert_eq!(first.0.workspace, a.workspace);
    assert_ne!(first.0.pane, second.0.pane);
    assert_ne!(first.0.session, second.0.session);
    assert_ne!(first.1, second.1);
    assert_eq!(tabs.snapshots().len(), 1);
}

#[test]
fn last_agent_directory_and_arguments_are_not_split_launch_policy() {
    let (mut tabs, a) = fixture();
    project_event(
        &mut tabs,
        a,
        "agent",
        &[
            "pwsh",
            "",
            "C:/old-agent",
            "codex",
            "--resume",
            "do-not-replay",
        ],
    );
    context(
        &mut tabs,
        a,
        &ProjectEnvironment::LocalPwsh,
        "C:/new-current",
    );
    assert_eq!(
        tabs.session_terminal(a.session)
            .unwrap()
            .lock()
            .unwrap()
            .project_snapshot()
            .unwrap()
            .agent
            .unwrap()
            .program,
        "codex"
    );
    let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    op.poll(&tabs, Instant::now(), |command, probe| {
        assert!(probe.is_some());
        assert!(command.args.last().unwrap().contains("C:/new-current"));
        for stale in ["old-agent", "codex", "--resume", "do-not-replay"] {
            assert!(!command.args.join(" ").contains(stale));
        }
        success(op_draw())
    });
}

#[test]
fn source_closure_while_cleaning_never_starts_fallback_or_steals_replacement() {
    let (mut tabs, a) = fixture();
    let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let cleaned = Arc::new(AtomicBool::new(false));
    op.poll(&tabs, Instant::now(), |_, _| AttemptLaunch {
        resources: Err(anyhow::anyhow!("partial inherited error")),
        cleanup: Some(Cleanup::Controlled(cleaned.clone())),
    });
    tabs.close(a.workspace);
    let replacement = source(&mut tabs);
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Pending
    ));
    cleaned.store(true, Ordering::Release);
    assert!(matches!(
        op.poll(&tabs, Instant::now(), no_launch),
        SplitPoll::Cancelled
    ));
    assert_eq!(tabs.active_binding(), Some(replacement));
}

#[test]
fn pending_output_keeps_only_latest_display_metadata_and_never_replays_clipboard() {
    use harbor_terminal::TerminalOutputEvent;
    let (mut tabs, _) = fixture();
    let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let mut probe = None;
    op.poll(&tabs, Instant::now(), |_, p| {
        probe = p.cloned();
        success(op_draw())
    });
    for index in 0..100 {
        {
            let mut terminal = op.pending_resources().unwrap().terminal().lock().unwrap();
            terminal.set_clipboard_delivery(harbor_terminal::ClipboardDelivery::Latest);
            terminal.put_bytes(format!("\x1b]2;title-{index}\x07\x1b]7;file:///C:/cwd-{index}\x07\x1b]133;A\x07\x1b]52;c;ZG8tbm90LXJlcGxheQ==\x07").as_bytes());
        }
        assert!(matches!(
            op.poll(&tabs, Instant::now(), no_launch),
            SplitPoll::Pending
        ));
        assert_eq!(op.metadata.len(), 3);
    }
    let p = probe.unwrap();
    op.pending_resources()
        .unwrap()
        .terminal()
        .lock()
        .unwrap()
        .put_bytes(&p.ready);
    (p.observer())(&p.ready);
    let prepared = ready(op.poll(&tabs, Instant::now(), no_launch))
        .claim(&tabs, op.id())
        .unwrap();
    assert_eq!(prepared.metadata.len(), 3);
    assert!(
        prepared
            .metadata
            .iter()
            .any(|e| matches!(e, TerminalOutputEvent::TitleChanged(title) if title == "title-99"))
    );
    assert!(
        prepared
            .metadata
            .iter()
            .all(|e| !matches!(e, TerminalOutputEvent::ClipboardWrite(_)))
    );
    assert!(
        prepared
            .resources
            .terminal()
            .lock()
            .unwrap()
            .drain_output_events()
            .is_empty()
    );
}

#[test]
fn remote_connection_eof_missing_target_and_directory_failure_choose_only_split_default() {
    for environment in [
        ProjectEnvironment::Wsl {
            distribution: "declared-fixture".into(),
        },
        ProjectEnvironment::Ssh {
            destination: "declared-fixture".into(),
        },
    ] {
        for failure in ["spawn", "connection-eof", "cwd"] {
            let (mut tabs, a) = fixture();
            context(&mut tabs, a, &environment, "/current-not-native");
            let default = ShellCommand {
                args: vec!["default-only".into()],
                ..ShellCommand::default()
            };
            let mut op = SplitOperation::begin(&mut tabs, default.clone()).unwrap();
            op.poll(&tabs, Instant::now(), |command, probe| {
                assert_eq!(command.program.as_deref(), Some(environment.program()));
                assert!(command.directory.is_none());
                if failure == "spawn" {
                    AttemptLaunch {
                        resources: Err(anyhow::anyhow!("controlled launcher failure")),
                        cleanup: None,
                    }
                } else {
                    let p = probe.unwrap();
                    let mut observe = p.observer();
                    if failure == "cwd" {
                        observe(&p.failed);
                    } else {
                        observe(&[]);
                    }
                    success(op_draw())
                }
            });
            if failure != "spawn" {
                assert!(matches!(
                    op.poll(&tabs, Instant::now(), no_launch),
                    SplitPoll::Pending
                ));
            }
            let r = ready(op.poll(&tabs, Instant::now(), |command, p| {
                assert_eq!(command, &default);
                assert!(p.is_none());
                success(op_draw())
            }));
            assert_eq!(
                r.claim(&tabs, op.id()).unwrap().kind,
                LaunchKind::DefaultStartupFailure
            );
        }
    }
}

#[cfg(windows)]
mod pty;

#[cfg(windows)]
mod native;

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn queued_startup_clipboard_before_readiness_cannot_escape_after_publication() {
    use std::io::Read;
    use std::sync::mpsc;
    struct QueuedReader<F> {
        chunks: std::collections::VecDeque<Vec<u8>>,
        observe: F,
        finished: mpsc::Sender<()>,
        hold: mpsc::Receiver<()>,
    }
    impl<F: FnMut(&[u8])> Read for QueuedReader<F> {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if let Some(chunk) = self.chunks.pop_front() {
                bytes[..chunk.len()].copy_from_slice(&chunk);
                (self.observe)(&chunk);
                Ok(chunk.len())
            } else {
                let _ = self.finished.send(());
                let _ = self.hold.recv();
                Ok(0)
            }
        }
    }
    let (mut tabs, source) = fixture();
    let mut op = SplitOperation::begin(&mut tabs, ShellCommand::default()).unwrap();
    let (queued_tx, queued_rx) = mpsc::channel();
    let (_hold_tx, hold_rx) = mpsc::channel();
    let mut hold_rx = Some(hold_rx);
    let (target, draw) = op.target();
    op.poll(&tabs, Instant::now(), |_, probe| {
        let probe = probe.unwrap();
        let mut chunks: std::collections::VecDeque<_> =
            (0..24).map(|_| b"startup\r\n".to_vec()).collect();
        chunks.push_back(b"\x1b]52;c;ZG8tbm90LXJlcGxheQ==\x07".to_vec());
        chunks.push_back(probe.ready.clone());
        let reader = QueuedReader {
            chunks,
            observe: probe.observer(),
            finished: queued_tx.clone(),
            hold: hold_rx.take().unwrap(),
        };
        let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            40,
            100,
            reader,
            std::io::sink(),
            || true,
        )));
        AttemptLaunch {
            resources: Ok(TerminalTabResources::new(
                terminal.clone(),
                TerminalWidgetBridge::new(draw, terminal, Arc::new(AtomicBool::new(false))),
                "queued",
            )),
            cleanup: None,
        }
    });
    queued_rx.recv_timeout(Duration::from_secs(3)).unwrap(); // all 26 chunks are enqueued, reader remains alive
    assert!(
        matches!(
            op.poll(&tabs, Instant::now(), no_launch),
            SplitPoll::Pending
        ),
        "raw readiness cannot publish ahead of parser progress"
    );
    assert_eq!(
        op.pending_resources()
            .unwrap()
            .terminal()
            .lock()
            .unwrap()
            .parsed_output_bytes(),
        16 * 9,
        "one poll must retain the 16-chunk ingestion budget"
    );
    let mut complete = None;
    for _ in 0..8 {
        match op.poll(&tabs, Instant::now(), no_launch) {
            SplitPoll::Ready(result) => {
                complete = Some(*result);
                break;
            }
            SplitPoll::Pending => {}
            _ => panic!("unexpected startup result"),
        }
    }
    let prepared = complete
        .expect("bounded startup progress")
        .claim(&tabs, op.id())
        .unwrap();
    let terminal = prepared.resources.terminal().clone();
    tabs.attach_split(
        source,
        target,
        draw,
        prepared.resources,
        harbor_widget::SplitAxis::Horizontal,
    )
    .unwrap();
    // Exercise only the existing parser delivery path, never an OS clipboard.
    let mut terminal = terminal.lock().unwrap();
    for _ in 0..8 {
        terminal.drain_pty();
    }
    assert!(
        terminal.drain_output_events().iter().all(|event| !matches!(
            event,
            harbor_terminal::TerminalOutputEvent::ClipboardWrite(_)
        )),
        "startup OSC52 escaped after attach"
    );
    terminal.put_bytes(b"\x1b]52;c;bGl2ZQ==\x07");
    assert!(
        terminal.drain_output_events().iter().any(|event| matches!(
            event,
            harbor_terminal::TerminalOutputEvent::ClipboardWrite(_)
        )),
        "live parser delivery path remains intact"
    );
}
