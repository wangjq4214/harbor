use harbor_config::{
    DiagnosticLevel, ProfileId, Settings, load_candidate_from_path, load_from_path,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Config(PathBuf);
impl Config {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "harbor-profile-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("child space 世界")).unwrap();
        Self(root.join("config.toml"))
    }
    fn write(&self, source: &str) {
        fs::write(&self.0, source).unwrap();
    }
    fn candidate(&self) -> Settings {
        load_candidate_from_path(&self.0).unwrap().settings
    }
}
impl Drop for Config {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.parent().unwrap());
    }
}

#[test]
fn independent_catalog_identity_add_edit_remove_and_config_relative_paths() {
    let f = Config::new();
    let shell = "[shell]\nprogram = 'legacy.exe'\nargs = ['legacy']\ndirectory = 'child space 世界'\nenvironment = { LEGACY = 'not inherited' }\n";
    f.write(&format!("{shell}[profiles.work_1]\nname = 'Work'\nprogram = 'tool.exe'\n[profiles.other]\nname = 'Other'\nprogram = 'other.exe'\ndirectory = 'child space 世界'\nargs = ['', 'a b', '世界', '\"', 'C:\\space path\\']\nenvironment = {{ LITERAL = '$VAR %VAR% $(echo synthetic)', EMPTY = '', TERM = 'xterm-256color' }}"));
    let first = f.candidate();
    assert_eq!(first, load_from_path(&f.0).settings);
    assert_eq!(first.profiles.len(), 2);
    let work = &first.profiles["work_1"];
    assert_eq!(work.launch.program.as_deref(), Some("tool.exe"));
    assert!(work.launch.args.is_empty());
    assert!(work.launch.directory.is_none());
    assert!(work.launch.environment.is_empty());
    let other = &first.profiles["other"];
    assert_eq!(
        other.launch.directory,
        Some(f.0.parent().unwrap().join("child space 世界"))
    );
    assert_eq!(other.launch.environment["EMPTY"], "");
    assert_eq!(
        other.launch.environment["LITERAL"],
        "$VAR %VAR% $(echo synthetic)"
    );
    assert_eq!(
        other.launch.args,
        ["", "a b", "世界", "\"", "C:\\space path\\"]
    );
    assert!(!format!("{first:?}").contains("$(echo synthetic)"));
    f.write(&format!(
        "{shell}[profiles.work_1]\nname = 'Renamed'\nprogram = 'new.exe'"
    ));
    let edited = f.candidate();
    assert_eq!(
        edited.profiles.keys().next(),
        Some(&ProfileId::new("work_1").unwrap())
    );
    assert_eq!(edited.profiles["work_1"].name, "Renamed");
    assert_eq!(edited.profiles.len(), 1);
    assert_eq!(first.profiles["work_1"].name, "Work");
    f.write(shell);
    assert!(f.candidate().profiles.is_empty());
    assert!(ProfileId::new("").is_none());
    assert!(ProfileId::new("a.b").is_none());
}

#[test]
fn every_invalid_profile_rejects_complete_candidate_and_is_unavailable_at_startup() {
    let f = Config::new();
    for invalid in [
        "name = ''\nprogram = 'p'",
        "name = 'Work'",
        "name = 1\nprogram = 'p'",
        "name = 'Work'\nprogram = ''",
        "name = 'Work'\nprogram = \"p\\u0000x\"",
        "name = 'Work'\nprogram = 'p'\nargs = [1]",
        "name = 'Work'\nprogram = 'p'\nargs = [\"x\\u0000y\"]",
        "name = 'Work'\nprogram = 'p'\ndirectory = 'missing'",
        "name = 'Work'\nprogram = 'p'\ndirectory = 'config.toml'",
        "name = 'Work'\nprogram = 'p'\ndirectory = ''",
        "name = 'Work'\nprogram = 'p'\nenvironment = { TERM = 'SYNTHETIC_SECRET' }",
        "name = 'Work'\nprogram = 'p'\nenvironment = { A = \"SYNTHETIC_SECRET\\u0000\" }",
        "name = 'Work'\nprogram = 'p'\nenvironment = { 'A=B' = 'SYNTHETIC_SECRET' }",
        "name = 'Work'\nprogram = 'p'\nenvironment = { A = 1 }",
        "name = 'Work'\nprogram = 'p'\ninherits = 'shell'",
    ] {
        f.write(&format!("[font]\nsize = 30\n[profiles.bad]\n{invalid}\n[profiles.good]\nname = 'Good'\nprogram = 'g'"));
        let error = load_candidate_from_path(&f.0).unwrap_err();
        assert!(!format!("{error:#} {error:?}").contains("SYNTHETIC_SECRET"));
        let startup = load_from_path(&f.0);
        assert!(!startup.settings.profiles.contains_key("bad"), "{invalid}");
        assert!(startup.settings.profiles.contains_key("good"));
        assert!(
            startup
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Error)
        );
    }
    for source in [
        "profiles = []",
        "[profiles]\nbad = 'not a table'",
        "[profiles.'bad.id']\nname = 'Work'\nprogram = 'p'",
        "[profiles.a]\nname = 'Same'\nprogram = 'a'\n[profiles.b]\nname = 'Same'\nprogram = 'b'",
    ] {
        f.write(source);
        assert!(load_candidate_from_path(&f.0).is_err());
    }
}

#[cfg(windows)]
#[test]
fn windows_profile_environment_conflicts_and_term_are_value_free() {
    let f = Config::new();
    for env in [
        "Path = 'SYNTHETIC_SECRET', PATH = 'other'",
        "Ä = 'SYNTHETIC_SECRET', ä = 'other'",
        "term = 'SYNTHETIC_SECRET'",
    ] {
        f.write(&format!(
            "[profiles.work]\nname = 'Work'\nprogram = 'p'\nenvironment = {{ {env} }}"
        ));
        let error = load_candidate_from_path(&f.0).unwrap_err();
        assert!(!format!("{error:#} {error:?}").contains("SYNTHETIC_SECRET"));
    }
}
