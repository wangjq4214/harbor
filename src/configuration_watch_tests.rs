//! Real Windows file saves routed through the existing GPU/model/native-PTY fixture.
//! No native Window or winit queue is constructed; wakes use an mpsc transport.
use super::*;
use crate::configuration_watch::{ConfigurationWatch, ReloadFeedback, WatchEvent};
use std::{path::PathBuf, sync::mpsc, time::Duration};

struct FileFixture(PathBuf);
impl FileFixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "harbor-watch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("config.toml")
    }
    fn save(&self, color: &str, size: u32, key: char) {
        std::fs::write(self.path(), format!(
            "[font]\nsize = {size}\n[colors]\nbackground = '{color}'\n[keybindings.app.reload-configuration]\nbindings = [{{ modifiers = ['ctrl', 'shift'], key = '{key}' }}]\n"
        )).unwrap();
    }
    fn watch(&self) -> (ConfigurationWatch, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        let watch =
            ConfigurationWatch::start(Some(self.path()), move || tx.send(()).is_ok()).unwrap();
        (watch, rx)
    }
}
impl Drop for FileFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wake(watch: &ConfigurationWatch, rx: &mpsc::Receiver<()>) {
    rx.recv_timeout(Duration::from_secs(8)).expect("watch wake");
    assert!(matches!(watch.take_pending(), Some(WatchEvent::Reload)));
}

fn reload_file(
    f: &mut Fixture,
    file: &FileFixture,
    fail: Option<ReloadStage>,
) -> anyhow::Result<String> {
    f.reload(
        harbor_config::load_candidate_from_path(file.path())?.settings,
        fail,
    )
}

fn assert_generation(f: &mut Fixture, color: Rgba, size: f32, key: char) {
    assert_eq!(f.accepted.colors.background, color);
    assert_eq!(f.accepted.font.size, size);
    assert_eq!(f.policy.font_settings, f.accepted.font);
    assert_eq!(f.policy.appearance.palette(), f.accepted.colors);
    f.assert_reload_binding(key);
    for (_, terminal, bridge) in f.tabs.configuration_targets() {
        assert_eq!(
            terminal
                .lock()
                .unwrap()
                .read_update(Instant::now())
                .appearance
                .palette()
                .background,
            color
        );
        assert_eq!(
            *bridge.renderer().unwrap().lock().unwrap().metrics(),
            f.policy.metrics
        );
    }
    let (terminal, renderer) = f
        .policy
        .prepare_new_terminal(
            TerminalGpuAccess::new(&f.device, &f.queue, wgpu::TextureFormat::Rgba8Unorm),
            load_system_fonts(&f.policy.font_settings).unwrap(),
            f.tabs.last_broadcast_size().unwrap(),
            (640, 320),
            false,
            harbor_terminal::ClipboardDelivery::Discard,
        )
        .unwrap();
    assert_eq!(
        terminal.read_update(Instant::now()).appearance.palette(),
        f.accepted.colors
    );
    assert_eq!(*renderer.metrics(), f.policy.metrics);
}

