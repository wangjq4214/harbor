use super::*;
use harbor_terminal::{Terminal, TerminalOutputEvent};

fn metadata(uri: &str) -> Option<WorkingDirectoryMetadata> {
    let mut terminal = Terminal::new_headless(2, 10);
    terminal.put_str(&format!("\x1b]7;{uri}\x07"));
    terminal
        .drain_output_events()
        .into_iter()
        .find_map(|event| match event {
            TerminalOutputEvent::WorkingDirectoryChanged(value) => Some(value),
            _ => None,
        })
}

#[test]
fn local_uri_conversion_is_once_only_and_remote_forms_never_become_native_paths() {
    #[cfg(windows)]
    {
        assert_eq!(
            native_local_path(&metadata("file:///C:/a%20space/%E4%B8%96%E7%95%8C%2520").unwrap()),
            Some(PathBuf::from("C:/a space/世界%20"))
        );
        assert_eq!(
            native_local_path(&metadata("file://LOCALHOST/C:/a").unwrap()),
            Some(PathBuf::from("C:/a"))
        );
        use windows::{
            Win32::System::SystemInformation::{ComputerNameNetBIOS, GetComputerNameExW},
            core::PWSTR,
        };
        let mut buffer = [0u16; 256];
        let mut size = buffer.len() as u32;
        unsafe {
            GetComputerNameExW(
                ComputerNameNetBIOS,
                Some(PWSTR(buffer.as_mut_ptr())),
                &mut size,
            )
        }
        .unwrap();
        let name = String::from_utf16(&buffer[..size as usize]).unwrap();
        assert!(native_local_path(&metadata(&format!("file://{name}/C:/a")).unwrap()).is_some());
    }
    for uri in [
        "file://remote.invalid/C:/never",
        "file:////remote.invalid/share",
        "file://127.0.0.1/C:/never",
        "file:///C:/%5C%5Cremote.invalid/share",
        "file:///C:/../never",
        "file:///C:/a:stream",
    ] {
        if let Some(metadata) = metadata(uri) {
            #[cfg(windows)]
            assert!(native_local_path(&metadata).is_none(), "{uri}");
            assert!(local_directory(&metadata).is_none(), "{uri}");
        }
    }
    for uri in [
        "",
        "https://localhost/C:/x",
        "file:///C:/bad%",
        "file:///C:/bad%00",
        "file://remote.invalid:22/C:/x",
    ] {
        assert!(metadata(uri).is_none(), "{uri}");
    }
}

#[test]
fn profile_resolution_observes_precedence_and_current_identity_without_shell_inheritance() {
    let process = std::env::current_dir().unwrap();
    let temp = std::env::temp_dir();
    let osc_uri = format!(
        "file:///{}",
        process
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    let osc = metadata(&osc_uri).unwrap();
    let id = ProfileId::new("work").unwrap();
    let mut profiles = BTreeMap::from([(
        id.clone(),
        SessionProfile {
            project: None,
            name: "Work".into(),
            launch: harbor_config::ShellSettings {
                program: Some("selected.exe".into()),
                ..Default::default()
            },
        },
    )]);
    let mut intent = ProfileLaunchIntent {
        profile: id.clone(),
        source: None,
        directory: None,
    };
    let command = resolve(&profiles, &intent, None).unwrap();
    assert_eq!(command.directory, Some(process.clone()));
    assert_eq!(command.executable_policy, ExecutablePolicy::Strict);
    assert_eq!(command.program.as_deref(), Some("selected.exe"));
    assert!(command.args.is_empty());
    assert!(command.environment.is_empty());
    assert_eq!(
        resolve(&profiles, &intent, Some(&osc)).unwrap().directory,
        Some(process.clone())
    );
    for uri in [
        "file://remote.invalid/C:/never",
        "file:///C:/harbor-missing-local-path",
    ] {
        let unusable = metadata(uri).unwrap();
        assert_eq!(
            resolve(&profiles, &intent, Some(&unusable))
                .unwrap()
                .directory,
            Some(process.clone())
        );
    }
    profiles.get_mut(&id).unwrap().launch.directory = Some(temp.clone());
    assert_eq!(
        resolve(&profiles, &intent, Some(&osc)).unwrap().directory,
        Some(temp)
    );
    intent.directory = Some(process.clone());
    assert_eq!(
        resolve(&profiles, &intent, Some(&osc)).unwrap().directory,
        Some(process.clone())
    );
    profiles.get_mut(&id).unwrap().launch.directory =
        Some(process.join("harbor-nonexistent-profile-dir"));
    assert!(resolve(&profiles, &intent, Some(&osc)).is_ok()); // explicit wins
    intent.directory = None;
    assert!(resolve(&profiles, &intent, Some(&osc)).is_err()); // configured failure cannot fall back
    intent.directory = Some(PathBuf::from("relative"));
    assert!(resolve(&profiles, &intent, None).is_err());
    profiles.get_mut(&id).unwrap().launch.program = Some("edited.exe".into());
    intent.directory = Some(process);
    assert_eq!(
        resolve(&profiles, &intent, None)
            .unwrap()
            .program
            .as_deref(),
        Some("edited.exe")
    );
    profiles.clear();
    assert!(resolve(&profiles, &intent, None).is_err());
}

#[test]
fn reset_and_malformed_osc_metadata_keep_the_terminal_metadata_contract() {
    let mut terminal = Terminal::new_headless(2, 10);
    terminal.put_str("\x1b]7;file:///C:/valid\x07\x1b]7;file:///bad%\x07");
    assert_eq!(terminal.drain_output_events().len(), 1);
    terminal.put_str("\x1b]7;\x07");
    assert!(matches!(
        terminal.drain_output_events().as_slice(),
        [TerminalOutputEvent::WorkingDirectoryReset]
    ));
}

#[cfg(windows)]
mod native {
    use super::*;

    #[test]
    #[ignore = "native: Windows directory handles and local junction fixture"]
    fn local_directory_lease_pins_ancestors_and_rejects_junctions() {
        let root =
            std::env::temp_dir().join(format!("harbor-profile-lease-{}", std::process::id()));
        let moved = root.with_extension("moved");
        std::fs::create_dir_all(root.join("target")).unwrap();
        let uri = |path: &std::path::Path| {
            format!(
                "file:///{}",
                path.to_string_lossy()
                    .replace('\\', "/")
                    .replace('%', "%25")
                    .replace(' ', "%20")
            )
        };
        let lease = local_directory(&metadata(&uri(&root.join("target"))).unwrap())
            .expect("local directory eligible");
        assert!(
            std::fs::rename(&root, &moved).is_err(),
            "an ancestor must stay pinned until spawn completes"
        );
        drop(lease);
        std::fs::rename(&root, &moved).unwrap();
        std::fs::rename(&moved, &root).unwrap();
        let link = root.join("junction");
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&link)
            .arg(root.join("target"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "local junction fixture creation failed"
        );
        assert!(local_directory(&metadata(&uri(&link)).unwrap()).is_none());
        assert!(local_directory(&metadata(&uri(&link.join("nested"))).unwrap()).is_none());
        std::fs::remove_dir(link).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
