//! Actual Windows children through startup conversion and the real reload transaction.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "harbor-launch-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("relative space")).unwrap();
        fs::create_dir_all(root.join("absolute space")).unwrap();
        Self(root)
    }
    fn settings(
        &self,
        environment: Option<&str>,
        directory: Option<&str>,
    ) -> harbor_config::Settings {
        let mut source = "[shell]\nprogram = 'pwsh.exe'\n".to_owned();
        if let Some(directory) = directory {
            source += &format!("directory = '{directory}'\n");
        }
        if let Some(value) = environment {
            source += &format!(
                "[shell.environment]\nHARBOR_T0002 = '{value}'\nHARBOR_LITERAL = '$VAR %VAR% $(echo synthetic)'\nterm = 'xterm-256color'\n"
            );
        }
        fs::write(self.0.join("config.toml"), source).unwrap();
        harbor_config::load_candidate_from_path(self.0.join("config.toml"))
            .unwrap()
            .settings
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Probe {
    terminal: Terminal,
    report: PathBuf,
    signal: PathBuf,
}
impl Probe {
    fn spawn(mut command: ShellCommand, root: &Path, index: usize, fallback: bool) -> Self {
        let report = root.join(format!("report-{index}.txt"));
        let signal = root.join(format!("signal-{index}"));
        let script_path = root.join(format!("probe-{index}.ps1"));
        // Values are read from the child environment, never embedded into shell code.
        let script = format!(
            r#"
function Report {{
    $v = [Environment]::GetEnvironmentVariable('HARBOR_T0002')
    if ($null -eq $v) {{ $v = '<missing>' }}
    [IO.File]::WriteAllLines('{}', [string[]]@([string]$PID, [Environment]::CurrentDirectory, ('value:' + $v), [Environment]::GetEnvironmentVariable('HARBOR_LITERAL'), [Environment]::GetEnvironmentVariable('TERM')))
}}
Report
while (!(Test-Path '{}')) {{ Start-Sleep -Milliseconds 20 }}
Report
while ($true) {{ Start-Sleep -Milliseconds 100 }}
"#,
            report.display().to_string().replace('\'', "''"),
            signal.display().to_string().replace('\'', "''")
        );
        fs::write(&script_path, script).unwrap();
        command.args = vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-File".into(),
            script_path.display().to_string(),
        ];
        if fallback {
            command.program = Some("harbor-definitely-missing-t0002.exe".into());
        }
        let endpoints = PtyEndpoints::spawn_shell(
            harbor_pty::TerminalSize {
                rows: 24,
                cols: 120,
            },
            &command,
        )
        .unwrap();
        assert_eq!(
            endpoints.shell_name().to_ascii_lowercase(),
            "pwsh",
            "this probe requires real PowerShell 7, not a disguised fallback"
        );
        let mut terminal = Terminal::new_headless(24, 120)
            .start_session_from_endpoints(endpoints, || true)
            .unwrap();
        if fallback {
            // Fallback intentionally drops configured arguments; run the probe in the
            // selected shell itself, not a nested child. No profile-dependent prompt assertion.
            let input = format!(
                "& '{}'\r",
                script_path.display().to_string().replace('\'', "''")
            );
            terminal.write_pty(input.as_bytes()).unwrap();
        }
        Self {
            terminal,
            report,
            signal,
        }
    }
    fn read(&mut self) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.terminal.drain_pty();
            if let Ok(text) = fs::read_to_string(&self.report) {
                let lines: Vec<_> = text.lines().map(str::to_owned).collect();
                if lines.len() == 5 {
                    return lines;
                }
            }
            assert!(!self.terminal.is_session_closed(), "probe exited");
            assert!(Instant::now() < deadline, "probe report deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn recheck(&mut self) -> Vec<String> {
        fs::remove_file(&self.report).unwrap();
        fs::write(&self.signal, "continue").unwrap();
        self.read()
    }
}

#[test]
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
            .args(["--exact", name, "--nocapture"])
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
