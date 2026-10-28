use super::*;

#[cfg(windows)]
#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn native_multi_tab_resize_yields_for_runtime_input_and_quiet_background_completion() {
    let mut f = Fixture::new();
    f.attach_native_children(); // actual cmd and PowerShell PTYs, already at READY
    let native = f.tabs.configuration_targets();
    for _ in 0..5 {
        f.tabs
            .create_tab(|_, draw| {
                #[allow(clippy::arc_with_non_send_sync)]
                let terminal = Arc::new(Mutex::new(Terminal::new_headless(10, 40)));
                let bridge = TerminalWidgetBridge::new(
                    draw,
                    terminal.clone(),
                    Arc::new(AtomicBool::new(false)),
                );
                Ok(TerminalTabResources::new(
                    terminal,
                    bridge,
                    "quiet-background",
                ))
            })
            .unwrap();
    }
    // The Runtime still mounts this original bridge, with a real reload shortcut.
    let active = native[1].0;
    f.tabs.activate(active);
    let target = TerminalSize { rows: 12, cols: 50 };
    f.tabs.resize_all_if_changed(target);
    let all = f.tabs.configuration_targets();
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    // Withhold dispatch of the active member's ACK, independently of reader timing.
    loop {
        for (id, _, _) in &all {
            if *id != active {
                f.tabs.process_output(*id);
            }
        }
        if all
            .iter()
            .filter(|(id, _, _)| *id != active)
            .all(|(_, terminal, _)| terminal.lock().unwrap().resize_ready())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native background ACK not dispatched"
        );
        std::thread::yield_now();
    }
    assert!(!native[1].1.lock().unwrap().resize_ready());
    let first = f.tabs.advance_resizes();
    assert_eq!(first.attempted, 2);
    assert!(!first.active_changed && first.ready_remaining);
    assert_eq!(f.tabs.last_broadcast_size(), None);
    // Real unrelated Runtime input completes before ACK admission/backlog completion.
    f.assert_reload_binding('r');
    assert_eq!(native[1].1.try_lock().unwrap().screen().cols(), 40);
    assert_eq!(
        all.iter()
            .filter(|(_, terminal, _)| terminal.lock().unwrap().screen().cols() == 50)
            .count(),
        2
    );

    loop {
        for (id, _, _) in &all {
            f.tabs.process_output(*id);
        }
        f.tabs.resize_all_if_changed(target);
        let turn = f.tabs.advance_resizes();
        assert!(turn.attempted <= 2);
        if f.tabs.last_broadcast_size() == Some(target) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "quiet resize backlog did not converge"
        );
        std::thread::yield_now();
    }
    for (_, terminal, _) in &all {
        let terminal = terminal.try_lock().unwrap();
        assert_eq!(
            (terminal.screen().rows(), terminal.screen().cols()),
            (12, 50)
        );
        assert!(!terminal.resize_pending());
    }
    // Both native readers were resumed; the original children respond after resizing.
    for (_, terminal, _) in &native {
        terminal.lock().unwrap().write_pty(b"continue\r").unwrap();
    }
    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let mut resumed = true;
        for (index, (id, terminal, _)) in native.iter().enumerate() {
            f.tabs.process_output(*id);
            resumed &=
                retained_text(&terminal.lock().unwrap()).contains(&format!("RESUMED{index}"));
        }
        if resumed {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native reader stranded after resize"
        );
        std::thread::yield_now();
    }
}

#[cfg(windows)]
#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn asynchronous_group_ack_skew_keeps_ui_locks_and_accepted_generation_available() {
    let mut f = Fixture::new();
    f.attach_native_children();
    let candidate = f.candidate();
    let accepted = f.accepted.clone();
    let old_metrics = f.policy.metrics;
    let acquisition = ConfigurationAcquisition::begin(&f.tabs).unwrap();
    // No consumer has dispatched the second member's ACK, even if its reader
    // has already queued it. Observe first-member readiness separately.
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    while !acquisition.members[0].2.is_ready().unwrap() {
        acquisition.members[0].1.lock().unwrap().drain_pty();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(!acquisition.members[1].2.is_ready().unwrap());
    for (_, terminal, bridge) in f.tabs.configuration_targets() {
        let terminal = terminal
            .try_lock()
            .expect("group acquisition must not retain engine locks");
        assert_eq!(
            (terminal.screen().rows(), terminal.screen().cols()),
            (10, 40)
        );
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            old_metrics
        );
    }
    f.assert_reload_binding('r'); // Real Runtime event dispatch while the group is pending.
    assert_eq!(f.accepted, accepted);
    assert_eq!(f.policy.font_settings, accepted.font);
    while !acquisition.poll(&f.tabs).unwrap() {
        for (id, _, _) in f.tabs.configuration_targets() {
            f.tabs.process_output(id);
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    crate::shell::accept_configuration(
        &mut f.accepted,
        &f.clipboard,
        candidate.clone(),
        |settings, bindings| {
            let warnings = f.policy.apply(
                &mut f.tabs,
                &f.ui,
                TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
                settings,
                1.0,
                (640, 320),
                |_, _| Ok(()),
            )?;
            f.ui.publish_keybindings(bindings);
            Ok(warnings)
        },
    )
    .unwrap();
    drop(acquisition);
    assert_eq!(f.accepted, candidate);
    f.native_input();
    f.wait_for_native_markers("RESUMED");
}

#[cfg(windows)]
#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn asynchronous_group_membership_change_rejects_and_releases_surviving_reader() {
    let mut f = Fixture::new();
    f.attach_native_children();
    let accepted = f.accepted.clone();
    let acquisition = ConfigurationAcquisition::begin(&f.tabs).unwrap();
    let removed = acquisition.members[1].0;
    f.tabs.close(removed);
    let error = acquisition.poll(&f.tabs).unwrap_err();
    assert!(error.to_string().contains("membership changed"));
    drop(acquisition); // Cancellation owns no Terminal lock and resumes surviving readers.
    assert_eq!(f.accepted, accepted);
    let terminal = f.tabs.active_terminal().unwrap();
    terminal.lock().unwrap().write_pty(b"continue\r").unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let mut terminal = terminal.lock().unwrap();
        terminal.drain_pty();
        if retained_text(&terminal).contains("RESUMED0") {
            break;
        }
        assert!(Instant::now() < deadline);
        drop(terminal);
        std::thread::yield_now();
    }
}

#[cfg(windows)]
#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn native_later_session_failure_compensates_resumes_and_retries() {
    native_group_rejection_and_retry(false);
}

#[cfg(windows)]
#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn native_compensation_failure_retains_policy_reconciles_resumes_and_retries() {
    native_group_rejection_and_retry(true);
}
