use super::*;

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn native_launch_environment_directory_acceptance_and_recovery() {
    // Isolate a synthetic inherited variable in a separate test process; never
    // mutate the test runner's process-global environment or cwd.
    const GUARD: &str = "HARBOR_T0002_ISOLATED";
    if std::env::var_os(GUARD).is_none() {
        let name = concat!(
            module_path!(),
            "::native_launch_environment_directory_acceptance_and_recovery"
        );
        let name = name.split_once("::").unwrap().1;
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--ignored", "--nocapture"])
            .env(GUARD, "1")
            .env("HARBOR_T0002", "synthetic-inherited")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("T0002_NATIVE_PASS"),
            "child test must actually run: {stdout}"
        );
        eprintln!("{stdout}");
        return;
    }
    let workspace = Workspace::new();
    let inherited_cwd = std::env::current_dir().unwrap();
    let parent_environment = std::env::var("HARBOR_T0002").unwrap();
    let startup = workspace.settings(Some("synthetic-startup"), Some("relative space"));
    let loaded = harbor_config::load_from_path(workspace.0.join("config.toml"));
    assert_eq!(loaded.settings, startup);
    let mut old = Probe::spawn(
        configured_shell(&loaded.settings.shell),
        &workspace.0,
        0,
        false,
    );
    let first = old.read();
    assert_eq!(
        fs::canonicalize(&first[1]).unwrap(),
        fs::canonicalize(workspace.0.join("relative space")).unwrap()
    );
    assert_eq!(first[2], "value:synthetic-startup");
    assert_eq!(first[3], "$VAR %VAR% $(echo synthetic)");
    assert_eq!(first[4], "xterm-256color");

    old.terminal
        .put_str("\x1b]7;file:///C:/not-the-launch-directory\x07");
    let mut fixture = Fixture::new();
    let mut probes = Vec::new();
    for (index, value) in [Some("synthetic-override"), Some(""), None]
        .into_iter()
        .enumerate()
    {
        let absolute = workspace.0.join("absolute space");
        let directory = if index == 2 {
            None
        } else {
            Some(absolute.to_str().unwrap())
        };
        let candidate = workspace.settings(value, directory);
        let feedback = fixture.reload(candidate, None).unwrap();
        assert!(feedback.contains("new sessions only:"));
        assert!(feedback.contains("environment"));
        assert!(!feedback.contains("synthetic-override"));
        let mut probe = Probe::spawn(
            fixture.policy.shell_command.clone(),
            &workspace.0,
            index + 1,
            index == 0,
        );
        let report = probe.read();
        assert_ne!(report[0], first[0]);
        assert_eq!(
            report[2],
            format!("value:{}", value.unwrap_or("synthetic-inherited"))
        );
        assert_eq!(
            fs::canonicalize(&report[1]).unwrap(),
            fs::canonicalize(directory.map(Path::new).unwrap_or(&inherited_cwd)).unwrap()
        );
        let expected = report.clone();
        probes.push((probe, expected));
    }
    fs::create_dir(workspace.0.join("vanishing")).unwrap();
    let candidate = workspace.settings(Some("synthetic-recovered"), Some("vanishing"));
    fixture.reload(candidate, None).unwrap();
    fs::remove_dir(workspace.0.join("vanishing")).unwrap();
    let policy = fixture.policy.shell_command.clone();
    let error = match PtyEndpoints::spawn_shell(
        harbor_pty::TerminalSize {
            rows: 24,
            cols: 120,
        },
        &policy,
    ) {
        Ok(_) => panic!("lost accepted directory must fail, not launch in fallback cwd"),
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("shell.directory"));
    assert!(!format!("{error:#}").contains("synthetic-recovered"));
    assert_eq!(fixture.policy.shell_command, policy);
    for (probe, expected) in &mut probes {
        assert_eq!(
            probe.recheck(),
            *expected,
            "prior children must survive later generations and failed creation"
        );
    }
    assert_eq!(
        old.recheck(),
        first,
        "existing child PID, environment and cwd must survive reloads"
    );
    fs::create_dir(workspace.0.join("vanishing")).unwrap();
    let mut recovered = Probe::spawn(policy, &workspace.0, 9, false);
    assert_eq!(recovered.read()[2], "value:synthetic-recovered");
    assert_eq!(std::env::current_dir().unwrap(), inherited_cwd);
    assert_eq!(std::env::var("HARBOR_T0002").unwrap(), parent_environment);
    println!(
        "T0002_NATIVE_PASS: startup, override, empty, removal, literal, config-relative/absolute/inherited cwd, actual pwsh fallback, old PID/env/cwd, lost-directory failure and recovery"
    );
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn invalid_launch_candidates_never_enter_application_transaction() {
    let workspace = Workspace::new();
    let mut fixture = Fixture::new();
    let accepted = fixture.accepted.clone();
    let policy = fixture.policy.shell_command.clone();
    let snapshots: Vec<_> = fixture
        .tabs
        .configuration_targets()
        .iter()
        .map(|(id, terminal, _)| (*id, terminal.lock().unwrap().snapshot()))
        .collect();
    for invalid in [
        "environment = { Path = 'synthetic', PATH = 'conflict' }",
        "environment = { TERM = 'synthetic-invalid' }",
        "environment = { TOKEN = 42 }",
        "directory = 'missing'",
        "directory = 'config.toml'",
    ] {
        let path = workspace.0.join("config.toml");
        fs::write(
            &path,
            format!("[colors]\nforeground = '#123456'\n[shell]\nprogram = 'cmd.exe'\n{invalid}"),
        )
        .unwrap();
        let result = (|| -> anyhow::Result<()> {
            let loaded = harbor_config::load_candidate_from_path(&path)?;
            fixture.reload(loaded.settings, None)?;
            Ok(())
        })();
        assert!(result.is_err());
        assert_eq!(fixture.accepted, accepted);
        assert_eq!(fixture.policy.shell_command, policy);
        assert_eq!(fixture.policy.appearance.palette(), accepted.colors);
        for ((id, terminal, _), (before_id, before)) in
            fixture.tabs.configuration_targets().iter().zip(&snapshots)
        {
            assert_eq!(id, before_id);
            assert_eq!(terminal.lock().unwrap().snapshot(), *before);
        }
    }
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn package_manual_watched_generations_keep_live_resources_and_native_launches_coherent() {
    use crate::configuration_watch::{ConfigurationWatch, WatchEvent};
    use std::sync::mpsc;

    fn document(generation: usize) -> String {
        let (color, size, key, directory) = match generation {
            0 => ("#112233", 24, 'r', "relative space"),
            1 => ("#223344", 28, 'g', "absolute space"),
            _ => ("#334455", 26, 'r', "relative space"),
        };
        format!(
            "[font]\nfamily = 'Consolas'\nsize = {size}\n[colors]\nbackground = '{color}'\n\
             [shell]\nprogram = 'pwsh.exe'\nargs = ['-NoLogo', '-NoProfile']\ndirectory = '{directory}'\n\
             [shell.environment]\nHARBOR_T0002 = 'synthetic-generation-{generation}'\n\
             HARBOR_LITERAL = '$VAR %VAR% $(echo synthetic)'\nterm = 'xterm-256color'\n\
             [clipboard]\nosc52_write = 'deny'\n[keybindings.app.reload-configuration]\n\
             bindings = [{{ modifiers = ['ctrl', 'shift'], key = '{key}' }}]\n"
        )
    }
    fn observe(watch: &ConfigurationWatch, rx: &mpsc::Receiver<()>) {
        rx.recv_timeout(Duration::from_secs(8))
            .expect("package watch wake");
        assert!(matches!(watch.take_pending(), Some(WatchEvent::Reload)));
    }
    fn reload(f: &mut Fixture, path: &Path, fail: Option<ReloadStage>) -> anyhow::Result<String> {
        f.reload(
            harbor_config::load_candidate_from_path(path)?.settings,
            fail,
        )
    }
    fn assert_generation(
        f: &mut Fixture,
        workspace: &Workspace,
        index: usize,
        probe_id: usize,
    ) -> Probe {
        assert_eq!(
            f.accepted,
            harbor_config::load_candidate_from_path(workspace.0.join("config.toml"))
                .unwrap()
                .settings
        );
        assert_eq!(f.policy.shell_command, configured_shell(&f.accepted.shell));
        assert_eq!(f.policy.font_settings, f.accepted.font);
        assert_eq!(f.policy.appearance.palette(), f.accepted.colors);
        assert_eq!(f.clipboard, harbor_config::ClipboardSettings::default());
        f.assert_reload_binding(if index == 1 { 'g' } else { 'r' });
        for (_, terminal, bridge) in f.tabs.configuration_targets() {
            assert_eq!(
                terminal
                    .lock()
                    .unwrap()
                    .read_update(Instant::now())
                    .appearance
                    .palette()
                    .background,
                f.accepted.colors.background
            );
            assert_eq!(
                *bridge.renderer().unwrap().lock().unwrap().metrics(),
                f.policy.metrics
            );
        }
        // Actually construct future resources, not only inspect a settings snapshot.
        let (terminal, renderer) = f
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
            terminal.read_update(Instant::now()).appearance.palette(),
            f.accepted.colors
        );
        assert_eq!(*renderer.metrics(), f.policy.metrics);
        let mut probe = Probe::spawn(
            f.policy.shell_command.clone(),
            &workspace.0,
            probe_id,
            false,
        );
        let report = probe.read();
        assert_eq!(report[2], format!("value:synthetic-generation-{index}"));
        assert_eq!(report[3], "$VAR %VAR% $(echo synthetic)");
        assert_eq!(report[4], "xterm-256color");
        assert_eq!(
            fs::canonicalize(&report[1]).unwrap(),
            fs::canonicalize(f.accepted.shell.directory.as_ref().unwrap()).unwrap()
        );
        probe
    }

    let workspace = Workspace::new();
    let path = workspace.0.join("config.toml");
    let mut f = Fixture::new();
    f.attach_native_children();
    let original = f.tabs.configuration_targets();
    for (_, terminal, _) in &original {
        terminal
            .lock()
            .unwrap()
            .put_str("\x1b[?47h\x1b[HINTEGRATED-ALT");
    }
    // One active alternate, one parked alternate with primary retained.
    original[1].1.lock().unwrap().put_str("\x1b[?47l");
    fs::write(&path, document(0)).unwrap();
    let (tx, rx) = mpsc::channel();
    let watch = ConfigurationWatch::start(Some(path.clone()), move || tx.send(()).is_ok()).unwrap();
    observe(&watch, &rx);
    let feedback = reload(&mut f, &path, None).unwrap();
    assert!(feedback.contains("applied live:"));
    assert!(feedback.contains("new sessions only:"));
    assert!(feedback.contains("restart required: clipboard.osc52_write"));
    assert!(!feedback.contains("synthetic-generation"));
    let mut first = assert_generation(&mut f, &workspace, 0, 20);
    let first_report = first.read();

    // Ordinary save changes every applicable policy while existing children survive.
    fs::write(&path, document(1)).unwrap();
    observe(&watch, &rx);
    reload(&mut f, &path, None).unwrap();
    let mut second = assert_generation(&mut f, &workspace, 1, 21);
    let second_report = second.read();
    assert_ne!(first_report[0], second_report[0]);
    let accepted = f.accepted.clone();
    // Invalid complete candidates cannot publish even the otherwise valid live edits.
    for invalid in [
        "[font\nSYNTHETIC-CONFIDENTIAL",
        "[colors]\nbackground = '#abcdef'\n[shell.environment]\nTERM = 'SYNTHETIC-CONFIDENTIAL'",
        "[colors]\nbackground = '#abcdef'\n[shell]\ndirectory = 'missing'",
        "[colors]\nbackground = '#abcdef'\n[keybindings.app.reload-configuration]\nbindings = [{ key = 'bad-key' }]",
    ] {
        fs::write(&path, invalid).unwrap();
        observe(&watch, &rx);
        let error = format!("{:#}", reload(&mut f, &path, None).unwrap_err());
        assert!(!error.contains("SYNTHETIC-CONFIDENTIAL"));
        assert_eq!(f.accepted, accepted);
        assert_eq!(f.policy.shell_command, configured_shell(&accepted.shell));
        f.assert_reload_binding('g');
        for (_, terminal, bridge) in f.tabs.configuration_targets() {
            assert_eq!(
                terminal
                    .lock()
                    .unwrap()
                    .read_update(Instant::now())
                    .appearance
                    .palette()
                    .background,
                accepted.colors.background
            );
            assert_eq!(
                *bridge.renderer().unwrap().lock().unwrap().metrics(),
                f.policy.metrics
            );
        }
    }

    // Atomic replacement remains observed; preparation rejection retains launch/live state.
    let replacement = workspace.0.join("replacement.toml");
    fs::write(&replacement, document(2)).unwrap();
    fs::rename(&replacement, &path).unwrap();
    observe(&watch, &rx);
    for stage in [ReloadStage::Font, ReloadStage::Gpu] {
        assert!(reload(&mut f, &path, Some(stage)).is_err());
        assert_eq!(f.accepted, accepted);
        assert_eq!(f.policy.shell_command, configured_shell(&accepted.shell));
    }
    reload(&mut f, &path, None).unwrap(); // Manual retry, no new save required.
    let mut third = assert_generation(&mut f, &workspace, 2, 22);
    let third_report = third.read();

    // Hold an older wake while manually accepting a newer complete document.
    fs::write(&path, document(1)).unwrap();
    rx.recv_timeout(Duration::from_secs(8)).unwrap();
    fs::remove_file(&path).unwrap();
    fs::write(&path, "[font\nintermediate").unwrap();
    fs::write(&path, document(0)).unwrap();
    reload(&mut f, &path, None).unwrap();
    assert!(matches!(watch.take_pending(), Some(WatchEvent::Reload)));
    reload(&mut f, &path, None).unwrap();
    let mut final_probe = assert_generation(&mut f, &workspace, 0, 23);
    assert_eq!(final_probe.read()[2], "value:synthetic-generation-0");
    assert_eq!(first.recheck(), first_report);
    assert_eq!(second.recheck(), second_report);
    assert_eq!(third.recheck(), third_report);
    for (index, ((id, terminal, bridge), (old_id, old_terminal, _))) in f
        .tabs
        .configuration_targets()
        .iter()
        .zip(&original)
        .enumerate()
    {
        assert_eq!(id, old_id);
        assert!(Arc::ptr_eq(terminal, old_terminal));
        let mut terminal = terminal.lock().unwrap();
        assert_eq!(terminal.is_alt_screen(), index == 0);
        if index == 1 {
            terminal.put_str("\x1b[?47h");
        }
        assert!(retained_text(&terminal).contains("INTEGRATED-ALT"));
        terminal.put_str("\x1b[?47l");
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            f.policy.metrics
        );
    }
    f.native_input();
    f.wait_for_native_markers("RESUMED");
    println!(
        "T0004_PACKAGE_PASS: watched/manual all-field generations, active/parked alternate, native child env/cwd continuity, future resources, rejection, replacement, burst and stale-wake ordering"
    );
}
