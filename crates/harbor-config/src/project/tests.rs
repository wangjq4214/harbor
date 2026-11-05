use super::*;
use crate::{SessionProfile, ShellSettings, append_profile, candidate_from_source};

fn recipe(kind: &str, target: &str) -> ProjectRecipe {
    ProjectRecipe {
        environment: ProjectEnvironment::new(kind, target).unwrap(),
        directory: if kind == "pwsh" {
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        } else {
            "/not-a-native-path/project ' 名".into()
        },
        agent: Some(AgentInvocation {
            program: "pi".into(),
            args: vec![
                "".into(),
                "a b".into(),
                "q'\"\\".into(),
                "名".into(),
                "--resume".into(),
            ],
        }),
    }
}
#[test]
fn complete_project_roundtrip_and_invalid_candidates_are_atomic() {
    let path = std::env::current_dir().unwrap().join("synthetic.toml");
    for (kind, target) in [("pwsh", ""), ("wsl", "Ubuntu"), ("ssh", "dev@host.invalid")] {
        for program in ["pi", "codex", "claude", "opencode"] {
            let mut project = recipe(kind, target);
            project.agent.as_mut().unwrap().program = program.into();
            let profile = SessionProfile {
                name: "Project".into(),
                launch: ShellSettings {
                    program: Some(project.environment.program().into()),
                    ..Default::default()
                },
                project: Some(project.clone()),
            };
            let (text, loaded) =
                append_profile(&path, "# preserved\n[font]\nsize=17\n", &profile).unwrap();
            assert!(text.starts_with("# preserved"));
            assert_eq!(
                loaded.settings.profiles.values().next().unwrap().project,
                Some(project)
            );
            assert_eq!(
                candidate_from_source(&path, &text)
                    .unwrap()
                    .settings
                    .profiles
                    .len(),
                1
            );
            let removed = candidate_from_source(&path, "[font]\nsize=19\n").unwrap();
            assert!(removed.settings.profiles.is_empty());
            // No partial settings are available on strict candidate rejection.
            for invalid in [
                text.replace("kind =", "unrecognized_kind ="),
                text.replace("agent_args =", "unknown_args ="),
                text.replace("--resume", "--token=synthetic-private"),
            ] {
                let failure = candidate_from_source(&path, &invalid).unwrap_err();
                assert!(!format!("{failure:?}").contains("synthetic-private"));
            }
        }
    }
}
#[test]
fn targets_paths_and_agent_commands_cannot_be_options_or_scripts() {
    for target in [
        "-oProxyCommand=anything",
        "host;cmd",
        "host name",
        "a\nb",
        "",
        "host:/path",
    ] {
        assert!(ProjectEnvironment::new("ssh", target).is_none());
    }
    for path in ["relative", "//host/share", "/tmp\nother"] {
        let mut r = recipe("ssh", "host");
        r.directory = path.into();
        assert!(r.validate().is_err());
    }
    let mut r = recipe("wsl", "Ubuntu");
    r.agent.as_mut().unwrap().program = "arbitrary-script".into();
    assert!(r.validate().is_err());
    let mut r = recipe("pwsh", "");
    r.directory = "\\\\server\\share".into();
    assert!(r.validate().is_err());
}
#[test]
fn protocol_is_bounded_secret_redacted_and_environment_scoped() {
    let hex = |s: &str| {
        s.as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let event = |action: &str, kind: &str, target: &str, cwd: &str, args: &[&str]| {
        let fields = [kind, target, cwd]
            .into_iter()
            .chain(args.iter().copied())
            .map(hex)
            .collect::<Vec<_>>()
            .join(";");
        ProjectEvent::parse(format!("harbor;1;{action};{fields}").as_bytes())
    };
    let mut state = ProjectRecognition::default();
    state.apply(event(
        "agent",
        "ssh",
        "one",
        "/before",
        &["pi", "", "a b", "--resume"],
    ));
    let running = state.snapshot().unwrap();
    state.apply(ProjectEvent::Unknown);
    assert!(state.snapshot().is_none());
    state.apply(event("context", "ssh", "one", "/after", &[]));
    let exited = state.snapshot().unwrap();
    assert_eq!(running.agent, exited.agent);
    assert_eq!(exited.directory, "/after");
    state.apply(event("context", "ssh", "two", "/other", &[]));
    assert!(state.snapshot().unwrap().agent.is_none());
    state.apply(event(
        "record",
        "ssh",
        "one",
        "/before",
        &["codex", "--token", "synthetic-private"],
    ));
    assert!(state.snapshot().is_none());
    state.apply(event("context", "ssh", "one", "/now", &[]));
    assert_eq!(state.snapshot().unwrap().agent.unwrap().program, "codex");
    assert!(!format!("{state:?}").contains("synthetic-private"));
    assert!(state.snapshot().unwrap().validate().is_err());
    assert!(ProjectRecognition::default().snapshot().is_none());
    for malformed in [
        "harbor;1;context;ff;;ab".to_owned(),
        "harbor;1;other".into(),
        "x".repeat(4001),
    ] {
        state.apply(ProjectEvent::parse(malformed.as_bytes()));
        assert!(state.snapshot().is_none());
    }
}
