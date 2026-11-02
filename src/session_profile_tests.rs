//! Actual children through the same named-launch transaction used by the Host.
mod pty {
    use super::super::*;
    use harbor_app::profiles::ProfileLaunchIntent;
    use harbor_config::ProfileId;
    use harbor_pty::PtyEndpoints;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };

    struct Workspace(PathBuf);
    impl Workspace {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("harbor-profile-native-{}", std::process::id()));
            for name in ["configured", "explicit", "osc space 世界%20", "vanishing"] {
                fs::create_dir_all(root.join(name)).unwrap();
            }
            Self(root)
        }
        fn settings(&self, directory: Option<&str>, value: Option<&str>, index: usize) -> Settings {
            let probe = std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("examples/launch_probe.exe");
            assert!(
                probe.is_file(),
                "build with cargo build -p harbor-pty --example launch_probe"
            );
            let mut source = format!(
                "[shell]\nprogram = 'legacy-not-inherited.exe'\nargs = ['legacy']\n[profiles.work]\nname = 'Work'\nprogram = '{}'\nargs = ['', 'a b', '\"quoted\"', 'C:\\space path\\', 'slash\\\"quote', '世界🙂', '$VAR %VAR%']\n",
                probe.display()
            );
            if let Some(directory) = directory {
                source += &format!("directory = '{directory}'\n");
            }
            source += &format!(
                "[profiles.work.environment]\nHARBOR_PROFILE_REPORT = '{}'\nHARBOR_PROFILE_LITERAL = '$VAR %VAR% $(echo synthetic)'\nHARBOR_PROFILE_EMPTY = ''\nterm = 'xterm-256color'\n",
                self.report(index).display()
            );
            if let Some(value) = value {
                source += &format!("HARBOR_PROFILE_INHERITED = '{value}'\n");
            }
            fs::write(self.0.join("config.toml"), source).unwrap();
            harbor_config::load_candidate_from_path(self.0.join("config.toml"))
                .unwrap()
                .settings
        }
        fn report(&self, index: usize) -> PathBuf {
            self.0.join(format!("report-{index}.txt"))
        }
    }
    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn intent(source: Option<TabId>, directory: Option<PathBuf>) -> ProfileLaunchIntent {
        ProfileLaunchIntent {
            profile: ProfileId::new("work").unwrap(),
            source,
            directory,
        }
    }
    fn launch(
        f: &mut Fixture,
        intent: &ProfileLaunchIntent,
    ) -> anyhow::Result<Arc<Mutex<Terminal>>> {
        let policy = &f.policy;
        crate::profile_launch::create_tab(
            &mut f.tabs,
            &policy.profiles,
            intent,
            |_, draw, command| {
                let endpoints = PtyEndpoints::spawn_shell(
                    harbor_pty::TerminalSize { rows: 10, cols: 40 },
                    command,
                )?;
                let title = endpoints.shell_name().to_owned();
                let (terminal, renderer) = policy.prepare_new_terminal(
                    TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
                    load_system_fonts(&policy.font_settings)?,
                    TerminalSize { rows: 10, cols: 40 },
                    (640, 320),
                    false,
                    harbor_terminal::ClipboardDelivery::Discard,
                )?;
                let terminal = terminal.start_session_from_endpoints(endpoints, || true)?;
                #[allow(clippy::arc_with_non_send_sync)]
                let terminal = Arc::new(Mutex::new(terminal));
                let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
                    draw,
                    terminal.clone(),
                    renderer,
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(|_| {}),
                );
                Ok(TerminalTabResources::new(terminal, bridge, title))
            },
        )?;
        Ok(f.tabs.active_terminal().unwrap())
    }
    fn report(f: &mut Fixture, path: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            for (id, _, _) in f.tabs.configuration_targets() {
                f.tabs.process_output(id);
            }
            if let Ok(report) = fs::read_to_string(path) {
                return report;
            }
            assert!(Instant::now() < deadline, "profile child report deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn assert_directory(report: &str, directory: &Path) {
        let directory = PathBuf::from(directory.to_string_lossy().replace('/', "\\"));
        assert!(
            report.contains(&format!("directory:{directory:?}\n")),
            "{report}"
        );
    }
    fn osc_uri(directory: &Path) -> String {
        let path = directory.to_str().unwrap().replace('\\', "/");
        let encoded = path
            .as_bytes()
            .iter()
            .map(|&b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'/' | b':' | b'-' | b'_' | b'.') {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>();
        format!("file:///{encoded}")
    }

    #[test]
    #[ignore = "pty: Windows ConPTY, WARP and launch_probe example"]
    fn named_profile_children_argv_environment_directory_reload_and_provenance() {
        // Child-only synthetic inheritance, without changing the runner's global env/cwd.
        const GUARD: &str = "HARBOR_PROFILE_ISOLATED";
        if std::env::var_os(GUARD).is_none() {
            let name = concat!(
                module_path!(),
                "::named_profile_children_argv_environment_directory_reload_and_provenance"
            );
            let name = name.split_once("::").unwrap().1;
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--ignored", "--nocapture"])
                .env(GUARD, "1")
                .env("HARBOR_PROFILE_INHERITED", "synthetic-inherited")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("T0001_NATIVE_PASS"));
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            return;
        }
        let w = Workspace::new();
        let parent_cwd = std::env::current_dir().unwrap();
        let mut f = Fixture::new();
        let startup = w.settings(Some("configured"), Some("synthetic-override"), 0);
        assert_eq!(
            harbor_config::load_from_path(w.0.join("config.toml")).settings,
            startup
        );
        assert!(f.reload(startup, None).unwrap().contains("named profiles"));
        let old = launch(&mut f, &intent(None, Some(w.0.join("explicit")))).unwrap();
        let old_id = f.tabs.active_id().unwrap();
        let old_facts = old.lock().unwrap().launch_provenance().unwrap().clone();
        let first = report(&mut f, &w.report(0));
        assert_directory(&first, &w.0.join("explicit"));
        assert!(
            first.contains(&format!("args:{:?}\n", old_facts.args)),
            "{first}"
        );
        assert_eq!(
            old_facts.args,
            [
                "",
                "a b",
                "\"quoted\"",
                "C:\\space path\\",
                "slash\\\"quote",
                "世界🙂",
                "$VAR %VAR%"
            ]
        );
        assert!(first.contains("literal:Some(\"$VAR %VAR% $(echo synthetic)\")"));
        assert!(first.contains("empty:Some(\"\")"));
        assert!(first.contains("term:Some(\"xterm-256color\")"));
        assert!(first.contains("inherited:Some(\"synthetic-override\")"));
        assert_eq!(old_facts.initial_directory, w.0.join("explicit"));

        // Rejected mixed live/profile input never enters the application transaction.
        let accepted = f.accepted.clone();
        let policy = f.policy.profiles.clone();
        fs::write(w.0.join("config.toml"), "[font]\nsize = 40\n[profiles.bad]\nname = 'Bad'\nprogram = 'p'\nenvironment = { Path = 'synthetic', PATH = 'conflict' }").unwrap();
        assert!(
            crate::shell::reload_configuration_from_path(
                Some(&w.0.join("config.toml")),
                &mut f.accepted,
                &f.clipboard,
                |_, _| panic!("invalid profile entered transaction")
            )
            .is_err()
        );
        assert_eq!(f.accepted, accepted);
        assert_eq!(f.policy.profiles, policy);
        // A failed live palette preparation also cannot publish a valid new catalog.
        let mut rejected = w.settings(None, None, 1);
        rejected.colors.foreground = Rgba::from_rgb8(1, 2, 3);
        assert!(f.reload(rejected, Some(ReloadStage::Palette)).is_err());
        assert_eq!(f.policy.profiles, policy);

        // Profile cwd wins over valid OSC, including config-relative resolution.
        old.lock().unwrap().put_str(&format!(
            "\x1b]7;{}\x07",
            osc_uri(&w.0.join("osc space 世界%20"))
        ));
        f.tabs.process_output(old_id);
        f.reload(w.settings(Some("configured"), Some(""), 1), None)
            .unwrap();
        launch(&mut f, &intent(Some(old_id), None)).unwrap();
        let second = report(&mut f, &w.report(1));
        assert_directory(&second, &w.0.join("configured"));
        assert!(second.contains("inherited:Some(\"\")"));
        // Explicit source admission rejects a stale tab rather than using another source.
        let before = f.tabs.snapshots();
        assert!(launch(&mut f, &intent(Some(old_id), None)).is_err());
        assert_eq!(f.tabs.snapshots(), before);

        // Accepted edit/removal of env override restores inheritance, not deletion.
        let source = f.tabs.active_id().unwrap();
        f.tabs
            .active_terminal()
            .unwrap()
            .lock()
            .unwrap()
            .put_str(&format!(
                "\x1b]7;{}\x07",
                osc_uri(&w.0.join("osc space 世界%20"))
            ));
        f.tabs.process_output(source);
        f.reload(w.settings(None, None, 2), None).unwrap();
        launch(&mut f, &intent(Some(source), None)).unwrap();
        let third = report(&mut f, &w.report(2));
        assert_directory(&third, &w.0.join("osc space 世界%20"));
        assert!(third.contains("inherited:Some(\"synthetic-inherited\")"));
        let remote_source = f.tabs.active_id().unwrap();
        f.tabs
            .active_terminal()
            .unwrap()
            .lock()
            .unwrap()
            .put_str("\x1b]7;file://remote.invalid/C:/never\x07");
        f.tabs.process_output(remote_source);
        f.reload(w.settings(None, None, 3), None).unwrap();
        launch(&mut f, &intent(Some(remote_source), None)).unwrap();
        assert_directory(&report(&mut f, &w.report(3)), &parent_cwd);
        let reset_source = f.tabs.active_id().unwrap();
        f.tabs
            .active_terminal()
            .unwrap()
            .lock()
            .unwrap()
            .put_str("\x1b]7;\x07\x1b]7;file:///bad%\x07");
        f.tabs.process_output(reset_source);
        f.reload(w.settings(None, None, 5), None).unwrap();
        launch(&mut f, &intent(Some(reset_source), None)).unwrap();
        assert_directory(&report(&mut f, &w.report(5)), &parent_cwd);

        // Remove an accepted selected path after resolution, before actual spawn.
        f.reload(w.settings(Some("vanishing"), None, 4), None)
            .unwrap();
        let request = intent(None, None);
        let command = crate::profile_launch::resolve(&f.policy.profiles, &request, None).unwrap();
        fs::remove_dir(w.0.join("vanishing")).unwrap();
        assert!(
            PtyEndpoints::spawn_shell(harbor_pty::TerminalSize { rows: 10, cols: 40 }, &command)
                .is_err()
        );
        let before = f.tabs.snapshots();
        assert!(launch(&mut f, &request).is_err());
        assert_eq!(f.tabs.snapshots(), before);
        fs::create_dir(w.0.join("vanishing")).unwrap();
        launch(&mut f, &request).unwrap();
        assert_directory(&report(&mut f, &w.report(4)), &w.0.join("vanishing"));

        // Missing executable: no substitute child or tab, old sessions remain live.
        let mut missing = f.accepted.clone();
        missing
            .profiles
            .get_mut(&ProfileId::new("work").unwrap())
            .unwrap()
            .launch
            .program = Some("harbor-missing-named-profile.exe".into());
        f.reload(missing, None).unwrap();
        let before = f.tabs.snapshots();
        let error = launch(&mut f, &request)
            .err()
            .expect("named executable must fail");
        assert!(format!("{error:#}").contains("no fallback"));
        assert_eq!(f.tabs.snapshots(), before);
        let mut removed = f.accepted.clone();
        removed.profiles.clear();
        f.reload(removed, None).unwrap();
        assert!(f.policy.profiles.is_empty());
        assert!(launch(&mut f, &request).is_err());
        fs::remove_file(w.report(0)).unwrap();
        old.lock().unwrap().write_pty(b"report\r").unwrap();
        assert_eq!(
            report(&mut f, &w.report(0)),
            first,
            "profile reload/removal must not change the old child's PID, argv, environment or cwd"
        );
        // A later command changes the child cwd, never its recorded initial facts.
        fs::remove_file(w.report(0)).unwrap();
        old.lock()
            .unwrap()
            .write_pty(format!("cd {}\r", w.0.join("configured").display()).as_bytes())
            .unwrap();
        let later = report(&mut f, &w.report(0));
        assert_directory(&later, &w.0.join("configured"));
        assert_eq!(later.lines().next(), first.lines().next());
        assert_eq!(old.lock().unwrap().launch_provenance(), Some(&old_facts));
        for (_, terminal, _) in f.tabs.configuration_targets().into_iter().skip(2) {
            assert!(!terminal.lock().unwrap().is_session_closed());
        }
        assert_eq!(std::env::current_dir().unwrap(), parent_cwd);
        assert_eq!(
            std::env::var("HARBOR_PROFILE_INHERITED").unwrap(),
            "synthetic-inherited"
        );
        println!(
            "T0001_NATIVE_PASS: exact argv, literal/empty/removal/TERM, all cwd levels, coherent reload, strict failure, disappearing path/retry, stable PID/provenance after child command"
        );
    }

    #[test]
    #[ignore = "pty: Windows ConPTY, WARP and launch_probe example"]
    fn chooser_fifo_to_real_child_and_accepted_reload_catalog() {
        use harbor_app::command::{AppCommand, AppCommandRequest};
        use harbor_widget::{
            input::event::{Key, KeyboardEvent, Modifiers, UiEvent},
            renderer::Viewport,
            runtime::Runtime,
        };
        fn tap(runtime: &mut Runtime, key: Key) {
            for event in [
                KeyboardEvent::KeyDown {
                    key,
                    modifiers: Modifiers::default(),
                },
                KeyboardEvent::KeyUp {
                    key,
                    modifiers: Modifiers::default(),
                },
            ] {
                runtime.dispatch(UiEvent::Keyboard(event));
            }
        }
        let w = Workspace::new();
        let mut f = Fixture::new();
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(1000, 500, 1.0));
        runtime.set_root(harbor_app::workspace::tab_workspace(f.ui.clone(), false));
        runtime.update(Instant::now());
        for (index, palette_entry) in [false, true].into_iter().enumerate() {
            let accepted = w.settings(Some("configured"), Some("synthetic-override"), index);
            f.reload(accepted.clone(), None).unwrap();
            let count = f.tabs.snapshots().len();
            if palette_entry {
                runtime.request_focus(&f.ui.open_palette());
                runtime.update(Instant::now());
                runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(
                    "app.open-profile".into(),
                )));
                tap(&mut runtime, Key::Enter);
                let requests = f.ui.drain_actions();
                assert_eq!(requests.len(), 1);
                assert!(matches!(
                    next_app_action(&mut requests.into_iter(), &f.ui, &f.tabs, false),
                    Some(AppActionAdmission::Execute {
                        request: AppCommandRequest {
                            command: AppCommand::OpenProfileChooser,
                            ..
                        },
                        ..
                    })
                ));
            }
            runtime.request_focus(&f.ui.open_profile_chooser());
            runtime.update(Instant::now());
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("work".into())));
            tap(&mut runtime, Key::Enter);
            let requests = f.ui.drain_actions();
            assert_eq!(requests.len(), 1);
            // Failed complete live acceptance must not leak the edited recipe to UI admission.
            let mut rejected = accepted.clone();
            rejected
                .profiles
                .get_mut("work")
                .unwrap()
                .launch
                .args
                .push("must-not-launch".into());
            rejected.colors.foreground = Rgba::from_rgb8(1, 2, 3);
            assert!(f.reload(rejected, Some(ReloadStage::Palette)).is_err());
            let request = requests[0];
            let mut queue = [request, request].into_iter();
            let Some(AppActionAdmission::Execute {
                profile: Some(intent),
                ..
            }) = next_app_action(&mut queue, &f.ui, &f.tabs, false)
            else {
                panic!("current accepted chooser request")
            };
            let child = launch(&mut f, &intent).unwrap();
            let observed = report(&mut f, &w.report(index));
            let provenance = child.lock().unwrap().launch_provenance().unwrap().clone();
            assert!(observed.contains(&format!("args:{:?}\n", provenance.args)));
            assert!(!provenance.args.iter().any(|arg| arg == "must-not-launch"));
            assert!(observed.contains("literal:Some(\"$VAR %VAR% $(echo synthetic)\")"));
            assert_directory(&observed, &w.0.join("configured"));
            assert_eq!(f.tabs.snapshots().len(), count + 1);
            assert!(matches!(
                next_app_action(&mut queue, &f.ui, &f.tabs, false),
                Some(AppActionAdmission::Rejected { .. })
            ));
            f.ui.tabs()
                .sync(f.tabs.snapshots(), f.tabs.active_bridge(), 1000.0);
            runtime.clear_focus();
            runtime.request_focus(&f.ui.terminal_focus());
            runtime.update(Instant::now());
            assert!(runtime.input().focused().is_some());
            assert!(!f.ui.popup_is_open());
        }
        println!(
            "T0002_CONPTY_PASS: both chooser entries, real FIFO admission, exactly one real child each, accepted argv/env/cwd after rejected reload, mounted terminal focus"
        );
    }
}
