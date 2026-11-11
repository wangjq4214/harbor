use super::*;
use harbor_config::{AgentInvocation, SessionProfile, ShellSettings};
fn profile(environment: ProjectEnvironment, directory: String, program: &str) -> SessionProfile {
    SessionProfile {
        name: "Synthetic project".into(),
        launch: ShellSettings {
            program: Some(environment.program().into()),
            ..Default::default()
        },
        project: Some(ProjectRecipe {
            environment,
            directory,
            agent: Some(AgentInvocation {
                program: program.into(),
                args: vec![
                    "".into(),
                    "a b".into(),
                    "'\"quoted".into(),
                    "C:\\space path\\".into(),
                    "slash\\\"quote".into(),
                    "世界🙂".into(),
                    "$(echo not-code); &".into(),
                    "--resume".into(),
                ],
            }),
        }),
    }
}
#[test]
fn project_launch_uses_explicit_environment_and_literal_boundary_templates() {
    for environment in [
        ProjectEnvironment::Wsl {
            distribution: "Ubuntu".into(),
        },
        ProjectEnvironment::Ssh {
            destination: "dev@unreachable.invalid".into(),
        },
    ] {
        for agent in ["pi", "codex", "claude", "opencode"] {
            let profile = profile(environment.clone(), "/absent/project ' 名".into(), agent);
            let recipe = profile.project.as_ref().unwrap();
            let command = resolve(&profile, recipe).unwrap(); // Deliberately no filesystem or network probe.
            assert_eq!(command.executable_policy, ExecutablePolicy::Strict);
            assert!(command.directory.is_none());
            let script = bash_script(recipe);
            assert!(script.starts_with("cd -- '/absent/project '"));
            assert!(script.contains(&format!("command '{}' '' 'a b'", agent)));
            assert!(script.contains("'--resume'"));
            match &environment {
                ProjectEnvironment::Wsl { .. } => assert_eq!(command.args.last().unwrap(), &script),
                ProjectEnvironment::Ssh { .. } => assert_eq!(
                    command.args.last().unwrap(),
                    &format!("bash -lc {}", posix_quote(&script))
                ),
                _ => unreachable!(),
            }
            assert!(!format!("{command:?}").contains("not-code"));
        }
    }
    assert_eq!(posix_quote("a'b"), "'a'\"'\"'b'");
    assert_eq!(pwsh_quote("a'b"), "'a''b'");
}

#[cfg(windows)]
mod pty {
    use super::*;
    use std::{
        fs,
        time::{Duration, Instant},
    };
    #[test]
    #[ignore = "pty: Windows ConPTY, pwsh 7.3+, build harbor-pty launch_probe example first"]
    fn project_pwsh_relaunch_delivers_original_argv_to_each_supported_executable() {
        let probe = crate::test_launch_probe::launch_probe();
        let root = tempfile::tempdir().unwrap();
        let project_dir = root.path().join("project ' 世界");
        fs::create_dir(&project_dir).unwrap();
        // Temp paths may use Windows 8.3 aliases; PowerShell expands them.
        let canonical = fs::canonicalize(&project_dir).unwrap();
        let project_dir =
            std::path::PathBuf::from(canonical.to_str().unwrap().trim_start_matches(r"\\?\"));
        let inherited_cwd = std::env::current_dir().unwrap();
        for agent in ["pi", "codex", "claude", "opencode"] {
            fs::copy(&probe, root.path().join(format!("{agent}.exe"))).unwrap();
            let report = root.path().join(format!("{agent}.txt"));
            let mut profile = profile(
                ProjectEnvironment::LocalPwsh,
                project_dir.to_string_lossy().into_owned(),
                agent,
            );
            profile.launch.environment.insert(
                "PATH".into(),
                format!(
                    "{};{}",
                    root.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            );
            profile.launch.environment.insert(
                "HARBOR_PROFILE_REPORT".into(),
                report.to_string_lossy().into_owned(),
            );
            let recipe = profile.project.as_ref().unwrap();
            let (_, loaded) =
                harbor_config::append_profile(&root.path().join("config.toml"), "", &profile)
                    .unwrap();
            assert_eq!(
                loaded
                    .settings
                    .profiles
                    .values()
                    .next()
                    .unwrap()
                    .project
                    .as_ref(),
                Some(recipe)
            );
            let intent = harbor_app::profiles::ProfileLaunchIntent {
                profile: loaded.settings.profiles.keys().next().unwrap().clone(),
                source: None,
                directory: None,
            };
            let command =
                crate::profile_launch::resolve(&loaded.settings.profiles, &intent, None).unwrap();
            let endpoints = harbor_pty::PtyEndpoints::spawn_shell(
                harbor_pty::TerminalSize {
                    rows: 24,
                    cols: 120,
                },
                &command,
            )
            .unwrap();
            let mut terminal = harbor_terminal::Terminal::new_headless(24, 120)
                .start_session_from_endpoints(endpoints, || true)
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            let text = loop {
                terminal.drain_pty();
                if let Ok(text) = fs::read_to_string(&report) {
                    break text;
                }
                assert!(
                    Instant::now() < deadline,
                    "synthetic project agent did not publish its report"
                );
                std::thread::sleep(Duration::from_millis(20));
            };
            terminal.drain_output_events();
            assert_eq!(terminal.project_snapshot().unwrap().agent, recipe.agent);
            assert!(text.contains(&format!("args:{:?}", recipe.agent.as_ref().unwrap().args)));
            assert!(
                text.contains(&format!("directory:{project_dir:?}")),
                "expected {project_dir:?}, observed {:?}",
                text.lines().find(|l| l.starts_with("directory:"))
            );
            drop(terminal); // Production PTY job teardown owns all synthetic children.
        }
        assert_eq!(std::env::current_dir().unwrap(), inherited_cwd);
    }
}
