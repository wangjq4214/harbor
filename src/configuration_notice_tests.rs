//! Startup/manual/watched file-I/O and application acceptance policy, without a native window.
use super::*;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;

struct ConfigFile(PathBuf);
impl ConfigFile {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "harbor-notices-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory.join("config.toml"))
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn write(&self, source: &str) {
        std::fs::write(&self.0, source).unwrap();
    }
}
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir(self.0.parent().unwrap());
    }
}

#[test]
fn startup_and_manual_absence_are_silent_but_live_absence_never_commits_defaults() {
    let file = ConfigFile::new();
    let startup = harbor_config::load_from_path(file.path());
    assert_eq!(startup.settings, harbor_config::Settings::default());
    assert!(startup.diagnostics.is_empty());
    let mut accepted = harbor_config::Settings::default();
    accepted.font.size = 23.0;
    let baseline = accepted.clone();
    let clipboard = accepted.clipboard.clone();
    assert!(
        reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _| panic!("missing live candidate must not invoke application transaction")
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(accepted, baseline);
    let unresolved =
        reload_configuration_from_path(None, &mut accepted, &clipboard, |_, _| unreachable!());
    assert!(
        unresolved.is_err(),
        "unresolvable path is not missing-file silence"
    );
    file.write("[font]\nsize = 'wrong type'\n");
    let startup = harbor_config::load_from_path(file.path());
    assert!(
        !startup.diagnostics.is_empty(),
        "unapplied startup setting remains actionable"
    );
}

#[test]
fn manual_success_no_change_launch_changes_restart_and_ignored_setting_have_selective_notices() {
    let file = ConfigFile::new();
    let mut accepted = harbor_config::Settings::default();
    let clipboard = accepted.clipboard.clone();
    file.write(
        "[font]\nsize = 22\n[shell.environment]\nSYNTHETIC = 'never-disclose-notice-value'\n",
    );
    for _ in 0..2 {
        let result =
            reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _| {
                Ok(vec![])
            })
            .unwrap()
            .unwrap();
        assert!(result.notice.is_none());
        assert!(!result.summary.contains("never-disclose-notice-value"));
    }
    assert_eq!(accepted.font.size, 22.0);
    file.write("[clipboard]\nosc52_write = 'deny'\n");
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _| {
            Ok(vec![])
        })
        .unwrap()
        .unwrap();
    let notice = result.notice.unwrap();
    assert!(notice.contains("restart required"));
    assert!(!notice.contains("applied live") && !notice.contains("new sessions only"));
    file.write("mystery = true\n");
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _| {
            Ok(vec![])
        })
        .unwrap()
        .unwrap();
    assert!(result.notice.unwrap().contains("mystery"));
    file.write("");
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _| {
            Ok(vec![
                "requested font unavailable; system font selected".into(),
            ])
        })
        .unwrap()
        .unwrap();
    assert!(
        result
            .notice
            .unwrap()
            .contains("requested font unavailable")
    );
}

#[test]
fn invalid_parse_setting_read_and_resource_failures_remain_errors_without_partial_acceptance() {
    let file = ConfigFile::new();
    let mut accepted = harbor_config::Settings::default();
    accepted.font.size = 23.0;
    let baseline = accepted.clone();
    let clipboard = accepted.clipboard.clone();
    for source in [
        "[shell.environment]\nSECRET = 'never-disclose-notice-value'\n[invalid",
        "[font]\nsize = 25\n[colors]\nforeground = 'invalid'\n",
    ] {
        file.write(source);
        let error =
            reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _| {
                panic!("rejected document may not apply valid subset")
            })
            .err()
            .unwrap();
        let diagnostic = format!("{error:#}");
        assert!(!diagnostic.contains("never-disclose-notice-value"));
        assert!(diagnostic.contains(file.path().to_str().unwrap()));
        assert_eq!(accepted, baseline);
    }
    file.write("[font]\nsize = 25\n");
    let error =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _| {
            anyhow::bail!(
                "session 2: PTY resize failed; session 1: recovery failed; session needs recovery"
            )
        })
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("session 1: recovery failed"));
    assert!(
        error.is::<ConfigurationOperationFailure>(),
        "resource failures have a typed presentation lifetime independent of file absence"
    );
    assert_eq!(accepted, baseline);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _exclusive = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(file.path())
            .unwrap();
        let error = harbor_config::load_candidate_from_path(file.path()).unwrap_err();
        assert!(
            error.io_kind.is_some() && error.io_kind != Some(std::io::ErrorKind::NotFound),
            "sharing violation is an actionable read failure, not absence"
        );
        let denied =
            harbor_config::load_candidate_from_path(file.path().parent().unwrap()).unwrap_err();
        assert_eq!(denied.io_kind, Some(std::io::ErrorKind::PermissionDenied));
        assert!(
            reload_configuration_from_path(
                Some(file.path()),
                &mut accepted,
                &clipboard,
                |_, _| unreachable!()
            )
            .is_err()
        );
        assert!(
            !harbor_config::load_from_path(file.path())
                .diagnostics
                .is_empty()
        );
        assert_eq!(accepted, baseline);
    }
}

