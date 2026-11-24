//! Local Bash syntax/quoting surrogate ONLY; this does not test WSL or SSH transport.
use super::*;
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn posix_path(path: &std::path::Path) -> String {
    let text = path
        .to_str()
        .unwrap()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    format!("/{}/{}", text[..1].to_ascii_lowercase(), &text[3..])
}

#[test]
#[ignore = "native: HARBOR_TEST_BASH must select a local Git Bash executable; not WSL/SSH evidence"]
fn local_bash_surrogate_enters_literal_directory_after_rc_for_both_remote_command_boundaries() {
    let bash =
        std::env::var_os("HARBOR_TEST_BASH").expect("select an authorized local Bash executable");
    let root = tempfile::tempdir().unwrap();
    let current = root.path().join("literal ' $ ; & [space] 世界");
    std::fs::create_dir(&current).unwrap();
    std::fs::write(
        root.path().join(".bashrc"),
        "cd /\nexport HARBOR_SPLIT_RC=loaded\n",
    )
    .unwrap();
    let current = posix_path(&std::fs::canonicalize(current).unwrap());
    let home = posix_path(&std::fs::canonicalize(root.path()).unwrap());
    for env in [
        ProjectEnvironment::Wsl {
            distribution: "declared-only".into(),
        },
        ProjectEnvironment::Ssh {
            destination: "declared-only".into(),
        },
    ] {
        for exists in [true, false] {
            let probe = StartupProbe::new(OperationId(900)).unwrap();
            let recipe = ProjectRecipe {
                environment: env.clone(),
                directory: if exists {
                    current.clone()
                } else {
                    format!("{current}/absent")
                },
                agent: None,
            };
            let command = probe.command(&recipe).unwrap();
            // Preserve the exact generated script and SSH quoting boundary, but disable
            // machine/user login files in this deliberately local, non-transport fixture.
            let script = match &env {
                ProjectEnvironment::Wsl { .. } => command.args[5].clone(),
                ProjectEnvironment::Ssh { .. } => {
                    command.args[3].replacen("bash -lc ", "bash --noprofile --norc -c ", 1)
                }
                _ => unreachable!(),
            };
            let mut child = Command::new(&bash)
                .args(["--noprofile", "--norc", "-c", &script])
                .env("HOME", &home)
                .env_remove("BASH_ENV")
                .env_remove("ENV")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let input = child.stdin.take().unwrap();
            if exists {
                let mut input = input;
                input
                    .write_all(
                        b"builtin printf 'OBSERVED:%s|%s\\n' \"$PWD\" \"$HARBOR_SPLIT_RC\"; exit\n",
                    )
                    .unwrap();
            } // A failing cwd never reads input; do not race its pipe closure.
            let output = child.wait_with_output().unwrap();
            let mut observe = probe.observer();
            observe(&output.stdout);
            assert_eq!(
                probe.status.load(Ordering::Acquire),
                if exists { READY } else { FAILED },
                "local Bash output: {:?}; stderr: {:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if exists {
                assert!(output.status.success());
                assert!(
                    String::from_utf8_lossy(&output.stdout)
                        .contains(&format!("OBSERVED:{current}|loaded"))
                );
            } else {
                assert!(!output.status.success());
                assert!(
                    !output
                        .stdout
                        .windows(probe.ready.len())
                        .any(|w| w == probe.ready)
                );
            }
        }
    }
}
