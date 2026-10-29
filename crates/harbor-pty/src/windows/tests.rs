#[path = "tests/pty.rs"]
mod pty;

use std::{
    thread,
    time::{Duration, Instant},
};

use super::*;

use ::windows::Win32::System::Pipes::PeekNamedPipe;
#[test]
fn cancelled_read_maps_to_interrupted_io_error() {
    let error =
        ::windows::core::Error::from_hresult(HRESULT::from_win32(ERROR_OPERATION_ABORTED.0));

    assert_eq!(
        reader_io_error(anyhow::Error::new(error)).kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn default_shells_are_tried_in_priority_order_and_report_the_started_shell() {
    let programs = [
        "pwsh.exe",
        "powershell.exe",
        r"C:\Windows\System32\cmd.exe",
        "cmd.exe",
    ];
    let titles = ["pwsh", "powershell", "cmd", "cmd"];
    for selected in 0..programs.len() {
        let mut attempted = Vec::new();
        let (process, title) = create_shell_process_with(
            &ShellCommand::default(),
            Some(OsString::from(programs[2])),
            |program, args| {
                assert!(args.is_empty());
                attempted.push(program.to_string_lossy().into_owned());
                if attempted.len() == selected + 1 {
                    Ok(42)
                } else {
                    anyhow::bail!("unavailable")
                }
            },
        )
        .unwrap();
        assert_eq!(attempted, programs[..=selected]);
        assert_eq!(process, 42);
        assert_eq!(title, titles[selected]);
    }
}

#[test]
fn missing_or_blank_comspec_still_falls_back_to_cmd() {
    for comspec in [None, Some(OsString::new()), Some(OsString::from("   "))] {
        let mut attempted = Vec::new();
        let (_, title) =
            create_shell_process_with(&ShellCommand::default(), comspec, |program, _| {
                attempted.push(program.to_string_lossy().into_owned());
                if program == OsStr::new("cmd.exe") {
                    Ok(())
                } else {
                    anyhow::bail!("unavailable")
                }
            })
            .unwrap();
        assert_eq!(attempted, ["pwsh.exe", "powershell.exe", "cmd.exe"]);
        assert_eq!(title, "cmd");
    }
}

#[test]
fn configured_shell_takes_precedence_and_preserves_args() {
    let command = ShellCommand::new(Some(r"C:\Tools\nu.exe".to_owned()), vec!["a b".to_owned()]);
    let mut attempts = 0;
    let (_, title) = create_shell_process_with(&command, None, |program, args| {
        attempts += 1;
        assert_eq!(program, OsStr::new(r"C:\Tools\nu.exe"));
        assert_eq!(args, command.args);
        Ok(())
    })
    .unwrap();
    assert_eq!(attempts, 1);
    assert_eq!(title, "nu");
}

#[test]
fn failed_configured_shell_uses_default_order_without_configured_args() {
    let command = ShellCommand::new(Some("missing.exe".to_owned()), vec!["--custom".to_owned()]);
    let mut attempted = Vec::new();
    let (_, title) = create_shell_process_with(&command, None, |program, args| {
        attempted.push(program.to_string_lossy().into_owned());
        if program == OsStr::new("missing.exe") {
            assert_eq!(args, command.args);
        } else {
            assert!(args.is_empty());
        }
        if program == OsStr::new("powershell.exe") {
            Ok(())
        } else {
            anyhow::bail!("unavailable")
        }
    })
    .unwrap();
    assert_eq!(attempted, ["missing.exe", "pwsh.exe", "powershell.exe"]);
    assert_eq!(title, "powershell");
}

#[test]
fn args_only_configuration_applies_to_default_shells() {
    for program in [None, Some("   ".to_owned())] {
        let command = ShellCommand::new(program, vec!["-NoLogo".to_owned()]);
        let (_, title) = create_shell_process_with(&command, None, |program, args| {
            assert_eq!(args, command.args);
            if program == OsStr::new("powershell.exe") {
                Ok(())
            } else {
                anyhow::bail!("unavailable")
            }
        })
        .unwrap();
        assert_eq!(title, "powershell");
    }
}

#[test]
fn all_shell_failures_are_reported() {
    let command = ShellCommand::new(Some("missing.exe".to_owned()), Vec::new());
    let error =
        create_shell_process_with::<()>(&command, None, |_, _| anyhow::bail!("unavailable"))
            .unwrap_err();
    let message = format!("{error:#}");
    for program in ["missing.exe", "pwsh.exe", "powershell.exe", "cmd.exe"] {
        assert!(
            message.contains(&format!("{program}: unavailable")),
            "{message}"
        );
    }
}

#[test]
fn configured_program_and_args_form_a_quoted_windows_command_line() {
    let command = ShellCommand::new(
        Some(r"C:\Program Files\PowerShell\pwsh.exe".to_owned()),
        vec![
            "-NoLogo".to_owned(),
            "a b".to_owned(),
            r#"say \"hi\""#.to_owned(),
        ],
    );

    let encoded = shell_command_line(
        OsStr::new(command.program.as_deref().unwrap()),
        &command.args,
    );
    let line = String::from_utf16(&encoded[..encoded.len() - 1]).unwrap();

    assert_eq!(
        line,
        "\"C:\\Program Files\\PowerShell\\pwsh.exe\" -NoLogo \"a b\" \"say \\\\\\\"hi\\\\\\\"\""
    );
}
#[test]
fn shell_display_name_uses_basename_without_exe_suffix() {
    assert_eq!(
        shell_display_name(OsStr::new(r"C:\Program Files\PowerShell\pwsh.exe")),
        "pwsh"
    );
    assert_eq!(shell_display_name(OsStr::new("CMD.EXE")), "CMD");
    assert_eq!(shell_display_name(OsStr::new("nu")), "nu");
}

#[test]
fn simple_wsl_options_stay_bare_but_empty_and_spaced_arguments_are_quoted() {
    let encoded = shell_command_line(
        OsStr::new("wsl.exe"),
        &[
            "-d",
            "Ubuntu",
            "--exec",
            "printf",
            "",
            "a b",
            "\t",
            r"C:\path\",
        ]
        .map(str::to_owned),
    );
    assert_eq!(
        String::from_utf16(&encoded[..encoded.len() - 1]).unwrap(),
        "wsl.exe -d Ubuntu --exec printf \"\" \"a b\" \"\t\" C:\\path\\"
    );
}

#[test]
fn trailing_backslashes_are_doubled_before_closing_quote() {
    let mut encoded = Vec::new();
    append_windows_argument(
        &mut encoded,
        &r"C:\a path\".encode_utf16().collect::<Vec<_>>(),
    );
    assert_eq!(String::from_utf16(&encoded).unwrap(), "\"C:\\a path\\\\\"");
}

#[test]
fn environment_block_overlays_ordinal_names_preserves_empty_hidden_and_literal_values() {
    let inherited = [
        ("Path", "inherited"),
        ("Ä", "old"),
        ("TERM", "old-term"),
        ("=C:", r"C:\drive"),
        ("UNCHANGED", "inherited"),
    ]
    .map(|(k, v)| (OsString::from(k), OsString::from(v)));
    let overrides = [
        ("PATH", ""),
        ("ä", "$VAR %VAR% $(echo synthetic)"),
        ("term", "xterm-256color"),
    ]
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .into();
    let block = build_environment_block_from(inherited, &overrides).unwrap();
    assert!(block.ends_with(&[0, 0]));
    let entries: Vec<_> = block
        .split(|&c| c == 0)
        .filter(|e| !e.is_empty())
        .map(|e| String::from_utf16(e).unwrap())
        .collect();
    assert_eq!(
        entries,
        [
            r"=C:=C:\drive",
            "PATH=",
            "TERM=xterm-256color",
            "UNCHANGED=inherited",
            "ä=$VAR %VAR% $(echo synthetic)",
        ]
    );
    for invalid in [
        [("PATH", "SYNTHETIC_SECRET"), ("Path", "other")].as_slice(),
        [("Ä", "SYNTHETIC_SECRET"), ("ä", "other")].as_slice(),
        [("term", "SYNTHETIC_SECRET")].as_slice(),
        [("", "SYNTHETIC_SECRET")].as_slice(),
        [("A=B", "SYNTHETIC_SECRET")].as_slice(),
        [("A", "SYNTHETIC_SECRET\0")].as_slice(),
    ] {
        let overrides = invalid
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let error = build_environment_block_from([], &overrides).unwrap_err();
        assert!(!format!("{error:#} {error:?}").contains("SYNTHETIC_SECRET"));
    }
}
fn contains_shell_prompt(output: &[u8]) -> bool {
    let text = String::from_utf8_lossy(output);
    text.contains("Microsoft Windows") || text.contains('>')
}

fn output_bytes_available(reader: &PtyReader) -> u32 {
    let mut bytes_available = 0_u32;
    unsafe {
        PeekNamedPipe(
            reader.output_read.handle(),
            None,
            0,
            None,
            Some(&mut bytes_available as *mut u32),
            None,
        )
    }
    .unwrap();
    bytes_available
}

#[test]
fn rejects_empty_size() {
    let error = match Pty::spawn_shell(PtySize { rows: 0, cols: 80 }, &ShellCommand::default()) {
        Ok(_) => panic!("zero-row pty size unexpectedly spawned a shell"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("pty size must be positive"),
        "{error:#}"
    );
}