#[test]
fn actual_watcher_reload_handles_missing_invalid_recovery_and_success_without_success_feed() {
    use crate::configuration_watch::{ConfigurationWatch, ReloadFeedback, WatchEvent};
    let file = ConfigFile::new();
    let (sender, wakes) = mpsc::channel();
    let watch = ConfigurationWatch::start(Some(file.path().to_owned()), move || {
        sender.send(()).is_ok()
    })
    .unwrap();
    let next = || {
        wakes
            .recv_timeout(Duration::from_secs(10))
            .expect("real watcher wake");
        match watch.take_pending().expect("settled observation") {
            WatchEvent::Reload => {}
            other => panic!("unexpected {other:?}"),
        }
    };
    let mut accepted = harbor_config::Settings::default();
    accepted.font.size = 23.0;
    let clipboard = accepted.clipboard.clone();
    next(); // starts missing
    assert!(
        reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _| unreachable!()
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(accepted.font.size, 23.0);
    file.write("[font]\nsize = 'bad'\n");
    next();
    let error = reload_configuration_from_path(
        Some(file.path()),
        &mut accepted,
        &clipboard,
        |_, _| unreachable!(),
    )
    .err()
    .unwrap();
    let mut feedback = ReloadFeedback::default();
    assert!(feedback.failure(&format!("{error:#}"), true));
    assert!(!feedback.failure(&format!("{error:#}"), true));
    file.write("[font]\nsize = 24\n");
    next();
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _| {
            Ok(vec![])
        })
        .unwrap()
        .unwrap();
    feedback.clear();
    assert!(result.notice.is_none());
    assert_eq!(accepted.font.size, 24.0);
    std::fs::remove_file(file.path()).unwrap();
    next();
    assert!(
        reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _| unreachable!()
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(accepted.font.size, 24.0);
    file.write("[font]\nsize = 'bad'\n");
    next();
    let error = reload_configuration_from_path(
        Some(file.path()),
        &mut accepted,
        &clipboard,
        |_, _| unreachable!(),
    )
    .err()
    .unwrap();
    assert!(
        feedback.failure(&format!("{error:#}"), true),
        "same failure after recovery is a new episode"
    );
}

#[test]
fn application_configuration_consumers_do_not_reresolve_the_source() {
    let source = include_str!("shell.rs");
    assert!(!source.contains("harbor_config::default_config_path("));
    assert!(!source.contains("harbor_config::load()"));
    assert_eq!(source.matches("harbor_config::load_with_path()").count(), 1);
}

#[test]
fn portable_startup_manual_and_watched_reload_keep_one_source() {
    use crate::configuration_watch::{ConfigurationWatch, WatchEvent};
    const CHILD: &str = "HARBOR_PORTABLE_CONFIGURATION_PROBE";
    if std::env::var_os(CHILD).is_none() {
        let file = ConfigFile::new();
        file.write("[font]\nsize = 19\n");
        let directory = file.path().parent().unwrap();
        let executable = directory.join(format!(
            "harbor portable 配置{}",
            std::env::consts::EXE_SUFFIX
        ));
        let marker = directory.join("harbor.portable");
        std::fs::write(&marker, "").unwrap();
        let current_exe = std::env::current_exe().unwrap();
        std::fs::hard_link(&current_exe, &executable)
            .or_else(|_| std::fs::copy(&current_exe, &executable).map(|_| ()))
            .unwrap();
        // A child has its own executable location without touching the real home
        // configuration or putting a marker beside other concurrently running tests.
        let output = std::process::Command::new(&executable)
            .args([
                "--exact",
                "shell::configuration_notice_tests::portable_startup_manual_and_watched_reload_keep_one_source",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .current_dir(std::env::temp_dir())
            .output();
        std::fs::remove_file(&executable).unwrap();
        let _ = std::fs::remove_file(&marker);
        let output = output.unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "stdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stdout.contains("1 passed; 0 failed"),
            "probe did not run: {stdout}"
        );
        return;
    }

    let (path, loaded) = harbor_config::load_with_path();
    let expected = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("config.toml");
    assert_eq!(path.as_deref(), Some(expected.as_path()));
    assert_eq!(loaded.settings.font.size, 19.0);
    assert_eq!(harbor_config::load().settings, loaded.settings);
    let mut accepted = loaded.settings;
    let clipboard = accepted.clipboard.clone();
    let (sender, wakes) = mpsc::channel();
    let watch = ConfigurationWatch::start(path.clone(), move || sender.send(()).is_ok()).unwrap();
    let next = || {
        wakes.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(matches!(watch.take_pending(), Some(WatchEvent::Reload)));
    };
    let reload = |accepted: &mut harbor_config::Settings| {
        reload_configuration_from_path(path.as_deref(), accepted, &clipboard, |_, _| Ok(vec![]))
    };
    next(); // The initial observation must not substitute another source.
    reload(&mut accepted).unwrap();
    assert_eq!(accepted.font.size, 19.0);

    // Mode selection is fixed for this session, even if the marker is removed.
    std::fs::remove_file(expected.with_file_name("harbor.portable")).unwrap();
    std::fs::write(&expected, "[font]\nsize = 23\n").unwrap();
    next();
    reload(&mut accepted).unwrap();
    assert_eq!(accepted.font.size, 23.0);
    std::fs::write(&expected, "[font]\nsize = 25\n").unwrap();
    reload(&mut accepted).unwrap(); // Manual reload uses that same saved path.
    assert_eq!(accepted.font.size, 25.0);

    std::fs::remove_file(&expected).unwrap();
    next();
    assert!(reload(&mut accepted).unwrap().is_none());
    assert_eq!(accepted.font.size, 25.0);
    std::fs::write(&expected, "[font\nsize = 42\n").unwrap();
    next();
    let error = reload(&mut accepted).err().unwrap();
    assert!(error.to_string().contains(expected.to_str().unwrap()));
    assert_eq!(accepted.font.size, 25.0);
}

/// Exercises the same UI FIFO, one-shot Host admission and live I/O/publication seam
/// as the native drain. GPU/font effects are covered by configuration_reload_tests.
#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn palette_reload_fifo_applies_once_and_rejected_candidate_keeps_effective_bindings() {
    use harbor_app::{
        command::{AppCommandRequest, CommandSource, resolve_keybindings},
        tab_manager::{TabManager, TerminalTabResources},
        terminal_view::TerminalWidgetBridge,
        workspace::{WorkspaceUiController, tab_workspace},
    };
    use harbor_widget::{
        input::event::{Key, KeyboardEvent, Modifiers, UiEvent},
        renderer::Viewport,
        runtime::Runtime,
    };
    use std::{
        sync::{Arc, Mutex, atomic::AtomicBool},
        time::Instant,
    };
    let file = ConfigFile::new();
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| {
        let terminal = Arc::new(Mutex::new(harbor_terminal::Terminal::new_headless(4, 20)));
        let bridge =
            TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
        Ok(TerminalTabResources::new(terminal, bridge, "synthetic"))
    })
    .unwrap();
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(1000, 500, 1.0));
    runtime.set_root(tab_workspace(ui.clone(), false));
    runtime.update(Instant::now());
    let mut accepted = harbor_config::Settings::default();
    let clipboard = accepted.clipboard.clone();
    let mut effective = resolve_keybindings(&accepted.keybindings).keybindings;
    let mut applied = 0;
    for (source, should_accept) in [
        (
            "[keybindings.app.new-tab]\nbindings = [{ modifiers = ['ctrl'], key = 'n' }]\n",
            true,
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [{ modifiers = ['ctrl'], key = 'w' }]\n",
            false,
        ),
    ] {
        file.write(source);
        runtime.request_focus(&ui.terminal_focus());
        runtime.update(Instant::now());
        let mods = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        };
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('p'),
            modifiers: mods,
        }));
        assert_eq!(
            ui.drain_actions(),
            [AppCommandRequest::shortcut(AppCommand::OpenCommandPalette)]
        );
        runtime.request_focus(&ui.open_palette());
        runtime.update(Instant::now());
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Character('p'),
            modifiers: mods,
        }));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(
            "app.reload-configuration".into(),
        )));
        for _ in 0..2 {
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: Key::Enter,
                modifiers: Modifiers::default(),
            }));
        }
        let actions = ui.drain_actions();
        assert_eq!(actions.len(), 1);
        let request = actions[0];
        assert_eq!(request.command, AppCommand::ReloadConfiguration);
        assert!(matches!(request.source, CommandSource::Palette { .. }));
        ui.take_palette_request(request, &tabs, false).unwrap();
        let previous = effective.clone();
        let result = reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, bindings| {
                applied += 1;
                effective = bindings.clone();
                ui.publish_keybindings(bindings);
                Ok(vec![])
            },
        );
        assert_eq!(result.is_ok(), should_accept);
        if should_accept {
            assert_ne!(effective, previous);
            assert_eq!(applied, 1);
            ui.report_palette_result(request.command, Ok(()));
        } else {
            assert_eq!(effective, previous);
            assert_eq!(
                applied, 1,
                "rejected candidates cannot invoke effects/publication"
            );
            ui.report_palette_result(request.command, Err("Failed: conflicting bindings"));
        }
        assert!(ui.take_palette_request(request, &tabs, false).is_err());
        runtime.request_focus(&ui.terminal_focus());
        runtime.update(Instant::now());
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        }));
        assert!(ui.drain_actions().is_empty());
    }
}
