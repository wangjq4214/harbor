use super::*;

fn parse(source: &str) -> SettingsLoad {
    parse_document(toml::from_str(source).expect("test TOML must parse"))
}

fn test_directory() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "harbor-config-portable-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn portable_marker_takes_precedence_and_missing_or_malformed_config_never_falls_back() {
    let directory = test_directory();
    let executable = directory.join("harbor.exe");
    let home = directory.join("home-config.toml");
    fs::write(directory.join(PORTABLE_MARKER), "").unwrap();
    fs::write(&home, "[font]\nsize = 42").unwrap();

    let portable = directory.join(PORTABLE_CONFIG);
    fs::write(&portable, "[font]\nsize = 19").unwrap();
    let selected = config_path(Some(&executable), || Some(home.clone())).unwrap();
    assert_eq!(selected, portable);
    assert_eq!(load_from_path(&selected).settings.font.size, 19.0);

    fs::remove_file(&portable).unwrap();
    let missing = config_path(Some(&executable), || panic!("home fallback consulted")).unwrap();
    let loaded = load_from_path(&missing);
    assert_eq!(loaded.settings, Settings::default());
    assert!(loaded.diagnostics.is_empty());
    assert_eq!(fs::read_to_string(&home).unwrap(), "[font]\nsize = 42");

    fs::write(&portable, "[font\nsize = 19").unwrap();
    let malformed = load_from_path(
        config_path(Some(&executable), || panic!("home fallback consulted")).unwrap(),
    );
    assert_eq!(malformed.settings, Settings::default());
    assert!(
        malformed.diagnostics[0]
            .message
            .contains("could not parse settings")
    );
    assert_eq!(fs::read_to_string(&home).unwrap(), "[font]\nsize = 42");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn portable_config_path_moves_with_executable_directory() {
    let original = test_directory();
    fs::write(original.join(PORTABLE_MARKER), "").unwrap();
    fs::write(original.join(PORTABLE_CONFIG), "[font]\nsize = 21").unwrap();
    let moved = original.with_file_name(format!(
        "{}-moved",
        original.file_name().unwrap().to_string_lossy()
    ));
    fs::rename(&original, &moved).unwrap();

    let path = config_path(Some(&moved.join("harbor.exe")), || {
        panic!("home fallback consulted")
    })
    .unwrap();
    assert_eq!(load_from_path(path).settings.font.size, 21.0);
    fs::remove_dir_all(moved).unwrap();
}

