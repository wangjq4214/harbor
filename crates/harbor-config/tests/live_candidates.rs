use harbor_config::{DiagnosticLevel, Settings, load_candidate_from_path, load_from_path};
use std::sync::atomic::{AtomicU64, Ordering};

struct ConfigFile(std::path::PathBuf);
impl ConfigFile {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "harbor-live-candidate-{}-{}.toml",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, source).unwrap();
        Self(path)
    }
}
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn live_rejects_every_recognized_error_without_startup_fallback() {
    for invalid in [
        "font = 1",
        "[font]\nsize = 0",
        "[font]\nsize = nan",
        "[font]\nfamily = []",
        "[font]\nfamily = ''",
        "shell = []",
        "[shell]\nprogram = 1",
        "[shell]\nargs = [1]",
        "clipboard = 1",
        "[clipboard]\nosc52_write = 'invalid'",
        "colors = 1",
        "[colors]\nbackground = 'bad'",
        "[colors]\nbackground_opacity = -0.1",
        "[colors]\nbackground_opacity = 1.1",
        "[colors]\nbackground_opacity = nan",
        "[colors]\nbackground_opacity = inf",
        "[colors]\nbackground_opacity = '0.6'",
        "[colors]\nbackground_opacity = true",
        "[colors]\nbackground_opacity = []",
        "[colors]\nbackground_opacity = {}",
        "[colors.normal]\nred = 1",
        "keybindings = 1",
        "[keybindings.app]\nnew-tab = ['Ctrl+T']",
        "[keybindings.app.new-tab]\nbindings = [{ key = 't' }]",
        "[keybindings.app.new-tab]\nbindings = [{ modifiers = [], key = 1 }]",
    ] {
        let file = ConfigFile::new(&format!("{invalid}\n[unknown]\nvalue = true\n"));
        let rejected = load_candidate_from_path(&file.0).unwrap_err();
        assert_eq!(rejected.path, file.0);
        assert!(
            rejected
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Error),
            "{invalid}"
        );
        assert!(!rejected.to_string().contains("using"), "{rejected}");
        assert!(
            load_from_path(&file.0)
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Error)
        );
    }
    let file = ConfigFile::new("[colors]\nforeground = '#112233'\n[font]\nsize = -1");
    assert!(load_candidate_from_path(&file.0).is_err());
    assert_ne!(
        load_from_path(&file.0).settings.colors,
        Settings::default().colors
    );
}

#[test]
fn complete_documents_restore_defaults_and_preserve_unknown_warnings() {
    let file = ConfigFile::new(
        "[font]\nsize = 30\nfamily = 'Unavailable but valid'\n[shell]\nargs = ['-NoLogo']\n[clipboard]\nosc52_write = 'deny'\n[extra]\nvalue = 1",
    );
    let loaded = load_candidate_from_path(&file.0).unwrap();
    assert_eq!(loaded.settings.font.size, 30.0);
    assert_eq!(loaded.diagnostics.len(), 1);
    assert_eq!(loaded.diagnostics[0].level, DiagnosticLevel::Warning);
    std::fs::write(&file.0, "").unwrap();
    assert_eq!(
        load_candidate_from_path(&file.0).unwrap().settings,
        Settings::default()
    );
}

#[test]
fn missing_unreadable_and_malformed_documents_reject_with_path() {
    let file = ConfigFile::new("[font");
    let error = load_candidate_from_path(&file.0).unwrap_err();
    assert!(error.to_string().contains("could not parse"));
    assert!(error.to_string().contains("line"));
    std::fs::remove_file(&file.0).unwrap();
    assert!(
        load_candidate_from_path(&file.0)
            .unwrap_err()
            .to_string()
            .contains("could not read")
    );
    assert_eq!(load_from_path(&file.0).settings, Settings::default());
    assert!(load_candidate_from_path(std::env::temp_dir()).is_err());
}
