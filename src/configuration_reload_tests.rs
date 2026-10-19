//! Real application transaction tests without a native Window/EventLoopProxy.
//! Only pre-effect rejection is mocked; native PTY effects, barriers and GPU preparation are real.
use super::*;
use harbor_config::{Osc52WritePolicy, RawKeybinding, Rgba, Settings};
use harbor_widget::{renderer::Viewport, runtime::Runtime};
use std::{sync::atomic::AtomicBool, time::Instant};

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    tabs: TabManager,
    ui: WorkspaceUiController,
    policy: TerminalConfiguration,
    accepted: Settings,
    clipboard: harbor_config::ClipboardSettings,
    _runtime: Runtime,
}

impl Fixture {
    fn new() -> Self {
        // Fail rather than silently count an unavailable GPU as a passing test.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("reload test GPU adapter");
        eprintln!("reload test adapter: {:?}", adapter.get_info());
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .expect("reload test GPU device");
        let accepted = Settings::default();
        let metrics = TextMetrics::from_font_metrics(
            load_system_fonts(&accepted.font).unwrap().font_metrics(),
        );
        let policy = TerminalConfiguration {
            geometry_recovery_pending: false,
            shell_command: ShellCommand::default(),
            font_settings: accepted.font.clone(),
            font_warnings: Vec::new(),
            metrics,
            appearance: TerminalAppearance::from_palette(accepted.colors),
        };
        let mut tabs = TabManager::new();
        for index in 0..2 {
            tabs.create_tab(|_, draw| {
                let (mut terminal, renderer) = policy.prepare_new_terminal(
                    TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm),
                    load_system_fonts(&policy.font_settings)?,
                    TerminalSize { rows: 10, cols: 40 },
                    (640, 320),
                    false,
                    harbor_terminal::ClipboardDelivery::Discard,
                )?;
                terminal.put_str("retained prompt");
                if index == 0 {
                    terminal.put_str("\x1b]10;#112233\x07");
                }
                #[allow(clippy::arc_with_non_send_sync)]
                let terminal = Arc::new(Mutex::new(terminal));
                let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
                    draw,
                    terminal.clone(),
                    renderer,
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(|_| {}),
                );
                Ok(TerminalTabResources::new(
                    terminal,
                    bridge,
                    "existing-child".to_owned(),
                ))
            })
            .unwrap();
        }
        tabs.accept_configuration_size(TerminalSize { rows: 10, cols: 40 });
        let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 640.0);
        ui.publish_keybindings(
            harbor_app::command::validate_keybindings(&accepted.keybindings).unwrap(),
        );
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(640, 320, 1.0));
        runtime.set_root(harbor_app::workspace::tab_workspace(ui.clone(), false));
        runtime.update(Instant::now());
        assert!(ui.latest_terminal_allocation().is_some());
        let clipboard = accepted.clipboard.clone();
        Self {
            device,
            queue,
            tabs,
            ui,
            policy,
            accepted,
            clipboard,
            _runtime: runtime,
        }
    }

    fn reload(&mut self, candidate: Settings, fail: Option<ReloadStage>) -> anyhow::Result<String> {
        self.reload_with(candidate, |index, stage| {
            if index == 1 && fail == Some(stage) {
                anyhow::bail!("injected later-session {stage:?} failure");
            }
            Ok(())
        })
    }

    fn reload_with(
        &mut self,
        candidate: Settings,
        preflight: impl FnMut(usize, ReloadStage) -> anyhow::Result<()>,
    ) -> anyhow::Result<String> {
        crate::shell::accept_configuration(
            &mut self.accepted,
            &self.clipboard,
            candidate,
            |settings, bindings| {
                let warnings = self.policy.apply(
                    &mut self.tabs,
                    &self.ui,
                    TerminalGpuAccess::new(
                        &self.device,
                        &self.queue,
                        wgpu::TextureFormat::Rgba8Unorm,
                    ),
                    settings,
                    1.0,
                    (640, 320),
                    preflight,
                )?;
                self.ui.publish_keybindings(bindings);
                Ok(warnings)
            },
        )
        .map(|accepted| accepted.summary)
    }

    fn candidate(&self) -> Settings {
        let mut candidate = self.accepted.clone();
        candidate.font.size += 4.0;
        candidate.colors.foreground = Rgba::from_rgb8(44, 55, 66);
        candidate.shell.program = Some("cmd.exe".to_owned());
        candidate.shell.args = vec!["/Q".to_owned()];
        candidate
            .shell
            .environment
            .insert("HARBOR_RELOAD_TEST".into(), "synthetic-new".into());
        candidate.shell.directory = Some(std::env::temp_dir());
        candidate.clipboard.osc52_write = Osc52WritePolicy::Deny;
        candidate.keybindings.entries.insert(
            "app.reload-configuration".to_owned(),
            vec![RawKeybinding {
                modifiers: vec!["ctrl".to_owned(), "shift".to_owned()],
                key: "g".to_owned(),
            }],
        );
        candidate
    }
}