#[test]
fn absent_portable_marker_preserves_home_path_selection() {
    let directory = test_directory();
    let home = directory.join("home-config.toml");
    let selected = config_path(Some(&directory.join("harbor.exe")), || Some(home.clone()));
    assert_eq!(selected, Some(home));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn clipboard_policy_defaults_and_supported_values() {
    for source in ["", "[clipboard]"] {
        let loaded = parse(source);
        assert_eq!(
            loaded.settings.clipboard.osc52_write,
            Osc52WritePolicy::Allow
        );
        assert!(loaded.diagnostics.is_empty());
    }
    for (value, expected) in [
        ("allow", Osc52WritePolicy::Allow),
        ("deny", Osc52WritePolicy::Deny),
        ("confirm", Osc52WritePolicy::Confirm),
    ] {
        let loaded = parse(&format!("[clipboard]\nosc52_write = {value:?}"));
        assert_eq!(loaded.settings.clipboard.osc52_write, expected);
        assert!(loaded.diagnostics.is_empty());
    }
}

#[test]
fn invalid_clipboard_shapes_recover_locally_without_echoing_values() {
    for clipboard in [
        "clipboard = 1",
        "clipboard = false",
        "clipboard = 'fixture-invalid-value'",
        "clipboard = []",
        "[[clipboard]]",
        "[clipboard]\nosc52_write = 'fixture-invalid-value'",
        "[clipboard]\nosc52_write = 'ALLOW'",
        "[clipboard]\nosc52_write = ''",
        "[clipboard]\nosc52_write = 1",
        "[clipboard]\nosc52_write = true",
        "[clipboard]\nosc52_write = []",
        "[clipboard.osc52_write]",
    ] {
        let loaded = parse(&format!(
            "{clipboard}\n[font]\nsize = 18\n[shell]\nprogram = 'pwsh.exe'"
        ));
        assert_eq!(
            loaded.settings.clipboard.osc52_write,
            Osc52WritePolicy::Allow
        );
        assert_eq!(loaded.settings.font.size, 18.0);
        assert_eq!(loaded.settings.shell.program.as_deref(), Some("pwsh.exe"));
        assert_eq!(loaded.diagnostics.len(), 1, "{clipboard}");
        assert_eq!(loaded.diagnostics[0].level, DiagnosticLevel::Error);
        assert!(
            !loaded.diagnostics[0]
                .message
                .contains("fixture-invalid-value")
        );
    }
}

#[test]
fn unsupported_clipboard_settings_are_not_capacity_or_read_controls() {
    let loaded = parse("[clipboard]\nosc52_write = 'deny'\nread = true\ncapacity = 100");
    assert_eq!(
        loaded.settings.clipboard.osc52_write,
        Osc52WritePolicy::Deny
    );
    assert_eq!(loaded.diagnostics.len(), 2);
    assert!(
        loaded
            .diagnostics
            .iter()
            .all(|d| d.level == DiagnosticLevel::Warning)
    );
}

#[test]
fn valid_fields_override_defaults() {
    let loaded = parse(
        r##"
        [font]
        family = "Cascadia Mono"
        size = 18
        [shell]
        program = "pwsh.exe"
        args = ["-NoLogo"]
        [colors]
        background = "#5C331440"
        [colors.normal]
        red = "#CD3131"
    "##,
    );

    assert_eq!(
        loaded.settings.font.family.as_deref(),
        Some("Cascadia Mono")
    );
    assert_eq!(loaded.settings.font.size, 18.0);
    assert_eq!(loaded.settings.shell.program.as_deref(), Some("pwsh.exe"));
    assert_eq!(loaded.settings.shell.args, ["-NoLogo"]);
    assert_eq!(
        loaded.settings.colors.background,
        Rgba::from_rgba8(0x5c, 0x33, 0x14, 0x40)
    );
    assert_eq!(
        loaded.settings.colors.normal[1],
        Rgba::from_rgb8(0xcd, 0x31, 0x31)
    );
    // Unspecified terminal colors inherit Mocha rather than the old palette.
    let defaults = Palette::default();
    assert_eq!(loaded.settings.colors.foreground, defaults.foreground);
    assert_eq!(loaded.settings.colors.cursor, defaults.cursor);
    assert_eq!(loaded.settings.colors.selection, defaults.selection);
    assert_eq!(loaded.settings.colors.normal[2..], defaults.normal[2..]);
    assert_eq!(loaded.settings.colors.bright, defaults.bright);
}

#[test]
fn background_opacity_overrides_only_background_alpha_and_preserves_legacy_colors() {
    for (source, expected) in [
        ("", BACKGROUND),
        ("[colors]", BACKGROUND),
        (
            "[colors]\nbackground_opacity = 0.8",
            [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 0.8],
        ),
        (
            "[colors]\nbackground = '#12345640'",
            [18.0 / 255.0, 52.0 / 255.0, 86.0 / 255.0, 64.0 / 255.0],
        ),
        (
            "[colors]\nbackground = '#123456'",
            [18.0 / 255.0, 52.0 / 255.0, 86.0 / 255.0, 1.0],
        ),
        (
            "[colors]\nbackground = '#12345640'\nbackground_opacity = 0.8",
            [18.0 / 255.0, 52.0 / 255.0, 86.0 / 255.0, 0.8],
        ),
        (
            "[colors]\nbackground_opacity = 0.8\nbackground = '#123456'",
            [18.0 / 255.0, 52.0 / 255.0, 86.0 / 255.0, 0.8],
        ),
        (
            "[colors]\nbackground_opacity = 0",
            [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 0.0],
        ),
        (
            "[colors]\nbackground_opacity = 1",
            [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 1.0],
        ),
        (
            "[colors]\nbackground_opacity = 0.0",
            [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 0.0],
        ),
        (
            "[colors]\nbackground_opacity = 1.0",
            [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 1.0],
        ),
    ] {
        let loaded = parse(source);
        assert!(
            loaded.diagnostics.is_empty(),
            "{source}: {:?}",
            loaded.diagnostics
        );
        let expected = Palette {
            background: Rgba::new(expected[0], expected[1], expected[2], expected[3]),
            ..Palette::default()
        };
        assert_eq!(loaded.settings.colors, expected, "{source}");
    }
}

#[test]
fn invalid_background_opacity_discards_the_whole_startup_palette() {
    for value in [
        "-0.01",
        "1.01",
        "-1",
        "2",
        "nan",
        "inf",
        "-inf",
        "1e100",
        "1.0000000001",
        "-0.0000000001",
        "'0.6'",
        "true",
        "[]",
        "{}",
    ] {
        let loaded = parse(&format!(
            "[font]\nsize = 18\n[colors]\nforeground = '#123456'\nbackground_opacity = {value}"
        ));
        assert_eq!(loaded.settings.font.size, 18.0);
        assert_eq!(loaded.settings.colors, Palette::default(), "{value}");
        assert!(
            loaded.diagnostics.iter().any(|diagnostic| {
                diagnostic.level == DiagnosticLevel::Error
                    && diagnostic.message.contains("colors.background_opacity")
            }),
            "{value}: {:?}",
            loaded.diagnostics
        );
    }
}

#[test]
fn ordinary_invalid_fields_recover_independently() {
    let loaded = parse(
        r##"
        [font]
        family = "Cascadia Mono"
        size = -1
        [shell]
        program = "pwsh.exe"
        args = "-NoLogo"
    "##,
    );

    assert_eq!(
        loaded.settings.font.family.as_deref(),
        Some("Cascadia Mono")
    );
    assert_eq!(loaded.settings.font.size, FONT_SIZE);
    assert_eq!(loaded.settings.shell.program.as_deref(), Some("pwsh.exe"));
    assert!(loaded.settings.shell.args.is_empty());
    assert_eq!(loaded.diagnostics.len(), 2);
}

#[test]
fn one_invalid_color_discards_all_color_overrides() {
    let loaded = parse(
        r##"
        [colors]
        foreground = "#123456"
        background = "not-a-color"
    "##,
    );
    assert_eq!(loaded.settings.colors, Palette::default());
}
#[test]
fn non_ascii_invalid_color_discards_palette_without_panicking() {
    let loaded = parse(
        r##"
        [colors]
        foreground = "#aééx"
        background = "#123456"
    "##,
    );

    assert_eq!(loaded.settings.colors, Palette::default());
    assert!(loaded.diagnostics.iter().any(|diagnostic| {
        diagnostic.level == DiagnosticLevel::Error && diagnostic.message.contains("non-hexadecimal")
    }));
}

#[test]
fn syntax_error_returns_complete_defaults() {
    let path = std::env::temp_dir().join(format!("harbor-invalid-{}.toml", std::process::id()));
    fs::write(&path, "[font\nsize = 12").unwrap();
    let loaded = load_from_path(&path);
    let _ = fs::remove_file(path);
    assert_eq!(loaded.settings, Settings::default());
    assert_eq!(loaded.diagnostics[0].level, DiagnosticLevel::Error);
}

#[test]
fn missing_file_returns_complete_defaults_silently() {
    let path = std::env::temp_dir().join(format!("harbor-missing-{}.toml", std::process::id()));
    let _ = fs::remove_file(&path);
    let loaded = load_from_path(path);
    assert_eq!(loaded.settings, Settings::default());
    assert!(loaded.diagnostics.is_empty());
}

#[test]
fn unknown_keys_warn_without_discarding_known_values() {
    let loaded = parse("mystery = true\n[font]\nsize = 20\nweights = true");
    assert_eq!(loaded.settings.font.size, 20.0);
    assert_eq!(
        loaded
            .diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .count(),
        2
    );
}

#[test]
fn structured_keybindings_reconstruct_ids_and_preserve_records() {
    let loaded = parse(
        r#"
        [keybindings.app.new-tab]
        bindings = [{ modifiers = ["ctrl"], key = "n" }]
        [keybindings.terminal.paste]
        bindings = [
          { modifiers = ["ctrl"], key = "v" },
          { modifiers = ["shift"], key = "insert" }
        ]
        [keybindings.terminal.page-up]
        bindings = []
        "#,
    );

    assert!(loaded.settings.keybindings.valid);
    assert_eq!(
        loaded.settings.keybindings.entries["app.new-tab"],
        [RawKeybinding {
            modifiers: vec!["ctrl".to_owned()],
            key: "n".to_owned(),
        }]
    );
    assert_eq!(
        loaded.settings.keybindings.entries["terminal.paste"].len(),
        2
    );
    assert!(loaded.settings.keybindings.entries["terminal.page-up"].is_empty());
}

#[test]
fn malformed_keybinding_shapes_are_atomic_and_preserve_other_settings() {
    let cases = [
        ("keybindings = []", "keybindings"),
        ("[keybindings]\napp = [\"bad\"]", "keybindings.app"),
        (
            "[keybindings.app]\nnew-tab = [\"ctrl+n\"]",
            "keybindings.app.new-tab",
        ),
        (
            "[keybindings.app.new-tab]\nother = []",
            "keybindings.app.new-tab.bindings",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = {}",
            "keybindings.app.new-tab.bindings",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [\"ctrl+n\"]",
            "keybindings.app.new-tab.bindings[0]",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [{ modifiers = \"ctrl\", key = \"n\" }]",
            "modifiers",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [{ modifiers = [\"ctrl\"] }]",
            ".key",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [{ modifiers = [\"ctrl\"], key = 1 }]",
            ".key",
        ),
        (
            "[keybindings]\n\"app.new-tab\" = [\"ctrl+n\"]",
            "keybindings.app.new-tab",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [{ key = \"n\" }]",
            ".modifiers",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [{ modifiers = [\"ctrl\", 1], key = \"n\" }]",
            ".modifiers",
        ),
        (
            "[keybindings.app.new-tab]\nbindings = [{ modifiers = [\"ctrl\"], key = \"n\", leader = true }]",
            ".leader",
        ),
    ];

    for (keybindings, expected_path) in cases {
        let loaded = parse(&format!("{keybindings}\n[font]\nsize = 18"));
        assert_eq!(loaded.settings.font.size, 18.0);
        assert!(!loaded.settings.keybindings.valid, "{keybindings}");
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.level == DiagnosticLevel::Error
                    && diagnostic.message.contains(expected_path)),
            "missing path {expected_path} in diagnostics for {keybindings}: {:?}",
            loaded.diagnostics
        );
    }
}

#[test]
fn malformed_record_does_not_hide_other_structurally_valid_entries() {
    let loaded = parse(
        r#"
        [keybindings.app.new-tab]
        bindings = [{ modifiers = "ctrl", key = "n" }]
        [keybindings.terminal.paste]
        bindings = [{ modifiers = ["ctrl"], key = "v" }]
        "#,
    );

    assert!(!loaded.settings.keybindings.valid);
    assert_eq!(
        loaded.settings.keybindings.entries["terminal.paste"],
        [RawKeybinding {
            modifiers: vec!["ctrl".to_owned()],
            key: "v".to_owned(),
        }]
    );
}

#[test]
fn should_expose_unified_backdrop_tint_defaults() {
    let style = WindowBackdropStyle::default();
    assert_eq!(style.tint_rgb, [1.0, 1.0, 1.0]);
    assert_eq!(style.tint_opacity, 0.06);
    assert_eq!(style.luminosity_opacity, 1.0);
    assert_eq!(style.fallback, [0.1176, 0.1176, 0.1176]);
}

#[test]
fn history_limits_defaults_boundaries_and_independent_startup_recovery() {
    assert_eq!(
        parse("").settings.history,
        HistoryLimits {
            max_rows: 4000,
            max_bytes: 4 * 1024 * 1024
        }
    );
    for (rows, bytes) in [(0, 0), (1, 1), (16000, 16 * 1024 * 1024)] {
        let loaded = parse(&format!(
            "[history]\nmax_rows = {rows}\nmax_bytes = {bytes}"
        ));
        assert!(loaded.diagnostics.is_empty());
        assert_eq!(
            loaded.settings.history,
            HistoryLimits {
                max_rows: rows,
                max_bytes: bytes
            }
        );
    }
    for bad in ["-1", "16001", "1.5", "true", "'secret'"] {
        let source = format!("[history]\nmax_rows = {bad}\nmax_bytes = 42\nunknown = true");
        let loaded = parse(&source);
        assert_eq!(loaded.settings.history.max_rows, 4000);
        assert_eq!(loaded.settings.history.max_bytes, 42);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Error)
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Warning)
        );
        assert!(!format!("{:?}", loaded.diagnostics).contains("secret"));
    }
    assert_eq!(
        parse("history = []").settings.history,
        HistoryLimits::default()
    );
}

#[test]
fn history_invalid_live_candidates_never_expose_fallback_settings() {
    let dir = test_directory();
    let path = dir.join("config.toml");
    for source in [
        "history = 1",
        "[history]\nmax_bytes = 16777217",
        "[history]\nmax_rows = -1",
        "[history]\nmax_rows = 1\n[font]\nsize = -1",
        "not toml",
    ] {
        fs::write(&path, source).unwrap();
        assert!(load_candidate_from_path(&path).is_err(), "{source}");
    }
    fs::write(&path, "[history]\nmax_rows = 1").unwrap();
    assert_eq!(
        load_candidate_from_path(&path).unwrap().settings.history,
        HistoryLimits {
            max_rows: 1,
            ..Default::default()
        }
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        load_candidate_from_path(&path).unwrap_err().io_kind,
        Some(std::io::ErrorKind::NotFound)
    );
}
