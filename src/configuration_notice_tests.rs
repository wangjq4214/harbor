//! Startup/manual/watched file-I/O and application acceptance policy, without a native window.
use super::*;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;

struct ConfigFile(PathBuf);
impl ConfigFile {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "harbor-notices-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory.join("config.toml"))
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn write(&self, source: &str) {
        std::fs::write(&self.0, source).unwrap();
    }
}
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir(self.0.parent().unwrap());
    }
}

#[test]
fn startup_and_manual_absence_are_silent_but_live_absence_never_commits_defaults() {
    let file = ConfigFile::new();
    let startup = harbor_config::load_from_path(file.path());
    assert_eq!(startup.settings, harbor_config::Settings::default());
    assert!(startup.diagnostics.is_empty());
    let mut accepted = harbor_config::Settings::default();
    accepted.font.size = 23.0;
    let baseline = accepted.clone();
    let clipboard = accepted.clipboard.clone();
    assert!(
        reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _, _| panic!("missing live candidate must not invoke application transaction")
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(accepted, baseline);
    let unresolved =
        reload_configuration_from_path(None, &mut accepted, &clipboard, |_, _, _| unreachable!());
    assert!(
        unresolved.is_err(),
        "unresolvable path is not missing-file silence"
    );
    file.write("[font]\nsize = 'wrong type'\n");
    let startup = harbor_config::load_from_path(file.path());
    assert!(
        !startup.diagnostics.is_empty(),
        "unapplied startup setting remains actionable"
    );
}

#[test]
fn manual_success_no_change_launch_changes_restart_and_ignored_setting_have_selective_notices() {
    let file = ConfigFile::new();
    let mut accepted = harbor_config::Settings::default();
    let clipboard = accepted.clipboard.clone();
    file.write(
        "[font]\nsize = 22\n[shell.environment]\nSYNTHETIC = 'never-disclose-notice-value'\n",
    );
    for _ in 0..2 {
        let result = reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _, _| Ok(vec![]),
        )
        .unwrap()
        .unwrap();
        assert!(result.notice.is_none());
        assert!(!result.summary.contains("never-disclose-notice-value"));
    }
    assert_eq!(accepted.font.size, 22.0);
    file.write("[clipboard]\nosc52_write = 'deny'\n");
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _, _| {
            Ok(vec![])
        })
        .unwrap()
        .unwrap();
    let notice = result.notice.unwrap();
    assert!(notice.contains("restart required"));
    assert!(!notice.contains("applied live") && !notice.contains("new sessions only"));
    file.write("mystery = true\n");
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _, _| {
            Ok(vec![])
        })
        .unwrap()
        .unwrap();
    assert!(result.notice.unwrap().contains("mystery"));
    file.write("");
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _, _| {
            Ok(vec![
                "requested font unavailable; system font selected".into(),
            ])
        })
        .unwrap()
        .unwrap();
    assert!(
        result
            .notice
            .unwrap()
            .contains("requested font unavailable")
    );
}

#[test]
fn invalid_parse_setting_read_and_resource_failures_remain_errors_without_partial_acceptance() {
    let file = ConfigFile::new();
    let mut accepted = harbor_config::Settings::default();
    accepted.font.size = 23.0;
    let baseline = accepted.clone();
    let clipboard = accepted.clipboard.clone();
    for source in [
        "[shell.environment]\nSECRET = 'never-disclose-notice-value'\n[invalid",
        "[font]\nsize = 25\n[colors]\nforeground = 'invalid'\n",
    ] {
        file.write(source);
        let error = reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _, _| panic!("rejected document may not apply valid subset"),
        )
        .err()
        .unwrap();
        let diagnostic = format!("{error:#}");
        assert!(!diagnostic.contains("never-disclose-notice-value"));
        assert!(diagnostic.contains(file.path().to_str().unwrap()));
        assert_eq!(accepted, baseline);
    }
    file.write("[font]\nsize = 25\n");
    let error =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _, _| {
            anyhow::bail!(
                "session 2: PTY resize failed; session 1: recovery failed; session needs recovery"
            )
        })
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("session 1: recovery failed"));
    assert!(
        error.is::<ConfigurationOperationFailure>(),
        "resource failures have a typed presentation lifetime independent of file absence"
    );
    assert_eq!(accepted, baseline);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _exclusive = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(file.path())
            .unwrap();
        let error = harbor_config::load_candidate_from_path(file.path()).unwrap_err();
        assert!(
            error.io_kind.is_some() && error.io_kind != Some(std::io::ErrorKind::NotFound),
            "sharing violation is an actionable read failure, not absence"
        );
        let denied =
            harbor_config::load_candidate_from_path(file.path().parent().unwrap()).unwrap_err();
        assert_eq!(denied.io_kind, Some(std::io::ErrorKind::PermissionDenied));
        assert!(
            reload_configuration_from_path(
                Some(file.path()),
                &mut accepted,
                &clipboard,
                |_, _, _| unreachable!()
            )
            .is_err()
        );
        assert!(
            !harbor_config::load_from_path(file.path())
                .diagnostics
                .is_empty()
        );
        assert_eq!(accepted, baseline);
    }
}

#[test]
fn actual_watcher_reload_handles_missing_invalid_recovery_and_success_without_success_feed() {
    use crate::configuration_watch::{ConfigurationWatch, ReloadFeedback, WatchEvent};
    let file = ConfigFile::new();
    let (sender, wakes) = mpsc::channel();
    let watch = ConfigurationWatch::start(Some(file.path().to_owned()), move || {
        sender.send(()).is_ok()
    })
    .unwrap();
    let next = || {
        wakes
            .recv_timeout(Duration::from_secs(10))
            .expect("real watcher wake");
        match watch.take_pending().expect("settled observation") {
            WatchEvent::Reload => {}
            other => panic!("unexpected {other:?}"),
        }
    };
    let mut accepted = harbor_config::Settings::default();
    accepted.font.size = 23.0;
    let clipboard = accepted.clipboard.clone();
    next(); // starts missing
    assert!(
        reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _, _| unreachable!()
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(accepted.font.size, 23.0);
    file.write("[font]\nsize = 'bad'\n");
    next();
    let error = reload_configuration_from_path(
        Some(file.path()),
        &mut accepted,
        &clipboard,
        |_, _, _| unreachable!(),
    )
    .err()
    .unwrap();
    let mut feedback = ReloadFeedback::default();
    assert!(feedback.failure(&format!("{error:#}"), true));
    assert!(!feedback.failure(&format!("{error:#}"), true));
    file.write("[font]\nsize = 24\n");
    next();
    let result =
        reload_configuration_from_path(Some(file.path()), &mut accepted, &clipboard, |_, _, _| {
            Ok(vec![])
        })
        .unwrap()
        .unwrap();
    feedback.clear();
    assert!(result.notice.is_none());
    assert_eq!(accepted.font.size, 24.0);
    std::fs::remove_file(file.path()).unwrap();
    next();
    assert!(
        reload_configuration_from_path(
            Some(file.path()),
            &mut accepted,
            &clipboard,
            |_, _, _| unreachable!()
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(accepted.font.size, 24.0);
    file.write("[font]\nsize = 'bad'\n");
    next();
    let error = reload_configuration_from_path(
        Some(file.path()),
        &mut accepted,
        &clipboard,
        |_, _, _| unreachable!(),
    )
    .err()
    .unwrap();
    assert!(
        feedback.failure(&format!("{error:#}"), true),
        "same failure after recovery is a new episode"
    );
}