#[test]
fn watched_saves_replacements_bursts_failures_and_manual_interleaving() {
    let file = FileFixture::new();
    let mut f = Fixture::new();
    f.attach_native_children();
    let original_targets = f.tabs.configuration_targets();
    file.save("#112233", 24, 'r');
    let (watch, rx) = file.watch();
    wake(&watch, &rx);
    reload_file(&mut f, &file, None).unwrap();

    // Ordinary save, replacement over an existing file, and a subsequent save.
    file.save("#223344", 26, 'g');
    wake(&watch, &rx);
    reload_file(&mut f, &file, None).unwrap();
    assert_generation(&mut f, Rgba::from_rgb8(34, 51, 68), 26.0, 'g');
    let replacement = file.0.join("replacement.toml");
    std::fs::write(&replacement, "[colors]\nbackground = '#334455'\n").unwrap();
    assert!(file.path().is_file());
    std::fs::rename(replacement, file.path()).unwrap();
    wake(&watch, &rx);
    reload_file(&mut f, &file, None).unwrap();
    assert_generation(&mut f, Rgba::from_rgb8(51, 68, 85), 24.0, 'r');
    file.save("#445566", 28, 'g');
    wake(&watch, &rx);
    reload_file(&mut f, &file, None).unwrap();
    assert_generation(&mut f, Rgba::from_rgb8(68, 85, 102), 28.0, 'g');

    // A burst briefly removes the file and exposes malformed content.
    std::fs::remove_file(file.path()).unwrap();
    std::fs::write(file.path(), "[font\nsecret-value").unwrap();
    file.save("#556677", 30, 'r');
    wake(&watch, &rx);
    reload_file(&mut f, &file, None).unwrap();
    assert_generation(&mut f, Rgba::from_rgb8(85, 102, 119), 30.0, 'r');
    let accepted = f.accepted.clone();

    // Persistent missing/malformed files reject, with no defaults or value leaks.
    std::fs::remove_file(file.path()).unwrap();
    wake(&watch, &rx);
    assert!(reload_file(&mut f, &file, None).is_err());
    assert_eq!(f.accepted, accepted);
    std::fs::write(file.path(), "[font\nsecret-value").unwrap();
    wake(&watch, &rx);
    let error = format!("{:#}", reload_file(&mut f, &file, None).unwrap_err());
    assert!(!error.contains("secret-value"));
    let mut feedback = ReloadFeedback::default();
    assert!(feedback.failure(&error, true));
    std::fs::write(file.path(), "[font\nsecret-value\n# another save").unwrap();
    wake(&watch, &rx);
    let repeated = format!("{:#}", reload_file(&mut f, &file, None).unwrap_err());
    assert_eq!(error, repeated);
    assert!(!feedback.failure(&repeated, true));
    std::fs::write(file.path(), "[font]\nsize = 'secret-value'\n").unwrap();
    wake(&watch, &rx);
    let changed = format!("{:#}", reload_file(&mut f, &file, None).unwrap_err());
    assert!(!changed.contains("secret-value"));
    assert!(feedback.failure(&changed, true));
    assert_eq!(f.accepted, accepted);

    // Queue an old wake; manually accept a newer file before consuming it.
    file.save("#667788", 32, 'g');
    rx.recv_timeout(Duration::from_secs(8)).unwrap();
    file.save("#778899", 34, 'r');
    reload_file(&mut f, &file, None).unwrap(); // manual path, not watch-dependent
    let latest = f.accepted.clone();
    assert!(matches!(watch.take_pending(), Some(WatchEvent::Reload)));
    reload_file(&mut f, &file, None).unwrap(); // delayed wake reads current file
    assert_eq!(f.accepted, latest);
    assert_generation(&mut f, Rgba::from_rgb8(119, 136, 153), 34.0, 'r');

    // Inject later-session preparation rejection; manual retry still succeeds.
    file.save("#8899aa", 36, 'g');
    wake(&watch, &rx);
    assert!(reload_file(&mut f, &file, Some(ReloadStage::Gpu)).is_err());
    assert_eq!(f.accepted, latest);
    assert_generation(&mut f, Rgba::from_rgb8(119, 136, 153), 34.0, 'r');
    reload_file(&mut f, &file, None).unwrap();
    assert_generation(&mut f, Rgba::from_rgb8(136, 153, 170), 36.0, 'g');
    for ((id, terminal, _), (old_id, old_terminal, _)) in
        f.tabs.configuration_targets().iter().zip(&original_targets)
    {
        assert_eq!(id, old_id);
        assert!(Arc::ptr_eq(terminal, old_terminal));
    }
    // Both original cmd/PowerShell readers/children survive every transaction.
    f.native_input();
    f.wait_for_native_markers("RESUMED");
}

#[test]
fn watched_setup_and_runtime_faults_leave_manual_transaction_usable() {
    let file = FileFixture::new();
    let mut f = Fixture::new();
    assert!(ConfigurationWatch::start(None, || true).is_err());
    file.save("#123456", 24, 'r');
    reload_file(&mut f, &file, None).unwrap();
    let accepted = f.accepted.clone();
    // A directory in place of the file deterministically faults the real reader.
    std::fs::remove_file(file.path()).unwrap();
    std::fs::create_dir(file.path()).unwrap();
    let (watch, rx) = file.watch();
    rx.recv_timeout(Duration::from_secs(8)).unwrap();
    assert!(matches!(
        watch.take_pending(),
        Some(WatchEvent::Unavailable(_))
    ));
    assert!(
        rx.recv_timeout(Duration::from_millis(900)).is_err(),
        "persistent fault must not flood"
    );
    assert_eq!(f.accepted, accepted);
    std::fs::remove_dir(file.path()).unwrap();
    file.save("#234567", 26, 'g');
    reload_file(&mut f, &file, None).unwrap(); // manual before automatic recovery
    wake(&watch, &rx);
    reload_file(&mut f, &file, None).unwrap();
    assert_generation(&mut f, Rgba::from_rgb8(35, 69, 103), 26.0, 'g');
}

#[test]
fn watched_missing_parent_recovers_and_outstanding_wakes_coalesce() {
    let file = FileFixture::new();
    std::fs::remove_dir(&file.0).unwrap();
    let (watch, rx) = file.watch();
    wake(&watch, &rx); // strict loader will reject the missing path
    assert!(harbor_config::load_candidate_from_path(file.path()).is_err());
    std::fs::create_dir(&file.0).unwrap();
    file.save("#123456", 24, 'r');
    rx.recv_timeout(Duration::from_secs(8)).unwrap();
    // Deliberately leave the wake unacknowledged while more documents settle.
    file.save("#234567", 26, 'g');
    assert!(rx.recv_timeout(Duration::from_millis(1000)).is_err());
    file.save("#345678", 28, 'r');
    assert!(rx.recv_timeout(Duration::from_millis(1000)).is_err());
    assert!(matches!(watch.take_pending(), Some(WatchEvent::Reload))); // before read
    let loaded = harbor_config::load_candidate_from_path(file.path()).unwrap();
    assert_eq!(
        loaded.settings.colors.background,
        Rgba::from_rgb8(52, 86, 120)
    );
    file.save("#456789", 30, 'g');
    wake(&watch, &rx);
    assert_eq!(
        harbor_config::load_candidate_from_path(file.path())
            .unwrap()
            .settings
            .font
            .size,
        30.0
    );
}
