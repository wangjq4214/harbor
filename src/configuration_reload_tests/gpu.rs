use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn later_session_preflight_failure_preserves_application_generation_and_retry() {
    let mut f = Fixture::new();
    let before = f.accepted.clone();
    let metrics = f.policy.metrics;
    let targets = f.tabs.configuration_targets();
    let snapshots: Vec<_> = targets
        .iter()
        .map(|(_, t, _)| t.lock().unwrap().snapshot())
        .collect();
    for stage in [
        ReloadStage::Font,
        ReloadStage::Metrics,
        ReloadStage::Model,
        ReloadStage::Gpu,
    ] {
        let candidate = f.candidate();
        let error = f.reload(candidate, Some(stage)).unwrap_err();
        assert!(format!("{error:#}").contains("session 2"));
        assert_eq!(f.accepted, before);
        assert_eq!(f.policy.font_settings, before.font);
        assert_eq!(f.policy.metrics, metrics);
        assert_eq!(f.policy.appearance.palette(), before.colors);
        assert_eq!(f.policy.shell_command, ShellCommand::default());
        for (index, (_, terminal, bridge)) in targets.iter().enumerate() {
            let mut terminal = terminal.lock().unwrap();
            assert_eq!(terminal.snapshot(), snapshots[index]);
            assert_eq!(
                *bridge.renderer().unwrap().lock().unwrap().metrics(),
                metrics
            );
            assert!(terminal.projection_requires_full_update());
            terminal.put_str(""); // Locks and input remain usable after guard cleanup.
        }
    }
    let candidate = f.candidate();
    f.reload(candidate.clone(), None).unwrap();
    assert_eq!(f.accepted, candidate);
    assert_eq!(f.policy.font_settings, candidate.font);
    assert_ne!(f.policy.metrics, metrics);
    for (_, terminal, bridge) in targets {
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            f.policy.metrics
        );
        assert_eq!(
            terminal.lock().unwrap().snapshot().cols,
            f.tabs.last_broadcast_size().unwrap().cols
        );
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn acceptance_updates_inactive_renderers_osc_baselines_and_new_session_policy() {
    let mut f = Fixture::new();
    let ids: Vec<_> = f
        .tabs
        .configuration_targets()
        .iter()
        .map(|(id, _, _)| *id)
        .collect();
    let candidate = f.candidate();
    let result = f.reload(candidate.clone(), None).unwrap();
    assert!(result.contains("applied live: colors, keybindings, terminal fonts"));
    assert!(result.contains("new sessions only: shell program/arguments"));
    assert!(result.contains("restart required: clipboard.osc52_write"));
    assert_eq!(f.clipboard.osc52_write, Osc52WritePolicy::Allow);
    assert_eq!(f.policy.shell_command, configured_shell(&candidate.shell));
    for (index, (id, terminal, bridge)) in f.tabs.configuration_targets().iter().enumerate() {
        assert_eq!(*id, ids[index]);
        let mut terminal = terminal.lock().unwrap();
        let update = terminal.read_update(Instant::now());
        let expected = if index == 0 {
            Rgba::from_rgb8(17, 34, 51)
        } else {
            candidate.colors.foreground
        };
        assert_eq!(update.appearance.palette().foreground, expected);
        let renderer = bridge.renderer().unwrap();
        let mut renderer = renderer.lock().unwrap();
        assert_eq!(*renderer.metrics(), f.policy.metrics);
        assert!(renderer.prepare(
            TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
            &update,
            false
        ));
        terminal.put_str("\x1b]110\x07");
        assert_eq!(
            terminal
                .read_update(Instant::now())
                .appearance
                .palette()
                .foreground,
            candidate.colors.foreground
        );
    }
    let (mut new_terminal, new_renderer) = f
        .policy
        .prepare_new_terminal(
            TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
            load_system_fonts(&f.policy.font_settings).unwrap(),
            f.tabs.last_broadcast_size().unwrap(),
            (640, 320),
            false,
            harbor_terminal::ClipboardDelivery::Discard,
        )
        .unwrap();
    assert_eq!(
        new_terminal
            .read_update(Instant::now())
            .appearance
            .palette(),
        candidate.colors
    );
    assert_eq!(*new_renderer.metrics(), f.policy.metrics);
    new_terminal.put_str("\x1b]52;c;aGVsbG8=\x07");
    assert!(new_terminal.drain_output_events().is_empty());
    f.reload(Settings::default(), None).unwrap();
    assert_eq!(f.accepted, Settings::default());
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn unchanged_binding_launch_and_restart_only_acceptance_avoid_session_resources() {
    let mut f = Fixture::new();
    let targets = f.tabs.configuration_targets();
    let warning = "requested family unavailable; using system font".to_owned();
    f.policy.font_warnings = vec![warning.clone()];
    for (_, terminal, bridge) in &targets {
        poison(terminal);
        poison(&bridge.renderer().unwrap());
    }
    let mut candidates = vec![f.accepted.clone()];
    let mut bindings = f.accepted.clone();
    bindings
        .keybindings
        .entries
        .insert("app.new-tab".into(), vec![]);
    candidates.push(bindings.clone());
    let mut launch = bindings;
    launch.shell.program = Some("cmd.exe".into());
    launch.shell.args = vec!["/Q".into()];
    launch
        .shell
        .environment
        .insert("SYNTHETIC".into(), "literal".into());
    launch.shell.directory = Some(std::env::temp_dir());
    candidates.push(launch.clone());
    launch.clipboard.osc52_write = Osc52WritePolicy::Deny;
    candidates.push(launch);
    for candidate in candidates {
        let summary = f
            .reload_with(candidate.clone(), |_, stage| {
                panic!("unrelated transaction stage: {stage:?}")
            })
            .unwrap();
        assert!(summary.contains(&warning));
        assert_eq!(f.accepted, candidate);
        assert_eq!(f.policy.shell_command, configured_shell(&candidate.shell));
        assert_eq!(f.policy.font_warnings, std::slice::from_ref(&warning));
    }
    assert_eq!(f.clipboard.osc52_write, Osc52WritePolicy::Allow);
    let mut invalid = f.accepted.clone();
    invalid
        .keybindings
        .entries
        .insert("app.unknown".into(), vec![]);
    let before = f.accepted.clone();
    assert!(
        f.reload_with(invalid, |_, _| panic!(
            "invalid binding reached transaction"
        ))
        .is_err()
    );
    assert_eq!(f.accepted, before);
    assert_eq!(f.policy.shell_command, configured_shell(&before.shell));
    assert!(f.reload(before, None).unwrap().contains("restart required"));
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn color_only_prepares_all_palettes_atomically_without_renderer_or_resize_work() {
    let mut f = Fixture::new();
    let before = f.accepted.clone();
    let size = f.tabs.last_broadcast_size();
    let targets = f.tabs.configuration_targets();
    for (_, _, bridge) in &targets {
        poison(&bridge.renderer().unwrap());
    }
    let mut candidate = before.clone();
    candidate.colors.foreground = Rgba::from_rgb8(88, 99, 111);
    candidate.shell.program = Some("cmd.exe".into());
    assert!(
        f.reload(candidate.clone(), Some(ReloadStage::Palette))
            .is_err()
    );
    assert_eq!(f.accepted, before);
    assert_eq!(f.policy.appearance.palette(), before.colors);
    assert_eq!(f.policy.shell_command, ShellCommand::default());
    for (_, terminal, _) in &targets {
        let mut terminal = terminal.lock().unwrap();
        terminal.put_str("\x1b]110\x07");
        assert_eq!(
            terminal.read_update(Instant::now()).appearance.palette(),
            before.colors
        );
    }
    let mut stages = Vec::new();
    f.reload_with(candidate.clone(), |index, stage| {
        stages.push((index, stage));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        stages,
        [(0, ReloadStage::Palette), (1, ReloadStage::Palette)]
    );
    assert_eq!(f.accepted, candidate);
    assert_eq!(f.tabs.last_broadcast_size(), size);
    assert!(!f.policy.geometry_recovery_pending);
    for (_, terminal, _) in &targets {
        let terminal = terminal.lock().unwrap();
        assert_eq!(
            terminal.read_update(Instant::now()).appearance.palette(),
            candidate.colors
        );
        assert_eq!(
            (terminal.screen().rows(), terminal.screen().cols()),
            (size.unwrap().rows, size.unwrap().cols)
        );
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn background_opacity_reload_updates_existing_sessions_and_future_policy() {
    let mut f = Fixture::new();
    let targets = f.tabs.configuration_targets();
    assert_ne!(f.tabs.active_id(), Some(targets[0].0));
    let size = f.tabs.last_broadcast_size();
    let metrics = f.policy.metrics;
    // Keep a background OSC override in an inactive alternate screen.
    targets[0]
        .1
        .lock()
        .unwrap()
        .put_str("\x1b[?1049h\x1b]11;#112233\x07");
    let path =
        std::env::temp_dir().join(format!("harbor-opacity-reload-{}.toml", std::process::id()));
    for (source, alpha) in [
        ("[colors]\nbackground_opacity = 0.8", 0.8),
        ("[colors]\nbackground = '#1E1E2E40'", 64.0 / 255.0),
        ("", 0.85),
    ] {
        std::fs::write(&path, source).unwrap();
        let loaded = harbor_config::load_candidate_from_path(&path);
        std::fs::remove_file(&path).unwrap();
        let loaded = loaded.unwrap();
        assert!(loaded.diagnostics.is_empty());
        let expected = loaded.settings.colors;
        assert_eq!(expected.background.components()[3], alpha);
        f.reload(loaded.settings, Some(ReloadStage::Font)).unwrap();
        assert_eq!(f.policy.appearance.palette(), expected);
        assert_eq!(f.tabs.last_broadcast_size(), size);
        assert_eq!(f.policy.metrics, metrics);
        for (index, (_, terminal, bridge)) in targets.iter().enumerate() {
            let mut terminal = terminal.lock().unwrap();
            let update = terminal.read_update(Instant::now());
            let background = if index == 0 {
                Rgba::new(17.0 / 255.0, 34.0 / 255.0, 51.0 / 255.0, alpha)
            } else {
                expected.background
            };
            assert_eq!(update.appearance.palette().background, background);
            assert_eq!(terminal.clear_rgba(true), background.components());
            assert_eq!(terminal.clear_rgba(false)[3], 1.0);
            let renderer = bridge.renderer().unwrap();
            assert!(renderer.lock().unwrap().prepare(
                TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
                &update,
                false
            ));
            if index == 0 {
                terminal.put_str("\x1b]111\x07");
                assert_eq!(terminal.clear_rgba(true), expected.background.components());
                terminal.put_str("\x1b]11;#112233\x07");
            }
        }
    }
    targets[0].1.lock().unwrap().put_str("\x1b[?1049l");
    assert_eq!(targets[0].1.lock().unwrap().snapshot().cell(0, 0).ch, 'r');
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn invalid_bindings_never_enter_real_transaction_and_colors_only_keep_grid() {
    let mut f = Fixture::new();
    let original = f.accepted.clone();
    let mut invalid = f.candidate();
    invalid
        .keybindings
        .entries
        .insert("app.does-not-exist".to_owned(), vec![]);
    assert!(f.reload(invalid, None).is_err());
    assert_eq!(f.accepted, original);
    let size = f.tabs.last_broadcast_size();
    let metrics = f.policy.metrics;
    let mut candidate = original;
    candidate.colors.background = Rgba::from_rgb8(4, 5, 6);
    f.reload(candidate.clone(), Some(ReloadStage::Font))
        .unwrap();
    assert_eq!(f.tabs.last_broadcast_size(), size);
    assert_eq!(f.policy.metrics, metrics);
    for (_, terminal, bridge) in f.tabs.configuration_targets() {
        assert_eq!(
            terminal
                .lock()
                .unwrap()
                .read_update(Instant::now())
                .appearance
                .palette()
                .background,
            candidate.colors.background
        );
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            metrics
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn same_grid_font_transition_replaces_metrics_without_pty_effect_or_ui_font_change() {
    let mut f = Fixture::new();
    // First establish the allocation-derived grid instead of the fixture's seed grid.
    let mut baseline = f.accepted.clone();
    baseline.font.size += 2.0;
    f.reload(baseline, None).unwrap();
    let size = f.tabs.last_broadcast_size().unwrap();
    let metrics = f.policy.metrics;
    let ui_metrics = *f._runtime.text_metrics();
    let before: Vec<_> = f
        .tabs
        .configuration_targets()
        .iter()
        .map(|(_, terminal, _)| terminal.lock().unwrap().snapshot())
        .collect();
    let mut candidate = f.accepted.clone();
    candidate.font.size += 0.0001;
    let measured =
        TextMetrics::from_font_metrics(load_system_fonts(&candidate.font).unwrap().font_metrics());
    assert_eq!(
        terminal_size_from_allocation(
            f.ui.latest_terminal_allocation().unwrap(),
            1.0,
            (640, 320),
            &measured
        ),
        Some(size)
    );
    assert_ne!(measured, metrics);
    f.reload_with(candidate.clone(), |_, stage| {
        assert!(
            !matches!(stage, ReloadStage::Pty(_)),
            "same grid must not manufacture a native resize"
        );
        Ok(())
    })
    .unwrap();
    f._runtime.update(Instant::now());
    assert_eq!(*f._runtime.text_metrics(), ui_metrics);
    assert_eq!(f.accepted, candidate);
    assert_eq!(f.policy.metrics, measured);
    for (index, (_, terminal, bridge)) in f.tabs.configuration_targets().iter().enumerate() {
        assert_eq!(
            terminal.lock().unwrap().snapshot().cells,
            before[index].cells
        );
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            measured
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn font_reload_accepts_bounded_eviction_and_resizes_active_and_parked_alternate() {
    let mut f = Fixture::new();
    let mut baseline = f.accepted.clone();
    baseline.font.size += 2.0;
    baseline.history.max_rows = 1000; // Explicit fixture, independent of shipping default.
    f.reload(baseline, None).unwrap();
    let targets = f.tabs.configuration_targets();
    let old_size = f.tabs.last_broadcast_size().unwrap();
    for (index, (_, terminal, _)) in targets.iter().enumerate() {
        let mut terminal = terminal.lock().unwrap();
        terminal.put_str("\x1b[2J\x1b[H");
        // Near the normal 1000-row history limit. Narrowing doubles long rows,
        // forcing ordinary oldest eviction rather than rejecting the candidate.
        for line in 0..980 {
            terminal.put_str(&format!(
                "old-{line:04}-{}\r\n",
                "x".repeat(old_size.cols.saturating_sub(10))
            ));
        }
        terminal.put_str("NEWEST-MARKER");
        assert!(retained_text(&terminal).contains("old-0000"));
        terminal.put_str("\x1b[?47h\x1b[H");
        terminal.put_str(&format!("ALT-{index}"));
        if index == 1 {
            terminal.put_str("\x1b[?47l");
        }
    }
    let mut candidate = f.accepted.clone();
    candidate.font.size *= 2.0;
    f.reload(candidate.clone(), None).unwrap();
    let new_size = f.tabs.last_broadcast_size().unwrap();
    assert!(new_size.cols < old_size.cols);
    assert_eq!(f.accepted, candidate);
    for (index, (_, terminal, bridge)) in targets.iter().enumerate() {
        let mut terminal = terminal.lock().unwrap();
        assert_eq!(terminal.is_alt_screen(), index == 0);
        if index == 1 {
            terminal.put_str("\x1b[?47h");
        }
        assert!(retained_text(&terminal).contains(&format!("ALT-{index}")));
        assert_eq!(
            (terminal.snapshot().rows, terminal.snapshot().cols),
            (new_size.rows, new_size.cols)
        );
        terminal.put_str("\x1b[?47l");
        let primary = retained_text(&terminal);
        assert!(
            primary.contains("NEWEST-MARKER"),
            "newest primary content lost"
        );
        assert!(
            !primary.contains("old-0000"),
            "normal reflow should evict oldest overflow"
        );
        assert!(terminal.screen().scroll_count() <= 1000);
        assert_eq!(
            (terminal.snapshot().rows, terminal.snapshot().cols),
            (new_size.rows, new_size.cols)
        );
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            f.policy.metrics
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn unavailable_family_accepts_with_warning_and_effective_fallback_metrics() {
    let mut f = Fixture::new();
    let ui_metrics = *f._runtime.text_metrics();
    let mut candidate = f.candidate();
    candidate.font.family = Some("Harbor Definitely Missing Font".to_owned());
    let fallback = load_system_fonts(&candidate.font).unwrap();
    let warning = fallback
        .selection_warning()
        .expect("missing family must be reported")
        .to_owned();
    let metrics = TextMetrics::from_font_metrics(fallback.font_metrics());
    assert!(metrics.cell_width > 0.0 && metrics.line_height > 0.0);
    let result = f.reload(candidate.clone(), None).unwrap();
    assert!(
        result.contains(&warning),
        "effective fallback must not be silently accepted: {result}"
    );
    let unchanged = f.reload(candidate.clone(), None).unwrap();
    assert!(
        unchanged.contains(&warning),
        "no-change reload must preserve actionable selection warning"
    );
    assert_eq!(f.accepted, candidate);
    assert_eq!(f.policy.metrics, metrics);
    assert_eq!(*f._runtime.text_metrics(), ui_metrics);
    let size = f.tabs.last_broadcast_size().unwrap();
    assert_eq!(
        terminal_size_from_allocation(
            f.ui.latest_terminal_allocation().unwrap(),
            1.0,
            (640, 320),
            &metrics
        ),
        Some(size)
    );
    for (_, terminal, bridge) in f.tabs.configuration_targets() {
        let terminal = terminal.lock().unwrap();
        assert_eq!(
            (terminal.snapshot().rows, terminal.snapshot().cols),
            (size.rows, size.cols)
        );
        let update = terminal.read_update(Instant::now());
        let renderer = bridge.renderer().unwrap();
        let mut renderer = renderer.lock().unwrap();
        assert_eq!(*renderer.metrics(), metrics);
        assert!(renderer.prepare(
            TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
            &update,
            false
        ));
    }
    candidate.font.family = None;
    f.reload(candidate, None).unwrap();
    assert!(
        f.policy.font_warnings.is_empty(),
        "successful font correction clears stale warning"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn history_manual_reload_is_atomic_across_hidden_saved_primary_and_future_sessions() {
    let mut f = Fixture::new();
    let targets = f.tabs.configuration_targets();
    for (index, (_, terminal, _)) in targets.iter().enumerate() {
        let mut terminal = terminal.lock().unwrap();
        terminal.put_str(&"history-marker\r\n".repeat(25));
        if index == 1 {
            terminal.put_str("\x1b[?1049hALT");
        }
    }
    let before: Vec<_> = targets
        .iter()
        .map(|(_, t, _)| t.lock().unwrap().screen().history_content_bytes())
        .collect();
    assert!(before.iter().all(|b| *b > 20));
    let mut candidate = f.candidate();
    candidate.history = harbor_config::HistoryLimits {
        max_rows: 2,
        max_bytes: 20,
    };
    assert!(
        f.reload(candidate.clone(), Some(ReloadStage::Model))
            .is_err()
    );
    assert_eq!(f.policy.history, f.accepted.history);
    for (index, (_, t, _)) in targets.iter().enumerate() {
        assert_eq!(
            t.lock().unwrap().screen().history_content_bytes(),
            before[index]
        );
    }
    let summary = f.reload(candidate.clone(), None).unwrap();
    assert!(summary.contains("history"));
    assert_eq!(f.accepted, candidate);
    assert_eq!(f.policy.history, candidate.history);
    let counts: Vec<_> = targets
        .iter()
        .map(|(_, t, _)| {
            let mut t = t.lock().unwrap();
            assert_eq!(t.screen().history_limits(), candidate.history);
            assert!(t.screen().history_content_bytes() <= 20);
            t.put_str("\x1b[?1049l");
            assert!(t.screen().scroll_count() <= 2);
            t.screen().history_content_bytes()
        })
        .collect();
    let mut growth = candidate.clone();
    growth.history = harbor_config::HistoryLimits::default();
    f.reload(growth, None).unwrap();
    for (index, (_, t, _)) in targets.iter().enumerate() {
        assert_eq!(
            t.lock().unwrap().screen().history_content_bytes(),
            counts[index]
        );
    }
    let (new, _) = f
        .policy
        .prepare_new_terminal(
            TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
            load_system_fonts(&f.policy.font_settings).unwrap(),
            f.tabs.last_broadcast_size().unwrap(),
            (640, 320),
            false,
            harbor_terminal::ClipboardDelivery::Discard,
        )
        .unwrap();
    assert_eq!(new.screen().history_limits(), f.accepted.history);
    for ((id, original, _), (current_id, current, _)) in
        targets.iter().zip(f.tabs.configuration_targets())
    {
        assert_eq!(*id, current_id);
        assert!(
            Arc::ptr_eq(original, &current),
            "sessions must not be recreated"
        );
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn history_watched_path_rejects_missing_malformed_and_invalid_then_accepts_shrink() {
    use crate::configuration_watch::{ConfigurationWatch, WatchEvent};
    let mut f = Fixture::new();
    let targets = f.tabs.configuration_targets();
    for (_, t, _) in &targets {
        t.lock().unwrap().put_str(&"old-marker\r\n".repeat(25));
    }
    let before: Vec<_> = targets
        .iter()
        .map(|(_, t, _)| t.lock().unwrap().screen().history_content_bytes())
        .collect();
    let path =
        std::env::temp_dir().join(format!("harbor-history-watch-{}.toml", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let apply_path = |f: &mut Fixture| {
        crate::shell::reload_configuration_from_path(
            Some(&path),
            &mut f.accepted,
            &f.clipboard,
            |settings, bindings| {
                let result = f.policy.apply(
                    &mut f.tabs,
                    &f.ui,
                    TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
                    settings,
                    1.0,
                    (640, 320),
                    |_, _| Ok(()),
                )?;
                f.ui.publish_keybindings(bindings);
                Ok(result)
            },
        )
    };
    assert!(apply_path(&mut f).unwrap().is_none());
    for source in [
        "[history",
        "[history]\nmax_rows = 0\n[font]\nsize = -1",
        "[history]\nmax_bytes = -1",
    ] {
        std::fs::write(&path, source).unwrap();
        assert!(apply_path(&mut f).is_err());
        assert_eq!(f.policy.history, harbor_config::HistoryLimits::default());
        for (index, (_, t, _)) in targets.iter().enumerate() {
            assert_eq!(
                t.lock().unwrap().screen().history_content_bytes(),
                before[index]
            );
        }
    }
    let (send, recv) = std::sync::mpsc::channel();
    let watch =
        ConfigurationWatch::start(Some(path.clone()), move || send.send(()).is_ok()).unwrap();
    std::fs::write(&path, "[history]\nmax_rows = 1\nmax_bytes = 10").unwrap();
    recv.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(matches!(watch.take_pending(), Some(WatchEvent::Reload)));
    assert!(
        apply_path(&mut f)
            .unwrap()
            .unwrap()
            .summary
            .contains("history")
    );
    assert_eq!(f.policy.history, f.accepted.history);
    for (_, t, _) in targets {
        assert!(t.lock().unwrap().screen().history_content_bytes() <= 10);
    }
    drop(watch);
    std::fs::remove_file(path).unwrap();
}
