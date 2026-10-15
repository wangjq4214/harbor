use harbor_config::{DiagnosticLevel, Settings, load_candidate_from_path, load_from_path};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Config(PathBuf);
impl Config {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "harbor-launch-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(directory.join("child space")).unwrap();
        Self(directory.join("config.toml"))
    }
    fn write(&self, source: &str) {
        fs::write(&self.0, source).unwrap();
    }
}
impl Drop for Config {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.parent().unwrap());
    }
}

#[test]
fn startup_and_reload_share_literal_launch_schema_and_complete_removal() {
    let config = Config::new();
    config.write("[shell]\ndirectory = 'child space'\n[shell.environment]\nHARBOR_LITERAL = '$VAR %VAR% $(echo synthetic)'\nHARBOR_EMPTY = ''\nTERM = 'xterm-256color'");
    let startup = load_from_path(&config.0);
    let live = load_candidate_from_path(&config.0).unwrap();
    assert!(live.diagnostics.is_empty());
    assert_eq!(startup, live);
    assert_eq!(
        live.settings.shell.environment["HARBOR_LITERAL"],
        "$VAR %VAR% $(echo synthetic)"
    );
    assert_eq!(live.settings.shell.environment["HARBOR_EMPTY"], "");
    assert_eq!(
        live.settings.shell.directory.as_deref(),
        Some(config.0.parent().unwrap().join("child space").as_path())
    );
    assert!(!format!("{:?}", live).contains("$(echo synthetic)"));
    let absolute = config.0.parent().unwrap().join("child space");
    config.write(&format!("[shell]\ndirectory = '{}'", absolute.display()));
    assert_eq!(
        load_candidate_from_path(&config.0)
            .unwrap()
            .settings
            .shell
            .directory,
        Some(absolute)
    );
    config.write("");
    assert_eq!(
        load_candidate_from_path(&config.0).unwrap().settings,
        Settings::default()
    );
}

#[test]
fn invalid_launch_fields_reject_whole_live_document_and_recover_locally_at_startup() {
    let config = Config::new();
    for invalid in [
        "environment = 'SYNTHETIC_SECRET'",
        "environment = { A = 42 }",
        "environment = { A = ['SYNTHETIC_SECRET'] }",
        "environment = { '' = 'SYNTHETIC_SECRET' }",
        "environment = { 'A=B' = 'SYNTHETIC_SECRET' }",
        "environment = { A = \"SYNTHETIC_SECRET\\u0000\" }",
        "environment = { \"A\\u0000\" = 'SYNTHETIC_SECRET' }",
        "environment = { TERM = 'SYNTHETIC_SECRET' }",
        "directory = []",
        "directory = ''",
        "directory = 'missing'",
        "directory = 'config.toml'",
        "directory = \"bad\\u0000path\"",
    ] {
        config.write(&format!(
            "[colors]\nforeground = '#112233'\n[shell]\nprogram = 'cmd.exe'\n{invalid}"
        ));
        let error = load_candidate_from_path(&config.0).unwrap_err();
        assert!(!format!("{error:#} {error:?}").contains("SYNTHETIC_SECRET"));
        assert!(!error.to_string().contains("using"));
        let startup = load_from_path(&config.0);
        assert!(
            startup
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Error)
        );
        assert_eq!(startup.settings.shell.program.as_deref(), Some("cmd.exe"));
        assert!(startup.settings.shell.environment.is_empty());
        assert!(startup.settings.shell.directory.is_none());
        assert_ne!(startup.settings.colors, Settings::default().colors);
    }
}

#[cfg(windows)]
#[test]
fn windows_names_use_case_insensitive_ordinal_conflicts_and_protected_term() {
    let config = Config::new();
    for mapping in [
        "Path = 'SYNTHETIC_SECRET', PATH = 'other'",
        "Ä = 'SYNTHETIC_SECRET', ä = 'other'",
        "term = 'SYNTHETIC_SECRET'",
    ] {
        config.write(&format!("[shell]\nenvironment = {{ {mapping} }}"));
        let error = load_candidate_from_path(&config.0).unwrap_err();
        assert!(!format!("{error:#} {error:?}").contains("SYNTHETIC_SECRET"));
    }
    config.write("[shell.environment]\nterm = 'xterm-256color'");
    assert!(load_candidate_from_path(&config.0).is_ok());
}

#[test]
fn malformed_toml_never_echoes_environment_source_or_parser_values() {
    let config = Config::new();
    for source in [
        "[shell.environment]\nTOKEN = 'SYNTHETIC_SECRET' broken",
        "[shell.environment]\nTOKEN = SYNTHETIC_SECRET",
        "[shell.environment]\nTOKEN = 'SYNTHETIC_SECRET'\nTOKEN = 'second'",
        "[shell.environment]\nTOKEN = \"SYNTHETIC_SECRET\\q\"",
        "[shell.environment]\nTOKEN = 'SYNTHETIC_SECRET",
    ] {
        config.write(source);
        let error = load_candidate_from_path(&config.0).unwrap_err();
        assert!(!format!("{error:#} {error:?}").contains("SYNTHETIC_SECRET"));
        let startup = load_from_path(&config.0);
        assert_eq!(startup.settings, Settings::default());
        assert!(!format!("{startup:?}").contains("SYNTHETIC_SECRET"));
    }
}