#[test]
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

fn poison<T>(resource: &Mutex<T>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = resource.lock().unwrap();
        panic!("intentional unrelated-resource poison");
    }));
    assert!(result.is_err());
}

#[test]
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
fn retained_text(terminal: &Terminal) -> String {
    let screen = terminal.screen();
    screen.selected_text(harbor_terminal::SelectionBounds {
        start_row: screen.history_start(),
        start_col: 0,
        end_row: screen.history_start() + (screen.scroll_count() + screen.rows()) as u64 - 1,
        end_col: screen.cols() - 1,
    })
}

#[cfg(windows)]
impl Fixture {
    fn assert_reload_binding(&mut self, key: char) {
        use harbor_widget::input::event::{Key, KeyboardEvent, Modifiers, UiEvent};
        self._runtime.update(Instant::now());
        self._runtime.request_focus(&self.ui.terminal_focus());
        let modifiers = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        };
        self._runtime
            .dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Character(key),
                modifiers,
            }));
        let actions = self.ui.drain_actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].command,
            harbor_app::command::AppCommand::ReloadConfiguration
        );
        self._runtime
            .dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
                key: Key::Character(key),
                modifiers,
            }));
        assert!(self.ui.drain_actions().is_empty());
    }

    fn attach_native_children(&mut self) {
        let size = self.tabs.last_broadcast_size().unwrap();
        let commands = [
            ShellCommand::new(Some("cmd.exe".to_owned()), vec!["/D".into(), "/Q".into()]),
            ShellCommand::new(Some("pwsh.exe".to_owned()), vec!["-NoLogo".into(), "-NoProfile".into(), "-Command".into(),
                "[Console]::WriteLine('READY1');$null=[Console]::ReadLine();[Console]::WriteLine('RESUMED1');$null=[Console]::ReadLine();[Console]::WriteLine('RETRIED1');$null=[Console]::ReadLine()".into()]),
        ];
        for ((_, terminal, _), command) in self.tabs.configuration_targets().iter().zip(commands) {
            let endpoints = harbor_pty::PtyEndpoints::spawn_shell(
                harbor_pty::TerminalSize {
                    rows: size.rows,
                    cols: size.cols,
                },
                &command,
            )
            .unwrap();
            // Do not count platform shell fallback as PowerShell coverage.
            assert_eq!(
                endpoints.shell_name().to_ascii_lowercase(),
                if command.program.as_deref() == Some("cmd.exe") {
                    "cmd"
                } else {
                    "pwsh"
                }
            );
            let mut terminal = terminal.lock().unwrap();
            let model = std::mem::replace(&mut *terminal, Terminal::new_headless(1, 2));
            *terminal = model
                .start_session_from_endpoints(endpoints, || true)
                .unwrap();
            if command.program.as_deref() == Some("cmd.exe") {
                // cmd's interactive transport echoes input; split marker spellings in
                // the command so only output (not the echoed request) can match.
                terminal.write_pty(b"@echo off\r echo REA^DY0&set /p a=&echo RESU^MED0&set /p a=&echo RETR^IED0&set /p a=\r").unwrap();
            }
        }
        self.wait_for_native_markers("READY");
    }

    fn wait_for_native_markers(&mut self, prefix: &str) {
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let mut ready = true;
            for (index, (_, terminal, _)) in self.tabs.configuration_targets().iter().enumerate() {
                let mut terminal = terminal.lock().unwrap();
                terminal.drain_pty();
                let text = retained_text(&terminal);
                let marker = format!("{prefix}{index}");
                let count = text.matches(&marker).count();
                assert!(count <= 1, "duplicate native marker: {text:?}");
                ready &= count == 1;
                assert!(
                    !terminal.is_session_closed(),
                    "dedicated child exited prematurely: {text:?}"
                );
                assert!(
                    Instant::now() < deadline,
                    "reader/child deadline: {marker}: {text:?}"
                );
            }
            if ready {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn native_input(&mut self) {
        for (_, terminal, _) in self.tabs.configuration_targets() {
            terminal.lock().unwrap().write_pty(b"continue\r").unwrap();
        }
    }
}

#[cfg(windows)]
fn native_group_rejection_and_retry(compensation_fails: bool) {
    let mut f = Fixture::new();
    f.attach_native_children();
    f.assert_reload_binding('r');
    let accepted = f.accepted.clone();
    let shell = f.policy.shell_command.clone();
    let metrics = f.policy.metrics;
    let old_size = f.tabs.last_broadcast_size().unwrap();
    let targets = f.tabs.configuration_targets();
    let before: Vec<_> = targets
        .iter()
        .map(|(_, terminal, bridge)| {
            let terminal = terminal.lock().unwrap();
            let renderer = bridge.renderer().unwrap();
            let projection = renderer.lock().unwrap();
            (
                terminal.read_update(Instant::now()).appearance.palette(),
                renderer.clone(),
                projection.viewport(),
                projection.text.presentation_stats().native,
            )
        })
        .collect();
    let candidate = f.candidate();
    let mut effects = Vec::new();
    let error = f
        .reload_with(candidate.clone(), |index, stage| {
            if let ReloadStage::Pty(size) = stage {
                effects.push((index, size));
                if index == 1 && size != old_size {
                    anyhow::bail!("mocked later native pre-effect rejection");
                }
                if compensation_fails && index == 0 && size == old_size {
                    anyhow::bail!("mocked compensation pre-effect rejection");
                }
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(effects.len(), 3);
    let new_size = effects[0].1;
    assert_ne!(new_size, old_size);
    assert_eq!(effects, vec![(0, new_size), (1, new_size), (0, old_size)]);
    let diagnostic = format!("{error:#}");
    assert!(
        diagnostic.contains("session 2: PTY resize failed"),
        "{diagnostic}"
    );
    assert_eq!(
        diagnostic.contains("session 1: recovery failed"),
        compensation_fails,
        "{diagnostic}"
    );
    assert_eq!(
        diagnostic.contains("model reconciled to last successful PTY geometry"),
        compensation_fails
    );
    assert_eq!(
        diagnostic.contains("session needs recovery"),
        compensation_fails
    );
    assert_eq!(f.accepted, accepted); // Includes bindings, font, palette, shell and clipboard.
    f.assert_reload_binding('r'); // Actual UI table, not just accepted raw settings.
    assert_eq!(f.clipboard, accepted.clipboard);
    assert_eq!(f.policy.font_settings, accepted.font);
    assert_eq!(f.policy.metrics, metrics);
    assert_eq!(f.policy.appearance.palette(), accepted.colors);
    assert_eq!(f.policy.shell_command, shell);
    assert_eq!(f.tabs.last_broadcast_size(), Some(old_size));
    for (index, (_, terminal, bridge)) in targets.iter().enumerate() {
        let terminal = terminal.lock().unwrap();
        let update = terminal.read_update(Instant::now());
        assert_eq!(update.appearance.palette(), before[index].0);
        let renderer = bridge.renderer().unwrap();
        assert!(
            Arc::ptr_eq(&renderer, &before[index].1),
            "projection owner identity changed"
        );
        let mut renderer = renderer.lock().unwrap();
        assert_eq!(*renderer.metrics(), metrics);
        assert_eq!(renderer.viewport(), before[index].2);
        assert_eq!(renderer.text.presentation_stats().native, before[index].3);
        let expected = if compensation_fails && index == 0 {
            new_size
        } else {
            old_size
        };
        assert_eq!(
            (update.snapshot.rows, update.snapshot.cols),
            (expected.rows, expected.cols)
        );
        // C4 replays reconciled geometry through the retained old-font renderer.
        assert!(renderer.prepare(
            TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
            &update,
            false
        ));
    }
    // These are actual blocked native readers resumed by ResizeBarrier::drop, not a fake resume.
    f.native_input();
    f.wait_for_native_markers("RESUMED");
    if compensation_fails {
        assert!(f.policy.geometry_recovery_pending);
        let mut recovery_stages = Vec::new();
        // The candidate is exactly the accepted settings, not the rejected font request.
        // ADR 0061 requires this retry to reconcile geometry rather than equality-skip.
        f.reload_with(accepted.clone(), |index, stage| {
            recovery_stages.push((index, stage));
            Ok(())
        })
        .unwrap();
        assert!(!f.policy.geometry_recovery_pending);
        assert!(recovery_stages.iter().all(|(_, stage)| !matches!(
            stage,
            ReloadStage::Font | ReloadStage::Metrics | ReloadStage::Gpu
        )));
        assert!(recovery_stages.contains(&(0, ReloadStage::Pty(old_size))));
        for (_, terminal, bridge) in &targets {
            let terminal = terminal.lock().unwrap();
            assert_eq!(
                (terminal.screen().rows(), terminal.screen().cols()),
                (old_size.rows, old_size.cols)
            );
            assert_eq!(
                *bridge.renderer().unwrap().lock().unwrap().metrics(),
                metrics
            );
        }
        assert_eq!(f.accepted, accepted);
    }
    let result = f.reload(candidate.clone(), None).unwrap();
    assert!(result.contains("applied live"));
    assert_eq!(f.accepted, candidate);
    f.assert_reload_binding('g');
    assert_eq!(f.policy.font_settings, candidate.font);
    assert_eq!(f.policy.appearance.palette(), candidate.colors);
    assert_ne!(f.policy.metrics, metrics);
    let size = f.tabs.last_broadcast_size().unwrap();
    for (_, terminal, bridge) in &targets {
        let terminal = terminal.lock().unwrap();
        assert_eq!(
            (terminal.snapshot().rows, terminal.snapshot().cols),
            (size.rows, size.cols)
        );
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            f.policy.metrics
        );
    }
    f.native_input();
    f.wait_for_native_markers("RETRIED");
    for (index, (_, terminal, _)) in targets.iter().enumerate() {
        let text = retained_text(&terminal.lock().unwrap());
        assert_eq!(text.matches(&format!("RESUMED{index}")).count(), 1);
        assert_eq!(text.matches(&format!("RETRIED{index}")).count(), 1);
    }
    // Terminal endpoint ownership tears down only these dedicated test children.
}

#[cfg(windows)]
#[test]
fn native_later_session_failure_compensates_resumes_and_retries() {
    native_group_rejection_and_retry(false);
}

#[cfg(windows)]
#[test]
fn native_compensation_failure_retains_policy_reconciles_resumes_and_retries() {
    native_group_rejection_and_retry(true);
}

#[cfg(windows)]
#[test]
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
fn font_reload_accepts_bounded_eviction_and_resizes_active_and_parked_alternate() {
    let mut f = Fixture::new();
    let mut baseline = f.accepted.clone();
    baseline.font.size += 2.0;
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

#[cfg(windows)]
#[path = "session_launch_tests.rs"]
mod session_launch_tests;

#[cfg(windows)]
#[path = "configuration_watch_tests.rs"]
mod configuration_watch_tests;
