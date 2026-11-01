//! Real application transaction tests without a native Window/EventLoopProxy.
//! Only pre-effect rejection is mocked; native PTY effects, barriers and GPU preparation are real.
#[path = "configuration_reload_tests/gpu.rs"]
mod gpu;
#[path = "configuration_reload_tests/pty.rs"]
mod pty;

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
        let (device, queue) = crate::test_gpu::device();
        let accepted = Settings::default();
        let metrics = TextMetrics::from_font_metrics(
            load_system_fonts(&accepted.font).unwrap().font_metrics(),
        );
        let policy = TerminalConfiguration {
            geometry_recovery_pending: false,
            shell_command: ShellCommand::default(),
            font_settings: accepted.font.clone(),
            history: accepted.history,
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
                let acquisition = if settings.font != self.policy.font_settings
                    || self.policy.geometry_recovery_pending
                {
                    Some(ConfigurationAcquisition::begin(&self.tabs)?)
                } else {
                    None
                };
                // Test driver only: the application yields between these polls.
                if let Some(acquisition) = &acquisition {
                    let deadline = Instant::now() + std::time::Duration::from_secs(3);
                    while !acquisition.poll(&self.tabs)? {
                        for (id, _, _) in self.tabs.configuration_targets() {
                            self.tabs.process_output(id);
                        }
                        anyhow::ensure!(
                            Instant::now() < deadline,
                            "configuration acquisition deadline"
                        );
                        std::thread::yield_now();
                    }
                }
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

fn poison<T>(resource: &Mutex<T>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = resource.lock().unwrap();
        panic!("intentional unrelated-resource poison");
    }));
    assert!(result.is_err());
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
                projection.presentation_stats().native,
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
        assert_eq!(renderer.presentation_stats().native, before[index].3);
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
#[path = "session_launch_tests.rs"]
mod session_launch_tests;

#[cfg(windows)]
#[path = "configuration_watch_tests.rs"]
mod configuration_watch_tests;
