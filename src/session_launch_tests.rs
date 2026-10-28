//! Actual Windows children through startup conversion and the real reload transaction.
#[path = "session_launch_tests/pty.rs"]
mod pty;

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
